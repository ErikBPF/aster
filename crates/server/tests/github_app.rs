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
