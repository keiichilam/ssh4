//! Terminal canvas: paints the vt100 screen and translates pointer/keyboard
//! interaction into terminal protocol bytes.

use crate::gui::search::SearchState;
use crate::gui::session::Session;
use crate::gui::theme;
use crate::terminal::brackets;
use crate::terminal::input::{egui_key_to_bytes, paste_payload};
use crate::terminal::mouse::{self, MouseButton};
use crate::terminal::selection::CellPos;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

pub const MIN_COLS: u16 = 40;
pub const MAX_COLS: u16 = 320;
pub const MIN_ROWS: u16 = 15;
pub const MAX_ROWS: u16 = 160;

/// Side effects the app layer must apply after rendering a frame.
#[derive(Default)]
pub struct TermOutput {
    /// Raw input bytes sent to the active session (replayed for sync input).
    pub input_sent: Vec<Vec<u8>>,
    /// Multiline paste text that needs the confirmation dialog.
    pub paste_dialog: Option<String>,
    /// Ctrl+P: upload clipboard image.
    pub upload_image: bool,
}

struct CellMetrics {
    w: f32,
    h: f32,
    origin: Pos2,
}

impl CellMetrics {
    fn cell_at(&self, pos: Pos2) -> (i32, i32) {
        let col = ((pos.x - self.origin.x) / self.w).floor() as i32;
        let row = ((pos.y - self.origin.y) / self.h).floor() as i32;
        (row, col)
    }
    fn rect(&self, row: u16, col: u16, len: u16) -> Rect {
        Rect::from_min_size(
            Pos2::new(
                self.origin.x + col as f32 * self.w,
                self.origin.y + row as f32 * self.h,
            ),
            Vec2::new(len as f32 * self.w, self.h),
        )
    }
}

pub fn terminal_ui(
    ui: &mut egui::Ui,
    session: &mut Session,
    search: &mut SearchState,
    font_size: f32,
    input_enabled: bool,
) -> TermOutput {
    let mut out = TermOutput::default();
    let avail = ui.available_rect_before_wrap();
    let font = FontId::monospace(font_size);
    let (cw, ch) = ui.fonts(|f| (f.glyph_width(&font, 'M'), f.row_height(&font)));

    let cols = ((avail.width() / cw) as u16).clamp(MIN_COLS, MAX_COLS);
    let rows = ((avail.height() / ch) as u16).clamp(MIN_ROWS, MAX_ROWS);
    session.resize(cols, rows);

    let metrics = CellMetrics {
        w: cw,
        h: ch,
        origin: avail.min,
    };

    let response = ui.allocate_rect(avail, Sense::click_and_drag());
    let painter = ui.painter_at(avail);
    painter.rect_filled(avail, 0.0, theme::current().term_bg);

    if input_enabled {
        handle_keyboard(ui, session, &mut out);
    }
    handle_pointer(ui, session, &response, &metrics);

    // Auto-hide the mouse cursor while typing; any pointer movement restores it.
    if ui.input(|i| i.pointer.delta() != Vec2::ZERO || i.pointer.any_down()) {
        session.hide_pointer = false;
    }
    if session.hide_pointer && response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::None);
    }

    // Alt+hover: show when the row under the pointer last changed.
    if session.scrollback == 0 && ui.input(|i| i.modifiers.alt) {
        if let Some(pos) = response.hover_pos() {
            let (row, _) = metrics.cell_at(pos);
            if row >= 0 {
                if let Some(t) = session.row_time(row as usize) {
                    egui::show_tooltip_at_pointer(
                        ui.ctx(),
                        ui.layer_id(),
                        egui::Id::new("row_timestamp"),
                        |ui| {
                            ui.label(format!("Row updated {}", t.format("%H:%M:%S")));
                        },
                    );
                }
            }
        }
    }

    session.parser.set_scrollback(session.scrollback);
    paint_screen(&painter, session, search, &metrics, &font);

    // Scroll position indicator.
    if session.scrollback > 0 {
        let label = format!("[scrollback +{}]", session.scrollback);
        painter.text(
            Pos2::new(avail.right() - 8.0, avail.top() + 4.0),
            Align2::RIGHT_TOP,
            label,
            FontId::proportional(12.0),
            theme::current().accent,
        );
    }
    out
}

fn handle_keyboard(ui: &mut egui::Ui, session: &mut Session, out: &mut TermOutput) {
    let events = ui.input(|i| i.events.clone());
    for ev in events {
        match ev {
            egui::Event::Text(t) => {
                send(session, out, t.into_bytes());
            }
            egui::Event::Paste(text) => {
                if text.contains('\n') {
                    out.paste_dialog = Some(text);
                } else {
                    let payload = paste_payload(&text, session.out_parser.bracketed_paste);
                    send(session, out, payload);
                }
            }
            egui::Event::Copy => {
                copy_selection(session);
            }
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                let ctrl = modifiers.ctrl || modifiers.command;
                // Ctrl+C copies an active selection instead of interrupting.
                if ctrl && key == egui::Key::C && session.selection.is_active() {
                    copy_selection(session);
                    session.selection.clear();
                    continue;
                }
                // Enter copies an active selection.
                if key == egui::Key::Enter && session.selection.is_active() {
                    copy_selection(session);
                    session.selection.clear();
                    continue;
                }
                if ctrl && key == egui::Key::P {
                    out.upload_image = true;
                    continue;
                }
                // Ctrl+V / Ctrl+C arrive as Paste/Copy events; skip the key.
                if ctrl && (key == egui::Key::V || key == egui::Key::C || key == egui::Key::X) {
                    continue;
                }
                if let Some(bytes) = egui_key_to_bytes(key, modifiers) {
                    // Any input snaps the view back to the live screen.
                    session.scrollback = 0;
                    send(session, out, bytes);
                }
            }
            _ => {}
        }
    }
}

fn send(session: &mut Session, out: &mut TermOutput, bytes: Vec<u8>) {
    out.input_sent.push(bytes.clone());
    session.hide_pointer = true;
    session.send_input(bytes);
}

fn selection_text(session: &Session) -> String {
    let screen = session.parser.screen();
    session.selection.extract_text(|row| {
        if row < 0 {
            return String::new();
        }
        screen.contents_between(row as u16, 0, row as u16 + 1, 0)
    })
}

fn copy_selection(session: &mut Session) {
    let text = selection_text(session);
    if !text.is_empty() {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            cb.set_text(text).ok();
        }
    }
}

fn handle_pointer(
    ui: &mut egui::Ui,
    session: &mut Session,
    response: &egui::Response,
    metrics: &CellMetrics,
) {
    let shift = ui.input(|i| i.modifiers.shift);
    let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
    let remote_mouse = session.out_parser.mouse_mode && !shift;
    let alt_screen = session.parser.screen().alternate_screen();

    // Wheel: remote forwarding in mouse mode, else local scrollback.
    let scroll = ui.input(|i| i.raw_scroll_delta.y);
    if scroll != 0.0 && response.hovered() {
        if remote_mouse {
            if let Some(pos) = response.hover_pos() {
                let (row, col) = metrics.cell_at(pos);
                let bytes = mouse::sgr_scroll(scroll > 0.0, col as u16 + 1, row as u16 + 1);
                session.send_input(bytes);
            }
        } else if !alt_screen {
            let lines = (scroll / metrics.h).abs().ceil() as usize * 3;
            if scroll > 0.0 {
                session.scrollback =
                    (session.scrollback + lines).min(crate::gui::session::SCROLLBACK_LINES);
            } else {
                session.scrollback = session.scrollback.saturating_sub(lines);
            }
        }
    }

    // Register the context menu before the pointer-position early-return:
    // once the menu is open the pointer hovers the popup, not the terminal,
    // and the menu must still be shown on those frames.
    if !remote_mouse {
        response.context_menu(|ui| {
            if ui.button("Copy").clicked() {
                copy_selection(session);
                session.selection.clear();
                ui.close_menu();
            }
            if ui.button("Paste").clicked() {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    if let Ok(text) = cb.get_text() {
                        let payload = paste_payload(&text, session.out_parser.bracketed_paste);
                        session.send_input(payload);
                    }
                }
                ui.close_menu();
            }
            if session.selection.is_active() && ui.button("Search online").clicked() {
                let text = selection_text(session);
                let query = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !query.is_empty() {
                    open_url(&format!(
                        "https://www.google.com/search?q={}",
                        url_encode(&query)
                    ));
                }
                session.selection.clear();
                ui.close_menu();
            }
        });
    }

    let Some(pos) = response.interact_pointer_pos().or(response.hover_pos()) else {
        return;
    };
    let (row, col) = metrics.cell_at(pos);
    let (col1, row1) = ((col + 1).max(1) as u16, (row + 1).max(1) as u16);

    if remote_mouse {
        if response.drag_started_by(egui::PointerButton::Primary) {
            session.send_input(mouse::sgr_button(MouseButton::Left, col1, row1, true));
        } else if response.dragged_by(egui::PointerButton::Primary) {
            session.send_input(mouse::sgr_drag(MouseButton::Left, col1, row1));
        } else if response.drag_stopped_by(egui::PointerButton::Primary) {
            session.send_input(mouse::sgr_button(MouseButton::Left, col1, row1, false));
        } else if response.clicked() {
            session.send_input(mouse::sgr_button(MouseButton::Left, col1, row1, true));
            session.send_input(mouse::sgr_button(MouseButton::Left, col1, row1, false));
        } else if response.secondary_clicked() {
            session.send_input(mouse::sgr_button(MouseButton::Right, col1, row1, true));
            session.send_input(mouse::sgr_button(MouseButton::Right, col1, row1, false));
        }
        return;
    }

    // Ctrl-click: open URL or copy path under the cursor.
    if ctrl && response.clicked() {
        if row >= 0 && col >= 0 {
            let text = session
                .parser
                .screen()
                .contents_between(row as u16, 0, row as u16 + 1, 0);
            if let Some(span) = link_at(&text, col as usize) {
                if span.starts_with("http") {
                    open_url(&span);
                } else if let Ok(mut cb) = arboard::Clipboard::new() {
                    cb.set_text(span).ok();
                }
                return;
            }
        }
    }

    // Local selection (primary button only, so right-click keeps it intact).
    let cell = CellPos { row, col };
    if response.drag_started_by(egui::PointerButton::Primary) {
        session.selection.start(cell);
    } else if response.dragged_by(egui::PointerButton::Primary) {
        session.selection.drag(cell);
    } else if response.clicked() {
        session.selection.clear();
    }
}

/// Terminal cell width of a string (CJK and other wide chars count as 2).
fn display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    s.width()
}

/// Find a URL or absolute path span covering the given column.
fn link_at(row_text: &str, col: usize) -> Option<String> {
    for (start, token) in tokenize(row_text) {
        let len = display_width(token);
        if col >= start && col < start + len {
            if token.starts_with("http://") || token.starts_with("https://") {
                return Some(token.trim_end_matches([')', '.', ',', ';']).to_string());
            }
            if (token.starts_with('/') || token.starts_with("~/")) && token.len() > 1 {
                return Some(token.trim_end_matches([':', '.', ',']).to_string());
            }
        }
    }
    None
}

/// Whitespace-separated tokens with their starting terminal column
/// (display-width aware, so columns stay correct past CJK text).
fn tokenize(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut col = 0;
    for part in text.split_inclusive(char::is_whitespace) {
        let token = part.trim_end_matches(char::is_whitespace);
        if !token.is_empty() {
            out.push((col, token));
        }
        col += display_width(part);
    }
    out
}

/// Percent-encode a search query for use in a URL query string.
/// Unreserved characters (RFC 3986) pass through; spaces become `+`.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn open_url(url: &str) {
    #[cfg(windows)]
    std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .ok();
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn().ok();
    #[cfg(all(unix, not(target_os = "macos")))]
    std::process::Command::new("xdg-open").arg(url).spawn().ok();
}

fn paint_screen(
    painter: &egui::Painter,
    session: &Session,
    search: &SearchState,
    metrics: &CellMetrics,
    font: &FontId,
) {
    let screen = session.parser.screen();
    let (rows, cols) = screen.size();

    // Background fills and text runs, batched per row by style. Wide (CJK)
    // glyphs are painted individually in two-cell rects so the batched run's
    // monospace advance never drifts from the grid.
    for row in 0..rows {
        let mut run = String::new();
        let mut run_cells: u16 = 0;
        let mut run_start: u16 = 0;
        let mut run_fg = theme::current().term_fg;
        let mut run_bg: Option<Color32> = None;
        let mut run_bold = false;
        let mut run_underline = false;

        let flush = |painter: &egui::Painter,
                     run: &mut String,
                     cells: &mut u16,
                     start: u16,
                     fg: Color32,
                     bg: Option<Color32>,
                     underline: bool,
                     row: u16| {
            if run.is_empty() {
                return;
            }
            let rect = metrics.rect(row, start, *cells);
            if let Some(bg) = bg {
                painter.rect_filled(rect, 0.0, bg);
            }
            painter.text(rect.min, Align2::LEFT_TOP, run.as_str(), font.clone(), fg);
            if underline {
                painter.line_segment(
                    [
                        Pos2::new(rect.left(), rect.bottom() - 1.0),
                        Pos2::new(rect.right(), rect.bottom() - 1.0),
                    ],
                    Stroke::new(1.0, fg),
                );
            }
            run.clear();
            *cells = 0;
        };

        for col in 0..cols {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let selected = session.selection.contains(row as i32, col as i32);
            let (mut fg, mut bg) = cell_colors(cell);
            if selected {
                bg = Some(theme::current().selection_bg);
            }
            if cell.inverse() {
                let old_fg = fg;
                fg = bg.unwrap_or(theme::current().term_bg);
                bg = Some(old_fg);
            }
            let bold = cell.bold();
            let underline = cell.underline();
            let contents = cell.contents();
            let ch = if contents.is_empty() { " " } else { &contents };

            if cell.is_wide() {
                flush(
                    painter,
                    &mut run,
                    &mut run_cells,
                    run_start,
                    run_fg,
                    run_bg,
                    run_underline,
                    row,
                );
                let rect = metrics.rect(row, col, 2);
                if let Some(bg) = bg {
                    painter.rect_filled(rect, 0.0, bg);
                }
                painter.text(
                    Pos2::new(rect.center().x, rect.top()),
                    Align2::CENTER_TOP,
                    ch,
                    font.clone(),
                    fg,
                );
                if underline {
                    painter.line_segment(
                        [
                            Pos2::new(rect.left(), rect.bottom() - 1.0),
                            Pos2::new(rect.right(), rect.bottom() - 1.0),
                        ],
                        Stroke::new(1.0, fg),
                    );
                }
                continue;
            }

            if run.is_empty() {
                run_start = col;
                run_fg = fg;
                run_bg = bg;
                run_bold = bold;
                run_underline = underline;
                run.push_str(ch);
            } else if fg == run_fg && bg == run_bg && bold == run_bold && underline == run_underline
            {
                run.push_str(ch);
            } else {
                flush(
                    painter,
                    &mut run,
                    &mut run_cells,
                    run_start,
                    run_fg,
                    run_bg,
                    run_underline,
                    row,
                );
                run_start = col;
                run_fg = fg;
                run_bg = bg;
                run_bold = bold;
                run_underline = underline;
                run.push_str(ch);
            }
            run_cells += 1;
        }
        flush(
            painter,
            &mut run,
            &mut run_cells,
            run_start,
            run_fg,
            run_bg,
            run_underline,
            row,
        );

        // Link highlight: underline detected URLs/paths in this row.
        let text = screen.contents_between(row, 0, row + 1, 0);
        for (start, token) in tokenize(&text) {
            if token.starts_with("http://") || token.starts_with("https://") {
                let rect = metrics.rect(row, start as u16, display_width(token) as u16);
                painter.line_segment(
                    [
                        Pos2::new(rect.left(), rect.bottom() - 1.0),
                        Pos2::new(rect.right(), rect.bottom() - 1.0),
                    ],
                    Stroke::new(1.0, theme::current().link),
                );
            }
        }
    }

    // Search highlights for matches visible at the current scrollback.
    for (i, m) in search.matches.iter().enumerate() {
        if m.scrollback != session.scrollback {
            continue;
        }
        let color = if i == search.active {
            theme::current().search_active_bg
        } else {
            theme::current().search_match_bg
        };
        let rect = metrics.rect(m.row, m.col, m.len);
        painter.rect_filled(rect, 2.0, color.gamma_multiply(0.55));
        painter.rect_stroke(rect, 2.0, Stroke::new(1.0, color));
    }

    // Cursor (only on the live screen).
    if session.scrollback == 0 && !screen.hide_cursor() && session.connected {
        let (r, c) = screen.cursor_position();

        // Bracket matching: outline the cursor's bracket and its partner.
        let grid: Vec<Vec<char>> = (0..rows)
            .map(|row| {
                (0..cols)
                    .map(|col| {
                        screen
                            .cell(row, col)
                            .map(|cell| cell.contents().chars().next().unwrap_or(' '))
                            .unwrap_or(' ')
                    })
                    .collect()
            })
            .collect();
        if let Some((mr, mc)) = brackets::match_bracket(&grid, r as usize, c as usize) {
            for (br, bc) in [(r, c), (mr as u16, mc as u16)] {
                painter.rect_stroke(
                    metrics.rect(br, bc, 1),
                    1.0,
                    Stroke::new(1.0, theme::current().accent),
                );
            }
        }

        // A wide (CJK) glyph under the cursor needs a two-cell block.
        let cursor_cells = match screen.cell(r, c) {
            Some(cell) if cell.is_wide() => 2,
            _ => 1,
        };
        let rect = metrics.rect(r, c, cursor_cells);
        painter.rect_filled(rect, 1.0, theme::current().cursor.gamma_multiply(0.8));
        if let Some(cell) = screen.cell(r, c) {
            let contents = cell.contents();
            if !contents.is_empty() {
                painter.text(
                    Pos2::new(rect.center().x, rect.top()),
                    Align2::CENTER_TOP,
                    contents,
                    font.clone(),
                    theme::current().term_bg,
                );
            }
        }
    }
}

fn cell_colors(cell: &vt100::Cell) -> (Color32, Option<Color32>) {
    let fg = theme::vt_color(cell.fgcolor(), theme::current().term_fg);
    let bg = match cell.bgcolor() {
        vt100::Color::Default => None,
        other => Some(theme::vt_color(other, theme::current().term_bg)),
    };
    (fg, bg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encode_passes_unreserved() {
        assert_eq!(url_encode("rust-lang_1.0~ok"), "rust-lang_1.0~ok");
    }

    #[test]
    fn url_encode_escapes_reserved_and_spaces() {
        assert_eq!(url_encode("a b&c=d?"), "a+b%26c%3Dd%3F");
        assert_eq!(url_encode("100%"), "100%25");
    }

    #[test]
    fn url_encode_handles_utf8() {
        assert_eq!(url_encode("héllo"), "h%C3%A9llo");
    }

    #[test]
    fn tokenize_columns_count_wide_chars_as_two() {
        // "你好 x": 你好 spans cols 0-3, the space is col 4, x starts at col 5.
        let tokens = tokenize("你好 x");
        assert_eq!(tokens, vec![(0, "你好"), (5, "x")]);
    }

    #[test]
    fn link_at_past_cjk_text() {
        // URL starts at col 5 (after a 2x2-cell CJK token and a space).
        let row = "你好 https://example.com tail";
        assert_eq!(link_at(row, 4), None); // the space
        assert_eq!(
            link_at(row, 6).as_deref(),
            Some("https://example.com"),
            "column inside the URL must resolve despite preceding wide chars"
        );
    }

    #[test]
    fn display_width_counts_cells() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("你好"), 4);
        assert_eq!(display_width("ｱｲｳ"), 3); // halfwidth katakana stay 1 cell
    }
}
