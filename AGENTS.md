Context
Build a Rust TUI application that mimics a notebook layout with multiple persistent PTY terminal panes. The user wants at least 3 panes: a dev server pane, a one-off commands pane, and a main coding agent shell. Layout and startup commands are configured via TOML.
User Preferences

Layout: Scrollable vertical list of N panes (default); optional fixed mode (60% main + 2×20% right column)
Focus: Mouse click to focus pane
Resize: Key shortcuts (Alt+Up/Down) to adjust pane height ratios
Config: TOML file at ~/.config/bamboo/config.toml
Scrollback: Mouse wheel scrolls within a focused pane


File Structure
bamboo/
├── Cargo.toml
└── src/
    ├── main.rs       # Entry point, terminal setup, tokio runtime
    ├── app.rs        # AppState, LayoutMode, focus/resize logic
    ├── config.rs     # Config/PaneConfig structs, TOML loading
    ├── pane.rs       # Pane struct: PTY master, vt100 Parser, scroll, scrollback
    ├── pty.rs        # spawn_pty() + launch_reader_task()
    ├── ui.rs         # render(), layout calc, vt100→ratatui cell conversion
    └── events.rs     # AppEvent enum, run_event_loop(), input routing

Cargo.toml Dependencies
toml[dependencies]
ratatui       = { version = "0.29", features = ["crossterm"] }
crossterm     = "0.28"
portable-pty  = "0.9"
vt100         = "0.15"
tokio         = { version = "1", features = ["full"] }
serde         = { version = "1", features = ["derive"] }
toml          = "0.8"
parking_lot   = "0.12"
anyhow        = "1"
dirs          = "5"

Module Designs
config.rs

Config { default_shell, layout: LayoutConfig, panes: Vec<PaneConfig> }
PaneConfig { name, command: Option<String>, cwd: Option<PathBuf>, env: HashMap }
LayoutConfig enum: Scroll (default) | Fixed
Config::load() reads ~/.config/bamboo/config.toml, falls back to 1 default shell pane

pane.rs

Pane { id, name, master: Box<dyn MasterPty+Send>, writer: Arc<Mutex<Box<dyn Write+Send>>>, parser: Arc<Mutex<vt100::Parser>>, pty_rx: Option<UnboundedReceiver<PtyEvent>>, scroll_offset: usize, cols, rows, scrollback: Vec<Vec<RenderedCell>> }
PtyEvent { Data(Vec<u8>), Closed }
RenderedCell { ch, fg, bg, bold, italic, underline } — snapshot cells for scrollback
Pane::resize(cols, rows) — calls master.resize() and replaces vt100 Parser with new dimensions (snapshot via screen.contents_formatted() and replay)
Pane::scroll_up/down/to_bottom()

pty.rs

spawn_pty(config, default_shell, cols, rows) -> SpawnedPty — calls native_pty_system().openpty(), builds CommandBuilder, spawns child into slave, extracts writer via take_writer()
launch_reader_task(reader, parser: Arc<Mutex<Parser>>, tx) — tokio::task::spawn_blocking, reads 4096-byte chunks, calls parser.lock().process(bytes), sends PtyEvent::Data(bytes) to notify main loop; sends PtyEvent::Closed on EOF

app.rs

AppState { panes, focused, layout_mode, pane_sizes: Vec<u16>, main_pane_ratio: u16, should_quit, last_pane_areas: Vec<Rect>, term_cols, term_rows }
pane_sizes sums to 100; initialized equally
focus(idx), focus_next(), focus_prev()
resize_focused_grow/shrink(delta: u16) — shifts % between focused pane and adjacent

ui.rs

compute_pane_areas(area, app) -> Vec<Rect> — Scroll mode: Layout::vertical(pane_sizes as Percentage); Fixed mode: 60/40 H-split then 50/50 V-split on right
render(frame, app, areas) — iterates panes, calls render_pane()
render_pane() — draws Block border (yellow if focused, dark gray otherwise) with pane name as title; calls render_terminal_cells() on inner area
render_terminal_cells() — locks pane.parser, reads screen.cell(row, col) for live view (scroll_offset==0) or pane.scrollback for scrolled view; calls buf.set_string() per cell
convert_color(vt100::Color) -> ratatui::style::Color — maps Idx 0-15 to named colors, rest to Indexed(n) or Rgb(r,g,b)

events.rs

AppEvent { Terminal(CrosstermEvent), PtyOutput { pane_id, event: PtyEvent }, Tick }
run_event_loop(terminal, app, unified_rx) — main async loop: draw → recv → dispatch
Crossterm reader: spawn_blocking with 20ms poll; sends AppEvent::Terminal or AppEvent::Tick
handle_key_event():

Ctrl+Q → quit
Alt+J/L → focus_next, Alt+K/H → focus_prev
Alt+Up/Down → resize_focused_shrink/grow(2) + notify_pane_resize
Ctrl+Up/Down → scroll_up/down(3) in focused pane
All other keys → key_event_to_bytes() → write to pane.writer


key_event_to_bytes() — maps crossterm KeyCode to ANSI byte sequences (arrows, F-keys, Ctrl combos)
handle_mouse_event():

Down(Left) → find_pane_at(col, row, app) using last_pane_areas → app.focus(idx)
ScrollUp/Down → pane.scroll_up/down(3) on focused pane


handle_resize(cols, rows, app) — recomputes areas, calls pane.resize() for each with inner dimensions (area minus 2 for borders)

main.rs

TerminalGuard drop struct for cleanup (disable_raw_mode, LeaveAlternateScreen, DisableMouseCapture)
tokio::main entry: load config → setup terminal → spawn PTYs → build AppState → wire PTY forwarding tasks → run event loop
PTY forwarding: take pty_rx from each pane (Option::take()), spawn async task per pane forwarding PtyEvent into unified AppEvent channel
Pass unified channel receiver to run_event_loop


Key Bindings
BindingActionCtrl+QQuitAlt+J / Alt+LFocus next paneAlt+K / Alt+HFocus prev paneAlt+UpShrink focused paneAlt+DownGrow focused paneCtrl+UpScroll pane upCtrl+DownScroll pane downMouse clickFocus clicked paneMouse scrollScroll focused paneAll other keysRoute to active PTY

Example Config
toml# ~/.config/bamboo/config.toml
layout = "Scroll"
default_shell = "/bin/zsh"

[[panes]]
name = "Agent"
command = "/bin/zsh"
cwd = "~/projects/myapp"

[[panes]]
name = "Dev Server"
command = "/bin/zsh"
cwd = "~/projects/myapp"

[[panes]]
name = "Commands"
command = "/bin/zsh"

Implementation Order

Cargo.toml + main.rs stub (terminal init/cleanup, empty ratatui loop)
config.rs — load + parse TOML
pty.rs — spawn PTY, verify shell starts
pane.rs — Pane struct + vt100 Parser integration
ui.rs — layout + border rendering (empty pane boxes)
ui.rs — render_terminal_cells() with vt100 cell extraction
events.rs — keyboard input routing to PTY writer
events.rs — mouse click focus + scroll wheel
events.rs — terminal resize + PTY resize
pane.rs — scrollback buffer + scroll rendering
Polish: graceful child process shutdown, edge cases


Verification
bashcd /Users/davidthorn/git/bamboo
cargo build            # Should compile cleanly
cargo run              # Opens TUI with default 1 pane
cargo run -- --config ~/.config/bamboo/config.toml  # 3-pane notebook
Manual tests:

Type in focused pane → input reaches shell
Click another pane → focus switches (yellow border moves)
Mouse scroll → pane scrolls up/down through output
Alt+Up/Down → pane height ratios change
Resize terminal window → all panes reflow and PTY resizes
npm run dev in dev server pane → output streams continuously
Ctrl+Q → clean exit, terminal restored