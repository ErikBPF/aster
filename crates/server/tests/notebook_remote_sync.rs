//! V7a assertion RED: Save stays local; explicit authorized Sync publishes the
//! exact session branch commit to a disposable bare repository.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aster_core::*;
use aster_server::{app, AppState, GitNotebookStore, Metrics, TeamPolicy, TeamWorkspaces};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::json;
use tower::ServiceExt;

struct Fixture {
    app: Router,
    sessions: Arc<InMemorySessions>,
    root: PathBuf,
    remote: PathBuf,
    _legacy: GitNotebookStore,
}

fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn git_ok(dir: &Path, args: &[&str]) -> String {
    let output = git(dir, args);
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

async fn fixture() -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "aster-sync-red-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let remote = root.join("alpha.git");
    git_ok(&root, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    let seed = root.join("seed");
    git_ok(&root, &["init", "-q", seed.to_str().unwrap()]);
    git_ok(&seed, &["config", "user.name", "fixture"]);
    git_ok(&seed, &["config", "user.email", "fixture@example.invalid"]);
    std::fs::write(
        seed.join("base.aster"),
        "# aster notebook v2\n# title: Base\n-- cell c1\nSELECT 101\n",
    )
    .unwrap();
    git_ok(&seed, &["add", "base.aster"]);
    git_ok(&seed, &["commit", "-qm", "base"]);
    git_ok(&seed, &["branch", "-M", "main"]);
    git_ok(
        &seed,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git_ok(&seed, &["push", "-q", "origin", "main"]);
    let policy = HashMap::from([(
        "alpha".into(),
        TeamPolicy {
            member_claim: "/teams/alpha/members".into(),
            maintainer_claim: "/teams/alpha/maintainers".into(),
            allowed_repositories: HashSet::from(["example/alpha".into()]),
            local_repositories: HashMap::from([("example/alpha".into(), remote.clone())]),
        },
    )]);
    let workspaces = Arc::new(TeamWorkspaces::new(root.join("workspaces"), policy).unwrap());
    workspaces
        .configure("alpha", "example/alpha", "main", None, "fixture")
        .unwrap();
    let legacy = GitNotebookStore::open(root.join("legacy"), "legacy").unwrap();
    let sessions = Arc::new(InMemorySessions::new(3600));
    let state = AppState {
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
        notebooks: Arc::new(legacy.clone()),
        notebook_owners: Arc::new(InMemoryNotebookOwners::default()),
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: Some(workspaces),
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
        sessions: sessions.clone(),
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: false,
        metrics: Arc::new(Metrics::new()),
    };
    Fixture {
        app: app(Arc::new(state)),
        sessions,
        root,
        remote,
        _legacy: legacy,
    }
}

async fn sid(sessions: &InMemorySessions, subject: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    sessions
        .create(
            subject,
            vec![Role::Editor],
            vec!["/teams/alpha/members".into()],
            None,
            now,
        )
        .await
        .unwrap()
        .sid
}

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    sid: &str,
    body: serde_json::Value,
    headers: &[(&str, &str)],
) -> axum::http::Response<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("cookie", format!("aster_session={sid}"));
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    app.clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

fn one_checkout(root: &Path) -> PathBuf {
    let sessions = root.join("workspaces/alpha/sessions");
    let subject = std::fs::read_dir(sessions)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::read_dir(subject)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
}

#[tokio::test]
async fn save_is_local_until_explicit_sync_verifies_the_exact_remote_ref() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice").await;
    let saved = call(
        &f.app,
        "PUT",
        "/api/teams/alpha/notebooks/sales",
        &alice,
        json!({"id":"sales", "title":"Sales", "cells":[{"id":"c1", "sql":"SELECT 1"}]}),
        &[("if-none-match", "*")],
    )
    .await;
    assert_eq!(saved.status(), StatusCode::OK);
    let checkout = one_checkout(&f.root);
    let reference = git_ok(&checkout, &["symbolic-ref", "HEAD"]);
    let local = git_ok(&checkout, &["rev-parse", "HEAD"]);
    assert!(!git(&f.remote, &["rev-parse", "--verify", &reference])
        .status
        .success());

    let response = call(
        &f.app,
        "POST",
        "/api/teams/alpha/notebooks/sales/sync",
        &alice,
        json!({}),
        &[],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let status: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(status["remote_revision"], local);
    assert_eq!(
        git_ok(&f.remote, &["rev-parse", "--verify", &reference]),
        local
    );
    let independent = f.root.join("independent");
    git_ok(
        &f.root,
        &[
            "clone",
            "-q",
            f.remote.to_str().unwrap(),
            independent.to_str().unwrap(),
        ],
    );
    let sql = git_ok(&independent, &["show", &format!("{local}:sales.aster")]);
    assert!(sql.contains("SELECT 1"));
    let retry = call(
        &f.app,
        "POST",
        "/api/teams/alpha/notebooks/sales/sync",
        &alice,
        json!({}),
        &[],
    )
    .await;
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(git_ok(&checkout, &["rev-parse", "HEAD"]), local);
    assert_eq!(
        git_ok(&f.remote, &["rev-parse", "--verify", &reference]),
        local
    );
}

#[tokio::test]
async fn sync_refuses_client_repository_or_branch_selection_before_remote_write() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice").await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            &alice,
            json!({"id":"sales", "title":"Sales", "cells":[{"id":"c1", "sql":"SELECT 2"}]}),
            &[("if-none-match", "*")],
        )
        .await
        .status(),
        StatusCode::OK
    );
    let checkout = one_checkout(&f.root);
    let reference = git_ok(&checkout, &["symbolic-ref", "HEAD"]);
    let rejected = call(
        &f.app,
        "POST",
        "/api/teams/alpha/notebooks/sales/sync",
        &alice,
        json!({"repository":"foreign/repo", "branch":"personal/bob"}),
        &[],
    )
    .await;
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    assert!(!git(&f.remote, &["rev-parse", "--verify", &reference])
        .status
        .success());
}

#[tokio::test]
async fn unavailable_remote_keeps_the_local_commit_pending() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice").await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            &alice,
            json!({"id":"sales", "title":"Sales", "cells":[{"id":"c1", "sql":"SELECT 3"}]}),
            &[("if-none-match", "*")]
        )
        .await
        .status(),
        StatusCode::OK
    );
    let checkout = one_checkout(&f.root);
    let local = git_ok(&checkout, &["rev-parse", "HEAD"]);
    std::fs::rename(&f.remote, f.root.join("unavailable.git")).unwrap();
    let response = call(
        &f.app,
        "POST",
        "/api/teams/alpha/notebooks/sales/sync",
        &alice,
        json!({}),
        &[],
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(git_ok(&checkout, &["rev-parse", "HEAD"]), local);
    assert!(git_ok(&checkout, &["show", "HEAD:sales.aster"]).contains("SELECT 3"));
}

#[tokio::test]
async fn divergent_remote_ref_is_not_forced_over() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice").await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            &alice,
            json!({"id":"sales", "title":"Sales", "cells":[{"id":"c1", "sql":"SELECT local"}]}),
            &[("if-none-match", "*")]
        )
        .await
        .status(),
        StatusCode::OK
    );
    let checkout = one_checkout(&f.root);
    let reference = git_ok(&checkout, &["symbolic-ref", "HEAD"]);
    let local = git_ok(&checkout, &["rev-parse", "HEAD"]);
    let seed = f.root.join("seed");
    git_ok(
        &seed,
        &[
            "checkout",
            "-qb",
            reference.strip_prefix("refs/heads/").unwrap(),
            "main",
        ],
    );
    std::fs::write(seed.join("remote.txt"), "remote-only change\n").unwrap();
    git_ok(&seed, &["add", "remote.txt"]);
    git_ok(&seed, &["commit", "-qm", "remote-only"]);
    git_ok(
        &seed,
        &["push", "-q", "origin", &format!("HEAD:{reference}")],
    );
    let remote = git_ok(&f.remote, &["rev-parse", "--verify", &reference]);
    assert_ne!(remote, local);

    let response = call(
        &f.app,
        "POST",
        "/api/teams/alpha/notebooks/sales/sync",
        &alice,
        json!({}),
        &[],
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(git_ok(&checkout, &["rev-parse", "HEAD"]), local);
    assert_eq!(
        git_ok(&f.remote, &["rev-parse", "--verify", &reference]),
        remote
    );
}

#[tokio::test]
async fn retry_after_lost_push_response_verifies_existing_remote_commit() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice").await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            &alice,
            json!({"id":"sales", "title":"Sales", "cells":[{"id":"c1", "sql":"SELECT retry"}]}),
            &[("if-none-match", "*")]
        )
        .await
        .status(),
        StatusCode::OK
    );
    let checkout = one_checkout(&f.root);
    let reference = git_ok(&checkout, &["symbolic-ref", "HEAD"]);
    let local = git_ok(&checkout, &["rev-parse", "HEAD"]);
    git_ok(
        &checkout,
        &[
            "push",
            "-q",
            f.remote.to_str().unwrap(),
            &format!("HEAD:{reference}"),
        ],
    );
    let response = call(
        &f.app,
        "POST",
        "/api/teams/alpha/notebooks/sales/sync",
        &alice,
        json!({}),
        &[],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let status: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(status["remote_revision"], local);
    assert_eq!(
        git_ok(&f.remote, &["rev-parse", "--verify", &reference]),
        local
    );
}

#[tokio::test]
async fn deleted_remote_branch_is_not_silently_recreated() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice").await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            &alice,
            json!({"id":"sales", "title":"Sales", "cells":[{"id":"c1", "sql":"SELECT 1"}]}),
            &[("if-none-match", "*")],
        )
        .await
        .status(),
        StatusCode::OK
    );
    let checkout = one_checkout(&f.root);
    let reference = git_ok(&checkout, &["symbolic-ref", "HEAD"]);
    let local = git_ok(&checkout, &["rev-parse", "HEAD"]);
    assert_eq!(
        call(
            &f.app,
            "POST",
            "/api/teams/alpha/notebooks/sales/sync",
            &alice,
            json!({}),
            &[]
        )
        .await
        .status(),
        StatusCode::OK
    );
    git_ok(&f.remote, &["update-ref", "-d", &reference]);
    assert_eq!(
        call(
            &f.app,
            "POST",
            "/api/teams/alpha/notebooks/sales/sync",
            &alice,
            json!({}),
            &[]
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(git_ok(&checkout, &["rev-parse", "HEAD"]), local);
    assert!(!git(&f.remote, &["rev-parse", "--verify", &reference])
        .status
        .success());
}

#[tokio::test]
async fn another_member_or_session_cannot_sync_the_owners_branch() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice").await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            &alice,
            json!({"id":"sales", "title":"Sales", "cells":[{"id":"c1", "sql":"SELECT private"}]}),
            &[("if-none-match", "*")]
        )
        .await
        .status(),
        StatusCode::OK
    );
    let checkout = one_checkout(&f.root);
    let reference = git_ok(&checkout, &["symbolic-ref", "HEAD"]);
    let bob = sid(&f.sessions, "bob").await;
    let other_alice_session = sid(&f.sessions, "alice").await;
    for caller in [&bob, &other_alice_session] {
        let response = call(
            &f.app,
            "POST",
            "/api/teams/alpha/notebooks/sales/sync",
            caller,
            json!({}),
            &[],
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    assert!(!git(&f.remote, &["rev-parse", "--verify", &reference])
        .status
        .success());
}

#[tokio::test]
async fn unauthenticated_sync_request_is_refused_before_remote_write() {
    let f = fixture().await;
    let request = Request::builder()
        .method("POST")
        .uri("/api/teams/alpha/notebooks/sales/sync")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let response = f.app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!f.root.join("workspaces/alpha").exists());
}
