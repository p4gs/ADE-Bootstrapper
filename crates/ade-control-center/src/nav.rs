//! The sidebar — task-framed navigation with count badges and a freshness
//! footer. Text labels only: no icons, no emoji, which sidesteps the
//! drawn-icon quality problem entirely.
//!
//! Pure view: it draws, and reports which scope the owner chose. Badges are
//! muted monospaced numerals and appear ONLY where something needs attention —
//! healthy is silent, so sixteen quiet rows spend no pigment saying "nothing
//! is wrong".

use crate::overview::animated_count;
use crate::theme;
use ade_core::gui::inventory::{CapabilityStatus, CAPABILITY_GROUPS};
use ade_core::gui::state::{SCOPE_ACTIVITY, SCOPE_OVERVIEW, SCOPE_PROJECTS};
use ade_core::gui::verdict::HealthVerdict;
use egui::emath::GuiRounding;
use egui::{FontId, RichText, Sense, StrokeKind};
use std::time::Duration;

/// What the owner did in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NavEvent {
    /// Switch the content area to this scope (a group id or a reserved scope).
    Select(String),
}

/// Everything the sidebar needs to draw itself.
pub(crate) struct NavState<'a> {
    /// The scope currently shown in the content area.
    pub scope: &'a str,
    /// The shared verdict — the single source of the attention badges.
    pub verdict: &'a HealthVerdict,
    /// Detection results, for the disabled-group treatment.
    pub capabilities: &'a [CapabilityStatus],
    /// Jobs currently running — the Activity badge.
    pub running_jobs: usize,
    /// A detection pass is running right now.
    pub refreshing: bool,
    /// Time since the last detection pass finished; `None` before the first.
    pub checked_ago: Option<Duration>,
}

pub(crate) fn sidebar(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    state: &NavState<'_>,
) -> Option<NavEvent> {
    let mut event = None;
    let mut select = |scope: &str| event = Some(NavEvent::Select(scope.to_string()));

    // The footer is laid bottom-up FIRST so it owns the bottom edge whatever
    // the list above it does; the nav list then fills what remains.
    egui::Panel::bottom("nav-footer")
        .frame(egui::Frame::new().inner_margin(theme::margin(theme::Space::S8, theme::Space::S8)))
        .show_separator_line(false)
        .show(ui, |ui| {
            freshness_footer(ui, p, state);
        });

    // With the titlebar melted into the window (fullsize content view), the
    // traffic lights float over the sidebar's top-left — the list starts
    // below their zone.
    ui.add_space(theme::TRAFFIC_LIGHT_INSET);

    // Scrolled, not laid out straight into `ui`: the footer above already
    // shrank `ui`'s cursor bound to stop short of its own top edge, but nothing
    // stops row content from being ALLOCATED past that bound — egui does not
    // refuse an over-budget allocation on its own, it just overflows. On a
    // short window (or with enough capability groups) the list's true height
    // exceeds what is left above the footer, and the last rows render
    // UNDER/behind it instead of being clipped or scrolled. A `ScrollArea`
    // clips to its allocated rect, so the worst case becomes "scrollable",
    // never "overlapping the footer".
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let attention = state.verdict.action_items().count();
            if nav_row(
                ui,
                p,
                "Overview",
                badge(attention),
                state.scope == SCOPE_OVERVIEW,
                false,
                "Overview",
            )
            .clicked()
            {
                select(SCOPE_OVERVIEW);
            }

            ui.add_space(theme::Space::S8.px());
            section_header(ui, p, "Capabilities");
            for group in CAPABILITY_GROUPS.iter() {
                let group_attention = state
                    .verdict
                    .attention
                    .iter()
                    .filter(|item| item.rank.needs_action() && item.group_id == group.id)
                    .count();
                // A group is "off" only when the owner has switched off every
                // provider it has — an explicit choice, rendered as disabled
                // text rather than a vanished row.
                let providers: Vec<&CapabilityStatus> = state
                    .capabilities
                    .iter()
                    .filter(|cap| cap.capability == group.id)
                    .collect();
                let off = !providers.is_empty() && providers.iter().all(|cap| !cap.enabled);
                if nav_row(
                    ui,
                    p,
                    group.name,
                    badge(group_attention),
                    state.scope == group.id,
                    off,
                    group.name,
                )
                .clicked()
                {
                    select(group.id);
                }
            }

            ui.add_space(theme::Space::S8.px());
            if nav_row(
                ui,
                p,
                "Projects",
                None,
                state.scope == SCOPE_PROJECTS,
                false,
                "Projects",
            )
            .clicked()
            {
                select(SCOPE_PROJECTS);
            }
            if nav_row(
                ui,
                p,
                "Activity",
                badge(state.running_jobs),
                state.scope == SCOPE_ACTIVITY,
                false,
                "Activity",
            )
            .clicked()
            {
                select(SCOPE_ACTIVITY);
            }
        });

    event
}

/// Healthy is silent: zero renders no badge at all.
fn badge(count: usize) -> Option<usize> {
    (count > 0).then_some(count)
}

/// The 11pt semibold muted section eyebrow (HIG sidebar section header).
fn section_header(ui: &mut egui::Ui, p: &theme::Palette, text: &str) {
    ui.add_space(theme::Space::S4.px());
    ui.label(
        RichText::new(text.to_uppercase())
            .color(p.muted)
            .font(theme::semibold(theme::SIZE_CAPTION))
            .extra_letter_spacing(0.8),
    );
    ui.add_space(theme::Space::S2.px());
}

/// One 28pt nav row. Selection is a background wash, hover and press are
/// lighter washes, and keyboard focus is a border whose box is reserved with a
/// transparent stroke — so focus arriving never shifts a pixel.
fn nav_row(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    label: &str,
    badge: Option<usize>,
    selected: bool,
    off: bool,
    ax_label: &str,
) -> egui::Response {
    let size = egui::vec2(ui.available_width(), theme::NAV_ROW_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let ax = ax_label.to_string();
    response.widget_info(move || {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            selected,
            ax.clone(),
        )
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    // Hover fades in and out (~100ms); selection and press are INSTANT —
    // navigation feedback must never lag the click.
    let hover = ui.ctx().animate_bool_with_time(
        response.id.with("hover-wash"),
        response.hovered() && !selected,
        theme::DURATION_HOVER,
    );
    let wash = if selected {
        // The accent-hued wash, not the neutral one — the macOS sidebar
        // selection idiom (ISC-310 cycle 1). List rows elsewhere keep the
        // neutral `selected` so status colors aren't fighting a blue field.
        Some(p.selected_accent)
    } else if response.is_pointer_button_down_on() {
        Some(p.ghost_active)
    } else if hover > 0.0 {
        Some(p.row_hover.gamma_multiply(hover))
    } else {
        None
    };
    let radius = egui::CornerRadius::same(theme::RADIUS_CONTROL);
    if let Some(wash) = wash {
        ui.painter()
            .rect_filled(rect.round_to_pixels(ui.pixels_per_point()), radius, wash);
    }
    ui.painter().rect_stroke(
        rect.round_to_pixels(ui.pixels_per_point()),
        radius,
        theme::focus_stroke(response.has_focus()),
        StrokeKind::Inside,
    );
    // ISC-309: the crisp line alone reads as a border change; real macOS
    // focus is an outer glow. Both layers together are the native look.
    theme::focus_ring(ui, rect, theme::RADIUS_CONTROL, response.has_focus());
    let text_color = if off { p.disabled_text } else { p.text };
    let pad = theme::Space::S8.px();
    let mut badge_width = 0.0;
    if let Some(count) = badge {
        // The same pill used for issue counts (row.rs) and job status
        // (activity.rs) — a bare numeral read as unstyled chrome and never
        // announced a change; the count now also animates through the same
        // interpolator the all-clear payoff and the bulk-preview footer use.
        let shown = animated_count(ui, &format!("nav-badge-{ax_label}"), count);
        // A true circle, painted directly (ISC-310 cycle 1 — the owner's
        // verbatim nit: "It should be a circle and the number inside should
        // be properly horizontally and vertically centered"). The generic
        // `chip` frame is a lozenge whose text baseline floats with the
        // frame margins; here the galley is measured first and the disc
        // derived from it, then the galley is placed at the disc's exact
        // center — centering is geometric, not layout-emergent. Two-plus
        // digits widen the circle into a capsule; the height never changes.
        let galley = ui.painter().layout_no_wrap(
            shown.to_string(),
            theme::medium(theme::SIZE_CAPTION),
            p.warn,
        );
        let diameter: f32 = 17.0;
        let width = diameter.max(galley.size().x + theme::Space::S8.px());
        let center = egui::pos2(rect.right() - pad - width * 0.5, rect.center().y);
        let badge_rect = egui::Rect::from_center_size(center, egui::vec2(width, diameter))
            .round_to_pixels(ui.pixels_per_point());
        ui.painter().rect_filled(
            badge_rect,
            egui::CornerRadius::same((diameter * 0.5) as u8),
            p.tint_warn,
        );
        ui.painter()
            .galley(badge_rect.center() - galley.size() * 0.5, galley, p.warn);
        badge_width = width + pad;
    }
    // The label elides into whatever the badge leaves — a 220pt sidebar and a
    // long group name must never collide or clip mid-glyph.
    // The selected row's label goes semibold — the macOS sidebar pairing
    // with the accent wash; weight says "you are here" without more color.
    let font = if selected {
        theme::semibold(theme::SIZE_BODY)
    } else {
        FontId::proportional(theme::SIZE_BODY)
    };
    let galley = elided(
        ui,
        label,
        text_color,
        font,
        rect.width() - 2.0 * pad - badge_width,
    );
    let pos = egui::pos2(rect.left() + pad, rect.center().y - galley.size().y * 0.5);
    ui.painter().galley(pos, galley, text_color);
    response
}

/// Lay the label out at body size; if it does not fit, trim characters and
/// append an ellipsis until it does.
fn elided(
    ui: &egui::Ui,
    label: &str,
    color: egui::Color32,
    font: FontId,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let full = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), color);
    if full.size().x <= max_width {
        return full;
    }
    let mut keep = label.to_string();
    loop {
        keep.pop();
        if keep.is_empty() {
            return ui
                .painter()
                .layout_no_wrap("…".to_string(), font.clone(), color);
        }
        let candidate = format!("{}…", keep.trim_end());
        let galley = ui.painter().layout_no_wrap(candidate, font.clone(), color);
        if galley.size().x <= max_width {
            return galley;
        }
    }
}

/// The sync-status line at the sidebar's bottom (HIG: that is where it lives).
/// A fixed icon slot is reserved unconditionally — a spinner while a
/// detection pass runs, a static check when idle — so the line never shows
/// blank chrome, and the caption checks `refreshing` FIRST so a stale
/// "Checked Ns ago" can never sit next to a spinner claiming otherwise.
fn freshness_footer(ui: &mut egui::Ui, p: &theme::Palette, state: &NavState<'_>) {
    ui.horizontal(|ui| {
        let side = crate::icons::IconSize::S10.px();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), Sense::hover());
        if state.refreshing {
            egui::Spinner::new()
                .size(theme::SIZE_CAPTION)
                .color(p.muted)
                .paint_at(ui, rect);
        } else {
            crate::icons::check(ui, rect, p.muted);
        }
        let text = if state.refreshing {
            "Checking…".to_string()
        } else {
            match state.checked_ago {
                Some(ago) => freshness_caption(ago),
                None => "Checking…".to_string(),
            }
        };
        ui.label(RichText::new(text).color(p.muted).size(theme::SIZE_CAPTION));
    });
}

/// "Checked 5s ago" — coarse on purpose; the poll repaints often enough that
/// the words never go stale by more than one interval. `pub(crate)`: the
/// capability page's stats strip (Phase G) shows the same freshness fact and
/// must never disagree with the sidebar footer about how to phrase it.
pub(crate) fn freshness_caption(ago: Duration) -> String {
    let secs = ago.as_secs();
    if secs < 5 {
        "Checked just now".to_string()
    } else if secs < 60 {
        format!("Checked {secs}s ago")
    } else if secs < 3600 {
        format!("Checked {}m ago", secs / 60)
    } else {
        format!("Checked {}h ago", secs / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{cap, harness_themed};
    use ade_core::gui::inventory::CAPABILITIES;
    use ade_core::gui::verdict::build_verdict;
    use egui_kittest::kittest::Queryable;

    fn mixed_capabilities() -> Vec<CapabilityStatus> {
        let mut caps: Vec<CapabilityStatus> = CAPABILITIES.iter().map(|def| cap(def.id)).collect();
        // A genuine gap (Execution Sandboxing badge = 1)…
        let hole = caps.iter_mut().find(|c| c.id == "nono").unwrap();
        hole.installed = false;
        hole.version = None;
        // …and a group the owner switched off entirely.
        for c in caps
            .iter_mut()
            .filter(|c| c.capability == "token-efficiency")
        {
            c.enabled = false;
        }
        caps
    }

    fn paint_sidebar(
        scope: &'static str,
        dark: bool,
        caps: Vec<CapabilityStatus>,
        checked_ago: Option<Duration>,
        refreshing: bool,
        sink: std::sync::Arc<std::sync::Mutex<Vec<NavEvent>>>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        move |ui| {
            let p = crate::theme::palette(dark);
            let verdict = build_verdict(&caps);
            let state = NavState {
                scope,
                verdict: &verdict,
                capabilities: &caps,
                running_jobs: 2,
                refreshing,
                checked_ago,
            };
            if let Some(event) = sidebar(ui, &p, &state) {
                sink.lock().expect("sink").push(event);
            }
        }
    }

    /// The sidebar at its real 220pt width: Overview badge (one gap), a quiet
    /// healthy majority, a disabled group in disabled text, the Activity
    /// running count, and the freshness footer.
    #[test]
    fn snapshot_the_sidebar_at_its_real_width() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(
            egui::vec2(theme::SIDEBAR_WIDTH, 600.0),
            true,
            paint_sidebar(
                "sandboxing",
                true,
                mixed_capabilities(),
                Some(Duration::from_secs(125)),
                false,
                sink,
            ),
        );
        h.snapshot("nav_sidebar");
    }

    #[test]
    fn snapshot_the_sidebar_light_while_refreshing() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(
            egui::vec2(theme::SIDEBAR_WIDTH, 600.0),
            false,
            paint_sidebar(
                ade_core::gui::state::SCOPE_OVERVIEW,
                false,
                mixed_capabilities(),
                Some(Duration::from_secs(7)),
                true,
                sink,
            ),
        );
        h.snapshot("nav_sidebar_light");
    }

    /// Clicking a row reports the scope and changes nothing itself; every row
    /// carries a unique AccessKit label so the Interceptor bridge can drive it.
    #[test]
    fn a_nav_row_reports_the_chosen_scope() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<NavEvent>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(theme::SIDEBAR_WIDTH, 600.0),
            true,
            paint_sidebar(
                ade_core::gui::state::SCOPE_OVERVIEW,
                true,
                mixed_capabilities(),
                Some(Duration::from_secs(10)),
                false,
                sink,
            ),
        );
        for label in ["Overview", "Secret Scanning", "Projects", "Activity"] {
            assert!(h.query_by_label(label).is_some(), "missing nav row {label}");
        }
        h.get_by_label("Projects").click();
        h.run_steps(2);
        h.get_by_label("Secret Scanning").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![
                NavEvent::Select("projects".to_string()),
                NavEvent::Select("secret-scanning".to_string()),
            ]
        );
    }

    #[test]
    fn freshness_captions_stay_coarse_and_honest() {
        assert_eq!(
            freshness_caption(Duration::from_secs(0)),
            "Checked just now"
        );
        assert_eq!(
            freshness_caption(Duration::from_secs(4)),
            "Checked just now"
        );
        assert_eq!(freshness_caption(Duration::from_secs(5)), "Checked 5s ago");
        assert_eq!(
            freshness_caption(Duration::from_secs(59)),
            "Checked 59s ago"
        );
        assert_eq!(
            freshness_caption(Duration::from_secs(125)),
            "Checked 2m ago"
        );
        assert_eq!(
            freshness_caption(Duration::from_secs(2 * 3600 + 60)),
            "Checked 2h ago"
        );
    }

    #[test]
    fn badges_are_absent_when_nothing_needs_attention() {
        assert_eq!(badge(0), None, "healthy is silent");
        assert_eq!(badge(3), Some(3));
    }
}
