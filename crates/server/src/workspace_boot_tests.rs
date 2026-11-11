//! Startup refusal must happen before exposing an incomplete workspace service.
use aster_core::AppConfig;

#[tokio::test]
async fn workspace_opt_in_requires_complete_authority_and_storage() {
    if let Ok(enabled) = std::env::var("ASTER_WORKSPACE_BOOT_CASE") {
        let result = crate::build_state(AppConfig {
            bind: String::new(),
            engines: vec![],
            catalogs: vec![],
            catalog_bindings: vec![],
            default_engine: None,
            default_catalog: None,
        })
        .await;
        if enabled == "0" {
            assert!(result.unwrap().durable_workspaces.is_none());
        } else {
            assert!(
                result.is_err(),
                "workspace opt-in must refuse missing team authority/storage and invalid flags"
            );
        }
        return;
    }
    for enabled in ["0", "1", "typo"] {
        let directory = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "workspace_boot_tests::workspace_opt_in_requires_complete_authority_and_storage",
                "--nocapture",
            ])
            .env("ASTER_WORKSPACE_BOOT_CASE", enabled)
            .env("ASTER_WORKSPACES_ENABLED", enabled)
            .env("ASTER_TEAM_GIT_ENABLED", "0")
            .env_remove("ASTER_TEAM_POLICY_FILE")
            .env("ASTER_METADATA_STORE", "memory")
            .env("ASTER_STATE_STORE", "memory")
            .env("ASTER_NOTEBOOK_MODE", "local")
            .env("ASTER_NOTEBOOK_DIR", directory.path())
            .env("ASTER_IDP_KIND", "none")
            .env("ASTER_SHARED_MODELS_ENABLED", "0")
            .env("ASTER_SHARED_MODEL_AUTHORITY_ENABLED", "0")
            .env("ASTER_SHARED_MODEL_USE_ENABLED", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "case {enabled}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; run alone through workspace-postgres.sh"]
async fn postgres_workspace_opt_in() {
    let root = tempfile::tempdir().unwrap();
    let policy = root.path().join("teams.json");
    std::fs::write(&policy, r#"{"alpha":{"member_group_uuid":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","maintainer_group_uuid":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","installation_id":41,"allowed_repositories":{"example/alpha":73}}}"#).unwrap();
    // This ignored test runs alone in a disposable process and never contacts the fixture origins.
    unsafe {
        for (key, value) in [
            ("ASTER_WORKSPACES_ENABLED", "1"),
            ("ASTER_TEAM_GIT_ENABLED", "1"),
            ("ASTER_METADATA_STORE", "postgres"),
            ("ASTER_STATE_STORE", "memory"),
            ("ASTER_NOTEBOOK_MODE", "local"),
            ("ASTER_IDP_KIND", "oidc"),
            (
                "ASTER_OIDC_ISSUER",
                "https://id.example.invalid/application/o/aster/",
            ),
            ("ASTER_IDP_USER_UUID_CLAIM", "aster_user_uuid"),
            ("ASTER_AUTHENTIK_API_ORIGIN", "https://id.example.invalid/"),
            ("ASTER_AUTHENTIK_READ_TOKEN", "disposable-test-token"),
            (
                "ASTER_AUTHENTIK_ADMIN_GROUP_UUID",
                "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            ),
            (
                "ASTER_AUTHENTIK_EDITOR_GROUP_UUID",
                "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            ),
            ("ASTER_GITHUB_APP_ID", "7"),
            ("ASTER_GITHUB_APP_KEY_NAME", "ASTER_TEST_GITHUB_APP_KEY"),
            ("ASTER_TEST_GITHUB_APP_KEY", "disposable-test-key"),
            ("ASTER_SHARED_MODELS_ENABLED", "0"),
            ("ASTER_SHARED_MODEL_AUTHORITY_ENABLED", "0"),
            ("ASTER_SHARED_MODEL_USE_ENABLED", "0"),
            ("ASTER_WORKSPACE_S3_ENDPOINT", "https://s3.example.invalid/"),
            ("ASTER_WORKSPACE_S3_BUCKET", "workspaces"),
            ("ASTER_WORKSPACE_S3_REGION", "us-east-1"),
            ("ASTER_WORKSPACE_S3_ACCESS_KEY", "fixture"),
            ("ASTER_WORKSPACE_S3_SECRET_KEY", "fixture"),
        ] {
            std::env::set_var(key, value);
        }
        std::env::set_var(
            "DATABASE_URL",
            std::env::var("ASTER_TEST_METADATA_URL").unwrap(),
        );
        std::env::set_var("ASTER_TEAM_GIT_POLICY_FILE", policy);
        std::env::set_var("ASTER_NOTEBOOK_DIR", root.path().join("notebooks"));
        std::env::remove_var("ASTER_TEAM_POLICY_FILE");
        std::env::remove_var("ASTER_DEV_LOGIN");
    }
    let config = AppConfig {
        bind: String::new(),
        engines: vec![],
        catalogs: vec![],
        catalog_bindings: vec![],
        default_engine: None,
        default_catalog: None,
    };
    let state = crate::build_state(config.clone()).await.unwrap();
    assert!(state.durable_workspaces.is_some());
    assert!(!state.local_notebooks_enabled());
    assert!(state.current_identity.is_some());
    for provider in [
        "ASTER_WORKSPACE_CHECKOUTS",
        "ASTER_WORKSPACE_HOSTING",
        "ASTER_WORKSPACE_RECOVERY",
    ] {
        unsafe {
            std::env::set_var(provider, "typo");
        }
        assert!(
            crate::build_state(config.clone()).await.is_err(),
            "unknown provider must fail startup"
        );
        unsafe {
            std::env::remove_var(provider);
        }
    }
    unsafe {
        std::env::remove_var("ASTER_AUTHENTIK_EDITOR_GROUP_UUID");
    }
    assert!(
        crate::build_state(config).await.is_err(),
        "workspace authority needs stable role UUIDs"
    );
}
