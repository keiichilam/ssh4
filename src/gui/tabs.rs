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

    // Trailing controls stay in the panel, right-aligned.
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
    });

    // The floating pill switcher is a genuinely centered, shrink-wrapped
    // element — nesting it in the panel's row-layout would not center it
    // over the window (the logo/controls bias it). An `Area` anchored to
    // the window's horizontal center matches the mockup's "floating pill"
    // and centers cleanly; the 60pt top strip already reserves the
    // vertical space so nothing renders underneath it.
    egui::Area::new(egui::Id::new("tab_switcher"))
        .anchor(egui::Align2::CENTER_TOP, [0.0, 13.0])
        .order(egui::Order::Middle)
        .show(ui.ctx(), |ui| {
            egui::Frame::none()
                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 200))
                .rounding(Rounding::same(14.0))
                .inner_margin(egui::Margin::same(4.0))
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
                                            .button(egui::RichText::new(name).color(color))
                                            .clicked()
                                        {
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
                        let (rect, resp) =
                            ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
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

    action
}
