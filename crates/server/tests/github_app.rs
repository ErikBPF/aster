#[path = "../src/github_app.rs"]
mod github_app;

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use aster_core::InMemorySecrets;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use ring::signature::{RsaKeyPair, UnparsedPublicKey, RSA_PKCS1_2048_8192_SHA256};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use github_app::GithubApp;

async fn token_request(
    State(sent): State<mpsc::Sender<(HeaderMap, Value)>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    sent.send((headers, body)).await.unwrap();
    Json(json!({"token":"disposable-test-token","expires_at":"2099-01-01T00:00:00Z"}))
}

async fn expired_token_request() -> Json<Value> {
    Json(json!({"token":"expired-test-token","expires_at":"2000-01-01T00:00:00Z"}))
}

#[derive(Clone)]
struct RepositoryFixture {
    installation_id: u64,
    repository_id: u64,
}

async fn repository_installation(State(fixture): State<RepositoryFixture>) -> Json<Value> {
    Json(json!({"id":fixture.installation_id}))
}

async fn repository_metadata(State(fixture): State<RepositoryFixture>) -> Json<Value> {
    Json(json!({
        "id":fixture.repository_id,
        "full_name":"acme/notebooks",
        "default_branch":"main"
    }))
}

async fn verification_token_request() -> Json<Value> {
    Json(json!({"token":"disposable-test-token","expires_at":"2099-01-01T00:00:00Z"}))
}

async fn default_ref() -> Json<Value> {
    Json(json!({
        "ref":"refs/heads/main",
        "object":{"type":"commit","sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
    }))
}

#[tokio::test]
async fn installation_token_request_is_signed_and_scoped_to_one_repository() {
    let key = Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
        ])
        .output()
        .expect("openssl test key");
    assert!(key.status.success());
    let secrets = Arc::new(InMemorySecrets::new());
    secrets.set(
        "TEST_GITHUB_APP_KEY",
        std::str::from_utf8(&key.stdout).unwrap(),
    );

    let (sender, mut received) = mpsc::channel(1);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/app/installations/73/access_tokens", post(token_request))
                .with_state(sender),
        )
        .await
        .unwrap();
    });

    assert!(GithubApp::new(&origin, 41, "TEST_GITHUB_APP_KEY", secrets.clone()).is_err());
    let app = GithubApp::new_for_test(&origin, 41, "TEST_GITHUB_APP_KEY", secrets).unwrap();
    let token = app.installation_token(73, 101).await.unwrap();
    assert_eq!(token.as_str(), "disposable-test-token");
    let (headers, body) = received.recv().await.unwrap();
    assert_eq!(
        body,
        json!({"repository_ids":[101],"permissions":{"contents":"write"}})
    );
    let bearer = headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("Bearer ")
        .unwrap();
    let parts: Vec<_> = bearer.split('.').collect();
    assert_eq!(parts.len(), 3);
    let header: Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[0])
            .unwrap(),
    )
    .unwrap();
    let claims: Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(header, json!({"alg":"RS256","typ":"JWT"}));
    assert_eq!(claims["iss"], 41);
    let issued = claims["iat"].as_i64().unwrap();
    let expires = claims["exp"].as_i64().unwrap();
    assert!((1..=600).contains(&(expires - issued)));
    assert!(!parts[2].is_empty());
    let private_key_der = base64::engine::general_purpose::STANDARD
        .decode(
            std::str::from_utf8(&key.stdout)
                .unwrap()
                .lines()
                .filter(|line| !line.starts_with("-----"))
                .collect::<String>(),
        )
        .unwrap();
    let private_key = RsaKeyPair::from_pkcs8(&private_key_der).unwrap();
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[2])
        .unwrap();
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, private_key.public().as_ref())
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .unwrap();
    server.abort();
}

#[tokio::test]
async fn expired_installation_token_is_refused() {
    let key = Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
        ])
        .output()
        .expect("openssl test key");
    assert!(key.status.success());
    let secrets = Arc::new(InMemorySecrets::new());
    secrets.set(
        "TEST_GITHUB_APP_KEY",
        std::str::from_utf8(&key.stdout).unwrap(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/app/installations/73/access_tokens",
                post(expired_token_request),
            ),
        )
        .await
        .unwrap();
    });
    let app = GithubApp::new_for_test(&origin, 41, "TEST_GITHUB_APP_KEY", secrets).unwrap();
    assert!(app.installation_token(73, 101).await.is_err());
    server.abort();
}

#[tokio::test]
async fn repository_identity_requires_exact_installation_and_numeric_id() {
    let key = Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
        ])
        .output()
        .expect("openssl test key");
    assert!(key.status.success());
    let secrets = Arc::new(InMemorySecrets::new());
    secrets.set(
        "TEST_GITHUB_APP_KEY",
        std::str::from_utf8(&key.stdout).unwrap(),
    );
    for (installation_id, repository_id, allowed) in
        [(73, 101, true), (74, 101, false), (73, 102, false)]
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route(
                        "/repos/acme/notebooks/installation",
                        get(repository_installation),
                    )
                    .route("/repos/acme/notebooks", get(repository_metadata))
                    .route(
                        "/app/installations/73/access_tokens",
                        post(verification_token_request),
                    )
                    .with_state(RepositoryFixture {
                        installation_id,
                        repository_id,
                    }),
            )
            .await
            .unwrap();
        });
        let app =
            GithubApp::new_for_test(&origin, 41, "TEST_GITHUB_APP_KEY", secrets.clone()).unwrap();
        assert_eq!(
            app.verify_repository("acme/notebooks", 73, 101)
                .await
                .is_ok(),
            allowed
        );
        use aster_core::workspace::WorkspaceHosting;
        let credential = app.credential("acme/notebooks", 73, 101).await;
        assert_eq!(credential.is_ok(), allowed);
        if allowed {
            let credential = credential.unwrap();
            assert_eq!(
                credential.remote_url,
                "https://github.com/acme/notebooks.git"
            );
            assert_eq!(credential.token, "disposable-test-token");
            assert!(credential.ca_bundle.is_none());
        }
        server.abort();
    }
}

#[tokio::test]
async fn configured_default_branch_yields_only_its_exact_commit() {
    let key = Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
        ])
        .output()
        .expect("openssl test key");
    assert!(key.status.success());
    let secrets = Arc::new(InMemorySecrets::new());
    secrets.set(
        "TEST_GITHUB_APP_KEY",
        std::str::from_utf8(&key.stdout).unwrap(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route(
                    "/repos/acme/notebooks/installation",
                    get(repository_installation),
                )
                .route("/repos/acme/notebooks", get(repository_metadata))
                .route("/repos/acme/notebooks/git/ref/heads/main", get(default_ref))
                .route(
                    "/app/installations/73/access_tokens",
                    post(verification_token_request),
                )
                .with_state(RepositoryFixture {
                    installation_id: 73,
                    repository_id: 101,
                }),
        )
        .await
        .unwrap();
    });
    let app = GithubApp::new_for_test(&origin, 41, "TEST_GITHUB_APP_KEY", secrets).unwrap();
    assert_eq!(
        app.verified_default_commit("acme/notebooks", 73, 101, "main")
            .await
            .unwrap(),
        "a".repeat(40)
    );
    assert!(app
        .verified_default_commit("acme/notebooks", 73, 101, "other")
        .await
        .is_err());
    server.abort();
}

#[tokio::test]
async fn repository_verification_never_follows_a_credentialed_redirect() {
    let key = Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
        ])
        .output()
        .expect("openssl test key");
    assert!(key.status.success());
    let secrets = Arc::new(InMemorySecrets::new());
    secrets.set(
        "TEST_GITHUB_APP_KEY",
        std::str::from_utf8(&key.stdout).unwrap(),
    );

    let hits = Arc::new(AtomicUsize::new(0));
    let destination = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination_url = format!("http://{}/steal", destination.local_addr().unwrap());
    let seen = hits.clone();
    let destination_server = tokio::spawn(async move {
        axum::serve(
            destination,
            Router::new().route(
                "/steal",
                get(move || {
                    let seen = seen.clone();
                    async move {
                        seen.fetch_add(1, Ordering::SeqCst);
                        "redirect reached"
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let source = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", source.local_addr().unwrap());
    let source_server = tokio::spawn(async move {
        axum::serve(
            source,
            Router::new()
                .route(
                    "/repos/acme/notebooks/installation",
                    get(|State(url): State<String>| async move { Redirect::temporary(&url) }),
                )
                .with_state(destination_url),
        )
        .await
        .unwrap();
    });
    let app = GithubApp::new_for_test(&origin, 41, "TEST_GITHUB_APP_KEY", secrets).unwrap();
    assert!(app
        .verify_repository("acme/notebooks", 73, 101)
        .await
        .is_err());
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    source_server.abort();
    destination_server.abort();
}

#[tokio::test]
async fn explicit_pull_request_retry_reuses_exact_repository_head_and_base() {
    use aster_core::workspace::WorkspaceHosting;
    use axum::extract::Query;
    use std::collections::HashMap;
    let key = Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
        ])
        .output()
        .unwrap();
    assert!(key.status.success());
    let secrets = Arc::new(InMemorySecrets::new());
    secrets.set(
        "TEST_GITHUB_APP_KEY",
        std::str::from_utf8(&key.stdout).unwrap(),
    );
    let reject = Arc::new(AtomicUsize::new(1));
    let server_reject = reject.clone();
    let created = Arc::new(AtomicUsize::new(0));
    let listed = created.clone();
    let writes = created.clone();
    let scoped_tokens = Arc::new(std::sync::Mutex::new(Vec::new()));
    let token_requests = scoped_tokens.clone();
    let pr = json!({"number":7,"state":"open","html_url":"https://github.com/acme/notebooks/pull/7",
        "head":{"ref":"aster/alice/work","repo":{"id":101}},"base":{"ref":"main","repo":{"id":101}}});
    let listed_pr = pr.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new()
            .route("/repos/acme/notebooks/installation", get(|| async { Json(json!({"id":73})) }))
            .route("/repos/acme/notebooks", get(|| async { Json(json!({"id":101,"full_name":"acme/notebooks"})) }))
            .route("/app/installations/73/access_tokens", post(move |Json(body): Json<Value>| {
                token_requests.lock().unwrap().push(body);
                async { Json(json!({"token":"disposable-test-token","expires_at":"2099-01-01T00:00:00Z"})) }
            }))
            .route("/repos/acme/notebooks/pulls", get(move |Query(query): Query<HashMap<String,String>>| {
                let listed = listed.clone(); let pr = listed_pr.clone();
                async move {
                    assert_eq!(query.get("head").map(String::as_str), Some("acme:aster/alice/work"));
                    assert_eq!(query.get("base").map(String::as_str), Some("main"));
                    assert_eq!(query.get("state").map(String::as_str), Some("open"));
                    Json(if listed.load(Ordering::SeqCst) == 0 { json!([]) } else { json!([pr]) })
                }
            }).post(move |Json(body): Json<Value>| {
                let writes = writes.clone(); let pr = pr.clone(); let reject = server_reject.clone();
                async move {
                    assert_eq!(body["head"], "aster/alice/work"); assert_eq!(body["base"], "main");
                    assert_eq!(body["title"], "Analysis");
                    let mode = reject.swap(0, Ordering::SeqCst);
                    if mode == 1 { return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"message":"no commits"}))); }
                    writes.fetch_add(1, Ordering::SeqCst);
                    (axum::http::StatusCode::CREATED, Json(if mode == 2 { json!({"incomplete":true}) } else { pr }))
                }
            }))).await.unwrap();
    });
    let app = GithubApp::new_for_test(&origin, 41, "TEST_GITHUB_APP_KEY", secrets).unwrap();
    assert_eq!(
        app.find_pull_request("acme/notebooks", 73, 101, "aster/alice/work", "main")
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        created.load(Ordering::SeqCst),
        0,
        "lookup after an uncertain creation must never POST"
    );
    let rejected = app
        .open_pull_request(
            "acme/notebooks",
            73,
            101,
            "aster/alice/work",
            "main",
            "Analysis",
        )
        .await
        .unwrap_err();
    assert!(!rejected.may_have_written);
    assert_eq!(created.load(Ordering::SeqCst), 0);
    reject.store(2, Ordering::SeqCst);
    let uncertain = app
        .open_pull_request(
            "acme/notebooks",
            73,
            101,
            "aster/alice/work",
            "main",
            "Analysis",
        )
        .await
        .unwrap_err();
    assert!(uncertain.may_have_written);
    for _ in 0..2 {
        assert_eq!(
            app.open_pull_request(
                "acme/notebooks",
                73,
                101,
                "aster/alice/work",
                "main",
                "Analysis"
            )
            .await
            .unwrap(),
            "https://github.com/acme/notebooks/pull/7"
        );
    }
    assert_eq!(created.load(Ordering::SeqCst), 1);
    assert_eq!(
        app.find_pull_request("acme/notebooks", 73, 101, "aster/alice/work", "main")
            .await
            .unwrap()
            .as_deref(),
        Some("https://github.com/acme/notebooks/pull/7")
    );
    assert!(scoped_tokens
        .lock()
        .unwrap()
        .iter()
        .any(|body| body["repository_ids"] == json!([101])
            && body["permissions"] == json!({"pull_requests":"write"})));
    assert!(app
        .open_pull_request("acme/notebooks", 73, 101, "main", "main", "Analysis")
        .await
        .is_err());
    server.abort();
}
