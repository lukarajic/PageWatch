use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

mod models;
mod storage;
mod checker;
use models::{TrackingMode, Watch};

use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::io::{self, stdout};

enum InputMode {
    Normal,
    Editing,
}

enum InputField {
    Name,
    Url,
    Mode,
}

use tokio::sync::mpsc;

struct App {
    watches: Vec<Watch>,
    state: ListState,
    input_mode: InputMode,
    input_field: InputField,
    name_input: String,
    url_input: String,
    mode_selection: usize,
    pending_checks: usize,
    last_tick: std::time::Instant,
    tx: mpsc::UnboundedSender<(usize, Watch)>,
    rx: mpsc::UnboundedReceiver<(usize, Watch)>,
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

        let (tx, rx) = mpsc::unbounded_channel();

        let mut app = App {
            watches,
            state: ListState::default(),
            input_mode: InputMode::Normal,
            input_field: InputField::Name,
            name_input: String::new(),
            url_input: String::new(),
            mode_selection: 0,
            pending_checks: 0,
            last_tick: std::time::Instant::now(),
            tx,
            rx,
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
        if i < self.watches.len() {
            self.watches[i].has_unread_change = false;
        }
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
        if i < self.watches.len() {
            self.watches[i].has_unread_change = false;
        }
    }

    fn submit_watch(&mut self) {
        if !self.name_input.trim().is_empty() && !self.url_input.trim().is_empty() {
            let mode = match self.mode_selection {
                1 => TrackingMode::Price { selector: None },
                2 => TrackingMode::Availability { in_stock_keywords: vec![], out_of_stock_keywords: vec![] },
                3 => TrackingMode::Keyword { keywords: vec![] },
                4 => TrackingMode::HtmlSection { selector: String::new() },
                _ => TrackingMode::FullPage,
            };
            let new_watch = Watch::new(
                self.name_input.clone(),
                self.url_input.clone(),
                mode,
            );
            self.watches.push(new_watch);
            self.state.select(Some(self.watches.len() - 1));
        }
        self.reset_input();
    }

    fn reset_input(&mut self) {
        self.name_input.clear();
        self.url_input.clear();
        self.mode_selection = 0;
        self.input_mode = InputMode::Normal;
        self.input_field = InputField::Name;
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
        // Handle background task results
        while let Ok((idx, watch)) = app.rx.try_recv() {
            if idx < app.watches.len() {
                app.watches[idx] = watch;
            }
            app.pending_checks = app.pending_checks.saturating_sub(1);
        }

        // Periodic automatic checks (every 5 seconds)
        if app.last_tick.elapsed() >= std::time::Duration::from_secs(5) {
            app.last_tick = std::time::Instant::now();
            let now = chrono::Utc::now();
            
            for (i, watch) in app.watches.iter().enumerate() {
                let should_check = match watch.last_checked {
                    Some(last) => (now - last).num_seconds() >= watch.interval_seconds as i64,
                    None => true,
                };

                if should_check {
                    app.pending_checks += 1;
                    let mut watch_clone = watch.clone();
                    let tx = app.tx.clone();
                    tokio::spawn(async move {
                        let _ = checker::check_watch(&mut watch_clone).await;
                        let _ = tx.send((i, watch_clone));
                    });
                }
            }
        }

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
                    let mut title_spans = vec![
                        Span::styled(&w.name, Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(format!(" ({})", w.url)),
                    ];

                    if w.has_unread_change {
                        title_spans.push(Span::raw(" "));
                        title_spans.push(Span::styled("● NEW", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
                    }

                    let mut lines = vec![
                        Line::from(title_spans),
                        Line::from(format!("  Mode: {:?}", w.mode)),
                    ];

                    if let Some(last) = w.last_checked {
                        lines.push(Line::from(format!("  Last checked: {}", last.format("%Y-%m-%d %H:%M:%S"))));
                    }

                    if let Some(val) = &w.last_value {
                        let snippet = if val.chars().count() > 50 {
                            format!("{}...", val.chars().take(50).collect::<String>())
                        } else {
                            val.clone()
                        };
                        lines.push(Line::from(vec![
                            Span::raw("  Value: "),
                            Span::styled(snippet, Style::default().fg(Color::Cyan)),
                        ]));
                    }

                    if let Some(err) = &w.last_error {
                        lines.push(Line::from(vec![
                            Span::raw("  Error: "),
                            Span::styled(err, Style::default().fg(Color::Red)),
                        ]));
                    }

                    ListItem::new(lines).style(Style::default().fg(Color::White))
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Watches"))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray));

            f.render_stateful_widget(list, chunks[0], &mut app.state);

            let status_text = if app.pending_checks > 0 {
                format!("Checking {} watch(es)... (UI is responsive)", app.pending_checks)
            } else {
                match app.input_mode {
                    InputMode::Normal => "Press 'q' to quit, 'n' to add, 'c' to check, 'd' to delete, Up/Down to navigate.".to_string(),
                    InputMode::Editing => "Editing: 'Enter' to next/submit, 'Esc' to cancel.".to_string(),
                }
            };
            let help = Paragraph::new(status_text)
                .block(Block::default().borders(Borders::ALL).title("Status"));
            f.render_widget(help, chunks[1]);

            // Render Input Popup if in Editing mode
            if let InputMode::Editing = app.input_mode {
                let block = Block::default().title("Add New Watch").borders(Borders::ALL);
                let area = centered_rect(60, 40, size);
                f.render_widget(Clear, area); // Clear the background
                f.render_widget(block, area);

                let popup_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(2)
                    .constraints(
                        [
                            Constraint::Length(3), // Name input
                            Constraint::Length(3), // URL input
                            Constraint::Min(5),    // Mode selection
                        ]
                        .as_ref(),
                    )
                    .split(area);

                let name_style = if let InputField::Name = app.input_field {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };

                let url_style = if let InputField::Url = app.input_field {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };

                let mode_style = if let InputField::Mode = app.input_field {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };

                let name_block = Block::default().title("Name").borders(Borders::ALL).style(name_style);
                let name_text = Paragraph::new(app.name_input.clone()).block(name_block);
                f.render_widget(name_text, popup_chunks[0]);

                let url_block = Block::default().title("URL").borders(Borders::ALL).style(url_style);
                let url_text = Paragraph::new(app.url_input.clone()).block(url_block);
                f.render_widget(url_text, popup_chunks[1]);

                let modes = vec!["Full Page", "Price", "Availability", "Keyword", "HTML Section"];
                let mode_items: Vec<ListItem> = modes.iter().enumerate().map(|(i, m)| {
                    let style = if i == app.mode_selection {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(format!("  {}", m)).style(style)
                }).collect();

                let mode_list = List::new(mode_items)
                    .block(Block::default().title("Tracking Mode").borders(Borders::ALL).style(mode_style));
                f.render_widget(mode_list, popup_chunks[2]);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('n') => {
                                app.input_mode = InputMode::Editing;
                                app.input_field = InputField::Name;
                            }
                            KeyCode::Char('c') => {
                                if let Some(i) = app.state.selected() {
                                    app.pending_checks += 1;
                                    let mut watch = app.watches[i].clone();
                                    let tx = app.tx.clone();
                                    
                                    tokio::spawn(async move {
                                        let _ = checker::check_watch(&mut watch).await;
                                        let _ = tx.send((i, watch));
                                    });
                                }
                            }
                            KeyCode::Char('d') => {
                                if let Some(i) = app.state.selected() {
                                    app.watches.remove(i);
                                    if app.watches.is_empty() {
                                        app.state.select(None);
                                    } else if i >= app.watches.len() {
                                        app.state.select(Some(app.watches.len() - 1));
                                    }
                                }
                            }
                            KeyCode::Down => app.next(),
                            KeyCode::Up => app.previous(),
                            _ => {}
                        },
                        InputMode::Editing => match key.code {
                            KeyCode::Esc => app.reset_input(),
                            KeyCode::Enter => {
                                match app.input_field {
                                    InputField::Name => app.input_field = InputField::Url,
                                    InputField::Url => app.input_field = InputField::Mode,
                                    InputField::Mode => app.submit_watch(),
                                }
                            }
                            KeyCode::Down if matches!(app.input_field, InputField::Mode) => {
                                app.mode_selection = (app.mode_selection + 1) % 5;
                            }
                            KeyCode::Up if matches!(app.input_field, InputField::Mode) => {
                                if app.mode_selection == 0 {
                                    app.mode_selection = 4;
                                } else {
                                    app.mode_selection -= 1;
                                }
                            }
                            KeyCode::Char(c) => {
                                match app.input_field {
                                    InputField::Name => app.name_input.push(c),
                                    InputField::Url => app.url_input.push(c),
                                    InputField::Mode => {}
                                }
                            }
                            KeyCode::Backspace => {
                                match app.input_field {
                                    InputField::Name => { app.name_input.pop(); },
                                    InputField::Url => { app.url_input.pop(); },
                                    InputField::Mode => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

/// Helper function to create a centered rect using up certain percentage of the available rect `r`
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}
