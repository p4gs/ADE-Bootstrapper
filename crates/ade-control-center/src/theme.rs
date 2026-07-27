//! Visual identity for the ADE Control Center — Inter typography, refined
//! dark + light palettes following the OS appearance, soft-rounded surfaces.
//! The "no stock-egui look" claim (ISC-182.1) lives here.

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Theme,
    Visuals,
};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const ACCENT: Color32 = Color32::from_rgb(76, 125, 255);
pub const OK: Color32 = Color32::from_rgb(47, 191, 118);
pub const WARN: Color32 = Color32::from_rgb(226, 156, 60);
pub const ERR: Color32 = Color32::from_rgb(224, 92, 92);
pub const INFO: Color32 = Color32::from_rgb(110, 163, 224);

pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub inset: Color32,
    pub widget: Color32,
    pub widget_hover: Color32,
    pub line: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint_text: Color32,
}

pub fn dark() -> Palette {
    Palette {
        bg: Color32::from_rgb(13, 17, 23),
        panel: Color32::from_rgb(22, 28, 36),
        inset: Color32::from_rgb(17, 22, 29),
        widget: Color32::from_rgb(31, 39, 50),
        widget_hover: Color32::from_rgb(41, 51, 64),
        line: Color32::from_rgb(38, 47, 58),
        text: Color32::from_rgb(230, 235, 241),
        muted: Color32::from_rgb(139, 152, 165),
        faint_text: Color32::from_rgb(100, 112, 125),
    }
}

pub fn light() -> Palette {
    Palette {
        bg: Color32::from_rgb(244, 246, 249),
        panel: Color32::WHITE,
        inset: Color32::from_rgb(238, 241, 245),
        widget: Color32::from_rgb(233, 237, 242),
        widget_hover: Color32::from_rgb(222, 228, 235),
        line: Color32::from_rgb(225, 230, 236),
        text: Color32::from_rgb(26, 34, 48),
        muted: Color32::from_rgb(94, 106, 120),
        faint_text: Color32::from_rgb(150, 160, 172),
    }
}

pub fn palette(dark_mode: bool) -> Palette {
    if dark_mode {
        dark()
    } else {
        light()
    }
}

fn visuals(p: &Palette, dark_mode: bool) -> Visuals {
    let mut v = if dark_mode {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let radius = CornerRadius::same(8);
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.inset;
    v.faint_bg_color = p.inset;
    v.window_corner_radius = CornerRadius::same(12);
    v.window_stroke = Stroke::new(1.0, p.line);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;

    v.widgets.noninteractive.bg_fill = p.panel;
    v.widgets.noninteractive.weak_bg_fill = p.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.noninteractive.corner_radius = radius;

    v.widgets.inactive.bg_fill = p.widget;
    v.widgets.inactive.weak_bg_fill = p.widget;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.corner_radius = radius;

    v.widgets.hovered.bg_fill = p.widget_hover;
    v.widgets.hovered.weak_bg_fill = p.widget_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    v.widgets.hovered.fg_stroke = Stroke::new(1.2, p.text);
    v.widgets.hovered.corner_radius = radius;

    v.widgets.active.bg_fill = p.widget_hover;
    v.widgets.active.weak_bg_fill = p.widget_hover;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.2, p.text);
    v.widgets.active.corner_radius = radius;

    v.widgets.open.bg_fill = p.widget;
    v.widgets.open.weak_bg_fill = p.widget;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.open.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.open.corner_radius = radius;

    v
}

pub const FAMILY_MEDIUM: &str = "inter-medium";
pub const FAMILY_SEMIBOLD: &str = "inter-semibold";
pub const FAMILY_BOLD: &str = "inter-bold";

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Regular.otf"
        ))),
    );
    fonts.font_data.insert(
        "inter-medium".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Medium.otf"
        ))),
    );
    fonts.font_data.insert(
        "inter-semibold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-SemiBold.otf"
        ))),
    );
    fonts.font_data.insert(
        "inter-bold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Bold.otf"
        ))),
    );
    // Inter leads the proportional family; egui's defaults stay as glyph fallback.
    if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
        family.insert(0, "inter".into());
    }
    fonts.families.insert(
        FontFamily::Name(FAMILY_MEDIUM.into()),
        vec!["inter-medium".into(), "inter".into()],
    );
    fonts.families.insert(
        FontFamily::Name(FAMILY_SEMIBOLD.into()),
        vec!["inter-semibold".into(), "inter".into()],
    );
    fonts.families.insert(
        FontFamily::Name(FAMILY_BOLD.into()),
        vec!["inter-bold".into(), "inter".into()],
    );
    fonts
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_SEMIBOLD.into()))
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_MEDIUM.into()))
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_visuals_of(Theme::Dark, visuals(&dark(), true));
    ctx.set_visuals_of(Theme::Light, visuals(&light(), false));
    ctx.all_styles_mut(|style| {
        let text_styles: BTreeMap<TextStyle, FontId> = [
            (TextStyle::Heading, semibold(18.0)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, medium(13.0)),
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
        style.text_styles = text_styles;
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 6.0);
        style.spacing.interact_size = egui::vec2(40.0, 26.0);
        style.spacing.window_margin = egui::Margin::same(16);
        style.spacing.menu_margin = egui::Margin::same(8);
    });
}
