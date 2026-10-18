use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crossterm::{cursor, event, execute, queue};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen};
use wtm_ui::dashboard::{DashboardView, DisplayItem, RefreshTracker};
use wtm_ui::render_buffer::RenderBuffer;
use wtm_ui::rendering::{compose_dashboard, render_row, DashboardFrame, RowOptions};
use wtm_ui::{Monitor, MonitorUpdate, Service};

#[cfg(windows)]
mod native;

struct Terminal {
    output: io::Stdout,
    raw: bool,
    screen: bool,
    cursor: bool,
    wrap: bool,
    #[cfg(windows)]
    modes: native::ConsoleModes,
}

impl Terminal {
    fn enter() -> io::Result<Self> {
        let mut session = Self { output: io::stdout(), raw: false, screen: false, cursor: false, wrap: false,
            #[cfg(windows)]
            modes: native::ConsoleModes::capture()?,
        };
        if !terminal::is_raw_mode_enabled()? {
            session.raw = true;
            terminal::enable_raw_mode()?;
        }
        session.screen = true;
        execute!(session.output, EnterAlternateScreen)?;
        session.cursor = true;
        execute!(session.output, cursor::Hide)?;
        session.wrap = true;
        execute!(session.output, DisableLineWrap, ResetColor, SetAttribute(Attribute::Reset), Clear(ClearType::All))?;
        Ok(session)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = execute!(self.output, SetAttribute(Attribute::Reset), ResetColor);
        if self.wrap { let _ = execute!(self.output, EnableLineWrap); }
        if self.cursor { let _ = execute!(self.output, cursor::Show); }
        if self.screen { let _ = execute!(self.output, LeaveAlternateScreen); }
        if self.raw { let _ = terminal::disable_raw_mode(); }
        #[cfg(windows)]
        self.modes.restore();
    }
}

#[derive(Default)]
struct View {
    latest: Option<Arc<MonitorUpdate>>,
    refresh: RefreshTracker,
    scroll: usize,
    previous: Option<RenderBuffer>,
}

impl View {
    fn frame(&mut self, columns: usize, rows: usize) -> DashboardFrame {
        let mut view = DashboardView::scanning();
        let mut polling = Duration::ZERO;
        let activities = self.latest.as_ref().map_or(&[][..], |update| update.scan_activities.as_slice());
        let refreshing = self.refresh.update(activities, Instant::now());
        if let Some(update) = &self.latest {
            view = DashboardView::from_scan(&update.snapshot, &update.scan_targets, &Default::default(), true);
            if let Some(error) = &update.launch_error { view.launch_error(error); }
            polling = update.polling_time_last_minute;
        }
        let header = render_row(&DisplayItem::Status { health: view.health, polling_time_last_minute: polling }, columns, RowOptions::default());
        let items: Vec<_> = view.items.iter().map(|item| render_row(&item.display, columns, RowOptions {
            refreshing: refreshing.contains(&item.key), ..RowOptions::default()
        })).collect();
        let frame = compose_dashboard(&header, &items, &[], columns, rows, self.scroll);
        self.scroll = frame.scroll_row;
        frame
    }

    fn key(&mut self, key: event::KeyEvent, rows: usize) -> bool {
        if key.kind == KeyEventKind::Release { return false; }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(rows.saturating_sub(2).max(1)),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(rows.saturating_sub(2).max(1)),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = usize::MAX,
            _ => {}
        }
        false
    }
}

fn present(output: &mut impl Write, buffer: &RenderBuffer) -> io::Result<()> {
    crossterm::style::force_color_output(true);
    queue!(output, SetAttribute(Attribute::Reset), ResetColor)?;
    let mut style = None;
    for row in 0..buffer.rows() {
        queue!(output, cursor::MoveTo(0, row as u16))?;
        for cell in buffer.row(row).unwrap() {
            if cell.text().is_empty() { continue; }
            let current = (cell.color, cell.flags.bold, cell.flags.underline);
            if style != Some(current) {
                queue!(output, SetAttribute(Attribute::Reset), SetForegroundColor(Color::Rgb {
                    r: cell.color.red, g: cell.color.green, b: cell.color.blue,
                }))?;
                if cell.flags.bold { queue!(output, SetAttribute(Attribute::Bold))?; }
                if cell.flags.underline { queue!(output, SetAttribute(Attribute::Underlined))?; }
                style = Some(current);
            }
            queue!(output, Print(cell.text()))?;
        }
    }
    queue!(output, SetAttribute(Attribute::Reset), ResetColor)?;
    output.flush()
}

fn watch(monitor: &Monitor) -> io::Result<()> {
    let mut terminal = Terminal::enter()?;
    let mut view = View::default();
    let mut size = terminal::size()?;
    loop {
        view.latest = monitor.latest();
        let frame = view.frame(usize::from(size.0), usize::from(size.1));
        if view.previous.as_ref() != Some(&frame.buffer) {
            present(&mut terminal.output, &frame.buffer)?;
            view.previous = Some(frame.buffer);
        }
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if view.key(key, usize::from(size.1)) => return Ok(()),
                Event::Resize(columns, rows) => {
                    size = (columns, rows);
                    view.previous = None;
                    execute!(terminal.output, Clear(ClearType::All))?;
                }
                _ => {}
            }
        }
    }
}

fn start() -> Result<(), Box<dyn std::error::Error>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("wtm-tui requires an interactive terminal for input and output".into());
    }
    let mut monitor = Service::user()?.monitor_from(std::env::current_dir()?)?;
    let result = watch(&monitor);
    let stopped = monitor.stop();
    result?;
    stopped?;
    Ok(())
}

pub fn run() -> std::process::ExitCode {
    match start() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => { eprintln!("Error: {error}"); std::process::ExitCode::FAILURE }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
