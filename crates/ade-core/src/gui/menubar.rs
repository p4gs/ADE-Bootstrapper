//! Menu-bar payload — core-formatted status for the tray (ISC-189).
//! The core formats EVERYTHING (glyphs, labels, counts) so adding a
//! capability never requires touching the tray binary (the Pulse lesson).
//!
//! The rollup itself comes from `gui::verdict`, which the Control Center also
//! uses. The tray reported "1 err" while the Control Center said "2 errors"
//! once already; a shared derivation is the fix that holds.

use crate::gui::inventory::CapabilityStatus;
use crate::gui::verdict::{build_verdict, HealthVerdict, Verdict};
use crate::types::{Finding, FindingLevel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateStatus {
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenubarCounts {
    pub ok: u32,
    pub warnings: u32,
    pub errors: u32,
    pub missing: u32,
    pub running_jobs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenubarItem {
    pub id: String,
    pub glyph: &'static str,
    pub label: String,
    pub level: FindingLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenubarPayload {
    pub status: AggregateStatus,
    pub label: String,
    pub counts: MenubarCounts,
    pub items: Vec<MenubarItem>,
}

fn worst_level(issues: &[Finding]) -> FindingLevel {
    issues
        .iter()
        .map(|issue| match issue.level {
            FindingLevel::Degraded => FindingLevel::Warn,
            other => other,
        })
        .max()
        .unwrap_or(FindingLevel::Ok)
}

fn rank(level: FindingLevel) -> u8 {
    match level {
        FindingLevel::Error => 0,
        FindingLevel::Warn | FindingLevel::Degraded => 1,
        FindingLevel::Info => 2,
        FindingLevel::Ok => 3,
    }
}

/// Build the tray payload from detected capabilities + the running-job count.
pub fn build_menubar_payload(
    capabilities: &[CapabilityStatus],
    running_jobs: u32,
) -> MenubarPayload {
    build_menubar_from_verdict(&build_verdict(capabilities), capabilities, running_jobs)
}

/// The tray's aggregate glyph, derived from the shared verdict.
pub fn aggregate_status(verdict: Verdict) -> AggregateStatus {
    match verdict {
        Verdict::NotWorking => AggregateStatus::Error,
        Verdict::NeedsAttention => AggregateStatus::Warn,
        Verdict::Sound | Verdict::Unknown => AggregateStatus::Ok,
    }
}

/// Format the payload against an already-computed verdict, so a caller that
/// needs both surfaces pays for the rollup once.
pub fn build_menubar_from_verdict(
    health: &HealthVerdict,
    capabilities: &[CapabilityStatus],
    running_jobs: u32,
) -> MenubarPayload {
    let enabled_total = capabilities.iter().filter(|cap| cap.enabled).count();
    let counts = health.counts;
    let status = aggregate_status(health.verdict);

    let mut items: Vec<MenubarItem> = capabilities
        .iter()
        .map(|cap| {
            let level = if cap.enabled {
                worst_level(&cap.issues)
            } else {
                FindingLevel::Info
            };
            let glyph = if !cap.enabled {
                "○"
            } else if !cap.installed {
                "·"
            } else {
                match level {
                    FindingLevel::Error => "✖",
                    FindingLevel::Warn | FindingLevel::Degraded => "⚠",
                    _ => "✓",
                }
            };
            let mut bits: Vec<String> = vec![cap.name.clone()];
            if let Some(version) = &cap.version {
                let stripped = version
                    .strip_prefix(&format!("{} ", cap.name.to_lowercase()))
                    .unwrap_or(version.as_str());
                bits.push(stripped.to_string());
            }
            if cap.running == Some(true) {
                bits.push("· running".to_string());
            }
            if !cap.enabled {
                bits.push("· disabled".to_string());
            } else if !cap.installed {
                bits.push("· not installed".to_string());
            }
            MenubarItem {
                id: cap.id.clone(),
                glyph,
                label: bits.join(" "),
                level,
            }
        })
        .collect();
    items.sort_by_key(|item| rank(item.level));

    MenubarPayload {
        status,
        label: format!(
            "{}/{enabled_total} healthy · {} warn · {} err",
            counts.ok, counts.warnings, counts.errors
        ),
        counts: MenubarCounts {
            ok: counts.ok,
            warnings: counts.warnings,
            errors: counts.errors,
            missing: counts.missing,
            running_jobs,
        },
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::inventory::{CapabilityKind, CapabilityStatus, LifecycleMethod};

    fn cap(id: &str, enabled: bool, installed: bool, issues: Vec<Finding>) -> CapabilityStatus {
        CapabilityStatus {
            id: id.to_string(),
            name: id.to_string(),
            kind: CapabilityKind::Tool,
            capability: "secret-scanning".to_string(),
            description: String::new(),
            method: LifecycleMethod::Brew,
            pkg: None,
            guidance: None,
            installed,
            path: None,
            version: if installed {
                Some(format!("{id} 1.0"))
            } else {
                None
            },
            reports_version: true,
            running: None,
            enabled,
            latest_version: None,
            update_available: false,
            issues,
        }
    }

    #[test]
    fn counts_status_and_ordering() {
        let caps = vec![
            cap("healthy", true, true, vec![]),
            cap("warned", true, true, vec![Finding::warn("w", "r")]),
            cap("broken", true, true, vec![Finding::error("e")]),
            cap(
                "absent",
                true,
                false,
                vec![Finding::warn("not installed", "install it")],
            ),
            cap("off", false, false, vec![Finding::info("disabled")]),
        ];
        let payload = build_menubar_payload(&caps, 2);
        assert_eq!(payload.status, AggregateStatus::Error);
        assert_eq!(payload.counts.errors, 1);
        assert_eq!(payload.counts.warnings, 2);
        assert_eq!(payload.counts.missing, 1);
        assert_eq!(payload.counts.ok, 2);
        assert_eq!(payload.counts.running_jobs, 2);
        assert_eq!(payload.label, "2/4 healthy · 2 warn · 1 err");
        // Errors sort first; disabled rows are info with ○.
        assert_eq!(payload.items[0].id, "broken");
        assert_eq!(payload.items[0].glyph, "✖");
        let off = payload.items.iter().find(|item| item.id == "off").unwrap();
        assert_eq!(off.glyph, "○");
        assert!(off.label.contains("· disabled"));
        let absent = payload
            .items
            .iter()
            .find(|item| item.id == "absent")
            .unwrap();
        assert_eq!(absent.glyph, "·");
        assert!(absent.label.contains("· not installed"));
    }

    #[test]
    fn all_green_is_ok_status() {
        let caps = vec![cap("a", true, true, vec![]), cap("b", true, true, vec![])];
        let payload = build_menubar_payload(&caps, 0);
        assert_eq!(payload.status, AggregateStatus::Ok);
        assert_eq!(payload.label, "2/2 healthy · 0 warn · 0 err");
        assert!(payload.items.iter().all(|item| item.glyph == "✓"));
    }

    #[test]
    fn the_tray_and_the_control_center_read_from_one_rollup() {
        // This is the bug that shipped twice: the tray said "1 err" while the
        // Control Center said "2 errors", because each counted for itself.
        // Both now derive from build_verdict, and this asserts it.
        let caps = vec![
            cap("trufflehog", true, true, vec![]),
            cap(
                "gitleaks",
                true,
                true,
                vec![Finding::error("version probe failed")],
            ),
            cap(
                "nono",
                true,
                false,
                vec![Finding::warn("not installed", "see nono.sh")],
            ),
        ];
        let health = crate::gui::verdict::build_verdict(&caps);
        let payload = build_menubar_payload(&caps, 0);
        assert_eq!(payload.counts.errors, health.counts.errors);
        assert_eq!(payload.counts.warnings, health.counts.warnings);
        assert_eq!(payload.counts.ok, health.counts.ok);
        assert_eq!(payload.counts.missing, health.counts.missing);
        assert_eq!(payload.status, aggregate_status(health.verdict));
        assert_eq!(payload.status, AggregateStatus::Error);
        // Same verdict in, same payload out — the two entry points agree.
        assert_eq!(payload, build_menubar_from_verdict(&health, &caps, 0));
    }

    #[test]
    fn aggregate_status_maps_every_verdict() {
        assert_eq!(
            aggregate_status(Verdict::NotWorking),
            AggregateStatus::Error
        );
        assert_eq!(
            aggregate_status(Verdict::NeedsAttention),
            AggregateStatus::Warn
        );
        assert_eq!(aggregate_status(Verdict::Sound), AggregateStatus::Ok);
        // Pre-detection is not a failure state — the tray shows calm, and the
        // label carries the "checking" text.
        assert_eq!(aggregate_status(Verdict::Unknown), AggregateStatus::Ok);
    }

    #[test]
    fn version_prefix_is_stripped_from_labels() {
        let mut status = cap("rtk", true, true, vec![]);
        status.version = Some("rtk 0.43.0".to_string());
        let payload = build_menubar_payload(&[status], 0);
        assert_eq!(payload.items[0].label, "rtk 0.43.0");
    }
}
