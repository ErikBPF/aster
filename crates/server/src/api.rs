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

    fn workspaces(&self) -> Result<&Arc<crate::workspace::Workspaces>, ConnectError> {
        self.state
            .durable_workspaces
            .as_ref()
            .ok_or_else(|| ConnectError::permission_denied("durable workspaces are disabled"))
    }
}

fn query_message(result: aster_core::QueryResult) -> Result<api::QueryResult, ConnectError> {
    let columns = result
        .columns
        .into_iter()
        .map(|column| api::Column {
            name: column.name,
            data_type: column.data_type,
            ..Default::default()
        })
        .collect();
    let rows_json = result
        .rows
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            tracing::error!(%error, "query row is not serialisable");
            ConnectError::internal("upstream dependency failed")
        })?;
    Ok(api::QueryResult {
        columns,
        rows_json,
        truncated: result.truncated,
        ..Default::default()
    })
}

fn checkout_message(view: crate::workspace::CheckoutView) -> api::WorkspaceCheckout {
    api::WorkspaceCheckout {
        id: view.id,
        workspace: view.workspace,
        team: view.team,
        repository: view.repository,
        branch: view.branch,
        version: view.version,
        head: view.head,
        dirty: view.dirty,
        notebooks: view.notebooks,
        draft_revision: view.draft_revision,
        ..Default::default()
    }
}

fn writer_context(
    workspace: &str,
    token: &str,
    epoch: u64,
) -> Result<crate::workspace::WriterContext, ConnectError> {
    let valid =
        |value: &str| value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !valid(workspace) || !valid(token) || epoch == 0 {
        return Err(ConnectError::invalid_argument(
            "workspace writer credential and epoch are required",
        ));
    }
    Ok(crate::workspace::WriterContext {
        workspace: workspace.into(),
        token: token.into(),
        epoch,
    })
}

fn workspace_message(view: crate::workspace::WorkspaceView) -> api::Workspace {
    api::Workspace {
        id: view.id,
        name: view.name,
        epoch: view.epoch,
        read_only: view.read_only,
        writer_token: view.writer_token,
        deleting: view.deleting,
        ..Default::default()
    }
}

fn repository_message(
    record: crate::store::TeamGitTargetRecord,
) -> Result<api::WorkspaceRepository, ConnectError> {
    Ok(api::WorkspaceRepository {
        team: record.team,
        repository: record.repository,
        repository_id: u64::try_from(record.repository_id)
            .map_err(|_| ConnectError::internal("invalid repository record"))?,
        default_branch: record.default_branch,
        default_commit: record.default_commit,
        version: u64::try_from(record.version)
            .map_err(|_| ConnectError::internal("invalid repository record"))?,
        ..Default::default()
    })
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

async fn workspace_caller(
    state: &AppState,
    ctx: &RequestContext,
    action: aster_core::Action,
) -> Result<Principal, ConnectError> {
    crate::current_identity::workspace_principal(state, ctx.headers(), action)
        .await
        .map_err(|error| connect_error(error.0))
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
        context: notebook
            .context
            .as_ref()
            .map(|context| api::NotebookContext {
                catalog_context: context.catalog_context.clone(),
                schema: context.schema.clone(),
                ..Default::default()
            })
            .into(),
        id: notebook.id.clone(),
        title: notebook.title.clone(),
        engine: notebook.engine.as_ref().map(|engine| engine.0.clone()),
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

fn skill_pb(view: crate::skills::SkillView) -> api::Skill {
    let skill = view.skill;
    api::Skill {
        id: skill.id,
        owner: skill.owner,
        title: skill.title,
        description: skill.description,
        revision: skill.revision,
        sharing_revision: skill.sharing_revision,
        audience: match skill.audience {
            aster_core::skills::SkillAudience::Private => "private",
            aster_core::skills::SkillAudience::Team => "team",
            aster_core::skills::SkillAudience::Organization => "organization",
            _ => "private",
        }
        .into(),
        team: skill.team,
        instructions: skill.instructions,
        references: skill
            .references
            .into_iter()
            .map(|r| api::SkillReference {
                name: r.name,
                content: r.content,
                ..Default::default()
            })
            .collect(),
        author: skill.author,
        created_at_ms: skill.created_at_ms,
        can_edit: view.can_edit,
        ..Default::default()
    }
}

fn mcp_pb(c: aster_core::mcp::McpConnection) -> api::McpConnection {
    api::McpConnection {
        id: c.id,
        name: c.name,
        description: c.description,
        url: c.url,
        auth: c.auth,
        enabled: c.enabled,
        revision: c.revision,
        subjects: c.subjects,
        teams: c.teams,
        ..Default::default()
    }
}

#[allow(refining_impl_trait)]
impl api::Aster for AsterApi {
    async fn export_workspace_checkout(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetWorkspaceCheckoutRequest>,
    ) -> ServiceResult<api::WorkspaceExport> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::ReadNotebook).await?;
        let request = request.to_owned_message();
        let archive = self
            .workspaces()?
            .export(&principal, &request.workspace, &request.checkout)
            .await
            .map_err(connect_error)?;
        Ok(api::WorkspaceExport {
            archive,
            ..Default::default()
        }
        .into())
    }

    async fn discover_mcp_tools(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::DiscoverMcpToolsRequest>,
    ) -> ServiceResult<api::DiscoverMcpToolsResponse> {
        let r = request.to_owned_message();
        let tools = crate::mcp::discover(&self.state, ctx.headers(), &r.id)
            .await
            .map_err(connect_error)?;
        Ok(api::DiscoverMcpToolsResponse {
            tools: tools
                .into_iter()
                .map(|(t, h, a)| api::McpTool {
                    name: t.name,
                    description: t.description,
                    schema_json: t.input_schema.to_string(),
                    schema_hash: h,
                    approved_read: a,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
        .into())
    }

    async fn import_workspace_notebook(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ImportWorkspaceNotebookRequest>,
    ) -> ServiceResult<api::WorkspaceCheckout> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let writer = writer_context(&request.workspace, &request.writer_token, request.epoch)?;
        let _read = self.state.notebook_write.lock().await;
        let record = self
            .state
            .notebook_owners
            .record(&self.state.notebooks.source_key(), &request.notebook)
            .await
            .map_err(connect_error)?
            .filter(|record| record.owner == principal.subject)
            .ok_or_else(|| ConnectError::permission_denied("legacy notebook unavailable"))?;
        let snapshot = self
            .state
            .notebooks
            .snapshot(&request.notebook)
            .await
            .map_err(connect_error)?;
        if snapshot.content_revision != request.expected_legacy_revision
            || record.content_revision != snapshot.content_revision
        {
            return Err(connect_error(aster_core::CoreError::Conflict(
                "legacy notebook revision changed".into(),
            )));
        }
        let view = self
            .workspaces()?
            .edit_checkout(
                &principal,
                &writer,
                &request.checkout,
                request.expected_version,
                crate::workspace::CheckoutEdit::SaveDraft {
                    notebook: snapshot.notebook,
                    expected: None,
                },
            )
            .await
            .map_err(connect_error)?;
        Ok(checkout_message(view).into())
    }

    async fn reconcile_workspace_remote(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetWorkspaceStateRequest>,
    ) -> ServiceResult<api::WorkspaceRemoteOutcome> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let outcome = self
            .workspaces()?
            .reconcile(&principal, &request.workspace)
            .await
            .map_err(connect_error)?;
        Ok(api::WorkspaceRemoteOutcome {
            checkout: outcome.checkout.map(checkout_message).into(),
            pull_request: outcome.pull_request,
            ..Default::default()
        }
        .into())
    }

    async fn approve_mcp_read(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ApproveMcpReadRequest>,
    ) -> ServiceResult<api::McpConnection> {
        let r = request.to_owned_message();
        Ok(mcp_pb(
            crate::mcp::approve_read(
                &self.state,
                ctx.headers(),
                &r.id,
                r.expected_revision,
                &r.tool,
                &r.schema_hash,
                r.approved,
            )
            .await
            .map_err(connect_error)?,
        )
        .into())
    }
    async fn call_mcp_tool(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::CallMcpToolRequest>,
    ) -> ServiceResult<api::CallMcpToolResponse> {
        let r = request.to_owned_message();
        let result = crate::mcp::call(
            &self.state,
            ctx.headers(),
            &r.id,
            &r.tool,
            &r.arguments_json,
            &r.confirmation,
        )
        .await
        .map_err(connect_error)?;
        Ok(api::CallMcpToolResponse {
            confirmation: result.confirmation,
            result_json: result.result,
            ..Default::default()
        }
        .into())
    }

    async fn run_workspace_cell(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::RunWorkspaceCellRequest>,
    ) -> ServiceResult<api::QueryResult> {
        let principal = workspace_caller(&self.state, &ctx, aster_core::Action::RunQuery).await?;
        let request = request.to_owned_message();
        let draft = self
            .workspaces()?
            .draft(
                &principal,
                &request.workspace,
                &request.checkout,
                &request.notebook,
            )
            .await
            .map_err(connect_error)?;
        if request.expected_version != draft.checkout_version {
            return Err(connect_error(aster_core::CoreError::Conflict(
                "checkout changed; reload before running".into(),
            )));
        }
        let cell = draft
            .notebook
            .cells
            .iter()
            .find(|cell| cell.id == request.cell)
            .ok_or_else(|| ConnectError::not_found("cell unavailable"))?;
        if cell.kind() == aster_core::CellKind::Markdown {
            return Err(ConnectError::invalid_argument(
                "markdown cells do not execute queries",
            ));
        }
        let (catalog_context, schema) = if let Some(context) = &draft.notebook.context {
            if context.catalog_context.as_deref().is_none_or(str::is_empty) {
                return Err(ConnectError::invalid_argument(
                    "select a notebook catalog before running",
                ));
            }
            (context.catalog_context.clone(), context.schema.clone())
        } else {
            let field = |name| {
                cell.metadata
                    .get(name)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            };
            (field("catalog_context"), field("schema"))
        };
        let body = crate::QueryBody {
            sql: cell.source().to_owned(),
            engine: draft
                .notebook
                .engine
                .as_ref()
                .or(cell.engine.as_ref())
                .map(|engine| engine.0.clone()),
            catalog_context,
            catalog: None,
            schema,
            max_rows: None,
            notebook: None,
            cell: None,
        };
        // This admitted cell does not enter the legacy notebook/result-cache namespace.
        let result = crate::execute_query(&self.state, &principal, true, &body)
            .await
            .map_err(connect_error)?;
        Ok(query_message(result)?.into())
    }

    async fn open_workspace_pull_request(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::OpenWorkspacePullRequestRequest>,
    ) -> ServiceResult<api::WorkspacePullRequest> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let writer = writer_context(&request.workspace, &request.writer_token, request.epoch)?;
        let url = self
            .workspaces()?
            .open_pull_request(
                &principal,
                &writer,
                &request.checkout,
                request.expected_version,
                &request.title,
            )
            .await
            .map_err(connect_error)?;
        Ok(api::WorkspacePullRequest {
            url,
            ..Default::default()
        }
        .into())
    }

    async fn get_workspace_state(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetWorkspaceStateRequest>,
    ) -> ServiceResult<api::WorkspaceState> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::ReadNotebook).await?;
        let request = request.to_owned_message();
        let state = self
            .workspaces()?
            .state(&principal, &request.workspace)
            .await
            .map_err(connect_error)?;
        Ok(api::WorkspaceState {
            checkouts: state.checkouts,
            layout_json: serde_json::to_string(&state.layout)
                .map_err(|_| ConnectError::internal("layout unavailable"))?,
            layout_version: state.layout_version,
            recovery_bytes: state.recovery_bytes,
            total_checkouts: state.total_checkouts,
            max_checkouts: crate::workspace::MAX_CHECKOUTS as u64,
            max_checkout_bytes: crate::workspace_snapshot::MAX_BYTES as u64,
            ..Default::default()
        }
        .into())
    }

    async fn save_workspace_layout(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::SaveWorkspaceLayoutRequest>,
    ) -> ServiceResult<api::WorkspaceLayoutVersion> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let writer = writer_context(&request.workspace, &request.writer_token, request.epoch)?;
        let layout = serde_json::from_str(&request.layout_json)
            .map_err(|_| ConnectError::invalid_argument("invalid layout"))?;
        let version = self
            .workspaces()?
            .save_layout(&principal, &writer, request.expected_version, layout)
            .await
            .map_err(connect_error)?;
        Ok(api::WorkspaceLayoutVersion {
            version,
            ..Default::default()
        }
        .into())
    }

    async fn list_workspaces(
        &self,
        ctx: RequestContext,
        _: ServiceRequest<'_, api::ListWorkspacesRequest>,
    ) -> ServiceResult<api::ListWorkspacesResponse> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::ReadNotebook).await?;
        let workspaces = self
            .workspaces()?
            .list(&principal)
            .await
            .map_err(connect_error)?
            .into_iter()
            .map(workspace_message)
            .collect();
        Ok(api::ListWorkspacesResponse {
            workspaces,
            max_workspaces: crate::workspace::MAX_WORKSPACES as u64,
            ..Default::default()
        }
        .into())
    }

    async fn list_mcp_connections(
        &self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, api::ListMcpConnectionsRequest>,
    ) -> ServiceResult<api::ListMcpConnectionsResponse> {
        let (configured, can_manage, connections) = crate::mcp::list(&self.state, ctx.headers())
            .await
            .map_err(connect_error)?;
        Ok(api::ListMcpConnectionsResponse {
            configured,
            can_manage,
            connections: connections.into_iter().map(mcp_pb).collect(),
            ..Default::default()
        }
        .into())
    }
    async fn put_mcp_connection(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::PutMcpConnectionRequest>,
    ) -> ServiceResult<api::McpConnection> {
        let r = request.to_owned_message();
        let c = aster_core::mcp::McpConnection {
            id: r.id,
            name: r.name,
            description: r.description,
            url: r.url,
            auth: r.auth,
            enabled: r.enabled,
            revision: 0,
            subjects: r.subjects,
            teams: r.teams,
            approved_reads: Default::default(),
        };
        Ok(mcp_pb(
            crate::mcp::put(&self.state, ctx.headers(), c, r.expected_revision, r.token)
                .await
                .map_err(connect_error)?,
        )
        .into())
    }

    async fn get_skill_defaults(
        &self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, api::GetSkillDefaultsRequest>,
    ) -> ServiceResult<api::SkillDefaults> {
        Ok(defaults_pb(
            crate::skills::defaults(&self.state, ctx.headers())
                .await
                .map_err(connect_error)?,
        )
        .into())
    }
    async fn put_skill_defaults(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::PutSkillDefaultsRequest>,
    ) -> ServiceResult<api::SkillDefaults> {
        let request = request.to_owned_message();
        let defaults = aster_core::skills::SkillDefaults {
            revision: request.expected_revision,
            skills: request
                .skills
                .into_iter()
                .map(|s| aster_core::skills::SkillSelection {
                    id: s.id,
                    revision: s.revision,
                })
                .collect(),
        };
        Ok(defaults_pb(
            crate::skills::put_defaults(&self.state, ctx.headers(), defaults)
                .await
                .map_err(connect_error)?,
        )
        .into())
    }
    async fn list_skills(
        &self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, api::ListSkillsRequest>,
    ) -> ServiceResult<api::ListSkillsResponse> {
        let skills = crate::skills::list(&self.state, ctx.headers())
            .await
            .map_err(connect_error)?
            .into_iter()
            .map(skill_pb)
            .collect();
        Ok(api::ListSkillsResponse {
            skills,
            ..Default::default()
        }
        .into())
    }

    async fn delete_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::DeleteWorkspaceRequest>,
    ) -> ServiceResult<api::DeleteWorkspaceResponse> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let writer = writer_context(&request.workspace, &request.writer_token, request.epoch)?;
        self.workspaces()?
            .delete(&principal, &writer)
            .await
            .map_err(connect_error)?;
        Ok(api::DeleteWorkspaceResponse::default().into())
    }

    async fn push_workspace_checkout(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetWorkspaceMutationRequest>,
    ) -> ServiceResult<api::WorkspaceCheckout> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let writer = writer_context(&request.workspace, &request.writer_token, request.epoch)?;
        if request.expected_version == 0 {
            return Err(ConnectError::invalid_argument(
                "checkout version is required",
            ));
        }
        let view = self
            .workspaces()?
            .push_checkout(
                &principal,
                &writer,
                &request.checkout,
                request.expected_version,
            )
            .await
            .map_err(connect_error)?;
        Ok(checkout_message(view).into())
    }

    async fn clone_workspace_repository(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::CloneWorkspaceRepositoryRequest>,
    ) -> ServiceResult<api::WorkspaceCheckout> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let writer = writer_context(&request.workspace, &request.writer_token, request.epoch)?;
        let view = self
            .workspaces()?
            .clone_repository(
                &principal,
                &writer,
                &request.team,
                request.repository_id,
                &request.branch,
            )
            .await
            .map_err(connect_error)?;
        Ok(checkout_message(view).into())
    }

    async fn get_workspace_checkout(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetWorkspaceCheckoutRequest>,
    ) -> ServiceResult<api::WorkspaceCheckout> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::ReadNotebook).await?;
        let request = request.to_owned_message();
        let view = self
            .workspaces()?
            .checkout(&principal, &request.workspace, &request.checkout)
            .await
            .map_err(connect_error)?;
        Ok(checkout_message(view).into())
    }

    async fn get_workspace_draft(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetWorkspaceDraftRequest>,
    ) -> ServiceResult<api::WorkspaceDraft> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::ReadNotebook).await?;
        let request = request.to_owned_message();
        let draft = self
            .workspaces()?
            .draft(
                &principal,
                &request.workspace,
                &request.checkout,
                &request.notebook,
            )
            .await
            .map_err(connect_error)?;
        let notebook_json = serde_json::to_string(&draft.notebook)
            .map_err(|_| ConnectError::internal("notebook serialization failed"))?;
        Ok(api::WorkspaceDraft {
            notebook_json,
            revision: draft.revision,
            checkout_version: draft.checkout_version,
            branch: draft.branch,
            ..Default::default()
        }
        .into())
    }

    async fn get_skill(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetSkillRequest>,
    ) -> ServiceResult<api::Skill> {
        let request = request.to_owned_message();
        Ok(skill_pb(
            crate::skills::get(&self.state, ctx.headers(), &request.id, request.revision)
                .await
                .map_err(connect_error)?,
        )
        .into())
    }
    async fn list_skill_versions(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ListSkillVersionsRequest>,
    ) -> ServiceResult<api::ListSkillsResponse> {
        let request = request.to_owned_message();
        let skills = crate::skills::versions(&self.state, ctx.headers(), &request.id)
            .await
            .map_err(connect_error)?
            .into_iter()
            .map(skill_pb)
            .collect();
        Ok(api::ListSkillsResponse {
            skills,
            ..Default::default()
        }
        .into())
    }

    async fn save_workspace_draft(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::SaveWorkspaceDraftRequest>,
    ) -> ServiceResult<api::WorkspaceCheckout> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let writer = writer_context(&request.workspace, &request.writer_token, request.epoch)?;
        if request.expected_version == 0 {
            return Err(ConnectError::invalid_argument(
                "checkout version is required",
            ));
        }
        let expected = match (request.expected_draft_revision, request.if_absent) {
            (None, true) => None,
            (Some(revision), false) if !revision.is_empty() && revision.len() <= 256 => {
                Some(revision)
            }
            _ => {
                return Err(ConnectError::invalid_argument(
                    "exactly one draft precondition is required",
                ))
            }
        };
        let notebook = serde_json::from_str(&request.notebook_json)
            .map_err(|_| ConnectError::invalid_argument("invalid notebook JSON"))?;
        let view = self
            .workspaces()?
            .edit_checkout(
                &principal,
                &writer,
                &request.checkout,
                request.expected_version,
                crate::workspace::CheckoutEdit::SaveDraft { notebook, expected },
            )
            .await
            .map_err(connect_error)?;
        Ok(checkout_message(view).into())
    }

    async fn pull_workspace_checkout(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetWorkspaceMutationRequest>,
    ) -> ServiceResult<api::WorkspaceCheckout> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let writer = writer_context(&request.workspace, &request.writer_token, request.epoch)?;
        if request.expected_version == 0 {
            return Err(ConnectError::invalid_argument(
                "checkout version is required",
            ));
        }
        let view = self
            .workspaces()?
            .edit_checkout(
                &principal,
                &writer,
                &request.checkout,
                request.expected_version,
                crate::workspace::CheckoutEdit::Pull,
            )
            .await
            .map_err(connect_error)?;
        Ok(checkout_message(view).into())
    }

    async fn switch_workspace_branch(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::SwitchWorkspaceBranchRequest>,
    ) -> ServiceResult<api::WorkspaceCheckout> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let writer = writer_context(&request.workspace, &request.writer_token, request.epoch)?;
        if request.expected_version == 0 {
            return Err(ConnectError::invalid_argument(
                "checkout version is required",
            ));
        }
        let view = self
            .workspaces()?
            .edit_checkout(
                &principal,
                &writer,
                &request.checkout,
                request.expected_version,
                crate::workspace::CheckoutEdit::SwitchBranch {
                    branch: request.branch,
                    create: request.create,
                },
            )
            .await
            .map_err(connect_error)?;
        Ok(checkout_message(view).into())
    }

    async fn commit_workspace_notebooks(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::CommitWorkspaceNotebooksRequest>,
    ) -> ServiceResult<api::WorkspaceCheckout> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let request = request.to_owned_message();
        let writer = writer_context(&request.workspace, &request.writer_token, request.epoch)?;
        if request.expected_version == 0 {
            return Err(ConnectError::invalid_argument(
                "checkout version is required",
            ));
        }
        let view = self
            .workspaces()?
            .edit_checkout(
                &principal,
                &writer,
                &request.checkout,
                request.expected_version,
                crate::workspace::CheckoutEdit::Commit {
                    notebooks: request.notebooks,
                    message: request.message,
                },
            )
            .await
            .map_err(connect_error)?;
        Ok(checkout_message(view).into())
    }

    async fn create_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::CreateWorkspaceRequest>,
    ) -> ServiceResult<api::Workspace> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let service =
            self.state.durable_workspaces.as_deref().ok_or_else(|| {
                ConnectError::permission_denied("durable workspaces are disabled")
            })?;
        let result = service
            .create(&principal, &request.to_owned_message().name)
            .await
            .map_err(connect_error)?;
        Ok(workspace_message(result).into())
    }

    async fn takeover_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::TakeoverWorkspaceRequest>,
    ) -> ServiceResult<api::Workspace> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::WriteNotebook).await?;
        let service =
            self.state.durable_workspaces.as_deref().ok_or_else(|| {
                ConnectError::permission_denied("durable workspaces are disabled")
            })?;
        let request = request.to_owned_message();
        let result = service
            .takeover(&principal, &request.workspace, request.expected_epoch)
            .await
            .map_err(connect_error)?;
        Ok(workspace_message(result).into())
    }

    async fn open_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::OpenWorkspaceRequest>,
    ) -> ServiceResult<api::Workspace> {
        let principal =
            workspace_caller(&self.state, &ctx, aster_core::Action::ReadNotebook).await?;
        let service =
            self.state.durable_workspaces.as_deref().ok_or_else(|| {
                ConnectError::permission_denied("durable workspaces are disabled")
            })?;
        let request = request.to_owned_message();
        let result = service
            .open(
                &principal,
                &request.workspace,
                request.writer_token.as_deref(),
            )
            .await
            .map_err(connect_error)?;
        Ok(workspace_message(result).into())
    }

    async fn configure_workspace_repository(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ConfigureWorkspaceRepositoryRequest>,
    ) -> ServiceResult<api::WorkspaceRepository> {
        let principal =
            crate::current_identity::current_principal(&self.state, ctx.headers(), false)
                .await
                .map_err(|error| connect_error(error.0))?;
        let targets = self
            .state
            .team_git_targets
            .as_deref()
            .ok_or_else(|| ConnectError::permission_denied("team Git is disabled"))?;
        let request = request.to_owned_message();
        targets
            .maintainer(&request.team, &principal)
            .map_err(connect_error)?;
        let expected = match (request.expected_version, request.if_absent) {
            (None, true) => None,
            (Some(version), false) if version > 0 => Some(version),
            _ => {
                return Err(ConnectError::invalid_argument(
                    "one repository version precondition is required",
                ))
            }
        };
        let record = targets
            .configure_repository(
                &request.team,
                &principal,
                &request.repository,
                &request.default_branch,
                expected,
            )
            .await
            .map_err(connect_error)?;
        Ok(repository_message(record)?.into())
    }

    async fn list_workspace_repositories(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ListWorkspaceRepositoriesRequest>,
    ) -> ServiceResult<api::ListWorkspaceRepositoriesResponse> {
        let principal =
            crate::current_identity::current_principal(&self.state, ctx.headers(), false)
                .await
                .map_err(|error| connect_error(error.0))?;
        let targets = self
            .state
            .team_git_targets
            .as_deref()
            .ok_or_else(|| ConnectError::permission_denied("team Git is disabled"))?;
        let request = request.to_owned_message();
        let records = targets
            .repositories(&request.team, &principal)
            .await
            .map_err(connect_error)?;
        Ok(api::ListWorkspaceRepositoriesResponse {
            repositories: records
                .into_iter()
                .map(repository_message)
                .collect::<Result<_, _>>()?,
            ..Default::default()
        }
        .into())
    }

    async fn put_skill(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::PutSkillRequest>,
    ) -> ServiceResult<api::Skill> {
        let request = request.to_owned_message();
        let references = request
            .references
            .into_iter()
            .map(|reference| aster_core::skills::SkillReference {
                name: reference.name,
                content: reference.content,
            })
            .collect();
        Ok(skill_pb(
            crate::skills::put(
                &self.state,
                ctx.headers(),
                request.id,
                request.expected_revision,
                request.title,
                request.description,
                request.instructions,
                references,
            )
            .await
            .map_err(connect_error)?,
        )
        .into())
    }
    async fn delete_skill(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::DeleteSkillRequest>,
    ) -> ServiceResult<api::DeleteSkillResponse> {
        let request = request.to_owned_message();
        crate::skills::delete(
            &self.state,
            ctx.headers(),
            &request.id,
            request.expected_revision,
            request.expected_sharing_revision,
        )
        .await
        .map_err(connect_error)?;
        Ok(api::DeleteSkillResponse::default().into())
    }
    async fn share_skill(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ShareSkillRequest>,
    ) -> ServiceResult<api::Skill> {
        let request = request.to_owned_message();
        let audience = match request.audience.as_str() {
            "private" => aster_core::skills::SkillAudience::Private,
            "team" => aster_core::skills::SkillAudience::Team,
            "organization" => aster_core::skills::SkillAudience::Organization,
            _ => return Err(ConnectError::invalid_argument("invalid skill audience")),
        };
        Ok(skill_pb(
            crate::skills::share(
                &self.state,
                ctx.headers(),
                &request.id,
                request.expected_sharing_revision,
                audience,
                request.team,
            )
            .await
            .map_err(connect_error)?,
        )
        .into())
    }
    async fn get_personal_model_policy(
        &self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, api::GetPersonalModelPolicyRequest>,
    ) -> ServiceResult<api::PersonalModelPolicyResponse> {
        let current = crate::personal_models::current(&self.state, ctx.headers())
            .await
            .map_err(connect_error)?;
        let policy = self
            .state
            .personal_model_policy
            .get()
            .await
            .map_err(connect_error)?;
        let is_admin = authorize(&current, aster_core::Action::Administer).is_ok();
        Ok(api::PersonalModelPolicyResponse {
            revision: policy.revision,
            subjects: if is_admin { policy.subjects } else { vec![] },
            teams: if is_admin { policy.teams } else { vec![] },
            can_register: crate::personal_models::allowed(&self.state, &current)
                .await
                .map_err(connect_error)?,
            is_admin,
            ..Default::default()
        }
        .into())
    }
    async fn put_personal_model_policy(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::PutPersonalModelPolicyRequest>,
    ) -> ServiceResult<api::PersonalModelPolicyResponse> {
        let request = request.to_owned_message();
        let policy = crate::personal_models::replace(
            &self.state,
            ctx.headers(),
            request.expected_revision,
            request.subjects,
            request.teams,
        )
        .await
        .map_err(connect_error)?;
        let current = crate::personal_models::current(&self.state, ctx.headers())
            .await
            .map_err(connect_error)?;
        Ok(api::PersonalModelPolicyResponse {
            revision: policy.revision,
            subjects: policy.subjects,
            teams: policy.teams,
            can_register: crate::personal_models::allowed(&self.state, &current)
                .await
                .map_err(connect_error)?,
            is_admin: true,
            ..Default::default()
        }
        .into())
    }
    async fn put_personal_model(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::PutPersonalModelRequest>,
    ) -> ServiceResult<api::PersonalModelResponse> {
        let request = request.to_owned_message();
        let config = crate::ai::register_personal(
            &self.state,
            ctx.headers(),
            request.id,
            request.base_url,
            request.model,
            request.token,
        )
        .await
        .map_err(connect_error)?;
        Ok(api::PersonalModelResponse {
            r#ref: format!("personal/{}", config.id),
            id: config.id,
            base_url: config.base_url,
            model: config.model,
            ..Default::default()
        }
        .into())
    }
    async fn delete_personal_model(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::DeletePersonalModelRequest>,
    ) -> ServiceResult<api::DeletePersonalModelResponse> {
        let principal = caller(&self.state, &ctx).await?;
        self.state
            .llm
            .remove(&principal.subject, &request.to_owned_message().id)
            .await
            .map_err(connect_error)?;
        Ok(api::DeletePersonalModelResponse::default().into())
    }
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
            match request.session.as_deref() {
                Some(session) => self
                    .state
                    .conversations
                    .get_session(&principal.subject, &key, session)
                    .await
                    .map_err(connect_error)?,
                None => self
                    .state
                    .conversations
                    .get(&principal.subject, &key)
                    .await
                    .map_err(connect_error)?,
            }
        } else {
            let principal = caller(&self.state, &ctx).await?;
            crate::conversations::get(
                &self.state,
                &principal,
                &request.notebook,
                request.cell.as_deref(),
                request.session.as_deref(),
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

    async fn list_conversations(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::ListConversationsRequest>,
    ) -> ServiceResult<api::ListConversationsResponse> {
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
        let sessions = if let Some((principal, key, _)) = context {
            self.state
                .conversations
                .list(&principal.subject, &key)
                .await
        } else {
            let principal = caller(&self.state, &ctx).await?;
            crate::conversations::list(
                &self.state,
                &principal,
                &request.notebook,
                request.cell.as_deref(),
            )
            .await
        }
        .map_err(connect_error)?;
        Ok(api::ListConversationsResponse {
            sessions: sessions
                .into_iter()
                .map(|session| api::ConversationSummary {
                    id: session.id,
                    revision: session.revision,
                    phrase: session.phrase,
                    created_at_ms: session.created_at_ms,
                    updated_at_ms: session.updated_at_ms,
                    message_count: session.message_count as i32,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
        .into())
    }

    async fn delete_conversation(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::DeleteConversationRequest>,
    ) -> ServiceResult<api::DeleteConversationResponse> {
        let request = request.to_owned_message();
        let context = notebook_context(
            &self.state,
            &ctx,
            request.team.as_deref(),
            request.workspace.as_deref(),
            &request.notebook,
        )
        .await?;
        let expected = request
            .expected_revision
            .filter(|revision| *revision >= 0)
            .ok_or_else(|| {
                ConnectError::invalid_argument("expected nonnegative revision is required")
            })?;
        if !aster_core::llm::valid_id(&request.session) {
            return Err(ConnectError::invalid_argument("invalid session id"));
        }
        if context.is_some() && request.cell.is_some() {
            return Err(ConnectError::invalid_argument(
                "cell conversations are not available in team workspaces",
            ));
        }
        if let Some((principal, key, _)) = context {
            self.state
                .conversations
                .delete_session(&principal.subject, &key, &request.session, expected)
                .await
        } else {
            let principal = caller(&self.state, &ctx).await?;
            crate::conversations::delete(
                &self.state,
                &principal,
                &request.notebook,
                request.cell.as_deref(),
                &request.session,
                expected,
            )
            .await
        }
        .map_err(connect_error)?;
        Ok(api::DeleteConversationResponse::default().into())
    }

    async fn new_conversation(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::NewConversationRequest>,
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
                .create(&principal.subject, &key)
                .await
        } else {
            let principal = caller(&self.state, &ctx).await?;
            crate::conversations::create(
                &self.state,
                &principal,
                &request.notebook,
                request.cell.as_deref(),
            )
            .await
        }
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
        let skill = request
            .skill
            .as_option()
            .map(|skill| aster_core::skills::SkillSelection {
                id: skill.id.clone(),
                revision: skill.revision,
            });
        let selected_skills = request.selected_skills.as_option().map(|s| {
            s.skills
                .iter()
                .map(|s| aster_core::skills::SkillSelection {
                    id: s.id.clone(),
                    revision: s.revision,
                })
                .collect::<Vec<_>>()
        });
        if selected_skills.is_some() && skill.is_some() {
            return Err(ConnectError::invalid_argument("use one skill selector"));
        }
        let turn = crate::conversations::Turn {
            notebook: &request.notebook,
            cell: request.cell.as_deref(),
            session: request.session.as_deref(),
            helper: &request.helper,
            prompt: &request.prompt,
            context: &request.context,
            expected,
            contract_selection: contract_selection.as_ref(),
            skill: skill.as_ref(),
            selected_skills: selected_skills.as_deref(),
            selected_mcp: request
                .selected_mcp
                .as_option()
                .map(|s| s.connections.as_slice()),
            tool_confirmation: &request.tool_confirmation,
            cancel_tool_turn: request.cancel_tool_turn,
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

    async fn list_notebook_revisions(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetNotebookRequest>,
    ) -> ServiceResult<api::NotebookRevisions> {
        let request = request.to_owned_message();
        let revisions = if let Some((_, store)) = notebook_workspace(
            &self.state,
            &ctx,
            request.team.as_deref(),
            request.workspace.as_deref(),
        )
        .await?
        {
            store.revisions(&request.id).await.map_err(connect_error)?
        } else {
            let caller = caller(&self.state, &ctx).await?;
            authorize(&caller, aster_core::Action::ReadNotebook).map_err(connect_error)?;
            let _read = self.state.notebook_write.lock().await;
            self.state
                .admit_notebook(&request.id, &caller.subject)
                .await
                .map_err(connect_error)?;
            self.state
                .notebooks
                .revisions(&request.id)
                .await
                .map_err(connect_error)?
        };
        Ok(api::NotebookRevisions {
            revisions,
            ..Default::default()
        }
        .into())
    }

    async fn get_notebook_revision(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::GetNotebookRevisionRequest>,
    ) -> ServiceResult<api::NotebookRevision> {
        let request = request.to_owned_message();
        let notebook = if let Some((_, store)) = notebook_workspace(
            &self.state,
            &ctx,
            request.team.as_deref(),
            request.workspace.as_deref(),
        )
        .await?
        {
            store
                .revision(&request.id, &request.revision)
                .await
                .map_err(connect_error)?
        } else {
            let caller = caller(&self.state, &ctx).await?;
            authorize(&caller, aster_core::Action::ReadNotebook).map_err(connect_error)?;
            let _read = self.state.notebook_write.lock().await;
            self.state
                .admit_notebook(&request.id, &caller.subject)
                .await
                .map_err(connect_error)?;
            self.state
                .notebooks
                .revision(&request.id, &request.revision)
                .await
                .map_err(connect_error)?
        };
        let notebook_json = serde_json::to_string(&notebook)
            .map_err(|_| ConnectError::internal("notebook serialization failed"))?;
        Ok(api::NotebookRevision {
            notebook_json,
            ..Default::default()
        }
        .into())
    }

    async fn restore_notebook_revision(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, api::RestoreNotebookRevisionRequest>,
    ) -> ServiceResult<api::SaveNotebookResponse> {
        let request = request.to_owned_message();
        let caller = caller(&self.state, &ctx).await?;
        authorize(&caller, aster_core::Action::WriteNotebook).map_err(connect_error)?;
        let oid = &request.expected_content_revision;
        if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ConnectError::invalid_argument(
                "a notebook content precondition is required",
            ));
        }
        let expected = aster_core::NotebookPrecondition::Blob(oid.clone());
        let saved = if let Some((principal, store)) = notebook_workspace(
            &self.state,
            &ctx,
            request.team.as_deref(),
            request.workspace.as_deref(),
        )
        .await?
        {
            let notebook = store
                .revision(&request.id, &request.revision)
                .await
                .map_err(connect_error)?;
            store
                .save_if(&notebook, &principal.subject, expected)
                .await
                .map_err(connect_error)?
        } else {
            let notebook = {
                let _read = self.state.notebook_write.lock().await;
                self.state
                    .admit_notebook(&request.id, &caller.subject)
                    .await
                    .map_err(connect_error)?;
                self.state
                    .notebooks
                    .revision(&request.id, &request.revision)
                    .await
                    .map_err(connect_error)?
            };
            // A restore preserves an absent legacy context exactly; ordinary old-client saves inherit it.
            self.state
                .save_notebook_owned_context(&notebook, &caller.subject, expected, false)
                .await
                .map_err(connect_error)?
        };
        Ok(api::SaveNotebookResponse {
            revision: saved.revision,
            content_revision: saved.content_revision,
            ..Default::default()
        }
        .into())
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
            context: message.context.clone().into_option().map(|context| {
                aster_core::NotebookContext {
                    catalog_context: context.catalog_context,
                    schema: context.schema,
                }
            }),
            id: requested.id.clone(),
            title: message.title.clone(),
            engine: message.engine.clone().map(aster_core::EngineId::new),
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

        let result = query_message(result)?;
        if let (Some(notebook), Some(cell)) = (&requested.notebook, &requested.cell) {
            let names: Vec<String> = result
                .columns
                .iter()
                .map(|column| column.name.clone())
                .collect();
            if let Err(error) = crate::exchange::record_result(
                &self.state,
                &caller,
                notebook,
                cell,
                &names,
                &result.rows_json,
                result.truncated,
            )
            .await
            {
                tracing::warn!(%error, "cell result was not recorded for the session exchange");
            }
        }
        Ok(result.into())
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
    let pending_tool = value.pending.map(|turn| {
        let call = turn.call;
        api::PendingNotebookTool {
            connections: turn.user.mcp_connections,
            skills: turn
                .user
                .skill_dependencies
                .unwrap_or_default()
                .into_iter()
                .map(|s| api::SkillSelection {
                    id: s.id,
                    revision: s.revision,
                    ..Default::default()
                })
                .collect(),
            phase: turn.phase,
            connection: call
                .as_ref()
                .map(|c| c.connection.clone())
                .unwrap_or_default(),
            tool: call.as_ref().map(|c| c.tool.clone()).unwrap_or_default(),
            arguments_json: call
                .as_ref()
                .map(|c| c.arguments.clone())
                .unwrap_or_default(),
            confirmation: call.map(|c| c.confirmation).unwrap_or_default(),
            expires_at: turn.expires_at,
            ..Default::default()
        }
    });
    api::Conversation {
        pending_tool: pending_tool.into(),
        id: value.id,
        revision: value.revision,
        messages: value
            .messages
            .into_iter()
            .map(|m| api::ChatMessage {
                mcp_connections: m.mcp_connections,
                role: m.role,
                content: m.content,
                helper: m.helper,
                skills: m
                    .skill_dependencies
                    .unwrap_or_default()
                    .into_iter()
                    .map(|skill| api::SkillSelection {
                        id: skill.id,
                        revision: skill.revision,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn defaults_pb(value: aster_core::skills::SkillDefaults) -> api::SkillDefaults {
    api::SkillDefaults {
        revision: value.revision,
        skills: value
            .skills
            .into_iter()
            .map(|s| api::SkillSelection {
                id: s.id,
                revision: s.revision,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}
