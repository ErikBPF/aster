use ratatui::{
    crossterm::{
        event::{self, Event, KeyCode},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::io::stdout;

fn main() -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut output = stdout();
    execute!(output, EnterAlternateScreen)?;

    let backend = ratatui::backend::CrosstermBackend::new(output);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(draw)?;
        if let Event::Key(key) = event::read()? {
            if key.code == KeyCode::Char('q') {
                break;
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn draw(frame: &mut Frame) {
    let body = Paragraph::new(
        "aster TUI scaffold.\n\nNotebooks, engine picker, and catalog browse land here.\nPress q to quit.",
    )
    .block(Block::default().title("aster").borders(Borders::ALL));
    frame.render_widget(body, frame.area());
}
