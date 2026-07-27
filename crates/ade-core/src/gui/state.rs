//! Machine-level GUI state — `$ADE_HOME/gui.json` (default `~/.ade/gui.json`).
//!
//! Lives OUTSIDE any project's `.ade/` tree on purpose: project trees are
//! lockfile-enumerated (ISC-146), so machine-scoped state must never be
//! planted there. Machine-level "disabled" is a GUI/menubar preference; the
//! real policy lever remains each project's `ade.json` module toggles.

use crate::fsutil::{read_if_exists, stable_stringify, write_ensured};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const GUI_STATE_SCHEMA_VERSION: u64 = 1;
pub const GUI_STATE_FILE: &str = "gui.json";
pub const JOBS_FILE: &str = "jobs.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiState {
    /// Capability ids disabled machine-wide (GUI preference, not project policy).
    pub disabled: BTreeSet<String>,
    /// Absolute paths of registered ADE-bootstrapped projects (insertion order).
    pub projects: Vec<String>,
    /// Control Center view preference: group tools by capability (ISC-206).
    pub group_by_capability: bool,
    /// Navigation section the window was last on — a capability-group id, or
    /// one of the reserved scopes below. Restored on launch so the app opens
    /// where the owner left it.
    pub scope: String,
    /// Capability id last selected in the content list, so the inspector can
    /// come back the way it was left.
    pub selection: Option<String>,
}

/// The reserved (non-capability-group) navigation scopes.
pub const SCOPE_OVERVIEW: &str = "overview";
pub const SCOPE_PROJECTS: &str = "projects";
pub const SCOPE_ACTIVITY: &str = "activity";

/// A scope is valid if it names a real capability group or a reserved section.
/// An unknown scope — a stale id after a capability is retired, or a hand-edited
/// file — falls back to the Overview rather than showing an empty window.
pub fn resolve_scope(raw: &str) -> String {
    let known = raw == SCOPE_OVERVIEW
        || raw == SCOPE_PROJECTS
        || raw == SCOPE_ACTIVITY
        || crate::gui::inventory::get_group(raw).is_some();
    if known {
        raw.to_string()
    } else {
        SCOPE_OVERVIEW.to_string()
    }
}

impl Default for GuiState {
    fn default() -> Self {
        GuiState {
            disabled: BTreeSet::new(),
            projects: Vec::new(),
            group_by_capability: false,
            scope: SCOPE_OVERVIEW.to_string(),
            selection: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GuiStateLoad {
    pub state: GuiState,
    /// Set when the on-disk file was corrupt/invalid and defaults were used.
    pub warning: Option<String>,
}

/// Resolve the ADE home directory ($ADE_HOME override, default ~/.ade).
pub fn ade_home(env_home: Option<&str>, ade_home_override: Option<&str>) -> PathBuf {
    if let Some(explicit) = ade_home_override {
        if !explicit.is_empty() {
            return PathBuf::from(explicit);
        }
    }
    PathBuf::from(env_home.unwrap_or("/tmp")).join(".ade")
}

/// Resolve from the live process environment.
pub fn ade_home_from_env() -> PathBuf {
    let home = std::env::var("HOME").ok();
    let overridden = std::env::var("ADE_HOME").ok();
    ade_home(home.as_deref(), overridden.as_deref())
}

fn string_array(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    let array = value?.as_array()?;
    if !array.iter().all(|entry| entry.is_string()) {
        return Some(Vec::new());
    }
    Some(
        array
            .iter()
            .filter_map(|entry| entry.as_str().map(String::from))
            .collect(),
    )
}

/// Load gui.json; corrupt or invalid content degrades to defaults with a
/// warning — never a crash (ISC-172).
pub fn load_gui_state(home: &Path) -> GuiStateLoad {
    let path = home.join(GUI_STATE_FILE);
    let Some(text) = read_if_exists(&path) else {
        return GuiStateLoad {
            state: GuiState::default(),
            warning: None,
        };
    };
    let raw: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            return GuiStateLoad {
                state: GuiState::default(),
                warning: Some(format!(
                    "{GUI_STATE_FILE} is not valid JSON — using defaults (file left untouched until next change)"
                )),
            };
        }
    };
    let Some(obj) = raw.as_object() else {
        return GuiStateLoad {
            state: GuiState::default(),
            warning: Some(format!(
                "{GUI_STATE_FILE} is not an object — using defaults"
            )),
        };
    };
    match obj.get("schemaVersion").and_then(|v| v.as_u64()) {
        Some(version) if version <= GUI_STATE_SCHEMA_VERSION => {}
        _ => {
            return GuiStateLoad {
                state: GuiState::default(),
                warning: Some(format!(
                    "{GUI_STATE_FILE} schemaVersion is unsupported — using defaults"
                )),
            };
        }
    }
    let disabled = string_array(obj.get("disabled")).unwrap_or_default();
    let projects = string_array(obj.get("projects")).unwrap_or_default();
    let group_by_capability = obj
        .get("groupByCapability")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let scope = resolve_scope(
        obj.get("scope")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    );
    let selection = obj
        .get("selection")
        .and_then(|value| value.as_str())
        .filter(|id| crate::gui::inventory::get_capability(id).is_some())
        .map(String::from);
    GuiStateLoad {
        state: GuiState {
            disabled: disabled.into_iter().collect(),
            projects,
            group_by_capability,
            scope,
            selection,
        },
        warning: None,
    }
}

/// Persist gui.json deterministically (sorted keys, sorted disabled list).
pub fn save_gui_state(home: &Path, state: &GuiState) -> std::io::Result<()> {
    let mut projects_dedup: Vec<String> = Vec::new();
    for project in &state.projects {
        if !projects_dedup.contains(project) {
            projects_dedup.push(project.clone());
        }
    }
    let value = serde_json::json!({
        "schemaVersion": GUI_STATE_SCHEMA_VERSION,
        "disabled": state.disabled.iter().collect::<Vec<_>>(),
        "projects": projects_dedup,
        "groupByCapability": state.group_by_capability,
        "scope": resolve_scope(&state.scope),
        "selection": state.selection,
    });
    write_ensured(&home.join(GUI_STATE_FILE), &stable_stringify(&value))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::make_temp_dir;

    #[test]
    fn ade_home_resolution_rules() {
        assert_eq!(
            ade_home(Some("/Users/x"), None),
            PathBuf::from("/Users/x/.ade")
        );
        assert_eq!(
            ade_home(Some("/Users/x"), Some("/custom")),
            PathBuf::from("/custom")
        );
        assert_eq!(
            ade_home(Some("/Users/x"), Some("")),
            PathBuf::from("/Users/x/.ade")
        );
    }

    #[test]
    fn missing_file_defaults_without_warning() {
        let home = make_temp_dir("gui-state");
        let load = load_gui_state(&home);
        assert_eq!(load.state, GuiState::default());
        assert!(load.warning.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn save_load_round_trip_deterministic_sorted_deduped() {
        let home = make_temp_dir("gui-state-rt");
        let mut state = GuiState::default();
        state.disabled.insert("zeta".into());
        state.disabled.insert("alpha".into());
        state.projects = vec!["/a".into(), "/b".into(), "/a".into()];
        state.group_by_capability = true;
        save_gui_state(&home, &state).unwrap();
        let first = read_if_exists(&home.join(GUI_STATE_FILE)).unwrap();
        let load = load_gui_state(&home);
        assert_eq!(
            load.state.disabled.iter().cloned().collect::<Vec<_>>(),
            vec!["alpha".to_string(), "zeta".to_string()]
        );
        assert_eq!(
            load.state.projects,
            vec!["/a".to_string(), "/b".to_string()]
        );
        save_gui_state(&home, &load.state).unwrap();
        let second = read_if_exists(&home.join(GUI_STATE_FILE)).unwrap();
        assert_eq!(first, second);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn navigation_state_round_trips_and_rejects_ids_that_no_longer_exist() {
        let home = make_temp_dir("gui-state-nav");
        let mut state = GuiState::default();
        assert_eq!(state.scope, SCOPE_OVERVIEW, "defaults open on the Overview");
        assert!(state.selection.is_none());
        state.scope = "secret-scanning".into();
        state.selection = Some("trufflehog".into());
        save_gui_state(&home, &state).unwrap();
        let load = load_gui_state(&home);
        assert_eq!(load.state.scope, "secret-scanning");
        assert_eq!(load.state.selection.as_deref(), Some("trufflehog"));

        // A capability retired between releases must not strand the window on
        // an empty section or a row that cannot be drawn.
        std::fs::write(
            home.join(GUI_STATE_FILE),
            r#"{"schemaVersion":1,"scope":"telepathy","selection":"retired-tool"}"#,
        )
        .unwrap();
        let stale = load_gui_state(&home);
        assert!(stale.warning.is_none(), "a stale id is not a corrupt file");
        assert_eq!(stale.state.scope, SCOPE_OVERVIEW);
        assert!(stale.state.selection.is_none());

        for scope in [SCOPE_OVERVIEW, SCOPE_PROJECTS, SCOPE_ACTIVITY] {
            assert_eq!(resolve_scope(scope), scope);
        }
        assert_eq!(resolve_scope("coding-harness"), "coding-harness");
        assert_eq!(resolve_scope(""), SCOPE_OVERVIEW);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn corrupt_variants_degrade_with_warning_never_crash() {
        let home = make_temp_dir("gui-state-bad");
        std::fs::write(home.join(GUI_STATE_FILE), "{not json").unwrap();
        let bad_json = load_gui_state(&home);
        assert!(bad_json.warning.unwrap().contains("not valid JSON"));

        std::fs::write(home.join(GUI_STATE_FILE), "[1,2]").unwrap();
        let not_object = load_gui_state(&home);
        assert!(not_object.warning.unwrap().contains("not an object"));

        std::fs::write(
            home.join(GUI_STATE_FILE),
            r#"{"schemaVersion": 99, "disabled": [], "projects": []}"#,
        )
        .unwrap();
        let newer = load_gui_state(&home);
        assert!(newer.warning.unwrap().contains("schemaVersion"));

        std::fs::write(
            home.join(GUI_STATE_FILE),
            r#"{"schemaVersion": 1, "disabled": "nope", "projects": [1]}"#,
        )
        .unwrap();
        let malformed = load_gui_state(&home);
        assert!(malformed.warning.is_none());
        assert!(malformed.state.disabled.is_empty());
        assert!(malformed.state.projects.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }
}
