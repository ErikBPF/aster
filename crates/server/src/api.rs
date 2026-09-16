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

use aster_core::{authorize, CoreError, EngineId, Principal};
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
        CoreError::Unauthorized(message) => ConnectError::permission_denied(message),
        CoreError::NotFound(message) => ConnectError::not_found(message),
        CoreError::Invalid(message) => ConnectError::invalid_argument(message),
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
    }
}

#[allow(refining_impl_trait)]
impl api::Aster for AsterApi {
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

    async fn list_engines(
        &self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, api::ListEnginesRequest>,
    ) -> ServiceResult<api::ListEnginesResponse> {
        caller(&self.state, &ctx).await?;
        let mut engines = Vec::new();
        for engine in self.state.engines.list() {
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
        caller(&self.state, &ctx).await?;
        let mut catalogs = Vec::new();
        for catalog in self.state.catalogs.list() {
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

    async fn list_notebooks(
        &self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, api::ListNotebooksRequest>,
    ) -> ServiceResult<api::ListNotebooksResponse> {
        let caller = caller(&self.state, &ctx).await?;
        authorize(&caller, aster_core::Action::ReadNotebook).map_err(connect_error)?;
        let ids = self
            .state
            .notebooks
            .list(&caller.subject)
            .await
            .map_err(connect_error)?;
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
        let caller = caller(&self.state, &ctx).await?;
        authorize(&caller, aster_core::Action::ReadNotebook).map_err(connect_error)?;
        let requested = request.to_owned_message();
        let notebook = self
            .state
            .notebooks
            .get(&requested.id)
            .await
            .map_err(connect_error)?;
        Ok(to_notebook(&notebook).into())
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
        let notebook = aster_core::Notebook {
            id: requested.id.clone(),
            title: message.title.clone(),
            cells: message.cells.iter().map(to_cell).collect(),
        };
        let revision = self
            .state
            .notebooks
            .save(&notebook, &caller.subject)
            .await
            .map_err(connect_error)?;
        Ok(api::SaveNotebookResponse {
            revision,
            ..Default::default()
        }
        .into())
    }

    async fn run_query(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::RunQueryRequest>,
    ) -> ServiceResult<api::QueryResult> {
        let caller = caller(&self.state, &ctx).await?;
        let requested = request.to_owned_message();
        let body = crate::QueryBody {
            sql: requested.sql.clone(),
            engine: requested.engine.clone(),
            catalog: requested.catalog.clone(),
            schema: requested.schema.clone(),
            max_rows: requested.max_rows.map(|rows| rows as usize),
        };
        let result = crate::execute_query(&self.state, &caller, &body)
            .await
            .map_err(connect_error)?;

        let columns = result
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
        Ok(api::QueryResult {
            columns,
            rows_json,
            truncated: result.truncated,
            ..Default::default()
        }
        .into())
    }
}
