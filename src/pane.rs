use parking_lot::Mutex;
use portable_pty::{MasterPty, PtySize};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::pty::PtyEvent;

#[derive(Debug, Clone)]
pub struct RenderedCell {
    pub ch: char,
    pub fg: vt100::Color,
    pub bg: vt100::Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for RenderedCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: vt100::Color::Default,
            bg: vt100::Color::Default,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

pub struct Pane {
    pub id: usize,
    pub name: String,
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Arc<Mutex<Box<dyn std::io::Write + Send>>>,
    pub parser: Arc<Mutex<vt100::Parser>>,
    pub pty_rx: Option<mpsc::UnboundedReceiver<PtyEvent>>,
    pub scroll_offset: usize,
    pub cols: u16,
    pub rows: u16,
    pub scrollback: Vec<Vec<RenderedCell>>,
    pub closed: bool,
    pub collapsed: bool,
    pub weight: u16,
}

impl Pane {
    pub fn new(
        id: usize,
        name: String,
        master: Box<dyn MasterPty + Send>,
        writer: Arc<Mutex<Box<dyn std::io::Write + Send>>>,
        parser: Arc<Mutex<vt100::Parser>>,
        pty_rx: mpsc::UnboundedReceiver<PtyEvent>,
        cols: u16,
        rows: u16,
    ) -> Self {
        Self {
            id,
            name,
            master,
            writer,
            parser,
            pty_rx: Some(pty_rx),
            scroll_offset: 0,
            cols,
            rows,
            scrollback: Vec::new(),
            closed: false,
            collapsed: false,
            weight: 10,
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }
        if cols == self.cols && rows == self.rows {
            return;
        }

        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });

        let mut parser = self.parser.lock();
        let contents = parser.screen().contents_formatted();
        let mut new_parser = vt100::Parser::new(rows, cols, 1000);
        new_parser.process(&contents);
        *parser = new_parser;

        self.cols = cols;
        self.rows = rows;
    }

    #[allow(dead_code)]
    pub fn snapshot_scrollback(&mut self) {
        let parser = self.parser.lock();
        let screen = parser.screen();
        let rows = screen.size().0;
        let cols = screen.size().1;

        for row in 0..rows {
            let mut line = Vec::with_capacity(cols as usize);
            for col in 0..cols {
                let cell = screen.cell(row, col);
                if let Some(cell) = cell {
                    line.push(RenderedCell {
                        ch: cell.contents().chars().next().unwrap_or(' '),
                        fg: cell.fgcolor(),
                        bg: cell.bgcolor(),
                        bold: cell.bold(),
                        italic: cell.italic(),
                        underline: cell.underline(),
                    });
                } else {
                    line.push(RenderedCell::default());
                }
            }
            self.scrollback.push(line);
        }

        const MAX_SCROLLBACK: usize = 10_000;
        if self.scrollback.len() > MAX_SCROLLBACK {
            let excess = self.scrollback.len() - MAX_SCROLLBACK;
            self.scrollback.drain(0..excess);
        }
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        let max = self.scrollback.len();
        if self.scroll_offset > max {
            self.scroll_offset = max;
        }
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    #[allow(dead_code)]
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn write_input(&self, data: &[u8]) {
        let mut writer = self.writer.lock();
        let _ = std::io::Write::write_all(&mut *writer, data);
    }
}
