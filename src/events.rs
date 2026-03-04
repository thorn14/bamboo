use std::time::Duration;

use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton,
    MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use crate::app::AppState;
use crate::config::PaneConfig;
use crate::pane::Pane;
use crate::pty::{self, PtyEvent};
use crate::ui::{self, FOOTER_HEIGHT};

pub enum AppEvent {
    Terminal(CrosstermEvent),
    PtyOutput { pane_id: usize, event: PtyEvent },
    Tick,
}

pub async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut AppState,
    mut unified_rx: mpsc::UnboundedReceiver<AppEvent>,
    unified_tx: mpsc::UnboundedSender<AppEvent>,
) -> anyhow::Result<()> {
    let (ct_tx, mut ct_rx) = mpsc::unbounded_channel::<AppEvent>();

    tokio::task::spawn_blocking(move || {
        loop {
            match event::poll(Duration::from_millis(20)) {
                Ok(true) => {
                    if let Ok(ev) = event::read() {
                        if ct_tx.send(AppEvent::Terminal(ev)).is_err() {
                            break;
                        }
                    }
                }
                Ok(false) => {
                    if ct_tx.send(AppEvent::Tick).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    loop {
        terminal.draw(|frame| {
            ui::render(frame, app);
        })?;

        let event = tokio::select! {
            ev = ct_rx.recv() => match ev {
                Some(e) => e,
                None => break,
            },
            ev = unified_rx.recv() => match ev {
                Some(e) => e,
                None => break,
            },
        };

        match event {
            AppEvent::Terminal(ct_event) => match ct_event {
                CrosstermEvent::Key(key) => handle_key_event(key, app, &unified_tx),
                CrosstermEvent::Mouse(mouse) => handle_mouse_event(mouse, app),
                CrosstermEvent::Resize(cols, rows) => handle_resize(cols, rows, app),
                _ => {}
            },
            AppEvent::PtyOutput { pane_id, event } => match event {
                PtyEvent::Data(_) => {
                    if let Some(pane) = app.panes.iter_mut().find(|p| p.id == pane_id) {
                        if pane.scroll_offset == 0 {
                            // live view; data already processed by parser in reader task
                        }
                    }
                }
                PtyEvent::Closed => {
                    if let Some(pane) = app.panes.iter_mut().find(|p| p.id == pane_id) {
                        pane.closed = true;
                    }
                }
            },
            AppEvent::Tick => {}
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn handle_key_event(
    key: KeyEvent,
    app: &mut AppState,
    unified_tx: &mpsc::UnboundedSender<AppEvent>,
) {
    if key.kind != KeyEventKind::Press {
        return;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    if ctrl && key.code == KeyCode::Char('q') {
        app.should_quit = true;
        return;
    }

    // Alt+S toggles selection mode
    if alt && key.code == KeyCode::Char('s') {
        if app.selection.is_some() {
            app.clear_selection();
        } else {
            app.start_selection();
        }
        return;
    }

    // In selection mode: intercept all keys for navigation/copy/cancel
    if app.selection.is_some() {
        match key.code {
            KeyCode::Up => app.move_selection_cursor(-1, 0),
            KeyCode::Down => app.move_selection_cursor(1, 0),
            KeyCode::Left => app.move_selection_cursor(0, -1),
            KeyCode::Right => app.move_selection_cursor(0, 1),
            KeyCode::Enter => {
                if let Some(text) = app.selection_text() {
                    copy_to_clipboard(&text);
                }
                app.clear_selection();
            }
            KeyCode::Char('y') if !alt && !ctrl => {
                if let Some(text) = app.selection_text() {
                    copy_to_clipboard(&text);
                }
                app.clear_selection();
            }
            KeyCode::Esc => app.clear_selection(),
            _ => {}
        }
        return;
    }

    if ctrl {
        match key.code {
            KeyCode::Up => {
                app.grow_focused_weight(2);
                return;
            }
            KeyCode::Down => {
                app.shrink_focused_weight(2);
                return;
            }
            _ => {}
        }
    }

    if alt {
        match key.code {
            KeyCode::Char('j') | KeyCode::Char('l') => {
                app.focus_next();
                return;
            }
            KeyCode::Char('k') | KeyCode::Char('h') => {
                app.focus_prev();
                return;
            }
            KeyCode::Char('n') => {
                spawn_new_pane(app, unified_tx);
                return;
            }
            KeyCode::Char('w') => {
                app.remove_focused_pane();
                return;
            }
            KeyCode::Char('c') => {
                app.toggle_collapse_focused();
                return;
            }
            KeyCode::Char('v') => {
                if let Some(text) = paste_from_clipboard() {
                    if let Some(pane) = app.focused_pane() {
                        pane.write_input(text.as_bytes());
                    }
                }
                return;
            }
            _ => {}
        }
    }

    if let Some(bytes) = key_event_to_bytes(&key) {
        if let Some(pane) = app.focused_pane() {
            pane.write_input(&bytes);
        }
    }
}

fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

fn paste_from_clipboard() -> Option<String> {
    use std::process::Command;
    let output = Command::new("pbpaste").output().ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

fn spawn_new_pane(app: &mut AppState, unified_tx: &mpsc::UnboundedSender<AppEvent>) {
    let pane_id = app.take_next_pane_id();
    let name = format!("Shell {}", pane_id);

    let cols = app.term_cols.saturating_sub(2).max(10);
    let n_panes = app.panes.len() + 1;
    let rows = (app.term_rows / n_panes as u16).saturating_sub(2).max(5);

    let pane_config = PaneConfig {
        name: name.clone(),
        command: None,
        cwd: None,
        env: std::collections::HashMap::new(),
    };

    let spawned = match pty::spawn_pty(&pane_config, &app.default_shell, cols, rows) {
        Ok(s) => s,
        Err(_) => return,
    };

    let parser = std::sync::Arc::new(parking_lot::Mutex::new(vt100::Parser::new(
        rows, cols, 1000,
    )));

    let (pty_tx, pty_rx) = mpsc::unbounded_channel::<PtyEvent>();
    pty::launch_reader_task(spawned.reader, parser.clone(), pty_tx);

    let pane = Pane::new(
        pane_id,
        name,
        spawned.master,
        spawned.writer,
        parser,
        pty_rx,
        cols,
        rows,
    );

    app.add_pane(pane);

    if let Some(mut pty_rx) = app.panes.last_mut().and_then(|p| p.pty_rx.take()) {
        let tx = unified_tx.clone();
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

fn handle_mouse_event(mouse: MouseEvent, app: &mut AppState) {
    let col = mouse.column;
    let row = mouse.row;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            app.last_mouse_pos = Some((row, col));
            // Click on "above" scroll indicator → page up
            if app.viewport_start > 0 && row == 0 {
                app.page_viewport_up();
                return;
            }

            // Click on "below" scroll indicator → page down
            let has_below = app
                .last_pane_areas
                .last()
                .map(|(idx, _)| *idx + 1 < app.panes.len())
                .unwrap_or(false);
            if has_below && row == app.term_rows.saturating_sub(1 + FOOTER_HEIGHT) {
                app.page_viewport_down();
                return;
            }

            let areas = app.last_pane_areas.clone();
            for &(pane_idx, area) in &areas {
                // Click on title bar (top border row)
                if row == area.y && col >= area.x && col < area.x + area.width {
                    // Close button [x] in rightmost 3 chars before border
                    if area.width >= 8 {
                        let close_start = area.x + area.width.saturating_sub(4);
                        let close_end = area.x + area.width.saturating_sub(2);
                        if col >= close_start && col <= close_end {
                            app.close_pane(pane_idx);
                            return;
                        }
                    }

                    // Collapse toggle [▾]/[▸] at positions x+1..x+3
                    if col >= area.x + 1 && col <= area.x + 3 {
                        app.focus(pane_idx);
                        app.toggle_collapse_at(pane_idx);
                        return;
                    }

                    app.focus(pane_idx);
                    app.clear_selection();
                    return;
                }

                // Click in pane body (inside borders)
                if col > area.x && col < area.x + area.width.saturating_sub(1)
                    && row > area.y && row < area.y + area.height.saturating_sub(1)
                {
                    app.focus(pane_idx);
                    app.clear_selection();
                    return;
                }

                // Click on border (not title bar)
                if col >= area.x && col < area.x + area.width
                    && row >= area.y && row < area.y + area.height
                {
                    app.focus(pane_idx);
                    app.clear_selection();
                    return;
                }
            }
            app.clear_selection();
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if app.selection.is_none() {
                if let Some((start_row, start_col)) = app.last_mouse_pos {
                    // Start selection if we've moved at least one cell
                    if (start_row as i32 - row as i32).abs() > 0 || (start_col as i32 - col as i32).abs() > 0 {
                        // Find which pane we're in
                        let areas = app.last_pane_areas.clone();
                        for &(pane_idx, area) in &areas {
                            if start_col > area.x && start_col < area.x + area.width.saturating_sub(1)
                                && start_row > area.y && start_row < area.y + area.height.saturating_sub(1)
                            {
                                let r = start_row - (area.y + 1);
                                let c = start_col - (area.x + 1);
                                app.start_selection_at(pane_idx, r, c);
                                break;
                            }
                        }
                    }
                }
            }

            if let Some(sel) = &app.selection {
                if let Some(&(_idx, area)) = app.last_pane_areas.iter().find(|(idx, _)| app.panes[*idx].id == sel.pane_id) {
                    let inner_w = area.width.saturating_sub(2);
                    let inner_h = area.height.saturating_sub(2);
                    if inner_w > 0 && inner_h > 0 {
                        let r = row.saturating_sub(area.y + 1).min(inner_h - 1);
                        let c = col.saturating_sub(area.x + 1).min(inner_w - 1);
                        app.update_selection_at(r, c);
                    }
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.last_mouse_pos = None;
            if let Some(sel) = &app.selection {
                if sel.anchor != sel.cursor {
                    if let Some(text) = app.selection_text() {
                        if !text.is_empty() {
                            copy_to_clipboard(&text);
                        }
                    }
                }
            }
        }
        MouseEventKind::ScrollUp => {
            if let Some(pane) = app.focused_pane_mut() {
                pane.scroll_up(3);
            }
        }
        MouseEventKind::ScrollDown => {
            if let Some(pane) = app.focused_pane_mut() {
                pane.scroll_down(3);
            }
        }
        _ => {}
    }
}

fn handle_resize(cols: u16, rows: u16, app: &mut AppState) {
    app.term_cols = cols;
    app.term_rows = rows;
}

fn key_event_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let byte = (c as u8).wrapping_sub(b'a').wrapping_add(1);
                if (1..=26).contains(&byte) {
                    Some(vec![byte])
                } else {
                    None
                }
            } else {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                Some(s.as_bytes().to_vec())
            }
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::F(1) => Some(b"\x1bOP".to_vec()),
        KeyCode::F(2) => Some(b"\x1bOQ".to_vec()),
        KeyCode::F(3) => Some(b"\x1bOR".to_vec()),
        KeyCode::F(4) => Some(b"\x1bOS".to_vec()),
        KeyCode::F(5) => Some(b"\x1b[15~".to_vec()),
        KeyCode::F(6) => Some(b"\x1b[17~".to_vec()),
        KeyCode::F(7) => Some(b"\x1b[18~".to_vec()),
        KeyCode::F(8) => Some(b"\x1b[19~".to_vec()),
        KeyCode::F(9) => Some(b"\x1b[20~".to_vec()),
        KeyCode::F(10) => Some(b"\x1b[21~".to_vec()),
        KeyCode::F(11) => Some(b"\x1b[23~".to_vec()),
        KeyCode::F(12) => Some(b"\x1b[24~".to_vec()),
        _ => None,
    }
}
