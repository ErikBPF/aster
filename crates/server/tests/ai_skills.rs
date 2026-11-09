use std::sync::Arc;

use aster_core::*;
use aster_server::{app, AppState, DestinationPolicy, Keyring, Metrics, SharedModels};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use base64::Engine;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn skill_state() -> AppState {
    let keyset = json!({"v1": base64::engine::general_purpose::STANDARD.encode([7u8; 32])});
    let shared_models = SharedModels::new(
        Arc::new(InMemorySharedModels::default()),
        DestinationPolicy::parse("https://approved.example.invalid").unwrap(),
        Keyring::parse("v1", &keyset.to_string()).unwrap(),
    );
    let notebooks = Arc::new(SalesNotebook);
    let notebook_owners = Arc::new(InMemoryNotebookOwners::default());
    notebook_owners
        .change(NotebookOwnerChange {
            source: notebooks.source_key(),
            id: "sales".into(),
            expected_owner: None,
            owner: "alice".into(),
            source_blob: notebooks.snapshot("sales").await.unwrap().content_revision,
            actor: "alice".into(),
            reason: Some("shared-model fixture notebook".into()),
        })
        .await
        .unwrap();
    AppState {
        catalog_assets: None,
        skills: Arc::new(aster_core::skills::InMemorySkills::default()),
        mcp: None,
        personal_model_policy: Arc::new(aster_core::InMemoryPersonalModelPolicy::with_subjects(&[
            "alice", "bob", "admin", "viewer", "editor", "carol",
        ])),
        personal_model_destinations: None,
        personal_models_enabled: true,
        config: AppConfig {
            bind: String::new(),
            engines: vec![],
            catalogs: vec![],
            catalog_bindings: vec![],
            default_engine: None,
            default_catalog: None,
        },
        engines: EngineRegistry::new(),
        catalogs: CatalogRegistry::new(),
        grants: Arc::new(InMemoryGrants::new()),
        audit: Arc::new(InMemoryAudit::new()),
        notebooks,
        notebook_owners,
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: None,
        team_git_targets: None,
        llm: Arc::new(InMemoryLlm::new()),
        shared_models: Some(Arc::new(shared_models)),
        current_identity: None,
        shared_model_use_enabled: false,
        conversations: Arc::new(InMemoryConversations::default()),
        exchanges: Arc::new(InMemoryExchanges::default()),
        compiled_contracts: None,
        contracts: Arc::new(vec![]),
        http: reqwest::Client::new(),
        sessions: Arc::new(InMemorySessions::new(3600)),
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: true,
        metrics: Arc::new(Metrics::new()),
    }
}
async fn skill_app() -> Router {
    app(Arc::new(skill_state().await))
}

struct SalesNotebook;

#[async_trait::async_trait]
impl NotebookStore for SalesNotebook {
    async fn get(&self, id: &str) -> Result<Notebook> {
        if id != "sales" {
            return Err(CoreError::NotFound("notebook".into()));
        }
        Ok(Notebook {
            context: None,
            id: id.into(),
            title: "Sales".into(),
            engine: None,
            cells: vec![],
        })
    }
    async fn snapshot(&self, id: &str) -> Result<NotebookSnapshot> {
        Ok(NotebookSnapshot {
            notebook: self.get(id).await?,
            content_revision: "1".repeat(40),
        })
    }
    async fn save(&self, _: &Notebook, _: &str) -> Result<String> {
        Err(CoreError::NotFound("no notebooks".into()))
    }
    async fn list(&self, _: &str) -> Result<Vec<String>> {
        Ok(vec!["sales".into()])
    }
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    subject: &str,
    role: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("x-aster-subject", subject)
                .header("x-aster-roles", role)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

async fn rpc(
    app: &Router,
    method: &str,
    subject: &str,
    role: &str,
    body: Value,
) -> (StatusCode, Value) {
    request(
        app,
        "POST",
        &format!("/aster.v1.Aster/{method}"),
        subject,
        role,
        body,
    )
    .await
}
#[tokio::test]
async fn private_skills_are_versioned_and_owner_only() {
    let app = skill_app().await;
    let (status, first)=rpc(&app,"PutSkill","alice","editor",json!({"title":"SQL review","instructions":"Review joins","references":[{"name":"rules.md","content":"Check nulls"}]})).await;
    assert_eq!(status, StatusCode::OK, "registration: {first}");
    let created_at_ms = first["createdAtMs"]
        .as_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    assert!(
        created_at_ms > 1_000_000_000_000,
        "createdAtMs must use epoch milliseconds: {created_at_ms}"
    );
    let id = first["id"].as_str().unwrap();
    let (_, hidden) = rpc(&app, "ListSkills", "bob", "editor", json!({})).await;
    assert!(hidden["skills"]
        .as_array()
        .is_none_or(|rows| rows.is_empty()));
    assert_eq!(
        rpc(&app, "GetSkill", "bob", "editor", json!({"id":id}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rpc(
            &app,
            "PutSkill",
            "bob",
            "editor",
            json!({"id":id,"expectedRevision":"1","title":"stolen","instructions":"evil"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status,second)=rpc(&app,"PutSkill","alice","editor",json!({"id":id,"expectedRevision":"1","title":"SQL review","instructions":"Review joins and filters"})).await;
    assert_eq!(status, StatusCode::OK, "update {second}");
    let (status, old) = rpc(
        &app,
        "GetSkill",
        "alice",
        "editor",
        json!({"id":id,"revision":"1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(old["instructions"], "Review joins");
    assert_eq!(
        rpc(
            &app,
            "PutSkill",
            "alice",
            "editor",
            json!({"id":id,"expectedRevision":"1","title":"stale","instructions":"stale"})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, versions) = rpc(
        &app,
        "ListSkillVersions",
        "alice",
        "editor",
        json!({"id":id}),
    )
    .await;
    assert_eq!(versions["skills"].as_array().unwrap().len(), 2);
}
#[tokio::test]
async fn organization_sharing_requires_admin_and_revocation_hides_old_versions() {
    let app = skill_app().await;
    let (status, skill) = rpc(
        &app,
        "PutSkill",
        "alice",
        "editor",
        json!({"title":"Rules","instructions":"Rules"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{skill}");
    let id = skill["id"].as_str().unwrap();
    assert_eq!(
        rpc(
            &app,
            "ShareSkill",
            "alice",
            "editor",
            json!({"id":id,"expectedSharingRevision":"1","audience":"organization"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, shared) = rpc(
        &app,
        "ShareSkill",
        "alice",
        "admin",
        json!({"id":id,"expectedSharingRevision":"1","audience":"organization"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{shared}");
    assert_eq!(
        rpc(
            &app,
            "GetSkill",
            "bob",
            "editor",
            json!({"id":id,"revision":"1"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        rpc(
            &app,
            "ShareSkill",
            "alice",
            "admin",
            json!({"id":id,"expectedSharingRevision":"2","audience":"private"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        rpc(
            &app,
            "GetSkill",
            "bob",
            "editor",
            json!({"id":id,"revision":"1"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rpc(&app, "ListSkillVersions", "bob", "editor", json!({"id":id}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

struct Current(
    std::sync::Mutex<Option<CurrentIdentity>>,
    std::sync::atomic::AtomicUsize,
);
#[async_trait::async_trait]
impl CurrentIdentityProvider for Current {
    async fn current(&self, _: &str) -> Result<CurrentIdentity> {
        self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.0
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| CoreError::Storage("authority unavailable".into()))
    }
    async fn group_exists(&self, _: &str) -> Result<bool> {
        Ok(true)
    }
}
#[tokio::test]
async fn team_skill_rechecks_membership_and_fails_closed_on_authority_failure() {
    let mut state = skill_state().await;
    let group = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let user = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let current = CurrentIdentity {
        user_uuid: user.into(),
        active: true,
        roles: vec![Role::Editor],
        groups: vec![group.into()],
    };
    let authority = Arc::new(Current(
        std::sync::Mutex::new(Some(current.clone())),
        std::sync::atomic::AtomicUsize::new(0),
    ));
    state.current_identity = Some(authority.clone());
    let temporary = tempfile::tempdir().unwrap();
    state.team_workspaces = Some(Arc::new(
        aster_server::TeamWorkspaces::new(
            temporary.path(),
            std::collections::HashMap::from([(
                "analytics".into(),
                aster_server::TeamPolicy {
                    member_claim: group.into(),
                    maintainer_claim: group.into(),
                    allowed_repositories: std::collections::HashSet::from(["acme/data".into()]),
                    local_repositories: Default::default(),
                },
            )]),
        )
        .unwrap(),
    ));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let owner = state
        .sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec![group.into()],
                user_uuid: Some(user.into()),
            },
            None,
            now,
        )
        .await
        .unwrap();
    let reader = state
        .sessions
        .create_verified(
            &Identity {
                subject: "bob".into(),
                roles: vec![Role::Editor],
                groups: vec![group.into()],
                user_uuid: Some(user.into()),
            },
            None,
            now,
        )
        .await
        .unwrap();
    let app = app(Arc::new(state));
    let (_, skill) = rpc(
        &app,
        "PutSkill",
        "alice",
        "editor",
        json!({"title":"Team rules","instructions":"Private team rules"}),
    )
    .await;
    let id = skill["id"].as_str().unwrap();
    let owner_cookie = format!("aster_session={}", owner.sid);
    let reader_cookie = format!("aster_session={}", reader.sid);
    assert_eq!(
        cookie_rpc(
            &app,
            "ShareSkill",
            &owner_cookie,
            json!({"id":id,"expectedSharingRevision":"1","audience":"team","team":"analytics"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        cookie_rpc(
            &app,
            "GetSkill",
            &reader_cookie,
            json!({"id":id,"revision":"1"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        cookie_rpc(
            &app,
            "PutSkill",
            &reader_cookie,
            json!({"id":id,"expectedRevision":"1","title":"evil","instructions":"evil"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, second) = rpc(
        &app,
        "PutSkill",
        "alice",
        "editor",
        json!({"title":"Other rules","instructions":"Other team rules"}),
    )
    .await;
    assert_eq!(cookie_rpc(&app,"ShareSkill",&owner_cookie,json!({"id":second["id"],"expectedSharingRevision":"1","audience":"team","team":"analytics"})).await.0,StatusCode::OK);
    let before = authority.1.load(std::sync::atomic::Ordering::SeqCst);
    let (status, listed) = cookie_rpc(&app, "ListSkills", &reader_cookie, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["skills"].as_array().unwrap().len(), 2);
    assert_eq!(
        authority.1.load(std::sync::atomic::Ordering::SeqCst) - before,
        1,
        "one current-authority snapshot for the whole list"
    );
    let mut demoted = current.clone();
    demoted.roles = vec![Role::Viewer];
    *authority.0.lock().unwrap() = Some(demoted);
    assert_eq!(
        cookie_rpc(
            &app,
            "ShareSkill",
            &owner_cookie,
            json!({"id":id,"expectedSharingRevision":"2","audience":"team","team":"analytics"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let mut removed = current.clone();
    removed.groups.clear();
    *authority.0.lock().unwrap() = Some(removed);
    assert_eq!(
        cookie_rpc(
            &app,
            "GetSkill",
            &reader_cookie,
            json!({"id":id,"revision":"1"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, hidden) = cookie_rpc(&app, "ListSkills", &reader_cookie, json!({})).await;
    assert!(hidden["skills"]
        .as_array()
        .is_none_or(|rows| rows.is_empty()));
    *authority.0.lock().unwrap() = None;
    assert_eq!(
        cookie_rpc(
            &app,
            "GetSkill",
            &reader_cookie,
            json!({"id":id,"revision":"1"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}
async fn cookie_rpc(app: &Router, method: &str, cookie: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/aster.v1.Aster/{method}"))
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn portable_skill_metadata_and_nested_references_round_trip() {
    let app = skill_app().await;
    let (status, saved) = rpc(&app, "PutSkill", "alice", "editor", json!({
        "title":"SQL review", "description":"Review joins and null handling",
        "instructions":"# SQL Review", "references":[{"name":"references/checklist.md","content":"Check grain"}]
    })).await;
    assert_eq!(status, StatusCode::OK, "nested text reference: {saved}");
    assert_eq!(saved["description"], "Review joins and null handling");
    let (_, listed) = rpc(&app, "ListSkills", "alice", "editor", json!({})).await;
    assert_eq!(
        listed["skills"][0]["description"],
        "Review joins and null handling"
    );
    let id = saved["id"].as_str().unwrap();
    let (_, reopened) = rpc(
        &app,
        "GetSkill",
        "alice",
        "editor",
        json!({"id":id,"revision":"1"}),
    )
    .await;
    assert_eq!(reopened["references"][0]["name"], "references/checklist.md");
    for path in [
        "../secret",
        "refs/../secret",
        "/etc/passwd",
        "refs//guide.md",
        "SKILL.md",
        "refs/./guide.md",
        "refs\\guide.md",
    ] {
        let (status, _) = rpc(&app, "PutSkill", "alice", "editor", json!({"title":"Bad path","instructions":"text","references":[{"name":path,"content":"x"}]})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "unsafe path {path}");
    }
    let (status, _) = rpc(&app, "PutSkill", "alice", "editor", json!({"title":"Collision","instructions":"text","references":[{"name":"refs","content":"x"},{"name":"refs/a.md","content":"y"}]})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "file/directory collision");
}

#[tokio::test]
async fn mcp_registry_reports_configuration_without_exposing_secrets() {
    let app = skill_app().await;
    let (status, body) = rpc(&app, "ListMcpConnections", "alice", "editor", json!({})).await;
    assert_eq!(status, StatusCode::OK, "MCP registry: {body}");
    assert!(!body["configured"].as_bool().unwrap_or(false));
    assert!(body["connections"]
        .as_array()
        .is_none_or(|items| items.is_empty()));
}

#[tokio::test]
async fn mcp_registry_is_admin_managed_encrypted_and_freshly_scoped() {
    let mut state = skill_state().await;
    let store = Arc::new(aster_core::mcp::InMemoryMcp::default());
    state.mcp = Some(Arc::new(aster_server::mcp::McpRegistry::new(
        store.clone(),
        DestinationPolicy::parse("https://tools.example.invalid").unwrap(),
        Keyring::parse(
            "v1",
            &json!({"v1": base64::engine::general_purpose::STANDARD.encode([9u8;32])}).to_string(),
        )
        .unwrap(),
        Arc::new(aster_server::HttpMcp),
    )));
    let app = app(Arc::new(state));
    let body = json!({"id":"tools","name":"Analytics tools","description":"Shared SQL tools","url":"https://tools.example.invalid/mcp","auth":"bearer","token":"fixture-only-token","enabled":true,"subjects":["alice"]});
    assert_eq!(
        rpc(&app, "PutMcpConnection", "alice", "editor", body.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (status, saved) = rpc(&app, "PutMcpConnection", "admin", "admin", body.clone()).await;
    assert_eq!(status, StatusCode::OK, "save: {saved}");
    assert!(!saved.to_string().contains("fixture-only-token"));
    let rows = aster_core::mcp::McpStore::list(store.as_ref())
        .await
        .unwrap();
    assert!(!serde_json::to_string(&rows[0])
        .unwrap()
        .contains("fixture-only-token"));
    let (_, alice) = rpc(&app, "ListMcpConnections", "alice", "editor", json!({})).await;
    assert_eq!(alice["connections"][0]["name"], "Analytics tools");
    assert!(alice["connections"][0]["url"]
        .as_str()
        .is_none_or(str::is_empty));
    let (_, bob) = rpc(&app, "ListMcpConnections", "bob", "editor", json!({})).await;
    assert!(bob["connections"]
        .as_array()
        .is_none_or(|rows| rows.is_empty()));
    assert_eq!(
        rpc(&app, "PutMcpConnection", "admin", "admin", body.clone())
            .await
            .0,
        StatusCode::CONFLICT
    );
    let mut disabled = body.clone();
    disabled["expectedRevision"] = json!("1");
    disabled["enabled"] = json!(false);
    disabled["token"] = json!("");
    assert_eq!(
        rpc(&app, "PutMcpConnection", "admin", "admin", disabled)
            .await
            .0,
        StatusCode::OK
    );
    let (_, alice) = rpc(&app, "ListMcpConnections", "alice", "editor", json!({})).await;
    assert!(alice["connections"]
        .as_array()
        .is_none_or(|rows| rows.is_empty()));
    let mut wrong = body;
    wrong["url"] = json!("https://unapproved.example/mcp");
    assert_eq!(
        rpc(&app, "PutMcpConnection", "admin", "admin", wrong)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn mcp_calls_require_exact_single_use_confirmation_unless_admin_approved_read() {
    use axum::{response::IntoResponse, routing::post, Json};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let lists = Arc::new(AtomicUsize::new(0));
    let pause_at = Arc::new(AtomicUsize::new(usize::MAX));
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    let (remote_lists, remote_pause, remote_entered, remote_resume) = (
        lists.clone(),
        pause_at.clone(),
        entered.clone(),
        resume.clone(),
    );
    let remote=Router::new().route("/mcp",post(move |Json(body):Json<Value>| {
        let calls=observed.clone();
        let (lists,pause,entered,resume)=(remote_lists.clone(),remote_pause.clone(),remote_entered.clone(),remote_resume.clone());
        async move {
            if body["method"] == "tools/list" && lists.fetch_add(1,Ordering::SeqCst)+1 == pause.load(Ordering::SeqCst) { entered.notify_one(); resume.notified().await; }
            let result=match body["method"].as_str().unwrap_or("") {
                "initialize"=>json!({"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"Donsetch mock","version":"1"}}),
                "tools/list"=>json!({"tools":[{"name":"inspect_catalog","description":"Mock read","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},{"name":"draft_note","description":"Mock changing action","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false}}]}),
                "tools/call"=>{calls.fetch_add(1,Ordering::SeqCst);json!({"content":[{"type":"text","text":"Mock result"}]})},
                _=>json!({})
            };
            let reply=json!({"jsonrpc":"2.0","id":body["id"],"result":result});
            if body["method"] == "tools/list" {
                ([("content-type","text/event-stream")],format!("data: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}}\n\ndata: {reply}\n\n")).into_response()
            } else { Json(reply).into_response() }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, remote).await.unwrap() });
    let mut state = skill_state().await;
    state.mcp = Some(Arc::new(
        aster_server::mcp::McpRegistry::new(
            Arc::new(aster_core::mcp::InMemoryMcp::default()),
            DestinationPolicy::parse("").unwrap(),
            Keyring::parse(
                "v1",
                &json!({"v1":base64::engine::general_purpose::STANDARD.encode([3u8;32])})
                    .to_string(),
            )
            .unwrap(),
            Arc::new(aster_server::HttpMcp),
        )
        .with_development_endpoint(url.clone())
        .unwrap(),
    ));
    let app = app(Arc::new(state));
    let (status,body)=rpc(&app,"PutMcpConnection","admin","admin",json!({"id":"donsetch","name":"Donsetch mock","url":url,"auth":"none","enabled":true,"subjects":["alice"]})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, tools) = rpc(
        &app,
        "DiscoverMcpTools",
        "alice",
        "editor",
        json!({"id":"donsetch"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "discover: {tools}");
    assert_eq!(tools["tools"].as_array().unwrap().len(), 2);
    let request =
        json!({"id":"donsetch","tool":"draft_note","argumentsJson":"{\"text\":\"hello\"}"});
    let (status, pending) = rpc(&app, "CallMcpTool", "alice", "editor", request.clone()).await;
    assert_eq!(status, StatusCode::OK, "pending: {pending}");
    let nonce = pending["confirmation"].as_str().unwrap();
    assert!(!nonce.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let mut confirmed = request.clone();
    confirmed["confirmation"] = json!(nonce);
    let mut altered = confirmed.clone();
    altered["argumentsJson"] = json!("{\"text\":\"changed\"}");
    assert_eq!(
        rpc(&app, "CallMcpTool", "alice", "editor", altered).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rpc(&app, "CallMcpTool", "bob", "editor", confirmed.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rpc(&app, "CallMcpTool", "alice", "editor", confirmed.clone())
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        rpc(&app, "CallMcpTool", "alice", "editor", confirmed)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let hash = tools["tools"][0]["schemaHash"].as_str().unwrap();
    let approval = json!({"id":"donsetch","expectedRevision":"1","tool":"inspect_catalog","schemaHash":hash,"approved":true});
    assert_eq!(
        rpc(&app, "ApproveMcpRead", "alice", "editor", approval.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rpc(&app, "ApproveMcpRead", "admin", "admin", approval)
            .await
            .0,
        StatusCode::OK
    );
    let (status, read) = rpc(
        &app,
        "CallMcpTool",
        "alice",
        "editor",
        json!({"id":"donsetch","tool":"inspect_catalog","argumentsJson":"{}"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "read: {read}");
    assert!(read["confirmation"].as_str().is_none_or(str::is_empty));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        rpc(
            &app,
            "CallMcpTool",
            "alice",
            "editor",
            json!({"id":"donsetch","tool":"draft_note","argumentsJson":"{}"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (_, pending) = rpc(&app, "CallMcpTool", "alice", "editor", request.clone()).await;
    let mut pending_request = request;
    pending_request["confirmation"] = pending["confirmation"].clone();
    pause_at.store(lists.load(Ordering::SeqCst) + 2, Ordering::SeqCst);
    let running_app = app.clone();
    let running = tokio::spawn(async move {
        rpc(
            &running_app,
            "CallMcpTool",
            "alice",
            "editor",
            pending_request,
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), entered.notified())
        .await
        .expect("second discovery must pause");
    let (status,disabled)=rpc(&app,"PutMcpConnection","admin","admin",json!({"id":"donsetch","name":"Donsetch mock","url":url,"auth":"none","enabled":false,"subjects":["alice"],"expectedRevision":"2"})).await;
    assert_eq!(status, StatusCode::OK, "disable: {disabled}");
    resume.notify_one();
    let (status, body) = running.await.unwrap();
    assert!(
        matches!(status, StatusCode::FORBIDDEN | StatusCode::CONFLICT),
        "revocation during discovery must refuse the call: {status} {body}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn deleting_skill_checks_owner_and_revisions_and_revokes_all_versions() {
    let state = Arc::new(skill_state().await);
    let app = app(state.clone());
    let (_, first) = rpc(
        &app,
        "PutSkill",
        "alice",
        "editor",
        json!({"title":"Disposable","instructions":"Version one"}),
    )
    .await;
    let id = first["id"].as_str().unwrap();
    let body = json!({"id":id,"expectedRevision":"1","expectedSharingRevision":"1"});
    assert_eq!(
        rpc(&app, "DeleteSkill", "bob", "admin", body.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rpc(&app, "DeleteSkill", "alice", "viewer", body.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (_, second) = rpc(
        &app,
        "PutSkill",
        "alice",
        "editor",
        json!({"id":id,"expectedRevision":"1","title":"Disposable","instructions":"Version two"}),
    )
    .await;
    assert_eq!(second["revision"], "2");
    assert_eq!(
        rpc(&app, "DeleteSkill", "alice", "editor", body).await.0,
        StatusCode::CONFLICT
    );
    let body = json!({"id":id,"expectedRevision":"2","expectedSharingRevision":"1"});
    assert_eq!(
        rpc(
            &app,
            "ShareSkill",
            "alice",
            "editor",
            json!({"id":id,"expectedSharingRevision":"1","audience":"private"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        rpc(&app, "DeleteSkill", "alice", "editor", body).await.0,
        StatusCode::CONFLICT
    );
    let body = json!({"id":id,"expectedRevision":"2","expectedSharingRevision":"2"});
    assert_eq!(
        rpc(&app, "DeleteSkill", "alice", "editor", body.clone())
            .await
            .0,
        StatusCode::OK
    );
    for revision in [1, 2] {
        assert_eq!(
            rpc(
                &app,
                "GetSkill",
                "alice",
                "editor",
                json!({"id":id,"revision":revision.to_string()})
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        rpc(
            &app,
            "ListSkillVersions",
            "alice",
            "editor",
            json!({"id":id})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert!(state.skills.list().await.unwrap().is_empty());
    assert!(
        state
            .skills
            .put(aster_core::skills::SkillDraft {
                id: id.into(),
                owner: "alice".into(),
                title: "Reclaimed".into(),
                description: String::new(),
                instructions: "New content".into(),
                references: vec![],
                expected_revision: 0,
                created_at_ms: 1
            })
            .await
            .is_err(),
        "deleted identifiers must never acquire replacement content"
    );
    assert_eq!(
        rpc(&app, "DeleteSkill", "alice", "editor", body).await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn notebook_mcp_selection_confirmation_and_replay_are_scoped() {
    use axum::{routing::post, Json};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let remote=Router::new().route("/mcp",post(move |Json(body):Json<Value>| {
        let calls=observed.clone();
        async move {
            let result=match body["method"].as_str().unwrap_or("") {
                "initialize"=>json!({"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}}),
                "tools/list"=>json!({"tools":[{"name":"read","inputSchema":{"type":"object"}},{"name":"write","inputSchema":{"type":"object"}}]}),
                "tools/call"=>{calls.fetch_add(1,Ordering::SeqCst);if body["params"]["arguments"]["unknown"]==true {return Json(json!({"jsonrpc":"2.0","id":body["id"],"error":{"code":-32000,"message":"unknown outcome"}}));}json!({"content":[{"type":"text","text":"TOOL_RESULT"}]})},
                _=>json!({})
            };
            Json(json!({"jsonrpc":"2.0","id":body["id"],"result":result}))
        }
    })).route("/v1/chat/completions",post(|Json(body):Json<Value>| async move {
        let messages=body["messages"].as_array().unwrap();
        let last=messages.last().unwrap();
        if last["role"] == "tool" || body["tools"].is_null() {
            Json(json!({"choices":[{"message":{"content":"Completed with authorized context."}}]}))
        } else {
            let target=if last["content"].as_str().unwrap_or("").contains("write") {"write"} else {"read"};
            let tool=body["tools"].as_array().unwrap().iter().find(|t| t["function"]["description"].as_str().unwrap_or("").contains(target)).unwrap();
            Json(json!({"choices":[{"message":{"content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":tool["function"]["name"],"arguments":if last["content"].as_str().unwrap_or("").contains("unknown") {"{\"unknown\":true}"} else {"{}"}}}]}}]}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, remote).await.unwrap() });
    let mut state = skill_state().await;
    state
        .llm
        .put(LlmConfig {
            subject: "alice".into(),
            id: "go".into(),
            base_url: origin.clone(),
            model: "fixture".into(),
            api_key: String::new(),
        })
        .await
        .unwrap();
    state.mcp = Some(Arc::new(
        aster_server::mcp::McpRegistry::new(
            Arc::new(aster_core::mcp::InMemoryMcp::default()),
            DestinationPolicy::parse("").unwrap(),
            Keyring::parse(
                "v1",
                &json!({"v1":base64::engine::general_purpose::STANDARD.encode([3u8;32])})
                    .to_string(),
            )
            .unwrap(),
            Arc::new(aster_server::HttpMcp),
        )
        .with_development_endpoint(format!("{origin}/mcp"))
        .unwrap(),
    ));
    let app = app(Arc::new(state));
    let (status, connection)=rpc(&app,"PutMcpConnection","admin","admin",json!({"id":"donsetch","name":"Donsetch","url":format!("{origin}/mcp"),"auth":"none","enabled":true,"subjects":["alice"],"expectedRevision":"0"})).await;
    assert_eq!(status, 200, "{connection}");
    let (_, discovered) = rpc(
        &app,
        "DiscoverMcpTools",
        "admin",
        "admin",
        json!({"id":"donsetch"}),
    )
    .await;
    let hash = &discovered["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "read")
        .unwrap()["schemaHash"];
    let (status, _)=rpc(&app,"ApproveMcpRead","admin","admin",json!({"id":"donsetch","expectedRevision":connection["revision"],"tool":"read","schemaHash":hash,"approved":true})).await;
    assert_eq!(status, 200);
    let (status, read)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","helper":"go","prompt":"read","expectedRevision":"0","selectedMcp":{"connections":["donsetch"]}})).await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(read["messages"][1]["mcpConnections"], json!(["donsetch"]));
    let (status, pending)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","helper":"go","prompt":"write","expectedRevision":read["revision"],"session":read["id"],"selectedMcp":{"connections":["donsetch"]}})).await;
    assert_eq!(status, 200, "{pending}");
    assert_eq!(pending["pendingTool"]["tool"], "write");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "changing tool must wait");
    let (_, new_session) = rpc(
        &app,
        "NewConversation",
        "alice",
        "editor",
        json!({"notebook":"sales"}),
    )
    .await;
    let (status,_)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":new_session["id"],"expectedRevision":new_session["revision"],"toolConfirmation":pending["pendingTool"]["confirmation"]})).await;
    assert_ne!(status, 200, "confirmation cannot move sessions");
    let resume = json!({"notebook":"sales","session":pending["id"],"expectedRevision":pending["revision"],"toolConfirmation":pending["pendingTool"]["confirmation"]});
    let (a, b) = tokio::join!(
        rpc(&app, "SendMessage", "alice", "editor", resume.clone()),
        rpc(&app, "SendMessage", "alice", "editor", resume)
    );
    assert!(a.0 == 200 || b.0 == 200, "{a:?} {b:?}");
    assert!(a.0 != 200 || b.0 != 200);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "only one mutation dispatch"
    );
    let done = if a.0 == 200 { a.1 } else { b.1 };
    let (status,off)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":done["id"],"expectedRevision":done["revision"],"helper":"go","prompt":"read","selectedMcp":{"connections":[]}})).await;
    assert_eq!(status, 200, "{off}");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "disabled MCP must not execute"
    );
    let (status,pending)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":off["id"],"expectedRevision":off["revision"],"helper":"go","prompt":"write cancel","selectedMcp":{"connections":["donsetch"]}})).await;
    assert_eq!(status, 200, "{pending}");
    let (status, _) = rpc(&app,"CallMcpTool","alice","editor",json!({"id":"donsetch","tool":"write","argumentsJson":"{}","confirmation":pending["pendingTool"]["confirmation"]})).await;
    assert_eq!(
        status, 403,
        "notebook confirmation must not work through standalone tools"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let (status,cancelled)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":pending["id"],"expectedRevision":pending["revision"],"cancelToolTurn":true})).await;
    assert_eq!(status, 200, "{cancelled}");
    assert_eq!(calls.load(Ordering::SeqCst), 2, "cancel must not dispatch");
    let (status, _) = rpc(&app,"CallMcpTool","alice","editor",json!({"id":"donsetch","tool":"write","argumentsJson":"{}","confirmation":pending["pendingTool"]["confirmation"]})).await;
    assert_eq!(
        status, 403,
        "cancelled notebook confirmation must not execute"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let (status,pending)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":cancelled["id"],"expectedRevision":cancelled["revision"],"helper":"go","prompt":"write unknown","selectedMcp":{"connections":["donsetch"]}})).await;
    assert_eq!(status, 200, "{pending}");
    let (status,failed)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":pending["id"],"expectedRevision":pending["revision"],"toolConfirmation":pending["pendingTool"]["confirmation"]})).await;
    assert_eq!(status, 200, "{failed}");
    assert_eq!(failed["pendingTool"]["phase"], "failed");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let (status,_)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":failed["id"],"expectedRevision":failed["revision"],"toolConfirmation":pending["pendingTool"]["confirmation"]})).await;
    assert_ne!(status, 200);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "unknown outcome must not retry"
    );
    let (_,cancelled)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":failed["id"],"expectedRevision":failed["revision"],"cancelToolTurn":true})).await;
    let (_, listed) = rpc(&app, "ListMcpConnections", "admin", "admin", json!({})).await;
    let current = &listed["connections"][0];
    let (status,_)=rpc(&app,"PutMcpConnection","admin","admin",json!({"id":"donsetch","name":"Donsetch","url":format!("{origin}/mcp"),"auth":"none","enabled":false,"subjects":["alice"],"expectedRevision":current["revision"]})).await;
    assert_eq!(status, 200);
    let (status,_)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":cancelled["id"],"expectedRevision":cancelled["revision"],"helper":"go","prompt":"off but history revoked","selectedMcp":{"connections":[]}})).await;
    assert_eq!(
        status, 403,
        "revoked history cannot replay even with tools off"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    if std::env::var_os("ASTER_NOTEBOOK_BROWSER").is_some() {
        let (_, listed) = rpc(&app, "ListMcpConnections", "admin", "admin", json!({})).await;
        let (status,_)=rpc(&app,"PutMcpConnection","admin","admin",json!({"id":"donsetch","name":"Donsetch","url":format!("{origin}/mcp"),"auth":"none","enabled":true,"subjects":["alice"],"expectedRevision":listed["connections"][0]["revision"]})).await;
        assert_eq!(status, 200);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let browser_url = format!("http://{}", listener.local_addr().unwrap());
        let ui = app.clone();
        let ui_server = tokio::spawn(async move { axum::serve(listener, ui).await.unwrap() });
        let result = tokio::task::spawn_blocking(move || {
            std::process::Command::new("python3")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../tests/notebook-ai-live-browser.py"
                ))
                .arg(browser_url)
                .status()
        })
        .await
        .unwrap()
        .unwrap();
        ui_server.abort();
        assert!(result.success(), "notebook browser failed");
    }
    server.abort();
}

#[tokio::test]
async fn protected_notebook_can_cancel_its_persisted_tool_turn() {
    let mut state = skill_state().await;
    state.config.catalog_bindings.push(CatalogBindingConfig {
        catalog: "protected".into(),
        engine: "fixture".into(),
        native_catalog: "protected".into(),
        policy: BindingPolicy::Protected,
    });
    let mut session = state.conversations.get("alice", "sales").await.unwrap();
    session.pending = Some(aster_core::conversation::ToolTurn {
        id: "fixture-turn".into(),
        user: ChatMessage {
            role: "user".into(),
            content: "authorized pending request".into(),
            helper: "go".into(),
            skill_dependencies: Some(vec![]),
            mcp_connections: vec![],
            contract_dependencies: Some(vec![]),
            catalog_dependencies: Some(vec![]),
        },
        messages: vec![],
        tools: vec![],
        call: None,
        phase: "confirmation".into(),
        calls: 0,
        expires_at: 0,
    });
    let session = state
        .conversations
        .save_turn("alice", "sales", 0, session)
        .await
        .unwrap();
    let app = app(Arc::new(state));
    let (status,cancelled)=rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":session.id,"expectedRevision":session.revision.to_string(),"cancelToolTurn":true})).await;
    assert_eq!(status, 200, "{cancelled}");
    assert!(cancelled.get("pendingTool").is_none());
    assert!(cancelled["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("cancelled"));
}
