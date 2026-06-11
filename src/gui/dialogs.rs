//! Connection form and modal overlays: help, paste dialog, add snippet,
//! upload confirmation, tab search.

use crate::gui::session::PendingConn;
use crate::gui::theme;
use std::path::PathBuf;

/// Modal ownership: while any modal is open, terminal input is not forwarded.
pub enum Modal {
    None,
    Help,
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

/// Render the connection form. Returns true when a connect was requested.
pub fn connection_form(ui: &mut egui::Ui, form: &mut PendingConn) -> bool {
    let mut connect = false;
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.heading("New SSH Connection");
        ui.add_space(16.0);
        egui::Grid::new("conn_form")
            .num_columns(2)
            .spacing([8.0, 10.0])
            .show(ui, |ui| {
                ui.label("Host");
                let host = ui.add(
                    egui::TextEdit::singleline(&mut form.host)
                        .hint_text("user@host[:port]")
                        .desired_width(280.0),
                );
                ui.end_row();

                ui.label("Key file");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut form.key_path)
                            .hint_text("optional; default keys auto-detected")
                            .desired_width(216.0),
                    );
                    if ui.button("Browse…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            form.key_path = path.to_string_lossy().to_string();
                        }
                    }
                });
                ui.end_row();

                ui.label("Password");
                ui.add(
                    egui::TextEdit::singleline(&mut form.password)
                        .password(true)
                        .hint_text("optional with key auth")
                        .desired_width(280.0),
                );
                ui.end_row();

                ui.label("Save as");
                ui.add(
                    egui::TextEdit::singleline(&mut form.save_as)
                        .hint_text("profile name (optional)")
                        .desired_width(280.0),
                );
                ui.end_row();

                if host.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    connect = true;
                }
            });
        ui.add_space(12.0);
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui
            .add_sized([120.0, 30.0], egui::Button::new("Connect"))
            .clicked()
            || enter
        {
            connect = true;
        }
        if form.connecting {
            ui.add_space(8.0);
            ui.spinner();
            ui.label("Connecting…");
        }
        if let Some(err) = &form.error {
            ui.add_space(8.0);
            ui.colored_label(theme::current().error, err);
        }
    });
    connect && !form.connecting
}

/// Paste confirmation dialog for multiline text.
pub fn paste_dialog(ctx: &egui::Context, text: &mut String) -> PasteAction {
    let mut action = PasteAction::None;
    egui::Window::new("Confirm Paste")
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let lines = text.lines().count();
            ui.label(format!("Pasting {lines} lines:"));
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(text)
                            .desired_width(420.0)
                            .font(egui::TextStyle::Monospace),
                    );
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Send").clicked() {
                    action = PasteAction::Send;
                }
                if ui.button("Send line-by-line").clicked() {
                    action = PasteAction::SendLineByLine;
                }
                if ui.button("Cancel").clicked() {
                    action = PasteAction::Cancel;
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                action = PasteAction::Cancel;
            }
        });
    action
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

/// Drag-and-drop upload confirmation.
pub fn upload_confirm_dialog(
    ctx: &egui::Context,
    paths: &[PathBuf],
    folder: &mut String,
) -> UploadAction {
    let mut action = UploadAction::None;
    egui::Window::new("Upload Files")
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!(
                "Upload {} item(s) to the remote host:",
                paths.len()
            ));
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    for p in paths {
                        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                        ui.monospace(format!("{}  ({})", p.display(), human_size(size)));
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Remote folder name:");
                ui.text_edit_singleline(folder);
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ok = !folder.trim().is_empty();
                if ui.add_enabled(ok, egui::Button::new("Upload")).clicked() {
                    action = UploadAction::Confirm;
                }
                if ui.button("Cancel").clicked() {
                    action = UploadAction::Cancel;
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                action = UploadAction::Cancel;
            }
        });
    action
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

/// Help overlay. Returns true while open; false once dismissed.
pub fn help_overlay(ctx: &egui::Context) -> bool {
    let mut open = true;
    egui::Window::new("Help")
        .collapsible(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(400.0)
                .show(ui, |ui| {
                    ui.heading("Shortcuts");
                    let rows = [
                        ("Ctrl+Shift+F", "Find in terminal scrollback"),
                        ("Ctrl+Shift+P", "Search open tabs"),
                        ("Ctrl+Shift+K", "Command palette"),
                        ("Alt+Hover", "Show when a row last changed"),
                        ("F11", "Toggle focus mode (hide sidebar)"),
                        ("Ctrl+C", "Copy selection (or send interrupt)"),
                        ("Ctrl+V", "Paste (multiline opens a dialog)"),
                        ("Ctrl+P", "Upload clipboard image over SCP"),
                        ("Enter", "Copy selection when one is active"),
                        ("Ctrl+Click", "Open URL / copy path under cursor"),
                        ("Shift+Drag", "Local selection while app uses mouse"),
                        ("Mouse wheel", "Scrollback (when no TUI owns mouse)"),
                    ];
                    egui::Grid::new("help_keys").striped(true).show(ui, |ui| {
                        for (k, v) in rows {
                            ui.monospace(k);
                            ui.label(v);
                            ui.end_row();
                        }
                    });
                    ui.add_space(8.0);
                    ui.heading("Behaviors");
                    ui.label("• Drag & drop files onto a terminal to upload them over SCP.");
                    ui.label("• Successful connections are saved as profiles automatically.");
                    ui.label("• Sync input broadcasts typed input to all connected tabs.");
                    ui.label("• Snippets send their command plus Enter to the active tab.");
                    ui.label("• Right-click: Copy / Paste / Search online (with a selection).");
                    ui.label("• The mouse cursor hides while typing; move it to bring it back.");
                    ui.label("• File tools (sidebar/palette): dual-pane local + remote manager.");
                    ui.label("• Remote Edit re-uploads the file every time you save it locally.");
                    ui.label("• Themes persist; pick one in the sidebar Display section.");
                    ui.label("• Keep alive (sidebar Session) probes idle sessions every 30 s.");
                });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                // handled via window close button state below
            }
        });
    open && !ctx.input(|i| i.key_pressed(egui::Key::Escape))
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

/// Command palette window. Returns the chosen action (or None/Close).
pub fn command_palette(
    ctx: &egui::Context,
    config: &crate::config::Config,
    query: &mut String,
    selected: &mut usize,
) -> PaletteAction {
    let mut action = PaletteAction::None;
    let entries = palette_entries(config);
    let visible = filter_palette(&entries, query);

    egui::Window::new("Command Palette")
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
        .show(ctx, |ui| {
            ui.set_min_width(380.0);
            let edit = ui.add(
                egui::TextEdit::singleline(query)
                    .hint_text("Type a command…")
                    .desired_width(f32::INFINITY),
            );
            edit.request_focus();
            if edit.changed() {
                *selected = 0;
            }

            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                *selected = (*selected + 1).min(visible.len().saturating_sub(1));
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                *selected = selected.saturating_sub(1);
            }
            *selected = (*selected).min(visible.len().saturating_sub(1));

            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    for (vis_i, &entry_i) in visible.iter().enumerate() {
                        let (label, act) = &entries[entry_i];
                        let is_sel = vis_i == *selected;
                        let resp = ui.selectable_label(is_sel, label);
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
        assert!(entries.iter().any(|(l, _)| l == "Theme: Paper"));
        assert!(entries
            .iter()
            .any(|(l, a)| l == "Snippet: uptime" && *a == PaletteAction::RunSnippet(0)));
    }
}
