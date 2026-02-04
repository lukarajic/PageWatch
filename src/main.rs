use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

mod models;
mod storage;
use models::{TrackingMode, Watch};

use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::io::{self, stdout};

struct App {
    watches: Vec<Watch>,
    state: ListState,
}

impl App {
    fn new() -> App {
        // Try to load existing watches
        let loaded_watches = storage::load_watches().unwrap_or_else(|_| Vec::new());

        // If no watches exist (first run or file missing), use mock data
        let watches = if loaded_watches.is_empty() {
            vec![
                Watch::new(
                    "Competitor Price".to_string(),
                    "https://example.com/product".to_string(),
                    TrackingMode::Price { selector: None },
                ),
                Watch::new(
                    "My Portfolio".to_string(),
                    "https://mysite.com".to_string(),
                    TrackingMode::FullPage,
                ),
                Watch::new(
                    "GPU Stock".to_string(),
                    "https://store.com/gpu".to_string(),
                    TrackingMode::Availability {
                        in_stock_keywords: vec![],
                        out_of_stock_keywords: vec![],
                    },
                ),
            ]
        } else {
            loaded_watches
        };

        let mut app = App {
            watches,
            state: ListState::default(),
        };
        // Select the first item by default
        if !app.watches.is_empty() {
            app.state.select(Some(0));
        }
        app
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.watches.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.watches.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new();

    // Main loop
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Save state
    if let Err(e) = storage::save_watches(&app.watches) {
        eprintln!("Failed to save watches: {}", e);
    }

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
                .split(size);

            let items: Vec<ListItem> = app
                .watches
                .iter()
                .map(|w| {
                    let lines = vec![
                        ratatui::text::Line::from(format!("{} ({})", w.name, w.url)),
                        ratatui::text::Line::from(format!("  Mode: {:?}", w.mode)),
                    ];
                    ListItem::new(lines).style(Style::default().fg(Color::White))
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Watches"))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray));

            f.render_stateful_widget(list, chunks[0], &mut app.state);

            let help = Paragraph::new("Press 'q' to quit. Use 'Up'/'Down' to navigate.")
                .block(Block::default().borders(Borders::ALL).title("Status"));
            f.render_widget(help, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Down => app.next(),
                        KeyCode::Up => app.previous(),
                        _ => {}
                    }
                }
            }
        }
    }
}
