//! Behavior contracts for `aster-core`, run as real Gherkin tests.
//!
//! `crates/core/features/*.feature` are the contract; this file binds their
//! steps to the domain API. Run with `cargo test -p aster-core` (declared in
//! Cargo.toml as a `harness = false` test target).

use aster_core::{
    authorize, authorize_engine, Action, Cell, CoreError, EngineId, InMemoryGrants, Notebook,
    Principal, Role,
};
use cucumber::{given, then, when, World};

#[derive(Debug, Default, World)]
struct App {
    principal: Option<Principal>,
    grants: InMemoryGrants,
    decision: Option<Result<(), CoreError>>,
    notebook: Option<Notebook>,
    text: String,
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

#[tokio::main]
async fn main() {
    App::run("features").await;
}
