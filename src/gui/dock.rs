//! Dock rail + contextual flyout: replaces the old fixed sidebar with a
//! slim icon dock (New / Hosts / Snippets / Files / Theme / Session / Help)
//! and a flyout panel whose content depends on which dock icon is active.

use crate::config::Config;
use crate::gui::theme::Weight;
use crate::gui::{chrome, icons, theme};

use egui::{Color32, Rounding, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSection {
    Hosts,
    Snippets,
    Theme,
    Session,
    Help,
}

pub enum DockAction {
    None,
    NewTab,
    ToggleFlyout(DockSection),
    OpenFileTools,
    ConnectProfile(String),
    DeleteProfile(String),
    RunSnippet(String),
    DeleteSnippet(usize),
    AddSnippet,
    ToggleDebugLog,
    OpenLogFolder,
    ToggleKeepAlive,
    SetTheme(usize),
}

/// Flyout-local state that isn't already owned by `Config`.
pub struct DockState {
    pub debug_log: bool,
    pub font_size: f32,
    pub ui_zoom: f32,
    pub hosts_query: String,
}

impl Default for DockState {
    fn default() -> Self {
        Self {
            debug_log: false,
            font_size: 14.0,
            ui_zoom: 1.0,
            hosts_query: String::new(),
        }
    }
}

/// A deterministic accent color for a profile's card border, derived from
/// its name (profiles have no persisted color of their own).
fn profile_color(name: &str) -> Color32 {
    let colors = theme::tab_colors();
    let hash = name
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33).wrapping_add(b as u32));
    colors[(hash as usize) % colors.len()].1
}

/// One dock rail item: icon tile + microlabel, highlighted when active.
fn dock_item(
    ui: &mut egui::Ui,
    icon: fn(&egui::Painter, egui::Rect, Color32),
    label: &str,
    active: bool,
) -> bool {
    let t = theme::chrome();
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::click());
        if active {
            ui.painter().rect_filled(
                rect,
                Rounding::same(12.0),
                Color32::from_rgb(0xf5, 0xf3, 0xff),
            );
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(rect, Rounding::same(12.0), t.surface_hover);
        }
        let icon_rect = rect.shrink((38.0 - 19.0) / 2.0);
        icon(
            ui.painter(),
            icon_rect,
            if active { t.accent } else { t.text_dim },
        );
        if resp.clicked() {
            clicked = true;
        }
        ui.label(
            egui::RichText::new(label)
                .font(theme::font(
                    if active {
                        Weight::SemiBold
                    } else {
                        Weight::Medium
                    },
                    9.0,
                ))
                .color(if active { t.accent } else { t.text_dim }),
        );
    });
    ui.add_space(4.0);
    clicked
}

/// The dock rail: fixed 84pt-wide column of navigation icons.
pub fn dock_rail(
    ui: &mut egui::Ui,
    active_flyout: Option<DockSection>,
    files_open: bool,
    session_live: bool,
) -> DockAction {
    let mut action = DockAction::None;
    let t = theme::chrome();

    ui.add_space(16.0);
    ui.vertical_centered(|ui| {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::click());
        chrome::paint_gradient(ui, rect, 12.0);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "+",
            theme::font(Weight::Bold, 18.0),
            Color32::WHITE,
        );
        if resp.clicked() {
            action = DockAction::NewTab;
        }
        ui.label(
            egui::RichText::new("New")
                .font(theme::font(Weight::SemiBold, 9.0))
                .color(t.accent),
        );
    });
    ui.add_space(8.0);
    let (line_rect, _) = ui.allocate_exact_size(egui::vec2(32.0, 1.0), egui::Sense::hover());
    ui.painter().hline(
        line_rect.x_range(),
        line_rect.center().y,
        egui::Stroke::new(1.0, Color32::from_rgb(0xf1, 0xf5, 0xf9)),
    );
    ui.add_space(8.0);

    if dock_item(
        ui,
        icons::link,
        "Hosts",
        active_flyout == Some(DockSection::Hosts),
    ) {
        action = DockAction::ToggleFlyout(DockSection::Hosts);
    }
    if dock_item(
        ui,
        icons::terminal,
        "Snippets",
        active_flyout == Some(DockSection::Snippets),
    ) {
        action = DockAction::ToggleFlyout(DockSection::Snippets);
    }
    if dock_item(ui, icons::folder, "Files", files_open) {
        action = DockAction::OpenFileTools;
    }
    if dock_item(
        ui,
        icons::sliders,
        "Theme",
        active_flyout == Some(DockSection::Theme),
    ) {
        action = DockAction::ToggleFlyout(DockSection::Theme);
    }

    // Flexible spacer: pushes Session + Help to the bottom of the rail.
    let remaining = (ui.available_height() - 112.0).max(0.0);
    ui.add_space(remaining);

    ui.vertical_centered(|ui| {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::click());
        let active = active_flyout == Some(DockSection::Session);
        if active {
            ui.painter().rect_filled(
                rect,
                Rounding::same(12.0),
                Color32::from_rgb(0xf5, 0xf3, 0xff),
            );
        }
        let icon_rect = rect.shrink((38.0 - 19.0) / 2.0);
        icons::pulse(
            ui.painter(),
            icon_rect,
            if active { t.accent } else { t.text_dim },
        );
        if session_live {
            let dot_center = rect.right_top() + Vec2::new(-4.0, 4.0);
            ui.painter().circle_filled(dot_center, 4.5, Color32::WHITE);
            ui.painter().circle_filled(dot_center, 3.5, t.success);
        }
        if resp.clicked() {
            action = DockAction::ToggleFlyout(DockSection::Session);
        }
        ui.label(
            egui::RichText::new("Session")
                .font(theme::font(
                    if active {
                        Weight::SemiBold
                    } else {
                        Weight::Medium
                    },
                    9.0,
                ))
                .color(if active { t.accent } else { t.text_dim }),
        );
    });
    ui.add_space(4.0);
    if dock_item(
        ui,
        icons::help,
        "Help",
        active_flyout == Some(DockSection::Help),
    ) {
        action = DockAction::ToggleFlyout(DockSection::Help);
    }

    action
}

/// The flyout panel's content, dispatched by active section.
pub fn flyout_ui(
    ui: &mut egui::Ui,
    section: DockSection,
    config: &Config,
    state: &mut DockState,
) -> DockAction {
    match section {
        DockSection::Hosts => hosts_flyout(ui, config, state),
        DockSection::Snippets => snippets_flyout(ui, config),
        DockSection::Theme => theme_flyout(ui, state),
        DockSection::Session => session_flyout(ui, config, state),
        DockSection::Help => {
            help_flyout(ui);
            DockAction::None
        }
    }
}

fn flyout_header(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .font(theme::font(Weight::Bold, 14.5))
            .color(theme::chrome().text_primary),
    );
    ui.add_space(10.0);
}

fn hosts_flyout(ui: &mut egui::Ui, config: &Config, state: &mut DockState) -> DockAction {
    let mut action = DockAction::None;
    let t = theme::chrome();
    flyout_header(ui, "Connections");

    ui.add(
        egui::TextEdit::singleline(&mut state.hosts_query)
            .hint_text("Search profiles…")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(10.0);

    egui::ScrollArea::vertical()
        .max_height((ui.available_height() - 46.0).max(0.0))
        .show(ui, |ui| {
            let q = state.hosts_query.to_lowercase();
            let mut names: Vec<String> = config.profiles.keys().cloned().collect();
            names.retain(|n| q.is_empty() || n.to_lowercase().contains(&q));
            if names.is_empty() {
                ui.weak("No saved profiles yet");
            }
            for name in names {
                let profile = &config.profiles[&name];
                let accent = profile_color(&name);
                let frame = egui::Frame::none()
                    .fill(Color32::from_rgb(0xfa, 0xfa, 0xfa))
                    .rounding(Rounding::same(12.0))
                    .inner_margin(egui::Margin {
                        left: 12.0,
                        right: 8.0,
                        top: 8.0,
                        bottom: 8.0,
                    });
                let resp = frame
                    .show(ui, |ui| {
                        let rect = ui.max_rect();
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
                            Rounding::same(1.5),
                            accent,
                        );
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&name)
                                        .font(theme::font(Weight::SemiBold, 12.5))
                                        .color(t.text_primary),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button("🗑")
                                            .on_hover_text("Delete profile")
                                            .clicked()
                                        {
                                            action = DockAction::DeleteProfile(name.clone());
                                        }
                                    },
                                );
                            });
                            ui.label(
                                egui::RichText::new(format!("{}@{}", profile.user, profile.host))
                                    .font(theme::font(Weight::Regular, 10.5))
                                    .color(t.text_dim),
                            );
                        });
                    })
                    .response;
                if ui
                    .interact(resp.rect, resp.id.with("click"), egui::Sense::click())
                    .clicked()
                {
                    action = DockAction::ConnectProfile(name.clone());
                }
                ui.add_space(6.0);
            }
        });

    ui.add_space(8.0);
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.0), egui::Sense::click());
    chrome::paint_gradient(ui, rect, 11.0);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "+ New connection",
        theme::font(Weight::SemiBold, 12.5),
        Color32::WHITE,
    );
    if resp.clicked() {
        action = DockAction::NewTab;
    }
    action
}

fn snippets_flyout(ui: &mut egui::Ui, config: &Config) -> DockAction {
    let mut action = DockAction::None;
    let t = theme::chrome();
    ui.horizontal(|ui| {
        flyout_header(ui, "Snippets");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if ui.small_button("+").on_hover_text("Add snippet").clicked() {
                action = DockAction::AddSnippet;
            }
        });
    });
    egui::ScrollArea::vertical().show(ui, |ui| {
        if config.snippets.is_empty() {
            ui.weak("No snippets yet");
        }
        for (i, snippet) in config.snippets.iter().enumerate() {
            let frame = egui::Frame::none()
                .fill(Color32::from_rgb(0xfa, 0xfa, 0xfa))
                .rounding(Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(10.0, 8.0));
            let resp = frame
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&snippet.name)
                                .font(theme::font(Weight::SemiBold, 12.5))
                                .color(t.text_primary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("🗑").clicked() {
                                action = DockAction::DeleteSnippet(i);
                            }
                        });
                    });
                })
                .response
                .on_hover_text(&snippet.command);
            if ui
                .interact(resp.rect, resp.id.with("click"), egui::Sense::click())
                .clicked()
            {
                action = DockAction::RunSnippet(snippet.command.clone());
            }
            ui.add_space(6.0);
        }
    });
    action
}

fn theme_flyout(ui: &mut egui::Ui, state: &mut DockState) -> DockAction {
    let mut action = DockAction::None;
    let t = theme::chrome();
    flyout_header(ui, "Appearance");

    let selected = theme::current_index();
    for (i, th) in theme::THEMES.iter().enumerate() {
        let is_sel = i == selected;
        let frame = egui::Frame::none()
            .fill(Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(
                if is_sel { 2.0 } else { 1.0 },
                if is_sel {
                    t.accent
                } else {
                    Color32::from_black_alpha(20)
                },
            ))
            .rounding(Rounding::same(12.0))
            .inner_margin(egui::Margin::symmetric(10.0, 8.0));
        let resp = frame
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(30.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, Rounding::same(8.0), th.term_bg);
                    ui.painter().rect_stroke(
                        rect,
                        Rounding::same(8.0),
                        egui::Stroke::new(1.0, t.border),
                    );
                    ui.label(
                        egui::RichText::new(th.name)
                            .font(theme::font(Weight::Medium, 12.0))
                            .color(Color32::from_rgb(0x33, 0x41, 0x55)),
                    );
                });
            })
            .response;
        if ui
            .interact(resp.rect, resp.id.with("theme"), egui::Sense::click())
            .clicked()
        {
            action = DockAction::SetTheme(i);
        }
        ui.add_space(8.0);
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Font");
        ui.add(egui::Slider::new(&mut state.font_size, 9.0..=24.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("Zoom");
        ui.add(egui::Slider::new(&mut state.ui_zoom, 0.7..=2.0).step_by(0.1));
    });
    if ui.button("Reset display").clicked() {
        state.font_size = 14.0;
        state.ui_zoom = 1.0;
    }
    action
}

fn session_flyout(ui: &mut egui::Ui, config: &Config, state: &mut DockState) -> DockAction {
    let mut action = DockAction::None;
    flyout_header(ui, "Session");

    let mut ka = config.keep_alive;
    if ui
        .checkbox(&mut ka, "Keep alive")
        .on_hover_text(
            "Send an SSH keep-alive probe every 30 s on connected tabs so idle \
             sessions are not dropped by NAT/firewall timeouts",
        )
        .clicked()
    {
        action = DockAction::ToggleKeepAlive;
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(12.0);
    let log_path = std::env::temp_dir().join("ssh4-debug.log");
    let mut debug = state.debug_log;
    if ui
        .checkbox(&mut debug, "Debug logging")
        .on_hover_text(format!("Log: {}", log_path.display()))
        .clicked()
    {
        action = DockAction::ToggleDebugLog;
    }
    if state.debug_log && ui.button("Open log folder").clicked() {
        action = DockAction::OpenLogFolder;
    }
    action
}

fn help_flyout(ui: &mut egui::Ui) {
    let t = theme::chrome();
    flyout_header(ui, "Shortcuts");
    egui::ScrollArea::vertical().show(ui, |ui| {
        let rows = [
            ("Ctrl+C", "Send interrupt (or copy selection)"),
            ("Ctrl+Shift+C", "Copy selection"),
            ("Ctrl+D", "Send EOF"),
            ("Ctrl+V", "Paste (multiline opens a dialog)"),
            ("Ctrl+P", "Upload clipboard image"),
            ("Ctrl+Shift+F", "Find in scrollback"),
            ("Ctrl+Shift+P", "Search open tabs"),
            ("Ctrl+Shift+K", "Command palette"),
            ("F11", "Focus mode"),
            ("Enter", "Copy active selection"),
            ("Alt+hover", "Show when row last changed"),
        ];
        for (key, desc) in rows {
            ui.horizontal(|ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgb(0xf5, 0xf3, 0xff))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(6.0, 3.0))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(key)
                                .monospace()
                                .size(10.0)
                                .color(t.accent),
                        );
                    });
                ui.label(
                    egui::RichText::new(desc)
                        .font(theme::font(Weight::Regular, 11.5))
                        .color(Color32::from_rgb(0x47, 0x55, 0x69)),
                );
            });
            ui.add_space(4.0);
        }
    });
}
