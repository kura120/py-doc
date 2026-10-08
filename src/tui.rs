use anyhow::Result;
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use tui_input::{Input, backend::crossterm::EventHandler};

use crate::{Args, LayoutKind, ThemeKind};

const LAYOUT_OPTIONS: [&str; 3] = ["Classic", "Minimal", "Modern"];
const THEME_OPTIONS: [&str; 4] = ["Auto", "Dark", "Light", "Slate"];

// How many entries the folder dropdown shows before scrolling, and the
// resulting box height (+2 for its own top/bottom border).
const DROPDOWN_CAPACITY: usize = 6;
const DROPDOWN_HEIGHT: u16 = DROPDOWN_CAPACITY as u16 + 2;

// Field indices. 7 is the Generate button.
const FIELD_COUNT: usize = 8;
const F_SRC: usize = 0;
const F_OUT: usize = 1;
const F_NAME: usize = 2;
const F_VERSION: usize = 3;
const F_TEMPLATES: usize = 4;
const F_LAYOUT: usize = 5;
const F_THEME: usize = 6;
const F_GENERATE: usize = 7;

fn is_path_field(focus: usize) -> bool {
    matches!(focus, F_SRC | F_OUT | F_TEMPLATES)
}

struct TextRow {
    input: Input,
    hint: &'static str,
}

impl TextRow {
    fn new(hint: &'static str) -> Self {
        Self {
            input: Input::default(),
            hint,
        }
    }

    fn with_value(hint: &'static str, value: &str) -> Self {
        Self {
            input: Input::new(value.to_string()),
            hint,
        }
    }
}

/// A text field paired with a live "browse this directory" dropdown.
/// `current_dir` is always the directory whose subfolders `entries`
/// reflects; it's recomputed from whatever the input currently contains
/// (falling back to the nearest valid parent, then cwd) every time the
/// text changes or the user navigates with ←/→.
struct PathField {
    row: TextRow,
    current_dir: PathBuf,
    entries: Vec<String>, // ".." (if not at filesystem root) followed by subfolder names, sorted
    highlighted: usize,
}

impl PathField {
    fn new(hint: &'static str) -> Self {
        let mut field = Self {
            row: TextRow::new(hint),
            current_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            entries: Vec::new(),
            highlighted: 0,
        };
        field.refresh();
        field
    }

    /// Recompute `current_dir` and `entries` from whatever the field's text
    /// currently says. Called after every keystroke and after every
    /// navigation step, so the dropdown never drifts out of sync with the
    /// text the user sees.
    fn refresh(&mut self) {
        let raw = self.row.input.value().trim();

        let base = if raw.is_empty() {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        } else {
            let candidate = PathBuf::from(raw);
            if candidate.is_dir() {
                candidate
            } else if let Some(parent) = candidate.parent().filter(|p| p.is_dir()) {
                parent.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        };

        let mut entries = Vec::new();
        if base.parent().is_some() {
            entries.push("..".to_string());
        }

        if let Ok(read_dir) = std::fs::read_dir(&base) {
            let mut names: Vec<String> = read_dir
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|name| !name.starts_with('.'))
                .collect();
            names.sort_by_key(|n| n.to_lowercase());
            entries.extend(names);
        }

        self.current_dir = base;
        self.entries = entries;
        self.highlighted = 0;
    }

    fn move_highlight(&mut self, delta: i32) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len() as i32;
        let mut next = self.highlighted as i32 + delta;
        next = next.rem_euclid(len);
        self.highlighted = next as usize;
    }

    /// → : descend into whatever's highlighted (".." ascends instead).
    fn descend(&mut self) {
        let Some(entry) = self.entries.get(self.highlighted).cloned() else {
            return;
        };

        let new_dir = if entry == ".." {
            match self.current_dir.parent() {
                Some(p) => p.to_path_buf(),
                None => return,
            }
        } else {
            self.current_dir.join(&entry)
        };

        self.row.input = Input::new(new_dir.display().to_string());
        self.refresh();
    }

    /// ← : go up one level, regardless of what's highlighted.
    fn ascend(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            let new_dir = parent.to_path_buf();
            self.row.input = Input::new(new_dir.display().to_string());
            self.refresh();
        }
    }
}

struct App {
    src: PathField,
    out: PathField,
    name: TextRow,
    version: TextRow,
    templates: PathField,
    layout_idx: usize,
    theme_idx: usize,
    focus: usize,
    name_touched: bool,
    error: Option<String>,
}

impl App {
    fn new() -> Self {
        Self {
            src: PathField::new("(required) path to your Python project"),
            out: PathField::new("(required) e.g. ./docs"),
            name: TextRow::new("(required) auto-filled from source folder name"),
            version: TextRow::with_value("(optional, default shown)", "1.0.0"),
            templates: PathField::new("(optional) blank = use bundled theme"),
            layout_idx: 0,
            theme_idx: 0,
            focus: F_SRC,
            name_touched: false,
            error: None,
        }
    }

    fn active_path_field_mut(&mut self) -> Option<&mut PathField> {
        match self.focus {
            F_SRC => Some(&mut self.src),
            F_OUT => Some(&mut self.out),
            F_TEMPLATES => Some(&mut self.templates),
            _ => None,
        }
    }

    fn maybe_autofill_name(&mut self) {
        if self.name_touched {
            return;
        }
        let src_val = self.src.row.input.value().trim();
        if src_val.is_empty() {
            return;
        }
        if let Some(folder) = Path::new(src_val).file_name().and_then(|f| f.to_str()) {
            self.name.input = Input::new(folder.to_string());
        }
    }

    fn validate(&mut self) -> bool {
        self.error = None;

        let src_val = self.src.row.input.value().trim().to_string();
        if src_val.is_empty() {
            self.error = Some("Source directory is required.".into());
            self.focus = F_SRC;
            return false;
        }
        if !Path::new(&src_val).is_dir() {
            self.error = Some(format!("'{}' is not a directory.", src_val));
            self.focus = F_SRC;
            return false;
        }

        if self.out.row.input.value().trim().is_empty() {
            self.error = Some("Output directory is required.".into());
            self.focus = F_OUT;
            return false;
        }

        if self.name.input.value().trim().is_empty() {
            self.error = Some("Package name is required.".into());
            self.focus = F_NAME;
            return false;
        }

        let templates_val = self.templates.row.input.value().trim().to_string();
        if !templates_val.is_empty() && !Path::new(&templates_val).is_dir() {
            self.error = Some(format!(
                "Templates directory '{}' does not exist.",
                templates_val
            ));
            self.focus = F_TEMPLATES;
            return false;
        }

        true
    }

    fn to_args(&self) -> Args {
        let version = {
            let v = self.version.input.value().trim();
            if v.is_empty() {
                "1.0.0".to_string()
            } else {
                v.to_string()
            }
        };

        let templates_val = self.templates.row.input.value().trim();
        let templates = if templates_val.is_empty() {
            None
        } else {
            Some(templates_val.to_string())
        };

        let layout = match self.layout_idx {
            0 => LayoutKind::Classic,
            1 => LayoutKind::Minimal,
            _ => LayoutKind::Modern,
        };
        let theme = match self.theme_idx {
            0 => ThemeKind::Auto,
            1 => ThemeKind::Dark,
            2 => ThemeKind::Light,
            _ => ThemeKind::Slate,
        };

        Args {
            src: self.src.row.input.value().trim().to_string(),
            out: self.out.row.input.value().trim().to_string(),
            name: self.name.input.value().trim().to_string(),
            doc_version: version,
            templates,
            layout,
            theme,
            exclude: Vec::new(),
            source_url: None,
            strict: false,
            clean: false,
        }
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}

/// Puts the terminal back to normal on every way out of the wizard,
/// including early returns on I/O errors.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

pub fn run_tui() -> Result<Option<Args>> {
    // The release profile aborts on panic, which skips destructors, so the
    // panic hook has to restore the terminal as well.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    enable_raw_mode()?;
    let _guard = TerminalGuard;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    run_app(&mut terminal)
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<Option<Args>> {
    let mut app = App::new();

    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Esc => return Ok(None),

                // Tab/Shift+Tab always move focus between fields, dropdown
                // browsing or not — this is the guaranteed way out of a
                // path field regardless of what its dropdown is doing.
                KeyCode::Tab => {
                    app.focus = (app.focus + 1) % FIELD_COUNT;
                    app.error = None;
                }
                KeyCode::BackTab => {
                    app.focus = (app.focus + FIELD_COUNT - 1) % FIELD_COUNT;
                    app.error = None;
                }

                // Up/Down: browse the folder dropdown if one's showing,
                // otherwise fall back to moving focus between fields.
                KeyCode::Down => {
                    if is_path_field(app.focus)
                        && let Some(field) = app.active_path_field_mut()
                        && !field.entries.is_empty()
                    {
                        field.move_highlight(1);
                        continue;
                    }
                    app.focus = (app.focus + 1) % FIELD_COUNT;
                    app.error = None;
                }
                KeyCode::Up => {
                    if is_path_field(app.focus)
                        && let Some(field) = app.active_path_field_mut()
                        && !field.entries.is_empty()
                    {
                        field.move_highlight(-1);
                        continue;
                    }
                    app.focus = (app.focus + FIELD_COUNT - 1) % FIELD_COUNT;
                    app.error = None;
                }

                KeyCode::Right => match app.focus {
                    F_LAYOUT => app.layout_idx = (app.layout_idx + 1) % LAYOUT_OPTIONS.len(),
                    F_THEME => app.theme_idx = (app.theme_idx + 1) % THEME_OPTIONS.len(),
                    f if is_path_field(f) => {
                        if let Some(field) = app.active_path_field_mut() {
                            field.descend();
                        }
                        if app.focus == F_SRC {
                            app.maybe_autofill_name();
                        }
                    }
                    _ => {}
                },
                KeyCode::Left => match app.focus {
                    F_LAYOUT => {
                        app.layout_idx =
                            (app.layout_idx + LAYOUT_OPTIONS.len() - 1) % LAYOUT_OPTIONS.len()
                    }
                    F_THEME => {
                        app.theme_idx =
                            (app.theme_idx + THEME_OPTIONS.len() - 1) % THEME_OPTIONS.len()
                    }
                    f if is_path_field(f) => {
                        if let Some(field) = app.active_path_field_mut() {
                            field.ascend();
                        }
                        if app.focus == F_SRC {
                            app.maybe_autofill_name();
                        }
                    }
                    _ => {}
                },

                KeyCode::Enter => {
                    if app.focus == F_GENERATE {
                        if app.validate() {
                            return Ok(Some(app.to_args()));
                        }
                    } else {
                        app.focus = (app.focus + 1) % FIELD_COUNT;
                    }
                }

                _ => match app.focus {
                    F_SRC => {
                        app.src.row.input.handle_event(&Event::Key(key));
                        app.src.refresh();
                        app.maybe_autofill_name();
                    }
                    F_OUT => {
                        app.out.row.input.handle_event(&Event::Key(key));
                        app.out.refresh();
                    }
                    F_NAME => {
                        app.name.input.handle_event(&Event::Key(key));
                        if !app.name.input.value().trim().is_empty() {
                            app.name_touched = true;
                        }
                    }
                    F_VERSION => {
                        app.version.input.handle_event(&Event::Key(key));
                    }
                    F_TEMPLATES => {
                        app.templates.row.input.handle_event(&Event::Key(key));
                        app.templates.refresh();
                    }
                    _ => {}
                },
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let area = f.area();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(area);

    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " py-doc ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  Documentation Generator — Setup"),
    ]));
    f.render_widget(title, outer[0]);

    // Build the form's row constraints dynamically: each path field gets an
    // extra dropdown row appended right after it, but only while it's the
    // focused field with something to show.
    let src_dropdown = app.focus == F_SRC && !app.src.entries.is_empty();
    let out_dropdown = app.focus == F_OUT && !app.out.entries.is_empty();
    let templates_dropdown = app.focus == F_TEMPLATES && !app.templates.entries.is_empty();

    let mut constraints = vec![Constraint::Length(3)]; // src
    if src_dropdown {
        constraints.push(Constraint::Length(DROPDOWN_HEIGHT));
    }
    constraints.push(Constraint::Length(3)); // out
    if out_dropdown {
        constraints.push(Constraint::Length(DROPDOWN_HEIGHT));
    }
    constraints.push(Constraint::Length(3)); // name
    constraints.push(Constraint::Length(3)); // version
    constraints.push(Constraint::Length(3)); // templates
    if templates_dropdown {
        constraints.push(Constraint::Length(DROPDOWN_HEIGHT));
    }
    constraints.push(Constraint::Length(3)); // layout
    constraints.push(Constraint::Length(3)); // theme
    constraints.push(Constraint::Length(3)); // generate button

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(outer[1]);

    let mut i = 0;
    render_path_field(
        f,
        rows[i],
        "Source directory",
        &app.src,
        app.focus == F_SRC,
        true,
    );
    i += 1;
    if src_dropdown {
        render_dropdown(f, rows[i], &app.src);
        i += 1;
    }

    render_path_field(
        f,
        rows[i],
        "Output directory",
        &app.out,
        app.focus == F_OUT,
        true,
    );
    i += 1;
    if out_dropdown {
        render_dropdown(f, rows[i], &app.out);
        i += 1;
    }

    render_text_row(
        f,
        rows[i],
        "Package name",
        &app.name,
        app.focus == F_NAME,
        true,
    );
    i += 1;
    render_text_row(
        f,
        rows[i],
        "Version",
        &app.version,
        app.focus == F_VERSION,
        false,
    );
    i += 1;

    render_path_field(
        f,
        rows[i],
        "Templates directory",
        &app.templates,
        app.focus == F_TEMPLATES,
        false,
    );
    i += 1;
    if templates_dropdown {
        render_dropdown(f, rows[i], &app.templates);
        i += 1;
    }

    render_select_row(
        f,
        rows[i],
        "Layout",
        &LAYOUT_OPTIONS,
        app.layout_idx,
        app.focus == F_LAYOUT,
    );
    i += 1;
    render_select_row(
        f,
        rows[i],
        "Color theme",
        &THEME_OPTIONS,
        app.theme_idx,
        app.focus == F_THEME,
    );
    i += 1;

    let generate_focused = app.focus == F_GENERATE;
    let generate_style = if generate_focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let generate_btn = Paragraph::new(Line::from(Span::styled(
        "  ▶ Generate Documentation  ",
        generate_style,
    )))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(if generate_focused {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            }),
    );
    f.render_widget(generate_btn, rows[i]);

    let footer_line = if let Some(err) = &app.error {
        Line::from(Span::styled(
            format!(" ⚠ {}", err),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else if is_path_field(app.focus) {
        Line::from(Span::styled(
            " ↑/↓: browse folders   →: open selected   ←: up a level   Tab: next field   Esc: quit",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(Span::styled(
            " ↑/↓ Tab: move field   ←/→: change option   Enter: next / generate   Esc: quit",
            Style::default().fg(Color::DarkGray),
        ))
    };
    let footer = Paragraph::new(footer_line);
    f.render_widget(footer, outer[2]);
}

fn render_text_row(
    f: &mut Frame,
    area: Rect,
    label: &str,
    row: &TextRow,
    focused: bool,
    required: bool,
) {
    let (title, text_style, cursor) = field_visuals(label, required, focused, row);
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let display_text = if row.input.value().is_empty() {
        row.hint.to_string()
    } else {
        row.input.value().to_string()
    };
    let paragraph = Paragraph::new(display_text).style(text_style).block(block);
    f.render_widget(paragraph, area);

    if let Some((x, y)) = cursor {
        f.set_cursor_position((area.x + 1 + x.min(area.width.saturating_sub(2)), area.y + y));
    }
}

fn render_path_field(
    f: &mut Frame,
    area: Rect,
    label: &str,
    field: &PathField,
    focused: bool,
    required: bool,
) {
    let (title, text_style, cursor) = field_visuals(label, required, focused, &field.row);
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let display_text = if field.row.input.value().is_empty() {
        field.row.hint.to_string()
    } else {
        field.row.input.value().to_string()
    };
    let paragraph = Paragraph::new(display_text).style(text_style).block(block);
    f.render_widget(paragraph, area);

    if let Some((x, y)) = cursor {
        f.set_cursor_position((area.x + 1 + x.min(area.width.saturating_sub(2)), area.y + y));
    }
}

/// Shared title/style/cursor-offset logic for both plain text rows and path
/// fields, so the two always look and behave the same way.
fn field_visuals(
    label: &str,
    required: bool,
    focused: bool,
    row: &TextRow,
) -> (Span<'static>, Style, Option<(u16, u16)>) {
    let marker = if required { " *" } else { "" };
    let title_style = if required {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Gray)
    };
    let title = Span::styled(format!(" {}{} ", label, marker), title_style);

    let text_style = if row.input.value().is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let cursor = if focused {
        Some((row.input.visual_cursor() as u16, 1))
    } else {
        None
    };

    (title, text_style, cursor)
}

fn render_dropdown(f: &mut Frame, area: Rect, field: &PathField) {
    let capacity = (area.height.saturating_sub(2) as usize).max(1);
    let (start, end) = if field.entries.len() <= capacity {
        (0, field.entries.len())
    } else {
        let start = field
            .highlighted
            .saturating_sub(capacity - 1)
            .min(field.entries.len() - capacity);
        (start, start + capacity)
    };

    let lines: Vec<Line> = field.entries[start..end]
        .iter()
        .enumerate()
        .map(|(offset, entry)| {
            let real_idx = start + offset;
            let is_up = entry == "..";
            let label = if is_up {
                "⬆  .. (up one level)".to_string()
            } else {
                format!("📁 {}", entry)
            };
            let style = if real_idx == field.highlighted {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if is_up {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(label, style))
        })
        .collect();

    let position = if field.entries.len() > capacity {
        format!(" [{}/{}] ", field.highlighted + 1, field.entries.len())
    } else {
        String::new()
    };
    let title = format!(" {}{} ", field.current_dir.display(), position);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(title, Style::default().fg(Color::Gray)));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn render_select_row(
    f: &mut Frame,
    area: Rect,
    label: &str,
    options: &[&str],
    selected: usize,
    focused: bool,
) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut spans = Vec::new();
    for (i, opt) in options.iter().enumerate() {
        if i == selected {
            spans.push(Span::styled(
                format!(" {} ", opt),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {} ", opt),
                Style::default().fg(Color::DarkGray),
            ));
        }
        if i != options.len() - 1 {
            spans.push(Span::raw("  "));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {} (optional) ", label),
            Style::default().fg(Color::Gray),
        ));

    let paragraph = Paragraph::new(Line::from(spans)).block(block);
    f.render_widget(paragraph, area);
}
