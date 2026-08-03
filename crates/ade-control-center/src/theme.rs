//! Visual identity for the ADE Control Center.
//!
//! The system is two layers, borrowed from how Zed's `crates/ui` is built:
//! a perceptual 12-step neutral ramp underneath (Radix "sand" in light,
//! Radix "slate" in dark — warm paper by day, cool blue-cast depth by
//! night, the Zed/Warp/Linear family) and named semantic roles on top. View code speaks
//! roles ("surface", "text_muted") and scales (`SIZE_*`, `Space`), never raw
//! hexes or invented sizes — the absence of intermediate values is what makes
//! the result read as designed rather than assembled.
//!
//! Interaction states are *alpha washes* from the ramp's alpha variant, so one
//! token composites correctly over any surface. Selection is a background;
//! only keyboard focus is a border, and the border box is reserved while
//! unfocused so nothing shifts. The "no stock-egui look" claim (ISC-182.1)
//! lives here.

use ade_core::gui::tokens::{self, Appearance, ColorRole};
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
/// Page titles (capability-page titles).
pub const SIZE_TITLE: f32 = 20.0;
/// The one display size — the Overview verdict headline, nothing else.
/// macOS's large-title metric: a page gets at most one voice this loud
/// (ISC-310 cycle 3, under the owner's standing "keep iterating" directive).
pub const SIZE_DISPLAY: f32 = 26.0;

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

// ───────────────────────── token resolution ─────────────────────────
// Phase J: the color DATA moved to `ade_core::gui::tokens` — the Radix
// ramps (sand light / slateDark dark), status solids, and every semantic
// role live there, once. This file is the ONLY sanctioned resolution layer
// from those tokens to `egui::Color32` (the ISC-304 grep gate holds every
// other file to zero raw color constructors), and the pinned-baseline test
// at the bottom pins every resolved value to its authority: Phase I
// (d5b1ba8) for light/durations/shadows/radii, the quoted 2026-08-02T19:45Z
// owner Decision for the slate dark palette and the six cycle-1 roles.

/// Appearance key for the token layer, from this file's `dark_mode` idiom.
const fn appearance(dark_mode: bool) -> Appearance {
    if dark_mode {
        Appearance::Dark
    } else {
        Appearance::Light
    }
}

/// Resolve a semantic role to a renderer color. The one place in the GUI
/// crates where token bytes become `Color32` — everything else asks the
/// `Palette` (or the named consts below) rather than constructing color.
const fn tc(role: ColorRole, dark_mode: bool) -> Color32 {
    let c = tokens::resolve(role, appearance(dark_mode));
    Color32::from_rgba_unmultiplied_const(c.r(), c.g(), c.b(), c.a())
}

/// Text painted on an `accent` fill — appearance-constant (white), exposed
/// as a const for the call sites (`ax_button`) that have no `Palette` in
/// scope.
pub const ON_ACCENT: Color32 = tc(ColorRole::OnAccent, true);

/// Explicit no-fill — the named form of "paint nothing here", so view code
/// states the decision instead of reaching for a raw transparent constant.
pub const NO_FILL: Color32 = tc(ColorRole::NoFill, true);

/// Interaction states for SATURATED button fills (accent, err): a white
/// lift on hover and a black shade on press, composited over the fill via
/// `blend_over` — hue-preserving, where lerping toward the neutral widget
/// ramp visibly grayed a blue button on approach. Appearance-constant.
pub const FILL_HOVER_LIFT: Color32 = tc(ColorRole::FillHoverLift, true);
/// See `FILL_HOVER_LIFT`.
pub const FILL_PRESS_SHADE: Color32 = tc(ColorRole::FillPressShade, true);
/// The 1px "gel" top light inside a filled button — the Big Sur cue.
pub const BUTTON_TOP_LIGHT: Color32 = tc(ColorRole::ButtonTopLight, true);

/// Public role resolution for surfaces that need a color OUTSIDE the current
/// appearance (the gallery shows both appearances side by side). Ordinary
/// view code holds a `Palette`; this exists for token introspection, not as
/// a second path around it.
pub fn role_color(role: ColorRole, dark_mode: bool) -> Color32 {
    tc(role, dark_mode)
}

/// Scale a color's ALPHA alone, preserving its RGB — `Color32::gamma_multiply`
/// scales all four channels, which is only safe for pure-black shadow colors
/// (their RGB is already 0). Entrance fades on real colors (the modal panel
/// fill/stroke/scrim) need this instead. Lives here because decomposing and
/// reconstructing a color is resolution-layer work — view code hands a
/// resolved color in and gets a resolved color back.
pub fn alpha_scaled(color: Color32, factor: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let a = (a as f32 * factor).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, a)
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

/// The dark-appearance accent, kept as a named const because `visuals()`
/// and `focus_stroke()` use it appearance-independently (the shipped Phase I
/// behavior this refactor must not change) — resolved from the token layer
/// like everything else. The full light/dark role story, including why the
/// light values are each hue's AA-safe Radix midpoint, is documented on the
/// token data in `ade_core::gui::tokens`.
pub const ACCENT: Color32 = tc(ColorRole::Accent, true);

/// The macOS-style focus ring (ISC-309, focus-ring surface): a soft accent
/// halo OUTSIDE the control's edge plus the crisp 1px line `focus_stroke`
/// already draws inside it. Real macOS focus rings sit outside the control
/// and glow rather than outline — a bare 1px inner stroke reads as a border
/// change, not as focus. Paints nothing while unfocused; the layout box is
/// already reserved by `focus_stroke`'s transparent stroke, so focus
/// arriving still shifts nothing.
pub fn focus_ring(ui: &egui::Ui, rect: egui::Rect, radius: u8, focused: bool) {
    use egui::emath::GuiRounding as _;
    if !focused {
        return;
    }
    let halo = rect.expand(1.5);
    ui.painter().rect_stroke(
        halo.round_to_pixels(ui.pixels_per_point()),
        CornerRadius::same(radius.saturating_add(2)),
        Stroke::new(3.0, ACCENT.gamma_multiply(0.35)),
        egui::StrokeKind::Outside,
    );
}

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
    /// Neutral selected wash (list selection) — alpha step 5. Nav rows
    /// moved to `selected_accent` in ISC-310 cycle 1; this stays as the
    /// vocabulary's neutral option for list rows where an accent field
    /// would fight per-row status colors (same ships-now rationale as the
    /// unused `Space` steps above).
    #[allow(dead_code)]
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
    /// Toggle-switch knob — appearance-constant white.
    pub knob: Color32,
    /// Toggle-switch track in the off position — the one appearance-varying
    /// gray that used to be hardcoded in widget code (Phase J moved it here).
    pub toggle_track_off: Color32,
    /// Modal backdrop scrim — translucent black, both appearances.
    pub scrim: Color32,
    /// 1px top rim light for elevated dark surfaces — white at ~5% alpha in
    /// dark, fully transparent in light (paper needs no rim light). Painted
    /// by `elevated_card` as a short inset line, never a full border.
    pub edge_highlight: Color32,
    /// Selected wash carrying the accent hue — the macOS sidebar-selection
    /// idiom (ISC-310 cycle 1). `selected` remains the neutral wash for
    /// list rows where a blue wash would fight per-row status colors.
    pub selected_accent: Color32,
    /// Translucent status tint for card surfaces — pre-blend over `panel`
    /// with `blend_over` before filling; never composite under text.
    pub tint_ok: Color32,
    /// See `tint_ok`.
    pub tint_warn: Color32,
    /// See `tint_ok`.
    pub tint_err: Color32,
    /// See `tint_ok`.
    pub tint_info: Color32,
}

pub fn palette(dark_mode: bool) -> Palette {
    Palette {
        bg: tc(ColorRole::Bg, dark_mode),
        panel: tc(ColorRole::Panel, dark_mode),
        inset: tc(ColorRole::Inset, dark_mode),
        widget: tc(ColorRole::Widget, dark_mode),
        widget_hover: tc(ColorRole::WidgetHover, dark_mode),
        widget_active: tc(ColorRole::WidgetActive, dark_mode),
        line: tc(ColorRole::Line, dark_mode),
        hairline: tc(ColorRole::Hairline, dark_mode),
        row_hover: tc(ColorRole::RowHover, dark_mode),
        ghost_active: tc(ColorRole::GhostActive, dark_mode),
        selected: tc(ColorRole::Selected, dark_mode),
        text: tc(ColorRole::Default, dark_mode),
        muted: tc(ColorRole::Muted, dark_mode),
        faint_text: tc(ColorRole::Faint, dark_mode),
        disabled_text: tc(ColorRole::Disabled, dark_mode),
        accent: tc(ColorRole::Accent, dark_mode),
        ok: tc(ColorRole::Success, dark_mode),
        warn: tc(ColorRole::Warning, dark_mode),
        err: tc(ColorRole::Error, dark_mode),
        info: tc(ColorRole::Info, dark_mode),
        knob: tc(ColorRole::Knob, dark_mode),
        toggle_track_off: tc(ColorRole::ToggleTrackOff, dark_mode),
        scrim: tc(ColorRole::Scrim, dark_mode),
        edge_highlight: tc(ColorRole::EdgeHighlight, dark_mode),
        selected_accent: tc(ColorRole::SelectedAccent, dark_mode),
        tint_ok: tc(ColorRole::TintOk, dark_mode),
        tint_warn: tc(ColorRole::TintWarn, dark_mode),
        tint_err: tc(ColorRole::TintErr, dark_mode),
        tint_info: tc(ColorRole::TintInfo, dark_mode),
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

/// Composite a translucent color over an opaque base, returning the opaque
/// result. Status tints go through this before becoming a card fill, so the
/// tinted surface is one real color — text renders on it exactly as on any
/// solid — instead of a translucent layer whose final value depends on
/// paint order.
pub fn blend_over(base: Color32, over: Color32) -> Color32 {
    // `Color32` stores channels PREMULTIPLIED — `over.r()` on a translucent
    // color is already alpha-scaled, so multiplying by alpha again here
    // would double-apply it and wash the tint out to near-nothing (caught
    // by pixel measurement on the first cycle-1 goldens). Unmultiply first.
    let [or, og, ob, oa] = over.to_srgba_unmultiplied();
    let a = oa as f32 / 255.0;
    let ch = |b: u8, o: u8| ((b as f32) * (1.0 - a) + (o as f32) * a).round() as u8;
    Color32::from_rgb(ch(base.r(), or), ch(base.g(), og), ch(base.b(), ob))
}

/// THE card surface — the one frame every section of rows sits on (it
/// absorbed the earlier `section_surface` once ISC-311 migrated its last
/// caller), plus the two depth cues polished dark UIs carry (ISC-310
/// cycle 1): an optional status tint pre-blended into the fill, and a 1px
/// top rim light inset past the corner radius — the inner-bevel light
/// source Zed and Warp both paint on elevated dark panels. In light
/// appearance `edge_highlight` resolves fully transparent, so the rim
/// simply isn't painted. `y_margin` comes from the spacing scale because
/// the cards this replaces each need real breathing room above their
/// headline (the cycle-1 spacing nit), not the 6px list default.
pub fn elevated_card<R>(
    ui: &mut egui::Ui,
    p: &Palette,
    dark_mode: bool,
    tint: Option<Color32>,
    y_margin: Space,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let fill = match tint {
        Some(t) => blend_over(p.panel, t),
        None => p.panel,
    };
    let out = egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(RADIUS_CONTAINER))
        .inner_margin(egui::Margin::symmetric(14, y_margin.px() as i8))
        .shadow(elevated_shadows(dark_mode)[0])
        .show(ui, add);
    if p.edge_highlight.a() > 0 {
        let rect = out.response.rect;
        let y = snap_y(rect.top() + 0.5, ui.pixels_per_point());
        let inset = RADIUS_CONTAINER as f32;
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + inset, y),
                egui::pos2(rect.right() - inset, y),
            ],
            Stroke::new(1.0, p.edge_highlight),
        );
    }
    out
}

/// The eyebrow above a section of rows — one code path for app and snapshots.
pub fn section_heading(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .color(p.faint_text)
            .font(medium(10.5))
            // Uppercase micro-labels need air between the caps to read as
            // typography instead of shouting — the tracking every polished
            // eyebrow (macOS, Linear, Zed) carries.
            .extra_letter_spacing(0.8),
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

    /// Rgba -> Color32 for test assertions that reach into the token layer
    /// directly (the same conversion `tc` performs).
    fn tc_test(c: tokens::Rgba) -> Color32 {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), c.a())
    }

    /// ISC-307's pinned baseline — the fixture Cato's WARNING-5 asked for.
    ///
    /// Every value below is a hardcoded literal (deriving them from the
    /// token layer would make this test circular), pinned by one of two
    /// authorities:
    ///
    /// - LIGHT palette, durations, radii, type scale, shadows: transcribed
    ///   from this file AS OF PHASE I CLOSE (commit
    ///   d5b1ba88afc97bf93c3327a2aeea800bd4252eb6). Unchanged since.
    /// - DARK palette + the six cycle-1 roles: Radix slateDark, adopted by
    ///   owner direction at the ISC-310 checkpoint ("this all still looks
    ///   rather bland and uninspired") — ISA `## Decisions`,
    ///   2026-08-02T19:45Z. That quoted Decision is the ONLY authority
    ///   under which these pins moved off the d5b1ba8 sand-dark values.
    ///
    /// If any assertion fails without a new quoted owner Decision, a
    /// reviewed value drifted — that is a regression, not a re-baselining
    /// opportunity.
    #[test]
    fn pinned_baseline_values_are_unchanged() {
        // Durations — the four motion constants, exactly as shipped.
        assert_eq!(DURATION_HOVER, 0.1);
        assert_eq!(DURATION_VALUE, 0.26);
        assert_eq!(DURATION_ENTRANCE, 0.15);
        assert_eq!(DURATION_PAYOFF, 0.2);

        // Radii and type scale.
        assert_eq!(
            (RADIUS_CONTROL, RADIUS_CONTAINER, RADIUS_POPOVER),
            (5, 10, 12)
        );
        assert_eq!(
            (
                SIZE_CAPTION,
                SIZE_BODY,
                SIZE_SECTION,
                SIZE_TITLE,
                SIZE_DISPLAY
            ),
            (11.0, 13.0, 15.0, 20.0, 26.0)
        );

        // Elevated shadows, both appearances, every field.
        for (dark_mode, hairline_alpha) in [(true, 15), (false, 8)] {
            let s = elevated_shadows(dark_mode);
            assert_eq!(s[0].offset, [0, 2]);
            assert_eq!((s[0].blur, s[0].spread), (3, 0));
            assert_eq!(s[0].color, Color32::from_black_alpha(31));
            assert_eq!(s[1].offset, [0, 1]);
            assert_eq!((s[1].blur, s[1].spread), (0, 0));
            assert_eq!(s[1].color, Color32::from_black_alpha(hairline_alpha));
        }

        // Modal shadows, both appearances, every field.
        for (dark_mode, top, mid, edge) in [(true, 31, 20, 31), (false, 15, 15, 10)] {
            let s = modal_shadows(dark_mode);
            assert_eq!(
                (s[0].offset, s[0].blur, s[0].color),
                ([0, 2], 3, Color32::from_black_alpha(top))
            );
            assert_eq!(
                (s[1].offset, s[1].blur, s[1].color),
                ([0, 3], 6, Color32::from_black_alpha(mid))
            );
            assert_eq!(
                (s[2].offset, s[2].blur, s[2].color),
                ([0, 6], 12, Color32::from_black_alpha(10))
            );
            assert_eq!(
                (s[3].offset, s[3].blur, s[3].color),
                ([0, 1], 0, Color32::from_black_alpha(edge))
            );
        }

        // The dark palette, field by field, against the Radix slateDark
        // literals (the 2026-08-02T19:45Z owner Decision — see the test doc).
        let d = dark();
        assert_eq!(d.bg, Color32::from_rgb(0x11, 0x11, 0x13));
        assert_eq!(d.panel, Color32::from_rgb(0x18, 0x19, 0x1b));
        assert_eq!(d.inset, Color32::from_rgb(0x11, 0x11, 0x13));
        assert_eq!(d.widget, Color32::from_rgb(0x21, 0x22, 0x25));
        assert_eq!(d.widget_hover, Color32::from_rgb(0x27, 0x2a, 0x2d));
        assert_eq!(d.widget_active, Color32::from_rgb(0x2e, 0x31, 0x35));
        assert_eq!(d.line, Color32::from_rgb(0x36, 0x3a, 0x3f));
        assert_eq!(
            d.hairline,
            Color32::from_rgba_unmultiplied(0xd3, 0xed, 0xf8, 0x1d)
        );
        assert_eq!(
            d.row_hover,
            Color32::from_rgba_unmultiplied(0xdd, 0xea, 0xf8, 0x14)
        );
        assert_eq!(
            d.ghost_active,
            Color32::from_rgba_unmultiplied(0xd3, 0xed, 0xf8, 0x1d)
        );
        assert_eq!(
            d.selected,
            Color32::from_rgba_unmultiplied(0xd9, 0xed, 0xfe, 0x25)
        );
        assert_eq!(d.text, Color32::from_rgb(0xed, 0xee, 0xf0));
        assert_eq!(d.muted, Color32::from_rgb(0xb0, 0xb4, 0xba));
        assert_eq!(d.faint_text, Color32::from_rgb(0xb0, 0xb4, 0xba));
        assert_eq!(d.disabled_text, Color32::from_rgb(0x69, 0x6e, 0x77));
        assert_eq!(d.accent, Color32::from_rgb(76, 125, 255));
        assert_eq!(d.ok, Color32::from_rgb(47, 191, 118));
        assert_eq!(d.warn, Color32::from_rgb(226, 156, 60));
        assert_eq!(d.err, Color32::from_rgb(224, 92, 92));
        assert_eq!(d.info, Color32::from_rgb(110, 163, 224));

        // The light palette, field by field. Surface planes carry the
        // cycle-3 values (white cards on a gray sand canvas, controls one
        // step darker) under the owner's quoted 2026-08-02T21:15Z standing
        // directive ("I still expect more polish ... Keep iterating") —
        // text/status/wash pins remain the d5b1ba8 literals.
        let l = light();
        assert_eq!(l.bg, Color32::from_rgb(0xf1, 0xf0, 0xef));
        assert_eq!(l.panel, Color32::from_rgb(0xff, 0xff, 0xff));
        assert_eq!(l.inset, Color32::from_rgb(0xf9, 0xf9, 0xf8));
        assert_eq!(l.widget, Color32::from_rgb(0xe9, 0xe8, 0xe6));
        assert_eq!(l.widget_hover, Color32::from_rgb(0xe2, 0xe1, 0xde));
        assert_eq!(l.widget_active, Color32::from_rgb(0xda, 0xd9, 0xd6));
        assert_eq!(l.line, Color32::from_rgb(0xda, 0xd9, 0xd6));
        assert_eq!(
            l.hairline,
            Color32::from_rgba_unmultiplied(0x1f, 0x15, 0x00, 0x19)
        );
        assert_eq!(
            l.row_hover,
            Color32::from_rgba_unmultiplied(0x20, 0x10, 0x00, 0x10)
        );
        assert_eq!(
            l.ghost_active,
            Color32::from_rgba_unmultiplied(0x1f, 0x15, 0x00, 0x19)
        );
        assert_eq!(
            l.selected,
            Color32::from_rgba_unmultiplied(0x1f, 0x18, 0x00, 0x21)
        );
        assert_eq!(l.text, Color32::from_rgb(0x21, 0x20, 0x1c));
        assert_eq!(l.muted, Color32::from_rgb(0x63, 0x63, 0x5e));
        assert_eq!(l.faint_text, Color32::from_rgb(0x63, 0x63, 0x5e));
        assert_eq!(l.disabled_text, Color32::from_rgb(0x8d, 0x8d, 0x86));
        assert_eq!(l.accent, Color32::from_rgb(0x3e, 0x63, 0xdd));
        assert_eq!(l.ok, Color32::from_rgb(0x1d, 0x5f, 0x42));
        assert_eq!(l.warn, Color32::from_rgb(0x7d, 0x4c, 0x11));
        assert_eq!(l.err, Color32::from_rgb(0x99, 0x21, 0x2a));
        assert_eq!(l.info, Color32::from_rgb(0x0f, 0x53, 0x99));

        // The Phase J additions resolve to the exact values their previously
        // hardcoded call sites shipped: white knob, gray-70/gray-190 track,
        // black-alpha-100 scrim, white-on-accent, full transparency.
        assert_eq!(d.knob, Color32::WHITE);
        assert_eq!(l.knob, Color32::WHITE);
        assert_eq!(d.toggle_track_off, Color32::from_gray(70));
        assert_eq!(l.toggle_track_off, Color32::from_gray(190));
        assert_eq!(d.scrim, Color32::from_black_alpha(100));
        assert_eq!(l.scrim, Color32::from_black_alpha(100));
        assert_eq!(ON_ACCENT, Color32::WHITE);
        assert_eq!(NO_FILL, Color32::TRANSPARENT);
        assert_eq!(
            FILL_HOVER_LIFT,
            Color32::from_rgba_unmultiplied(255, 255, 255, 26)
        );
        assert_eq!(
            FILL_PRESS_SHADE,
            Color32::from_rgba_unmultiplied(0, 0, 0, 36)
        );
        assert_eq!(
            BUTTON_TOP_LIGHT,
            Color32::from_rgba_unmultiplied(255, 255, 255, 46)
        );

        // The six cycle-1 roles (same 19:45Z Decision).
        assert_eq!(
            d.edge_highlight,
            Color32::from_rgba_unmultiplied(255, 255, 255, 14)
        );
        assert_eq!(
            l.edge_highlight,
            Color32::from_rgba_unmultiplied(255, 255, 255, 0)
        );
        assert_eq!(
            d.selected_accent,
            Color32::from_rgba_unmultiplied(76, 125, 255, 46)
        );
        assert_eq!(
            l.selected_accent,
            Color32::from_rgba_unmultiplied(0x3e, 0x63, 0xdd, 36)
        );
        assert_eq!(d.tint_ok, Color32::from_rgba_unmultiplied(47, 191, 118, 20));
        assert_eq!(
            d.tint_warn,
            Color32::from_rgba_unmultiplied(226, 156, 60, 20)
        );
        assert_eq!(d.tint_err, Color32::from_rgba_unmultiplied(224, 92, 92, 20));
        assert_eq!(
            d.tint_info,
            Color32::from_rgba_unmultiplied(110, 163, 224, 20)
        );
        assert_eq!(
            l.tint_ok,
            Color32::from_rgba_unmultiplied(0x1d, 0x5f, 0x42, 20)
        );
        assert_eq!(
            l.tint_warn,
            Color32::from_rgba_unmultiplied(0x7d, 0x4c, 0x11, 22)
        );
        assert_eq!(
            l.tint_err,
            Color32::from_rgba_unmultiplied(0x99, 0x21, 0x2a, 20)
        );
        assert_eq!(
            l.tint_info,
            Color32::from_rgba_unmultiplied(0x0f, 0x53, 0x99, 20)
        );

        // The theme-level consts still resolve to the shipped dark values.
        assert_eq!(ACCENT, Color32::from_rgb(76, 125, 255));
    }

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
            let step10 = tc_test(tokens::neutral_step(appearance(dark_mode), 10));
            assert!(
                contrast(step10, p.panel) < 4.5,
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

    /// The traffic-light inset and the core titlebar-band contract are one
    /// value (ISC-309) — the egui nav inset, the drag band, and any native
    /// positioning all measure the same zone.
    #[test]
    fn the_titlebar_band_matches_the_traffic_light_inset() {
        assert_eq!(
            TRAFFIC_LIGHT_INSET,
            ade_core::gui::chrome::TITLEBAR_BAND_PT as f32
        );
    }

    /// `blend_over` composites UNMULTIPLIED channels — `Color32` stores
    /// premultiplied, and reading `.r()` off a translucent color then
    /// multiplying by alpha again double-applies it, washing every status
    /// tint out to a near-invisible gray (the exact bug the first cycle-1
    /// goldens shipped, caught by pixel measurement).
    #[test]
    fn blend_over_composites_unmultiplied_channels() {
        let base = Color32::from_rgb(24, 25, 27); // slate panel
        let over = Color32::from_rgba_unmultiplied(226, 156, 60, 20); // tint_warn dark
        assert_eq!(blend_over(base, over), Color32::from_rgb(40, 35, 30));
        assert_eq!(blend_over(base, Color32::TRANSPARENT), base);
        // Opaque over wins completely.
        let opaque = Color32::from_rgb(1, 2, 3);
        assert_eq!(blend_over(base, opaque), opaque);
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
