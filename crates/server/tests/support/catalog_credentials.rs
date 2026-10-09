#[path = "../../../catalogs/tests/support/quality.rs"]
#[allow(dead_code)]
pub mod quality;

pub async fn partial_polaris_environment_refuses_startup() {
    use serde_json::json;
    use std::{
        process::{Command, Stdio},
        time::Duration,
    };
    let upstream = quality::Fixture::start(false).await;
    upstream.respond(
        "/api/catalog/v1/lake/namespaces",
        200,
        json!({"namespaces":[]}),
    );
    upstream.respond(
        "/api/catalog/v1/oauth/tokens",
        200,
        json!({"access_token":"fixture-env-token"}),
    );
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    let mut refused = Vec::new();
    for (id, secret, valid) in [
        (None, None, true),
        (Some("FIXTURE_ENV_ID"), Some("FIXTURE_ENV_SECRET"), true),
        (Some("FIXTURE_ENV_ID"), None, false),
        (None, Some("FIXTURE_ENV_SECRET"), false),
    ] {
        let root = tempfile::tempdir().unwrap();
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        drop(socket);
        let log_path = root.path().join("startup.log");
        let log = std::fs::File::create(&log_path).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_aster-server"));
        command
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap())
            .env("HOME", root.path())
            .env("ASTER_BIND", address.to_string())
            .env("ASTER_METRICS_BIND", "127.0.0.1:0")
            .env("ASTER_NOTEBOOK_DIR", root.path().join("notebooks"))
            .env("ASTER_CONTRACTS_DIR", root.path().join("contracts"))
            .env("ASTER_ENGINES", "e;mock;local")
            .env("POLARIS_ENDPOINT", &upstream.endpoint)
            .env("POLARIS_CATALOG", "lake")
            .env("ASTER_CATALOG_BINDINGS", "polaris;e;lake;unprotected")
            .env("ASTER_IDP_KIND", "none")
            .env("ASTER_DEV_LOGIN", "1")
            .env("ASTER_METADATA_STORE", "memory")
            .env("ASTER_STATE_STORE", "memory")
            .env("ASTER_SECRET_STORE", "env")
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log));
        if let Some(id) = id {
            command.env("POLARIS_CLIENT_ID", id);
        }
        if let Some(secret) = secret {
            command.env("POLARIS_CLIENT_SECRET", secret);
        }
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let before = upstream.requests.lock().unwrap().len();
        let mut child = Child(command.spawn().unwrap());
        let mut exited = None;
        let mut ready = false;
        for _ in 0..100 {
            exited = child.0.try_wait().unwrap();
            if exited.is_some() {
                break;
            }
            if http
                .get(format!("http://{address}/healthz"))
                .send()
                .await
                .is_ok()
            {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // On the broken implementation, exercise the actual anonymous fallback.
        if ready {
            let response = http
                .get(format!("http://{address}/api/catalogs/polaris/namespaces"))
                .header("x-aster-subject", "fixture")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
        }
        drop(child);
        let output = std::fs::read_to_string(log_path).unwrap();
        for sentinel in ["FIXTURE_ENV_ID", "FIXTURE_ENV_SECRET", "fixture-env-token"] {
            assert!(
                !output.contains(sentinel),
                "startup diagnostics exposed fixture credentials"
            );
        }
        let seen = upstream.requests.lock().unwrap();
        if valid {
            assert!(
                ready && exited.is_none(),
                "both absent and complete credentials must remain supported"
            );
            assert_eq!(seen.len() - before, if id.is_some() { 2 } else { 1 });
            let request = seen.last().unwrap().to_lowercase();
            assert_eq!(
                request.contains("authorization: bearer fixture-env-token"),
                id.is_some()
            );
            if id.is_none() {
                assert!(!request.contains("authorization:"));
            }
        } else {
            refused.push((
                exited.is_some_and(|status| !status.success()),
                seen.len() - before,
            ));
        }
    }
    assert_eq!(
        refused,
        [(true, 0), (true, 0)],
        "each partial OAuth environment must refuse startup with zero upstream requests"
    );
}

pub async fn secret_selection_reaches_only_the_configured_catalog() {
    use serde_json::json;
    use std::{
        process::{Command, Stdio},
        time::Duration,
    };
    let upstream = quality::Fixture::start(false).await;
    upstream.respond(
        "/api/catalog/v1/lake/namespaces",
        200,
        json!({"namespaces":[]}),
    );
    upstream.respond("/api/v1/databaseSchemas?limit=200", 200, json!({"data":[]}));
    upstream.respond("/cubejs-api/v1/meta", 200, json!({"cubes":[]}));
    let root = tempfile::tempdir().unwrap();
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    drop(socket);
    let spec = format!("p;polaris;{};lake;secret://S6_POLARIS,o;openmetadata;{};;secret://S6_OM,c;cube;{};;secret://S6_CUBE",upstream.endpoint,upstream.endpoint,upstream.endpoint);
    let mut command = Command::new(env!("CARGO_BIN_EXE_aster-server"));
    command
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap())
        .env("HOME", root.path())
        .env("ASTER_BIND", address.to_string())
        .env("ASTER_METRICS_BIND", "127.0.0.1:0")
        .env("ASTER_NOTEBOOK_DIR", root.path().join("notebooks"))
        .env("ASTER_CONTRACTS_DIR", root.path().join("contracts"))
        .env("ASTER_ENGINES", "e;mock;http://127.0.0.1:9")
        .env("ASTER_CATALOGS", spec)
        .env(
            "ASTER_CATALOG_BINDINGS",
            "p;e;p;unprotected,o;e;o;unprotected,c;e;c;unprotected",
        )
        .env("ASTER_IDP_KIND", "none")
        .env("ASTER_DEV_LOGIN", "1")
        .env("ASTER_METADATA_STORE", "memory")
        .env("ASTER_STATE_STORE", "memory")
        .env("ASTER_SECRET_STORE", "env")
        .env("S6_POLARIS", "fixture-p-only")
        .env("S6_OM", "fixture-o-only")
        .env("S6_CUBE", "fixture-c-only")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Child(command.spawn().unwrap());
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let mut ready = false;
    for _ in 0..100 {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "configured server exited before readiness"
        );
        if http
            .get(format!("http://{address}/healthz"))
            .send()
            .await
            .is_ok()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ready, "server did not become ready");
    for id in ["p", "o", "c"] {
        let response = http
            .get(format!("http://{address}/api/catalogs/{id}/namespaces"))
            .header("x-aster-subject", "fixture")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let seen = upstream.requests.lock().unwrap();
        let request = seen.last().unwrap().to_lowercase();
        assert!(
            request.contains(&format!("authorization: bearer fixture-{id}-only")),
            "selected secret did not reach its catalog"
        );
        assert!(!request.contains("secret://"));
        for other in ["p", "o", "c"].into_iter().filter(|other| *other != id) {
            assert!(!request.contains(&format!("fixture-{other}-only")));
        }
    }
    drop(child);
    // The selected memory provider cannot fall through to process environment.
    command.env("ASTER_SECRET_STORE", "memory");
    let mut child = Child(command.spawn().unwrap());
    for _ in 0..100 {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(!status.success());
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("missing selected secret must refuse startup, not fall back to env");
}
