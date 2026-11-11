use aster_core::*;
use aster_server::{app, AppState, Metrics};
use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Request},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

struct Notebooks;
#[async_trait::async_trait]
impl NotebookStore for Notebooks {
    async fn get(&self, id: &str) -> Result<Notebook> {
        if !["sales", "other"].contains(&id) {
            return Err(CoreError::NotFound("notebook".into()));
        }
        Ok(Notebook {
            context: None,
            id: id.into(),
            title: id.into(),
            engine: None,
            cells: vec![],
        })
    }
    async fn list(&self, _: &str) -> Result<Vec<String>> {
        Ok(vec!["sales".into(), "other".into()])
    }
    async fn save(&self, _: &Notebook, _: &str) -> Result<String> {
        unreachable!()
    }
    async fn snapshot(&self, id: &str) -> Result<NotebookSnapshot> {
        Ok(NotebookSnapshot {
            notebook: self.get(id).await?,
            content_revision: "fixture".into(),
        })
    }
}

async fn setup() -> (Router, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
    setup_store(Arc::new(InMemoryConversations::default())).await
}

async fn setup_store(
    store: Arc<dyn ConversationStore>,
) -> (Router, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
    setup_with(store, CatalogRegistry::new(), vec![]).await
}

async fn setup_with(
    store: Arc<dyn ConversationStore>,
    catalogs: CatalogRegistry,
    contracts: Vec<DataContract>,
) -> (Router, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let mock = Router::new().route(
        "/v1/chat/completions",
        post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let seen = captured.clone();
            async move {
                assert!(headers.get("x-opencode-session").is_some());
                assert!(headers.get("cookie").is_none());
                let mut record = body.clone();
                record["_session"] = json!(headers["x-opencode-session"].to_str().unwrap());
                seen.lock().unwrap().push(record);
                if body["messages"].as_array().unwrap().last().unwrap()["content"]
                    .as_str()
                    .unwrap()
                    .contains("fail-upstream")
                {
                    return (
                        axum::http::StatusCode::BAD_GATEWAY,
                        Json(json!({"error":"failed"})),
                    );
                }
                if body["messages"].as_array().unwrap().last().unwrap()["content"]
                    == "oversized-reply"
                {
                    return (
                        axum::http::StatusCode::OK,
                        Json(json!({"choices":[{"message":{"content":"x".repeat(32769)}}]})),
                    );
                }
                (
                    axum::http::StatusCode::OK,
                    Json(json!({"choices":[{"message":{"content":"Use SELECT 42 AS answer."}}]})),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let llm = Arc::new(InMemoryLlm::new());
    llm.put(LlmConfig {
        subject: "alice".into(),
        id: "go".into(),
        base_url: endpoint,
        model: "test".into(),
        api_key: "fake".into(),
    })
    .await
    .unwrap();
    let owners = Arc::new(InMemoryNotebookOwners::default());
    for id in ["sales", "other"] {
        owners
            .change(NotebookOwnerChange {
                source: "unconfigured".into(),
                id: id.into(),
                expected_owner: None,
                owner: "alice".into(),
                source_blob: "fixture".into(),
                actor: "alice".into(),
                reason: None,
            })
            .await
            .unwrap();
    }
    let state = Arc::new(AppState {
        catalog_assets: None,
        skills: Arc::new(aster_core::skills::InMemorySkills::default()),
        mcp: None,
        personal_model_policy: Arc::new(aster_core::InMemoryPersonalModelPolicy::with_subjects(&[
            "alice", "bob", "admin", "viewer", "editor", "carol",
        ])),
        personal_model_destinations: None,
        personal_models_enabled: true,
        conversations: store,
        exchanges: Arc::new(InMemoryExchanges::default()),
        config: AppConfig {
            bind: "".into(),
            engines: vec![],
            catalogs: vec![],
            catalog_bindings: vec![CatalogBindingConfig {
                catalog: "polaris".into(),
                engine: "fixture".into(),
                native_catalog: "polaris".into(),
                policy: BindingPolicy::Unprotected,
            }],
            default_engine: None,
            default_catalog: None,
        },
        engines: EngineRegistry::new(),
        catalogs,
        grants: Arc::new(InMemoryGrants::new()),
        audit: Arc::new(InMemoryAudit::new()),
        notebooks: Arc::new(Notebooks),
        notebook_owners: owners,
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: None,
        durable_workspaces: None,
        team_git_targets: None,
        llm,
        shared_models: None,
        current_identity: None,
        shared_model_use_enabled: false,
        compiled_contracts: None,
        contracts: Arc::new(contracts),
        http: reqwest::Client::new(),
        sessions: Arc::new(InMemorySessions::new(3600)),
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: true,
        metrics: Arc::new(Metrics::new()),
    });
    (app(state), seen, task)
}
async fn rpc(app: &Router, method: &str, who: &str, role: &str, body: Value) -> (u16, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri(format!("/aster.v1.Aster/{method}"))
        .header("content-type", "application/json")
        .header("connect-protocol-version", "1")
        .header("x-opencode-session", "forged-browser-session");
    if !who.is_empty() {
        req = req.header("cookie", format!("aster_subject={who}; aster_roles={role}"));
    }
    let response = app
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
#[tokio::test]
async fn conversations_replay_private_history_and_reject_failed_or_stale_turns() {
    let (app, seen, task) = setup().await;
    let (status, first) = rpc(
        &app,
        "GetConversation",
        "alice",
        "editor",
        json!({"notebook":"sales"}),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    let send = |prompt: &str, revision: &str| json!({"notebook":"sales","helper":"go","prompt":prompt,"expectedRevision":revision});
    let (status, turn) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        send("Remember revenue", "0"),
    )
    .await;
    assert_eq!(status, 200, "{turn}");
    assert_eq!(turn["messages"].as_array().unwrap().len(), 2);
    let (status, next) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        send("Explain that query", "1"),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    assert_eq!(next["messages"].as_array().unwrap().len(), 4);
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_ne!(seen[0]["_session"], seen[1]["_session"]);
        assert_ne!(seen[0]["_session"], "forged-browser-session");
        assert_ne!(seen[1]["_session"], "forged-browser-session");
        assert_eq!(
            next["id"], first["id"],
            "local conversation identity stays stable"
        );
        let messages = seen[1]["messages"].as_array().unwrap();
        assert!(messages.iter().any(|m| m["content"] == "Remember revenue"));
        assert!(messages
            .iter()
            .any(|m| m["role"] == "assistant" && m["content"] == "Use SELECT 42 AS answer."));
    }
    for (who, notebook) in [("bob", "sales"), ("alice", "other")] {
        let (status, other) = rpc(
            &app,
            "GetConversation",
            who,
            "editor",
            json!({"notebook":notebook}),
        )
        .await;
        assert_eq!(status, if who == "bob" { 403 } else { 200 });
        assert!(other
            .get("messages")
            .is_none_or(|m| m.as_array().unwrap().is_empty()));
    }
    let (status, _) = rpc(&app, "SendMessage", "alice", "editor", send("stale", "0")).await;
    assert_eq!(status, 409);
    let count = seen.lock().unwrap().len();
    assert_eq!(
        rpc(&app, "SendMessage", "alice", "viewer", send("no", "2"))
            .await
            .0,
        403
    );
    assert_eq!(
        rpc(&app, "GetConversation", "", "", json!({"notebook":"sales"}))
            .await
            .0,
        401
    );
    assert_eq!(seen.lock().unwrap().len(), count);
    assert_eq!(
        rpc(
            &app,
            "SendMessage",
            "alice",
            "editor",
            send("fail-upstream", "2")
        )
        .await
        .0,
        500
    );
    let (_, history) = rpc(
        &app,
        "GetConversation",
        "alice",
        "editor",
        json!({"notebook":"sales"}),
    )
    .await;
    assert_eq!(history["messages"].as_array().unwrap().len(), 4);
    assert_eq!(history["id"], first["id"]);
    task.abort();
}

struct FailAppend(InMemoryConversations);
#[async_trait::async_trait]
impl ConversationStore for FailAppend {
    async fn get(&self, subject: &str, notebook: &str) -> Result<Conversation> {
        self.0.get(subject, notebook).await
    }
    async fn append(
        &self,
        _: &str,
        _: &str,
        _: i64,
        _: ChatMessage,
        _: ChatMessage,
    ) -> Result<Conversation> {
        Err(CoreError::Storage("injected unavailable database".into()))
    }
}
#[tokio::test]
async fn failed_storage_and_oversized_turns_never_report_a_saved_exchange() {
    let (app, seen, task) =
        setup_store(Arc::new(FailAppend(InMemoryConversations::default()))).await;
    let send = |prompt: String| json!({"notebook":"sales","helper":"go","prompt":prompt,"expectedRevision":"0"});
    assert_eq!(
        rpc(
            &app,
            "SendMessage",
            "alice",
            "editor",
            send("x".repeat(8193))
        )
        .await
        .0,
        400
    );
    assert!(seen.lock().unwrap().is_empty());
    assert_eq!(
        rpc(
            &app,
            "SendMessage",
            "alice",
            "editor",
            send("oversized-reply".into())
        )
        .await
        .0,
        400
    );
    assert_eq!(
        rpc(
            &app,
            "SendMessage",
            "alice",
            "editor",
            send("valid question".into())
        )
        .await
        .0,
        500
    );
    let (_, history) = rpc(
        &app,
        "GetConversation",
        "alice",
        "editor",
        json!({"notebook":"sales"}),
    )
    .await;
    assert!(history
        .get("messages")
        .is_none_or(|m| m.as_array().unwrap().is_empty()));
    task.abort();
}

struct OrdersCatalog {
    id: CatalogId,
}

#[async_trait::async_trait]
impl Catalog for OrdersCatalog {
    fn metadata_read_bytes(&self) -> Option<usize> {
        Some(0)
    }
    fn id(&self) -> &CatalogId {
        &self.id
    }
    fn kind(&self) -> &str {
        "stub"
    }
    async fn health(&self) -> Health {
        Health::Healthy
    }
    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        Ok(vec![Namespace {
            segments: Vec::new(),
            name: "sales".into(),
        }])
    }
    async fn list_tables(&self, _namespace: &str) -> Result<Vec<TableRef>> {
        Ok(vec![TableRef {
            namespace_segments: Vec::new(),
            namespace: "sales".into(),
            name: "orders".into(),
        }])
    }
    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        Ok(TableSchema {
            table: table.clone(),
            columns: vec![
                ColumnSchema {
                    name: "order_id".into(),
                    data_type: "bigint".into(),
                    nullable: false,
                },
                ColumnSchema {
                    name: "total".into(),
                    data_type: "decimal(12,2)".into(),
                    nullable: false,
                },
            ],
        })
    }
}

/// The reference material the question earns is retrieval-scoped: it names the
/// table, carries only the admitted catalog's live schema, and
/// stays out of an unrelated turn.
#[tokio::test]
async fn chat_grounding_carries_admitted_schema_without_unadmitted_contracts() {
    let mut catalogs = CatalogRegistry::new();
    catalogs.register(Arc::new(OrdersCatalog {
        id: CatalogId::new("polaris"),
    }));
    let contract = DataContract::parse(
        "orders.odcs.yaml",
        r#"name: orders
description: one row per order
owner: data-team
schema:
  - name: orders
    properties:
      - name: order_id
        logicalType: bigint
        required: true
      - name: total
        logicalType: decimal(12,2)
        semanticType: measure
"#,
    )
    .unwrap();
    let (app, seen, task) = setup_with(
        Arc::new(InMemoryConversations::default()),
        catalogs,
        vec![contract],
    )
    .await;
    let (status, turn) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({"notebook":"sales","helper":"go","prompt":"How is revenue from orders shaped?","expectedRevision":"0"}),
    )
    .await;
    assert_eq!(status, 200, "{turn}");
    {
        let seen = seen.lock().unwrap();
        let system = seen[0]["messages"][0]["content"].as_str().unwrap();
        assert!(system.contains("total decimal(12,2)"), "{system}");
        assert!(system.contains("catalog table sales.orders"), "{system}");
        assert!(!system.contains("one row per order"), "{system}");
        assert!(!system.contains("cubes:"), "{system}");
        assert!(!system.contains("measures:"), "{system}");
    }
    let (status, turn) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({"notebook":"sales","helper":"go","prompt":"hello there","expectedRevision":"1"}),
    )
    .await;
    assert_eq!(status, 200, "{turn}");
    {
        let seen = seen.lock().unwrap();
        let system = seen[1]["messages"][0]["content"].as_str().unwrap();
        assert!(!system.contains("catalog table"), "{system}");
        assert!(!system.contains("cubes:"), "{system}");
    }
    task.abort();
}

/// A cell conversation is stored under its own key: it neither reads nor
/// writes the notebook conversation or another cell's.
#[tokio::test]
async fn cell_conversations_are_scoped_to_their_cell() {
    let (app, seen, task) = setup().await;
    let (status, turn) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({"notebook":"sales","helper":"go","prompt":"notebook secret","expectedRevision":"0"}),
    )
    .await;
    assert_eq!(status, 200, "{turn}");
    let (status, cell) = rpc(
        &app,
        "GetConversation",
        "alice",
        "editor",
        json!({"notebook":"sales","cell":"q1"}),
    )
    .await;
    assert_eq!(status, 200, "{cell}");
    assert_eq!(cell["messages"].as_array().map_or(0, Vec::len), 0, "{cell}");
    let (status, turn) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({"notebook":"sales","cell":"q1","helper":"go","prompt":"which orders?","context":"SELECT orderkey FROM orders","expectedRevision":"0"}),
    )
    .await;
    assert_eq!(status, 200, "{turn}");
    assert_eq!(turn["revision"], "1", "{turn}");
    let (status, notebook) = rpc(
        &app,
        "GetConversation",
        "alice",
        "editor",
        json!({"notebook":"sales"}),
    )
    .await;
    assert_eq!(status, 200, "{notebook}");
    assert!(
        !serde_json::to_string(&notebook)
            .unwrap()
            .contains("which orders?"),
        "{notebook}"
    );
    let (status, other) = rpc(
        &app,
        "GetConversation",
        "alice",
        "editor",
        json!({"notebook":"sales","cell":"q2"}),
    )
    .await;
    assert_eq!(status, 200, "{other}");
    assert_eq!(
        other["messages"].as_array().map_or(0, Vec::len),
        0,
        "{other}"
    );
    {
        let seen = seen.lock().unwrap();
        let request = serde_json::to_string(&seen[1]).unwrap();
        assert!(request.contains("which orders?"), "{request}");
        assert!(!request.contains("notebook secret"), "{request}");
    }
    task.abort();
}

#[tokio::test]
async fn skill_versions_remain_pinned_until_explicit_upgrade() {
    let (app, seen, task) = setup().await;
    let (status, skill) = rpc(
        &app,
        "PutSkill",
        "alice",
        "editor",
        json!({
            "title":"SQL review", "instructions":"SKILL_VERSION_ONE", "expectedRevision":"0",
            "references":[{"name":"guide.txt","content":"REFERENCE_ONE"}]
        }),
    )
    .await;
    assert_eq!(status, 200, "{skill}");
    let pin = json!({"id":skill["id"],"revision":"1"});
    let (status, first) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({
            "notebook":"sales","helper":"go","prompt":"review","expectedRevision":"0","skill":pin
        }),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["messages"][0]["skills"][0]["revision"], "1");
    assert!(seen.lock().unwrap()[0]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("SKILL_VERSION_ONE"));
    let (status, updated) = rpc(&app, "PutSkill", "alice", "editor", json!({
        "id":skill["id"],"title":"SQL review","instructions":"SKILL_VERSION_TWO","expectedRevision":"1"
    })).await;
    assert_eq!(status, 200, "{updated}");
    let (status, inherited) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({
            "notebook":"sales","helper":"go","prompt":"again","expectedRevision":"1"
        }),
    )
    .await;
    assert_eq!(status, 200, "{inherited}");
    let system = seen.lock().unwrap()[1]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(system.contains("SKILL_VERSION_ONE"));
    assert!(system.contains("REFERENCE_ONE"));
    assert!(!system.contains("SKILL_VERSION_TWO"));
    let (status, upgraded) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({
            "notebook":"sales","helper":"go","prompt":"upgrade","expectedRevision":"2",
            "skill":{"id":skill["id"],"revision":"2"}
        }),
    )
    .await;
    assert_eq!(status, 200, "{upgraded}");
    assert!(seen.lock().unwrap()[2]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("SKILL_VERSION_TWO"));
    let (status, cleared) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({
            "notebook":"sales","helper":"go","prompt":"clear","expectedRevision":"3",
            "skill":{"id":"","revision":"0"}
        }),
    )
    .await;
    assert_eq!(status, 200, "{cleared}");
    let system = seen.lock().unwrap()[3]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!system.contains("SKILL_VERSION_"));
    let (status, unpinned) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({
            "notebook":"sales","helper":"go","prompt":"still clear","expectedRevision":"4"
        }),
    )
    .await;
    assert_eq!(status, 200, "{unpinned}");
    assert!(!seen.lock().unwrap()[4]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("SKILL_VERSION_"));
    task.abort();
}

#[tokio::test]
async fn revoked_skill_history_blocks_next_upstream_request() {
    let (app, seen, task) = setup().await;
    let (status, skill) = rpc(
        &app,
        "PutSkill",
        "admin",
        "admin",
        json!({
            "title":"Shared guidance","instructions":"ORG_SKILL","expectedRevision":"0"
        }),
    )
    .await;
    assert_eq!(status, 200, "{skill}");
    let (status, shared) = rpc(&app, "ShareSkill", "admin", "admin", json!({
        "id":skill["id"],"expectedSharingRevision":skill["sharingRevision"],"audience":"organization"
    })).await;
    assert_eq!(status, 200, "{shared}");
    let (status, first) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({
            "notebook":"sales","helper":"go","prompt":"review","expectedRevision":"0",
            "skill":{"id":skill["id"],"revision":"1"}
        }),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(seen.lock().unwrap().len(), 1);
    let (status, revoked) = rpc(&app, "ShareSkill", "admin", "admin", json!({
        "id":skill["id"],"expectedSharingRevision":shared["sharingRevision"],"audience":"private"
    })).await;
    assert_eq!(status, 200, "{revoked}");
    let (status, refused) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({
            "notebook":"sales","helper":"go","prompt":"again","expectedRevision":"1",
            "skill":{"id":"","revision":"0"}
        }),
    )
    .await;
    assert_eq!(status, 403, "{refused}");
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert!(refused.to_string().contains("new session"));
    task.abort();
}

#[tokio::test]
async fn deleted_skill_history_blocks_replay_but_preserves_messages() {
    let (app, seen, task) = setup().await;
    let (status, skill) = rpc(
        &app,
        "PutSkill",
        "admin",
        "admin",
        json!({
            "title":"Shared guidance","instructions":"ORG_SKILL","expectedRevision":"0"
        }),
    )
    .await;
    assert_eq!(status, 200, "{skill}");
    let (status, shared) = rpc(&app, "ShareSkill", "admin", "admin", json!({
        "id":skill["id"],"expectedSharingRevision":skill["sharingRevision"],"audience":"organization"
    })).await;
    assert_eq!(status, 200, "{shared}");
    let (status, first) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({
            "notebook":"sales","helper":"go","prompt":"review","expectedRevision":"0",
            "skill":{"id":skill["id"],"revision":"1"}
        }),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(seen.lock().unwrap().len(), 1);
    let (status, revoked) = rpc(&app, "DeleteSkill", "admin", "admin", json!({
        "id":skill["id"],"expectedRevision":"1","expectedSharingRevision":shared["sharingRevision"]
    })).await;
    assert_eq!(status, 200, "{revoked}");
    let (status, refused) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({
            "notebook":"sales","helper":"go","prompt":"again","expectedRevision":"1",
            "skill":{"id":"","revision":"0"}
        }),
    )
    .await;
    assert_eq!(status, 403, "{refused}");
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert!(refused.to_string().contains("new session"));
    let (status, retained) = rpc(
        &app,
        "GetConversation",
        "alice",
        "editor",
        json!({"notebook":"sales"}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(retained["messages"], first["messages"]);
    task.abort();
}

#[tokio::test]
async fn personal_defaults_and_multiple_skills_are_explicit_and_isolated() {
    let (app, seen, task) = setup().await;
    let mut pins = Vec::new();
    for text in ["DEFAULT_ALPHA", "DEFAULT_BETA"] {
        let (status, skill) = rpc(
            &app,
            "PutSkill",
            "alice",
            "editor",
            json!({"title":text,"instructions":text,"expectedRevision":"0"}),
        )
        .await;
        assert_eq!(status, 200);
        pins.push(json!({"id":skill["id"],"revision":"1"}));
    }
    let (status, defaults) = rpc(
        &app,
        "PutSkillDefaults",
        "alice",
        "editor",
        json!({"expectedRevision":"0","skills":pins}),
    )
    .await;
    assert_eq!(status, 200, "{defaults}");
    let (_, bob) = rpc(&app, "GetSkillDefaults", "bob", "editor", json!({})).await;
    assert!(bob["skills"].as_array().is_none_or(Vec::is_empty));
    let (status, _) = rpc(
        &app,
        "PutSkillDefaults",
        "alice",
        "editor",
        json!({"expectedRevision":"0","skills":[]}),
    )
    .await;
    assert_ne!(status, 200, "stale preferences must conflict");
    let (status, first) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({"notebook":"sales","helper":"go","prompt":"review","expectedRevision":"0"}),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["messages"][0]["skills"].as_array().unwrap().len(), 2);
    let system = seen.lock().unwrap()[0]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(system.contains("DEFAULT_ALPHA") && system.contains("DEFAULT_BETA"));
    let (status, _) = rpc(
        &app,
        "PutSkillDefaults",
        "alice",
        "editor",
        json!({"expectedRevision":defaults["revision"],"skills":[]}),
    )
    .await;
    assert_eq!(status, 200);
    let (status, inherited) = rpc(
        &app,
        "SendMessage",
        "alice",
        "editor",
        json!({"notebook":"sales","helper":"go","prompt":"keep pins","expectedRevision":"1"}),
    )
    .await;
    assert_eq!(status, 200, "{inherited}");
    assert_eq!(
        inherited["messages"][2]["skills"].as_array().unwrap().len(),
        2
    );
    let (status, cleared) = rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","helper":"go","prompt":"off","expectedRevision":"2","selectedSkills":{"skills":[]}})).await;
    assert_eq!(status, 200, "{cleared}");
    assert!(!seen.lock().unwrap()[2]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("DEFAULT_"));
    let calls = seen.lock().unwrap().len();
    let (status, _) = rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","helper":"go","prompt":"duplicate","expectedRevision":"3","selectedSkills":{"skills":[pins[0],pins[0]]}})).await;
    assert_eq!(status, 400);
    assert_eq!(seen.lock().unwrap().len(), calls);
    task.abort();
}

#[tokio::test]
async fn session_deletion_is_owned_revision_checked_and_does_not_resurrect() {
    let store = Arc::new(InMemoryConversations::default());
    let (app, seen, task) = setup_store(store.clone()).await;
    let (status, first) = rpc(
        &app,
        "NewConversation",
        "alice",
        "editor",
        json!({"notebook":"sales"}),
    )
    .await;
    assert_eq!(status, 200);
    let (_, kept) = rpc(
        &app,
        "NewConversation",
        "alice",
        "editor",
        json!({"notebook":"sales"}),
    )
    .await;
    let id = first["id"].as_str().unwrap();
    let (status, _) = rpc(&app, "SendMessage", "alice", "editor", json!({"notebook":"sales","session":id,"helper":"go","prompt":"private question","expectedRevision":"0"})).await;
    assert_eq!(status, 200);
    let deletion = json!({"notebook":"sales","session":id,"expectedRevision":"1"});
    assert_eq!(
        rpc(&app, "DeleteConversation", "", "", deletion.clone())
            .await
            .0,
        401
    );
    assert_eq!(
        rpc(&app, "DeleteConversation", "bob", "admin", deletion.clone())
            .await
            .0,
        403
    );
    for override_fields in [json!({"notebook":"other"}), json!({"cell":"c1"})] {
        let mut request = deletion.clone();
        request
            .as_object_mut()
            .unwrap()
            .extend(override_fields.as_object().unwrap().clone());
        assert_eq!(
            rpc(&app, "DeleteConversation", "alice", "editor", request)
                .await
                .0,
            404
        );
    }
    assert_eq!(
        rpc(
            &app,
            "DeleteConversation",
            "alice",
            "editor",
            json!({"notebook":"sales","session":id})
        )
        .await
        .0,
        400
    );
    assert_eq!(
        rpc(
            &app,
            "DeleteConversation",
            "alice",
            "editor",
            json!({"notebook":"sales","session":id,"expectedRevision":"0"})
        )
        .await
        .0,
        409
    );
    let mut current = store.get_session("alice", "sales", id).await.unwrap();
    current.pending=Some(serde_json::from_value(json!({"id":"turn","user":current.messages[0],"messages":[],"tools":[],"call":null,"phase":"executing","calls":0,"expires_at":0})).unwrap());
    let pending = store.save_turn("alice", "sales", 1, current).await.unwrap();
    assert_eq!(
        rpc(
            &app,
            "DeleteConversation",
            "alice",
            "editor",
            json!({"notebook":"sales","session":id,"expectedRevision":pending.revision.to_string()})
        )
        .await
        .0,
        409
    );
    let mut cleared = pending.clone();
    cleared.pending = None;
    let cleared = store
        .save_turn("alice", "sales", pending.revision, cleared)
        .await
        .unwrap();
    let request =
        json!({"notebook":"sales","session":id,"expectedRevision":cleared.revision.to_string()});
    let (a, b) = tokio::join!(
        rpc(
            &app,
            "DeleteConversation",
            "alice",
            "editor",
            request.clone()
        ),
        rpc(&app, "DeleteConversation", "alice", "editor", request)
    );
    assert_eq!(
        [a.0, b.0]
            .into_iter()
            .filter(|status| *status == 200)
            .count(),
        1
    );
    assert!([a.0, b.0].contains(&404));
    let (_, listed) = rpc(
        &app,
        "ListConversations",
        "alice",
        "editor",
        json!({"notebook":"sales"}),
    )
    .await;
    assert_eq!(listed["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(listed["sessions"][0]["id"], kept["id"]);
    assert_eq!(
        rpc(
            &app,
            "GetConversation",
            "alice",
            "editor",
            json!({"notebook":"sales","session":id})
        )
        .await
        .0,
        404
    );
    assert_eq!(rpc(&app,"SendMessage","alice","editor",json!({"notebook":"sales","session":id,"helper":"go","prompt":"stale tab","expectedRevision":cleared.revision.to_string()})).await.0,404);
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!(
        rpc(
            &app,
            "DeleteConversation",
            "alice",
            "editor",
            json!({"notebook":"sales","session":kept["id"],"expectedRevision":"0"})
        )
        .await
        .0,
        200
    );
    assert!(store.list("alice", "sales").await.unwrap().is_empty());
    let (_, cell) = rpc(
        &app,
        "NewConversation",
        "alice",
        "editor",
        json!({"notebook":"sales","cell":"c1"}),
    )
    .await;
    assert_eq!(
        rpc(
            &app,
            "DeleteConversation",
            "alice",
            "editor",
            json!({"notebook":"sales","cell":"c1","session":cell["id"],"expectedRevision":"0"})
        )
        .await
        .0,
        200
    );
    task.abort();
}
