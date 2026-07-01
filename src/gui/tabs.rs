//! Tab bar: floating pill switcher, sync input, focus mode, tab search.

use crate::gui::theme::{self, Weight};
use egui::{Color32, Rounding};

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

/// One pill in the floating tab switcher: accent dot + label, active pill
/// gets a solid white background and a soft shadow.
fn tab_pill(ui: &mut egui::Ui, tab: &TabInfo, selected: bool) -> egui::Response {
    let t = theme::chrome();
    let frame = egui::Frame::none()
        .fill(if selected {
            Color32::WHITE
        } else {
            Color32::TRANSPARENT
        })
        .rounding(Rounding::same(11.0))
        .inner_margin(egui::Margin::symmetric(10.0, 6.0));
    let frame = if selected {
        frame.shadow(chrome_shadow())
    } else {
        frame
    };
    let inner = frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
            let dot_color = tab.color.unwrap_or(t.accent);
            ui.painter()
                .circle_filled(dot_rect.center(), 3.5, dot_color);
            let text_color = if selected { t.text_primary } else { t.text_dim };
            let alpha = if !selected && !tab.connected {
                160
            } else {
                255
            };
            ui.label(
                egui::RichText::new(&tab.title)
                    .font(theme::font(Weight::Medium, 12.5))
                    .color(text_color.gamma_multiply(alpha as f32 / 255.0)),
            );
        });
    });
    ui.interact(
        inner.response.rect,
        inner.response.id.with("pill"),
        egui::Sense::click(),
    )
}

fn chrome_shadow() -> egui::Shadow {
    egui::Shadow {
        offset: egui::Vec2::new(0.0, 1.0),
        blur: 4.0,
        spread: 0.0,
        color: Color32::from_black_alpha(20),
    }
}

pub fn tab_bar(
    ui: &mut egui::Ui,
    tabs: &[TabInfo],
    active: usize,
    sync_input: bool,
    focus_mode: bool,
) -> TabAction {
    let mut action = TabAction::None;

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui
            .button("🔍")
            .on_hover_text("Search tabs (Ctrl+Shift+P)")
            .clicked()
        {
            action = TabAction::OpenTabSearch;
        }
        let sync = ui
            .selectable_label(sync_input, "⇶")
            .on_hover_text("Sync input to all tabs");
        if sync.clicked() {
            action = TabAction::ToggleSyncInput;
        }
        let focus = ui
            .selectable_label(focus_mode, "⛶")
            .on_hover_text("Focus mode (F11)");
        if focus.clicked() {
            action = TabAction::ToggleFocusMode;
        }
        ui.add_space(8.0);

        // Centered floating pill switcher fills whatever space remains
        // between the logo (already placed by the caller) and these
        // trailing icons. `with_layout` alone would shrink-wrap to
        // content, so the remaining space is claimed explicitly first.
        let remaining = ui.available_size();
        ui.allocate_ui_with_layout(
            remaining,
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 128))
                    .rounding(Rounding::same(14.0))
                    .inner_margin(egui::Margin::same(4.0))
                    .show(ui, |ui| {
                        egui::ScrollArea::horizontal()
                            .id_source("tabbar")
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for (i, tab) in tabs.iter().enumerate() {
                                        let resp = tab_pill(ui, tab, i == active);
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
                                                    if ui
                                                        .button(
                                                            egui::RichText::new(name).color(color),
                                                        )
                                                        .clicked()
                                                    {
                                                        action =
                                                            TabAction::SetColor(i, Some(color));
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
                                    let (rect, resp) = ui.allocate_exact_size(
                                        egui::vec2(26.0, 26.0),
                                        egui::Sense::click(),
                                    );
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "+",
                                        theme::font(Weight::SemiBold, 15.0),
                                        theme::chrome().accent,
                                    );
                                    if resp.clicked() {
                                        action = TabAction::New;
                                    }
                                });
                            });
                    });
            },
        );
    });

    action
}
