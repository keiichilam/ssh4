//! Sidebar: profiles, snippets, display controls, debug log toggle, help.

use crate::config::Config;
use crate::gui::theme;

pub enum SidebarAction {
    None,
    ConnectProfile(String),
    DeleteProfile(String),
    RunSnippet(String),
    DeleteSnippet(usize),
    AddSnippet,
    OpenHelp,
    OpenFileTools,
    ToggleDebugLog,
    ToggleKeepAlive,
    OpenLogFolder,
    SetTheme(usize),
}

pub struct SidebarState {
    pub debug_log: bool,
    pub font_size: f32,
    pub ui_zoom: f32,
}

pub fn sidebar_ui(ui: &mut egui::Ui, config: &Config, state: &mut SidebarState) -> SidebarAction {
    let mut action = SidebarAction::None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(6.0);
        theme::section_header(ui, "PROFILES");
        ui.add_space(4.0);
        if config.profiles.is_empty() {
            ui.weak("No saved profiles yet");
        }
        let names: Vec<String> = config.profiles.keys().cloned().collect();
        for name in names {
            // Lay out right-to-left so the main button fills exactly the
            // remaining width; sizing it from available_width in a
            // left-to-right row makes the panel grow every frame. The row
            // height must be fixed: a bare with_layout child spans all
            // remaining panel height and Align::Center then parks the row
            // in the middle of it, shoving everything below off-screen.
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 22.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let profile = &config.profiles[&name];
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Delete profile")
                        .clicked()
                    {
                        action = SidebarAction::DeleteProfile(name.clone());
                    }
                    let label = ui.add(
                        egui::Button::new(&name)
                            .fill(theme::current().surface_raised)
                            .min_size(egui::vec2(ui.available_width(), 22.0)),
                    );
                    if label
                        .on_hover_text(format!(
                            "{}@{}:{}",
                            profile.user, profile.host, profile.port
                        ))
                        .clicked()
                    {
                        action = SidebarAction::ConnectProfile(name.clone());
                    }
                },
            );
        }

        ui.add_space(12.0);
        ui.separator();
        ui.horizontal(|ui| {
            theme::section_header(ui, "SNIPPETS");
            if ui.small_button("+").on_hover_text("Add snippet").clicked() {
                action = SidebarAction::AddSnippet;
            }
        });
        ui.add_space(4.0);
        if config.snippets.is_empty() {
            ui.weak("No snippets yet");
        }
        for (i, snippet) in config.snippets.iter().enumerate() {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 22.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if ui.small_button("🗑").clicked() {
                        action = SidebarAction::DeleteSnippet(i);
                    }
                    let b = ui.add(
                        egui::Button::new(&snippet.name)
                            .fill(theme::current().surface_raised)
                            .min_size(egui::vec2(ui.available_width(), 22.0)),
                    );
                    if b.on_hover_text(&snippet.command).clicked() {
                        action = SidebarAction::RunSnippet(snippet.command.clone());
                    }
                },
            );
        }

        ui.add_space(12.0);
        ui.separator();
        theme::section_header(ui, "DISPLAY");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Font");
            ui.add(egui::Slider::new(&mut state.font_size, 9.0..=24.0).step_by(1.0));
        });
        ui.horizontal(|ui| {
            ui.label("Zoom");
            ui.add(egui::Slider::new(&mut state.ui_zoom, 0.7..=2.0).step_by(0.1));
        });
        ui.horizontal(|ui| {
            ui.label("Theme");
            let mut selected = theme::current_index();
            egui::ComboBox::from_id_source("theme_picker")
                .selected_text(theme::THEMES[selected].name)
                .show_ui(ui, |ui| {
                    for (i, t) in theme::THEMES.iter().enumerate() {
                        ui.selectable_value(&mut selected, i, t.name);
                    }
                });
            if selected != theme::current_index() {
                action = SidebarAction::SetTheme(selected);
            }
        });
        if ui.button("Reset display").clicked() {
            state.font_size = 14.0;
            state.ui_zoom = 1.0;
        }

        ui.add_space(12.0);
        ui.separator();
        theme::section_header(ui, "SESSION");
        ui.add_space(4.0);
        let ka = ui
            .checkbox(&mut config.keep_alive.clone(), "Keep alive")
            .on_hover_text(
                "Send an SSH keep-alive probe every 30 s on connected tabs \
                 so idle sessions are not dropped by NAT/firewall timeouts",
            );
        if ka.clicked() {
            action = SidebarAction::ToggleKeepAlive;
        }

        ui.add_space(12.0);
        ui.separator();
        let log_path = std::env::temp_dir().join("ssh4-debug.log");
        let toggle = ui
            .checkbox(&mut state.debug_log.clone(), "Debug logging")
            .on_hover_text(format!("Log: {}", log_path.display()));
        if toggle.clicked() {
            action = SidebarAction::ToggleDebugLog;
        }
        if state.debug_log && ui.button("Open log folder").clicked() {
            action = SidebarAction::OpenLogFolder;
        }

        ui.add_space(12.0);
        if ui.button("File tools").clicked() {
            action = SidebarAction::OpenFileTools;
        }
        if ui.button("Help").clicked() {
            action = SidebarAction::OpenHelp;
        }
    });

    action
}
