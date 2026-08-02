//! Visual identity for the ADE Control Center.
//!
//! The system is two layers, borrowed from how Zed's `crates/ui` is built:
//! a perceptual 12-step neutral ramp (Radix "sand", light + dark + alpha
//! variants) underneath, and named semantic roles on top. View code speaks
//! roles ("surface", "text_muted") and scales (`SIZE_*`, `Space`), never raw
//! hexes or invented sizes — the absence of intermediate values is what makes
//! the result read as designed rather than assembled.
//!
//! Interaction states are *alpha washes* from the ramp's alpha variant, so one
//! token composites correctly over any surface. Selection is a background;
//! only keyboard focus is a border, and the border box is reserved while
//! unfocused so nothing shifts. The "no stock-egui look" claim (ISC-182.1)
//! lives here.

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Shadow, Stroke,
    TextStyle, Theme, Visuals,
};
use std::collections::BTreeMap;
use std::sync::Arc;

// ───────────────────────── type + spacing scales ─────────────────────────
// macOS metrics (body is 13pt on the Mac, not iOS's 17), Zed's discipline
// (four sizes, no intermediates).

/// Captions, timestamps, secondary metadata.
pub const SIZE_CAPTION: f32 = 11.0;
/// Body text and row primary labels — the macOS default.
pub const SIZE_BODY: f32 = 13.0;
/// Section titles.
pub const SIZE_SECTION: f32 = 15.0;
/// Page titles (the verdict headline, capability-page titles).
pub const SIZE_TITLE: f32 = 20.0;

/// The spacing scale. Fine-grained below 8 (where almost all UI rhythm
/// lives), coarse above. Nothing off-scale: if a gap is not one of these, the
/// layout is wrong, not the scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // the full scale ships now; views adopt steps as they are extracted
pub enum Space {
    S0,
    S1,
    S2,
    S3,
    S4,
    S6,
    S8,
    S12,
    S16,
    S20,
    S24,
    S32,
}

impl Space {
    pub const fn px(self) -> f32 {
        match self {
            Space::S0 => 0.0,
            Space::S1 => 1.0,
            Space::S2 => 2.0,
            Space::S3 => 3.0,
            Space::S4 => 4.0,
            Space::S6 => 6.0,
            Space::S8 => 8.0,
            Space::S12 => 12.0,
            Space::S16 => 16.0,
            Space::S20 => 20.0,
            Space::S24 => 24.0,
            Space::S32 => 32.0,
        }
    }
}

/// A symmetric margin from the spacing scale — view code never writes a
/// literal margin value.
pub fn margin(x: Space, y: Space) -> egui::Margin {
    egui::Margin::symmetric(x.px() as i8, y.px() as i8)
}

/// Corner radii: one for controls, one for grouped containers, one for
/// popovers. Small radii are also where egui's tessellated corners are
/// indistinguishable from analytically-rendered ones.
pub const RADIUS_CONTROL: u8 = 5;
pub const RADIUS_CONTAINER: u8 = 10;
pub const RADIUS_POPOVER: u8 = 12;

/// Shared animation durations. Every `animate_bool_with_time` /
/// `animate_value_with_time` call site names one of these instead of
/// re-hardcoding its own number — that drift is exactly how row washes
/// (0.1) and the overflow-menu reveal (0.12) ended up disagreeing.
pub const DURATION_HOVER: f32 = 0.1;
pub const DURATION_VALUE: f32 = 0.26;
pub const DURATION_ENTRANCE: f32 = 0.15;
pub const DURATION_PAYOFF: f32 = 0.2;

/// Content rows: title + subtitle at a comfortable macOS height.
pub const ROW_HEIGHT: f32 = 44.0;

/// Compact single-line rows (the Overview coverage list): one fact per row,
/// tighter than a title+subtitle content row.
pub const COVERAGE_ROW_HEIGHT: f32 = 30.0;

/// The sidebar: HIG metrics — 220pt wide, 28pt nav rows.
pub const SIDEBAR_WIDTH: f32 = 220.0;
pub const NAV_ROW_HEIGHT: f32 = 28.0;
/// The zone the floating traffic lights occupy once the titlebar melts into
/// the window (fullsize content view) — the nav list starts below it.
pub const TRAFFIC_LIGHT_INSET: f32 = 34.0;

/// Modal widths: one for a confirm dialog (a sentence and two buttons), one
/// for a list modal (checkbox + name + command + group need the room).
pub const MODAL_CONFIRM_WIDTH: f32 = 380.0;
pub const MODAL_LIST_WIDTH: f32 = 460.0;

// ───────────────────────── the neutral ramp ─────────────────────────
// Radix "sand" (MIT, radix-ui/colors), fetched verbatim from src/{light,dark}.ts.
// Step semantics (Radix's own): 1-2 app backgrounds · 3-5 element rest/hover/
// active · 6-8 borders · 9-10 solids · 11-12 text. The alpha variant is the
// same ramp as translucent overlays — what makes interaction washes composite
// correctly over any surface beneath them.

const fn c(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

const fn ca(rgba: u32) -> Color32 {
    Color32::from_rgba_unmultiplied_const(
        (rgba >> 24) as u8,
        (rgba >> 16) as u8,
        (rgba >> 8) as u8,
        rgba as u8,
    )
}

pub struct Ramp {
    pub steps: [Color32; 12],
    pub alpha: [Color32; 12],
}

impl Ramp {
    /// Radix steps are 1-based in every doc that describes them; keep that
    /// numbering at the call site instead of scattering `- 1`.
    pub const fn step(&self, n: usize) -> Color32 {
        self.steps[n - 1]
    }

    pub const fn alpha_step(&self, n: usize) -> Color32 {
        self.alpha[n - 1]
    }
}

/// Radix `sand` (light).
pub const SAND_LIGHT: Ramp = Ramp {
    steps: [
        c(0xfdfdfc),
        c(0xf9f9f8),
        c(0xf1f0ef),
        c(0xe9e8e6),
        c(0xe2e1de),
        c(0xdad9d6),
        c(0xcfceca),
        c(0xbcbbb5),
        c(0x8d8d86),
        c(0x82827c),
        c(0x63635e),
        c(0x21201c),
    ],
    alpha: [
        ca(0x55550003),
        ca(0x25250007),
        ca(0x20100010),
        ca(0x1f150019),
        ca(0x1f180021),
        ca(0x19130029),
        ca(0x19140035),
        ca(0x1915014a),
        ca(0x0f0f0079),
        ca(0x0c0c0083),
        ca(0x080800a1),
        ca(0x060500e3),
    ],
};

/// Radix `sandDark`.
pub const SAND_DARK: Ramp = Ramp {
    steps: [
        c(0x111110),
        c(0x191918),
        c(0x222221),
        c(0x2a2a28),
        c(0x31312e),
        c(0x3b3a37),
        c(0x494844),
        c(0x62605b),
        c(0x6f6d66),
        c(0x7c7b74),
        c(0xb5b3ad),
        c(0xeeeeec),
    ],
    alpha: [
        ca(0x00000000),
        ca(0xf4f4f309),
        ca(0xf6f6f513),
        ca(0xfefef31b),
        ca(0xfbfbeb23),
        ca(0xfffaed2d),
        ca(0xfffbed3c),
        ca(0xfff9eb57),
        ca(0xfffae965),
        ca(0xfffdee73),
        ca(0xfffcf4b0),
        ca(0xfffffded),
    ],
};

pub const fn neutral(dark_mode: bool) -> &'static Ramp {
    if dark_mode {
        &SAND_DARK
    } else {
        &SAND_LIGHT
    }
}

// ───────────────────────── shadows ─────────────────────────
// Zed's elevation recipe: pure black, alpha 0.03–0.12, *stronger in dark
// mode* (dark surfaces are separated by so little luminance the shadow has to
// carry it). The final zero-blur 1px layer of the modal stack is a hairline
// edge drawn as a shadow.

fn black(alpha_255: u8) -> Color32 {
    Color32::from_black_alpha(alpha_255)
}

/// Two-layer shadow for elevated surfaces (cards, hover popouts).
/// `egui::Frame` takes one shadow — paint layers manually where it matters.
pub fn elevated_shadows(dark_mode: bool) -> [Shadow; 2] {
    [
        Shadow {
            offset: [0, 2],
            blur: 3,
            spread: 0,
            color: black(31), // 0.12
        },
        Shadow {
            offset: [0, 1],
            blur: 0,
            spread: 0,
            color: black(if dark_mode { 15 } else { 8 }), // 0.06 / 0.03
        },
    ]
}

/// Four-layer shadow for modal surfaces: contact, mid, wide ambient, and a
/// zero-blur 1px edge line.
pub fn modal_shadows(dark_mode: bool) -> [Shadow; 4] {
    let (top, edge) = if dark_mode { (31, 31) } else { (15, 10) };
    [
        Shadow {
            offset: [0, 2],
            blur: 3,
            spread: 0,
            color: black(top),
        },
        Shadow {
            offset: [0, 3],
            blur: 6,
            spread: 0,
            color: black(if dark_mode { 20 } else { 15 }),
        },
        Shadow {
            offset: [0, 6],
            blur: 12,
            spread: 0,
            color: black(10),
        },
        Shadow {
            offset: [0, 1],
            blur: 0,
            spread: 0,
            color: black(edge),
        },
    ]
}

// ───────────────────────── pixel-grid snapping ─────────────────────────

/// Snap a y-coordinate to the physical pixel grid. A 1px hairline drawn at a
/// fractional device coordinate blurs across two pixels at fractional DPI —
/// this is the single biggest "muddy vs surgical" lever available in egui.
pub fn snap_y(y: f32, pixels_per_point: f32) -> f32 {
    (y * pixels_per_point).round() / pixels_per_point
}

/// The stroke a focusable element carries while *unfocused*: transparent but
/// present, so the border box is reserved and nothing shifts when focus
/// arrives.
pub fn focus_stroke(focused: bool) -> Stroke {
    if focused {
        Stroke::new(1.0, ACCENT)
    } else {
        Stroke::new(1.0, Color32::TRANSPARENT)
    }
}

pub const ACCENT: Color32 = Color32::from_rgb(76, 125, 255);
pub const OK: Color32 = Color32::from_rgb(47, 191, 118);
pub const WARN: Color32 = Color32::from_rgb(226, 156, 60);
pub const ERR: Color32 = Color32::from_rgb(224, 92, 92);
pub const INFO: Color32 = Color32::from_rgb(110, 163, 224);

/// Light-appearance role solids, one hue-appropriate AA-safe step per role —
/// fetched the same way `SAND_LIGHT` was (radix-ui/colors `src/*.ts`, light
/// variant). `indigo-9` is the "solid button" step — the one meant for a
/// filled control with a light label on top — chosen for `accent` over a
/// `-11` text step so `accent` and `info` stay two different, recognizable
/// blues in light mode exactly as they are in dark (`ACCENT` reads more
/// indigo, `INFO` reads more sky).
///
/// `ok`/`warn`/`err`/`info` are each hue's OWN midpoint between Radix's
/// `-11` (text-safe, Radix's stated target 4.5:1 against the app
/// background) and `-12` (max-contrast text) steps, not `-11` alone:
/// `-11` measured 4.47:1 against this ramp's `panel` step (`SAND_LIGHT`
/// step 2, slightly warmer/lower than a neutral white) — Radix's own 4.5:1
/// claim is against ITS "app background," not this app's actual `panel`
/// surface, and the two are close enough to matter. The midpoint clears
/// `panel` with real margin (~6-7:1) while staying visibly the same hue as
/// `-11`, not sliding all the way to `-12`'s much darker, less
/// recognizably "success/warning/error" character.
const ACCENT_LIGHT: Color32 = c(0x3e63dd);
const OK_LIGHT: Color32 = c(0x1d5f42);
const WARN_LIGHT: Color32 = c(0x7d4c11);
const ERR_LIGHT: Color32 = c(0x99212a);
const INFO_LIGHT: Color32 = c(0x0f5399);

/// Semantic roles over the ramp. View code holds a `Palette`, never a step
/// number and never a hex — the role names ARE the design decisions.
///
/// Legacy field names (`bg`, `panel`, `inset`, …) are kept through the
/// migration; each is documented with the role it now means. Contrast facts
/// (verified in `tests::text_roles_hold_aa_contrast`): `muted` (step 11) holds
/// ≥4.5:1 on steps 1-3 in both appearances; step 10 does NOT, which is why
/// `faint_text` no longer maps to it.
pub struct Palette {
    /// Content plane — ramp step 1. The extreme end; everything else recedes.
    pub bg: Color32,
    /// Chrome/surface plane — step 2, one notch back. Sections, headers.
    pub panel: Color32,
    /// Sunken content (log panels) — step 1 again: content, not chrome.
    pub inset: Color32,
    /// Element at rest (buttons, inputs) — solid step 3.
    pub widget: Color32,
    /// Element hovered — solid step 4.
    pub widget_hover: Color32,
    /// Element pressed — solid step 5.
    pub widget_active: Color32,
    /// Border on non-interactive containers — step 6.
    pub line: Color32,
    /// Hairline between rows — *alpha* step 4, so it reads as a suggestion on
    /// any surface rather than a drawn grid line.
    pub hairline: Color32,
    /// Ghost-element hover wash — alpha step 3. Transparent at rest is the
    /// defining property of ghost chrome.
    pub row_hover: Color32,
    /// Ghost-element active wash — alpha step 4.
    pub ghost_active: Color32,
    /// Selected wash (nav rows, list selection) — alpha step 5.
    pub selected: Color32,
    /// High-contrast text — step 12.
    pub text: Color32,
    /// Secondary text — step 11 (the lowest step that holds AA at 11px).
    pub muted: Color32,
    /// Tertiary/metadata text. Also step 11: below it AA fails, so the third
    /// level of hierarchy comes from size and case, not a third grey.
    pub faint_text: Color32,
    /// Disabled text — step 9. Exempt from AA by role; never used for
    /// information.
    pub disabled_text: Color32,
    /// Primary-action fill. `ACCENT` in dark; a distinct AA-safe indigo in
    /// light (see `ACCENT_LIGHT`) — used as a button fill under white text,
    /// not as a text-on-background color, so it is not held to the same
    /// 4.5:1-against-background bar as `ok`/`warn`/`err`/`info` below
    /// (`accent_fill_holds_aa_contrast_for_white_text` covers its real bar).
    pub accent: Color32,
    /// Healthy/success role. `OK` in dark; AA-safe green in light.
    pub ok: Color32,
    /// Attention role. `WARN` in dark; AA-safe amber in light.
    pub warn: Color32,
    /// Error role. `ERR` in dark; AA-safe red in light.
    pub err: Color32,
    /// Informational / "this is clickable" role. `INFO` in dark; AA-safe
    /// blue in light.
    pub info: Color32,
}

pub fn palette(dark_mode: bool) -> Palette {
    let ramp = neutral(dark_mode);
    Palette {
        bg: ramp.step(1),
        panel: ramp.step(2),
        inset: ramp.step(1),
        widget: ramp.step(3),
        widget_hover: ramp.step(4),
        widget_active: ramp.step(5),
        line: ramp.step(6),
        hairline: ramp.alpha_step(4),
        row_hover: ramp.alpha_step(3),
        ghost_active: ramp.alpha_step(4),
        selected: ramp.alpha_step(5),
        text: ramp.step(12),
        muted: ramp.step(11),
        faint_text: ramp.step(11),
        disabled_text: ramp.step(9),
        accent: if dark_mode { ACCENT } else { ACCENT_LIGHT },
        ok: if dark_mode { OK } else { OK_LIGHT },
        warn: if dark_mode { WARN } else { WARN_LIGHT },
        err: if dark_mode { ERR } else { ERR_LIGHT },
        info: if dark_mode { INFO } else { INFO_LIGHT },
    }
}

pub fn dark() -> Palette {
    palette(true)
}

pub fn light() -> Palette {
    palette(false)
}

fn visuals(p: &Palette, dark_mode: bool) -> Visuals {
    let mut v = if dark_mode {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let radius = CornerRadius::same(RADIUS_CONTROL);
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.inset;
    v.faint_bg_color = p.inset;
    v.window_corner_radius = CornerRadius::same(RADIUS_POPOVER);
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_shadow = modal_shadows(dark_mode)[1];
    v.popup_shadow = elevated_shadows(dark_mode)[0];
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;

    v.widgets.noninteractive.bg_fill = p.panel;
    v.widgets.noninteractive.weak_bg_fill = p.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.noninteractive.corner_radius = radius;

    // Elements: solid ramp steps 3/4/5 for rest/hover/press, NO borders —
    // state lives in the fill, borders are reserved for focus. The hovered
    // accent outline the old theme drew made every button shout on approach.
    v.widgets.inactive.bg_fill = p.widget;
    v.widgets.inactive.weak_bg_fill = p.widget;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.corner_radius = radius;

    v.widgets.hovered.bg_fill = p.widget_hover;
    v.widgets.hovered.weak_bg_fill = p.widget_hover;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.corner_radius = radius;

    v.widgets.active.bg_fill = p.widget_active;
    v.widgets.active.weak_bg_fill = p.widget_active;
    v.widgets.active.bg_stroke = Stroke::NONE;
    v.widgets.active.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.active.corner_radius = radius;

    v.widgets.open.bg_fill = p.widget;
    v.widgets.open.weak_bg_fill = p.widget;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.open.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.open.corner_radius = radius;

    v
}

pub const FAMILY_SEMIBOLD: &str = "ui-semibold";

/// SF Pro's weight axis value for a named instance (from the font's own fvar
/// table: Regular 400, Semibold 590). `opsz` is pinned to the Text optical
/// size — SF's small-text cut — since almost everything this app sets is
/// under 20pt.
fn sf_variable(bytes: Vec<u8>, wght: f32) -> FontData {
    let mut data = FontData::from_owned(bytes);
    data.tweak.coords =
        egui::epaint::text::VariationCoords::new([(b"wght", wght), (b"opsz", 17.0)]);
    data
}

/// Two weights, exactly: regular and semibold. Hierarchy beyond that comes
/// from color and size, not from more weights.
///
/// The UI face is SF Pro, read at runtime from the system path — Apple's
/// license permits use on macOS but not redistribution, so the file is never
/// embedded. Inter (embedded) is the fallback if the file is ever absent.
/// SF Mono leads the monospace family (its fvar DEFAULT is Light, so Regular
/// must be pinned explicitly).
fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    match std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        Ok(bytes) => {
            fonts
                .font_data
                .insert("ui".into(), Arc::new(sf_variable(bytes.clone(), 400.0)));
            fonts
                .font_data
                .insert("ui-semibold".into(), Arc::new(sf_variable(bytes, 590.0)));
        }
        Err(_) => {
            fonts.font_data.insert(
                "ui".into(),
                Arc::new(FontData::from_static(include_bytes!(
                    "../assets/fonts/Inter-Regular.otf"
                ))),
            );
            fonts.font_data.insert(
                "ui-semibold".into(),
                Arc::new(FontData::from_static(include_bytes!(
                    "../assets/fonts/Inter-SemiBold.otf"
                ))),
            );
        }
    }
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNSMono.ttf") {
        let mut mono = FontData::from_owned(bytes);
        // SFNSMono's Regular named instance per its fvar table.
        mono.tweak.coords =
            egui::epaint::text::VariationCoords::new([(b"wght", 400.0), (b"YAXS", 324.33)]);
        fonts.font_data.insert("ui-mono".into(), Arc::new(mono));
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.insert(0, "ui-mono".into());
        }
    }
    // The UI face leads the proportional family; egui's defaults stay as
    // glyph fallback.
    if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
        family.insert(0, "ui".into());
    }
    fonts.families.insert(
        FontFamily::Name(FAMILY_SEMIBOLD.into()),
        vec!["ui-semibold".into(), "ui".into()],
    );
    fonts
}

/// The one surface every section of rows sits on. The flat and grouped views
/// used to draw *different* frames for the same concept (one stroked, one
/// not), and the snapshot tests drew a third by hand — which made them
/// evidence about nothing. All three call this now.
pub fn section_surface(p: &Palette, dark_mode: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(RADIUS_CONTAINER))
        .inner_margin(egui::Margin::symmetric(14, 6))
        // The single contact-shadow layer of the elevated-surface stack —
        // ordinary content cards otherwise sat at zero elevation while
        // modals got the full four-layer stack, an unjustified two-tier
        // system. `egui::Frame` carries one `Shadow`; the modal-only second
        // layer (ambient + hairline edge) stays modal-exclusive.
        .shadow(elevated_shadows(dark_mode)[0])
}

/// The eyebrow above a section of rows — one code path for app and snapshots.
pub fn section_heading(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .color(p.faint_text)
            .font(medium(10.5)),
    );
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_SEMIBOLD.into()))
}

/// Two weights only: what used to be "medium" is regular — the emphasis those
/// call sites wanted comes from color and size. Shim kept while views migrate.
pub fn medium(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_visuals_of(Theme::Dark, visuals(&dark(), true));
    ctx.set_visuals_of(Theme::Light, visuals(&light(), false));
    ctx.all_styles_mut(|style| {
        let text_styles: BTreeMap<TextStyle, FontId> = [
            (TextStyle::Heading, semibold(SIZE_SECTION)),
            (TextStyle::Body, FontId::proportional(SIZE_BODY)),
            (TextStyle::Button, medium(SIZE_BODY)),
            (TextStyle::Small, FontId::proportional(SIZE_CAPTION)),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
        style.text_styles = text_styles;
        style.spacing.item_spacing = egui::vec2(Space::S8.px(), Space::S6.px());
        style.spacing.button_padding = egui::vec2(Space::S12.px(), 5.0);
        style.spacing.interact_size = egui::vec2(40.0, 26.0);
        style.spacing.window_margin = egui::Margin::same(16);
        style.spacing.menu_margin = egui::Margin::same(8);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel_lin(byte: u8) -> f64 {
        let c = byte as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(color: Color32) -> f64 {
        0.2126 * channel_lin(color.r())
            + 0.7152 * channel_lin(color.g())
            + 0.0722 * channel_lin(color.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// The doc-comment claim on `Palette`, held by a test: `muted` (step 11)
    /// carries real information at caption size, so it must clear WCAG AA
    /// (4.5:1) on every surface it appears over, in both appearances. Step 10
    /// does NOT clear it — asserted too, so nobody "optimizes" muted back down
    /// a step without meeting this test.
    #[test]
    fn text_roles_hold_aa_contrast() {
        for dark_mode in [true, false] {
            let p = palette(dark_mode);
            let ramp = neutral(dark_mode);
            for surface in [p.bg, p.panel, p.widget] {
                assert!(
                    contrast(p.text, surface) >= 7.0,
                    "text on surface below AAA (dark_mode={dark_mode})"
                );
                assert!(
                    contrast(p.muted, surface) >= 4.5,
                    "muted on surface below AA (dark_mode={dark_mode})"
                );
            }
            // The four status/hint roles carry real information as literal
            // text (chip labels, coverage-row summaries, issue levels) — but
            // unlike `text`/`muted`, they are never painted directly on the
            // `widget` (button-rest) fill anywhere in the app; where a
            // status color sits behind text it is `bg`/`panel`, a translucent
            // tinted chip, or a frameless ghost button (effectively `panel`).
            // Held to `muted`'s 4.5:1 floor on the surfaces they actually
            // appear over — `widget` is excluded on purpose, not by oversight.
            for surface in [p.bg, p.panel] {
                assert!(
                    contrast(p.ok, surface) >= 4.5,
                    "ok on surface below AA (dark_mode={dark_mode})"
                );
                assert!(
                    contrast(p.warn, surface) >= 4.5,
                    "warn on surface below AA (dark_mode={dark_mode})"
                );
                assert!(
                    contrast(p.err, surface) >= 4.5,
                    "err on surface below AA (dark_mode={dark_mode})"
                );
                assert!(
                    contrast(p.info, surface) >= 4.5,
                    "info on surface below AA (dark_mode={dark_mode})"
                );
            }
            assert!(
                contrast(ramp.step(10), p.panel) < 4.5,
                "step 10 unexpectedly clears AA — muted could move down a step"
            );
        }
    }

    /// `accent` is never painted as text-on-background — every call site
    /// uses it as a button `fill` under a white label — so its bar is
    /// white-on-accent AA, not accent-as-foreground like the four roles
    /// above. Held separately so nobody "fixes" `accent` back down to a
    /// value that clears the wrong test.
    ///
    /// Light holds the full 4.5:1 text floor — `ACCENT_LIGHT` is a new value
    /// this fix introduces and can be held to the ambitious bar. Dark keeps
    /// `ACCENT`, the pre-existing shipped literal this fix deliberately does
    /// NOT touch (already visually reviewed, already the baseline for every
    /// existing button snapshot) — held to WCAG's 3:1 non-text/large-text
    /// floor, which a short semibold button label legitimately qualifies
    /// for and which this value already clears. Its true text-sized ratio is
    /// ~3.7:1, short of 4.5 — a real, narrow gap, left open on purpose
    /// rather than perturbing a reviewed dark-mode baseline as a side effect
    /// of a light-mode fix; worth a dedicated follow-up.
    #[test]
    fn accent_fill_holds_aa_contrast_for_white_text() {
        let light = palette(false);
        assert!(
            contrast(Color32::WHITE, light.accent) >= 4.5,
            "white text on the light accent fill below AA"
        );
        let dark = palette(true);
        assert!(
            contrast(Color32::WHITE, dark.accent) >= 3.0,
            "white text on the dark accent fill below the non-text/large-text floor"
        );
    }

    /// The spacing scale has no intermediate values and stays sorted — the
    /// discipline is the feature.
    #[test]
    fn the_spacing_scale_is_the_contract() {
        let all = [
            Space::S0,
            Space::S1,
            Space::S2,
            Space::S3,
            Space::S4,
            Space::S6,
            Space::S8,
            Space::S12,
            Space::S16,
            Space::S20,
            Space::S24,
            Space::S32,
        ];
        let px: Vec<f32> = all.iter().map(|s| s.px()).collect();
        assert_eq!(
            px,
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 20.0, 24.0, 32.0]
        );
    }

    /// Zed's inversion, held by a test: dark-mode shadows are STRONGER than
    /// light — dark surfaces are separated by so little luminance the shadow
    /// has to carry it.
    #[test]
    fn dark_mode_shadows_are_stronger() {
        let (light_elev, dark_elev) = (elevated_shadows(false), elevated_shadows(true));
        assert!(dark_elev[1].color.a() > light_elev[1].color.a());
        let (light_modal, dark_modal) = (modal_shadows(false), modal_shadows(true));
        assert!(dark_modal[0].color.a() > light_modal[0].color.a());
        // The modal stack ends in a zero-blur hairline edge.
        assert_eq!(dark_modal[3].blur, 0);
        assert_eq!(dark_modal[3].offset, [0, 1]);
    }

    /// Hairline snapping lands on the physical pixel grid at 1x and 2x.
    #[test]
    fn snapped_coordinates_land_on_the_pixel_grid() {
        for ppp in [1.0_f32, 2.0] {
            for y in [0.3_f32, 7.49, 7.51, 103.7] {
                let snapped = snap_y(y, ppp);
                let device = snapped * ppp;
                assert!(
                    (device - device.round()).abs() < 1e-4,
                    "y={y} ppp={ppp} -> {snapped} not on grid"
                );
            }
        }
    }

    /// The UI face is SF Pro read at runtime from the system path (never
    /// embedded — Apple's license), with the weight axis pinned per fvar;
    /// Inter is the fallback. On a Mac with the system font present, the
    /// loaded bytes must be the system file's, byte-for-length.
    #[test]
    fn the_ui_face_prefers_sf_pro_and_is_never_embedded() {
        let defs = fonts();
        assert!(defs.font_data.contains_key("ui"));
        assert!(defs.font_data.contains_key("ui-semibold"));
        assert_eq!(defs.families[&FontFamily::Proportional][0], "ui");
        let system = std::path::Path::new("/System/Library/Fonts/SFNS.ttf");
        if system.exists() {
            let len = std::fs::metadata(system).expect("metadata").len() as usize;
            assert_eq!(
                defs.font_data["ui"].font.len(),
                len,
                "runtime-loaded SF Pro"
            );
            assert_ne!(
                defs.font_data["ui"].tweak.coords, defs.font_data["ui-semibold"].tweak.coords,
                "regular and semibold pin different weight-axis values"
            );
        }
        if std::path::Path::new("/System/Library/Fonts/SFNSMono.ttf").exists() {
            assert_eq!(defs.families[&FontFamily::Monospace][0], "ui-mono");
        }
    }

    /// The egui sidebar panel and the native glass strip (Phase E) must be
    /// the same width — the two constants live in different crates, so this
    /// binds them.
    #[test]
    fn the_sidebar_width_matches_the_native_strip_contract() {
        assert_eq!(
            SIDEBAR_WIDTH,
            ade_core::gui::chrome::SIDEBAR_WIDTH_PT as f32
        );
    }

    /// The focus box is reserved while unfocused: same width stroke,
    /// transparent color — so focus arriving never shifts layout.
    #[test]
    fn focus_border_box_is_reserved_when_unfocused() {
        let off = focus_stroke(false);
        let on = focus_stroke(true);
        assert_eq!(off.width, on.width);
        assert_eq!(off.color, Color32::TRANSPARENT);
        assert_eq!(on.color, ACCENT);
    }
}
