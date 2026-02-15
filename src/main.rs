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
    Details,
    ConfirmDelete,
    Search,
}

enum InputField {
    Name,
    Url,
    Interval,
    Mode,
    Advanced,
}

use tokio::sync::mpsc;

struct App {
    watches: Vec<Watch>,
    state: ListState,
    input_mode: InputMode,
    input_field: InputField,
    name_input: String,
    url_input: String,
    interval_input: String,
    advanced_input: String,
    search_query: String,
    mode_selection: usize,
    editing_watch_index: Option<usize>,
    pending_checks: usize,
    last_tick: std::time::Instant,
    scroll: u16,
    tx: mpsc::UnboundedSender<(usize, Watch)>,
    rx: mpsc::UnboundedReceiver<(usize, Watch)>,
}

impl App {
    fn new() -> App {
        let watches = match storage::load_watches() {
            Ok(w) => w,
            Err(_) => Vec::new(),
        };

        let (tx, rx) = mpsc::unbounded_channel();
        let mut app = App {
            watches,
            state: ListState::default(),
            input_mode: InputMode::Normal,
            input_field: InputField::Name,
            name_input: String::new(),
            url_input: String::new(),
            interval_input: String::new(),
            advanced_input: String::new(),
            search_query: String::new(),
            mode_selection: 0,
            editing_watch_index: None,
            pending_checks: 0,
            last_tick: std::time::Instant::now(),
            scroll: 0,
            tx,
            rx,
        };
        if !app.watches.is_empty() { app.state.select(Some(0)); }
        app
    }

    fn save(&self) {
        if let Err(e) = storage::save_watches(&self.watches) {
            eprintln!("Failed to auto-save watches: {}", e);
        }
    }

    fn filtered_indices(&self) -> Vec<usize> {
        self.watches.iter().enumerate()
            .filter(|(_, w)| {
                if self.search_query.is_empty() { return true; }
                let q = self.search_query.to_lowercase();
                w.name.to_lowercase().contains(&q) || w.url.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn next(&mut self) {
        let indices = self.filtered_indices();
        let filtered_count = indices.len();
        if filtered_count == 0 { return; }
        let i = match self.state.selected() {
            Some(i) => if i >= filtered_count - 1 { 0 } else { i + 1 },
            None => 0,
        };
        self.state.select(Some(i));
        if let Some(&orig_idx) = indices.get(i) {
            self.watches[orig_idx].has_unread_change = false;
        }
    }

    fn previous(&mut self) {
        let indices = self.filtered_indices();
        let filtered_count = indices.len();
        if filtered_count == 0 { return; }
        let i = match self.state.selected() {
            Some(i) => if i == 0 { filtered_count - 1 } else { i - 1 },
            None => 0,
        };
        self.state.select(Some(i));
        if let Some(&orig_idx) = indices.get(i) {
            self.watches[orig_idx].has_unread_change = false;
        }
    }

    fn submit_watch(&mut self) {
        if !self.name_input.trim().is_empty() && !self.url_input.trim().is_empty() {
            let interval = self.interval_input.parse::<u64>().unwrap_or(3600);
            let mode = match self.mode_selection {
                1 => TrackingMode::Price { selector: None },
                2 => TrackingMode::Availability { in_stock_keywords: vec![], out_of_stock_keywords: vec![] },
                3 => TrackingMode::Keyword { 
                    keywords: self.advanced_input.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
                },
                4 => TrackingMode::HtmlSection { selector: self.advanced_input.trim().to_string() },
                _ => TrackingMode::FullPage,
            };
            if let Some(index) = self.editing_watch_index {
                if index < self.watches.len() {
                    let watch = &mut self.watches[index];
                    watch.name = self.name_input.clone();
                    watch.url = self.url_input.clone();
                    watch.interval_seconds = interval;
                    watch.mode = mode;
                }
            } else {
                let mut new_watch = Watch::new(self.name_input.clone(), self.url_input.clone(), mode);
                new_watch.interval_seconds = interval;
                self.watches.push(new_watch);
                self.state.select(Some(self.watches.len() - 1));
            }
            self.save();
        }
        self.reset_input();
    }

    fn reset_input(&mut self) {
        self.name_input.clear();
        self.url_input.clear();
        self.advanced_input.clear();
        self.interval_input.clear();
        self.mode_selection = 0;
        self.editing_watch_index = None;
        self.input_mode = InputMode::Normal;
        self.input_field = InputField::Name;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    app.save();
    if let Err(err) = res { println!("{:?}", err); }
    Ok(())
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        while let Ok((idx, watch)) = app.rx.try_recv() {
            if idx < app.watches.len() { 
                app.watches[idx] = watch;
                app.save();
            }
            app.pending_checks = app.pending_checks.saturating_sub(1);
        }

        if app.last_tick.elapsed() >= std::time::Duration::from_secs(5) {
            app.last_tick = std::time::Instant::now();
            let now = chrono::Utc::now();
            for (i, watch) in app.watches.iter().enumerate() {
                let should_check = match watch.last_checked { Some(last) => (now - last).num_seconds() >= watch.interval_seconds as i64, None => true };
                if should_check {
                    app.pending_checks += 1;
                    let mut watch_clone = watch.clone();
                    let tx = app.tx.clone();
                    tokio::spawn(async move { let _ = checker::check_watch(&mut watch_clone).await; let _ = tx.send((i, watch_clone)); });
                }
            }
        }

        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(if app.search_query.is_empty() && !matches!(app.input_mode, InputMode::Search) { 0 } else { 3 }),
                    Constraint::Min(0),
                    Constraint::Length(3)
                ].as_ref())
                .split(size);

            if !app.search_query.is_empty() || matches!(app.input_mode, InputMode::Search) {
                let search_style = if let InputMode::Search = app.input_mode { Style::default().fg(Color::Cyan) } else { Style::default() };
                let search_bar = Paragraph::new(format!(" {}", app.search_query))
                    .block(Block::default().borders(Borders::ALL).title(" Search (Name or URL) ").border_style(search_style));
                f.render_widget(search_bar, chunks[0]);
            }

            let filtered_indices = app.filtered_indices();
            let items: Vec<ListItem> = filtered_indices.iter().map(|&idx| {
                let w = &app.watches[idx];
                let mut title_spans = vec![Span::styled(&w.name, Style::default().add_modifier(Modifier::BOLD)), Span::raw(format!(" ({})", w.url))];
                if w.has_unread_change { title_spans.push(Span::raw(" ")); title_spans.push(Span::styled("● NEW", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))); }
                let mut lines = vec![Line::from(title_spans), Line::from(format!("  Mode: {:?} | Interval: {}s", w.mode, w.interval_seconds))];
                if let Some(last) = w.last_checked { lines.push(Line::from(format!("  Last checked: {}", last.format("%Y-%m-%d %H:%M:%S")))); }
                if let Some(success) = w.last_success { lines.push(Line::from(vec![Span::raw("  Last success: "), Span::styled(success.format("%Y-%m-%d %H:%M:%S").to_string(), Style::default().fg(Color::Green))])); }
                if let Some(val) = &w.last_value {
                    let snippet = if val.chars().count() > 50 { format!("{}...", val.chars().take(50).collect::<String>()) } else { val.clone() };
                    if w.has_unread_change && w.previous_value.is_some() {
                        let prev_val = w.previous_value.as_ref().unwrap();
                        let prev_snippet = if prev_val.chars().count() > 50 { format!("{}...", prev_val.chars().take(50).collect::<String>()) } else { prev_val.clone() };
                        lines.push(Line::from(vec![Span::raw("  Change: "), Span::styled(prev_snippet, Style::default().fg(Color::Red).add_modifier(Modifier::CROSSED_OUT)), Span::raw(" -> "), Span::styled(snippet, Style::default().fg(Color::Green))]));
                    } else { lines.push(Line::from(vec![Span::raw("  Value: "), Span::styled(snippet, Style::default().fg(Color::Cyan))])); }
                }
                if let Some(err) = &w.last_error { lines.push(Line::from(vec![Span::raw("  Error: "), Span::styled(err, Style::default().fg(Color::Red))])); }
                ListItem::new(lines).style(Style::default().fg(Color::White))
            }).collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(format!(" Watches ({}/{}) ", filtered_indices.len(), app.watches.len())))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray));
            f.render_stateful_widget(list, chunks[1], &mut app.state);

            let status_widget = if app.pending_checks > 0 {
                let text = format!("Checking {} watch(es)... (UI is responsive)", app.pending_checks);
                Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Status").border_style(Style::default().fg(Color::Cyan)))
            } else {
                let (text, color) = match app.input_mode {
                    InputMode::Normal => ("Press 'q' to quit, 'n' to add, 'e' to edit, 'c' to check, 'd' to delete, '/' to search.".to_string(), Color::White),
                    InputMode::Editing => ("Editing: 'Enter' to next/submit, 'Esc' to cancel.".to_string(), Color::Yellow),
                    InputMode::Details => ("Details: 'Esc' to go back, 'Up'/'Down' to scroll.".to_string(), Color::Blue),
                    InputMode::ConfirmDelete => ("Are you sure? 'y' to delete, 'n' to cancel.".to_string(), Color::Red),
                    InputMode::Search => ("Searching: Type to filter, 'Enter' or 'Esc' to finish.".to_string(), Color::Cyan),
                };
                Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Status").border_style(Style::default().fg(color)))
            };
            f.render_widget(status_widget, chunks[2]);

            if let InputMode::ConfirmDelete = app.input_mode {
                let area = centered_rect(40, 20, size); f.render_widget(Clear, area);
                let block = Block::default().title(" Confirm Delete ").borders(Borders::ALL).border_style(Style::default().fg(Color::Red));
                let text = vec![Line::from(""), Line::from("  Delete this watch?").alignment(ratatui::layout::Alignment::Center), Line::from(""), Line::from("  (y) Yes  /  (n) No").alignment(ratatui::layout::Alignment::Center)];
                f.render_widget(Paragraph::new(text).block(block), area);
            }

            if let InputMode::Details = app.input_mode {
                if let Some(i) = app.state.selected() {
                    if let Some(&orig_idx) = filtered_indices.get(i) {
                        let w = &app.watches[orig_idx];
                        let area = centered_rect(80, 80, size); f.render_widget(Clear, area);
                        let mut details_text = vec![
                            Line::from(vec![Span::styled("URL: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(&w.url)]),
                            Line::from(vec![Span::styled("Mode: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(format!("{:?}", w.mode))]),
                            Line::from(vec![Span::styled("Interval: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(format!("{} seconds", w.interval_seconds))]),
                            Line::from(vec![
                                Span::styled("Stats: ", Style::default().add_modifier(Modifier::BOLD)),
                                Span::raw(format!("{} checks, {} successes", w.total_checks, w.total_successes)),
                                Span::raw(if w.total_checks > 0 { format!(" ({:.1}% success rate)", (w.total_successes as f64 / w.total_checks as f64) * 100.0) } else { String::new() }),
                            ]),
                            Line::from(""), Span::styled("Last Extracted Value:", Style::default().add_modifier(Modifier::BOLD)).into(), Line::from("----------------------"),
                        ];
                        if let Some(val) = &w.last_value { details_text.push(Line::from(val.clone())); } else { details_text.push(Line::from("No data collected yet.")); }
                        if let Some(err) = &w.last_error {
                            details_text.push(Line::from(""));
                            details_text.push(Span::styled("Last Error:", Style::default().add_modifier(Modifier::BOLD).fg(Color::Red)).into());
                            details_text.push(Line::from(err.clone()));
                        }
                        if !w.error_log.is_empty() {
                            details_text.push(Line::from(""));
                            details_text.push(Span::styled("Error History (Last 10):", Style::default().add_modifier(Modifier::BOLD)).into());
                            for (time, msg) in w.error_log.iter().rev() {
                                details_text.push(Line::from(format!("  {} - {}", time.format("%H:%M:%S"), msg)));
                            }
                        }
                        f.render_widget(Paragraph::new(details_text).block(Block::default().title(format!(" Details: {} ", w.name)).borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan))).wrap(ratatui::widgets::Wrap { trim: true }).scroll((app.scroll, 0)), area);
                    }
                }
            }

            if let InputMode::Editing = app.input_mode {
                let show_advanced = app.mode_selection == 3 || app.mode_selection == 4;
                let title = if app.editing_watch_index.is_some() { "Edit Watch" } else { "Add New Watch" };
                let area = centered_rect(60, if show_advanced { 80 } else { 65 }, size);
                f.render_widget(Clear, area);
                f.render_widget(Block::default().title(title).borders(Borders::ALL), area);
                let mut constraints = vec![Constraint::Length(3), Constraint::Length(3), Constraint::Length(3), Constraint::Length(7)];
                if show_advanced { constraints.push(Constraint::Length(3)); }
                let popup_chunks = Layout::default().direction(Direction::Vertical).margin(2).constraints(constraints).split(area);
                f.render_widget(Paragraph::new(app.name_input.clone()).block(Block::default().title("Name").borders(Borders::ALL).style(if let InputField::Name = app.input_field { Style::default().fg(Color::Yellow) } else { Style::default() })), popup_chunks[0]);
                f.render_widget(Paragraph::new(app.url_input.clone()).block(Block::default().title("URL").borders(Borders::ALL).style(if let InputField::Url = app.input_field { Style::default().fg(Color::Yellow) } else { Style::default() })), popup_chunks[1]);
                f.render_widget(Paragraph::new(app.interval_input.clone()).block(Block::default().title("Check Interval (seconds)").borders(Borders::ALL).style(if let InputField::Interval = app.input_field { Style::default().fg(Color::Yellow) } else { Style::default() })), popup_chunks[2]);
                let modes = vec!["[1] Full page text", "[2] Price", "[3] Availability (in stock / sold out)", "[4] Specific keyword(s)", "[5] HTML section (advanced)"];
                let mode_items: Vec<ListItem> = modes.iter().enumerate().map(|(i, m)| {
                    let style = if i == app.mode_selection { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default() };
                    ListItem::new(format!("  {}", m)).style(style)
                }).collect();
                f.render_widget(List::new(mode_items).block(Block::default().title("Tracking Mode").borders(Borders::ALL).style(if let InputField::Mode = app.input_field { Style::default().fg(Color::Yellow) } else { Style::default() })), popup_chunks[3]);
                if show_advanced {
                    let adv_title = if app.mode_selection == 3 { "Keywords (comma-separated)" } else { "CSS Selector" };
                    f.render_widget(Paragraph::new(app.advanced_input.clone()).block(Block::default().title(adv_title).borders(Borders::ALL).style(if let InputField::Advanced = app.input_field { Style::default().fg(Color::Yellow) } else { Style::default() })), popup_chunks[4]);
                }
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('/') => { app.input_mode = InputMode::Search; }
                            KeyCode::Enter => { if app.state.selected().is_some() { app.input_mode = InputMode::Details; app.scroll = 0; } }
                            KeyCode::Char('n') => { app.input_mode = InputMode::Editing; app.input_field = InputField::Name; app.editing_watch_index = None; app.name_input.clear(); app.url_input.clear(); app.interval_input.clear(); app.advanced_input.clear(); app.mode_selection = 0; }
                            KeyCode::Char('e') => {
                                let indices = app.filtered_indices();
                                if let Some(i) = app.state.selected() {
                                    if let Some(&orig_idx) = indices.get(i) {
                                        let watch = &app.watches[orig_idx];
                                        app.input_mode = InputMode::Editing; app.input_field = InputField::Name; app.editing_watch_index = Some(orig_idx);
                                        app.name_input = watch.name.clone(); app.url_input = watch.url.clone(); app.interval_input = watch.interval_seconds.to_string();
                                        app.mode_selection = match watch.mode { TrackingMode::FullPage => 0, TrackingMode::Price { .. } => 1, TrackingMode::Availability { .. } => 2, TrackingMode::Keyword { .. } => 3, TrackingMode::HtmlSection { .. } => 4 };
                                        app.advanced_input = match &watch.mode { TrackingMode::Keyword { keywords } => keywords.join(", "), TrackingMode::HtmlSection { selector } => selector.clone(), _ => String::new() };
                                    }
                                }
                            }
                            KeyCode::Char('c') => {
                                let indices = app.filtered_indices();
                                if let Some(i) = app.state.selected() {
                                    if let Some(&orig_idx) = indices.get(i) {
                                        app.pending_checks += 1;
                                        let mut watch_clone = app.watches[orig_idx].clone();
                                        let tx = app.tx.clone();
                                        tokio::spawn(async move { let _ = checker::check_watch(&mut watch_clone).await; let _ = tx.send((orig_idx, watch_clone)); });
                                    }
                                }
                            }
                            KeyCode::Char('d') => { if app.state.selected().is_some() { app.input_mode = InputMode::ConfirmDelete; } }
                            KeyCode::Down => app.next(), KeyCode::Up => app.previous(), _ => {}
                        },
                        InputMode::Search => match key.code {
                            KeyCode::Enter | KeyCode::Esc => { app.input_mode = InputMode::Normal; }
                            KeyCode::Char(c) => { app.search_query.push(c); app.state.select(Some(0)); }
                            KeyCode::Backspace => { app.search_query.pop(); app.state.select(Some(0)); }
                            _ => {}
                        },
                        InputMode::ConfirmDelete => match key.code {
                            KeyCode::Char('y') | KeyCode::Enter => {
                                let indices = app.filtered_indices();
                                if let Some(i) = app.state.selected() {
                                    if let Some(&orig_idx) = indices.get(i) {
                                        app.watches.remove(orig_idx); app.save();
                                        let new_indices = app.filtered_indices();
                                        if new_indices.is_empty() { app.state.select(None); }
                                        else { app.state.select(Some(0)); }
                                    }
                                }
                                app.input_mode = InputMode::Normal;
                            }
                            KeyCode::Char('n') | KeyCode::Esc => { app.input_mode = InputMode::Normal; }
                            _ => {}
                        },
                        InputMode::Details => match key.code { KeyCode::Esc | KeyCode::Char('q') => app.input_mode = InputMode::Normal, KeyCode::Down => app.scroll = app.scroll.saturating_add(1), KeyCode::Up => app.scroll = app.scroll.saturating_sub(1), _ => {} },
                        InputMode::Editing => match key.code {
                            KeyCode::Esc => app.reset_input(),
                            KeyCode::Enter => match app.input_field {
                                InputField::Name => app.input_field = InputField::Url,
                                InputField::Url => app.input_field = InputField::Interval,
                                InputField::Interval => app.input_field = InputField::Mode,
                                InputField::Mode => { if app.mode_selection == 3 || app.mode_selection == 4 { app.input_field = InputField::Advanced; } else { app.submit_watch(); } }
                                InputField::Advanced => app.submit_watch(),
                            },
                            KeyCode::Down if matches!(app.input_field, InputField::Mode) => { app.mode_selection = (app.mode_selection + 1) % 5; }
                            KeyCode::Up if matches!(app.input_field, InputField::Mode) => { app.mode_selection = if app.mode_selection == 0 { 4 } else { app.mode_selection - 1 }; }
                            KeyCode::Char(c) => match app.input_field { InputField::Name => app.name_input.push(c), InputField::Url => app.url_input.push(c), InputField::Interval => if c.is_digit(10) { app.interval_input.push(c); }, InputField::Advanced => app.advanced_input.push(c), InputField::Mode => {} },
                            KeyCode::Backspace => match app.input_field { InputField::Name => { app.name_input.pop(); } InputField::Url => { app.url_input.pop(); } InputField::Interval => { app.interval_input.pop(); } InputField::Advanced => { app.advanced_input.pop(); } InputField::Mode => {} },
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2)].as_ref()).split(r);
    Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2)].as_ref()).split(popup_layout[1])[1]
}
