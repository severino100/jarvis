//! HUD look, same palette and type as the Electron version (renderer/styles.css).

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};
use std::{collections::BTreeMap, fs, sync::Arc};

pub const BG: Color32 = Color32::from_rgb(0x03, 0x07, 0x0e);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x05, 0x0d, 0x19);
pub const GLASS: Color32 = Color32::from_rgb(0x0a, 0x16, 0x28);
pub const TEXT: Color32 = Color32::from_rgb(0xd8, 0xec, 0xfa);
pub const BRIGHT: Color32 = Color32::from_rgb(0xea, 0xf8, 0xff);
pub const MUTED: Color32 = Color32::from_rgb(0x7f, 0x9c, 0xb8);
pub const FAINT: Color32 = Color32::from_rgb(0x56, 0x70, 0x8c);
pub const AMBER: Color32 = Color32::from_rgb(0xff, 0xb5, 0x47);
pub const RED: Color32 = Color32::from_rgb(0xff, 0x6b, 0x78);
pub const GREEN: Color32 = Color32::from_rgb(0x3d, 0xf0, 0xa6);

/// Accent presets (Definições → Cor principal); every cyan in the UI derives from these two.
#[derive(Clone, Copy)]
pub struct Accent {
    pub main: Color32,
    pub light: Color32,
}

pub fn accent(key: &str) -> Accent {
    let (main, light) = match key {
        "orange" => ((0xff, 0x8a, 0x3d), (0xff, 0xc0, 0x8f)),
        "green" => ((0x3d, 0xf0, 0xa6), (0xa4, 0xf8, 0xd4)),
        "violet" => ((0xa9, 0x8b, 0xff), (0xd6, 0xc9, 0xff)),
        "red" => ((0xff, 0x54, 0x70), (0xff, 0xa3, 0xb1)),
        "ice" => ((0xcf, 0xe3, 0xff), (0xff, 0xff, 0xff)),
        _ => ((0x45, 0xd4, 0xff), (0x8f, 0xe6, 0xff)),
    };
    Accent { main: Color32::from_rgb(main.0, main.1, main.2), light: Color32::from_rgb(light.0, light.1, light.2) }
}

/// Same colour at a given opacity (0..1), like `color-mix(in srgb, X n%, transparent)` in the CSS.
pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn display() -> FontFamily {
    FontFamily::Name("display".into())
}

/// Windows ships Bahnschrift (DIN), Segoe UI and Cascadia Mono; falls back to egui's own fonts if one is missing.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let mut load = |name: &str, file: &str| -> bool {
        match fs::read(format!("C:\\Windows\\Fonts\\{file}")) {
            Ok(bytes) => {
                defs.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
                true
            }
            Err(_) => false,
        }
    };
    let has_display = load("bahnschrift", "bahnschrift.ttf");
    let has_body = load("segoe", "segoeui.ttf");
    let has_mono = load("cascadia", "CascadiaMono.ttf");
    let mut fam = |family: FontFamily, first: Option<&str>| {
        let list = defs.families.entry(family).or_default();
        if let Some(f) = first {
            list.insert(0, f.into());
        }
    };
    fam(FontFamily::Proportional, has_body.then_some("segoe"));
    fam(FontFamily::Monospace, has_mono.then_some("cascadia"));
    let fallback: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut display_list: Vec<String> = Vec::new();
    if has_display {
        display_list.push("bahnschrift".into());
    }
    display_list.extend(fallback);
    defs.families.insert(display(), display_list);
    ctx.set_fonts(defs);
}

pub fn install_style(ctx: &egui::Context, acc: Accent) {
    let mut styles: BTreeMap<TextStyle, FontId> = BTreeMap::new();
    styles.insert(TextStyle::Heading, FontId::new(size::TITLE, display()));
    styles.insert(TextStyle::Body, FontId::new(size::BODY, FontFamily::Proportional));
    styles.insert(TextStyle::Button, FontId::new(size::BUTTON, display()));
    styles.insert(TextStyle::Small, FontId::new(12.0, FontFamily::Proportional));
    styles.insert(TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace));
    ctx.all_styles_mut(|style| {
        style.text_styles = styles.clone();
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(TEXT);
        v.panel_fill = BG;
        v.window_fill = GLASS;
        v.extreme_bg_color = Color32::from_rgb(0x02, 0x06, 0x0c);
        v.faint_bg_color = alpha(acc.main, 0.05);
        v.selection.bg_fill = alpha(acc.main, 0.3);
        v.selection.stroke = Stroke::new(1.0, acc.main);
        v.hyperlink_color = acc.main;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, alpha(acc.main, 0.14));
        v.widgets.inactive.bg_fill = alpha(acc.main, 0.03);
        v.widgets.inactive.weak_bg_fill = alpha(acc.main, 0.03);
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, alpha(acc.main, 0.3));
        v.widgets.hovered.bg_fill = alpha(acc.main, 0.10);
        v.widgets.hovered.weak_bg_fill = alpha(acc.main, 0.10);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, acc.main);
        v.widgets.active.bg_fill = alpha(acc.main, 0.18);
        v.widgets.active.weak_bg_fill = alpha(acc.main, 0.18);
        v.widgets.active.bg_stroke = Stroke::new(1.0, acc.light);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.noninteractive] {
            w.corner_radius = egui::CornerRadius::same(4);
        }
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(11.0, 4.5);
        style.spacing.interact_size.y = 26.0;
    });
}

/// Linear blend from `a` to `b` (0..1), for colours that animate between two states.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// One type scale for the whole app (points; the Windows scale is applied on top). Same role, same size everywhere.
pub mod size {
    /// The greeting on Hoje.
    pub const HERO: f32 = 30.0;
    /// Section titles (AGENDA, PRS…).
    pub const TITLE: f32 = 24.0;
    /// Conversation / dialog / week titles.
    pub const SUBTITLE: f32 = 17.0;
    /// Title of an item, PR or meeting card.
    pub const CARD: f32 = 16.0;
    /// Every highlighted number (stat tiles, day counter, hours).
    pub const STAT: f32 = 22.0;
    pub const BODY: f32 = 14.5;
    pub const BUTTON: f32 = 11.5;
}
