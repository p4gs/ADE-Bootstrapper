//! Menu-bar payload — core-formatted status for the tray (ISC-189).
//! The core formats EVERYTHING (glyphs, labels, counts) so adding a
//! capability never requires touching the tray binary (the Pulse lesson).

use crate::gui::inventory::CapabilityStatus;
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
    let enabled: Vec<&CapabilityStatus> = capabilities.iter().filter(|cap| cap.enabled).collect();
    let errors = enabled
        .iter()
        .filter(|cap| {
            cap.issues
                .iter()
                .any(|issue| issue.level == FindingLevel::Error)
        })
        .count() as u32;
    let warnings = enabled
        .iter()
        .filter(|cap| {
            !cap.issues
                .iter()
                .any(|issue| issue.level == FindingLevel::Error)
                && cap.issues.iter().any(|issue| {
                    issue.level == FindingLevel::Warn || issue.level == FindingLevel::Degraded
                })
        })
        .count() as u32;
    let missing = enabled.iter().filter(|cap| !cap.installed).count() as u32;
    let ok = enabled
        .iter()
        .filter(|cap| cap.installed && worst_level(&cap.issues) != FindingLevel::Error)
        .count() as u32;
    let status = if errors > 0 {
        AggregateStatus::Error
    } else if warnings > 0 {
        AggregateStatus::Warn
    } else {
        AggregateStatus::Ok
    };

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
            "{ok}/{} healthy · {warnings} warn · {errors} err",
            enabled.len()
        ),
        counts: MenubarCounts {
            ok,
            warnings,
            errors,
            missing,
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
    fn version_prefix_is_stripped_from_labels() {
        let mut status = cap("rtk", true, true, vec![]);
        status.version = Some("rtk 0.43.0".to_string());
        let payload = build_menubar_payload(&[status], 0);
        assert_eq!(payload.items[0].label, "rtk 0.43.0");
    }
}
