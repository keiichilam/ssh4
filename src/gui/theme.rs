//! Theme system (TR-004): named color roles, selectable built-in themes.
//!
//! Every UI color is a named role on [`Theme`]; painting code reads the
//! active theme through [`current()`]. Three built-ins ship: the light
//! "Lavender" default (FlashLearn design language), the signature "Amber
//! Phosphor", and a "Green Phosphor" sibling. Chrome renders in Inter
//! (see [`install_fonts()`]); the terminal canvas keeps a monospace face
//! (platform candidates below) regardless of the active theme.

use std::sync::atomic::{AtomicUsize, Ordering};

use egui::{Color32, FontFamily, Margin, Rounding, Shadow, Stroke, Vec2};

/// Named color roles for one theme.
pub struct Theme {
    pub name: &'static str,
    pub dark: bool,

    // Surfaces (brightening with elevation)
    pub surface_bg: Color32,
    pub surface_panel: Color32,
    pub surface_raised: Color32,
    pub surface_hover: Color32,
    pub border: Color32,

    // Text
    pub text_primary: Color32,
    pub text_dim: Color32,

    // Accent / semantic
    pub accent: Color32,
    pub accent_dim: Color32,
    pub error: Color32,
    pub success: Color32,

    // Terminal
    pub term_bg: Color32,
    pub term_fg: Color32,
    pub cursor: Color32,
    pub selection_bg: Color32,
    pub search_match_bg: Color32,
    pub search_active_bg: Color32,
    pub link: Color32,

    /// ANSI 16-color palette. Each hue must stay unmistakable
    /// (ls/grep/htop must still read correctly).
    pub ansi: [Color32; 16],
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Lavender canvas with a violet accent, after the FlashLearn design
/// system. The light default theme.
pub static LAVENDER: Theme = Theme {
    name: "Lavender",
    dark: false,
    surface_bg: rgb(0xf5, 0xf4, 0xff),
    surface_panel: rgb(0xf5, 0xf4, 0xff),
    surface_raised: rgb(0xff, 0xff, 0xff),
    surface_hover: rgb(0xf5, 0xf3, 0xff),
    border: rgb(0xe9, 0xe7, 0xf5),
    text_primary: rgb(0x0f, 0x17, 0x2a),
    text_dim: rgb(0x94, 0xa3, 0xb8),
    accent: rgb(0x7c, 0x3a, 0xed),
    accent_dim: rgb(0xa7, 0x8b, 0xfa),
    error: rgb(0xef, 0x44, 0x44),
    success: rgb(0x10, 0xb9, 0x81),
    term_bg: rgb(0xfb, 0xfa, 0xff),
    term_fg: rgb(0x0f, 0x17, 0x2a),
    cursor: rgb(0x7c, 0x3a, 0xed),
    selection_bg: rgb(0xed, 0xe9, 0xfe),
    search_match_bg: rgb(0xfe, 0xf3, 0xc7),
    search_active_bg: rgb(0xfb, 0xbf, 0x24),
    link: rgb(0x4f, 0x46, 0xe5),
    ansi: [
        rgb(0x1e, 0x29, 0x3b), // black
        rgb(0xdc, 0x26, 0x26), // red
        rgb(0x15, 0x80, 0x3d), // green
        rgb(0xb4, 0x53, 0x09), // yellow
        rgb(0x25, 0x63, 0xeb), // blue
        rgb(0x7c, 0x3a, 0xed), // magenta
        rgb(0x08, 0x91, 0xb2), // cyan
        rgb(0x64, 0x74, 0x8b), // white
        rgb(0x47, 0x55, 0x69), // bright black
        rgb(0xef, 0x44, 0x44), // bright red
        rgb(0x22, 0xc5, 0x5e), // bright green
        rgb(0xf5, 0x9e, 0x0b), // bright yellow
        rgb(0x3b, 0x82, 0xf6), // bright blue
        rgb(0xa7, 0x8b, 0xfa), // bright magenta
        rgb(0x06, 0xb6, 0xd4), // bright cyan
        rgb(0x0f, 0x17, 0x2a), // bright white
    ],
};

/// Deep warm-charcoal surfaces with a phosphor-amber accent, after the DEC
/// VT terminals this app descends from.
pub static AMBER_PHOSPHOR: Theme = Theme {
    name: "Amber Phosphor",
    dark: true,
    surface_bg: rgb(0x14, 0x11, 0x0c),
    surface_panel: rgb(0x1a, 0x16, 0x10),
    surface_raised: rgb(0x26, 0x20, 0x16),
    surface_hover: rgb(0x31, 0x29, 0x1b),
    border: rgb(0x3a, 0x31, 0x21),
    text_primary: rgb(0xe2, 0xd9, 0xc5),
    text_dim: rgb(0x8f, 0x85, 0x70),
    accent: rgb(0xff, 0xb3, 0x40),
    accent_dim: rgb(0xa6, 0x74, 0x28),
    error: rgb(0xe3, 0x59, 0x4f),
    success: rgb(0x9b, 0xb9, 0x53),
    term_bg: rgb(0x0f, 0x0d, 0x09),
    term_fg: rgb(0xdd, 0xd4, 0xc0),
    cursor: rgb(0xff, 0xb0, 0x00),
    selection_bg: rgb(0x52, 0x3f, 0x1c),
    search_match_bg: rgb(0x5a, 0x46, 0x12),
    search_active_bg: rgb(0xc2, 0x85, 0x10),
    link: rgb(0xf5, 0xc1, 0x6a),
    ansi: [
        rgb(0x21, 0x1c, 0x16), // black
        rgb(0xe3, 0x59, 0x4f), // red
        rgb(0x9b, 0xb9, 0x53), // green
        rgb(0xe5, 0xb5, 0x66), // yellow
        rgb(0x6e, 0x9c, 0xc6), // blue
        rgb(0xc5, 0x85, 0xc2), // magenta
        rgb(0x6c, 0xb5, 0xa8), // cyan
        rgb(0xd8, 0xcf, 0xc0), // white
        rgb(0x6e, 0x65, 0x57), // bright black
        rgb(0xf2, 0x77, 0x6d), // bright red
        rgb(0xb2, 0xcc, 0x6e), // bright green
        rgb(0xf5, 0xcc, 0x80), // bright yellow
        rgb(0x8c, 0xb5, 0xd9), // bright blue
        rgb(0xd9, 0xa0, 0xd4), // bright magenta
        rgb(0x87, 0xcc, 0xbf), // bright cyan
        rgb(0xf2, 0xea, 0xd9), // bright white
    ],
};

/// Cool near-black greens, after monochrome P1-phosphor displays.
pub static GREEN_PHOSPHOR: Theme = Theme {
    name: "Green Phosphor",
    dark: true,
    surface_bg: rgb(0x0c, 0x12, 0x0d),
    surface_panel: rgb(0x10, 0x18, 0x11),
    surface_raised: rgb(0x18, 0x24, 0x19),
    surface_hover: rgb(0x20, 0x30, 0x21),
    border: rgb(0x28, 0x3a, 0x2a),
    text_primary: rgb(0xc8, 0xe2, 0xc8),
    text_dim: rgb(0x74, 0x8f, 0x75),
    accent: rgb(0x4f, 0xe8, 0x7a),
    accent_dim: rgb(0x2e, 0x96, 0x4e),
    error: rgb(0xe3, 0x59, 0x4f),
    success: rgb(0x6f, 0xd8, 0x6f),
    term_bg: rgb(0x09, 0x0e, 0x0a),
    term_fg: rgb(0xc4, 0xdc, 0xc4),
    cursor: rgb(0x3f, 0xff, 0x6e),
    selection_bg: rgb(0x1d, 0x47, 0x26),
    search_match_bg: rgb(0x1c, 0x52, 0x28),
    search_active_bg: rgb(0x2f, 0xa8, 0x4d),
    link: rgb(0x8d, 0xeb, 0xa8),
    ansi: [
        rgb(0x16, 0x1f, 0x17), // black
        rgb(0xe3, 0x59, 0x4f), // red
        rgb(0x5f, 0xc7, 0x5f), // green
        rgb(0xd6, 0xc9, 0x5e), // yellow
        rgb(0x66, 0x9c, 0xce), // blue
        rgb(0xc0, 0x84, 0xc4), // magenta
        rgb(0x5e, 0xc1, 0xae), // cyan
        rgb(0xc8, 0xd6, 0xc8), // white
        rgb(0x5c, 0x6e, 0x5d), // bright black
        rgb(0xf2, 0x77, 0x6d), // bright red
        rgb(0x86, 0xe8, 0x86), // bright green
        rgb(0xe8, 0xdd, 0x82), // bright yellow
        rgb(0x8a, 0xb8, 0xe0), // bright blue
        rgb(0xd8, 0xa4, 0xda), // bright magenta
        rgb(0x84, 0xd8, 0xc8), // bright cyan
        rgb(0xe4, 0xf0, 0xe4), // bright white
    ],
};

/// Built-in themes, in picker order.
pub static THEMES: [&Theme; 3] = [&LAVENDER, &AMBER_PHOSPHOR, &GREEN_PHOSPHOR];

static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// The active theme. Only the terminal canvas (grid cells, cursor,
/// selection, search highlight, ANSI palette) reads colors through this —
/// see [`chrome()`] for everything else.
pub fn current() -> &'static Theme {
    THEMES[CURRENT.load(Ordering::Relaxed).min(THEMES.len() - 1)]
}

/// The fixed FlashLearn chrome palette (dock, flyout, tab switcher,
/// terminal-card frame/header, dialogs, file tools). Per the redesign
/// brief, chrome never changes with the terminal theme picker — only the
/// terminal canvas itself does — so this always resolves to [`LAVENDER`]
/// regardless of [`current()`].
pub fn chrome() -> &'static Theme {
    &LAVENDER
}

/// Index of the active theme in [`THEMES`].
pub fn current_index() -> usize {
    CURRENT.load(Ordering::Relaxed).min(THEMES.len() - 1)
}

/// Switch the active theme and restyle the context.
pub fn set_current(index: usize, ctx: &egui::Context) {
    CURRENT.store(index.min(THEMES.len() - 1), Ordering::Relaxed);
    apply_style(ctx);
}

/// Resolve a persisted theme name (case-insensitive) to an index.
pub fn index_by_name(name: &str) -> Option<usize> {
    THEMES
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(name))
}

/// Map a vt100 color to an egui color using the active palette.
pub fn vt_color(c: vt100::Color, default: Color32) -> Color32 {
    match c {
        vt100::Color::Default => default,
        vt100::Color::Idx(i) if (i as usize) < 16 => current().ansi[i as usize],
        vt100::Color::Idx(i) => idx256(i),
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    }
}

/// xterm 256-color cube and grayscale ramp.
fn idx256(i: u8) -> Color32 {
    if i < 16 {
        return current().ansi[i as usize];
    }
    if i >= 232 {
        let v = 8 + 10 * (i - 232);
        return Color32::from_rgb(v, v, v);
    }
    let i = i - 16;
    let step = |n: u8| if n == 0 { 0 } else { 55 + 40 * n };
    Color32::from_rgb(step(i / 36), step((i / 6) % 6), step(i % 6))
}

/// Platform monospace candidates, best first. The UI proportional family is
/// pointed at the same face so chrome and terminal share one voice.
#[cfg(windows)]
const FONT_CANDIDATES: &[&str] = &[
    "C:\\Windows\\Fonts\\CascadiaMono.ttf",
    "C:\\Windows\\Fonts\\CascadiaCode.ttf",
    "C:\\Windows\\Fonts\\consola.ttf",
];
#[cfg(target_os = "macos")]
const FONT_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/Menlo.ttc",
    "/System/Library/Fonts/Monaco.ttf",
];
#[cfg(all(unix, not(target_os = "macos")))]
const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
];

/// CJK fallback faces, appended after the monospace face so Han / kana /
/// hangul glyphs render instead of tofu. Every candidate that exists is
/// loaded: one face per script family keeps Japanese and Korean forms correct.
#[cfg(windows)]
const CJK_FONT_CANDIDATES: &[&str] = &[
    "C:\\Windows\\Fonts\\msyh.ttc", // Microsoft YaHei (Simplified Chinese)
    "C:\\Windows\\Fonts\\msjh.ttc", // Microsoft JhengHei (Traditional Chinese)
    "C:\\Windows\\Fonts\\YuGothM.ttc", // Yu Gothic (Japanese)
    "C:\\Windows\\Fonts\\msgothic.ttc", // MS Gothic (Japanese, older systems)
    "C:\\Windows\\Fonts\\malgun.ttf", // Malgun Gothic (Korean)
];
#[cfg(target_os = "macos")]
const CJK_FONT_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/PingFang.ttc",         // Chinese
    "/System/Library/Fonts/Hiragino Sans GB.ttc", // Chinese fallback
    "/System/Library/Fonts/AppleSDGothicNeo.ttc", // Korean
];
#[cfg(all(unix, not(target_os = "macos")))]
const CJK_FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/wenquanyi/wqy-zenhei/wqy-zenhei.ttc",
];

/// Bundled Inter weights (OFL-1.1, `assets/fonts/LICENSE.txt`) used for all
/// chrome text. egui has no font-weight axis, so each weight gets its own
/// named [`FontFamily::Name`]; pick one explicitly via [`font()`].
/// `Proportional` itself defaults to regular (400).
const INTER_WEIGHTS: &[(&str, &[u8])] = &[
    (
        "inter-400",
        include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
    ),
    (
        "inter-500",
        include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
    ),
    (
        "inter-600",
        include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"),
    ),
    (
        "inter-700",
        include_bytes!("../../assets/fonts/Inter-Bold.ttf"),
    ),
];

/// A chrome font weight, mapped to one of the bundled Inter faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
    Bold,
}

impl Weight {
    fn family_name(self) -> &'static str {
        match self {
            Weight::Regular => "inter-400",
            Weight::Medium => "inter-500",
            Weight::SemiBold => "inter-600",
            Weight::Bold => "inter-700",
        }
    }
}

/// A chrome [`egui::FontId`] at the given weight and size. Only the
/// terminal canvas should use `FontFamily::Monospace`/`TextStyle::Monospace`
/// directly; everything else should go through this (or `Weight::Regular`
/// via the default `Proportional` family) to render in Inter.
pub fn font(weight: Weight, size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name(weight.family_name().into()))
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Monospace: platform candidate, used only by the terminal canvas.
    let found = FONT_CANDIDATES.iter().find_map(|p| std::fs::read(p).ok());
    if let Some(data) = found {
        fonts
            .font_data
            .insert("ui-mono".to_owned(), egui::FontData::from_owned(data));
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "ui-mono".to_owned());
    }
    // No candidate found: egui's embedded Hack remains the monospace face.

    // Proportional/chrome: bundled Inter, one named family per weight;
    // `Proportional` itself is pointed at regular (400).
    for &(name, data) in INTER_WEIGHTS {
        fonts
            .font_data
            .insert(name.to_owned(), egui::FontData::from_owned(data.to_vec()));
        fonts
            .families
            .entry(FontFamily::Name(name.into()))
            .or_default()
            .insert(0, name.to_owned());
    }
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter-400".to_owned());

    // CJK fallback faces, appended after the primary face in every family
    // (monospace, proportional, and each Inter weight) so Han / kana /
    // hangul glyphs render instead of tofu.
    for (i, path) in CJK_FONT_CANDIDATES.iter().enumerate() {
        let Ok(data) = std::fs::read(path) else {
            continue;
        };
        let name = format!("cjk-{i}");
        fonts
            .font_data
            .insert(name.clone(), egui::FontData::from_owned(data));
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .push(name.clone());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push(name.clone());
        for &(family_name, _) in INTER_WEIGHTS {
            fonts
                .families
                .entry(FontFamily::Name(family_name.into()))
                .or_default()
                .push(name.clone());
        }
    }
    ctx.set_fonts(fonts);
}

/// Small uppercase label used for dock/dialog section headers (chrome).
pub fn section_header(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(chrome().text_dim)
            .strong(),
    );
}

/// Apply the active theme plus fonts to the egui context (startup).
pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    apply_style(ctx);
}

/// Apply the chrome style (no font reload). Chrome is always the fixed
/// Lavender palette — see [`chrome()`] — regardless of the active
/// terminal theme, so egui's default widget visuals (buttons, checkboxes,
/// scrollbars, text-edit carets) stay consistent across theme switches.
fn apply_style(ctx: &egui::Context) {
    let t = chrome();

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 4.0);
    style.spacing.window_margin = Margin::same(12.0);
    style.spacing.menu_margin = Margin::same(8.0);

    let mut v = if t.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = t.surface_panel;
    v.window_fill = t.surface_panel;
    v.window_stroke = Stroke::new(1.0, t.border);
    v.window_rounding = Rounding::same(4.0);
    v.window_shadow = Shadow {
        offset: Vec2::new(0.0, 6.0),
        blur: 20.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if t.dark { 160 } else { 60 }),
    };
    v.popup_shadow = Shadow {
        offset: Vec2::new(0.0, 3.0),
        blur: 10.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if t.dark { 120 } else { 45 }),
    };
    v.extreme_bg_color = t.surface_bg;
    v.faint_bg_color = t.surface_raised;
    v.code_bg_color = t.surface_bg;
    v.selection.bg_fill = t.selection_bg;
    v.selection.stroke = Stroke::new(1.0, t.accent);
    v.hyperlink_color = t.link;
    v.error_fg_color = t.error;
    v.warn_fg_color = t.accent;
    v.text_cursor.stroke = Stroke::new(2.0, t.cursor);

    let rounding = Rounding::same(2.0);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text_primary);
    v.widgets.noninteractive.rounding = rounding;
    v.widgets.inactive.bg_fill = t.surface_raised;
    v.widgets.inactive.weak_bg_fill = t.surface_raised;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text_primary);
    v.widgets.inactive.rounding = rounding;
    v.widgets.hovered.bg_fill = t.surface_hover;
    v.widgets.hovered.weak_bg_fill = t.surface_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.accent_dim);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, t.text_primary);
    v.widgets.hovered.rounding = rounding;
    v.widgets.active.bg_fill = t.surface_hover;
    v.widgets.active.weak_bg_fill = t.surface_hover;
    v.widgets.active.bg_stroke = Stroke::new(1.0, t.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.5, t.accent);
    v.widgets.active.rounding = rounding;
    v.widgets.open.bg_fill = t.surface_raised;
    v.widgets.open.weak_bg_fill = t.surface_raised;
    v.widgets.open.bg_stroke = Stroke::new(1.0, t.accent_dim);
    v.widgets.open.fg_stroke = Stroke::new(1.0, t.text_primary);
    v.widgets.open.rounding = rounding;

    style.visuals = v;
    ctx.set_style(style);
}

/// Tab accent color choices (chrome — fixed regardless of terminal theme).
pub fn tab_colors() -> [(&'static str, Color32); 6] {
    let t = chrome();
    [
        ("Accent", t.accent),
        ("Green", t.ansi[2]),
        ("Red", t.ansi[1]),
        ("Blue", t.ansi[4]),
        ("Magenta", t.ansi[5]),
        ("Cyan", t.ansi[6]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_names_resolve() {
        assert_eq!(index_by_name("lavender"), Some(0));
        assert_eq!(index_by_name("Amber Phosphor"), Some(1));
        assert_eq!(index_by_name("nope"), None);
    }

    #[test]
    fn ansi_palettes_distinct_hues() {
        // red/green/blue must stay distinguishable in every theme
        for t in THEMES {
            assert_ne!(t.ansi[1], t.ansi[2], "{}", t.name);
            assert_ne!(t.ansi[2], t.ansi[4], "{}", t.name);
        }
    }
}
