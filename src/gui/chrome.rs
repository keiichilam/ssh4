//! Shared FlashLearn "shell" primitives: the brand gradient, a themed card
//! frame, the logo mark, and the terminal card's status header. Reused by
//! `dock.rs`, `tabs.rs`, `dialogs.rs`, and `app.rs` instead of each
//! reimplementing gradient fills / rounded-card chrome independently.

use std::time::Duration;

use egui::{Color32, Rounding, Shadow, Stroke, Vec2};

use crate::gui::session::Session;
use crate::gui::theme::{self, Weight};

/// Brand gradient endpoints (135°: top-left violet -> bottom-right indigo).
const GRADIENT_START: Color32 = Color32::from_rgb(0x7c, 0x3a, 0xed);
const GRADIENT_END: Color32 = Color32::from_rgb(0x4f, 0x46, 0xe5);
const GRADIENT_TEXTURE_SIZE: usize = 32;

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        lerp_u8(a.r(), b.r(), t),
        lerp_u8(a.g(), b.g(), t),
        lerp_u8(a.b(), b.b(), t),
    )
}

/// egui has no native linear-gradient fill and no rounded-corner clipping
/// for custom meshes, so the gradient is baked into a small texture once
/// and drawn back via `egui::Image`, whose `rounding()` + `paint_at()`
/// already clip correctly to any rect/rounding and scale cleanly.
fn gradient_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("chrome-gradient-texture");
    if let Some(tex) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return tex;
    }
    let size = GRADIENT_TEXTURE_SIZE;
    let mut pixels = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let t = (x + y) as f32 / (2 * (size - 1)) as f32;
            pixels.push(lerp_color(GRADIENT_START, GRADIENT_END, t));
        }
    }
    let image = egui::ColorImage {
        size: [size, size],
        pixels,
    };
    let tex = ctx.load_texture("chrome-gradient", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

/// Paint the brand gradient into `rect`, clipped to `rounding`.
pub fn paint_gradient(ui: &egui::Ui, rect: egui::Rect, rounding: impl Into<Rounding>) {
    let tex = gradient_texture(ui.ctx());
    egui::Image::from_texture(&tex)
        .rounding(rounding)
        .paint_at(ui, rect);
}

/// A themed drop shadow (single-layer; egui has no multi-layer shadows).
pub fn shadow(offset: Vec2, blur: f32, color: Color32) -> Shadow {
    Shadow {
        offset,
        blur,
        spread: 0.0,
        color,
    }
}

/// Shared rounded/bordered/shadowed card frame — dock, flyout, terminal
/// card, command palette, and bottom sheets all start from this and
/// override `.shadow()`/`.fill()` where the mockup specifies a different
/// treatment.
pub fn card_frame(rounding: f32) -> egui::Frame {
    let t = theme::chrome();
    egui::Frame::none()
        .fill(t.surface_raised)
        .stroke(Stroke::new(1.0, t.border))
        .rounding(Rounding::same(rounding))
        .shadow(shadow(
            Vec2::new(0.0, 8.0),
            24.0,
            Color32::from_black_alpha(20),
        ))
}

/// A full-window dimming scrim, painted behind modal overlays (command
/// palette, bottom sheets). Must be painted before the overlay itself so
/// the overlay's own layer draws on top within the same `Order` tier.
pub fn paint_scrim(ctx: &egui::Context) {
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("chrome_scrim"),
    ))
    .rect_filled(
        ctx.screen_rect(),
        Rounding::ZERO,
        Color32::from_rgba_unmultiplied(0x0f, 0x17, 0x2a, 89),
    );
}

/// The small drag-handle bar at the top of a bottom sheet.
pub fn drag_handle(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(36.0, 4.0), egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            Rounding::same(2.0),
            Color32::from_rgb(0xe2, 0xe8, 0xf0),
        );
    });
}

/// The `>_` logo mark: a rounded gradient tile with a white monospace
/// glyph. `size` is the tile's edge length in points.
pub fn logo_tile(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let rounding = size * 0.28;
    paint_gradient(ui, rect, rounding);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        ">_",
        egui::FontId::monospace(size * 0.42),
        Color32::WHITE,
    );
}

/// The logo tile plus the "ssh4" wordmark, as used in the top strip.
pub fn logo_with_wordmark(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        logo_tile(ui, 26.0);
        ui.label(
            egui::RichText::new("ssh4")
                .font(theme::font(Weight::Bold, 12.5))
                .color(theme::chrome().text_primary),
        );
    });
}

/// The terminal card's internal 40px header: connection status dot,
/// identity, dimensions, byte counters, and heartbeat age.
pub fn terminal_header(ui: &mut egui::Ui, session: &Session) {
    let t = theme::chrome();
    ui.horizontal(|ui| {
        ui.set_min_height(40.0);
        ui.add_space(4.0);

        // Status dot with a soft halo when connected.
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
        let center = dot_rect.center();
        if session.connected {
            ui.painter().circle_filled(
                center,
                7.0,
                Color32::from_rgba_unmultiplied(0x10, 0xb9, 0x81, 38),
            );
            ui.painter().circle_filled(center, 3.5, t.success);
        } else {
            ui.painter().circle_filled(center, 3.5, t.text_dim);
        }

        ui.label(
            egui::RichText::new(&session.identity)
                .font(theme::font(Weight::SemiBold, 12.0))
                .color(t.text_primary),
        );
        ui.label(
            egui::RichText::new(format!("{}×{}", session.cols, session.rows))
                .font(theme::font(Weight::Regular, 11.5))
                .color(t.text_dim),
        );
        ui.label(
            egui::RichText::new(format!(
                "idle {}s",
                session.last_data_at.elapsed().as_secs()
            ))
            .font(theme::font(Weight::Regular, 11.5))
            .color(t.text_dim),
        );

        if let Some((msg, is_ok, at)) = &session.status {
            if at.elapsed() < Duration::from_secs(6) {
                let c = if *is_ok { t.success } else { t.error };
                ui.colored_label(c, msg);
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            if let Some(hb) = &session.heartbeat {
                ui.label(
                    egui::RichText::new(format!("hb {}s", hb.at.elapsed().as_secs()))
                        .font(theme::font(Weight::Medium, 11.0))
                        .color(t.success),
                )
                .on_hover_text(format!(
                    "queued {} · buffered {} B · read timeouts {}",
                    hb.pending_chunks, hb.buffered_output, hb.read_timeouts
                ));
            }
            ui.label(
                egui::RichText::new(format!(
                    "↑{} ↓{}",
                    crate::gui::dialogs::human_size(session.sent),
                    crate::gui::dialogs::human_size(session.received)
                ))
                .monospace()
                .size(11.0)
                .color(t.text_dim),
            );
        });
    });
    ui.add(egui::Separator::default().spacing(0.0));
}
