//! Contract admission is independent of physical metadata and query grants.
use crate::{
    odcs_intake::{read_regular, CompiledBundle, MAX_CONFIG_BYTES},
    AppState,
};
use aster_core::{CoreError, Principal, Result};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Grants {
    format_version: u32,
    version: String,
    grants: Vec<Grant>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    path: String,
    sha256: String,
    teams: Vec<String>,
}

fn denied() -> CoreError {
    CoreError::Unauthorized("contract access denied".into())
}

pub(crate) async fn prepare(
    state: &AppState,
    headers: &HeaderMap,
    path: &str,
    digest: &str,
    object: &str,
) -> Result<Value> {
    prepare_inner(state, headers, path, digest, object, false).await
}

pub(crate) async fn prepare_for_ai(
    state: &AppState,
    headers: &HeaderMap,
    path: &str,
    digest: &str,
    object: &str,
) -> Result<Value> {
    prepare_inner(state, headers, path, digest, object, true).await
}

async fn prepare_inner(
    state: &AppState,
    headers: &HeaderMap,
    path: &str,
    digest: &str,
    object: &str,
    bounded: bool,
) -> Result<Value> {
    let context = read(state, headers, Some((path, digest))).await?;
    let index = object
        .strip_prefix("/schema/")
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|i| object == format!("/schema/{i}"))
        .ok_or_else(|| CoreError::Invalid("select an exact schema object pointer".into()))?;
    let declared = context["declared"]["schema"]
        .get(index)
        .ok_or_else(|| CoreError::Invalid("unknown schema object".into()))?;
    if bounded {
        crate::ai_context::validate(declared)?;
        crate::ai_context::validate(&context["declared"]["description"])?;
    }
    let bundle = state.compiled_contracts.as_ref().ok_or_else(denied)?;
    let candidates: Vec<_> = bundle
        .physical_bindings
        .iter()
        .flat_map(|c| c.bindings.iter().map(move |b| (c, b)))
        .filter(|(_, b)| b.path == path && b.object == object)
        .collect();
    let mut binding = json!({"status":"missing"});
    let observation = match candidates.as_slice() {
        [] => json!({"status":"unavailable"}),
        [(config, physical)] => {
            binding = json!({"status":"resolved","version":config.version,"catalog":physical.catalog,
                "namespaceSegments":physical.namespace_segments,"physicalName":physical.physical_name});
            if crate::require_catalog_metadata(state, &physical.catalog).is_err() {
                json!({"status":"denied"})
            } else if let Some(catalog) = state
                .catalogs
                .get(&aster_core::CatalogId::new(&physical.catalog))
            {
                let table = aster_core::TableRef {
                    namespace: physical.namespace_segments.join("."),
                    namespace_segments: physical.namespace_segments.clone(),
                    name: physical.physical_name.clone(),
                };
                let observed = if bounded {
                    tokio::time::timeout(
                        std::time::Duration::from_secs(1),
                        catalog.table_schema_bounded(&table, 256 * 1024, 2),
                    )
                    .await
                    .unwrap_or_else(|_| Err(CoreError::Catalog("observation deadline".into())))
                } else {
                    catalog.table_schema(&table).await
                };
                match observed {
                    Ok(schema)
                        if schema.table.namespace_segments == table.namespace_segments
                            && schema.table.name == table.name
                            && (!bounded
                                || (schema.columns.len() <= 64
                                    && crate::ai_context::serialize(&schema, 4096).is_ok())) =>
                    {
                        json!({"status":"observed","catalog":physical.catalog,"observedAt":crate::now(),"schema":schema})
                    }
                    Ok(_) => json!({"status":"unavailable"}),
                    Err(error) => {
                        tracing::warn!(%error, "contract observation unavailable");
                        json!({"status":"unavailable"})
                    }
                }
            } else {
                json!({"status":"unavailable"})
            }
        }
        _ => {
            binding = json!({"status":"ambiguous"});
            json!({"status":"unavailable"})
        }
    };
    let comparison = if !bounded && observation["status"] == "observed" {
        let fields = declared["properties"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let columns = observation["schema"]["columns"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let physical = |field: &Value| field["physicalName"].as_str().map(str::to_owned);
        json!({"status":"separate_facts",
            "declaredOnly":fields.iter().filter(|f| physical(f).is_some_and(|name| !columns.iter().any(|c| c["name"] == name))).collect::<Vec<_>>(),
            "unmappedDeclared":fields.iter().filter(|f| physical(f).is_none()).collect::<Vec<_>>(),
            "observedOnly":columns.iter().filter(|c| !fields.iter().any(|f| physical(f).is_some_and(|name| c["name"] == name))).map(|c| &c["name"]).collect::<Vec<_>>(),
            "matched":fields.iter().filter_map(|f| physical(f).and_then(|name| columns.iter().find(|c| c["name"] == name).map(|c| json!({"declared":f,"observed":c,"mapping":"declared physicalName"})))).collect::<Vec<_>>()})
    } else {
        json!({"status":"unknown"})
    };
    Ok(
        json!({"declared":declared,"contractMeaning":context["declared"]["description"],"provenance":context["provenance"],
        "object":object,"binding":binding,"observation":observation,"comparison":comparison}),
    )
}

fn grants(bundle: &CompiledBundle) -> Result<Grants> {
    let path = bundle.root.join("grants.json");
    if !path.exists() {
        return Ok(Grants {
            format_version: 1,
            version: "absent".into(),
            grants: vec![],
        });
    }
    let bytes = read_regular(&path, MAX_CONFIG_BYTES)?;
    let grants: Grants = serde_json::from_slice(&bytes).map_err(|_| denied())?;
    let mut seen = std::collections::HashSet::new();
    if grants.format_version != 1
        || grants.version.trim().is_empty()
        || grants.grants.iter().any(|g| {
            g.teams.is_empty()
                || g.teams.iter().any(|t| t.is_empty())
                || !seen.insert((&g.path, &g.sha256))
                || !bundle
                    .documents
                    .iter()
                    .any(|d| d.selection.path == g.path && d.selection.sha256 == g.sha256)
        })
    {
        return Err(denied());
    }
    Ok(grants)
}

pub(crate) async fn read(
    state: &AppState,
    headers: &HeaderMap,
    selection: Option<(&str, &str)>,
) -> Result<Value> {
    let principal = crate::current_identity::current_principal(state, headers, false)
        .await
        .map_err(|_| denied())?;
    read_for_principal(state, &principal, selection).await
}

/// Reuse the inventory operation's freshly verified identity, never refresh a
/// second time and combine different membership snapshots in one decision.
pub(crate) async fn read_for_principal(
    state: &AppState,
    principal: &Principal,
    selection: Option<(&str, &str)>,
) -> Result<Value> {
    let bundle = state.compiled_contracts.clone().ok_or_else(denied)?;
    let input = bundle.clone();
    // Re-read grants on every request, including cached documents.
    let grants = tokio::task::spawn_blocking(move || grants(&input))
        .await
        .map_err(|_| denied())?
        .map_err(|_| denied())?;
    if grants.grants.iter().flat_map(|g| &g.teams).any(|team| {
        if let Some(teams) = &state.team_git_targets {
            !teams.contains_team(team)
        } else {
            !state
                .team_workspaces
                .as_ref()
                .is_some_and(|teams| teams.contains_team(team))
        }
    }) {
        return Err(denied());
    }
    let admitted = |path: &str, digest: &str| {
        grants.grants.iter().any(|g| {
            g.path == path
                && g.sha256 == digest
                && g.teams.iter().any(|team| {
                    if let Some(teams) = &state.team_git_targets {
                        teams.member(team, principal).is_ok()
                    } else {
                        state
                            .team_workspaces
                            .as_ref()
                            .is_some_and(|teams| teams.member(team, principal).is_ok())
                    }
                })
        })
    };
    if let Some((path, digest)) = selection {
        if !admitted(path, digest) {
            return Err(denied());
        }
        let artifact = bundle
            .documents
            .iter()
            .find(|d| d.selection.path == path && d.selection.sha256 == digest)
            .ok_or_else(denied)?;
        let original_source = std::str::from_utf8(&artifact.document.source)
            .map_err(|_| CoreError::Invalid("ODCS input is not UTF-8".into()))?;
        return Ok(
            json!({"declared":artifact.document.raw,"originalSource":original_source,"provenance":{"bundle":bundle.bundle,"version":bundle.version,
            "manifestSha256":bundle.manifest_sha256,"path":path,"sha256":digest,"target":artifact.selection.target,"grantsVersion":grants.version}}),
        );
    }
    Ok(
        json!({"contracts":bundle.documents.iter().filter(|d| admitted(&d.selection.path, &d.selection.sha256)).map(|d|
        json!({"path":d.selection.path,"sha256":d.selection.sha256,"id":d.selection.id,"version":d.selection.version,"target":d.selection.target})
    ).collect::<Vec<_>>()}),
    )
}
