//! Text helpers the sidebar and the capability page share. The sidebar view
//! itself lives in `sidebar.rs` and renders `ade_core::gui::views::SidebarModel`;
//! what remains here is the label elision every list surface uses and the
//! freshness phrasing the stats strip must never disagree with.

use egui::FontId;
use std::time::Duration;

/// Lay the label out at body size; if it does not fit, trim characters and
/// append an ellipsis until it does.
pub(crate) fn elided(
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
}
