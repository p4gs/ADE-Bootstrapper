//! The component gallery — Zed's `component_preview` pattern (ISC-308).
//!
//! A debug view inside the real running app that renders every design token
//! and every shared component, in every state, side by side. Opened with
//! Cmd+Shift+G; the same render function feeds the kittest goldens, so the
//! gallery is simultaneously the fast-iteration surface for design work and
//! a permanent regression fixture — the exact reason it exists instead of a
//! throwaway mockup (ISA Decisions 2026-08-02, order-of-operations item 2).
//!
//! Boundary (ISC-308): this file WRAPS existing components — `ax_button`,
//! `toggle`, `chip`, `status_dot`, the theme surfaces — it never restyles
//! them, and no live screen changes to accommodate it. The one wiring touch
//! is `app.rs`'s keyboard shortcut + takeover branch.

use crate::app::{ax_button, ax_ghost_button, chip, status_dot, toggle, Dot};
use crate::theme;
use ade_core::gui::tokens::{self, Appearance, ColorRole, Density, Roundness};
use egui::{Color32, CornerRadius, RichText, Stroke, StrokeKind};

/// Demo-widget state while the gallery is open. `None` in `LocalState`
/// means closed — the flag and the state are one field on purpose.
pub(crate) struct GalleryState {
    pub demo_toggle_on: bool,
    pub demo_toggle_off: bool,
}

impl Default for GalleryState {
    fn default() -> Self {
        GalleryState {
            demo_toggle_on: true,
            demo_toggle_off: false,
        }
    }
}

/// Every color role, in gallery display order. Lives here rather than on the
/// enum because the ORDER is a presentation decision (surfaces, then washes,
/// then text, then status, then odds and ends) — the token layer has no
/// opinion about it.
const ROLES: [ColorRole; 34] = [
    ColorRole::Bg,
    ColorRole::Panel,
    ColorRole::Inset,
    ColorRole::Widget,
    ColorRole::WidgetHover,
    ColorRole::WidgetActive,
    ColorRole::Line,
    ColorRole::Hairline,
    ColorRole::RowHover,
    ColorRole::GhostActive,
    ColorRole::Selected,
    ColorRole::Default,
    ColorRole::Muted,
    ColorRole::Faint,
    ColorRole::Disabled,
    ColorRole::Accent,
    ColorRole::OnAccent,
    ColorRole::Success,
    ColorRole::Warning,
    ColorRole::Error,
    ColorRole::Info,
    ColorRole::Knob,
    ColorRole::ToggleTrackOff,
    ColorRole::Scrim,
    ColorRole::NoFill,
    ColorRole::EdgeHighlight,
    ColorRole::SelectedAccent,
    ColorRole::TintOk,
    ColorRole::TintWarn,
    ColorRole::TintErr,
    ColorRole::TintInfo,
    ColorRole::FillHoverLift,
    ColorRole::FillPressShade,
    ColorRole::ButtonTopLight,
];

fn role_name(role: ColorRole) -> &'static str {
    match role {
        ColorRole::Bg => "Bg",
        ColorRole::Panel => "Panel",
        ColorRole::Inset => "Inset",
        ColorRole::Widget => "Widget",
        ColorRole::WidgetHover => "WidgetHover",
        ColorRole::WidgetActive => "WidgetActive",
        ColorRole::Line => "Line",
        ColorRole::Hairline => "Hairline",
        ColorRole::RowHover => "RowHover",
        ColorRole::GhostActive => "GhostActive",
        ColorRole::Selected => "Selected",
        ColorRole::Default => "Default",
        ColorRole::Muted => "Muted",
        ColorRole::Faint => "Faint",
        ColorRole::Disabled => "Disabled",
        ColorRole::Accent => "Accent",
        ColorRole::OnAccent => "OnAccent",
        ColorRole::Success => "Success",
        ColorRole::Warning => "Warning",
        ColorRole::Error => "Error",
        ColorRole::Info => "Info",
        ColorRole::Knob => "Knob",
        ColorRole::ToggleTrackOff => "ToggleTrackOff",
        ColorRole::Scrim => "Scrim",
        ColorRole::NoFill => "NoFill",
        ColorRole::EdgeHighlight => "EdgeHighlight",
        ColorRole::SelectedAccent => "SelectedAccent",
        ColorRole::TintOk => "TintOk",
        ColorRole::TintWarn => "TintWarn",
        ColorRole::TintErr => "TintErr",
        ColorRole::TintInfo => "TintInfo",
        ColorRole::FillHoverLift => "FillHoverLift",
        ColorRole::FillPressShade => "FillPressShade",
        ColorRole::ButtonTopLight => "ButtonTopLight",
    }
}

fn hex(color: Color32) -> String {
    if color.a() == 255 {
        format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r(),
            color.g(),
            color.b(),
            color.a()
        )
    }
}

fn caption(ui: &mut egui::Ui, p: &theme::Palette, text: &str) {
    ui.label(RichText::new(text).color(p.muted).size(theme::SIZE_CAPTION));
}

fn mono_caption(ui: &mut egui::Ui, p: &theme::Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .color(p.muted)
            .font(egui::FontId::monospace(11.0)),
    );
}

fn swatch(ui: &mut egui::Ui, p: &theme::Palette, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 16.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        3.0,
        color,
        Stroke::new(1.0, p.line),
        StrokeKind::Inside,
    );
}

/// The gallery view. Pure like every other view: takes the palette and demo
/// state, paints, mutates only its own demo state.
pub(crate) fn gallery(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    dark_mode: bool,
    state: &mut GalleryState,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            crate::app::content_column(ui, |ui| {
                ui.add_space(theme::Space::S16.px());
                ui.label(
                    RichText::new("Design gallery")
                        .font(theme::semibold(theme::SIZE_TITLE))
                        .color(p.text),
                );
                caption(
                    ui,
                    p,
                    "Every token and shared component, in every state. Cmd+Shift+G closes.",
                );
                ui.add_space(theme::Space::S12.px());

                color_roles_section(ui, p);
                contrast_audit_section(ui, p);
                type_scale_section(ui, p);
                spacing_section(ui, p);
                shape_and_elevation_section(ui, p, dark_mode);
                motion_section(ui, p);
                components_section(ui, p, state);
                ui.add_space(theme::Space::S24.px());
            });
        });
}

fn section(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    dark_mode: bool,
    title: &str,
    add: impl FnOnce(&mut egui::Ui),
) {
    theme::section_heading(ui, p, title);
    theme::elevated_card(ui, p, dark_mode, None, theme::Space::S12, |ui| add(ui));
    ui.add_space(theme::Space::S16.px());
}

fn color_roles_section(ui: &mut egui::Ui, p: &theme::Palette) {
    let dark_mode = ui.visuals().dark_mode;
    section(ui, p, dark_mode, "Color roles", |ui| {
        egui::Grid::new("gallery-roles")
            .num_columns(5)
            .spacing(egui::vec2(theme::Space::S12.px(), theme::Space::S3.px()))
            .show(ui, |ui| {
                mono_caption(ui, p, "role");
                mono_caption(ui, p, "light");
                mono_caption(ui, p, "");
                mono_caption(ui, p, "dark");
                mono_caption(ui, p, "");
                ui.end_row();
                for role in ROLES {
                    let light = theme::role_color(role, false);
                    let dark = theme::role_color(role, true);
                    ui.label(
                        RichText::new(role_name(role))
                            .color(p.text)
                            .size(theme::SIZE_CAPTION),
                    );
                    swatch(ui, p, light);
                    mono_caption(ui, p, &hex(light));
                    swatch(ui, p, dark);
                    mono_caption(ui, p, &hex(dark));
                    ui.end_row();
                }
            });
    });
}

/// The audit table: (label, fg, bg, floor_light, floor_dark). ONE constant
/// shared by the display function and the gate test below, so what the
/// gallery shows and what the suite enforces can never drift apart. The
/// OnAccent/Accent dark floor is 3.0 — the shipped, documented large-text
/// exception (`accent_fill_holds_aa_contrast_for_white_text`), not a
/// gallery invention.
const CONTRAST_PAIRS: [(&str, ColorRole, ColorRole, f64, f64); 9] = [
    ("Default / Bg", ColorRole::Default, ColorRole::Bg, 7.0, 7.0),
    (
        "Default / Panel",
        ColorRole::Default,
        ColorRole::Panel,
        7.0,
        7.0,
    ),
    ("Muted / Bg", ColorRole::Muted, ColorRole::Bg, 4.5, 4.5),
    (
        "Muted / Panel",
        ColorRole::Muted,
        ColorRole::Panel,
        4.5,
        4.5,
    ),
    (
        "Success / Panel",
        ColorRole::Success,
        ColorRole::Panel,
        4.5,
        4.5,
    ),
    (
        "Warning / Panel",
        ColorRole::Warning,
        ColorRole::Panel,
        4.5,
        4.5,
    ),
    (
        "Error / Panel",
        ColorRole::Error,
        ColorRole::Panel,
        4.5,
        4.5,
    ),
    ("Info / Panel", ColorRole::Info, ColorRole::Panel, 4.5, 4.5),
    (
        "OnAccent / Accent",
        ColorRole::OnAccent,
        ColorRole::Accent,
        4.5,
        3.0,
    ),
];

/// The gallery doubles as a live audit surface: the ratios below are
/// computed from the token data in-place, so a token change that breaks a
/// floor is visible HERE, in the same view a design pass iterates in —
/// not only in a test run afterward.
fn contrast_audit_section(ui: &mut egui::Ui, p: &theme::Palette) {
    let pairs = CONTRAST_PAIRS;
    let dark_mode = ui.visuals().dark_mode;
    section(ui, p, dark_mode, "Contrast audit", |ui| {
        egui::Grid::new("gallery-contrast")
            .num_columns(3)
            .spacing(egui::vec2(theme::Space::S12.px(), theme::Space::S3.px()))
            .show(ui, |ui| {
                mono_caption(ui, p, "pair");
                mono_caption(ui, p, "light");
                mono_caption(ui, p, "dark");
                ui.end_row();
                for (label, fg, bg, floor_light, floor_dark) in pairs {
                    ui.label(RichText::new(label).color(p.text).size(theme::SIZE_CAPTION));
                    for (appearance, floor) in [
                        (Appearance::Light, floor_light),
                        (Appearance::Dark, floor_dark),
                    ] {
                        let ratio = tokens::contrast_ratio(
                            tokens::resolve(fg, appearance),
                            tokens::resolve(bg, appearance),
                        );
                        let pass = ratio >= floor;
                        ui.label(
                            RichText::new(format!(
                                "{ratio:.2}:1 {} {floor}",
                                if pass { "\u{2265}" } else { "BELOW" }
                            ))
                            .color(if pass { p.ok } else { p.err })
                            .font(egui::FontId::monospace(11.0)),
                        );
                    }
                    ui.end_row();
                }
            });
    });
}

fn type_scale_section(ui: &mut egui::Ui, p: &theme::Palette) {
    let dark_mode = ui.visuals().dark_mode;
    section(ui, p, dark_mode, "Type scale", |ui| {
        for (name, size) in [
            ("SIZE_TITLE", theme::SIZE_TITLE),
            ("SIZE_SECTION", theme::SIZE_SECTION),
            ("SIZE_BODY", theme::SIZE_BODY),
            ("SIZE_CAPTION", theme::SIZE_CAPTION),
        ] {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("The quick brown fox — {size}pt"))
                        .size(size)
                        .color(p.text),
                );
                mono_caption(ui, p, name);
            });
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Semibold carries emphasis; there is no medium.")
                    .font(theme::semibold(theme::SIZE_BODY))
                    .color(p.text),
            );
            mono_caption(ui, p, "FAMILY_SEMIBOLD");
        });
    });
}

fn spacing_section(ui: &mut egui::Ui, p: &theme::Palette) {
    let dark_mode = ui.visuals().dark_mode;
    section(ui, p, dark_mode, "Spacing", |ui| {
        caption(ui, p, "theme::Space — the app's shipped scale");
        for space in [
            theme::Space::S0,
            theme::Space::S1,
            theme::Space::S2,
            theme::Space::S3,
            theme::Space::S4,
            theme::Space::S6,
            theme::Space::S8,
            theme::Space::S12,
            theme::Space::S16,
            theme::Space::S20,
            theme::Space::S24,
            theme::Space::S32,
        ] {
            ui.horizontal(|ui| {
                let (rect, _) = ui
                    .allocate_exact_size(egui::vec2(space.px() * 4.0, 10.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, p.accent);
                mono_caption(ui, p, &format!("{}px", space.px()));
            });
        }
        ui.add_space(theme::Space::S8.px());
        caption(
            ui,
            p,
            "tokens::Spacing — the density-preset scale (cosmic-theme shape)",
        );
        egui::Grid::new("gallery-density")
            .num_columns(4)
            .spacing(egui::vec2(theme::Space::S12.px(), theme::Space::S2.px()))
            .show(ui, |ui| {
                mono_caption(ui, p, "step");
                mono_caption(ui, p, "compact");
                mono_caption(ui, p, "standard");
                mono_caption(ui, p, "spacious");
                ui.end_row();
                let compact = tokens::Spacing::for_density(Density::Compact);
                let standard = tokens::Spacing::for_density(Density::Standard);
                let spacious = tokens::Spacing::for_density(Density::Spacious);
                let rows: [(&str, u16, u16, u16); 10] = [
                    (
                        "none",
                        compact.space_none,
                        standard.space_none,
                        spacious.space_none,
                    ),
                    (
                        "xxxs",
                        compact.space_xxxs,
                        standard.space_xxxs,
                        spacious.space_xxxs,
                    ),
                    (
                        "xxs",
                        compact.space_xxs,
                        standard.space_xxs,
                        spacious.space_xxs,
                    ),
                    ("xs", compact.space_xs, standard.space_xs, spacious.space_xs),
                    ("s", compact.space_s, standard.space_s, spacious.space_s),
                    ("m", compact.space_m, standard.space_m, spacious.space_m),
                    ("l", compact.space_l, standard.space_l, spacious.space_l),
                    ("xl", compact.space_xl, standard.space_xl, spacious.space_xl),
                    (
                        "xxl",
                        compact.space_xxl,
                        standard.space_xxl,
                        spacious.space_xxl,
                    ),
                    (
                        "xxxl",
                        compact.space_xxxl,
                        standard.space_xxxl,
                        spacious.space_xxxl,
                    ),
                ];
                for (name, c, s, sp) in rows {
                    mono_caption(ui, p, name);
                    mono_caption(ui, p, &c.to_string());
                    mono_caption(ui, p, &s.to_string());
                    mono_caption(ui, p, &sp.to_string());
                    ui.end_row();
                }
            });
    });
}

fn shape_and_elevation_section(ui: &mut egui::Ui, p: &theme::Palette, dark_mode: bool) {
    section(ui, p, dark_mode, "Shape and elevation", |ui| {
        caption(ui, p, "theme radii — control 5 / container 10 / popover 12");
        ui.horizontal(|ui| {
            for radius in [
                theme::RADIUS_CONTROL,
                theme::RADIUS_CONTAINER,
                theme::RADIUS_POPOVER,
            ] {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(56.0, 32.0), egui::Sense::hover());
                ui.painter().rect(
                    rect,
                    CornerRadius::same(radius),
                    p.widget,
                    Stroke::new(1.0, p.line),
                    StrokeKind::Inside,
                );
            }
        });
        ui.add_space(theme::Space::S8.px());
        caption(
            ui,
            p,
            "tokens::CornerRadii presets — Round / SlightlyRound / Square (radius_s step)",
        );
        ui.horizontal(|ui| {
            for roundness in [
                Roundness::Round,
                Roundness::SlightlyRound,
                Roundness::Square,
            ] {
                let radii = tokens::CornerRadii::for_roundness(roundness);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(56.0, 32.0), egui::Sense::hover());
                ui.painter().rect(
                    rect,
                    CornerRadius::same(radii.radius_s[0] as u8),
                    p.widget,
                    Stroke::new(1.0, p.line),
                    StrokeKind::Inside,
                );
            }
        });
        ui.add_space(theme::Space::S8.px());
        caption(
            ui,
            p,
            "elevation — the section surface above carries elevated_shadows()[0]; modals add three more layers",
        );
    });
}

fn motion_section(ui: &mut egui::Ui, p: &theme::Palette) {
    let dark_mode = ui.visuals().dark_mode;
    section(ui, p, dark_mode, "Motion", |ui| {
        for (name, value, used_by) in [
            (
                "DURATION_HOVER",
                theme::DURATION_HOVER,
                "row washes, button fades, chevron reveals",
            ),
            ("DURATION_VALUE", theme::DURATION_VALUE, "animated counts"),
            (
                "DURATION_ENTRANCE",
                theme::DURATION_ENTRANCE,
                "modal entrances, footer icon cross-fade",
            ),
            (
                "DURATION_PAYOFF",
                theme::DURATION_PAYOFF,
                "the all-clear check materializing",
            ),
        ] {
            ui.horizontal(|ui| {
                mono_caption(ui, p, &format!("{name} = {value}s"));
                caption(ui, p, used_by);
            });
        }
    });
}

fn components_section(ui: &mut egui::Ui, p: &theme::Palette, state: &mut GalleryState) {
    let dark_mode = ui.visuals().dark_mode;
    section(ui, p, dark_mode, "Components", |ui| {
        caption(ui, p, "buttons — rest / accent-filled / disabled / ghost");
        ui.horizontal(|ui| {
            ax_button(ui, "Rest", "Gallery rest button", None, true);
            ax_button(ui, "Filled", "Gallery filled button", Some(p.accent), true);
            ax_button(ui, "Disabled", "Gallery disabled button", None, false);
            ax_ghost_button(ui, "Ghost action", "Gallery ghost button", p.info);
        });
        ui.add_space(theme::Space::S8.px());

        caption(ui, p, "toggle — on / off");
        ui.horizontal(|ui| {
            toggle(ui, &mut state.demo_toggle_on, p, "Gallery toggle on");
            toggle(ui, &mut state.demo_toggle_off, p, "Gallery toggle off");
        });
        ui.add_space(theme::Space::S8.px());

        caption(ui, p, "chips — one tinted pill per status role");
        ui.horizontal(|ui| {
            chip(ui, "ok", p.ok);
            chip(ui, "warn", p.warn);
            chip(ui, "error", p.err);
            chip(ui, "info", p.info);
            chip(ui, "accent", p.accent);
        });
        ui.add_space(theme::Space::S8.px());

        caption(ui, p, "status dots — ok / warn / err / missing / disabled");
        ui.horizontal(|ui| {
            for dot in [Dot::Ok, Dot::Warn, Dot::Err, Dot::Missing, Dot::Disabled] {
                status_dot(ui, dot, p);
            }
        });
        ui.add_space(theme::Space::S8.px());

        caption(ui, p, "spinner — always muted beside its caption");
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(14.0).color(p.muted));
            caption(ui, p, "working\u{2026}");
        });
        ui.add_space(theme::Space::S8.px());

        caption(
            ui,
            p,
            "focus stroke — reserved while unfocused, accent when focused",
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::Space::S12.px();
            for focused in [false, true] {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(72.0, 26.0), egui::Sense::hover());
                ui.painter().rect(
                    rect,
                    CornerRadius::same(theme::RADIUS_CONTROL),
                    p.widget,
                    theme::focus_stroke(focused),
                    StrokeKind::Inside,
                );
                theme::focus_ring(ui, rect, theme::RADIUS_CONTROL, focused);
            }
        });
        ui.add_space(theme::Space::S8.px());

        caption(ui, p, "hairline — the row separator");
        crate::app::row_hairline(ui, p);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::harness_themed;
    use egui_kittest::kittest::Queryable;

    fn paint(dark: bool) -> impl FnMut(&mut egui::Ui) + 'static {
        let mut state = GalleryState::default();
        move |ui| {
            let p = theme::palette(dark);
            // The gallery renders real text roles; the harness's blanket
            // override would flatten the muted/faint hierarchy the gallery
            // exists to show.
            ui.visuals_mut().override_text_color = None;
            gallery(ui, &p, dark, &mut state);
        }
    }

    /// The gallery renders every section, both appearances — the goldens ARE
    /// the design-iteration surface, reviewed like any other screen.
    #[test]
    fn gallery_renders_every_token_and_component() {
        let mut h = harness_themed(egui::vec2(860.0, 3560.0), true, paint(true));
        h.snapshot("gallery");
    }

    #[test]
    fn gallery_renders_light() {
        let mut h = harness_themed(egui::vec2(860.0, 3560.0), false, paint(false));
        h.snapshot("gallery_light");
    }

    /// The section inventory is the contract: a section silently dropped
    /// from the gallery is a regression this catches by label, not by pixel.
    #[test]
    fn gallery_carries_every_section() {
        let h = harness_themed(egui::vec2(860.0, 3560.0), true, paint(true));
        for label in [
            "Design gallery",
            "COLOR ROLES",
            "CONTRAST AUDIT",
            "TYPE SCALE",
            "SPACING",
            "SHAPE AND ELEVATION",
            "MOTION",
            "COMPONENTS",
        ] {
            assert!(
                h.query_by_label(label).is_some(),
                "gallery section missing: {label}"
            );
        }
    }

    /// Every contrast-audit row the gallery shows must actually pass its
    /// floor — the gallery showing a BELOW row in a shipped build means a
    /// token regressed, and this test is that fact expressed as a gate.
    #[test]
    fn the_contrast_audit_the_gallery_displays_holds() {
        // The SAME const the display renders — what the gallery shows and
        // what this test gates are one table by construction.
        for (_label, fg, bg, floor_light, floor_dark) in CONTRAST_PAIRS {
            for (appearance, floor) in [
                (Appearance::Light, floor_light),
                (Appearance::Dark, floor_dark),
            ] {
                let ratio = tokens::contrast_ratio(
                    tokens::resolve(fg, appearance),
                    tokens::resolve(bg, appearance),
                );
                assert!(
                    ratio >= floor,
                    "{fg:?} on {bg:?} ({appearance:?}) = {ratio:.2}:1, floor {floor}"
                );
            }
        }
    }

    /// The role table covers the whole enum — a role added to the tokens
    /// without a gallery row would be invisible to design review.
    #[test]
    fn the_role_table_is_exhaustive() {
        // Compile-time half: role_name's match is exhaustive, so a new
        // enum variant breaks the build until it is named — the human adding
        // it is thereby pointed at this file, but nothing MECHANICALLY forces
        // the new variant into ROLES (updating both is the documented,
        // build-error-guided convention, not an airtight gate). Runtime
        // half: ROLES carries no duplicates and every entry resolves.
        let mut seen: Vec<&str> = ROLES.iter().map(|r| role_name(*r)).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(
            seen.len(),
            ROLES.len(),
            "duplicate role in the gallery table"
        );
        for role in ROLES {
            let _ = tokens::resolve(role, Appearance::Light);
            let _ = tokens::resolve(role, Appearance::Dark);
        }
    }
}
