use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Processor, Rgb};
use alacritty_terminal::Term;
use parking_lot::Mutex;
use std::sync::Arc;

/// No-op event listener — we poll state rather than react to events.
#[derive(Clone)]
pub struct VoidListener;

impl EventListener for VoidListener {
    fn send_event(&self, _event: Event) {}
}

/// Terminal size passed to `Term::new` and `Term::resize`.
pub struct TermSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

/// Create a new alacritty `Term` wrapped in `Arc<Mutex<_>>`.
pub fn new_term(rows: u16, cols: u16, scrollback: usize) -> Arc<Mutex<Term<VoidListener>>> {
    let size = TermSize {
        cols: cols as usize,
        rows: rows as usize,
    };
    let config = TermConfig {
        scrolling_history: scrollback,
        ..TermConfig::default()
    };
    Arc::new(Mutex::new(Term::new(config, &size, VoidListener)))
}

/// Create a new `vte::ansi::Processor` for feeding bytes into the `Term`.
pub fn new_processor() -> Processor {
    Processor::new()
}

/// Feed bytes from PTY output into the terminal.
pub fn process_bytes(
    term: &mut Term<VoidListener>,
    processor: &mut Processor,
    bytes: &[u8],
) {
    processor.advance(term, bytes);
}

/// Get the character in a cell at the given (row, col) position.
/// Row 0 is the top of the visible viewport (accounting for display_offset/scrollback).
pub fn cell_char(term: &Term<VoidListener>, row: usize, col: usize) -> char {
    use alacritty_terminal::index::{Column, Line};
    let grid = term.grid();
    let line = Line(row as i32);
    let column = Column(col);
    if row < term.screen_lines() && col < term.columns() {
        grid[line][column].c
    } else {
        ' '
    }
}

/// Information about a single cell for rendering.
pub struct CellInfo {
    pub ch: char,
    pub fg: AnsiColor,
    pub bg: AnsiColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    #[allow(dead_code)]
    pub strikethrough: bool,
}

/// Get cell info at the given (row, col) in the visible viewport.
pub fn cell_info(term: &Term<VoidListener>, row: usize, col: usize) -> CellInfo {
    use alacritty_terminal::index::{Column, Line};
    let grid = term.grid();
    let line = Line(row as i32);
    let column = Column(col);
    if row < term.screen_lines() && col < term.columns() {
        let cell = &grid[line][column];
        CellInfo {
            ch: cell.c,
            fg: cell.fg,
            bg: cell.bg,
            bold: cell.flags.contains(CellFlags::BOLD),
            dim: cell.flags.contains(CellFlags::DIM),
            italic: cell.flags.contains(CellFlags::ITALIC),
            underline: cell.flags.contains(CellFlags::UNDERLINE)
                || cell.flags.contains(CellFlags::DOUBLE_UNDERLINE)
                || cell.flags.contains(CellFlags::UNDERCURL)
                || cell.flags.contains(CellFlags::DOTTED_UNDERLINE)
                || cell.flags.contains(CellFlags::DASHED_UNDERLINE),
            inverse: cell.flags.contains(CellFlags::INVERSE),
            strikethrough: cell.flags.contains(CellFlags::STRIKEOUT),
        }
    } else {
        CellInfo {
            ch: ' ',
            fg: AnsiColor::Named(NamedColor::Foreground),
            bg: AnsiColor::Named(NamedColor::Background),
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            inverse: false,
            strikethrough: false,
        }
    }
}

/// Convert an alacritty `Color` to a ratatui `Color`.
pub fn convert_color(color: AnsiColor) -> ratatui::style::Color {
    use ratatui::style::Color;
    match color {
        AnsiColor::Named(named) => match named {
            NamedColor::Black | NamedColor::DimBlack => Color::Black,
            NamedColor::Red | NamedColor::DimRed => Color::Red,
            NamedColor::Green | NamedColor::DimGreen => Color::Green,
            NamedColor::Yellow | NamedColor::DimYellow => Color::Yellow,
            NamedColor::Blue | NamedColor::DimBlue => Color::Blue,
            NamedColor::Magenta | NamedColor::DimMagenta => Color::Magenta,
            NamedColor::Cyan | NamedColor::DimCyan => Color::Cyan,
            NamedColor::White | NamedColor::DimWhite => Color::White,
            NamedColor::BrightBlack => Color::DarkGray,
            NamedColor::BrightRed => Color::LightRed,
            NamedColor::BrightGreen => Color::LightGreen,
            NamedColor::BrightYellow => Color::LightYellow,
            NamedColor::BrightBlue => Color::LightBlue,
            NamedColor::BrightMagenta => Color::LightMagenta,
            NamedColor::BrightCyan => Color::LightCyan,
            NamedColor::BrightWhite => Color::Gray,
            NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::DimForeground => {
                Color::Reset
            }
            NamedColor::Background => Color::Reset,
            NamedColor::Cursor => Color::Reset,
        },
        AnsiColor::Spec(Rgb { r, g, b }) => Color::Rgb(r, g, b),
        AnsiColor::Indexed(idx) => match idx {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::White,
            8 => Color::DarkGray,
            9 => Color::LightRed,
            10 => Color::LightGreen,
            11 => Color::LightYellow,
            12 => Color::LightBlue,
            13 => Color::LightMagenta,
            14 => Color::LightCyan,
            15 => Color::Gray,
            n => Color::Indexed(n),
        },
    }
}

/// Get the number of screen lines (rows) in the terminal.
pub fn screen_rows(term: &Term<VoidListener>) -> usize {
    term.screen_lines()
}

/// Get the number of columns in the terminal.
pub fn screen_cols(term: &Term<VoidListener>) -> usize {
    term.columns()
}

/// Get cursor position (row, col) in the visible viewport.
pub fn cursor_position(term: &Term<VoidListener>) -> (usize, usize) {
    let point = term.grid().cursor.point;
    (point.line.0 as usize, point.column.0)
}

/// Get the current scrollback display offset (0 = no scroll, positive = scrolled up).
#[allow(dead_code)]
pub fn display_offset(term: &Term<VoidListener>) -> usize {
    term.grid().display_offset()
}
