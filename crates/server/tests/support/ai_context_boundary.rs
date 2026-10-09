use aster_core::*;
use aster_server::{app, AppState, Metrics, TeamPolicy, TeamWorkspaces};
use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::{
    rustls::{
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
        ServerConfig,
    },
    TlsAcceptor,
};
use tower::ServiceExt;

pub struct Capture {
    pub origin: String,
    pub http: reqwest::Client,
    pub seen: Arc<Mutex<Vec<Value>>>,
    #[allow(dead_code)] // S5 inspects session isolation; S1 inspects request bodies.
    pub sessions: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub fn command(dir: &Path, program: &str, args: &[&str]) {
    let output = std::process::Command::new(program)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{program}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub async fn capture() -> Capture {
    let dir = tempfile::tempdir().unwrap();
    command(
        dir.path(),
        "openssl",
        &[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=Aster test CA",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
            "-keyout",
            "ca-key.pem",
            "-out",
            "ca.pem",
        ],
    );
    command(
        dir.path(),
        "openssl",
        &[
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=localhost",
            "-keyout",
            "key.pem",
            "-out",
            "leaf.csr",
        ],
    );
    std::fs::write(dir.path().join("leaf.ext"), "basicConstraints=critical,CA:FALSE\nsubjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n").unwrap();
    command(
        dir.path(),
        "openssl",
        &[
            "x509",
            "-req",
            "-in",
            "leaf.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca-key.pem",
            "-CAcreateserial",
            "-days",
            "1",
            "-extfile",
            "leaf.ext",
            "-out",
            "cert.pem",
        ],
    );
    command(
        dir.path(),
        "openssl",
        &[
            "x509", "-in", "cert.pem", "-outform", "DER", "-out", "cert.der",
        ],
    );
    command(
        dir.path(),
        "openssl",
        &[
            "pkcs8", "-topk8", "-nocrypt", "-in", "key.pem", "-outform", "DER", "-out", "key.der",
        ],
    );
    let cert = std::fs::read(dir.path().join("cert.der")).unwrap();
    let http = reqwest::Client::builder()
        .add_root_certificate(
            reqwest::Certificate::from_pem(&std::fs::read(dir.path().join("ca.pem")).unwrap())
                .unwrap(),
        )
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let tls = ServerConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![CertificateDer::from(cert)],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            std::fs::read(dir.path().join("key.der")).unwrap(),
        )),
    )
    .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(tls));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("https://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let sessions = Arc::new(Mutex::new(Vec::new()));
    let captured_sessions = sessions.clone();
    let task = tokio::spawn(async move {
        let mut failed_contexts = HashSet::new();
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let mut stream = acceptor.accept(socket).await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            let (end, length) = loop {
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0 && bytes.len() + count < 131_072);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = std::str::from_utf8(&bytes[..end]).unwrap();
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(n, _)| n.eq_ignore_ascii_case("content-length"))
                                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break (end, length);
                    }
                }
            };
            let request: Value = serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
            let session = std::str::from_utf8(&bytes[..end])
                .unwrap()
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("x-opencode-session"))
                        .map(|(_, value)| value.trim().to_owned())
                })
                .unwrap_or_default();
            captured_sessions.lock().unwrap().push(session.clone());
            let serialized = request.to_string();
            let reference = request["messages"][0]["content"]
                .as_str()
                .and_then(|system| {
                    system
                        .lines()
                        .find_map(|line| serde_json::from_str::<Value>(line).ok())
                });
            let mut reply = if serialized.contains("OVERSIZED_REPLY") {
                if serialized.contains("FAILED_CONTEXT_CACHE") {
                    failed_contexts.insert(session.clone());
                }
                "x".repeat(32769)
            } else if serialized.contains("RESUME_FAILED_CONTEXT_CACHE")
                && failed_contexts.contains(&session)
            {
                "Cached NET_AFTER_REFUNDS from failed turn".into()
            } else if let Some(reference) =
                reference.filter(|value| value.get("declared").is_some())
            {
                let meaning = reference["declared"]["properties"][0]["description"]
                    .as_str()
                    .unwrap_or("no field meaning");
                if reference["binding"]["status"] == "resolved" {
                    let quote = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
                    let mut table = vec![quote(reference["binding"]["catalog"].as_str().unwrap())];
                    table.extend(
                        reference["binding"]["namespaceSegments"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|value| quote(value.as_str().unwrap())),
                    );
                    table.push(quote(
                        reference["binding"]["physicalName"].as_str().unwrap(),
                    ));
                    format!(
                        "Selected {meaning}; reviewable draft:\n```sql\nSELECT {} FROM {}\n```",
                        quote(
                            reference["declared"]["properties"][0]["physicalName"]
                                .as_str()
                                .unwrap()
                        ),
                        table.join(".")
                    )
                } else {
                    format!("Selected {meaning}; missing or ambiguous explicit table binding, no executable SQL.")
                }
            } else if serialized.contains("NET_AFTER_REFUNDS") {
                "Selected NET_AFTER_REFUNDS: gross - refunds; missing explicit table binding, no executable SQL.".into()
            } else if serialized.contains("GROSS_BEFORE_REFUNDS") {
                "Selected GROSS_BEFORE_REFUNDS: gross; missing explicit table binding, no executable SQL.".into()
            } else {
                "SELECT 42".into()
            };
            if request["messages"][0]["content"]
                .as_str()
                .is_some_and(|system| system.contains("OBSERVED_ONLY_SENTINEL"))
            {
                reply.push_str("\nObserved OBSERVED_ONLY_SENTINEL");
            }
            captured.lock().unwrap().push(request);
            let body = json!({"choices":[{"message":{"content":reply}}]}).to_string();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        }
    });
    http.post(format!("{origin}/v1/chat/completions"))
        .json(&json!({"messages":[]}))
        .send()
        .await
        .expect("HTTPS fixture must be reachable with certificate verification")
        .error_for_status()
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    seen.lock().unwrap().clear();
    sessions.lock().unwrap().clear();
    Capture {
        origin,
        http,
        seen,
        sessions,
        task,
        _dir: dir,
    }
}

struct Notebooks;
#[async_trait::async_trait]
impl NotebookStore for Notebooks {
    async fn get(&self, id: &str) -> Result<Notebook> {
        Ok(notebook(id, "GLOBAL_FORBIDDEN"))
    }
    async fn list(&self, _: &str) -> Result<Vec<String>> {
        Ok(vec!["base".into()])
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
fn notebook(id: &str, sentinel: &str) -> Notebook {
    Notebook {
        id: id.into(),
        title: id.into(),
        cells: vec![Cell {
            id: "c1".into(),
            sql: format!("SELECT '{sentinel}'"),
            engine: None,
            metadata: Default::default(),
        }],
    }
}

struct CatalogProbe {
    id: CatalogId,
    calls: Arc<AtomicUsize>,
    sentinel: &'static str,
}
#[async_trait::async_trait]
impl Catalog for CatalogProbe {
    fn metadata_read_bytes(&self) -> Option<usize> {
        Some(0)
    }
    fn id(&self) -> &CatalogId {
        &self.id
    }
    fn kind(&self) -> &str {
        "fixture"
    }
    async fn health(&self) -> Health {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Health::Healthy
    }
    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![Namespace {
            segments: Vec::new(),
            name: "sales".into(),
        }])
    }
    async fn list_tables(&self, _: &str) -> Result<Vec<TableRef>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![TableRef {
            namespace_segments: Vec::new(),
            namespace: "sales".into(),
            name: self.id.0.clone(),
        }])
    }
    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(TableSchema {
            table: table.clone(),
            columns: vec![ColumnSchema {
                name: self.sentinel.into(),
                data_type: "bigint".into(),
                nullable: false,
            }],
        })
    }
}

pub async fn state(capture: &Capture) -> AppState {
    let llm = Arc::new(InMemoryLlm::new());
    llm.put(LlmConfig {
        subject: "alice".into(),
        id: "go".into(),
        base_url: capture.origin.clone(),
        model: "fixture".into(),
        api_key: "fixture-only".into(),
    })
    .await
    .unwrap();
    let owners = Arc::new(InMemoryNotebookOwners::default());
    owners
        .change(NotebookOwnerChange {
            source: "unconfigured".into(),
            id: "base".into(),
            expected_owner: None,
            owner: "alice".into(),
            source_blob: "fixture".into(),
            actor: "alice".into(),
            reason: None,
        })
        .await
        .unwrap();
    AppState {
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
        notebooks: Arc::new(Notebooks),
        notebook_owners: owners,
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: None,
        team_git_targets: None,
        llm,
        shared_models: None,
        current_identity: None,
        shared_model_use_enabled: false,
        conversations: Arc::new(InMemoryConversations::default()),
        exchanges: Arc::new(InMemoryExchanges::default()),
        compiled_contracts: None,
        contracts: Arc::new(vec![]),
        http: capture.http.clone(),
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

fn metadata(
    state: &mut AppState,
    protected: bool,
    mixed: bool,
) -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let allowed = Arc::new(AtomicUsize::new(0));
    let denied = Arc::new(AtomicUsize::new(0));
    state.catalogs.register(Arc::new(CatalogProbe {
        id: CatalogId::new("orders"),
        calls: allowed.clone(),
        sentinel: "ALLOWED_SCHEMA",
    }));
    if mixed {
        state.catalogs.register(Arc::new(CatalogProbe {
            id: CatalogId::new("hidden"),
            calls: denied.clone(),
            sentinel: "UNBOUND_SCHEMA_FORBIDDEN",
        }));
    }
    state.config.catalog_bindings.push(CatalogBindingConfig {
        catalog: "orders".into(),
        engine: "fixture".into(),
        native_catalog: "orders".into(),
        policy: if protected {
            BindingPolicy::Protected
        } else {
            BindingPolicy::Unprotected
        },
    });
    state.contracts = Arc::new(
        ["orders", "hidden"]
            .into_iter()
            .map(|name| {
                DataContract::parse(
                    name,
                    &format!("name: {name}\ndescription: CONTRACT_FORBIDDEN\nschema: []\n"),
                )
                .unwrap()
            })
            .collect(),
    );
    (allowed, denied)
}

pub async fn request(app: &Router, path: &str, cookie: &str, body: Value) -> (u16, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .header("connect-protocol-version", "1")
                .header("cookie", cookie)
                .header("x-opencode-session", "client-selected-session")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status().as_u16(),
        String::from_utf8(
            to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap(),
    )
}
fn turn() -> Value {
    json!({"notebook":"base", "helper":"go", "prompt":"Explain orders and hidden", "expectedRevision":"0"})
}
const SEND: &str = "/aster.v1.Aster/SendMessage";
const COOKIE: &str = "aster_subject=alice; aster_roles=editor";

pub async fn denied_metadata() {
    let cap = capture().await;
    let mut positive = state(&cap).await;
    metadata(&mut positive, false, false);
    let (status, body) = request(&app(Arc::new(positive)), SEND, COOKIE, turn()).await;
    assert_eq!(status, 200, "positive helper control: {body}");
    assert!(serde_json::to_string(&*cap.seen.lock().unwrap())
        .unwrap()
        .contains("ALLOWED_SCHEMA"));
    cap.seen.lock().unwrap().clear();
    let mut denied = state(&cap).await;
    let (calls, _) = metadata(&mut denied, true, false);
    let app = app(Arc::new(denied));
    for (path, body) in [
        (SEND, turn()),
        (SEND, {
            let mut t = turn();
            t["cell"] = json!("c1");
            t
        }),
        (
            "/api/ai",
            json!({"helper":"go", "prompt":"Explain orders", "sql":"SELECT * FROM orders"}),
        ),
    ] {
        let (status, body) = request(&app, path, COOKIE, body).await;
        assert_eq!(
            status, 403,
            "protected metadata must refuse before retrieval: {body}"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0, "denied catalog reads");
    assert!(
        cap.seen.lock().unwrap().is_empty(),
        "denied turn reached helper"
    );
}

pub async fn mixed_metadata() {
    let cap = capture().await;
    let mut state = state(&cap).await;
    let (allowed, denied) = metadata(&mut state, false, true);
    let app = app(Arc::new(state));
    for (path, body) in [
        (SEND, turn()),
        (SEND, {
            let mut t = turn();
            t["cell"] = json!("c1");
            t
        }),
        (
            "/api/ai",
            json!({"helper":"go", "prompt":"Explain orders and hidden", "sql":"SELECT * FROM orders, hidden"}),
        ),
    ] {
        let (status, body) = request(&app, path, COOKIE, body).await;
        assert_eq!(status, 200, "mixed allowed control: {body}");
    }
    assert_eq!(
        denied.load(Ordering::SeqCst),
        0,
        "unbound catalog must never be read"
    );
    assert!(allowed.load(Ordering::SeqCst) > 0);
    let seen = cap.seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    for body in seen.iter() {
        let text = body.to_string();
        assert!(
            !text.contains("CONTRACT_FORBIDDEN"),
            "unadmitted contract disclosed: {text}"
        );
        assert!(
            !text.contains("UNBOUND_SCHEMA_FORBIDDEN"),
            "unbound schema disclosed: {text}"
        );
    }
    assert!(seen[0].to_string().contains("ALLOWED_SCHEMA"));
}

pub async fn workspace_index() {
    let cap = capture().await;
    let mut state = state(&cap).await;
    let root = tempfile::tempdir().unwrap();
    let mut policies = HashMap::new();
    for team in ["alpha", "beta"] {
        let repo = root.path().join(team);
        std::fs::create_dir(&repo).unwrap();
        command(&repo, "git", &["init", "-q", "-b", "main"]);
        std::fs::write(
            repo.join("base.aster"),
            "# aster notebook v2\n# title: Base\n-- cell c1\nSELECT 1\n",
        )
        .unwrap();
        command(&repo, "git", &["add", "base.aster"]);
        command(
            &repo,
            "git",
            &[
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
        );
        policies.insert(
            team.into(),
            TeamPolicy {
                member_claim: format!("/teams/{team}/members"),
                maintainer_claim: format!("/teams/{team}/maintainers"),
                allowed_repositories: HashSet::from([format!("example/{team}")]),
                local_repositories: HashMap::from([(format!("example/{team}"), repo)]),
            },
        );
    }
    let workspaces =
        Arc::new(TeamWorkspaces::new(root.path().join("workspaces"), policies).unwrap());
    for team in ["alpha", "beta"] {
        workspaces
            .configure(team, &format!("example/{team}"), "main", None, "fixture")
            .unwrap();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let groups = vec!["/teams/alpha/members".into(), "/teams/beta/members".into()];
    let first = state
        .sessions
        .create("alice", vec![Role::Editor], groups.clone(), None, now)
        .await
        .unwrap();
    let second = state
        .sessions
        .create("alice", vec![Role::Editor], groups.clone(), None, now)
        .await
        .unwrap();
    let principal = Principal {
        subject: "alice".into(),
        roles: vec![Role::Editor],
        groups,
        user_uuid: None,
    };
    let cases = [
        ("alpha", &first.sid, false, "ALPHA_FIRST"),
        ("alpha", &second.sid, false, "ALPHA_SECOND"),
        ("beta", &first.sid, false, "BETA_FIRST"),
        ("alpha", &first.sid, true, "ALPHA_PERSONAL"),
    ];
    for (team, sid, personal, sentinel) in cases {
        workspaces
            .workspace(team, &principal, sid, personal)
            .unwrap()
            .save(&notebook("base", sentinel), "fixture")
            .await
            .unwrap();
    }
    state.team_workspaces = Some(workspaces);
    state.dev_login = false;
    let app = app(Arc::new(state));
    for (team, sid, personal, sentinel) in cases {
        let mut body = turn();
        body["team"] = json!(team);
        body["workspace"] = json!(if personal { "personal" } else { "session" });
        body["prompt"] = json!("Explain the first cell");
        let (status, response) = request(&app, SEND, &format!("aster_session={sid}"), body).await;
        assert_eq!(status, 200, "{response}");
        let seen = cap.seen.lock().unwrap();
        let text = seen.last().unwrap().to_string();
        assert!(
            text.contains(sentinel),
            "admitted workspace index missing {sentinel}: {text}"
        );
        assert!(
            !text.contains("GLOBAL_FORBIDDEN"),
            "global notebook disclosed"
        );
        for (_, _, _, other) in cases {
            if other != sentinel {
                assert!(!text.contains(other), "other workspace disclosed");
            }
        }
    }
    let before = cap.seen.lock().unwrap().len();
    let mut body = turn();
    body["team"] = json!("alpha");
    body["cell"] = json!("c1");
    assert_eq!(
        request(&app, SEND, &format!("aster_session={}", first.sid), body)
            .await
            .0,
        400
    );
    assert_eq!(cap.seen.lock().unwrap().len(), before);
}
