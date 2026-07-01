//! Connection form and modal overlays: help, paste dialog, add snippet,
//! upload confirmation, tab search.

use crate::gui::session::PendingConn;
use crate::gui::theme::{self, Weight};
use crate::gui::{chrome, icons};
use egui::{Color32, Rounding};
use std::path::PathBuf;

/// Modal ownership: while any modal is open, terminal input is not forwarded.
pub enum Modal {
    None,
    AddSnippet { name: String, command: String },
    Paste { text: String },
    UploadConfirm { paths: Vec<PathBuf>, folder: String },
    TabSearch { query: String },
    Palette { query: String, selected: usize },
}

impl Modal {
    pub fn is_open(&self) -> bool {
        !matches!(self, Modal::None)
    }
}

pub enum PasteAction {
    None,
    Send,
    SendLineByLine,
    Cancel,
}

/// A single 38px-tall, rounding-10, `#e2e8f0`-bordered text input, matching
/// the connection form's input styling.
fn form_input(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: &str,
    width: f32,
    password: bool,
) -> egui::Response {
    ui.scope(|ui| {
        let v = &mut ui.style_mut().visuals;
        v.widgets.inactive.rounding = Rounding::same(10.0);
        v.widgets.inactive.bg_stroke = egui::Stroke::new(1.5, Color32::from_rgb(0xe2, 0xe8, 0xf0));
        v.widgets.hovered.rounding = Rounding::same(10.0);
        v.widgets.active.rounding = Rounding::same(10.0);
        ui.add_sized(
            [width, 38.0],
            egui::TextEdit::singleline(text)
                .hint_text(hint)
                .password(password),
        )
    })
    .inner
}

/// Render the connection form: centered content inside the terminal card,
/// matching the design handoff's "new tab" screen.
pub fn connection_form(ui: &mut egui::Ui, form: &mut PendingConn) -> bool {
    let mut connect = false;
    let t = theme::chrome();
    ui.vertical_centered(|ui| {
        let top_pad = ((ui.available_height() - 420.0) / 2.0).max(24.0);
        ui.add_space(top_pad);
        chrome::logo_tile(ui, 40.0);
        ui.add_space(14.0);
        ui.label(
            egui::RichText::new("New connection")
                .font(theme::font(Weight::Bold, 17.0))
                .color(t.text_primary),
        );
        ui.label(
            egui::RichText::new("This tab isn't connected yet")
                .font(theme::font(Weight::Regular, 12.5))
                .color(t.text_dim),
        );
        ui.add_space(20.0);

        let host = form_input(ui, &mut form.host, "user@host[:port]", 280.0, false);
        ui.add_space(8.0);
        // Claim exactly 280pt (matching the other inputs) so the row
        // centers identically. Lay out right-to-left: the Browse button
        // takes its natural width at the right edge, and the key-path
        // input fills the rest — keeping total width at 280 so the button
        // never pokes past the other fields' right edge.
        ui.allocate_ui_with_layout(
            egui::vec2(280.0, 38.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if ui.button("Browse…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        form.key_path = path.to_string_lossy().to_string();
                    }
                }
                let input_width = ui.available_width();
                form_input(
                    ui,
                    &mut form.key_path,
                    "optional; default keys auto-detected",
                    input_width,
                    false,
                );
            },
        );
        ui.add_space(8.0);
        form_input(
            ui,
            &mut form.password,
            "optional with key auth",
            280.0,
            true,
        );
        ui.add_space(8.0);
        form_input(
            ui,
            &mut form.save_as,
            "profile name (optional)",
            280.0,
            false,
        );
        ui.add_space(14.0);

        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        if host.lost_focus() && enter {
            connect = true;
        }
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(280.0, 40.0), egui::Sense::click());
        chrome::paint_gradient(ui, rect, 11.0);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Connect →",
            theme::font(Weight::SemiBold, 14.0),
            Color32::WHITE,
        );
        if resp.clicked() || enter {
            connect = true;
        }

        if form.connecting {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Connecting…");
            });
        }
        if let Some(err) = &form.error {
            ui.add_space(8.0);
            ui.colored_label(t.error, err);
        }

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("or pick a profile from the dock ←")
                .font(theme::font(Weight::Regular, 11.5))
                .color(Color32::from_rgb(0xcb, 0xd5, 0xe1)),
        );
    });
    connect && !form.connecting
}

/// A bottom sheet: scrim + a rounded-top card anchored to the bottom edge,
/// with a drag-handle bar for affordance (no slide-up animation).
fn bottom_sheet<R>(
    ctx: &egui::Context,
    id: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    chrome::paint_scrim(ctx);
    let frame = chrome::card_frame(20.0)
        .rounding(Rounding {
            nw: 20.0,
            ne: 20.0,
            sw: 0.0,
            se: 0.0,
        })
        .inner_margin(egui::Margin {
            left: 24.0,
            right: 24.0,
            top: 10.0,
            bottom: 20.0,
        });
    egui::Window::new(id)
        .id(egui::Id::new(id))
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, 0.0])
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(440.0);
            chrome::drag_handle(ui);
            ui.add_space(10.0);
            add_contents(ui)
        })
        .and_then(|ir| ir.inner)
}

/// A row of `n` equal-width sheet buttons: `(label, primary, enabled)`.
/// `primary` is filled with the brand gradient, everything else is flat.
fn sheet_buttons(ui: &mut egui::Ui, buttons: &[(&str, bool, bool)]) -> Option<usize> {
    let mut clicked = None;
    ui.columns(buttons.len(), |cols| {
        for (i, &(label, primary, enabled)) in buttons.iter().enumerate() {
            let ui = &mut cols[i];
            ui.add_enabled_ui(enabled, |ui| {
                let (rect, resp) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 38.0),
                    egui::Sense::click(),
                );
                if primary {
                    chrome::paint_gradient(ui, rect, 11.0);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        theme::font(Weight::SemiBold, 12.5),
                        Color32::WHITE,
                    );
                } else {
                    let last = i == buttons.len() - 1;
                    let fill = if last {
                        Color32::from_rgb(0xf8, 0xfa, 0xfc)
                    } else {
                        Color32::from_rgb(0xf5, 0xf3, 0xff)
                    };
                    let text_color = if last {
                        Color32::from_rgb(0x64, 0x74, 0x8b)
                    } else {
                        theme::chrome().accent
                    };
                    ui.painter().rect_filled(rect, Rounding::same(11.0), fill);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        theme::font(Weight::SemiBold, 12.5),
                        text_color,
                    );
                }
                if resp.clicked() {
                    clicked = Some(i);
                }
            });
        }
    });
    clicked
}

/// Multiline-paste confirmation, rendered as a bottom sheet (FR-012).
pub fn paste_dialog(ctx: &egui::Context, text: &mut String) -> PasteAction {
    let t = theme::chrome();
    bottom_sheet(ctx, "paste_sheet", |ui| {
        let mut action = PasteAction::None;
        let lines = text.lines().count();
        ui.label(
            egui::RichText::new("Paste multiline text?")
                .font(theme::font(Weight::Bold, 15.0))
                .color(t.text_primary),
        );
        ui.label(
            egui::RichText::new(format!(
                "Clipboard contains {lines} lines. Choose how to send them."
            ))
            .font(theme::font(Weight::Regular, 12.5))
            .color(Color32::from_rgb(0x64, 0x74, 0x8b)),
        );
        ui.add_space(10.0);
        egui::Frame::none()
            .fill(Color32::from_rgb(0xf8, 0xfa, 0xfc))
            .rounding(Rounding::same(10.0))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(text)
                                .desired_width(f32::INFINITY)
                                .frame(false)
                                .font(egui::TextStyle::Monospace),
                        );
                    });
            });
        ui.add_space(12.0);
        match sheet_buttons(
            ui,
            &[
                ("Send", true, true),
                ("Line-by-line", false, true),
                ("Cancel", false, true),
            ],
        ) {
            Some(0) => action = PasteAction::Send,
            Some(1) => action = PasteAction::SendLineByLine,
            Some(2) => action = PasteAction::Cancel,
            _ => {}
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = PasteAction::Cancel;
        }
        action
    })
    .unwrap_or(PasteAction::None)
}

/// Add-snippet dialog. Returns Some((name, command)) on save, and sets
/// `*cancelled` on dismiss.
pub fn add_snippet_dialog(
    ctx: &egui::Context,
    name: &mut String,
    command: &mut String,
    cancelled: &mut bool,
) -> Option<(String, String)> {
    let mut result = None;
    egui::Window::new("Add Snippet")
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(name);
            ui.label("Command");
            ui.text_edit_singleline(command);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ok = !name.trim().is_empty() && !command.trim().is_empty();
                if ui.add_enabled(ok, egui::Button::new("Save")).clicked() {
                    result = Some((name.trim().to_string(), command.trim().to_string()));
                }
                if ui.button("Cancel").clicked() {
                    *cancelled = true;
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                *cancelled = true;
            }
        });
    result
}

pub enum UploadAction {
    None,
    Confirm,
    Cancel,
}

/// Drag-and-drop upload confirmation, rendered as a bottom sheet (FR-019).
pub fn upload_confirm_dialog(
    ctx: &egui::Context,
    paths: &[PathBuf],
    folder: &mut String,
) -> UploadAction {
    let t = theme::chrome();
    bottom_sheet(ctx, "upload_sheet", |ui| {
        let mut action = UploadAction::None;
        ui.label(
            egui::RichText::new("Upload to remote")
                .font(theme::font(Weight::Bold, 15.0))
                .color(t.text_primary),
        );
        ui.add_space(10.0);
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                for p in paths {
                    let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| p.display().to_string());
                    egui::Frame::none()
                        .fill(Color32::from_rgb(0xf8, 0xfa, 0xfc))
                        .rounding(Rounding::same(8.0))
                        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (icon_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(16.0, 16.0),
                                    egui::Sense::hover(),
                                );
                                icons::folder(ui.painter(), icon_rect, t.accent);
                                ui.label(
                                    egui::RichText::new(&name)
                                        .font(theme::font(Weight::Medium, 12.5))
                                        .color(t.text_primary),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(human_size(size))
                                                .font(theme::font(Weight::Regular, 11.0))
                                                .color(t.text_dim),
                                        );
                                    },
                                );
                            });
                        });
                    ui.add_space(4.0);
                }
            });
        ui.add_space(8.0);
        ui.scope(|ui| {
            let v = &mut ui.style_mut().visuals;
            v.widgets.inactive.rounding = Rounding::same(10.0);
            v.widgets.inactive.bg_stroke =
                egui::Stroke::new(1.5, Color32::from_rgb(0xe2, 0xe8, 0xf0));
            ui.add_sized(
                [ui.available_width(), 36.0],
                egui::TextEdit::singleline(folder).font(egui::TextStyle::Monospace),
            );
        });
        ui.add_space(12.0);
        let ok = !folder.trim().is_empty();
        match sheet_buttons(ui, &[("Upload", true, ok), ("Cancel", false, true)]) {
            Some(0) => action = UploadAction::Confirm,
            Some(1) => action = UploadAction::Cancel,
            _ => {}
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = UploadAction::Cancel;
        }
        action
    })
    .unwrap_or(UploadAction::None)
}

pub fn human_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MiB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// Command palette actions (TR-003).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaletteAction {
    None,
    Close,
    NewTab,
    CloseTab,
    SearchTabs,
    ToggleFocus,
    ToggleSync,
    OpenHelp,
    OpenFileTools,
    SetTheme(usize),
    RunSnippet(usize),
}

/// All palette entries for the current app state, in display order.
pub fn palette_entries(config: &crate::config::Config) -> Vec<(String, PaletteAction)> {
    let mut entries: Vec<(String, PaletteAction)> = vec![
        ("New tab".into(), PaletteAction::NewTab),
        ("Close tab".into(), PaletteAction::CloseTab),
        ("Search tabs".into(), PaletteAction::SearchTabs),
        ("Toggle focus mode".into(), PaletteAction::ToggleFocus),
        ("Toggle sync input".into(), PaletteAction::ToggleSync),
        ("Open help".into(), PaletteAction::OpenHelp),
        ("Open file tools".into(), PaletteAction::OpenFileTools),
    ];
    for (i, t) in theme::THEMES.iter().enumerate() {
        entries.push((format!("Theme: {}", t.name), PaletteAction::SetTheme(i)));
    }
    for (i, s) in config.snippets.iter().enumerate() {
        entries.push((format!("Snippet: {}", s.name), PaletteAction::RunSnippet(i)));
    }
    entries
}

/// Case-insensitive substring filter; returns indices into `entries`.
pub fn filter_palette(entries: &[(String, PaletteAction)], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, (label, _))| q.is_empty() || label.to_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect()
}

/// Which eyebrow group a palette entry displays under.
fn group_of(action: PaletteAction) -> &'static str {
    match action {
        PaletteAction::SearchTabs => "NAVIGATE",
        PaletteAction::NewTab
        | PaletteAction::CloseTab
        | PaletteAction::ToggleFocus
        | PaletteAction::ToggleSync => "SESSION",
        _ => "TOOLS",
    }
}

/// The bound shortcut shown at the right of a palette row, if any.
fn shortcut_for(action: PaletteAction) -> Option<&'static str> {
    match action {
        PaletteAction::SearchTabs => Some("Ctrl Shift P"),
        PaletteAction::ToggleFocus => Some("F11"),
        PaletteAction::OpenFileTools => Some("Ctrl Shift K"),
        _ => None,
    }
}

/// Command palette window. Returns the chosen action (or None/Close).
pub fn command_palette(
    ctx: &egui::Context,
    config: &crate::config::Config,
    query: &mut String,
    selected: &mut usize,
) -> PaletteAction {
    let mut action = PaletteAction::None;
    let t = theme::chrome();
    let entries = palette_entries(config);
    let mut visible = filter_palette(&entries, query);
    let group_rank = |g: &str| match g {
        "NAVIGATE" => 0,
        "SESSION" => 1,
        _ => 2,
    };
    visible.sort_by_key(|&i| group_rank(group_of(entries[i].1)));
    *selected = (*selected).min(visible.len().saturating_sub(1));

    chrome::paint_scrim(ctx);
    egui::Window::new("Command Palette")
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 46.0])
        .frame(chrome::card_frame(22.0).shadow(chrome::shadow(
            egui::Vec2::new(0.0, 24.0),
            48.0,
            Color32::from_rgba_unmultiplied(0x0f, 0x17, 0x2a, 89),
        )))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.horizontal(|ui| {
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                icons::search(ui.painter(), icon_rect, t.text_dim);
                let edit = ui.add(
                    egui::TextEdit::singleline(query)
                        .hint_text("Type a command or search…")
                        .frame(false)
                        .font(theme::font(Weight::Regular, 13.5))
                        .desired_width(ui.available_width() - 40.0),
                );
                edit.request_focus();
                if edit.changed() {
                    *selected = 0;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::none()
                        .fill(Color32::from_rgb(0xf8, 0xfa, 0xfc))
                        .rounding(Rounding::same(5.0))
                        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("ESC")
                                    .font(theme::font(Weight::SemiBold, 10.0))
                                    .color(t.text_dim),
                            );
                        });
                });
            });
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                *selected = (*selected + 1).min(visible.len().saturating_sub(1));
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                *selected = selected.saturating_sub(1);
            }
            *selected = (*selected).min(visible.len().saturating_sub(1));

            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    let mut last_group: Option<&str> = None;
                    for (vis_i, &entry_i) in visible.iter().enumerate() {
                        let (label, act) = &entries[entry_i];
                        let group = group_of(*act);
                        if last_group != Some(group) {
                            if last_group.is_some() {
                                ui.add_space(8.0);
                            }
                            ui.label(
                                egui::RichText::new(group)
                                    .font(theme::font(Weight::SemiBold, 10.0))
                                    .color(Color32::from_rgb(0xcb, 0xd5, 0xe1)),
                            );
                            last_group = Some(group);
                        }
                        let is_sel = vis_i == *selected;
                        let row = egui::Frame::none()
                            .fill(if is_sel {
                                Color32::from_rgb(0xf5, 0xf3, 0xff)
                            } else {
                                Color32::TRANSPARENT
                            })
                            .rounding(Rounding::same(9.0))
                            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(label)
                                            .font(theme::font(Weight::Medium, 13.0))
                                            .color(t.text_primary),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if let Some(shortcut) = shortcut_for(*act) {
                                                ui.label(
                                                    egui::RichText::new(shortcut)
                                                        .monospace()
                                                        .size(10.5)
                                                        .color(t.text_dim),
                                                );
                                            }
                                        },
                                    );
                                });
                            });
                        let resp = ui.interact(
                            row.response.rect,
                            row.response.id.with("row"),
                            egui::Sense::click(),
                        );
                        if is_sel {
                            resp.scroll_to_me(None);
                        }
                        if resp.clicked() {
                            action = *act;
                        }
                    }
                    if visible.is_empty() {
                        ui.weak("No matching commands");
                    }
                });

            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(&entry_i) = visible.get(*selected) {
                    action = entries[entry_i].1;
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                action = PaletteAction::Close;
            }
        });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_filter_is_case_insensitive() {
        let entries = palette_entries(&crate::config::Config::default());
        let hits = filter_palette(&entries, "FOCUS");
        assert_eq!(hits.len(), 1);
        assert_eq!(entries[hits[0]].0, "Toggle focus mode");
    }

    #[test]
    fn palette_empty_query_lists_all() {
        let entries = palette_entries(&crate::config::Config::default());
        assert_eq!(filter_palette(&entries, "").len(), entries.len());
    }

    #[test]
    fn palette_includes_themes_and_snippets() {
        let mut config = crate::config::Config::default();
        config.snippets.push(crate::config::Snippet {
            name: "uptime".into(),
            command: "uptime".into(),
        });
        let entries = palette_entries(&config);
        assert!(entries.iter().any(|(l, _)| l == "Theme: Lavender"));
        assert!(entries
            .iter()
            .any(|(l, a)| l == "Snippet: uptime" && *a == PaletteAction::RunSnippet(0)));
    }
}
