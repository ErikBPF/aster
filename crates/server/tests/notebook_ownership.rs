use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aster_core::*;
use aster_server::{app, AppState, GitNotebookStore, Metrics};
use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use axum::Router;
use serde_json::json;
use tower::ServiceExt;

struct Fixture {
    app: Router,
    dir: PathBuf,
    _store: GitNotebookStore,
}

fn notebook(id: &str, sql: &str) -> Notebook {
    Notebook {
        context: None,
        id: id.into(),
        title: "Sales".into(),
        engine: None,
        cells: vec![Cell {
            id: "c1".into(),
            sql: sql.into(),
            engine: None,
            metadata: Default::default(),
        }],
    }
}

async fn fixture() -> Fixture {
    fixture_with_teams(false).await
}

async fn fixture_with_teams(teams: bool) -> Fixture {
    fixture_configured(teams, |_| {}).await
}

async fn fixture_configured(teams: bool, configure: impl FnOnce(&mut AppState)) -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "aster-ownership-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let store = GitNotebookStore::open(&dir, "session/alice").unwrap();
    let mut state = AppState {
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
        notebooks: Arc::new(store.clone()),
        notebook_owners: Arc::new(InMemoryNotebookOwners::default()),
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: teams.then(|| {
            Arc::new(
                aster_server::TeamWorkspaces::new(
                    dir.join("teams"),
                    std::collections::HashMap::new(),
                )
                .unwrap(),
            )
        }),
        team_git_targets: None,
        llm: Arc::new(InMemoryLlm::new()),
        shared_models: None,
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
    };
    configure(&mut state);
    Fixture {
        app: app(Arc::new(state)),
        dir,
        _store: store,
    }
}

tokio::task_local! {
    static EXCHANGE_READ: ();
}

/// Pause only an exchange request's second read until a real administrator
/// transfer and the new owner's save have completed. Other requests use Git
/// normally, including the administrator's locked snapshot read.
struct TransferReadStore {
    inner: Arc<dyn NotebookStore>,
    reads: AtomicU64,
    admitted: Arc<tokio::sync::Notify>,
    transferred: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl NotebookStore for TransferReadStore {
    async fn snapshot(&self, id: &str) -> aster_core::Result<NotebookSnapshot> {
        let read = EXCHANGE_READ
            .try_with(|_| self.reads.fetch_add(1, Ordering::SeqCst))
            .ok();
        if read.is_some_and(|read| read > 0) {
            self.transferred.notified().await;
        }
        let snapshot = self.inner.snapshot(id).await?;
        if read == Some(0) {
            self.admitted.notify_one();
        }
        Ok(snapshot)
    }
    async fn get(&self, id: &str) -> aster_core::Result<Notebook> {
        Ok(self.snapshot(id).await?.notebook)
    }
    async fn list(&self, actor: &str) -> aster_core::Result<Vec<String>> {
        self.inner.list(actor).await
    }
    async fn save(&self, notebook: &Notebook, actor: &str) -> aster_core::Result<String> {
        self.inner.save(notebook, actor).await
    }
    async fn save_if(
        &self,
        notebook: &Notebook,
        actor: &str,
        expected: NotebookPrecondition,
    ) -> aster_core::Result<NotebookSave> {
        self.inner.save_if(notebook, actor, expected).await
    }
    fn source_key(&self) -> String {
        self.inner.source_key()
    }
}

async fn transfer_between_exchange_reads(method: &str) {
    let admitted = Arc::new(tokio::sync::Notify::new());
    let transferred = Arc::new(tokio::sync::Notify::new());
    let fixture = fixture_configured(false, |state| {
        state.notebooks = Arc::new(TransferReadStore {
            inner: state.notebooks.clone(),
            reads: AtomicU64::new(0),
            admitted: admitted.clone(),
            transferred: transferred.clone(),
        });
    })
    .await;
    let created = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("sales", "SELECT 'alice-original'")),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(created.into_body(), 1 << 20)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let oid = created["content_revision"].as_str().unwrap().to_string();
    let original_oid = oid.clone();
    let writer_app = fixture.app.clone();
    let writer = tokio::spawn(async move {
        admitted.notified().await;
        let assigned = call(&writer_app, "PUT", "/api/admin/notebooks/sales/owner", "admin", "admin", None,
            json!({"owner":"bob", "expected_owner":"alice", "expected_content_revision":oid, "reason":"transfer race test"})).await;
        assert_eq!(assigned.status(), StatusCode::OK);
        let saved = call(
            &writer_app,
            "PUT",
            "/api/notebooks/sales",
            "bob",
            "editor",
            Some(("if-match", format!("\"{oid}\""))),
            json!(notebook("sales", "SELECT 'bob-private'")),
        )
        .await;
        assert_eq!(saved.status(), StatusCode::OK);
        transferred.notify_one();
    });
    let response = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let response = EXCHANGE_READ
            .scope(
                (),
                call(
                    &fixture.app,
                    "POST",
                    &format!("/aster.v1.Aster/{method}"),
                    "alice",
                    "editor",
                    None,
                    json!({"notebook":"sales","cell":"c1"}),
                ),
            )
            .await;
        writer.await.unwrap();
        response
    })
    .await
    .expect("ownership transfer and exchange read must complete");
    assert_eq!(response.status(), StatusCode::OK);
    // Prove Bob really saved, even when the fixed handler never performs a
    // second read and therefore finishes before the concurrent writer.
    assert_eq!(
        fixture._store.get("sales").await.unwrap().cells[0].sql,
        "SELECT 'bob-private'"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sql = if method == "FetchQuery" {
        &response["sql"]
    } else {
        &response["cells"][0]["sql"]
    };
    assert_eq!(
        sql, "SELECT 'alice-original'",
        "never disclose SQL saved after ownership transfer"
    );
    if method == "FetchQuery" {
        assert_eq!(response["contentRevision"], original_oid);
    }
}

#[tokio::test]
async fn benchmark_fetch_query_keeps_snapshot_admitted_before_transfer() {
    transfer_between_exchange_reads("FetchQuery").await;
}

#[tokio::test]
async fn benchmark_fetch_summary_keeps_snapshot_admitted_before_transfer() {
    transfer_between_exchange_reads("FetchSummary").await;
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[tokio::test]
async fn benchmark_local_notebooks_are_owner_isolated_across_surfaces() {
    // A unique notebook identifier distinguishes disclosure from generic UI privacy labels.
    let fixture = fixture().await;
    let created = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/owner-secret-7f2d",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("owner-secret-7f2d", "SELECT 17")),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    for path in [
        "/api/notebooks/owner-secret-7f2d",
        "/notebooks/owner-secret-7f2d",
        "/api/notebooks/owner-secret-7f2d/helper",
    ] {
        let response = call(&fixture.app, "GET", path, "bob", "editor", None, json!({})).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
    }
    for (method, body) in [
        ("GetNotebook", json!({"id":"owner-secret-7f2d"})),
        ("GetNotebookHelper", json!({"notebook":"owner-secret-7f2d"})),
        ("GetConversation", json!({"notebook":"owner-secret-7f2d"})),
        (
            "RunQuery",
            json!({"notebook":"owner-secret-7f2d","cell":"c1","sql":"SELECT 1"}),
        ),
        (
            "FetchQuery",
            json!({"notebook":"owner-secret-7f2d","cell":"c1"}),
        ),
        (
            "FetchResult",
            json!({"notebook":"owner-secret-7f2d","cell":"c1"}),
        ),
        ("FetchSummary", json!({"notebook":"owner-secret-7f2d"})),
    ] {
        let response = call(
            &fixture.app,
            "POST",
            &format!("/aster.v1.Aster/{method}"),
            "bob",
            "editor",
            None,
            body,
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method}");
    }
    for (verb, path, body) in [
        ("GET", "/api/notebooks", json!({})),
        ("POST", "/aster.v1.Aster/ListNotebooks", json!({})),
        ("GET", "/", json!({})),
    ] {
        let response = call(&fixture.app, verb, path, "bob", "editor", None, body).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&bytes).contains("owner-secret-7f2d"),
            "{path}"
        );
    }
}

#[tokio::test]
async fn benchmark_rpc_edits_preserve_rest_cell_metadata() {
    let fixture = fixture().await;
    let mut document = json!(notebook("benchmark", "SELECT 1"));
    document["cells"][0]["metadata"] =
        json!({"catalog_context":"tpch","schema":"tiny","other":false});
    let created = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/benchmark",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        document.clone(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(created.into_body(), 1 << 20)
        .await
        .unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let edited = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/SaveNotebook",
        "alice",
        "editor",
        None,
        json!({"id":"benchmark", "expectedContentRevision":saved["content_revision"],
        "notebook":{"title":"Edited", "cells":[{"id":"c1", "sql":"SELECT 2"}]}}),
    )
    .await;
    assert_eq!(edited.status(), StatusCode::OK);
    let reloaded = call(
        &fixture.app,
        "GET",
        "/api/notebooks/benchmark",
        "alice",
        "editor",
        None,
        json!({}),
    )
    .await;
    let bytes = axum::body::to_bytes(reloaded.into_body(), 1 << 20)
        .await
        .unwrap();
    let reloaded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        reloaded["cells"][0]["metadata"],
        document["cells"][0]["metadata"]
    );
    assert_eq!(reloaded["cells"][0]["sql"], "SELECT 2");
}

#[tokio::test]
async fn benchmark_explicit_local_mode_preserves_team_policy() {
    if std::env::var_os("ASTER_LOCAL_NOTEBOOK_CHILD").is_none() {
        for mode in ["local", "default"] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "benchmark_explicit_local_mode_preserves_team_policy",
                    "--nocapture",
                ])
                .env("ASTER_LOCAL_NOTEBOOK_CHILD", mode)
                .env_remove("ASTER_NOTEBOOK_MODE");
            if mode == "local" {
                command.env("ASTER_NOTEBOOK_MODE", "local");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{mode}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    }
    let fixture = fixture_with_teams(true).await;
    let expected = if std::env::var("ASTER_LOCAL_NOTEBOOK_CHILD").unwrap() == "local" {
        StatusCode::OK
    } else {
        StatusCode::FORBIDDEN
    };
    for (method, path, body, precondition) in [
        ("GET", "/", json!({}), None),
        (
            "PUT",
            "/api/notebooks/local",
            json!(notebook("local", "SELECT 1")),
            Some(("if-none-match", "*".into())),
        ),
        ("POST", "/aster.v1.Aster/ListNotebooks", json!({}), None),
    ] {
        let response = call(
            &fixture.app,
            method,
            path,
            "alice",
            "editor",
            precondition,
            body,
        )
        .await;
        assert_eq!(response.status(), expected, "{path}");
    }
}

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    subject: &str,
    role: &str,
    precondition: Option<(&str, String)>,
    body: serde_json::Value,
) -> Response<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("x-aster-subject", subject)
        .header("x-aster-roles", role)
        .header("content-type", "application/json");
    if path.starts_with("/aster.v1.Aster/") {
        request = request.header("connect-protocol-version", "1");
    }
    if let Some((name, value)) = precondition {
        request = request.header(name, value);
    }
    app.clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn legacy_unassigned_save_is_refused_before_git_mutation() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let before = git(&fixture.dir, &["rev-parse", "HEAD"]);
    let blob = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);

    let response = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        Some(("if-match", format!("\"{blob}\""))),
        json!(notebook("sales", "SELECT 2")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(git(&fixture.dir, &["rev-parse", "HEAD"]), before);
    assert_eq!(git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]), blob);
}

#[tokio::test]
async fn create_requires_an_explicit_absence_precondition() {
    let fixture = fixture().await;
    let response = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        None,
        json!(notebook("sales", "SELECT 1")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(fixture._store.list("alice").await.unwrap().is_empty());
}

#[tokio::test]
async fn legacy_get_exposes_the_committed_blob_oid() {
    let fixture = fixture().await;
    let created = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("sales", "SELECT 1")),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let blob = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let response = call(
        &fixture.app,
        "GET",
        "/api/notebooks/sales",
        "alice",
        "editor",
        None,
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("etag")
            .map(|value| value.to_str().unwrap()),
        Some(format!("\"{blob}\"").as_str())
    );

    let page = call(
        &fixture.app,
        "GET",
        "/notebooks/sales",
        "alice",
        "editor",
        None,
        json!({}),
    )
    .await;
    let body = axum::body::to_bytes(page.into_body(), 1 << 20)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains(&blob));
}

#[tokio::test]
async fn rpc_create_without_precondition_is_refused() {
    let fixture = fixture().await;
    let response = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/SaveNotebook",
        "alice",
        "editor",
        None,
        json!({"id":"sales","notebook":notebook("sales", "SELECT 1")}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(fixture._store.list("alice").await.unwrap().is_empty());
}

#[tokio::test]
async fn rpc_create_get_and_stale_update_use_content_blob_revision() {
    let fixture = fixture().await;
    let create = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/SaveNotebook",
        "alice",
        "editor",
        None,
        json!({"id":"sales", "ifAbsent":true, "notebook":notebook("sales", "SELECT 1")}),
    )
    .await;
    assert_eq!(create.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(create.into_body(), 1 << 20)
        .await
        .unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let oid = saved["contentRevision"].as_str().unwrap();
    assert_eq!(oid, git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]));
    assert_eq!(
        saved["revision"].as_str().unwrap(),
        git(&fixture.dir, &["rev-parse", "HEAD"])
    );
    let read = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/GetNotebook",
        "alice",
        "editor",
        None,
        json!({"id":"sales"}),
    )
    .await;
    let bytes = axum::body::to_bytes(read.into_body(), 1 << 20)
        .await
        .unwrap();
    let loaded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(loaded["contentRevision"], oid);
    let first = call(&fixture.app, "POST", "/aster.v1.Aster/SaveNotebook", "alice", "editor", None,
        json!({"id":"sales", "expectedContentRevision":oid, "notebook":notebook("sales", "SELECT 2")})).await;
    assert_eq!(first.status(), StatusCode::OK);
    let stale = call(&fixture.app, "POST", "/aster.v1.Aster/SaveNotebook", "alice", "editor", None,
        json!({"id":"sales", "expectedContentRevision":oid, "notebook":notebook("sales", "SELECT 3")})).await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn rpc_assignment_requires_admin_and_valid_blob_precondition() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let oid = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let path = "/aster.v1.Aster/AssignNotebookOwner";
    let body = json!({"id":"sales", "owner":"alice", "expectedContentRevision":oid});
    let denied = call(
        &fixture.app,
        "POST",
        path,
        "bob",
        "editor",
        None,
        body.clone(),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let granted = call(&fixture.app, "POST", path, "root", "admin", None, body).await;
    assert_eq!(granted.status(), StatusCode::OK);
    let invalid = call(&fixture.app, "POST", "/aster.v1.Aster/SaveNotebook", "alice", "editor", None,
        json!({"id":"sales", "expectedContentRevision":"bad", "notebook":notebook("sales", "SELECT 2")})).await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_assignment_requires_a_committed_blob_and_admin_role() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let blob = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let body = json!({"owner":"alice", "expected_content_revision":blob});
    let path = "/api/admin/notebooks/sales/owner";
    let ordinary = call(
        &fixture.app,
        "PUT",
        path,
        "bob",
        "editor",
        None,
        body.clone(),
    )
    .await;
    assert_eq!(ordinary.status(), StatusCode::FORBIDDEN);
    let admin = call(&fixture.app, "PUT", path, "root", "admin", None, body).await;
    assert_eq!(admin.status(), StatusCode::OK);
}

#[tokio::test]
async fn stale_tab_conflicts_but_an_unrelated_commit_does_not() {
    let fixture = fixture().await;
    let first = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("sales", "SELECT 1")),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let old = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let unrelated = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/other",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("other", "SELECT 9")),
    )
    .await;
    assert_eq!(unrelated.status(), StatusCode::OK);
    let update = |oid: &str, sql: &str| {
        call(
            &fixture.app,
            "PUT",
            "/api/notebooks/sales",
            "alice",
            "editor",
            Some(("if-match", format!("\"{oid}\""))),
            json!(notebook("sales", sql)),
        )
    };
    let first = update(&old, "SELECT 2").await;
    assert_eq!(first.status(), StatusCode::OK);
    let current = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let before = git(&fixture.dir, &["rev-parse", "HEAD"]);
    let stale = update(&old, "SELECT 3").await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(git(&fixture.dir, &["rev-parse", "HEAD"]), before);
    assert_eq!(
        git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]),
        current
    );
}

#[tokio::test]
async fn correction_requires_exact_owner_blob_and_reason_then_revokes_old_owner() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let blob = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let path = "/api/admin/notebooks/sales/owner";
    let assign = |body| call(&fixture.app, "PUT", path, "root", "admin", None, body);
    let stale = assign(json!({"owner":"alice", "expected_content_revision":"0".repeat(40)})).await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(
        assign(json!({"owner":"alice", "expected_content_revision":blob}))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        assign(json!({"owner":"bob", "expected_content_revision":blob}))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        assign(json!({"owner":"bob", "expected_owner":"alice", "expected_content_revision":blob}))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        assign(json!({"owner":"bob", "expected_owner":"mallory", "expected_content_revision":blob, "reason":"correct typo"}))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(assign(json!({"owner":"bob", "expected_owner":"alice", "expected_content_revision":blob, "reason":"correct typo"})).await.status(), StatusCode::OK);
    let alice = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        Some(("if-match", format!("\"{blob}\""))),
        json!(notebook("sales", "SELECT 2")),
    )
    .await;
    assert_eq!(alice.status(), StatusCode::FORBIDDEN);
    let bob = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "bob",
        "editor",
        Some(("if-match", format!("\"{blob}\""))),
        json!(notebook("sales", "SELECT 2")),
    )
    .await;
    assert_eq!(bob.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_owner_inputs_are_bounded_and_exact() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let blob = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    for body in [
        json!({"owner":"a".repeat(257), "expected_content_revision":blob}),
        json!({"owner":" alice", "expected_content_revision":blob}),
        json!({"owner":"bob", "expected_owner":" alice", "expected_content_revision":blob, "reason":"fix"}),
        json!({"owner":"bob", "expected_owner":"alice", "expected_content_revision":blob, "reason":"x".repeat(1025)}),
    ] {
        let response = call(
            &fixture.app,
            "PUT",
            "/api/admin/notebooks/sales/owner",
            "root",
            "admin",
            None,
            body,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn concurrent_first_creates_claim_one_actor() {
    let fixture = fixture().await;
    let a = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("sales", "SELECT 1")),
    );
    let b = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "bob",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("sales", "SELECT 2")),
    );
    let (a, b) = tokio::join!(a, b);
    let statuses = [a.status(), b.status()];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::CONFLICT));
    assert_eq!(fixture._store.list("alice").await.unwrap(), vec!["sales"]);
}

#[tokio::test]
async fn moved_checkout_keeps_stable_owner_source_key() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "aster-source-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let old = dir.with_extension("old");
    let store = GitNotebookStore::open(&old, "session/alice").unwrap();
    let source = store.source_key();
    drop(store);
    std::fs::rename(&old, &dir).unwrap();
    let moved = GitNotebookStore::open(&dir, "session/alice").unwrap();
    assert_eq!(moved.source_key(), source);
}

#[tokio::test]
async fn corrupt_head_and_missing_blob_are_storage_errors() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let oid = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let object = fixture
        .dir
        .join(".git/objects")
        .join(&oid[..2])
        .join(&oid[2..]);
    std::fs::remove_file(object).unwrap();
    assert!(matches!(
        fixture._store.get("sales").await,
        Err(CoreError::Storage(_))
    ));
    assert!(matches!(
        fixture
            ._store
            .save_if(
                &notebook("sales", "SELECT 2"),
                "seed",
                NotebookPrecondition::Blob(oid)
            )
            .await,
        Err(CoreError::Storage(_))
    ));
    let head = fixture.dir.join(".git/refs/heads/session/alice");
    std::fs::write(head, "broken\n").unwrap();
    assert!(matches!(
        fixture._store.list("alice").await,
        Err(CoreError::Storage(_))
    ));
    assert!(matches!(
        fixture
            ._store
            .save_if(
                &notebook("new", "SELECT 1"),
                "seed",
                NotebookPrecondition::Absent
            )
            .await,
        Err(CoreError::Storage(_))
    ));
}

struct ContextCaptureEngine {
    info: EngineInfo,
    calls: Arc<std::sync::Mutex<Vec<QueryRequest>>>,
}
#[async_trait::async_trait]
impl QueryEngine for ContextCaptureEngine {
    fn info(&self) -> &EngineInfo {
        &self.info
    }
    async fn health(&self) -> Health {
        Health::Healthy
    }
    async fn execute(&self, request: QueryRequest) -> aster_core::Result<QueryResult> {
        self.calls.lock().unwrap().push(request);
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            truncated: false,
        })
    }
}

async fn response_json(response: Response<Body>) -> serde_json::Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn notebook_context_controls_execution_audit_and_legacy_client_saves() {
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let audit = Arc::new(InMemoryAudit::new());
    let fixture = fixture_configured(false, |state| {
        state.config.default_engine = Some("compute".into());
        state.config.catalog_bindings = vec![CatalogBindingConfig {
            catalog: "warehouse".into(),
            engine: "compute".into(),
            native_catalog: "native".into(),
            policy: BindingPolicy::Unprotected,
        }];
        state
            .catalogs
            .register(Arc::new(aster_catalogs::MockCatalog::new("warehouse")));
        state.engines.register(Arc::new(ContextCaptureEngine {
            info: EngineInfo {
                id: EngineId::new("compute"),
                kind: "mock".into(),
                endpoint: String::new(),
                routing_group: None,
            },
            calls: calls.clone(),
        }));
        let grants = InMemoryGrants::new();
        grants.grant("alice", "compute");
        state.grants = Arc::new(grants);
        state.audit = audit.clone();
    })
    .await;
    let mut document = json!(notebook("scope", "SELECT 1"));
    document["context"] = json!({"catalog_context":"warehouse","schema":"sales.eu"});
    let created = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/scope",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        document.clone(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let mut revision = response_json(created).await["content_revision"]
        .as_str()
        .unwrap()
        .to_owned();
    for patch in [
        json!({"schema":"other"}),
        json!({"catalog_context":"other"}),
        json!({"catalog":"other"}),
    ] {
        let mut request = json!({"notebook":"scope","cell":"c1","sql":"SELECT 1"});
        request
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let response = call(
            &fixture.app,
            "POST",
            "/api/query",
            "alice",
            "editor",
            None,
            request,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "conflicting notebook context must refuse"
        );
    }
    assert!(calls.lock().unwrap().is_empty());
    for path in ["/api/query", "/aster.v1.Aster/RunQuery"] {
        let response = call(
            &fixture.app,
            "POST",
            path,
            "alice",
            "editor",
            None,
            json!({"notebook":"scope","cell":"c1","sql":"SELECT 1"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.catalog.as_deref() == Some("native")
                && r.schema.as_deref() == Some("sales.eu"))
    );
    let events = audit.events().await.unwrap();
    assert!(events
        .iter()
        .filter(|e| e.ok)
        .all(|e| e.catalog.as_deref() == Some("warehouse")
            && e.schema.as_deref() == Some("sales.eu")));
    for rpc in [false, true] {
        let mut old = document.clone();
        old.as_object_mut().unwrap().remove("context");
        old["title"] = json!(if rpc { "RPC edit" } else { "REST edit" });
        let response = if rpc {
            call(
                &fixture.app,
                "POST",
                "/aster.v1.Aster/SaveNotebook",
                "alice",
                "editor",
                None,
                json!({"id":"scope","notebook":old,"expectedContentRevision":revision}),
            )
            .await
        } else {
            call(
                &fixture.app,
                "PUT",
                "/api/notebooks/scope",
                "alice",
                "editor",
                Some(("if-match", format!("\"{revision}\""))),
                old,
            )
            .await
        };
        assert_eq!(response.status(), StatusCode::OK);
        let reloaded = call(
            &fixture.app,
            "GET",
            "/api/notebooks/scope",
            "alice",
            "editor",
            None,
            json!({}),
        )
        .await;
        revision = reloaded.headers()["etag"]
            .to_str()
            .unwrap()
            .trim_matches('"')
            .to_owned();
        assert_eq!(
            response_json(reloaded).await["context"],
            document["context"]
        );
    }
    let rpc = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/GetNotebook",
        "alice",
        "editor",
        None,
        json!({"id":"scope"}),
    )
    .await;
    assert_eq!(
        response_json(rpc).await["context"],
        json!({"catalogContext":"warehouse","schema":"sales.eu"})
    );
    document["context"] = json!({});
    let cleared = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/scope",
        "alice",
        "editor",
        Some(("if-match", format!("\"{revision}\""))),
        document,
    )
    .await;
    assert_eq!(cleared.status(), StatusCode::OK);
    let response = call(
        &fixture.app,
        "POST",
        "/api/query",
        "alice",
        "editor",
        None,
        json!({"notebook":"scope","cell":"c1","sql":"SELECT 1"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn notebook_history_is_owned_and_preserves_historical_metadata() {
    let fixture = fixture().await;
    let mut original = notebook("history", "SELECT 1");
    original.cells[0]
        .metadata
        .insert("note".into(), json!("original"));
    let created = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/history",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(original),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let first = git(&fixture.dir, &["rev-parse", "HEAD"]);
    let oid = git(&fixture.dir, &["rev-parse", "HEAD:history.aster"]);
    let mut changed = notebook("history", "SELECT 2");
    changed.context = Some(NotebookContext {
        catalog_context: Some("new-context".into()),
        schema: None,
    });
    let updated = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/history",
        "alice",
        "editor",
        Some(("if-match", format!("\"{oid}\""))),
        json!(changed),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    for actor in ["alice", "bob"] {
        let response = call(
            &fixture.app,
            "POST",
            "/aster.v1.Aster/ListNotebookRevisions",
            actor,
            "editor",
            None,
            json!({"id":"history"}),
        )
        .await;
        assert_eq!(
            response.status(),
            if actor == "alice" {
                StatusCode::OK
            } else {
                StatusCode::FORBIDDEN
            }
        );
    }
    let response = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/GetNotebookRevision",
        "alice",
        "editor",
        None,
        json!({"id":"history", "revision":first}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let document: Notebook = serde_json::from_str(value["notebookJson"].as_str().unwrap()).unwrap();
    assert_eq!(document.cells[0].sql, "SELECT 1");
    assert_eq!(document.cells[0].metadata["note"], "original");
    let current = git(&fixture.dir, &["rev-parse", "HEAD:history.aster"]);
    let denied = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/RestoreNotebookRevision",
        "alice",
        "viewer",
        None,
        json!({"id":"history", "revision":first, "expectedContentRevision":current}),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let restore = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/RestoreNotebookRevision",
        "alice",
        "editor",
        None,
        json!({"id":"history", "revision":first, "expectedContentRevision":current}),
    )
    .await;
    assert_eq!(restore.status(), StatusCode::OK);
    let restored = fixture._store.get("history").await.unwrap();
    assert_eq!(restored.cells[0].metadata["note"], "original");
    assert!(restored.context.is_none());
    let stale = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/RestoreNotebookRevision",
        "alice",
        "editor",
        None,
        json!({"id":"history", "revision":first, "expectedContentRevision":current}),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    for (actor, revision, status) in [
        ("bob", first.as_str(), StatusCode::FORBIDDEN),
        ("alice", "HEAD", StatusCode::BAD_REQUEST),
        ("alice", "--all", StatusCode::BAD_REQUEST),
    ] {
        let response = call(
            &fixture.app,
            "POST",
            "/aster.v1.Aster/GetNotebookRevision",
            actor,
            "editor",
            None,
            json!({"id":"history", "revision":revision}),
        )
        .await;
        assert_eq!(response.status(), status);
    }
}
