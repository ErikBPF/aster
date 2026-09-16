//! aster TUI client. Talks to the same JSON API as the web UI, so it shares the
//! server's authorization and audit path. ponytail: blocking event loop with a
//! runtime handle per request; move to an async event loop when concurrent
//! requests matter.

use std::io::stdout;

use aster_core::{Notebook, QueryResult};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

struct Client {
    http: reqwest::Client,
    base: String,
    subject: String,
    roles: String,
    runtime: tokio::runtime::Runtime,
}

impl Client {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            http: reqwest::Client::new(),
            base: std::env::var("ASTER_SERVER").unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
            subject: std::env::var("ASTER_SUBJECT").unwrap_or_else(|_| "alice".into()),
            roles: std::env::var("ASTER_ROLES").unwrap_or_else(|_| "editor".into()),
            runtime: tokio::runtime::Runtime::new()?,
        })
    }

    fn call<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, String> {
        let request = request
            .header("x-aster-subject", &self.subject)
            .header("x-aster-roles", &self.roles);
        self.runtime.block_on(async {
            let response = request.send().await.map_err(|e| e.to_string())?;
            let status = response.status();
            let body = response.text().await.map_err(|e| e.to_string())?;
            if !status.is_success() {
                return Err(format!("{status}: {}", error_message(&body)));
            }
            serde_json::from_str(&body).map_err(|e| e.to_string())
        })
    }

    fn list_notebooks(&self) -> Result<Vec<String>, String> {
        self.call(self.http.get(format!("{}/api/notebooks", self.base)))
    }

    fn engines(&self) -> Result<Vec<String>, String> {
        let engines: Vec<EngineSummary> =
            self.call(self.http.get(format!("{}/api/engines", self.base)))?;
        Ok(engines.into_iter().map(|engine| engine.id).collect())
    }

    fn notebook(&self, id: &str) -> Result<Notebook, String> {
        self.call(self.http.get(format!("{}/api/notebooks/{id}", self.base)))
    }

    /// Where this subject left off, resolved from the shared state plane.
    fn working_state(&self) -> Result<WorkingState, String> {
        self.call(self.http.get(format!("{}/api/state", self.base)))
    }

    fn save(&self, notebook: &Notebook) -> Result<String, String> {
        #[derive(serde::Deserialize)]
        struct Saved {
            revision: String,
        }
        let saved: Saved = self.call(
            self.http
                .put(format!("{}/api/notebooks/{}", self.base, notebook.id))
                .json(notebook),
        )?;
        Ok(saved.revision)
    }

    fn run(&self, sql: &str, engine: Option<String>) -> Result<QueryResult, String> {
        self.call(
            self.http
                .post(format!("{}/api/query", self.base))
                .json(&serde_json::json!({ "sql": sql, "engine": engine })),
        )
    }
}

#[derive(serde::Deserialize)]
struct EngineSummary {
    id: String,
}

/// The subset of `/api/state` the client resumes from.
#[derive(serde::Deserialize)]
struct WorkingState {
    notebook: Option<String>,
}

fn error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_string))
        .unwrap_or_else(|| body.trim().chars().take(120).collect())
}

#[derive(PartialEq)]
enum Screen {
    Notebooks,
    Cells,
}

struct App {
    screen: Screen,
    notebooks: Vec<String>,
    selected: usize,
    notebook: Option<Notebook>,
    cell: usize,
    engines: Vec<String>,
    result: Option<QueryResult>,
    status: String,
}

impl App {
    fn new(client: &Client) -> Self {
        let mut app = Self {
            screen: Screen::Notebooks,
            notebooks: Vec::new(),
            selected: 0,
            notebook: None,
            cell: 0,
            engines: Vec::new(),
            result: None,
            status: String::new(),
        };
        app.refresh(client);
        match client.engines() {
            Ok(engines) => app.engines = engines,
            Err(error) => app.status = format!("engines: {error}"),
        }
        // Resume where this subject left off, if the state plane knows.
        if let Ok(working) = client.working_state() {
            if let Some(id) = working.notebook {
                if let Some(index) = app.notebooks.iter().position(|open| open == &id) {
                    app.selected = index;
                    app.status = format!("resuming {id}");
                }
            }
        }
        app
    }

    fn refresh(&mut self, client: &Client) {
        match client.list_notebooks() {
            Ok(notebooks) => {
                self.notebooks = notebooks;
                self.selected = self.selected.min(self.notebooks.len().saturating_sub(1));
                self.status = format!("{} notebook(s)", self.notebooks.len());
            }
            Err(error) => self.status = error,
        }
    }

    fn open(&mut self, client: &Client) {
        let Some(id) = self.notebooks.get(self.selected) else {
            return;
        };
        match client.notebook(id) {
            Ok(notebook) => {
                self.notebook = Some(notebook);
                self.cell = 0;
                self.result = None;
                self.screen = Screen::Cells;
                self.status = format!("opened {id}");
            }
            Err(error) => self.status = error,
        }
    }

    fn run_cell(&mut self, client: &Client) {
        let Some(notebook) = &self.notebook else {
            return;
        };
        let Some(cell) = notebook.cells.get(self.cell) else {
            return;
        };
        let (sql, engine) = (
            cell.sql.clone(),
            cell.engine.as_ref().map(|id| id.to_string()),
        );
        match client.run(&sql, engine) {
            Ok(result) => {
                self.status = format!("{} row(s)", result.rows.len());
                self.result = Some(result);
            }
            Err(error) => self.status = error,
        }
    }

    fn save(&mut self, client: &Client) {
        let Some(notebook) = &self.notebook else {
            return;
        };
        match client.save(notebook) {
            Ok(revision) => {
                self.status = format!("saved {}", revision.chars().take(8).collect::<String>())
            }
            Err(error) => self.status = error,
        }
    }

    /// Cycle the selected cell through the configured engine pool, then unset.
    fn cycle_engine(&mut self) {
        let (Some(notebook), false) = (&mut self.notebook, self.engines.is_empty()) else {
            return;
        };
        let Some(cell) = notebook.cells.get_mut(self.cell) else {
            return;
        };
        let current = cell.engine.as_ref().map(|id| id.to_string());
        let next = self
            .engines
            .iter()
            .position(|id| Some(id) == current.as_ref());
        cell.engine = match next {
            None => Some(aster_core::EngineId::new(self.engines[0].clone())),
            Some(index) if index + 1 < self.engines.len() => {
                Some(aster_core::EngineId::new(self.engines[index + 1].clone()))
            }
            Some(_) => None,
        };
        self.status = format!(
            "engine: {}",
            cell.engine
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "default".into())
        );
    }
}

fn main() -> anyhow::Result<()> {
    let client = Client::from_env()?;
    let mut app = App::new(&client);

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let outcome = event_loop(&mut terminal, &mut app, &client);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    outcome
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    client: &Client,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(());
        }
        match key.code {
            KeyCode::Char('q') => return Ok(()),
            KeyCode::Char('r') if app.screen == Screen::Notebooks => app.refresh(client),
            KeyCode::Char('j') | KeyCode::Down => match app.screen {
                Screen::Notebooks => {
                    app.selected = advance(app.selected, app.notebooks.len());
                }
                Screen::Cells => {
                    let len = app.notebook.as_ref().map(|n| n.cells.len()).unwrap_or(0);
                    app.cell = advance(app.cell, len);
                }
            },
            KeyCode::Char('k') | KeyCode::Up => match app.screen {
                Screen::Notebooks => app.selected = retreat(app.selected, app.notebooks.len()),
                Screen::Cells => app.cell = retreat(app.cell, cell_count(app)),
            },
            KeyCode::Enter => match app.screen {
                Screen::Notebooks => app.open(client),
                Screen::Cells => app.run_cell(client),
            },
            KeyCode::Char('g') if app.screen == Screen::Cells => app.cycle_engine(),
            KeyCode::Char('s') if app.screen == Screen::Cells => app.save(client),
            KeyCode::Esc if app.screen == Screen::Cells => {
                app.screen = Screen::Notebooks;
                app.notebook = None;
                app.result = None;
            }
            _ => {}
        }
    }
}

fn cell_count(app: &App) -> usize {
    app.notebook.as_ref().map(|n| n.cells.len()).unwrap_or(0)
}

fn advance(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + 1).min(len - 1)
    }
}

fn retreat(current: usize, len: usize) -> usize {
    current.min(len.saturating_sub(1)).saturating_sub(1)
}

fn draw(frame: &mut Frame, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),
            Constraint::Length(10),
            Constraint::Length(1),
        ])
        .split(frame.area());
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(20)])
        .split(rows[0]);

    let items: Vec<ListItem> = app
        .notebooks
        .iter()
        .map(|id| ListItem::new(id.clone()))
        .collect();
    let mut state = ListState::default();
    if !app.notebooks.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title("notebooks"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        top[0],
        &mut state,
    );

    let mut lines: Vec<Line> = Vec::new();
    match &app.notebook {
        Some(notebook) => {
            lines.push(Line::from(format!("{} ({})", notebook.title, notebook.id)));
            for (index, cell) in notebook.cells.iter().enumerate() {
                let engine = cell
                    .engine
                    .as_ref()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "default".into());
                let marker = if index == app.cell { ">" } else { " " };
                lines.push(Line::from(vec![Span::styled(
                    format!("{marker} {} [{}]", cell.id, engine),
                    if index == app.cell {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                )]));
                for sql_line in cell.sql.lines() {
                    lines.push(Line::from(format!("    {sql_line}")));
                }
            }
            if notebook.cells.is_empty() {
                lines.push(Line::from("no cells"));
            }
        }
        None => lines.push(Line::from("select a notebook and press enter")),
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("cells"))
            .wrap(Wrap { trim: false }),
        top[1],
    );

    let result_lines = app
        .result
        .as_ref()
        .map(|result| table_lines(result, rows[1].width as usize))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(result_lines).block(Block::default().borders(Borders::ALL).title("result")),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(app.status.clone()).style(Style::default().add_modifier(Modifier::DIM)),
        rows[2],
    );
}

fn table_lines(result: &QueryResult, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(truncate(
        &result
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect::<Vec<_>>()
            .join(" | "),
        width,
    ))];
    for row in &result.rows {
        let text = row
            .iter()
            .map(|value| match value {
                serde_json::Value::Null => "∅".to_string(),
                serde_json::Value::String(value) => value.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" | ");
        lines.push(Line::from(truncate(&text, width)));
    }
    if result.rows.is_empty() {
        lines.push(Line::from("(no rows)"));
    }
    if result.truncated {
        lines.push(Line::from("(truncated)"));
    }
    lines
}

fn truncate(value: &str, width: usize) -> String {
    if width == 0 || value.chars().count() <= width {
        return value.to_string();
    }
    value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_core::Column;

    #[test]
    fn advance_and_retreat_stay_inside_the_list() {
        assert_eq!(advance(0, 3), 1);
        assert_eq!(advance(2, 3), 2);
        assert_eq!(advance(0, 0), 0);
        assert_eq!(retreat(2, 3), 1);
        assert_eq!(retreat(0, 3), 0);
        assert_eq!(retreat(0, 0), 0);
    }

    #[test]
    fn renders_a_result_as_text_rows() {
        let result = QueryResult {
            columns: vec![Column {
                name: "n".into(),
                data_type: "bigint".into(),
            }],
            rows: vec![vec![serde_json::json!(1)]],
            truncated: false,
        };
        let lines = table_lines(&result, 40);
        assert_eq!(lines[0].to_string().trim_end(), "n");
        assert_eq!(lines[1].to_string().trim_end(), "1");
    }

    #[test]
    fn truncates_long_lines_to_the_pane_width() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
    }
}
