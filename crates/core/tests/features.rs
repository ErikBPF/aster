//! Behavior contracts for `aster-core`, run as real Gherkin tests.
//!
//! `crates/core/features/*.feature` are the contract; this file binds their
//! steps to the domain API. Run with `cargo test -p aster-core` (declared in
//! Cargo.toml as a `harness = false` test target).

use aster_core::{
    authorize, authorize_engine, relevant, require, Action, Cell, ColumnSchema, CoreError,
    DataContract, EngineId, HandshakeStore, InMemoryGrants, InMemoryHandshakes, InMemorySecrets,
    InMemorySessions, InMemoryUserState, Notebook, Principal, Role, SecretStore, SessionRecord,
    SessionRegistry, TableModel, TableRef, TableSchema, UserState, WorkingState,
};
use cucumber::{given, then, when, World};

#[derive(Debug, Default, World)]
struct App {
    principal: Option<Principal>,
    grants: InMemoryGrants,
    decision: Option<Result<(), CoreError>>,
    notebook: Option<Notebook>,
    text: String,
    sessions: Option<InMemorySessions>,
    handshakes: Option<InMemoryHandshakes>,
    user_state: Option<InMemoryUserState>,
    resumed: Option<WorkingState>,
    sid: String,
    record: Option<SessionRecord>,
    listed: Vec<SessionRecord>,
    redeemed: Option<String>,
    secrets: Option<InMemorySecrets>,
    secret: Option<Result<String, CoreError>>,
    looked_up: Option<Option<String>>,
    contract_text: String,
    contract: Option<Result<DataContract, CoreError>>,
    contracts: Vec<DataContract>,
    selected: Vec<String>,
    table: Option<TableSchema>,
    model: Option<Result<String, CoreError>>,
}

impl App {
    fn sessions(&self) -> &InMemorySessions {
        self.sessions.as_ref().expect("a session store exists")
    }

    fn handshakes(&self) -> &InMemoryHandshakes {
        self.handshakes.as_ref().expect("a handshake store exists")
    }

    fn user_state(&self) -> &InMemoryUserState {
        self.user_state.as_ref().expect("a user state store exists")
    }

    fn secrets(&self) -> &InMemorySecrets {
        self.secrets.as_ref().expect("a secret store exists")
    }
}

fn role(name: &str) -> Role {
    match name {
        "viewer" => Role::Viewer,
        "admin" => Role::Admin,
        _ => Role::Editor,
    }
}

fn decide(world: &mut App, outcome: Result<(), CoreError>) {
    world.decision = Some(outcome);
}

#[given("an empty grant store")]
async fn empty_grants(world: &mut App) {
    world.grants = InMemoryGrants::new();
}

#[given(expr = "a principal {string} with roles {string}")]
async fn a_principal(world: &mut App, subject: String, roles: String) {
    world.principal = Some(Principal {
        subject,
        roles: roles
            .split(',')
            .map(|role_name| role(role_name.trim()))
            .collect(),
        groups: vec![],
        user_uuid: None,
    });
}

#[given(expr = "{string} is granted engine {string}")]
async fn granted(world: &mut App, subject: String, engine: String) {
    world.grants.grant(subject, engine);
}

#[when("the principal asks to run a query")]
async fn ask_run(world: &mut App) {
    if let Some(principal) = world.principal.clone() {
        decide(world, authorize(&principal, Action::RunQuery));
    }
}

#[when("the principal asks to administer")]
async fn ask_administer(world: &mut App) {
    if let Some(principal) = world.principal.clone() {
        decide(world, authorize(&principal, Action::Administer));
    }
}

#[when(expr = "{string} asks to use engine {string}")]
async fn ask_engine(world: &mut App, subject: String, engine: String) {
    let outcome = authorize_engine(&world.grants, &subject, &EngineId::new(engine)).await;
    decide(world, outcome);
}

#[then("the request is refused as unauthorized")]
async fn refused(world: &mut App) {
    let decision = world.decision.as_ref().expect("a decision was made");
    assert!(
        matches!(decision, Err(CoreError::Unauthorized(_))),
        "expected unauthorized, got {decision:?}"
    );
}

#[then("the request is allowed")]
async fn allowed(world: &mut App) {
    let decision = world.decision.as_ref().expect("a decision was made");
    assert!(decision.is_ok(), "expected allowed, got {decision:?}");
}

#[then("the engine request is refused as unauthorized")]
async fn engine_refused(world: &mut App) {
    refused(world).await;
}

#[then("the engine request is allowed")]
async fn engine_allowed(world: &mut App) {
    allowed(world).await;
}

#[given(expr = "a notebook with the title {string}")]
async fn notebook_with_title(world: &mut App, title: String) {
    world.notebook = Some(Notebook {
        id: "nb".into(),
        title,
        cells: Vec::new(),
    });
}

#[given(expr = "a cell {string} holding {string}")]
async fn cell(world: &mut App, id: String, sql: String) {
    push_cell(world, id, sql, None);
}

#[given(expr = "a cell {string} holding {string} bound to engine {string}")]
async fn cell_with_engine(world: &mut App, id: String, sql: String, engine: String) {
    push_cell(world, id, sql, Some(engine));
}

fn push_cell(world: &mut App, id: String, sql: String, engine: Option<String>) {
    let notebook = world.notebook.as_mut().expect("a notebook exists");
    notebook.cells.push(Cell {
        id,
        sql,
        engine: engine.map(EngineId::new),
    });
}

#[given("that cell has a SQL comment resembling a cell marker")]
async fn marker_like_sql(world: &mut App) {
    world.notebook.as_mut().unwrap().cells[0]
        .sql
        .push_str("\n-- cell not-a-cell\nSELECT 2");
}

#[when("the notebook is written to text and read back")]
async fn round_trip(world: &mut App) {
    let text = world
        .notebook
        .as_ref()
        .expect("a notebook exists")
        .to_text();
    world.notebook = Some(Notebook::from_text(&text).expect("text parses"));
}

#[then(expr = "the notebook has {int} cells")]
async fn has_cells(world: &mut App, count: usize) {
    let notebook = world.notebook.as_ref().expect("a notebook exists");
    assert_eq!(notebook.cells.len(), count);
}

#[then(expr = "that SQL comment remains in cell {string}")]
async fn marker_like_sql_preserved(world: &mut App, id: String) {
    let notebook = world.notebook.as_ref().unwrap();
    let cell = notebook.cells.iter().find(|cell| cell.id == id).unwrap();
    assert_eq!(cell.sql, "SELECT 1\n-- cell not-a-cell\nSELECT 2");
}

#[then(expr = "cell {string} is bound to engine {string}")]
async fn cell_bound(world: &mut App, id: String, engine: String) {
    let notebook = world.notebook.as_ref().expect("a notebook exists");
    let cell = notebook
        .cells
        .iter()
        .find(|cell| cell.id == id)
        .expect("the cell exists");
    assert_eq!(
        cell.engine.as_ref().map(|bound| bound.0.clone()),
        Some(engine)
    );
}

#[given(expr = "the text {string}")]
async fn given_text(world: &mut App, text: String) {
    world.text = text.replace("\\n", "\n");
}

#[when("the text is parsed as a notebook")]
async fn parse_text(world: &mut App) {
    match Notebook::from_text(&world.text) {
        Ok(notebook) => {
            world.notebook = Some(notebook);
            decide(world, Ok(()));
        }
        Err(error) => decide(world, Err(error)),
    }
}

#[then("parsing fails as invalid")]
async fn parsing_fails(world: &mut App) {
    let decision = world.decision.as_ref().expect("a parse was attempted");
    assert!(
        matches!(decision, Err(CoreError::Invalid(_))),
        "expected an invalid-notebook error, got {decision:?}"
    );
}

#[given(expr = "a session store with a {int} second idle timeout")]
async fn session_store(world: &mut App, ttl: i64) {
    world.sessions = Some(InMemorySessions::new(ttl));
}

#[given(expr = "a handshake store with a {int} second timeout")]
async fn handshake_store(world: &mut App, ttl: i64) {
    world.handshakes = Some(InMemoryHandshakes::new(ttl));
}

#[when(expr = "a session is created for {string} with roles {string} at time {int}")]
async fn create_session(world: &mut App, subject: String, roles: String, now: i64) {
    let roles = roles.split(',').map(|r| role(r.trim())).collect();
    let record = world
        .sessions()
        .create(&subject, roles, vec![], None, now)
        .await
        .expect("the session is created");
    world.sid = record.sid.clone();
    world.record = Some(record);
}

#[given(expr = "a session created for {string} with roles {string} at time {int}")]
async fn a_session(world: &mut App, subject: String, roles: String, now: i64) {
    create_session(world, subject, roles, now).await;
}

#[when(expr = "that session is resolved at time {int}")]
async fn resolve_session(world: &mut App, now: i64) {
    world.record = world
        .sessions()
        .get(&world.sid, now)
        .await
        .expect("the store answers");
}

#[when("that session is revoked")]
async fn revoke_session(world: &mut App) {
    world
        .sessions()
        .revoke(&world.sid)
        .await
        .expect("the store answers");
}

#[then(expr = "the session resolves to subject {string}")]
async fn session_subject(world: &mut App, subject: String) {
    let record = world.record.as_ref().expect("a session was resolved");
    assert_eq!(record.subject, subject);
}

#[then("no session is resolved")]
async fn no_session(world: &mut App) {
    assert!(world.record.is_none(), "expected no session");
}

#[then("the session id reveals nothing about the subject")]
async fn opaque_sid(world: &mut App) {
    assert_eq!(world.sid.len(), 32);
    assert!(!world.sid.contains("alice"));
}

#[then(expr = "subject {string} has {int} sessions listed")]
async fn listed(world: &mut App, subject: String, count: usize) {
    world.listed = world
        .sessions()
        .list(&subject)
        .await
        .expect("the store answers");
    assert_eq!(world.listed.len(), count);
}

#[when(expr = "the handshake payload {string} is stored for state {string} at time {int}")]
async fn store_handshake(world: &mut App, payload: String, state: String, now: i64) {
    world
        .handshakes()
        .put(&state, &payload, now)
        .await
        .expect("the store answers");
}

#[then(expr = "redeeming state {string} at time {int} yields {string}")]
async fn redeem_handshake(world: &mut App, state: String, now: i64, expected: String) {
    world.redeemed = world
        .handshakes()
        .take(&state, now)
        .await
        .expect("the store answers");
    assert_eq!(world.redeemed.as_deref(), Some(expected.as_str()));
}

#[then(expr = "redeeming state {string} at time {int} yields nothing")]
async fn redeem_handshake_nothing(world: &mut App, state: String, now: i64) {
    world.redeemed = world
        .handshakes()
        .take(&state, now)
        .await
        .expect("the store answers");
    assert!(world.redeemed.is_none(), "expected nothing to be redeemed");
}

#[given("an empty user state store")]
async fn empty_user_state(world: &mut App) {
    world.user_state = Some(InMemoryUserState::new());
}

#[when(expr = "subject {string} records notebook {string} cell {string} engine {string}")]
async fn record_state(
    world: &mut App,
    subject: String,
    notebook: String,
    cell: String,
    engine: String,
) {
    let mut state = WorkingState::default();
    state.notebook = Some(notebook);
    state.cell = Some(cell);
    state.engine = Some(engine);
    world
        .user_state()
        .put(&subject, &state)
        .await
        .expect("the store answers");
}

#[then(expr = "subject {string} resumes notebook {string}")]
async fn resumes_notebook(world: &mut App, subject: String, notebook: String) {
    world.resumed = world
        .user_state()
        .get(&subject)
        .await
        .expect("the store answers");
    let resumed = world.resumed.as_ref().expect("state was recorded");
    assert_eq!(resumed.notebook.as_deref(), Some(notebook.as_str()));
}

#[then(expr = "subject {string} resumes cell {string}")]
async fn resumes_cell(world: &mut App, _subject: String, cell: String) {
    let resumed = world.resumed.as_ref().expect("state was recorded");
    assert_eq!(resumed.cell.as_deref(), Some(cell.as_str()));
}

#[then(expr = "subject {string} resumes engine {string}")]
async fn resumes_engine(world: &mut App, _subject: String, engine: String) {
    let resumed = world.resumed.as_ref().expect("state was recorded");
    assert_eq!(resumed.engine.as_deref(), Some(engine.as_str()));
}

#[then(expr = "subject {string} has no recorded state")]
async fn no_recorded_state(world: &mut App, subject: String) {
    let state = world
        .user_state()
        .get(&subject)
        .await
        .expect("the store answers");
    assert!(state.is_none(), "expected nothing recorded, got {state:?}");
}

#[given("a secret store")]
async fn a_secret_store(world: &mut App) {
    world.secrets = Some(InMemorySecrets::new());
}

#[given(expr = "the secret store holds {string} = {string}")]
async fn secret_holds(world: &mut App, key: String, value: String) {
    world.secrets().set(&key, &value);
}

#[when(expr = "the server requires {string}")]
async fn requires_secret(world: &mut App, key: String) {
    world.secret = Some(require(world.secrets(), &key).await);
}

#[then(expr = "it receives {string}")]
async fn receives_secret(world: &mut App, expected: String) {
    let secret = world.secret.as_ref().expect("a secret was required");
    match secret {
        Ok(value) => assert_eq!(value, &expected),
        Err(error) => panic!("expected {expected}, got {error}"),
    }
}

#[then(expr = "it is refused naming {string}")]
async fn refused_naming(world: &mut App, key: String) {
    let secret = world.secret.as_ref().expect("a secret was required");
    let error = secret.as_ref().expect_err("expected a refusal").to_string();
    assert!(error.contains(&key), "expected the key in {error}");
}

#[when(expr = "the server looks up {string}")]
async fn looks_up_secret(world: &mut App, key: String) {
    let looked_up = world.secrets().get(&key).await.expect("the store answers");
    world.looked_up = Some(looked_up);
}

#[then("the lookup reports nothing configured")]
async fn lookup_reports_nothing(world: &mut App) {
    let looked_up = world.looked_up.as_ref().expect("a lookup happened");
    assert!(looked_up.is_none(), "expected nothing, got {looked_up:?}");
}

/* ---- data contracts -------------------------------------------------- */

/// Builds the YAML a team actually commits, so the parser is exercised on the
/// real shape rather than on our own convenience format.
#[given(expr = "an ODCS v3.2 contract named {string} with fields {string} and owner {string}")]
async fn an_odcs_contract(world: &mut App, name: String, fields: String, owner: String) {
    let mut properties = String::new();
    for (index, field) in fields.split(',').map(str::trim).enumerate() {
        let semantic = if field == "total" {
            "measure"
        } else {
            "dimension"
        };
        properties.push_str(&format!(
            "      - name: {field}\n        logicalType: bigint\n        semanticType: {semantic}\n"
        ));
        let _ = index;
    }
    world.contract_text = format!(
        "version: 3.2.0\napiVersion: v3.2.0\nkind: DataContract\nid: {name}\nteam:\n  name: {owner}\nschema:\n  - name: {name}\n    properties:\n{properties}"
    );
}

#[given(expr = "a flat contract object with name {string} and a required field {string}")]
async fn a_flat_contract(world: &mut App, name: String, field: String) {
    world.contract_text = format!(
        r#"{{"name":"{name}","schema":[{{"name":"{field}","type":"bigint","required":true}}]}}"#
    );
}

#[given("a contract document without a name")]
async fn a_contract_without_a_name(world: &mut App) {
    world.contract_text = "schema: []\n".into();
}

#[when("the contract is parsed")]
async fn parse_contract(world: &mut App) {
    world.contract = Some(DataContract::parse("contract.yaml", &world.contract_text));
}

#[then(expr = "the contract is named {string}")]
async fn contract_is_named(world: &mut App, name: String) {
    assert_eq!(contract(world).name, name);
}

#[then(expr = "the owner is {string}")]
async fn contract_owner_is(world: &mut App, owner: String) {
    assert_eq!(contract(world).owner.as_deref(), Some(owner.as_str()));
}

#[then(expr = "the contract has {int} fields")]
async fn contract_field_count(world: &mut App, count: usize) {
    assert_eq!(contract(world).fields.len(), count);
}

/// Same assertion, singular wording in a scenario.
#[then(expr = "the contract has {int} field")]
async fn contract_field_count_singular(world: &mut App, count: usize) {
    assert_eq!(contract(world).fields.len(), count);
}

#[then(expr = "the field {string} is a {string}")]
async fn field_semantic_type(world: &mut App, name: String, semantic: String) {
    let field = contract(world)
        .fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("no field named {name}"));
    assert_eq!(field.semantic_type.as_deref(), Some(semantic.as_str()));
}

#[then("the parse is refused as invalid")]
async fn parse_is_refused(world: &mut App) {
    let parsed = world.contract.as_ref().expect("a contract was parsed");
    assert!(
        matches!(parsed, Err(CoreError::Invalid(_))),
        "expected an invalid contract, got {parsed:?}"
    );
}

#[given(expr = "a contract named {string} and a contract named {string}")]
async fn two_contracts(world: &mut App, first: String, second: String) {
    world.contracts = [first, second]
        .iter()
        .map(|name| DataContract::parse(name, &format!(r#"{{"name":"{name}"}}"#)).expect("parses"))
        .collect();
}

#[when(expr = "the statement {string} is prepared")]
async fn prepare_statement(world: &mut App, sql: String) {
    world.selected = relevant(&world.contracts, &sql)
        .into_iter()
        .map(|contract| contract.name.clone())
        .collect();
}

#[then(expr = "the selected contracts are {string}")]
async fn selected_contracts(world: &mut App, expected: String) {
    assert_eq!(world.selected.join(", "), expected);
}

#[then("no contract is selected")]
async fn no_contract_selected(world: &mut App) {
    assert!(
        world.selected.is_empty(),
        "expected none, got {:?}",
        world.selected
    );
}

fn contract(world: &App) -> &DataContract {
    world
        .contract
        .as_ref()
        .expect("a contract was parsed")
        .as_ref()
        .expect("the contract parsed")
}

/* ---- semantic models -------------------------------------------------- */

#[given(expr = "a table {string} {string} with columns {string}")]
async fn a_table(world: &mut App, namespace: String, name: String, columns: String) {
    let columns = columns
        .split(',')
        .map(str::trim)
        .filter(|column| !column.is_empty())
        .map(|column| {
            let (name, data_type) = column.split_once(' ').unwrap_or((column, "varchar"));
            ColumnSchema {
                name: name.to_string(),
                data_type: data_type.trim().to_string(),
                nullable: true,
            }
        })
        .collect();
    world.table = Some(TableSchema {
        table: TableRef { namespace, name },
        columns,
    });
}

#[when(expr = "the table is rendered as {string}")]
async fn render_table(world: &mut App, target: String) {
    let rendered = {
        let schema = world.table.as_ref().expect("a table");
        let contract = world
            .contract
            .as_ref()
            .and_then(|contract| contract.as_ref().ok());
        let model = TableModel { schema, contract };
        aster_core::semantic::format(&target).and_then(|format| format.render(&model))
    };
    world.model = Some(rendered);
}

#[when("the rendered contract is parsed")]
async fn parse_rendered_contract(world: &mut App) {
    world.contract = Some(DataContract::parse("rendered.yaml", model(world)));
}

#[then(expr = "the model declares the cube {string}")]
async fn model_declares_cube(world: &mut App, name: String) {
    let text = model(world);
    assert!(
        text.contains(&format!("cubes:\n  - name: \"{name}\"")),
        "model was:\n{text}"
    );
}

#[then(expr = "the model names the table {string}")]
async fn model_names_table(world: &mut App, table: String) {
    let text = model(world);
    assert!(
        text.contains(&format!("sql_table: \"{table}\"")),
        "model was:\n{text}"
    );
}

#[then(expr = "the model declares the dimension {string}")]
async fn model_declares_dimension(world: &mut App, name: String) {
    let found = members(model(world), "dimensions");
    assert!(
        found.contains(&name.as_str()),
        "expected {name} among {found:?} in:\n{}",
        model(world)
    );
}

#[then(expr = "the model declares the measure {string}")]
async fn model_declares_measure(world: &mut App, name: String) {
    let found = members(model(world), "measures");
    assert!(
        found.contains(&name.as_str()),
        "expected {name} among {found:?} in:\n{}",
        model(world)
    );
}

/// The `- name: "..."` entries of one four-space-indented section.
fn members<'a>(text: &'a str, header: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        if indent == 4 && line.trim_start().starts_with(&format!("{header}:")) {
            inside = true;
            continue;
        }
        if inside && indent <= 4 && !line.trim().is_empty() {
            break;
        }
        if inside {
            if let Some(rest) = line.trim_start().strip_prefix("- name: ") {
                found.push(rest.trim_matches('"'));
            }
        }
    }
    found
}

fn model(world: &App) -> &str {
    world
        .model
        .as_ref()
        .expect("a table was rendered")
        .as_ref()
        .expect("the table rendered")
}

#[tokio::main]
async fn main() {
    App::run("features").await;
}
