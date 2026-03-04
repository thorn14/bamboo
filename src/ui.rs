use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use crate::app::{AppState, SelectionState};
use crate::pane::Pane;

const COLLAPSED_HEIGHT: u16 = 3;
const MIN_EXPANDED_HEIGHT: u16 = 5;
const INDICATOR_HEIGHT: u16 = 1;
pub const FOOTER_HEIGHT: u16 = 1;

pub fn render(frame: &mut Frame, app: &mut AppState) {
    let full_area = frame.area();
    if full_area.height == 0 || full_area.width == 0 {
        return;
    }

    let pane_area = Rect::new(
        full_area.x,
        full_area.y,
        full_area.width,
        full_area.height.saturating_sub(FOOTER_HEIGHT),
    );
    let footer_area = Rect::new(
        full_area.x,
        full_area.y + pane_area.height,
        full_area.width,
        FOOTER_HEIGHT,
    );

    ensure_focused_visible(app, pane_area.height);

    let above_count = app.viewport_start;
    let (layout, visible_end) = compute_visible_layout(app, pane_area);
    let below_count = app.panes.len().saturating_sub(visible_end);

    app.last_pane_areas = layout.clone();

    let focused = app.focused;
    let selection = app.selection.clone();
    for &(pane_idx, pa) in &layout {
        let is_focused = pane_idx == focused;
        let pane = &mut app.panes[pane_idx];
        let pane_sel = selection.as_ref().filter(|s| s.pane_id == pane.id);
        render_pane(frame, pane, pa, is_focused, pane_sel);
    }

    let buf = frame.buffer_mut();
    if above_count > 0 {
        let msg = format!(" ▲ {} more above ", above_count);
        let style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        buf.set_string(pane_area.x, pane_area.y, &msg, style);
    }

    if below_count > 0 {
        let msg = format!(" ▼ {} more below ", below_count);
        let style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let y = pane_area.y + pane_area.height - 1;
        buf.set_string(pane_area.x, y, &msg, style);
    }

    render_footer(buf, footer_area, app.active_shoot.as_deref(), app.selection.is_some());
}

fn render_footer(buf: &mut Buffer, area: Rect, active_shoot: Option<&str>, selection_active: bool) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let bg_style = Style::default().fg(Color::White);
    for x in area.x..area.x + area.width {
        buf.set_string(x, area.y, " ", bg_style);
    }

    // Right-aligned shoot badge rendered first so we know how much space it takes.
    // Use Unicode display width (e.g. emoji 🎋 is 2 cells) so badge and hints don't overlap.
    let shoot_badge_width = if let Some(name) = active_shoot {
        let badge = format!(" 🎋 {} ", name);
        let badge_width = Line::from(badge.as_str()).width() as u16;
        if area.width >= badge_width {
            let badge_x = area.x + area.width - badge_width;
            let shoot_style = Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD);
            buf.set_string(badge_x, area.y, &badge, shoot_style);
            badge_width
        } else {
            0
        }
    } else {
        0
    };

    let key_style = if selection_active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)
    };
    let desc_style = Style::default().fg(Color::Gray);

    let hints: &[(&str, &str)] = if selection_active {
        &[
            ("SELECT", ""),
            ("↑↓←→", "move"),
            ("Enter/y", "copy"),
            ("Esc", "cancel"),
        ]
    } else {
        &[
            ("Ctrl+Q", "Quit"),
            ("Alt+j/k", "Focus"),
            ("Alt+n", "New"),
            ("Alt+w", "Close"),
            ("Alt+c", "Collapse"),
            ("Alt+v", "Paste"),
            ("Alt+s", "Select"),
            ("Ctrl+↑/↓", "Resize"),
        ]
    };

    let right_margin = shoot_badge_width + 1;
    let usable_right = area.x + area.width.saturating_sub(right_margin);
    let mut x = area.x + 1;
    for (key, desc) in hints {
        let key_width = Line::from(*key).width() as u16;
        if x + key_width + 2 >= usable_right {
            break;
        }
        buf.set_string(x, area.y, key, key_style);
        x += key_width;

        let label = format!(" {}  ", desc);
        let label_width = Line::from(label.as_str()).width() as u16;
        if x + label_width > usable_right {
            break;
        }
        buf.set_string(x, area.y, &label, desc_style);
        x += label_width;
    }
}

fn compute_visible_layout(app: &AppState, area: Rect) -> (Vec<(usize, Rect)>, usize) {
    let total_height = area.height;
    let start = app.viewport_start;

    let above_count = start;
    let has_above = above_count > 0;

    let usable_top = if has_above { area.y + INDICATOR_HEIGHT } else { area.y };

    // First pass: determine which panes fit with minimum heights,
    // reserving space for a bottom indicator if needed.
    let mut remaining = total_height.saturating_sub(if has_above { INDICATOR_HEIGHT } else { 0 });
    let mut expanded_indices: Vec<usize> = Vec::new();
    let mut visible: Vec<usize> = Vec::new();
    let mut total_weight: u32 = 0;

    for i in start..app.panes.len() {
        let min_h = if app.panes[i].collapsed {
            COLLAPSED_HEIGHT
        } else {
            MIN_EXPANDED_HEIGHT
        };

        let below_after = app.panes.len().saturating_sub(i + 1);
        let need_below_indicator = below_after > 0;
        let reserved = if need_below_indicator { INDICATOR_HEIGHT } else { 0 };

        if remaining < min_h + reserved && !visible.is_empty() {
            break;
        }

        remaining = remaining.saturating_sub(min_h);
        if !app.panes[i].collapsed {
            expanded_indices.push(visible.len());
            total_weight += app.panes[i].weight as u32;
        }
        visible.push(i);
    }

    let visible_end = visible.last().map(|&i| i + 1).unwrap_or(start);
    let has_below = visible_end < app.panes.len();
    let remaining = remaining.saturating_sub(if has_below { INDICATOR_HEIGHT } else { 0 });

    // Second pass: compute heights with weighted distribution for expanded panes
    let mut heights: Vec<u16> = visible
        .iter()
        .map(|&i| {
            if app.panes[i].collapsed {
                COLLAPSED_HEIGHT
            } else {
                MIN_EXPANDED_HEIGHT
            }
        })
        .collect();

    if !expanded_indices.is_empty() && remaining > 0 && total_weight > 0 {
        let mut distributed = 0u16;
        for (j, &idx) in expanded_indices.iter().enumerate() {
            let pane_idx = visible[idx];
            let w = app.panes[pane_idx].weight as u32;
            let bonus = if j == expanded_indices.len() - 1 {
                remaining - distributed
            } else {
                ((remaining as u32 * w) / total_weight) as u16
            };
            heights[idx] += bonus;
            distributed += bonus;
        }
    }

    let mut y = usable_top;
    let mut result = Vec::with_capacity(visible.len());
    for (j, &pane_idx) in visible.iter().enumerate() {
        let h = heights[j];
        result.push((pane_idx, Rect::new(area.x, y, area.width, h)));
        y += h;
    }

    (result, visible_end)
}

fn ensure_focused_visible(app: &mut AppState, total_height: u16) {
    if app.panes.is_empty() {
        return;
    }

    if app.focused < app.viewport_start {
        app.viewport_start = app.focused;
        return;
    }

    loop {
        let end = compute_visible_end(&app.panes, app.viewport_start, total_height);
        if app.focused < end {
            break;
        }
        app.viewport_start += 1;
        if app.viewport_start >= app.panes.len() {
            app.viewport_start = app.panes.len().saturating_sub(1);
            break;
        }
    }
}

fn compute_visible_end(panes: &[Pane], start: usize, total_height: u16) -> usize {
    let has_above = start > 0;
    let mut remaining = total_height.saturating_sub(if has_above { INDICATOR_HEIGHT } else { 0 });
    let mut end = start;

    for i in start..panes.len() {
        let min_h = if panes[i].collapsed {
            COLLAPSED_HEIGHT
        } else {
            MIN_EXPANDED_HEIGHT
        };
        let below_after = panes.len().saturating_sub(i + 1);
        let reserved = if below_after > 0 { INDICATOR_HEIGHT } else { 0 };

        if remaining < min_h + reserved && end > start {
            break;
        }
        remaining = remaining.saturating_sub(min_h);
        end = i + 1;
    }

    end
}

fn render_pane(frame: &mut Frame, pane: &mut Pane, area: Rect, is_focused: bool, selection: Option<&SelectionState>) {
    let border_color = if is_focused {
        Color::Green
    } else {
        Color::Black
    };

    let ty = area.y;

    let toggle_style = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let name_style = if is_focused {
        Style::default()
            .fg(Color::White)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let close_style = Style::default()
        .fg(Color::LightRed)
        .add_modifier(Modifier::BOLD);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let buf = frame.buffer_mut();

    // Collapse toggle button
    let toggle = if pane.collapsed { "[▸]" } else { "[▾]" };
    buf.set_string(area.x + 1, ty, toggle, toggle_style);

    // Pane name + status
    let status = if pane.collapsed {
        pane.name.clone()
    } else if let Some(sel) = selection.filter(|s| s.pane_id == pane.id) {
        format!("{} [SEL {},{} → {},{}]", pane.name, sel.anchor.0, sel.anchor.1, sel.cursor.0, sel.cursor.1)
    } else if pane.scroll_offset > 0 {
        format!("{} [scroll: -{}]", pane.name, pane.scroll_offset)
    } else {
        format!("{} (w:{})", pane.name, pane.weight)
    };
    let max_name_len = area.width.saturating_sub(10) as usize;
    let display_name = if status.len() > max_name_len {
        &status[..max_name_len]
    } else {
        status.as_str()
    };
    buf.set_string(area.x + 5, ty, display_name, name_style);

    // Close button
    if area.width >= 8 {
        let close_x = area.x + area.width.saturating_sub(4);
        buf.set_string(close_x, ty, "[x]", close_style);
    }

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if pane.collapsed {
        // Show last terminal line so status is visible when collapsed
        render_last_terminal_line(buf, pane, inner);
        return;
    }

    if inner.width != pane.cols || inner.height != pane.rows {
        pane.resize(inner.width, inner.height);
    }

    // Sync scroll_offset to the actual depth available in vt100's scrollback buffer
    if pane.scroll_offset > 0 {
        let mut parser = pane.parser.lock();
        parser.screen_mut().set_scrollback(pane.scroll_offset);
        pane.scroll_offset = parser.screen().scrollback();
    }

    render_terminal_cells(buf, pane, inner, selection);
}

fn render_terminal_cells(buf: &mut Buffer, pane: &Pane, area: Rect, selection: Option<&SelectionState>) {
    let mut parser = pane.parser.lock();
    parser.screen_mut().set_scrollback(pane.scroll_offset);
    let screen = parser.screen();
    let (screen_rows, screen_cols) = screen.size();

    for row in 0..area.height {
        for col in 0..area.width {
            if row >= screen_rows || col >= screen_cols {
                continue;
            }
            if let Some(cell) = screen.cell(row, col) {
                let ch = cell.contents();
                let fg = convert_color(cell.fgcolor());
                let bg = convert_color(cell.bgcolor());
                let mut style = Style::default().fg(fg).bg(bg);
                if cell.bold() {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.dim() {
                    style = style.add_modifier(Modifier::DIM);
                }
                if cell.italic() {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline() {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if cell.inverse() {
                    style = style.add_modifier(Modifier::REVERSED);
                }

                // Selection highlighting
                if let Some(sel) = selection {
                    if sel.cursor == (row, col) {
                        style = Style::default()
                            .fg(Color::Black)
                            .bg(Color::Yellow)
                            .add_modifier(Modifier::BOLD);
                    } else if sel.contains(row, col) {
                        style = Style::default().fg(Color::White).bg(Color::Blue);
                    }
                }

                let x = area.x + col;
                let y = area.y + row;
                if x < area.x + area.width && y < area.y + area.height {
                    let display = if ch.is_empty() { " " } else { &ch };
                    buf.set_string(x, y, display, style);
                }
            }
        }
    }
}

fn render_last_terminal_line(buf: &mut Buffer, pane: &Pane, area: Rect) {
    let parser = pane.parser.lock();
    let screen = parser.screen();
    let (screen_rows, screen_cols) = screen.size();
    if screen_rows == 0 || screen_cols == 0 {
        return;
    }
    // Find the last row with any non-empty content; fall back to cursor row
    let target_row = (0..screen_rows)
        .rev()
        .find(|&row| {
            (0..screen_cols).any(|col| {
                screen.cell(row, col)
                    .map(|c| !c.contents().is_empty())
                    .unwrap_or(false)
            })
        })
        .unwrap_or_else(|| screen.cursor_position().0.min(screen_rows - 1));

    for col in 0..area.width.min(screen_cols) {
        if let Some(cell) = screen.cell(target_row, col) {
            let ch = cell.contents();
            let fg = convert_color(cell.fgcolor());
            let bg = convert_color(cell.bgcolor());
            let mut style = Style::default().fg(fg).bg(bg);
            if cell.bold() {
                style = style.add_modifier(Modifier::BOLD);
            }
            if cell.dim() {
                style = style.add_modifier(Modifier::DIM);
            }
            if cell.italic() {
                style = style.add_modifier(Modifier::ITALIC);
            }
            if cell.underline() {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if cell.inverse() {
                style = style.add_modifier(Modifier::REVERSED);
            }
            let display = if ch.is_empty() { " " } else { &ch };
            buf.set_string(area.x + col, area.y, display, style);
        }
    }
}

fn convert_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => match idx {
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
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
