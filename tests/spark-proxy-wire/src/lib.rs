pub mod spark {
    pub mod connect {
        tonic::include_proto!("spark.connect");
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::spark::connect::{
        spark_connect_service_server::{SparkConnectService, SparkConnectServiceServer},
        *,
    };
    use aster_core::{
        self as core, Catalog, CoreError, QueryEngine, QueryRequest, SessionRegistry,
    };
    use aster_engines::SparkEngine;
    use aster_server::{app, AppState, Metrics};
    use axum::{
        body::{to_bytes, Body},
        http::{Request as HttpRequest, StatusCode},
        Router,
    };
    use serde_json::{json, Value};
    use spark_connect_core::{ChannelBuilder, SparkConnectClient};
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::{Request, Response, Status};
    use tower::ServiceExt;

    #[derive(Default)]
    struct Proxy {
        seen: Arc<Mutex<Vec<(String, String)>>>,
        owners: Arc<Mutex<HashMap<String, String>>>,
        attempts: Arc<AtomicUsize>,
    }

    impl Proxy {
        fn record<T>(&self, request: &Request<T>, user: &str, session: &str) -> Result<(), Status> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            check(request, user, session)?;
            let mut owners = self.owners.lock().unwrap();
            if owners.get(session).is_some_and(|owner| owner != user) {
                return Err(Status::permission_denied("session belongs to another user"));
            }
            owners.insert(session.to_owned(), user.to_owned());
            self.seen
                .lock()
                .unwrap()
                .push((user.to_owned(), session.to_owned()));
            Ok(())
        }
    }

    fn check<T>(request: &Request<T>, user: &str, session: &str) -> Result<(), Status> {
        let auth = request
            .metadata()
            .get("authorization")
            .and_then(|value| value.to_str().ok());
        if auth != Some(format!("Bearer {user}-ok").as_str()) {
            return Err(Status::unauthenticated("proxy credential rejected"));
        }
        if session.is_empty() {
            return Err(Status::invalid_argument("missing session"));
        }
        Ok(())
    }

    struct NoNotebooks;

    #[async_trait::async_trait]
    impl core::NotebookStore for NoNotebooks {
        async fn get(&self, _: &str) -> core::Result<core::Notebook> {
            unreachable!()
        }
        async fn list(&self, _: &str) -> core::Result<Vec<String>> {
            unreachable!()
        }
        async fn save(&self, _: &core::Notebook, _: &str) -> core::Result<String> {
            unreachable!()
        }
    }

    struct RegisteredCatalog(core::CatalogId);

    #[async_trait::async_trait]
    impl core::Catalog for RegisteredCatalog {
        fn id(&self) -> &core::CatalogId {
            &self.0
        }
        fn kind(&self) -> &str {
            "polaris"
        }
        async fn health(&self) -> core::Health {
            core::Health::Healthy
        }
        async fn list_namespaces(&self) -> core::Result<Vec<core::Namespace>> {
            unreachable!()
        }
        async fn list_tables(&self, _: &str) -> core::Result<Vec<core::TableRef>> {
            unreachable!()
        }
        async fn table_schema(&self, _: &core::TableRef) -> core::Result<core::TableSchema> {
            unreachable!()
        }
    }

    async fn route_query(app: &Router, sid: &str, connect: bool) -> (StatusCode, Value) {
        let path = if connect {
            "/aster.v1.Aster/RunQuery"
        } else {
            "/api/query"
        };
        let mut request = HttpRequest::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .header("cookie", format!("aster_session={sid}"));
        if connect {
            request = request.header("connect-protocol-version", "1");
        }
        let body = if connect {
            json!({"engine":"spark-protected", "catalogContext":"polaris", "sql":"SELECT 1"})
        } else {
            json!({"engine":"spark-protected", "catalog_context":"polaris", "sql":"SELECT 1"})
        };
        let response = app
            .clone()
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tonic::async_trait]
    impl SparkConnectService for Proxy {
        type ExecutePlanStream = tokio_stream::Empty<Result<ExecutePlanResponse, Status>>;
        async fn execute_plan(
            &self,
            request: Request<ExecutePlanRequest>,
        ) -> Result<Response<Self::ExecutePlanStream>, Status> {
            let user = request
                .get_ref()
                .user_context
                .as_ref()
                .map(|context| context.user_id.as_str())
                .unwrap_or("");
            self.record(&request, user, &request.get_ref().session_id)?;
            Err(Status::unimplemented("no row fixture; credential alice-ok"))
        }
        async fn analyze_plan(
            &self,
            request: Request<AnalyzePlanRequest>,
        ) -> Result<Response<AnalyzePlanResponse>, Status> {
            let user = request
                .get_ref()
                .user_context
                .as_ref()
                .map(|context| context.user_id.as_str())
                .unwrap_or("");
            self.record(&request, user, &request.get_ref().session_id)?;
            Err(Status::unimplemented(
                "no analysis fixture; credential alice-ok",
            ))
        }
        async fn config(
            &self,
            _: Request<ConfigRequest>,
        ) -> Result<Response<ConfigResponse>, Status> {
            Err(Status::unimplemented("no config fixture"))
        }
        async fn add_artifacts(
            &self,
            _: Request<tonic::Streaming<AddArtifactsRequest>>,
        ) -> Result<Response<AddArtifactsResponse>, Status> {
            Err(Status::unimplemented("no artifact fixture"))
        }
        async fn artifact_status(
            &self,
            _: Request<ArtifactStatusesRequest>,
        ) -> Result<Response<ArtifactStatusesResponse>, Status> {
            Err(Status::unimplemented("no artifact fixture"))
        }
        async fn interrupt(
            &self,
            _: Request<InterruptRequest>,
        ) -> Result<Response<InterruptResponse>, Status> {
            Err(Status::unimplemented("no interrupt fixture"))
        }
        type ReattachExecuteStream = tokio_stream::Empty<Result<ExecutePlanResponse, Status>>;
        async fn reattach_execute(
            &self,
            _: Request<ReattachExecuteRequest>,
        ) -> Result<Response<Self::ReattachExecuteStream>, Status> {
            Err(Status::unimplemented("no reattach fixture"))
        }
        async fn release_execute(
            &self,
            _: Request<ReleaseExecuteRequest>,
        ) -> Result<Response<ReleaseExecuteResponse>, Status> {
            Err(Status::unimplemented("no release fixture"))
        }
        async fn release_session(
            &self,
            request: Request<ReleaseSessionRequest>,
        ) -> Result<Response<ReleaseSessionResponse>, Status> {
            let user = request
                .get_ref()
                .user_context
                .as_ref()
                .map(|context| context.user_id.as_str())
                .unwrap_or("");
            let session = request.get_ref().session_id.as_str();
            self.record(&request, user, session)?;
            Ok(Response::new(ReleaseSessionResponse::default()))
        }
        async fn fetch_error_details(
            &self,
            _: Request<FetchErrorDetailsRequest>,
        ) -> Result<Response<FetchErrorDetailsResponse>, Status> {
            Err(Status::unimplemented("no error fixture"))
        }
        async fn clone_session(
            &self,
            _: Request<CloneSessionRequest>,
        ) -> Result<Response<CloneSessionResponse>, Status> {
            Err(Status::unimplemented("no clone fixture"))
        }
        async fn get_status(
            &self,
            _: Request<GetStatusRequest>,
        ) -> Result<Response<GetStatusResponse>, Status> {
            Err(Status::unimplemented("no status fixture"))
        }
    }

    #[tokio::test]
    async fn real_client_reaches_authenticated_fake_proxy() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let proxy = Proxy::default();
        let seen = Arc::clone(&proxy.seen);
        let attempts = Arc::clone(&proxy.attempts);
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(SparkConnectServiceServer::new(proxy))
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        });
        let builder =
            ChannelBuilder::parse(&format!("sc://{address}/;user_id=alice;token=alice-ok"))
                .unwrap();
        let client = SparkConnectClient::connect(&builder).await.unwrap();
        client
            .release_session()
            .await
            .expect("healthy authenticated gRPC control");
        assert_eq!(seen.lock().unwrap().len(), 1);
        let alice_session = seen.lock().unwrap()[0].1.clone();
        let builder = ChannelBuilder::parse(&format!(
            "sc://{address}/;user_id=bob;token=bob-ok;session_id={alice_session}"
        ))
        .unwrap();
        let client = SparkConnectClient::connect(&builder).await.unwrap();
        assert!(
            client.release_session().await.is_err(),
            "Bob resumed Alice's proxy session with a Bob credential"
        );
        assert_eq!(seen.lock().unwrap().len(), 1);
        for token in ["forged", "bob-ok"] {
            let builder =
                ChannelBuilder::parse(&format!("sc://{address}/;user_id=alice;token={token}"))
                    .unwrap();
            let client = SparkConnectClient::connect(&builder).await.unwrap();
            assert!(
                client.release_session().await.is_err(),
                "proxy accepted a forged or Bob-bound credential for Alice"
            );
        }
        assert_eq!(seen.lock().unwrap().len(), 1);
        println!("V3B2A_WIRE_CONTROL_READY");
        let engine = SparkEngine::new_authenticated(
            "spark-protected",
            format!("http://{address}"),
            None,
            Arc::new(|subject| Ok(format!("{subject}-ok"))),
        )
        .unwrap();
        let first_query = engine
            .execute_as_verified(
                QueryRequest {
                    sql: "SELECT 1".into(),
                    catalog: None,
                    schema: None,
                    max_rows: Some(1),
                },
                "alice",
            )
            .await;
        assert!(
            !format!("{first_query:?}").contains("alice-ok"),
            "protected Spark surfaced proxy credential in query error"
        );
        let observed = seen.lock().unwrap().clone();
        assert!(
            observed.len() > 1,
            "protected Spark must hand verified Alice to the authenticated proxy"
        );
        assert!(observed[1..]
            .iter()
            .all(|(user, session)| user == "alice" && session != &observed[0].1));
        let count = observed.len();
        drop(observed);
        let delimiter_subject = "alice;user_id=bob;token=forged";
        let _ = engine
            .execute_as_verified(
                QueryRequest {
                    sql: "SELECT 1".into(),
                    catalog: None,
                    schema: None,
                    max_rows: Some(1),
                },
                delimiter_subject,
            )
            .await;
        let observed = seen.lock().unwrap().clone();
        assert!(
            observed.len() > count,
            "encoded subject never reached the proxy"
        );
        assert!(
            observed[count..]
                .iter()
                .all(|(user, _)| user == delimiter_subject),
            "a delimiter-bearing subject changed the proxy user identity"
        );
        let count = observed.len();
        drop(observed);
        std::env::set_var("SPARK_CONNECT_AUTHENTICATE_TOKEN", "ambient-shared");
        let blocked = engine
            .execute_as_verified(
                QueryRequest {
                    sql: "SELECT 1".into(),
                    catalog: None,
                    schema: None,
                    max_rows: Some(1),
                },
                "alice",
            )
            .await;
        std::env::remove_var("SPARK_CONNECT_AUTHENTICATE_TOKEN");
        assert!(
            blocked.is_err(),
            "ambient Spark token must not authorize a protected query"
        );
        assert_eq!(seen.lock().unwrap().len(), count);
        let broken_resolver = SparkEngine::new_authenticated(
            "spark-resolver-error",
            format!("http://{address}"),
            None,
            Arc::new(|_| Err(CoreError::Unauthorized("resolver secret-value".into()))),
        )
        .unwrap();
        let resolver_error = broken_resolver
            .execute_as_verified(
                QueryRequest {
                    sql: "SELECT 1".into(),
                    catalog: None,
                    schema: None,
                    max_rows: Some(1),
                },
                "alice",
            )
            .await;
        assert!(
            !format!("{resolver_error:?}").contains("secret-value"),
            "protected Spark surfaced a credential resolver detail"
        );
        assert_eq!(
            seen.lock().unwrap().len(),
            count,
            "failed credential resolution reached the proxy"
        );
        let before = attempts.load(Ordering::SeqCst);
        let ordinary = engine
            .execute(QueryRequest {
                sql: "SELECT 1".into(),
                catalog: None,
                schema: None,
                max_rows: Some(1),
            })
            .await;
        assert!(
            matches!(ordinary, Err(CoreError::Unauthorized(_))),
            "protected Spark instance must refuse ordinary shared execute"
        );
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            before,
            "ordinary execute attempted a Spark RPC on protected instance"
        );

        let mut engines = core::EngineRegistry::new();
        engines.register(Arc::new(engine));
        let mut catalogs = core::CatalogRegistry::new();
        catalogs.register(Arc::new(RegisteredCatalog(core::CatalogId::new("polaris"))));
        let grants = Arc::new(core::InMemoryGrants::new());
        for subject in ["alice", "bob"] {
            grants.grant(subject, "spark-protected");
        }
        let sessions = Arc::new(core::InMemorySessions::new(3600));
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let mut sids = Vec::new();
        for subject in ["alice", "bob"] {
            sids.push(
                sessions
                    .create_verified(
                        &core::Identity {
                            subject: subject.into(),
                            roles: vec![core::Role::Editor],
                            groups: vec![],
                            user_uuid: None,
                        },
                        None,
                        now,
                    )
                    .await
                    .unwrap()
                    .sid,
            );
        }
        let expired_sid = sessions
            .create_verified(
                &core::Identity {
                    subject: "alice".into(),
                    roles: vec![core::Role::Editor],
                    groups: vec![],
                    user_uuid: None,
                },
                None,
                now - 7200,
            )
            .await
            .unwrap()
            .sid;
        let state = AppState {
            config: core::AppConfig {
                bind: String::new(),
                engines: vec![core::EngineConfig {
                    id: "spark-protected".into(),
                    kind: "spark".into(),
                    endpoint: format!("http://{address}"),
                    routing_group: None,
                    delegation: None,
                }],
                catalogs: vec![core::CatalogConfig {
                    id: "polaris".into(),
                    kind: "polaris".into(),
                    endpoint: "fixture".into(),
                    catalog: Some("fixture".into()),
                    token: None,
                    credential: None,
                }],
                catalog_bindings: vec![core::CatalogBindingConfig {
                    catalog: "polaris".into(),
                    engine: "spark-protected".into(),
                    native_catalog: "polaris".into(),
                    policy: core::BindingPolicy::Protected,
                }],
                default_engine: Some("spark-protected".into()),
                default_catalog: Some("polaris".into()),
            },
            engines,
            catalogs,
            grants,
            audit: Arc::new(core::InMemoryAudit::new()),
            notebooks: Arc::new(NoNotebooks),
            notebook_owners: Arc::new(core::InMemoryNotebookOwners::default()),
            notebook_write: Arc::new(tokio::sync::Mutex::new(())),
            team_workspaces: None,
            team_git_targets: None,
            llm: Arc::new(core::InMemoryLlm::new()),
            shared_models: None,
            current_identity: None,
            shared_model_use_enabled: false,
            conversations: Arc::new(core::InMemoryConversations::default()),
            contracts: Arc::new(vec![]),
            http: reqwest::Client::new(),
            sessions,
            handshakes: Arc::new(core::InMemoryHandshakes::new(300)),
            session_ttl_seconds: 3600,
            user_state: Arc::new(core::InMemoryUserState::new()),
            secrets: Arc::new(core::InMemorySecrets::new()),
            identity: None,
            dev_login: true,
            metrics: Arc::new(Metrics::new()),
        };
        state.config.validate_catalog_bindings().unwrap();
        let app = app(Arc::new(state));
        let mut route_sessions = Vec::new();
        for (sid, subject) in sids.iter().zip(["alice", "bob"]) {
            for connect in [false, true] {
                let baseline = seen.lock().unwrap().len();
                let (status, _body) = route_query(&app, sid, connect).await;
                assert_ne!(
                    status,
                    StatusCode::OK,
                    "the transport-only fixture has no Spark rows"
                );
                let observed = seen.lock().unwrap().clone();
                assert!(
                    observed.len() > baseline,
                    "route did not reach authenticated proxy"
                );
                assert!(
                    observed[baseline..].iter().all(|(user, _)| user == subject),
                    "REST or Connect changed the verified subject at the proxy"
                );
                route_sessions.push(observed[baseline].1.clone());
            }
        }
        assert_eq!(
            route_sessions.iter().collect::<HashSet<_>>().len(),
            4,
            "REST and Connect reused a Spark session across calls or subjects"
        );
        let baseline = seen.lock().unwrap().len();
        let (status, _) = route_query(&app, &expired_sid, false).await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(
            seen.lock().unwrap().len(),
            baseline,
            "expired session reached the protected Spark proxy"
        );
        println!("V3B2A_ROUTE_HANDOFF_READY");
        let _ = stop.send(());
        server.await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn pinned_client_reads_from_live_spark_413() {
        let endpoint = std::env::var("V3B2B_SPARK_ENDPOINT").expect("disposable Spark endpoint");
        let engine = SparkEngine::new("spark-live-control", endpoint, None);
        let result = engine
            .execute(QueryRequest {
                sql: "SELECT 1 AS value".into(),
                catalog: None,
                schema: None,
                max_rows: Some(2),
            })
            .await
            .expect("Spark 4.2 client must read exact row from Spark 4.1.3 server");
        assert_eq!(result.rows, vec![vec![serde_json::json!(1)]]);
        println!("V3B2B_SPARK_413_CLIENT_420_CONTROL_READY");
    }

    #[tokio::test]
    #[ignore]
    async fn pinned_spark_413_reads_delta_410_location() {
        let endpoint = std::env::var("V3B2B_SPARK_ENDPOINT").expect("disposable Spark endpoint");
        let engine = SparkEngine::new("spark-delta-control", endpoint, None);
        for sql in [
            "CREATE TABLE default.asterv3b2_delta_control (id BIGINT, name STRING) USING DELTA",
            "INSERT INTO default.asterv3b2_delta_control VALUES (1, 'alpha'), (2, 'beta')",
        ] {
            engine
                .execute(QueryRequest {
                    sql: sql.into(),
                    catalog: None,
                    schema: None,
                    max_rows: Some(2),
                })
                .await
                .expect("Spark 4.1.3 Delta 4.1 setup query");
        }
        let result = engine
            .execute(QueryRequest {
                sql: "SELECT id, name FROM default.asterv3b2_delta_control ORDER BY id".into(),
                catalog: None,
                schema: None,
                max_rows: Some(3),
            })
            .await
            .expect("Spark 4.1.3 Delta 4.1 exact row read");
        assert_eq!(
            result.rows,
            vec![
                vec![json!(1), json!("alpha")],
                vec![json!(2), json!("beta")]
            ]
        );
        println!("V3B2B_DELTA_410_ROWS_READY");
    }

    #[tokio::test]
    #[ignore]
    async fn pinned_spark_reads_catalog_registered_delta_with_alice_key() {
        let endpoint = std::env::var("V3B2B_SPARK_ENDPOINT").expect("Alice Connect endpoint");
        let polaris = std::env::var("V3B2B_POLARIS_ENDPOINT").expect("Polaris endpoint");
        let secret = std::env::var("V3B2B_POLARIS_ROOT_SECRET").expect("synthetic Polaris secret");
        let bucket = std::env::var("V3B2B_BUCKET").expect("fixture bucket");
        let catalog = aster_catalogs::PolarisCatalog::new("polaris", polaris, "v3b2b_catalog")
            .with_credential(Some(format!("root:{secret}")))
            .with_generic_tables(true);
        let descriptors = catalog.list_table_descriptors("sales").await.unwrap();
        assert_eq!(descriptors.len(), 1);
        let table = &descriptors[0];
        assert_eq!(table.table.name, "orders_delta");
        assert_eq!(table.format.as_deref(), Some("delta"));
        assert!(!table.schema_available);
        let expected_location = format!("s3://{bucket}/sales/orders_delta");
        assert_eq!(
            table.base_location.as_deref(),
            Some(expected_location.as_str())
        );
        let engine = SparkEngine::new("spark-alice", endpoint, None);
        let spark_location = table
            .base_location
            .as_deref()
            .unwrap()
            .replacen("s3://", "s3a://", 1);
        let sql = format!("SELECT id, name FROM delta.`{spark_location}` ORDER BY id");
        let result = engine
            .execute(QueryRequest {
                sql,
                catalog: None,
                schema: None,
                max_rows: Some(3),
            })
            .await
            .expect("Alice scoped S3 key reads catalog-returned Delta location");
        assert_eq!(
            result.rows,
            vec![
                vec![json!(1), json!("alpha")],
                vec![json!(2), json!("beta")]
            ]
        );
        println!("V3B2B_GENERIC_ALICE_ROWS_READY");
    }

    #[tokio::test]
    #[ignore]
    async fn pinned_spark_bob_cannot_read_alice_catalog_delta() {
        let endpoint = std::env::var("V3B2B_BOB_SPARK_ENDPOINT").expect("Bob Connect endpoint");
        let polaris = std::env::var("V3B2B_POLARIS_ENDPOINT").expect("Polaris endpoint");
        let secret = std::env::var("V3B2B_POLARIS_ROOT_SECRET").expect("synthetic Polaris secret");
        let bucket = std::env::var("V3B2B_BUCKET").expect("fixture bucket");
        let catalog = aster_catalogs::PolarisCatalog::new("polaris", polaris, "v3b2b_catalog")
            .with_credential(Some(format!("root:{secret}")))
            .with_generic_tables(true);
        let descriptors = catalog.list_table_descriptors("sales").await.unwrap();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(
            descriptors[0].base_location.as_deref(),
            Some(format!("s3://{bucket}/sales/orders_delta").as_str())
        );
        let engine = SparkEngine::new("spark-bob", endpoint, None);
        let spark_location = descriptors[0]
            .base_location
            .as_deref()
            .unwrap()
            .replacen("s3://", "s3a://", 1);
        let sql = format!("SELECT id, name FROM delta.`{spark_location}` ORDER BY id");
        let result = engine
            .execute(QueryRequest {
                sql,
                catalog: None,
                schema: None,
                max_rows: Some(3),
            })
            .await;
        let error = result.expect_err("Bob read Alice Delta rows under a shared S3 credential");
        assert!(
            format!("{error:?}").contains("AccessDenied") || format!("{error:?}").contains("403"),
            "Bob query failed for a reason other than scoped S3 policy"
        );
        println!("V3B2B_BOB_STORAGE_DENIED");
    }
}
