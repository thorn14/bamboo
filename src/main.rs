mod app;
mod config;
mod events;
mod pane;
mod pty;
mod ui;

use std::io::{self, Write};

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    event::{EnableMouseCapture, DisableMouseCapture},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use app::AppState;
use config::Config;
use events::{AppEvent, run_event_loop};
use pane::Pane;
use pty::{PtyEvent, launch_reader_task, spawn_pty};

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "--config")
        .map(|w| w[1].clone());

    let config = Config::load(config_path.as_deref())?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let size = terminal.size()?;

    let initial_cols = size.width.saturating_sub(2).max(10);
    let n_panes = config.panes.len().max(1);
    let initial_rows = (size.height / n_panes as u16).saturating_sub(2).max(5);

    let (unified_tx, unified_rx) = mpsc::unbounded_channel::<AppEvent>();

    let mut panes = Vec::new();
    for (i, pane_config) in config.panes.iter().enumerate() {
        let spawned = spawn_pty(pane_config, &config.default_shell, initial_cols, initial_rows)?;

        let parser = std::sync::Arc::new(parking_lot::Mutex::new(vt100::Parser::new(
            initial_rows,
            initial_cols,
            1000,
        )));

        let (pty_tx, pty_rx) = mpsc::unbounded_channel::<PtyEvent>();
        launch_reader_task(spawned.reader, parser.clone(), pty_tx);

        let pane = Pane::new(
            i,
            pane_config.name.clone(),
            spawned.master,
            spawned.writer,
            parser,
            pty_rx,
            initial_cols,
            initial_rows,
        );
        panes.push(pane);
    }

    for pane in &mut panes {
        if let Some(mut pty_rx) = pane.pty_rx.take() {
            let tx = unified_tx.clone();
            let pane_id = pane.id;
            tokio::spawn(async move {
                while let Some(event) = pty_rx.recv().await {
                    let is_closed = matches!(event, PtyEvent::Closed);
                    if tx.send(AppEvent::PtyOutput { pane_id, event }).is_err() {
                        break;
                    }
                    if is_closed {
                        break;
                    }
                }
            });
        }
    }

    let mut app = AppState::new(panes, config.layout, config.default_shell);
    app.term_cols = size.width;
    app.term_rows = size.height;

    run_event_loop(&mut terminal, &mut app, unified_rx, unified_tx).await?;

    // Restore terminal and exit immediately so Ctrl+Q alone is enough.
    // (Otherwise the runtime waits for PTY reader tasks and the process can hang until Ctrl+C.)
    drop(terminal); // flush and release backend
    let _ = crossterm::terminal::disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
    let _ = stdout.flush();
    std::process::exit(0);
}
