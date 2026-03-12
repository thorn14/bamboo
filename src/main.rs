mod app;
mod config;
mod events;
mod pane;
mod pty;
mod terminal;
mod ui;
mod worktree;

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

/// RAII guard that cleans up a created worktree on drop if it was not taken.
/// On drop: remove silently when there are no changes; otherwise print path/branch
/// for manual cleanup. The interactive prompt runs only when the guard is disarmed
/// via `take()` on the normal shutdown path.
struct WorktreeGuard(Option<worktree::Worktree>);

impl WorktreeGuard {
    fn get(&self) -> Option<&worktree::Worktree> {
        self.0.as_ref()
    }

    /// Take the worktree out of the guard. Drop will then be a no-op.
    fn take(&mut self) -> Option<worktree::Worktree> {
        self.0.take()
    }
}

impl Drop for WorktreeGuard {
    fn drop(&mut self) {
        if let Some(wt) = self.0.take() {
            if wt.has_changes() {
                eprintln!();
                eprintln!(
                    "Shoot '{}' was created but bamboo exited before cleanup (branch: {}).",
                    wt.name, wt.branch
                );
                eprintln!("  Path: {}", wt.path.display());
                eprintln!("  Remove manually if desired: git worktree remove --force \"{}\"", wt.path.display());
            } else {
                let _ = wt.remove();
            }
        }
    }
}

/// Parse CLI arguments and return (config_path, worktree_name).
///
/// `worktree_name` (from the `--shoot` / `-s` flag):
///   - `None`        → no `--shoot` / `-s` flag
///   - `Some(name)`  → `--shoot` / `-s` flag present; `name` is either the
///                     supplied value or an auto-generated one
fn parse_args() -> (Option<String>, Option<String>) {
    let args: Vec<String> = std::env::args().collect();
    let mut config_path: Option<String> = None;
    let mut worktree_name: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                if i + 1 < args.len() {
                    config_path = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--shoot" | "-s" => {
                // If the next argument exists and doesn't look like a flag, treat it
                // as the worktree name; otherwise auto-generate one.
                let name = if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    let n = args[i + 1].clone();
                    i += 2;
                    n
                } else {
                    i += 1;
                    worktree::random_name()
                };
                worktree_name = Some(name);
            }
            _ => {
                i += 1;
            }
        }
    }

    (config_path, worktree_name)
}

/// After the TUI exits, decide whether to keep or remove the worktree.
fn handle_worktree_cleanup(wt: &worktree::Worktree) {
    if wt.has_changes() {
        println!();
        println!(
            "Shoot '{}' has changes (branch: {}).",
            wt.name, wt.branch
        );
        println!("  Path: {}", wt.path.display());
        print!("Keep shoot? [K]eep / [r]emove: ");
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            let choice = input.trim().to_lowercase();
            if choice == "r" || choice == "remove" {
                match wt.remove() {
                    Ok(()) => println!("Shoot removed."),
                    Err(e) => eprintln!("Failed to remove shoot: {}", e),
                }
            } else {
                println!("Shoot kept at: {}", wt.path.display());
                println!("  Branch: {}", wt.branch);
            }
        }
    } else {
        // No changes — remove silently.
        match wt.remove() {
            Ok(()) => println!("Shoot '{}' cleaned up (no changes).", wt.name),
            Err(e) => eprintln!("Warning: failed to remove shoot: {}", e),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let (config_path, worktree_flag) = parse_args();

    // Create a git worktree when --shoot / -s is requested. RAII guard cleans up
    // on error paths (best-effort remove or print path for manual cleanup).
    let active_worktree: Option<worktree::Worktree> = if let Some(name) = worktree_flag {
        let wt = worktree::Worktree::create(&name)?;
        eprintln!(
            "Created shoot '{}' at {}  (branch: {})",
            wt.name,
            wt.path.display(),
            wt.branch
        );
        Some(wt)
    } else {
        None
    };
    let mut worktree_guard = WorktreeGuard(active_worktree);

    let mut config = Config::load(config_path.as_deref())?;

    // When running inside a worktree, redirect every pane's working directory
    // to the worktree path so all shells/commands start there in isolation.
    if let Some(wt) = worktree_guard.get() {
        let wt_path = wt.path.to_string_lossy().to_string();
        for pane in &mut config.panes {
            pane.cwd = Some(wt_path.clone());
        }
    }

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

        let term = terminal::new_term(initial_rows, initial_cols, 1000);

        let (pty_tx, pty_rx) = mpsc::unbounded_channel::<PtyEvent>();
        launch_reader_task(spawned.reader, term.clone(), pty_tx);

        let pane = Pane::new(
            i,
            pane_config.name.clone(),
            spawned.master,
            spawned.writer,
            term,
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

    let shoot_name = worktree_guard.get().map(|wt| wt.name.clone());
    let mut app = AppState::new(panes, config.layout, config.default_shell, shoot_name);
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

    // Disarm the guard and run interactive cleanup after the terminal is fully
    // restored so the prompt and any output are readable.
    if let Some(wt) = worktree_guard.take() {
        handle_worktree_cleanup(&wt);
    }

    std::process::exit(0);
}
