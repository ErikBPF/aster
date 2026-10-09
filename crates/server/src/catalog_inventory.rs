//! Request-local physical inventory; declarations are not existence evidence.
use aster_core::{authorize, Action, CatalogId, CoreError, Principal, Result};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    odcs_intake::{read_regular, MAX_CONFIG_BYTES},
    AppState,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Owners {
    format_version: u32,
    version: String,
    schemas: Vec<Owner>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Owner {
    catalog: String,
    namespace_segments: Vec<String>,
    team: String,
}

fn denied() -> CoreError {
    CoreError::Unauthorized("inventory authority unavailable".into())
}

fn undisclosed() -> Value {
    json!({"physical":{"status":"denied"},"entries":[]})
}

async fn current_principal(state: &AppState, headers: &HeaderMap) -> Result<Principal> {
    let principal = crate::current_identity::current_principal(state, headers, false)
        .await
        .map_err(|_| denied())?;
    authorize(&principal, Action::ReadNotebook)?;
    Ok(principal)
}

/// Follow ODCS property structure, not annotation-shaped examples/custom data.
fn has_semantics(node: &Value) -> bool {
    node.get("semanticType").is_some()
        || node.get("transformLogic").is_some()
        || node["properties"]
            .as_array()
            .is_some_and(|properties| properties.iter().any(has_semantics))
        || node.get("items").is_some_and(has_semantics)
        || ["key", "value"]
            .iter()
            .any(|key| node["map"].get(key).is_some_and(has_semantics))
}

pub(crate) async fn visible(state: &AppState, headers: &HeaderMap, id: &str) -> bool {
    if !crate::catalog_metadata_visible(state, id) {
        return false;
    }
    if state.compiled_contracts.is_none() {
        return true;
    }
    namespaces(state, headers, id)
        .await
        .is_ok_and(|namespaces| !namespaces.is_empty())
}

pub(crate) async fn all(state: &AppState, headers: &HeaderMap) -> Result<Vec<Value>> {
    let principal = current_principal(state, headers).await?;
    let owners = owners(state).await?;
    if owners.schemas.len() > 128 {
        return Err(denied());
    }
    let mut inventories = Vec::new();
    for owner in owners.schemas {
        let mut inventory =
            read_for_principal(state, &principal, &owner.catalog, &owner.namespace_segments)
                .await?;
        if inventory["physical"]["status"] != "denied" {
            inventory["catalog"] = json!(owner.catalog);
            inventory["namespaceSegments"] = json!(owner.namespace_segments);
            inventories.push(inventory);
        }
    }
    Ok(inventories)
}

async fn owners(state: &AppState) -> Result<Owners> {
    let bundle = state.compiled_contracts.as_ref().ok_or_else(denied)?;
    let path = bundle.root.join("schema-owners.json");
    tokio::task::spawn_blocking(move || {
        serde_json::from_slice(&read_regular(&path, MAX_CONFIG_BYTES).map_err(|_| denied())?)
            .map_err(|_| denied())
    })
    .await
    .map_err(|_| denied())?
}

pub(crate) async fn namespaces(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
) -> Result<Vec<aster_core::Namespace>> {
    crate::require_catalog_metadata(state, id)?;
    if state.compiled_contracts.is_none() {
        return state
            .catalogs
            .get(&CatalogId::new(id))
            .ok_or_else(denied)?
            .list_namespaces()
            .await;
    }
    let principal = current_principal(state, headers).await?;
    let mut result = Vec::new();
    for owner in owners(state).await?.schemas {
        if owner.catalog != id {
            continue;
        }
        let inventory =
            read_for_principal(state, &principal, id, &owner.namespace_segments).await?;
        if inventory["physical"]["status"] == "unknown" {
            return Err(CoreError::Catalog("physical inventory unavailable".into()));
        }
        if inventory["entries"]
            .as_array()
            .is_some_and(|entries| !entries.is_empty())
        {
            result.push(aster_core::Namespace {
                name: owner.namespace_segments.join("."),
                segments: owner.namespace_segments,
            });
        }
    }
    Ok(result)
}

pub(crate) async fn tables(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
    segments: &[String],
) -> Result<Vec<aster_core::TableDescriptor>> {
    crate::require_catalog_metadata(state, id)?;
    if state.compiled_contracts.is_none() {
        return state
            .catalogs
            .get(&CatalogId::new(id))
            .ok_or_else(denied)?
            .list_descriptors_qualified(segments)
            .await;
    }
    let inventory = read(state, headers, id, segments).await?;
    if inventory["physical"]["status"] == "unknown" {
        return Err(CoreError::Catalog("physical inventory unavailable".into()));
    }
    inventory["entries"]
        .as_array()
        .ok_or_else(denied)?
        .iter()
        .map(|entry| serde_json::from_value(entry["descriptor"].clone()).map_err(|_| denied()))
        .collect()
}

pub(crate) async fn read(
    state: &AppState,
    headers: &HeaderMap,
    catalog_id: &str,
    segments: &[String],
) -> Result<Value> {
    let principal = current_principal(state, headers).await?;
    read_for_principal(state, &principal, catalog_id, segments).await
}

async fn read_for_principal(
    state: &AppState,
    principal: &Principal,
    catalog_id: &str,
    segments: &[String],
) -> Result<Value> {
    let segments = aster_core::catalog::namespace_segments("", segments)?;
    let bundle = state.compiled_contracts.as_ref().ok_or_else(denied)?;
    let owners = owners(state).await?;
    let mut seen = std::collections::HashSet::new();
    if owners.format_version != 1
        || owners.version.trim().is_empty()
        || owners.schemas.len() > 128
        || owners.schemas.iter().any(|owner| {
            owner.catalog.is_empty()
                || aster_core::catalog::namespace_segments("", &owner.namespace_segments).is_err()
                || !seen.insert((&owner.catalog, &owner.namespace_segments))
                || if let Some(teams) = &state.team_git_targets {
                    !teams.contains_team(&owner.team)
                } else {
                    !state
                        .team_workspaces
                        .as_ref()
                        .is_some_and(|teams| teams.contains_team(&owner.team))
                }
        })
    {
        return Err(denied());
    }
    // Global grant failures must not distinguish a configured schema from an
    // unknown one either. Resolve them before either undisclosed outcome.
    let contracts = crate::contract_reads::read_for_principal(state, principal, None).await?;
    let Some(owner) = owners
        .schemas
        .iter()
        .find(|owner| owner.catalog == catalog_id && owner.namespace_segments == segments)
    else {
        return Ok(undisclosed());
    };
    let is_owner = if let Some(teams) = &state.team_git_targets {
        teams.member(&owner.team, principal).is_ok()
    } else {
        state
            .team_workspaces
            .as_ref()
            .is_some_and(|teams| teams.member(&owner.team, principal).is_ok())
    };
    let admitted: Vec<_> = bundle
        .physical_bindings
        .iter()
        .flat_map(|config| &config.bindings)
        .filter(|binding| binding.catalog == catalog_id && binding.namespace_segments == segments)
        .filter_map(|binding| {
            bundle
                .documents
                .iter()
                .find(|document| document.selection.path == binding.path)
                .filter(|document| {
                    contracts["contracts"].as_array().is_some_and(|grants| {
                        grants.iter().any(|grant| {
                            grant["path"] == document.selection.path
                                && grant["sha256"] == document.selection.sha256
                        })
                    })
                })
                .map(|document| (binding, document))
        })
        .collect();
    let unavailable = || json!({"physical":{"status":"unknown"},"entries":[]});
    if !is_owner && admitted.is_empty() {
        return Ok(undisclosed());
    }
    crate::require_catalog_metadata(state, catalog_id)?;
    let Some(catalog) = state.catalogs.get(&CatalogId::new(catalog_id)) else {
        return Ok(unavailable());
    };
    if !catalog.authoritative_inventory() {
        return Ok(unavailable());
    }
    // Fresh complete listing for this request only; never cache positive existence.
    let descriptors = match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        catalog.list_descriptors_qualified(&segments),
    )
    .await
    {
        Ok(Ok(descriptors)) if descriptors.len() <= 4096 => descriptors,
        _ => return Ok(unavailable()),
    };
    let mut names = std::collections::HashSet::new();
    if descriptors.iter().any(|descriptor| {
        descriptor.table.namespace_segments != segments
            || descriptor.table.name.is_empty()
            || !names.insert(&descriptor.table.name)
    }) {
        return Ok(unavailable());
    }
    let mut entries = Vec::new();
    for descriptor in descriptors {
        let bindings: Vec<_> = admitted
            .iter()
            .filter(|(binding, _)| binding.physical_name == descriptor.table.name)
            .collect();
        if !is_owner && bindings.is_empty() {
            continue;
        }
        let mut coverage = Vec::new();
        let mut semantic = false;
        for (binding, document) in &bindings {
            let declared = document
                .document
                .raw
                .pointer(&binding.object)
                .ok_or_else(denied)?;
            semantic |= has_semantics(declared);
            coverage.push(json!({"path":document.selection.path,"sha256":document.selection.sha256,"object":binding.object}));
        }
        entries.push(json!({"catalog":catalog_id,"namespaceSegments":segments,"physicalName":descriptor.table.name,
            "descriptor":descriptor,
            "physical":{"status":"present"},
            "contract":{"status":if bindings.is_empty() {"not_admitted"} else {"admitted"},"artifacts":coverage},
            "semantics":{"status":if bindings.is_empty() {"unknown"} else if semantic {"declared"} else {"not_declared"}},"queryAuthorization":"not_evaluated"}));
    }
    entries.sort_by(|a, b| a["physicalName"].as_str().cmp(&b["physicalName"].as_str()));
    Ok(
        json!({"physical":{"status":"present","observedAt":crate::now()},"ownersVersion":owners.version,"entries":entries}),
    )
}
