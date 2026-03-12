use ratatui::layout::Rect;

use crate::config::LayoutConfig;
use crate::pane::Pane;

#[derive(Clone)]
pub struct SelectionState {
    pub pane_id: usize,
    pub anchor: (u16, u16),
    pub cursor: (u16, u16),
}

impl SelectionState {
    pub fn normalized(&self) -> ((u16, u16), (u16, u16)) {
        let (ar, ac) = self.anchor;
        let (cr, cc) = self.cursor;
        if (ar, ac) <= (cr, cc) {
            ((ar, ac), (cr, cc))
        } else {
            ((cr, cc), (ar, ac))
        }
    }

    pub fn contains(&self, row: u16, col: u16) -> bool {
        let ((sr, sc), (er, ec)) = self.normalized();
        if row < sr || row > er {
            return false;
        }
        if row == sr && col < sc {
            return false;
        }
        if row == er && col > ec {
            return false;
        }
        true
    }
}

pub struct AppState {
    pub panes: Vec<Pane>,
    pub focused: usize,
    #[allow(dead_code)]
    pub layout_mode: LayoutConfig,
    pub should_quit: bool,
    pub last_pane_areas: Vec<(usize, Rect)>,
    pub term_cols: u16,
    pub term_rows: u16,
    pub default_shell: String,
    pub next_pane_id: usize,
    pub viewport_start: usize,
    /// Name of the active shoot (git worktree), if any.
    pub active_shoot: Option<String>,
    pub selection: Option<SelectionState>,
    pub last_mouse_pos: Option<(u16, u16)>,
}

impl AppState {
    pub fn new(panes: Vec<Pane>, layout_mode: LayoutConfig, default_shell: String, active_shoot: Option<String>) -> Self {
        let next_pane_id = panes.len();

        Self {
            panes,
            focused: 0,
            layout_mode,
            should_quit: false,
            last_pane_areas: Vec::new(),
            term_cols: 0,
            term_rows: 0,
            default_shell,
            next_pane_id,
            viewport_start: 0,
            active_shoot,
            selection: None,
            last_mouse_pos: None,
        }
    }

    pub fn focus(&mut self, idx: usize) {
        if idx < self.panes.len() {
            self.focused = idx;
        }
    }

    pub fn focus_next(&mut self) {
        if !self.panes.is_empty() {
            self.focused = (self.focused + 1) % self.panes.len();
        }
    }

    pub fn focus_prev(&mut self) {
        if !self.panes.is_empty() {
            self.focused = (self.focused + self.panes.len() - 1) % self.panes.len();
        }
    }

    pub fn toggle_collapse_focused(&mut self) {
        if let Some(pane) = self.panes.get_mut(self.focused) {
            pane.collapsed = !pane.collapsed;
        }
    }

    pub fn grow_focused_weight(&mut self, delta: u16) {
        if let Some(pane) = self.panes.get_mut(self.focused) {
            if !pane.collapsed {
                pane.weight = pane.weight.saturating_add(delta).min(50);
            }
        }
    }

    pub fn shrink_focused_weight(&mut self, delta: u16) {
        if let Some(pane) = self.panes.get_mut(self.focused) {
            if !pane.collapsed {
                pane.weight = pane.weight.saturating_sub(delta).max(1);
            }
        }
    }

    pub fn take_next_pane_id(&mut self) -> usize {
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        id
    }

    pub fn add_pane(&mut self, pane: Pane) {
        self.panes.push(pane);
        self.focused = self.panes.len() - 1;
    }

    pub fn close_pane(&mut self, idx: usize) -> bool {
        if self.panes.len() <= 1 || idx >= self.panes.len() {
            return false;
        }
        self.panes.remove(idx);
        if self.focused > idx {
            self.focused -= 1;
        } else if self.focused >= self.panes.len() {
            self.focused = self.panes.len() - 1;
        }
        if self.viewport_start > 0 && self.viewport_start >= self.panes.len() {
            self.viewport_start = self.panes.len().saturating_sub(1);
        }
        true
    }

    pub fn remove_focused_pane(&mut self) -> bool {
        self.close_pane(self.focused)
    }

    pub fn toggle_collapse_at(&mut self, idx: usize) {
        if let Some(pane) = self.panes.get_mut(idx) {
            pane.collapsed = !pane.collapsed;
        }
    }

    pub fn page_viewport_up(&mut self) {
        let page = self.visible_pane_count().max(1);
        self.viewport_start = self.viewport_start.saturating_sub(page);
    }

    pub fn page_viewport_down(&mut self) {
        let page = self.visible_pane_count().max(1);
        let max = self.panes.len().saturating_sub(1);
        self.viewport_start = (self.viewport_start + page).min(max);
    }

    fn visible_pane_count(&self) -> usize {
        self.last_pane_areas.len()
    }

    pub fn focused_pane(&self) -> Option<&Pane> {
        self.panes.get(self.focused)
    }

    pub fn focused_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(self.focused)
    }

    pub fn start_selection(&mut self) {
        if let Some(pane) = self.panes.get(self.focused) {
            self.selection = Some(SelectionState {
                pane_id: pane.id,
                anchor: (0, 0),
                cursor: (0, 0),
            });
        }
    }

    pub fn start_selection_at(&mut self, pane_idx: usize, row: u16, col: u16) {
        if let Some(pane) = self.panes.get(pane_idx) {
            self.selection = Some(SelectionState {
                pane_id: pane.id,
                anchor: (row, col),
                cursor: (row, col),
            });
            self.focused = pane_idx;
        }
    }

    pub fn update_selection_at(&mut self, row: u16, col: u16) {
        if let Some(sel) = &mut self.selection {
            sel.cursor = (row, col);
        }
    }

    pub fn move_selection_cursor(&mut self, dr: i32, dc: i32) {
        let bounds = if let Some(sel) = &self.selection {
            let pane_id = sel.pane_id;
            self.panes
                .iter()
                .find(|p| p.id == pane_id)
                .map(|p| (p.rows as i32, p.cols as i32))
                .unwrap_or((0, 0))
        } else {
            return;
        };
        if let Some(sel) = &mut self.selection {
            let r = (sel.cursor.0 as i32 + dr).clamp(0, bounds.0 - 1) as u16;
            let c = (sel.cursor.1 as i32 + dc).clamp(0, bounds.1 - 1) as u16;
            sel.cursor = (r, c);
        }
    }

    pub fn selection_text(&self) -> Option<String> {
        let sel = self.selection.as_ref()?;
        let pane = self.panes.iter().find(|p| p.id == sel.pane_id)?;
        let term = pane.term.lock();
        let ((sr, sc), (er, ec)) = sel.normalized();
        let mut text = String::new();
        for row in sr..=er {
            let start_col = if row == sr { sc } else { 0 };
            let end_col = if row == er { ec } else { pane.cols.saturating_sub(1) };
            let mut row_text = String::new();
            for col in start_col..=end_col {
                let ch = crate::terminal::cell_char(&term, row as usize, col as usize);
                row_text.push(if ch == '\0' { ' ' } else { ch });
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(row_text.trim_end());
        }
        Some(text)
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }
}
