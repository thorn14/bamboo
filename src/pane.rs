use parking_lot::Mutex;
use portable_pty::{MasterPty, PtySize};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::pty::PtyEvent;

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

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    pub fn write_input(&self, data: &[u8]) {
        let mut writer = self.writer.lock();
        let _ = std::io::Write::write_all(&mut *writer, data);
    }
}
