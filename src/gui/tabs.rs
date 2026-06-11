//! Tab bar: session tabs, tab actions, sync input, focus mode, tab search.

use crate::gui::theme;
use egui::Color32;

pub enum TabAction {
    None,
    Select(usize),
    New,
    Close(usize),
    CloseOthers(usize),
    CloseRight(usize),
    SetColor(usize, Option<Color32>),
    ToggleSyncInput,
    ToggleFocusMode,
    OpenTabSearch,
}

pub struct TabInfo {
    pub title: String,
    pub color: Option<Color32>,
    pub connected: bool,
}

pub fn tab_bar(
    ui: &mut egui::Ui,
    tabs: &[TabInfo],
    active: usize,
    sync_input: bool,
    focus_mode: bool,
) -> TabAction {
    let mut action = TabAction::None;

    ui.horizontal(|ui| {
        egui::ScrollArea::horizontal()
            .id_source("tabbar")
            .max_width(ui.available_width() - 130.0)
            .show(ui, |ui| {
                for (i, tab) in tabs.iter().enumerate() {
                    let selected = i == active;
                    let mut text = egui::RichText::new(&tab.title);
                    if let Some(c) = tab.color {
                        text = text.color(c);
                    } else if !tab.connected {
                        text = text.color(theme::current().text_dim);
                    }
                    let button = egui::Button::new(text).fill(if selected {
                        theme::current().surface_raised
                    } else {
                        theme::current().surface_panel
                    });
                    let resp = ui.add(button);
                    if resp.clicked() {
                        action = TabAction::Select(i);
                    }
                    if resp.middle_clicked() {
                        action = TabAction::Close(i);
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Close").clicked() {
                            action = TabAction::Close(i);
                            ui.close_menu();
                        }
                        if ui.button("Close others").clicked() {
                            action = TabAction::CloseOthers(i);
                            ui.close_menu();
                        }
                        if ui.button("Close tabs to the right").clicked() {
                            action = TabAction::CloseRight(i);
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.menu_button("Tab color", |ui| {
                            for (name, color) in theme::tab_colors() {
                                if ui.button(egui::RichText::new(name).color(color)).clicked() {
                                    action = TabAction::SetColor(i, Some(color));
                                    ui.close_menu();
                                }
                            }
                            if ui.button("None").clicked() {
                                action = TabAction::SetColor(i, None);
                                ui.close_menu();
                            }
                        });
                    });
                }
                if ui.button("+").on_hover_text("New tab").clicked() {
                    action = TabAction::New;
                }
            });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let focus = ui
                .selectable_label(focus_mode, "⛶")
                .on_hover_text("Focus mode (F11)");
            if focus.clicked() {
                action = TabAction::ToggleFocusMode;
            }
            let sync = ui
                .selectable_label(sync_input, "⇶")
                .on_hover_text("Sync input to all tabs");
            if sync.clicked() {
                action = TabAction::ToggleSyncInput;
            }
            if ui
                .button("🔍")
                .on_hover_text("Search tabs (Ctrl+Shift+P)")
                .clicked()
            {
                action = TabAction::OpenTabSearch;
            }
        });
    });

    action
}
