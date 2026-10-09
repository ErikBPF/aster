//! The RPC surface, generated from `proto/aster.proto`.
//!
//! One registration serves gRPC (internal callers), the Connect protocol's
//! JSON encoding (browser and curl) and gRPC-Web (browser streaming), so the
//! proto is the only place an endpoint is declared. The `google.api.http`
//! bindings in the proto are the machine-readable REST contract if a
//! transcoding proxy is ever added.
//!
//! The older `/api/*` JSON handlers still exist for the server-rendered web
//! page; they and these RPCs read the same `AppState`, and the JSON layer goes
//! away once the page moves to Connect.

use std::sync::Arc;

use aster_core::{authorize, CoreError, EngineId, NotebookStore, Principal};
use connectrpc::{ConnectError, RequestContext, ServiceRequest, ServiceResult};

use crate::AppState;

pub mod pb {
    connectrpc::include_generated!();
}

use pb::aster::v1 as api;
use pb::aster::v1::AsterExt;

pub struct AsterApi {
    state: Arc<AppState>,
}

impl AsterApi {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

/// Registers the service on a ConnectRPC router.
pub fn router(state: Arc<AppState>) -> connectrpc::Router {
    Arc::new(AsterApi::new(state)).register(connectrpc::Router::new())
}

/// Domain failures keep their meaning on the wire; everything else is an
/// upstream problem and is logged instead of returned. `unauthorized` never
/// appears here: an unauthenticated call is refused by `caller` before the
/// error can reach this mapping, so `Unauthorized` means the caller is known but
/// not allowed — the same meaning the REST layer gives it with 403.
fn connect_error(error: CoreError) -> ConnectError {
    match error {
        CoreError::Conflict(message) => ConnectError::aborted(message),
        CoreError::Unauthorized(message) => ConnectError::permission_denied(message),
        CoreError::NotFound(message) => ConnectError::not_found(message),
        CoreError::Invalid(message) | CoreError::Query(message) => {
            ConnectError::invalid_argument(message)
        }
        other => {
            tracing::error!(%other, "rpc failed");
            ConnectError::internal("upstream dependency failed")
        }
    }
}

/// Identity comes from the session cookie or the dev seam, never the body.
async fn caller(state: &AppState, ctx: &RequestContext) -> Result<Principal, ConnectError> {
    crate::principal(state, ctx.headers())
        .await
        .map_err(|_| ConnectError::unauthenticated("sign in required"))
}

async fn notebook_context(
    state: &AppState,
    ctx: &RequestContext,
    team: Option<&str>,
    workspace: Option<&str>,
    notebook: &str,
) -> Result<Option<(Principal, String, aster_core::Notebook)>, ConnectError> {
    if team.is_none() && workspace.is_none() && state.local_notebooks_enabled() {
        let principal = caller(state, ctx).await?;
        authorize(&principal, aster_core::Action::ReadNotebook).map_err(connect_error)?;
        state
            .admit_notebook(notebook, &principal.subject)
            .await
            .map_err(connect_error)?;
        return Ok(None);
    }
    if state.team_workspaces.is_none() {
        if team.is_some() || workspace.is_some() {
            return Err(ConnectError::permission_denied(
                "team notebooks are disabled",
            ));
        }
        return Ok(None);
    }
    let team = team.ok_or_else(|| {
        ConnectError::permission_denied("workspace-qualified notebook route required")
    })?;
    let personal = match workspace.unwrap_or("session") {
        "session" => false,
        "personal" => true,
        _ => {
            return Err(ConnectError::invalid_argument(
                "invalid workspace selection",
            ))
        }
    };
    crate::team_notebook_context_selected(state, ctx.headers(), team, notebook, personal)
        .await
        .map(Some)
        .map_err(connect_error)
}

async fn notebook_workspace(
    state: &AppState,
    ctx: &RequestContext,
    team: Option<&str>,
    workspace: Option<&str>,
) -> Result<Option<(Principal, Arc<crate::GitNotebookStore>)>, ConnectError> {
    if team.is_none() && workspace.is_none() && state.local_notebooks_enabled() {
        return Ok(None);
    }
    let Some(workspaces) = state.team_workspaces.as_deref() else {
        if team.is_some() || workspace.is_some() {
            return Err(ConnectError::permission_denied(
                "team notebooks are disabled",
            ));
        }
        return Ok(None);
    };
    let team = team.ok_or_else(|| {
        ConnectError::permission_denied("workspace-qualified notebook route required")
    })?;
    let personal = match workspace.unwrap_or("session") {
        "session" => false,
        "personal" => true,
        _ => {
            return Err(ConnectError::invalid_argument(
                "invalid workspace selection",
            ))
        }
    };
    let (principal, sid) = crate::team_member_session(state, ctx.headers(), team)
        .await
        .map_err(|error| connect_error(error.0))?;
    authorize(&principal, aster_core::Action::ReadNotebook).map_err(connect_error)?;
    let store = workspaces
        .workspace(team, &principal, &sid, personal)
        .map_err(connect_error)?;
    Ok(Some((principal, store)))
}

fn to_message(state: aster_core::WorkingState) -> api::WorkingState {
    api::WorkingState {
        notebook: state.notebook,
        cell: state.cell,
        engine: state.engine,
        ..Default::default()
    }
}

fn from_message(message: &api::PutStateRequest) -> aster_core::WorkingState {
    let mut state = aster_core::WorkingState::default();
    state.notebook = message.notebook.clone();
    state.cell = message.cell.clone();
    state.engine = message.engine.clone();
    state
}

fn to_notebook(notebook: &aster_core::Notebook) -> api::Notebook {
    let cells = notebook
        .cells
        .iter()
        .map(|cell| api::Cell {
            id: cell.id.clone(),
            sql: cell.sql.clone(),
            engine: cell.engine.as_ref().map(|engine| engine.0.clone()),
            ..Default::default()
        })
        .collect();
    api::Notebook {
        id: notebook.id.clone(),
        title: notebook.title.clone(),
        cells,
        ..Default::default()
    }
}

fn to_cell(cell: &api::Cell) -> aster_core::Cell {
    aster_core::Cell {
        id: cell.id.clone(),
        sql: cell.sql.clone(),
        engine: cell.engine.clone().map(EngineId::new),
        metadata: Default::default(),
    }
}

#[allow(refining_impl_trait)]
impl api::Aster for AsterApi {
    async fn list_catalog_inventory(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ListTablesRequest>,
    ) -> ServiceResult<api::ContractReadResponse> {
        let requested = request.to_owned_message();
        let segments = aster_core::catalog::namespace_segments(
            &requested.namespace,
            &requested.namespace_segments,
        )
        .map_err(connect_error)?;
        let context = crate::catalog_inventory::read(
            &self.state,
            ctx.headers(),
            &requested.catalog,
            &segments,
        )
        .await
        .map_err(connect_error)?;
        Ok(api::ContractReadResponse {
            context_json: context.to_string(),
            ..Default::default()
        }
        .into())
    }
    async fn prepare_contract_query(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ContractSelection>,
    ) -> ServiceResult<api::ContractReadResponse> {
        let request = request.to_owned_message();
        let value = crate::contract_reads::prepare(
            &self.state,
            ctx.headers(),
            &request.path,
            &request.sha256,
            &request.object,
        )
        .await
        .map_err(connect_error)?;
        Ok(api::ContractReadResponse {
            context_json: value.to_string(),
            ..Default::default()
        }
        .into())
    }
    async fn list_contracts(
        &self,
        ctx: RequestContext,
        _: ServiceRequest<'_, api::ListContractsRequest>,
    ) -> ServiceResult<api::ContractReadResponse> {
        let value = crate::contract_reads::read(&self.state, ctx.headers(), None)
            .await
            .map_err(connect_error)?;
        Ok(api::ContractReadResponse {
            context_json: value.to_string(),
            ..Default::default()
        }
        .into())
    }

    async fn get_contract(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ContractSelection>,
    ) -> ServiceResult<api::ContractReadResponse> {
        let request = request.to_owned_message();
        let value = crate::contract_reads::read(
            &self.state,
            ctx.headers(),
            Some((&request.path, &request.sha256)),
        )
        .await
        .map_err(connect_error)?;
        Ok(api::ContractReadResponse {
            context_json: value.to_string(),
            ..Default::default()
        }
        .into())
    }

    async fn get_conversation(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetConversationRequest>,
    ) -> ServiceResult<api::Conversation> {
        let request = request.to_owned_message();
        let context = notebook_context(
            &self.state,
            &ctx,
            request.team.as_deref(),
            request.workspace.as_deref(),
            &request.notebook,
        )
        .await?;
        if context.is_some() && request.cell.is_some() {
            return Err(ConnectError::invalid_argument(
                "cell conversations are not available in team workspaces",
            ));
        }
        let conversation = if let Some((principal, key, _)) = context {
            self.state
                .conversations
                .get(&principal.subject, &key)
                .await
                .map_err(connect_error)?
        } else {
            let principal = caller(&self.state, &ctx).await?;
            crate::conversations::get(
                &self.state,
                &principal,
                &request.notebook,
                request.cell.as_deref(),
            )
            .await
            .map_err(connect_error)?
        };
        tokio::time::timeout(
            crate::ai_context::DEADLINE,
            crate::ai_context::history(&self.state, ctx.headers(), &conversation),
        )
        .await
        .map_err(|_| {
            ConnectError::permission_denied(
                "conversation context unavailable; start a fresh conversation",
            )
        })?
        .map_err(connect_error)?;
        Ok(to_conversation(conversation).into())
    }

    async fn send_message(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::SendMessageRequest>,
    ) -> ServiceResult<api::Conversation> {
        let request = request.to_owned_message();
        let expected = request
            .expected_revision
            .ok_or_else(|| ConnectError::invalid_argument("expected revision is required"))?;
        let context = notebook_context(
            &self.state,
            &ctx,
            request.team.as_deref(),
            request.workspace.as_deref(),
            &request.notebook,
        )
        .await?;
        let principal = if let Some((principal, _, _)) = &context {
            principal.clone()
        } else {
            caller(&self.state, &ctx).await?
        };
        if context.is_some() && request.cell.is_some() {
            return Err(ConnectError::invalid_argument(
                "cell conversations are not available in team workspaces",
            ));
        }
        let contract_selection = request.contract_selection.as_option().map(|selection| {
            aster_core::conversation::ContractSelection {
                path: selection.path.clone(),
                sha256: selection.sha256.clone(),
                object: selection.object.clone(),
            }
        });
        let turn = crate::conversations::Turn {
            notebook: &request.notebook,
            cell: request.cell.as_deref(),
            helper: &request.helper,
            prompt: &request.prompt,
            context: &request.context,
            expected,
            contract_selection: contract_selection.as_ref(),
        };
        let conversation = if let Some((_, key, document)) = context {
            crate::conversations::send_scoped(
                &self.state,
                ctx.headers(),
                &principal,
                turn,
                &key,
                &document,
            )
            .await
        } else {
            crate::conversations::send(&self.state, ctx.headers(), &principal, turn).await
        }
        .map_err(connect_error)?;
        Ok(to_conversation(conversation).into())
    }

    async fn get_state(
        &self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, api::GetStateRequest>,
    ) -> ServiceResult<api::WorkingState> {
        let caller = caller(&self.state, &ctx).await?;
        let state = self
            .state
            .user_state
            .get(&caller.subject)
            .await
            .map_err(connect_error)?
            .unwrap_or_default();
        Ok(to_message(state).into())
    }

    async fn put_state(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::PutStateRequest>,
    ) -> ServiceResult<api::WorkingState> {
        let caller = caller(&self.state, &ctx).await?;
        authorize(&caller, aster_core::Action::ReadNotebook).map_err(connect_error)?;
        let state = from_message(&request.to_owned_message());
        self.state
            .user_state
            .put(&caller.subject, &state)
            .await
            .map_err(connect_error)?;
        Ok(to_message(state).into())
    }

    async fn get_notebook_helper(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetNotebookHelperRequest>,
    ) -> ServiceResult<api::NotebookHelper> {
        let request = request.to_owned_message();
        let notebook = request.notebook;
        let helper = if let Some((principal, key, _)) = notebook_context(
            &self.state,
            &ctx,
            request.team.as_deref(),
            request.workspace.as_deref(),
            &notebook,
        )
        .await?
        {
            crate::ai::selected_helper_scoped(&self.state, &principal.subject, &key).await
        } else {
            let principal = caller(&self.state, &ctx).await?;
            authorize(&principal, aster_core::Action::ReadNotebook).map_err(connect_error)?;
            crate::ai::selected_helper(&self.state, &principal.subject, &notebook).await
        }
        .map_err(connect_error)?;
        Ok(api::NotebookHelper {
            notebook,
            helper,
            ..Default::default()
        }
        .into())
    }

    async fn put_notebook_helper(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::PutNotebookHelperRequest>,
    ) -> ServiceResult<api::NotebookHelper> {
        let requested = request.to_owned_message();
        let context = notebook_context(
            &self.state,
            &ctx,
            requested.team.as_deref(),
            requested.workspace.as_deref(),
            &requested.notebook,
        )
        .await?;
        if let Some((principal, key, _)) = context {
            crate::ai::choose_helper_scoped(
                &self.state,
                ctx.headers(),
                &principal.subject,
                &key,
                &requested.helper,
            )
            .await
            .map_err(connect_error)?;
        } else {
            let principal = caller(&self.state, &ctx).await?;
            authorize(&principal, aster_core::Action::ReadNotebook).map_err(connect_error)?;
            crate::ai::choose_helper(
                &self.state,
                ctx.headers(),
                &principal.subject,
                &requested.notebook,
                &requested.helper,
            )
            .await
            .map_err(connect_error)?;
        }
        Ok(api::NotebookHelper {
            notebook: requested.notebook,
            helper: Some(requested.helper),
            ..Default::default()
        }
        .into())
    }

    async fn list_engines(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ListEnginesRequest>,
    ) -> ServiceResult<api::ListEnginesResponse> {
        let principal = caller(&self.state, &ctx).await?;
        let requested = request.to_owned_message();
        let mut engines = Vec::new();
        for engine in crate::available_engines(
            &self.state,
            &principal,
            requested.catalog_context.as_deref(),
        )
        .await
        .map_err(connect_error)?
        {
            let info = engine.info().clone();
            engines.push(api::Engine {
                id: info.id.0,
                kind: info.kind,
                endpoint: info.endpoint,
                routing_group: info.routing_group.unwrap_or_default(),
                health: engine.health().await.as_str().to_string(),
                ..Default::default()
            });
        }
        Ok(api::ListEnginesResponse {
            engines,
            ..Default::default()
        }
        .into())
    }

    async fn list_catalogs(
        &self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, api::ListCatalogsRequest>,
    ) -> ServiceResult<api::ListCatalogsResponse> {
        let principal = caller(&self.state, &ctx).await?;
        authorize(&principal, aster_core::Action::ReadNotebook).map_err(connect_error)?;
        let mut catalogs = Vec::new();
        for catalog in self.state.catalogs.list() {
            if !crate::catalog_inventory::visible(&self.state, ctx.headers(), &catalog.id().0).await
            {
                continue;
            }
            catalogs.push(api::Catalog {
                id: catalog.id().0.clone(),
                kind: catalog.kind().to_string(),
                health: catalog.health().await.as_str().to_string(),
                ..Default::default()
            });
        }
        Ok(api::ListCatalogsResponse {
            catalogs,
            ..Default::default()
        }
        .into())
    }

    async fn list_tables(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ListTablesRequest>,
    ) -> ServiceResult<api::ListTablesResponse> {
        let caller = caller(&self.state, &ctx).await?;
        authorize(&caller, aster_core::Action::ReadNotebook).map_err(connect_error)?;
        let requested = request.to_owned_message();
        crate::require_catalog_metadata(&self.state, &requested.catalog).map_err(connect_error)?;
        let tables = crate::catalog_inventory::tables(
            &self.state,
            ctx.headers(),
            &requested.catalog,
            &aster_core::catalog::namespace_segments(
                &requested.namespace,
                &requested.namespace_segments,
            )
            .map_err(connect_error)?,
        )
        .await
        .map_err(connect_error)?
        .into_iter()
        .map(|descriptor| api::TableDescriptor {
            namespace: descriptor.table.namespace,
            namespace_segments: descriptor.table.namespace_segments,
            name: descriptor.table.name,
            format: descriptor.format,
            base_location: descriptor.base_location,
            schema_available: Some(descriptor.schema_available),
            ..Default::default()
        })
        .collect();
        Ok(api::ListTablesResponse {
            tables,
            ..Default::default()
        }
        .into())
    }

    async fn list_notebooks(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ListNotebooksRequest>,
    ) -> ServiceResult<api::ListNotebooksResponse> {
        let requested = request.to_owned_message();
        let ids = if let Some((principal, store)) = notebook_workspace(
            &self.state,
            &ctx,
            requested.team.as_deref(),
            requested.workspace.as_deref(),
        )
        .await?
        {
            store
                .list(&principal.subject)
                .await
                .map_err(connect_error)?
        } else {
            let caller = caller(&self.state, &ctx).await?;
            authorize(&caller, aster_core::Action::ReadNotebook).map_err(connect_error)?;
            self.state
                .owned_notebooks(&caller.subject)
                .await
                .map_err(connect_error)?
        };
        Ok(api::ListNotebooksResponse {
            ids,
            ..Default::default()
        }
        .into())
    }

    async fn get_notebook(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetNotebookRequest>,
    ) -> ServiceResult<api::Notebook> {
        let requested = request.to_owned_message();
        let snapshot = if let Some((_, store)) = notebook_workspace(
            &self.state,
            &ctx,
            requested.team.as_deref(),
            requested.workspace.as_deref(),
        )
        .await?
        {
            store.snapshot(&requested.id).await.map_err(connect_error)?
        } else {
            let caller = caller(&self.state, &ctx).await?;
            authorize(&caller, aster_core::Action::ReadNotebook).map_err(connect_error)?;
            self.state
                .owned_notebook_snapshot(&requested.id, &caller.subject)
                .await
                .map_err(connect_error)?
        };
        let mut response = to_notebook(&snapshot.notebook);
        response.content_revision = Some(snapshot.content_revision);
        Ok(response.into())
    }

    async fn save_notebook(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::SaveNotebookRequest>,
    ) -> ServiceResult<api::SaveNotebookResponse> {
        let caller = caller(&self.state, &ctx).await?;
        authorize(&caller, aster_core::Action::WriteNotebook).map_err(connect_error)?;
        let requested = request.to_owned_message();
        let message = requested.notebook.clone().into_option().unwrap_or_default();
        let mut notebook = aster_core::Notebook {
            id: requested.id.clone(),
            title: message.title.clone(),
            cells: message.cells.iter().map(to_cell).collect(),
        };
        let expected = match (
            requested.expected_content_revision.as_deref(),
            requested.if_absent,
        ) {
            (None, true) => aster_core::NotebookPrecondition::Absent,
            (Some(oid), false)
                if matches!(oid.len(), 40 | 64)
                    && oid.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
            {
                aster_core::NotebookPrecondition::Blob(oid.to_string())
            }
            _ => {
                return Err(ConnectError::invalid_argument(
                    "one notebook precondition is required",
                ))
            }
        };
        // The legacy RPC Cell has no metadata field. Preserve browser context
        // for matching cells rather than silently deleting it on an RPC edit.
        if matches!(expected, aster_core::NotebookPrecondition::Blob(_)) {
            let previous = self
                .state
                .owned_notebook_snapshot(&requested.id, &caller.subject)
                .await
                .map_err(connect_error)?;
            for cell in &mut notebook.cells {
                if let Some(old) = previous.notebook.cells.iter().find(|old| old.id == cell.id) {
                    cell.metadata = old.metadata.clone();
                }
            }
        }
        let saved = self
            .state
            .save_notebook_owned(&notebook, &caller.subject, expected)
            .await
            .map_err(connect_error)?;
        Ok(api::SaveNotebookResponse {
            revision: saved.revision,
            content_revision: saved.content_revision,
            ..Default::default()
        }
        .into())
    }

    async fn assign_notebook_owner(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::AssignNotebookOwnerRequest>,
    ) -> ServiceResult<api::AssignNotebookOwnerResponse> {
        let caller = caller(&self.state, &ctx).await?;
        authorize(&caller, aster_core::Action::Administer).map_err(connect_error)?;
        let requested = request.to_owned_message();
        self.state
            .assign_notebook_owner(
                &requested.id,
                &caller.subject,
                crate::AssignNotebookOwner {
                    owner: requested.owner,
                    expected_content_revision: requested.expected_content_revision,
                    expected_owner: requested.expected_owner,
                    reason: requested.reason,
                },
            )
            .await
            .map_err(connect_error)?;
        Ok(api::AssignNotebookOwnerResponse::default().into())
    }

    async fn run_query(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::RunQueryRequest>,
    ) -> ServiceResult<api::QueryResult> {
        let (caller, verified_session) = crate::principal_with_session(&self.state, ctx.headers())
            .await
            .map_err(|_| ConnectError::unauthenticated("sign in required"))?;
        let requested = request.to_owned_message();
        let body = crate::QueryBody {
            sql: requested.sql.clone(),
            engine: requested.engine.clone(),
            catalog_context: requested.catalog_context.clone(),
            catalog: requested.catalog.clone(),
            schema: requested.schema.clone(),
            max_rows: requested.max_rows.map(|rows| rows as usize),
            notebook: requested.notebook.clone(),
            cell: requested.cell.clone(),
        };
        let result = crate::execute_query(&self.state, &caller, verified_session, &body)
            .await
            .map_err(connect_error)?;

        let columns: Vec<api::Column> = result
            .columns
            .iter()
            .map(|column| api::Column {
                name: column.name.clone(),
                data_type: column.data_type.clone(),
                ..Default::default()
            })
            .collect();
        let mut rows_json = Vec::with_capacity(result.rows.len());
        for row in &result.rows {
            match serde_json::to_string(row) {
                Ok(encoded) => rows_json.push(encoded),
                Err(error) => {
                    tracing::error!(%error, "query row is not serialisable");
                    return Err(ConnectError::internal("upstream dependency failed"));
                }
            }
        }
        if let (Some(notebook), Some(cell)) = (&requested.notebook, &requested.cell) {
            let names: Vec<String> = columns.iter().map(|column| column.name.clone()).collect();
            if let Err(error) = crate::exchange::record_result(
                &self.state,
                &caller,
                notebook,
                cell,
                &names,
                &rows_json,
                result.truncated,
            )
            .await
            {
                tracing::warn!(%error, "cell result was not recorded for the session exchange");
            }
        }
        Ok(api::QueryResult {
            columns,
            rows_json,
            truncated: result.truncated,
            ..Default::default()
        }
        .into())
    }

    async fn render_semantic(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::RenderSemanticRequest>,
    ) -> ServiceResult<api::RenderSemanticResponse> {
        let caller = caller(&self.state, &ctx).await?;
        authorize(&caller, aster_core::Action::ReadNotebook).map_err(connect_error)?;
        let requested = request.to_owned_message();

        crate::require_catalog_metadata(&self.state, &requested.catalog).map_err(connect_error)?;

        let catalog = self
            .state
            .catalogs
            .get(&aster_core::CatalogId::new(requested.catalog.clone()))
            .ok_or_else(|| {
                ConnectError::not_found(format!("unknown catalog: {}", requested.catalog))
            })?;
        let segments = aster_core::catalog::namespace_segments(
            &requested.namespace,
            &requested.namespace_segments,
        )
        .map_err(connect_error)?;
        // Existing emitters understand one unambiguous component, not structured SQL names.
        let namespace = aster_core::catalog::legacy_namespace(&segments).map_err(connect_error)?;
        let table = aster_core::TableRef {
            namespace_segments: segments,
            namespace,
            name: requested.table.clone(),
        };
        if self.state.compiled_contracts.is_some()
            && !crate::catalog_inventory::tables(
                &self.state,
                ctx.headers(),
                &requested.catalog,
                &table.namespace_segments,
            )
            .await
            .map_err(connect_error)?
            .iter()
            .any(|entry| entry.table.name == table.name)
        {
            return Err(ConnectError::permission_denied(
                "table metadata unavailable",
            ));
        }
        let schema = catalog.table_schema(&table).await.map_err(connect_error)?;
        let contract = aster_core::for_table(&self.state.contracts, &table.name);
        let format = aster_core::semantic::format(&requested.target).map_err(connect_error)?;
        let content = format
            .render(&aster_core::TableModel {
                schema: &schema,
                contract,
            })
            .map_err(connect_error)?;

        Ok(api::RenderSemanticResponse {
            target: format.target().to_string(),
            path: format.path(&table),
            content,
            ..Default::default()
        }
        .into())
    }

    async fn fetch_query(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::FetchQueryRequest>,
    ) -> ServiceResult<api::FetchQueryResponse> {
        let caller = caller(&self.state, &ctx).await?;
        let requested = request.to_owned_message();
        let (sql, content_revision) = crate::exchange::fetch_query(
            &self.state,
            &caller,
            &requested.notebook,
            &requested.cell,
        )
        .await
        .map_err(connect_error)?;
        Ok(api::FetchQueryResponse {
            sql,
            content_revision,
            ..Default::default()
        }
        .into())
    }

    async fn fetch_result(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::FetchResultRequest>,
    ) -> ServiceResult<api::FetchResultResponse> {
        let caller = caller(&self.state, &ctx).await?;
        let requested = request.to_owned_message();
        let result = crate::exchange::fetch_result(
            &self.state,
            &caller,
            &requested.notebook,
            &requested.cell,
        )
        .await
        .map_err(connect_error)?;
        Ok(api::FetchResultResponse {
            columns: result.columns,
            rows_json: result.rows_json,
            truncated: result.truncated,
            ..Default::default()
        }
        .into())
    }

    async fn update_query(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::UpdateQueryRequest>,
    ) -> ServiceResult<api::UpdateQueryResponse> {
        let caller = caller(&self.state, &ctx).await?;
        let requested = request.to_owned_message();
        let content_revision = crate::exchange::update_query(
            &self.state,
            &caller,
            &requested.notebook,
            &requested.cell,
            &requested.sql,
            &requested.expected_content_revision,
        )
        .await
        .map_err(connect_error)?;
        Ok(api::UpdateQueryResponse {
            content_revision,
            ..Default::default()
        }
        .into())
    }

    async fn fetch_summary(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::FetchSummaryRequest>,
    ) -> ServiceResult<api::FetchSummaryResponse> {
        let caller = caller(&self.state, &ctx).await?;
        let requested = request.to_owned_message();
        let (summary, cells) =
            crate::exchange::fetch_summary(&self.state, &caller, &requested.notebook)
                .await
                .map_err(connect_error)?;
        Ok(api::FetchSummaryResponse {
            summary,
            cells: cells
                .into_iter()
                .map(|(id, sql)| api::NotebookCell {
                    id,
                    sql,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
        .into())
    }

    async fn send_summary(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::SendSummaryRequest>,
    ) -> ServiceResult<api::SendSummaryResponse> {
        let caller = caller(&self.state, &ctx).await?;
        let requested = request.to_owned_message();
        crate::exchange::send_summary(
            &self.state,
            &caller,
            &requested.notebook,
            Some(requested.summary.clone()),
        )
        .await
        .map_err(connect_error)?;
        Ok(api::SendSummaryResponse {
            summary: requested.summary,
            ..Default::default()
        }
        .into())
    }
}

fn to_conversation(value: aster_core::Conversation) -> api::Conversation {
    api::Conversation {
        id: value.id,
        revision: value.revision,
        messages: value
            .messages
            .into_iter()
            .map(|m| api::ChatMessage {
                role: m.role,
                content: m.content,
                helper: m.helper,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}
