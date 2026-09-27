use aster_core::{CoreError, QueryEngine, QueryRequest, TrinoDelegationConfig};
use aster_engines::TrinoEngine;

// The disposable Compose runner supplies these paths and selects this ignored
// test. Never run against a standing Trino endpoint or committed credentials.
#[tokio::test]
#[ignore]
async fn verified_subject_reaches_trino_table_policy() {
    let endpoint = std::env::var("V3B1B_TRINO_ENDPOINT").expect("fixture Trino endpoint");
    let token_file = std::env::var("V3B1B_TRINO_TOKEN_FILE").expect("fixture token file");
    let ca_file = std::env::var("V3B1B_TRINO_CA_FILE").expect("fixture CA file");
    let engine = TrinoEngine::new_authenticated(
        "trino-protected",
        endpoint,
        None,
        TrinoDelegationConfig {
            token_file,
            ca_file,
        },
    )
    .expect("configured fixture delegation");
    let query = QueryRequest {
        sql: "SELECT id, label FROM polaris.sales.orders ORDER BY id".into(),
        catalog: Some("polaris".into()),
        schema: Some("sales".into()),
        max_rows: Some(10),
    };
    let alice = engine
        .execute_as_verified(query.clone(), "alice")
        .await
        .expect("verified Alice reads independently seeded Iceberg rows");
    assert_eq!(
        alice.rows,
        vec![
            vec![serde_json::json!(1), serde_json::json!("alpha")],
            vec![serde_json::json!(2), serde_json::json!("beta")]
        ]
    );
    let bob = engine.execute_as_verified(query, "bob").await;
    assert!(
        matches!(bob, Err(CoreError::Query(message)) if message.contains("Access Denied")),
        "Bob must reach Trino and be denied by its table policy"
    );
    let expired = TrinoEngine::new_authenticated(
        "trino-expired",
        engine.info().endpoint.clone(),
        None,
        TrinoDelegationConfig {
            token_file: std::env::var("V3B1B_TRINO_EXPIRED_TOKEN_FILE").unwrap(),
            ca_file: std::env::var("V3B1B_TRINO_CA_FILE").unwrap(),
        },
    )
    .unwrap();
    let control = QueryRequest {
        sql: "SELECT 1".into(),
        catalog: None,
        schema: None,
        max_rows: Some(1),
    };
    assert!(matches!(
        expired.execute_as_verified(control.clone(), "alice").await,
        Err(CoreError::Unauthorized(_))
    ));
    let missing = TrinoEngine::new_authenticated(
        "trino-missing-token",
        engine.info().endpoint.clone(),
        None,
        TrinoDelegationConfig {
            token_file: format!(
                "{}.missing",
                std::env::var("V3B1B_TRINO_TOKEN_FILE").unwrap()
            ),
            ca_file: std::env::var("V3B1B_TRINO_CA_FILE").unwrap(),
        },
    )
    .unwrap();
    assert!(matches!(
        missing.execute_as_verified(control.clone(), "alice").await,
        Err(CoreError::Unauthorized(_))
    ));
    let wrong_ca = TrinoEngine::new_authenticated(
        "trino-wrong-ca",
        engine.info().endpoint.clone(),
        None,
        TrinoDelegationConfig {
            token_file: std::env::var("V3B1B_TRINO_TOKEN_FILE").unwrap(),
            ca_file: std::env::var("V3B1B_TRINO_WRONG_CA_FILE").unwrap(),
        },
    )
    .unwrap();
    assert!(matches!(
        wrong_ca.execute_as_verified(control, "alice").await,
        Err(CoreError::Engine(_))
    ));
    let redirect = TrinoEngine::new_authenticated(
        "trino-redirect",
        std::env::var("V3B1B_REDIRECT_ENDPOINT").unwrap(),
        None,
        TrinoDelegationConfig {
            token_file: std::env::var("V3B1B_TRINO_TOKEN_FILE").unwrap(),
            ca_file: std::env::var("V3B1B_TRINO_CA_FILE").unwrap(),
        },
    )
    .unwrap();
    assert!(matches!(
        redirect
            .execute_as_verified(
                QueryRequest {
                    sql: "SELECT 1".into(),
                    catalog: None,
                    schema: None,
                    max_rows: Some(1),
                },
                "alice"
            )
            .await,
        Err(CoreError::Engine(_))
    ));
}
