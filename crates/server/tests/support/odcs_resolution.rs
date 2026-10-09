use aster_core::*;
use aster_server::{app, AppState, Metrics, TeamPolicy, TeamWorkspaces};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const GROUP: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const USER: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

struct Authority(Mutex<bool>, std::sync::atomic::AtomicBool, Mutex<Vec<bool>>);
#[async_trait::async_trait]
impl CurrentIdentityProvider for Authority {
    async fn current(&self, _: &str) -> Result<CurrentIdentity> {
        let mut next = self.0.lock().unwrap();
        let member = *next;
        if self.1.load(std::sync::atomic::Ordering::SeqCst) {
            *next = !member;
        }
        self.2.lock().unwrap().push(member);
        Ok(CurrentIdentity {
            user_uuid: USER.into(),
            active: true,
            roles: vec![Role::Viewer],
            groups: if member { vec![GROUP.into()] } else { vec![] },
        })
    }
    async fn group_exists(&self, group: &str) -> Result<bool> {
        Ok(group == GROUP)
    }
}
struct NoNotebooks;
struct NoExecution(EngineInfo);
#[async_trait::async_trait]
impl QueryEngine for NoExecution {
    fn info(&self) -> &EngineInfo {
        &self.0
    }
    async fn health(&self) -> Health {
        panic!("preview must not probe an engine")
    }
    async fn execute(&self, _: QueryRequest) -> Result<QueryResult> {
        panic!("preview must never execute SQL")
    }
    async fn execute_as_verified(&self, _: QueryRequest, _: &str) -> Result<QueryResult> {
        panic!("preview must never execute delegated SQL")
    }
}

fn files(root: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut result = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(files(&path));
        } else {
            result.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    result
}
#[async_trait::async_trait]
impl NotebookStore for NoNotebooks {
    async fn get(&self, _: &str) -> Result<Notebook> {
        panic!("unexpected notebook read")
    }
    async fn save(&self, _: &Notebook, _: &str) -> Result<String> {
        panic!("unexpected write")
    }
    async fn list(&self, _: &str) -> Result<Vec<String>> {
        panic!("unexpected notebook list")
    }
}

pub async fn whole_contract_team_access_is_default_deny() {
    checks(false).await;
}

pub async fn checks(prepare: bool) {
    fixture_checks(prepare, false).await;
}
pub async fn bound_checks() {
    fixture_checks(true, true).await;
}
async fn fixture_checks(prepare: bool, bound: bool) {
    run_checks(prepare, bound, false).await;
}
pub async fn observation_checks() {
    run_checks(true, true, true).await;
}
pub async fn unavailable_checks() {
    run_case(true, true, true, true, false, false).await;
}
#[allow(dead_code)]
pub async fn ui_checks() {
    run_case(true, true, false, false, true, false).await;
}
#[allow(dead_code)]
pub async fn full_document_preview_preserves_all_content() {
    // No catalog, denied observation, and available observation all return the same source.
    for (observed, unavailable) in [(false, true), (false, false), (true, false)] {
        run_case(false, true, observed, unavailable, false, true).await;
    }
}
struct ObservedCatalog(CatalogId, Arc<std::sync::atomic::AtomicUsize>);
struct InventoryCatalog(
    CatalogId,
    Arc<std::sync::atomic::AtomicUsize>,
    Arc<std::sync::atomic::AtomicUsize>,
    Arc<std::sync::atomic::AtomicUsize>,
);
#[async_trait::async_trait]
impl Catalog for InventoryCatalog {
    fn authoritative_inventory(&self) -> bool {
        self.1.load(std::sync::atomic::Ordering::SeqCst) != 3
    }
    fn id(&self) -> &CatalogId {
        &self.0
    }
    fn kind(&self) -> &str {
        "fixture"
    }
    async fn health(&self) -> Health {
        panic!("inventory does not probe health")
    }
    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        panic!("no global scan")
    }
    async fn list_tables(&self, _: &str) -> Result<Vec<TableRef>> {
        panic!("exact segments required")
    }
    async fn list_descriptors_qualified(
        &self,
        segments: &[String],
    ) -> Result<Vec<TableDescriptor>> {
        self.2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(segments == ["sales.eu"] || segments == ["sales.us"]);
        match self.1.load(std::sync::atomic::Ordering::SeqCst) {
            1 => return Ok(vec![]),
            2 => return Err(CoreError::Catalog("PRIVATE_UPSTREAM_ERROR".into())),
            _ => {}
        }
        Ok(["orders", "uncovered"]
            .into_iter()
            .map(|name| TableDescriptor {
                table: TableRef {
                    namespace: segments.join("."),
                    namespace_segments: segments.to_vec(),
                    name: name.into(),
                },
                format: None,
                base_location: None,
                schema_available: true,
            })
            .collect())
    }
    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        self.3.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(table.namespace_segments, ["sales.eu"]);
        assert!(matches!(table.name.as_str(), "orders" | "uncovered"));
        Ok(TableSchema {
            table: table.clone(),
            columns: vec![ColumnSchema {
                name: format!("DETAIL_{}_COLUMN", table.name.to_uppercase()),
                data_type: "varchar".into(),
                nullable: false,
            }],
        })
    }
}
#[async_trait::async_trait]
impl Catalog for ObservedCatalog {
    fn id(&self) -> &CatalogId {
        &self.0
    }
    fn kind(&self) -> &str {
        "fixture"
    }
    async fn health(&self) -> Health {
        panic!("no health scan")
    }
    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        panic!("no catalog scan")
    }
    async fn list_tables(&self, _: &str) -> Result<Vec<TableRef>> {
        panic!("no table scan")
    }
    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(self.0 .0, "warehouse");
        assert_eq!(table.namespace_segments, ["sales.eu"]);
        assert_eq!(table.name, "orders");
        Ok(TableSchema {
            table: table.clone(),
            columns: vec![
                ColumnSchema {
                    name: "net_amount".into(),
                    data_type: "decimal(12,2)".into(),
                    nullable: true,
                },
                ColumnSchema {
                    name: "observed_only".into(),
                    data_type: "text".into(),
                    nullable: false,
                },
            ],
        })
    }
}
async fn run_checks(prepare: bool, bound: bool, observed: bool) {
    run_case(prepare, bound, observed, false, false, false).await;
}
async fn run_case(
    prepare: bool,
    bound: bool,
    observed: bool,
    unavailable: bool,
    ui: bool,
    preview: bool,
) {
    run_case_inner(prepare, bound, observed, unavailable, ui, preview, None).await;
}

#[allow(dead_code)]
pub async fn physical_inventory_admission() {
    run_case_inner(false, true, true, false, false, false, Some("base")).await;
}

#[allow(dead_code)]
pub async fn mock_table_grants_do_not_disclose_siblings() {
    run_case_inner(false, true, true, false, false, false, Some("mock-table")).await;
}

#[allow(dead_code)]
pub async fn inventory_unknown_and_unauthorized_are_indistinguishable() {
    run_case_inner(false, true, true, false, false, false, Some("existence")).await;
}

#[allow(dead_code)]
pub async fn inventory_uses_one_current_identity_snapshot() {
    run_case_inner(false, true, true, false, false, false, Some("identity")).await;
}

#[allow(dead_code)]
pub async fn inventory_nested_semantic_annotations_are_detected() {
    for shape in ["object", "array", "map-key", "map-value"] {
        for annotation in ["none", "semanticType", "transformLogic"] {
            let case = format!("nested-{shape}:{annotation}");
            run_case_inner(false, true, true, false, false, false, Some(&case)).await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_case_inner(
    prepare: bool,
    bound: bool,
    observed: bool,
    unavailable: bool,
    ui: bool,
    preview: bool,
    inventory: Option<&str>,
) {
    let review = inventory.unwrap_or("");
    let inventory = inventory.is_some();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("compiled")).unwrap();
    let mut document = json!({"apiVersion":"v3.2.0","kind":"DataContract","id":"sales","version":"1",
        "description":{"purpose":"TEAM_ONLY_MEANING"},"schema":[{"name":"orders","description":"ONE_ROW_PER_ORDER CUSTOMER_REFERENCE_ONLY",
        "properties":[
            {"name":"net","physicalName":"net_amount","description":"NET_AFTER_REFUNDS NET_FIELD_DEFINITION","logicalType":"number","physicalType":"bigint","required":true,"semanticType":"measure","transformLogic":"gross - refunds"},
            {"name":"expected_code","physicalName":"expected_code","logicalType":"string","required":true},
            {"name":"business_label","logicalType":"string","description":"DECLARED_WITHOUT_PHYSICAL_MAPPING"}
        ]},{"name":"refunds"}]});
    if ui {
        document["schema"][0]["description"] = json!("ONE_ROW_PER_ORDER CUSTOMER_REFERENCE_ONLY <script>window.contractInjected=true</script>");
        let mut gross = document["schema"][0].clone();
        gross["name"] = json!("gross_orders");
        gross["properties"][0]["description"] =
            json!("GROSS_BEFORE_REFUNDS GROSS_FIELD_DEFINITION");
        gross["properties"][0]["transformLogic"] = json!("gross");
        gross["properties"][0]["physicalName"] = json!("gross_amount");
        document["schema"].as_array_mut().unwrap().push(gross);
        document["schema"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"ambiguous_orders"}));
    }
    if preview {
        document["version"] = json!("1.4.0");
        document["description"]["limitations"] =
            json!("Original declaration, not observed truth — café");
        document["schema"][0]["id"] = json!("orders-id");
        document["schema"][1]["id"] = json!("refunds-id");
        document["schema"][0]["properties"].as_array_mut().unwrap().push(json!({
            "name":"details.with-punctuation","logicalType":"object","properties":[
                {"name":"code","logicalType":"string","quality":[{"type":"text","description":"Keep leading zeros"}]}]}));
        document["schema"][0]["quality"] =
            json!([{"type":"text","description":"One row per order"}]);
        document["servers"] = json!([
            {"server":"declared-primary","type":"custom","description":"Not a live binding"},
            {"server":"declared-secondary","type":"custom","environment":"dev"}]);
        document["team"] = json!({"name":"Declared owner, not an Aster grant","members":[{"username":"owner@example.invalid","role":"owner"}]});
        document["support"] = json!([{"channel":"help","tool":"email"}]);
        document["roles"] = json!([{"role":"declared-reader","access":"read"}]);
        document["price"] = json!({"priceAmount":12.5,"priceCurrency":"EUR","priceUnit":"month"});
        document["slaProperties"] = json!([{"property":"latency","value":24,"unit":"h"}]);
        document["customProperties"] =
            json!([{"property":"opaque","value":{"keep":[true,7,"untouched"]}}]);
    }
    if inventory {
        document["team"] = json!({"name":"alpha","members":[{"username":"alice","role":"owner"}]});
    }
    if let Some(case) = review.strip_prefix("nested-") {
        let (shape, annotation) = case.split_once(':').unwrap();
        let mut leaf = json!({"name":"amount","logicalType":"number"});
        match annotation {
            "semanticType" => leaf["semanticType"] = json!("measure"),
            "transformLogic" => leaf["transformLogic"] = json!("gross - refunds"),
            "none" => {}
            _ => panic!("unknown review fixture"),
        }
        let nested = match shape {
            "object" => json!({"name":"nested","logicalType":"object","properties":[leaf]}),
            "array" => json!({"name":"nested","logicalType":"array","items":leaf}),
            "map-key" => {
                json!({"name":"nested","logicalType":"map","map":{"key":leaf,"value":{"logicalType":"number"}}})
            }
            "map-value" => {
                json!({"name":"nested","logicalType":"map","map":{"key":{"logicalType":"string"},"value":leaf}})
            }
            _ => panic!("unknown nested shape"),
        };
        document["schema"][0]["properties"] = json!([
            {"name":"details","logicalType":"object","properties":[nested]}
        ]);
        // Annotation-shaped arbitrary custom data is not a supported property.
        document["schema"][0]["customProperties"] = json!([
            {"property":"opaque","value":{"semanticType":"measure"}}
        ]);
    }
    let expected = document.clone();
    let document = if preview {
        format!(
            "# Original source: preserve comments and CRLF\n{}\n# end\n",
            serde_json::to_string_pretty(&document).unwrap()
        )
        .replace('\n', "\r\n")
    } else {
        document.to_string()
    };
    let digest = aster_server::odcs_intake::sha256(document.as_bytes());
    std::fs::write(dir.path().join("compiled/sales.json"), &document).unwrap();
    let forbidden = json!({"apiVersion":"v3.2.0","kind":"DataContract","id":"private","version":"1","description":{"purpose":"DENIED_QUERY_MEANING"}}).to_string();
    let forbidden_digest = aster_server::odcs_intake::sha256(forbidden.as_bytes());
    std::fs::write(dir.path().join("compiled/private.json"), forbidden).unwrap();
    let mut manifest = json!({"manifestVersion":1,"bundle":"fixture","version":"1","documents":[
        {"path":"compiled/sales.json","sha256":digest,"id":"sales","version":expected["version"],"target":"warehouse"},
        {"path":"compiled/private.json","sha256":forbidden_digest,"id":"private","version":"1","target":"warehouse"}]});
    if bound {
        let mut bindings = json!({"formatVersion":1,"version":"binding-1","bindings":[{"path":"compiled/sales.json","object":"/schema/0","catalog":"warehouse","namespaceSegments":["sales.eu"],"physicalName":"orders"}]});
        if ui {
            bindings["bindings"].as_array_mut().unwrap().push(json!({"path":"compiled/sales.json","object":"/schema/2","catalog":"warehouse","namespaceSegments":["sales.eu"],"physicalName":"gross_orders"}));
        }
        let bindings = bindings.to_string();
        std::fs::write(dir.path().join("bindings.json"), &bindings).unwrap();
        manifest["bindingsSha256"] = json!(aster_server::odcs_intake::sha256(bindings.as_bytes()));
    }
    let manifest = manifest.to_string();
    std::fs::write(dir.path().join("manifest.json"), &manifest).unwrap();
    std::fs::write(
        dir.path().join("grants.json"),
        json!({"formatVersion":1,"version":"1","grants":[
        {"path":"compiled/sales.json","sha256":digest,"teams":["alpha"]}]})
        .to_string(),
    )
    .unwrap();
    let mut bundle = aster_server::odcs_intake::load_bundle(
        dir.path(),
        &aster_server::odcs_intake::sha256(manifest.as_bytes()),
        "1",
    )
    .unwrap();
    if ui {
        // Intake rejects ambiguity; exercise the resolver's defensive state explicitly.
        for catalog in ["warehouse", "other"] {
            bundle.physical_bindings.as_mut().unwrap().bindings.push(
                serde_json::from_value(json!({
                    "path":"compiled/sales.json", "object":"/schema/3", "catalog":catalog,
                    "namespaceSegments":["sales.eu"], "physicalName":"orders"
                }))
                .unwrap(),
            );
        }
    }
    let mock_grants = if review == "mock-table" {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/mock-catalog");
        let manifest = std::fs::read(root.join("manifest.json")).unwrap();
        let manifest_tree: Value = serde_json::from_slice(&manifest).unwrap();
        let pins: Value =
            serde_json::from_slice(&std::fs::read(root.join("fixture-pins.json")).unwrap())
                .unwrap();
        let original = aster_server::odcs_intake::load_bundle(
            &root,
            pins["manifestSha256"].as_str().unwrap(),
            manifest_tree["version"].as_str().unwrap(),
        )
        .unwrap();
        for artifact in &original.documents {
            std::fs::write(
                dir.path().join(&artifact.selection.path),
                &artifact.document.source,
            )
            .unwrap();
        }
        std::fs::write(dir.path().join("manifest.json"), manifest).unwrap();
        std::fs::write(
            dir.path().join("bindings.json"),
            std::fs::read(root.join("bindings.json")).unwrap(),
        )
        .unwrap();
        bundle = aster_server::odcs_intake::load_bundle(
            dir.path(),
            pins["manifestSha256"].as_str().unwrap(),
            &original.version,
        )
        .unwrap();
        Some(
            serde_json::from_slice::<Value>(&std::fs::read(root.join("grants.json")).unwrap())
                .unwrap(),
        )
    } else {
        None
    };
    let authority = Arc::new(Authority(
        Mutex::new(true),
        std::sync::atomic::AtomicBool::new(false),
        Mutex::new(vec![]),
    ));
    let sessions = Arc::new(InMemorySessions::new(3600));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let session = sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Viewer],
                groups: vec![GROUP.into()],
                user_uuid: Some(USER.into()),
            },
            None,
            now,
        )
        .await
        .unwrap();
    let teams = TeamWorkspaces::new(
        dir.path().join("teams"),
        std::collections::HashMap::from([
            (
                if review == "mock-table" {
                    "aster-editors".into()
                } else {
                    "alpha".into()
                },
                TeamPolicy {
                    member_claim: GROUP.into(),
                    maintainer_claim: "maintainer".into(),
                    allowed_repositories: std::collections::HashSet::from(
                        ["test/notebooks".into()],
                    ),
                    local_repositories: Default::default(),
                },
            ),
            (
                "beta".into(),
                TeamPolicy {
                    member_claim: "cccccccc-cccc-4ccc-8ccc-cccccccccccc".into(),
                    maintainer_claim: "other-maintainer".into(),
                    allowed_repositories: std::collections::HashSet::from(["test/other".into()]),
                    local_repositories: Default::default(),
                },
            ),
        ]),
    )
    .unwrap();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let inventory_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let detail_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut catalogs = CatalogRegistry::new();
    if !preview || !unavailable {
        catalogs.register(Arc::new(ObservedCatalog(
            CatalogId::new("other"),
            calls.clone(),
        )));
    }
    if !unavailable {
        catalogs.register(Arc::new(ObservedCatalog(
            CatalogId::new("warehouse"),
            calls.clone(),
        )));
    }
    let mut engines = EngineRegistry::new();
    if inventory {
        catalogs.register(Arc::new(InventoryCatalog(
            CatalogId::new("warehouse"),
            calls.clone(),
            inventory_calls.clone(),
            detail_calls.clone(),
        )));
        std::fs::write(
            dir.path().join("schema-owners.json"),
            json!({
            "formatVersion":1,"version":"owners-1","schemas":[
                {"catalog":"warehouse","namespaceSegments":["sales.eu"],"team":"alpha"}
            ]})
            .to_string(),
        )
        .unwrap();
    }
    if review == "mock-table" {
        catalogs.register(Arc::new(aster_catalogs::MockCatalog::new("mock-local")));
        let schemas = ["aster_demo","aster_demo.sales","aster_raw"].map(|namespace| json!({"catalog":"mock-local","namespaceSegments":[namespace],"team":"beta"}));
        std::fs::write(
            dir.path().join("schema-owners.json"),
            json!({"formatVersion":1,"version":"mock-owner-test","schemas":schemas}).to_string(),
        )
        .unwrap();
    }
    if preview || inventory {
        engines.register(Arc::new(NoExecution(EngineInfo {
            id: EngineId::new("engine"),
            kind: "fixture".into(),
            endpoint: "https://engine.invalid".into(),
            routing_group: None,
        })));
    }
    let router = app(Arc::new(AppState {
        config: AppConfig {
            bind: String::new(),
            engines: vec![],
            catalogs: vec![],
            catalog_bindings: if observed {
                vec![CatalogBindingConfig {
                    catalog: if review == "mock-table" {
                        "mock-local".into()
                    } else {
                        "warehouse".into()
                    },
                    engine: "engine".into(),
                    native_catalog: "warehouse".into(),
                    policy: BindingPolicy::Unprotected,
                }]
            } else {
                vec![]
            },
            default_engine: None,
            default_catalog: None,
        },
        engines,
        catalogs,
        grants: Arc::new(InMemoryGrants::new()),
        audit: Arc::new(InMemoryAudit::new()),
        notebooks: Arc::new(NoNotebooks),
        notebook_owners: Arc::new(InMemoryNotebookOwners::default()),
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: Some(Arc::new(teams)),
        team_git_targets: None,
        llm: Arc::new(InMemoryLlm::new()),
        shared_models: None,
        current_identity: Some(authority.clone()),
        shared_model_use_enabled: false,
        conversations: Arc::new(InMemoryConversations::default()),
        exchanges: Arc::new(InMemoryExchanges::default()),
        contracts: Arc::new(vec![]),
        compiled_contracts: Some(Arc::new(bundle)),
        http: reqwest::Client::new(),
        sessions,
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: true,
        metrics: Arc::new(Metrics::new()),
    }));
    let cookie = format!("aster_session={}", session.sid);
    if review == "mock-table" {
        let grants = mock_grants.unwrap();
        let bindings: Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("bindings.json")).unwrap())
                .unwrap();
        for (allowed, sibling, sibling_column) in [
            ("orders", "customers", "email"),
            ("events", "clickstream", "session_id"),
        ] {
            let binding = |table: &str| {
                bindings["bindings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|b| b["physicalName"] == table)
                    .unwrap()
            };
            let grant = |table: &str| {
                grants["grants"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|g| g["path"] == binding(table)["path"])
                    .unwrap()
                    .clone()
            };
            let selection = |table: &str| json!({"path":grant(table)["path"],"sha256":grant(table)["sha256"],"object":binding(table)["object"]});
            for (visible, hidden) in [(allowed, sibling), (sibling, allowed)] {
                std::fs::write(
                    dir.path().join("grants.json"),
                    json!({"formatVersion":1,"version":"one-table","grants":[grant(visible)]})
                        .to_string(),
                )
                .unwrap();
                let (status, body) =
                    request(&router, "GetContract", &cookie, selection(visible)).await;
                assert_eq!(status, StatusCode::OK);
                let context: Value =
                    serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
                let objects: Vec<_> = context["declared"]["schema"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|o| o["physicalName"].as_str().unwrap())
                    .collect();
                assert_eq!(
                    objects,
                    [visible],
                    "one materialization grant must not disclose its sibling"
                );
                if visible == allowed {
                    assert!(!context["originalSource"]
                        .as_str()
                        .unwrap()
                        .contains(sibling_column));
                } else {
                    assert!(context["originalSource"]
                        .as_str()
                        .unwrap()
                        .contains(sibling_column));
                }
                assert_eq!(
                    request(&router, "GetContract", &cookie, selection(hidden))
                        .await
                        .0,
                    StatusCode::FORBIDDEN
                );
                let (status, body) = request(&router, "ListContracts", &cookie, json!({})).await;
                assert_eq!(status, StatusCode::OK);
                let listed: Value =
                    serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
                assert_eq!(listed["contracts"].as_array().unwrap().len(), 1);
                assert_eq!(listed["contracts"][0]["path"], grant(visible)["path"]);
                let (status, body) =
                    request(&router, "PrepareContractQuery", &cookie, selection(visible)).await;
                assert_eq!(status, StatusCode::OK);
                let prepared: Value =
                    serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
                assert_eq!(prepared["declared"]["physicalName"], visible);
                assert_eq!(prepared["binding"]["physicalName"], visible);
                let scope = json!({"catalog":"mock-local","namespaceSegments":binding(visible)["namespaceSegments"]});
                let (status, body) =
                    request(&router, "ListCatalogInventory", &cookie, scope.clone()).await;
                assert_eq!(status, StatusCode::OK);
                let inventory: Value =
                    serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
                assert_eq!(inventory["entries"].as_array().unwrap().len(), 1);
                assert_eq!(inventory["entries"][0]["physicalName"], visible);
                let (status, body) = request(&router, "ListTables", &cookie, scope).await;
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body["tables"].as_array().unwrap().len(), 1);
                assert_eq!(body["tables"][0]["name"], visible);
            }
        }
        return;
    }
    if review == "existence" {
        let allowed = request(
            &router,
            "ListCatalogInventory",
            &cookie,
            json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
        )
        .await;
        assert_eq!(allowed.0, StatusCode::OK);
        assert!(allowed.1.to_string().contains("orders"));
        assert_eq!(inventory_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        *authority.0.lock().unwrap() = false;
        for unavailable_grants in [false, true] {
            if unavailable_grants {
                std::fs::write(dir.path().join("grants.json"), "{malformed").unwrap();
            }
            for method in ["ListCatalogInventory", "ListTables"] {
                let known = request(
                    &router,
                    method,
                    &cookie,
                    json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
                )
                .await;
                let unknown = request(
                    &router,
                    method,
                    &cookie,
                    json!({"catalog":"warehouse","namespaceSegments":["not-configured"]}),
                )
                .await;
                assert_eq!(
                    known, unknown,
                    "unauthorized schema probes must have identical status and payload on {method}"
                );
                assert!(!known.1.to_string().contains("orders"));
                assert!(!known.1.to_string().contains("sales.eu"));
            }
        }
        assert_eq!(
            inventory_calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "denied probes perform no catalog IO"
        );
        return;
    }
    if review == "identity" {
        authority.1.store(true, std::sync::atomic::Ordering::SeqCst);
        for index in 0..4 {
            let (status, body) = request(
                &router,
                "ListCatalogInventory",
                &cookie,
                json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            let context: Value =
                serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
            let observed = authority.2.lock().unwrap();
            if !observed.last().unwrap() {
                assert_eq!(
                    context["entries"],
                    json!([]),
                    "observed membership revocation must not leave stale owner disclosure"
                );
            } else {
                assert_eq!(context["entries"].as_array().unwrap().len(), 2);
                assert_eq!(
                    context["entries"][0]["contract"]["status"], "admitted",
                    "ownership and grants use the same identity"
                );
            }
            assert_eq!(
                observed.len(),
                index + 1,
                "one fresh identity resolution per inventory request"
            );
            assert_eq!(observed[index], index % 2 == 0);
        }
        assert_eq!(
            inventory_calls.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "only admitted requests reach the catalog"
        );
        std::fs::write(
            dir.path().join("schema-owners.json"),
            json!({"formatVersion":1,"version":"two-scopes","schemas":[
            {"catalog":"warehouse","namespaceSegments":["sales.eu"],"team":"alpha"},
            {"catalog":"warehouse","namespaceSegments":["sales.us"],"team":"alpha"}]})
            .to_string(),
        )
        .unwrap();
        for url in ["/catalog", "/api/catalogs/warehouse/namespaces"] {
            *authority.0.lock().unwrap() = true;
            authority.2.lock().unwrap().clear();
            for member in [true, false] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri(url)
                            .header("cookie", &cookie)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                let body = String::from_utf8(
                    to_bytes(response.into_body(), 1 << 20)
                        .await
                        .unwrap()
                        .to_vec(),
                )
                .unwrap();
                let observed = authority.2.lock().unwrap();
                if !observed.last().unwrap() {
                    assert!(
                        !body.contains("sales.eu") && !body.contains("sales.us"),
                        "multi-schema request retained metadata after observed revocation: {body}"
                    );
                }
                assert_eq!(
                    observed.len(),
                    if member { 1 } else { 2 },
                    "one identity snapshot across the inventory operation"
                );
                assert_eq!(
                    body.contains("sales.eu") && body.contains("sales.us"),
                    member
                );
            }
        }
        return;
    }
    if review.starts_with("nested-") {
        let (status, body) = request(
            &router,
            "ListCatalogInventory",
            &cookie,
            json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let context: Value = serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
        assert_eq!(context["entries"][0]["contract"]["status"], "admitted");
        assert_eq!(
            context["entries"][0]["semantics"]["status"],
            if review.ends_with(":none") {
                "not_declared"
            } else {
                "declared"
            },
            "nested property annotation must remain visible: {review}"
        );
        assert_eq!(context["entries"][0]["queryAuthorization"], "not_evaluated");
        return;
    }
    if inventory {
        let (status, body) = request(
            &router,
            "ListCatalogInventory",
            &cookie,
            json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "owner inventory must be available: {body}"
        );
        let context: Value = serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
        assert_eq!(context["physical"]["status"], "present");
        assert_eq!(context["entries"].as_array().unwrap().len(), 2);
        assert_eq!(context["entries"][0]["physicalName"], "orders");
        assert_eq!(context["entries"][0]["contract"]["status"], "admitted");
        assert_eq!(context["entries"][0]["semantics"]["status"], "declared");
        assert_eq!(context["entries"][1]["physicalName"], "uncovered");
        assert_eq!(context["entries"][1]["contract"]["status"], "not_admitted");
        assert_eq!(context["entries"][1]["semantics"]["status"], "unknown");
        assert_eq!(
            detail_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "listing must not load columns"
        );
        use base64::Engine;
        let token = format!(
            "~{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"["sales.eu"]"#)
        );
        for table in ["orders", "uncovered"] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/catalog/warehouse/{token}/{table}"))
                        .header("cookie", &cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = String::from_utf8(
                to_bytes(response.into_body(), 1 << 20)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap();
            assert!(body.contains(&format!("DETAIL_{}_COLUMN", table.to_uppercase())));
            assert!(body.contains("varchar"));
        }
        assert_eq!(detail_calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        let owners = std::fs::read(dir.path().join("schema-owners.json")).unwrap();
        std::fs::write(
            dir.path().join("schema-owners.json"),
            json!({"formatVersion":1,"version":"owners-2","schemas":[
            {"catalog":"warehouse","namespaceSegments":["sales.eu"],"team":"beta"}]})
            .to_string(),
        )
        .unwrap();
        let (status, body) = request(
            &router,
            "ListCatalogInventory",
            &cookie,
            json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let context: Value = serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
        assert_eq!(
            context["entries"].as_array().unwrap().len(),
            1,
            "nonowner receives only contracted entries"
        );
        assert_eq!(context["entries"][0]["physicalName"], "orders");
        let grants = std::fs::read(dir.path().join("grants.json")).unwrap();
        std::fs::write(
            dir.path().join("grants.json"),
            json!({"formatVersion":1,"version":"revoked","grants":[]}).to_string(),
        )
        .unwrap();
        let (status, body) = request(
            &router,
            "ListCatalogInventory",
            &cookie,
            json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let context: Value = serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
        assert_eq!(
            context["entries"],
            json!([]),
            "ODCS owner does not grant metadata access"
        );
        std::fs::write(dir.path().join("grants.json"), grants).unwrap();
        std::fs::write(dir.path().join("schema-owners.json"), owners).unwrap();
        let (status, _) = request(
            &router,
            "ListCatalogInventory",
            "",
            json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, body) = request(
            &router,
            "ListCatalogInventory",
            &cookie,
            json!({"catalog":"warehouse","namespaceSegments":["sales","eu"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            serde_json::from_str::<Value>(body["contextJson"].as_str().unwrap()).unwrap(),
            json!({"physical":{"status":"denied"},"entries":[]}),
            "literal namespace segments cannot collapse into an admitted schema"
        );
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/catalog")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = String::from_utf8(
            to_bytes(response.into_body(), 1 << 20)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(
            html.contains("Catalog inventory")
                && html.contains("uncovered")
                && html.contains("Semantics")
        );
        *authority.0.lock().unwrap() = false;
        let (status, body) = request(
            &router,
            "ListCatalogInventory",
            &cookie,
            json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(!body.to_string().contains("orders"));
        assert!(!body.to_string().contains("uncovered"));
        let token = format!(
            "~{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"["sales.eu"]"#)
        );
        for url in [
            "/catalog".to_string(),
            format!("/catalog/warehouse/{token}/uncovered"),
            "/api/catalogs/warehouse/namespaces".into(),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(&url)
                        .header("cookie", &cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
            let body = String::from_utf8(bytes.to_vec()).unwrap();
            assert!(
                !body.contains("uncovered"),
                "revoked metadata leaked through {url}: {body}"
            );
            assert!(!body.contains("orders"));
        }
        assert_eq!(
            detail_calls.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "revoked detail performs no schema read"
        );
        let (status, body) = request(
            &router,
            "ListTables",
            &cookie,
            json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            !body.to_string().contains("orders"),
            "legacy RPC must use inventory admission: {body}"
        );
        assert!(!body.to_string().contains("uncovered"));
        *authority.0.lock().unwrap() = true;
        for mode in [1, 2, 3] {
            calls.store(mode, std::sync::atomic::Ordering::SeqCst);
            let (status, body) = request(
                &router,
                "ListCatalogInventory",
                &cookie,
                json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            let context: Value =
                serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
            assert_eq!(
                context["entries"],
                json!([]),
                "bindings are not existence proof"
            );
            assert_eq!(
                context["physical"]["status"],
                if mode == 1 { "present" } else { "unknown" }
            );
            assert!(!body.to_string().contains("PRIVATE_UPSTREAM_ERROR"));
        }
        calls.store(0, std::sync::atomic::Ordering::SeqCst);
        for contents in [
            "{bad".to_string(),
            json!({"formatVersion":1,"version":"2","schemas":[
            {"catalog":"warehouse","namespaceSegments":["sales.eu"],"team":"unknown"}]})
            .to_string(),
        ] {
            std::fs::write(dir.path().join("schema-owners.json"), contents).unwrap();
            let (status, body) = request(
                &router,
                "ListCatalogInventory",
                &cookie,
                json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert!(!body.to_string().contains("orders"));
        }
        std::fs::remove_file(dir.path().join("schema-owners.json")).unwrap();
        let (status, _) = request(
            &router,
            "ListCatalogInventory",
            &cookie,
            json!({"catalog":"warehouse","namespaceSegments":["sales.eu"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        return;
    }
    let before = preview.then(|| files(dir.path()));
    if ui {
        if let Ok(receipt) = std::env::var("ASTER_S7_BROWSER_FIXTURE") {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            std::fs::write(
                receipt,
                json!({"base":format!("http://{}", listener.local_addr().unwrap()),
                "cookie":session.sid,"bundle":dir.path(),"sha256":digest})
                .to_string(),
            )
            .unwrap();
            axum::serve(listener, router).await.unwrap();
            return;
        }
    }
    if ui {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/catalog")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            html.contains("id=\"contract-context\""),
            "primary catalog must offer contract query context"
        );
        assert!(html.contains("Declared contract"));
        assert!(!html.contains("DENIED_QUERY_MEANING"));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
    let (status, body) = request(&router, "ListContracts", &cookie, json!({})).await;
    assert_eq!(status, StatusCode::OK, "contract discovery route: {body}");
    assert!(body.to_string().contains("sales"));
    assert!(!body.to_string().contains("private"));
    let (status, body) = request(
        &router,
        "GetContract",
        &cookie,
        json!({"path":"compiled/private.json","sha256":forbidden_digest}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!body.to_string().contains("DENIED_QUERY_MEANING"));
    let selection = json!({"path":"compiled/sales.json","sha256":digest});
    let (status, _) = request(&router, "GetContract", "", selection.clone()).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "development identity is not contract authority"
    );
    let (status, _) = request(
        &router,
        "GetContract",
        &cookie,
        json!({"path":"compiled/sales.json","sha256":"wrong"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    if prepare {
        let (status, body) = request(
            &router,
            "PrepareContractQuery",
            &cookie,
            json!({"path":"compiled/sales.json","sha256":digest,"object":"/schema/0"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "preparation route: {body}");
        let context: Value = serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
        assert_eq!(
            context["binding"]["status"],
            if bound { "resolved" } else { "missing" }
        );
        if bound {
            assert_eq!(context["binding"]["catalog"], "warehouse");
            assert_eq!(context["binding"]["namespaceSegments"], json!(["sales.eu"]));
            assert_eq!(context["binding"]["physicalName"], "orders");
            assert_eq!(context["binding"]["version"], "binding-1");
        }
        for value in [
            "NET_AFTER_REFUNDS",
            "NET_FIELD_DEFINITION",
            "ONE_ROW_PER_ORDER",
            "CUSTOMER_REFERENCE_ONLY",
            "gross - refunds",
        ] {
            assert!(context.to_string().contains(value), "{context}");
        }
        assert_eq!(
            context["observation"]["status"],
            if unavailable {
                "unavailable"
            } else if observed {
                "observed"
            } else if bound {
                "denied"
            } else {
                "unavailable"
            }
        );
        if observed && !unavailable {
            assert_eq!(
                context["observation"]["schema"]["columns"][1]["name"],
                "observed_only"
            );
            assert!(!context["declared"].to_string().contains("observed_only"));
            assert_eq!(context["observation"]["catalog"], "warehouse");
            assert!(context["observation"]["observedAt"].as_u64().unwrap() > 0);
            assert_eq!(
                context["comparison"]["observedOnly"],
                json!(["observed_only"])
            );
            assert_eq!(context["comparison"]["status"], "separate_facts");
            assert_eq!(
                context["comparison"]["declaredOnly"],
                json!([
                    {"name":"expected_code","physicalName":"expected_code","logicalType":"string","required":true}
                ])
            );
            assert_eq!(
                context["comparison"]["unmappedDeclared"],
                json!([
                    {"name":"business_label","logicalType":"string","description":"DECLARED_WITHOUT_PHYSICAL_MAPPING"}
                ])
            );
            assert_eq!(
                context["comparison"]["matched"],
                json!([{
                    "declared":{"name":"net","physicalName":"net_amount","description":"NET_AFTER_REFUNDS NET_FIELD_DEFINITION",
                        "logicalType":"number","physicalType":"bigint","required":true,"semanticType":"measure","transformLogic":"gross - refunds"},
                    "observed":{"name":"net_amount","type":"decimal(12,2)","nullable":true},
                    "mapping":"declared physicalName"
                }])
            );
        }
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            usize::from(observed && !unavailable)
        );
        assert!(context.get("sql").is_none());
        assert!(!context.to_string().contains("DENIED_QUERY_MEANING"));
        let (status, body) = request(
            &router,
            "PrepareContractQuery",
            &cookie,
            json!({"path":"compiled/sales.json","sha256":digest,"object":"orders"}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body.to_string().contains("TEAM_ONLY_MEANING"));
    }
    let (status, body) = request(&router, "GetContract", &cookie, selection.clone()).await;
    assert_eq!(status, StatusCode::OK);
    if preview {
        let context: Value = serde_json::from_str(body["contextJson"].as_str().unwrap()).unwrap();
        assert_eq!(
            context["declared"], expected,
            "complete original tree, not a selected-object projection"
        );
        // Independent published-schema oracle, not Aster's projection/parser or validation helper.
        let schema_bytes = include_bytes!("../../schemas/odcs-v3.2.0.json");
        assert_eq!(
            aster_server::odcs_intake::sha256(schema_bytes),
            "edb41f33ec46e84780e99872ab2bd67f074959d2bf3e9c9fc54e61f8982b0d93"
        );
        let schema: Value = serde_json::from_slice(schema_bytes).unwrap();
        let validator = jsonschema::draft201909::options().build(&schema).unwrap();
        validator.validate(&context["declared"]).unwrap();
        let mut invalid = context["declared"].clone();
        invalid["schema"][0]["properties"][0]["logicalType"] = json!("bigint");
        assert!(
            !validator.is_valid(&invalid),
            "oracle must reject invalid logical types"
        );
        assert_eq!(
            context["originalSource"].as_str().map(str::as_bytes),
            Some(document.as_bytes()),
            "preview must return original bytes, including comments and formatting"
        );
        let source_tree = aster_core::odcs::OdcsDocument::parse(
            context["originalSource"].as_str().unwrap().as_bytes(),
        )
        .unwrap()
        .raw;
        assert_eq!(source_tree, expected);
        validator.validate(&source_tree).unwrap();
        assert_eq!(
            context["provenance"],
            json!({"bundle":"fixture","version":"1",
            "manifestSha256":aster_server::odcs_intake::sha256(manifest.as_bytes()),
            "path":"compiled/sales.json","sha256":digest,"target":"warehouse","grantsVersion":"1"})
        );
        assert!(context.get("observation").is_none());
        assert!(context.get("sql").is_none());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(
            std::fs::read(dir.path().join("compiled/sales.json")).unwrap(),
            document.as_bytes()
        );
        assert_eq!(
            std::fs::read(dir.path().join("manifest.json")).unwrap(),
            manifest.as_bytes()
        );
        // Object selection cannot turn whole-document permission into a narrower grant.
        let (status, selected) = request(
            &router,
            "GetContract",
            &cookie,
            json!({"path":"compiled/sales.json","sha256":digest,"object":"/schema/1"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(selected, body);
        assert_eq!(
            files(dir.path()),
            before.unwrap(),
            "preview must not write or publish any files"
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
    for value in ["TEAM_ONLY_MEANING", "orders", "refunds"] {
        assert!(body.to_string().contains(value), "{body}");
    }
    *authority.0.lock().unwrap() = false;
    let (status, body) = request(&router, "GetContract", &cookie, selection).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!body.to_string().contains("TEAM_ONLY_MEANING"));
    assert!(!body.to_string().contains("Original source"));
    let (status, body) = request(&router, "ListContracts", &cookie, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.to_string().contains("sales"));
    *authority.0.lock().unwrap() = true;
    std::fs::write(dir.path().join("grants.json"), json!({"formatVersion":1,"version":"2","grants":[{"path":"compiled/sales.json","sha256":digest,"teams":["alpha","unknown"]}]}).to_string()).unwrap();
    let (status, _) = request(
        &router,
        "GetContract",
        &cookie,
        json!({"path":"compiled/sales.json","sha256":digest}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "unknown team configuration must fail closed even beside an allowed team"
    );
    std::fs::write(dir.path().join("grants.json"), "{malformed").unwrap();
    let (status, _) = request(
        &router,
        "GetContract",
        &cookie,
        json!({"path":"compiled/sales.json","sha256":digest}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "malformed replacement fails closed"
    );
    std::fs::write(
        dir.path().join("grants.json"),
        json!({"formatVersion":1,"version":"2","grants":[]}).to_string(),
    )
    .unwrap();
    let (status, _) = request(
        &router,
        "GetContract",
        &cookie,
        json!({"path":"compiled/sales.json","sha256":digest}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "fresh grant revocation overrides cached artifact"
    );
}

async fn request(router: &Router, method: &str, cookie: &str, body: Value) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/aster.v1.Aster/{method}"))
                .header("content-type", "application/json")
                .header("connect-protocol-version", "1")
                .header("cookie", cookie)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
