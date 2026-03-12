use alacritty_terminal::grid::Scroll;
use alacritty_terminal::Term;
use parking_lot::Mutex;
use portable_pty::{MasterPty, PtySize};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::pty::PtyEvent;
use crate::terminal::{TermSize, VoidListener};

pub struct Pane {
    pub id: usize,
    pub name: String,
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Arc<Mutex<Box<dyn std::io::Write + Send>>>,
    pub term: Arc<Mutex<Term<VoidListener>>>,
    pub pty_rx: Option<mpsc::UnboundedReceiver<PtyEvent>>,
    pub scroll_offset: usize,
    pub cols: u16,
    pub rows: u16,
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
        term: Arc<Mutex<Term<VoidListener>>>,
        pty_rx: mpsc::UnboundedReceiver<PtyEvent>,
        cols: u16,
        rows: u16,
    ) -> Self {
        Self {
            id,
            name,
            master,
            writer,
            term,
            pty_rx: Some(pty_rx),
            scroll_offset: 0,
            cols,
            rows,
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

        let size = TermSize {
            cols: cols as usize,
            rows: rows as usize,
        };
        self.term.lock().resize(size);

        self.cols = cols;
        self.rows = rows;
    }

    pub fn scroll_up(&mut self, lines: usize) {
        let mut term = self.term.lock();
        term.scroll_display(Scroll::Delta(-(lines as i32)));
        self.scroll_offset = term.display_offset();
    }

    pub fn scroll_down(&mut self, lines: usize) {
        let mut term = self.term.lock();

        let current_offset = term.display_offset();
        if current_offset == 0 {
            // Already at the bottom; nothing to do.
            self.scroll_offset = 0;
            return;
        }

        if lines >= current_offset {
            // Scrolling past the bottom; just jump to the bottom.
            term.scroll_display(Scroll::Bottom);
        } else {
            term.scroll_display(Scroll::Delta(lines as i32));
        }

        self.scroll_offset = term.display_offset();
    }

    pub fn write_input(&self, data: &[u8]) {
        let mut writer = self.writer.lock();
        let _ = std::io::Write::write_all(&mut *writer, data);
    }
}
