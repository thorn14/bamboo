use ratatui::layout::Rect;

use crate::config::LayoutConfig;
use crate::pane::Pane;

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
}

impl AppState {
    pub fn new(panes: Vec<Pane>, layout_mode: LayoutConfig, default_shell: String) -> Self {
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
}
