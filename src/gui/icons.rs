//! Hand-drawn dock/flyout icons, procedurally approximating the Lucide-style
//! 24x24 stroke icons from the FlashLearn design handoff. Hand-drawn rather
//! than SVG-rendered (see redesign plan) to avoid adding `egui_extras` +
//! `resvg`/`usvg`/`tiny-skia` for seven small glyphs.
//!
//! Every icon is authored in a 24x24 viewBox and mapped into an arbitrary
//! target `rect` by [`map`], so callers just pick a size.

use egui::{Color32, Pos2, Rect, Shape, Stroke, Vec2};

/// Map a point in the 24x24 icon viewBox into `rect`, preserving aspect
/// ratio and centering the icon within it.
fn map(rect: Rect, x: f32, y: f32) -> Pos2 {
    let scale = rect.width().min(rect.height()) / 24.0;
    let offset = rect.center() - Vec2::splat(12.0 * scale);
    Pos2::new(offset.x + x * scale, offset.y + y * scale)
}

fn stroke(rect: Rect, color: Color32) -> Stroke {
    // Stroke width scales with the icon so it stays crisp at any dock size.
    Stroke::new((rect.width().min(rect.height()) / 24.0) * 2.0, color)
}

fn polyline(painter: &egui::Painter, rect: Rect, pts: &[(f32, f32)], color: Color32) {
    let s = stroke(rect, color);
    let mapped: Vec<Pos2> = pts.iter().map(|&(x, y)| map(rect, x, y)).collect();
    for w in mapped.windows(2) {
        painter.line_segment([w[0], w[1]], s);
    }
}

/// A rotated stadium/capsule outline, used to approximate the interlocking
/// hooks of the "link" icon without solving SVG arc endpoint parameters.
fn capsule(
    painter: &egui::Painter,
    rect: Rect,
    center: (f32, f32),
    len: f32,
    radius: f32,
    angle_deg: f32,
    color: Color32,
) {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    let rotate = |x: f32, y: f32| (center.0 + x * cos - y * sin, center.1 + x * sin + y * cos);
    let half = len / 2.0 - radius;
    let mut pts = Vec::with_capacity(40);
    let arc = |start_deg: f32, cx: f32, cy: f32| -> Vec<(f32, f32)> {
        (0..=16)
            .map(|i| {
                let a = (start_deg + i as f32 * (180.0 / 16.0)).to_radians();
                (cx + radius * a.cos(), cy + radius * a.sin())
            })
            .collect()
    };
    pts.extend(arc(-90.0, half, 0.0));
    pts.extend(arc(90.0, -half, 0.0));
    pts.push(pts[0]);
    let mapped: Vec<Pos2> = pts
        .iter()
        .map(|&(x, y)| {
            let (rx, ry) = rotate(x, y);
            map(rect, rx, ry)
        })
        .collect();
    painter.add(Shape::closed_line(mapped, stroke(rect, color)));
}

/// Hosts: two interlocking chain-link capsules.
pub fn link(painter: &egui::Painter, rect: Rect, color: Color32) {
    capsule(painter, rect, (9.5, 12.5), 11.0, 3.4, 45.0, color);
    capsule(painter, rect, (14.5, 11.5), 11.0, 3.4, 45.0, color);
}

/// Snippets: a terminal prompt (`>_`-style chevron + cursor bar).
pub fn terminal(painter: &egui::Painter, rect: Rect, color: Color32) {
    polyline(
        painter,
        rect,
        &[(4.0, 5.0), (10.0, 11.0), (4.0, 17.0)],
        color,
    );
    polyline(painter, rect, &[(12.0, 19.0), (20.0, 19.0)], color);
}

/// Files: a folder tab + body.
pub fn folder(painter: &egui::Painter, rect: Rect, color: Color32) {
    polyline(
        painter,
        rect,
        &[
            (2.0, 6.0),
            (2.0, 18.0),
            (20.0, 18.0),
            (20.0, 8.0),
            (12.0, 8.0),
            (10.0, 5.0),
            (2.0, 5.0),
            (2.0, 6.0),
        ],
        color,
    );
}

/// Theme: three vertical tracks with circle handles at staggered heights.
pub fn sliders(painter: &egui::Painter, rect: Rect, color: Color32) {
    let tracks = [(6.0, 15.0), (12.0, 9.0), (18.0, 17.0)];
    for &(x, handle_y) in &tracks {
        polyline(painter, rect, &[(x, 4.0), (x, 20.0)], color);
        painter.circle_filled(
            map(rect, x, handle_y),
            rect.width().min(rect.height()) / 24.0 * 2.2,
            color,
        );
    }
}

/// Session: an activity/pulse trace.
pub fn pulse(painter: &egui::Painter, rect: Rect, color: Color32) {
    polyline(
        painter,
        rect,
        &[
            (22.0, 12.0),
            (18.0, 12.0),
            (15.0, 21.0),
            (9.0, 3.0),
            (6.0, 12.0),
            (2.0, 12.0),
        ],
        color,
    );
}

/// Help: a circle with a "?" glyph (rendered as text rather than traced
/// beziers — visually equivalent, far less code).
pub fn help(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.width().min(rect.height()) / 2.0 - stroke(rect, color).width;
    painter.circle_stroke(rect.center(), r, stroke(rect, color));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "?",
        egui::FontId::proportional(rect.height() * 0.6),
        color,
    );
}

/// A magnifying-glass search icon (command palette header).
pub fn search(painter: &egui::Painter, rect: Rect, color: Color32) {
    let s = stroke(rect, color);
    let center = map(rect, 10.0, 10.0);
    let r = (map(rect, 17.0, 10.0).x - center.x).abs();
    painter.circle_stroke(center, r, s);
    painter.line_segment([map(rect, 16.0, 16.0), map(rect, 21.0, 21.0)], s);
}
