//! Unified project configuration — `ade.json`. Port of `src/config.ts`.
//! Validation happens here ONCE (core-owned); modules never re-validate.
//! Error message strings are oracle-identical (CLI parity, ISC-164).

use crate::fsutil::{read_if_exists, stable_stringify};
use crate::types::{AdeConfig, ModuleConfig};
use crate::version::ADE_SCHEMA_VERSION;
use std::collections::BTreeMap;
use std::path::Path;

pub const CONFIG_FILE: &str = "ade.json";

pub type ConfigResult = Result<AdeConfig, String>;

/// Build the default (secure-by-default) config: every module enabled.
pub fn default_config(module_ids: &[&str], harnesses: &[String]) -> AdeConfig {
    let mut modules = BTreeMap::new();
    for id in module_ids {
        modules.insert(
            id.to_string(),
            ModuleConfig {
                enabled: true,
                options: serde_json::json!({}),
            },
        );
    }
    AdeConfig {
        schema_version: ADE_SCHEMA_VERSION,
        harnesses: harnesses.to_vec(),
        modules,
    }
}

/// Serialize a config deterministically (byte-identical to the oracle).
pub fn serialize_config(config: &AdeConfig) -> String {
    let value = serde_json::to_value(config).unwrap_or(serde_json::Value::Null);
    stable_stringify(&value)
}

/// Validate a parsed JSON value as AdeConfig against the known ids.
pub fn validate_config(
    raw: &serde_json::Value,
    known_module_ids: &[&str],
    known_harness_ids: &[&str],
    file_label: &str,
) -> ConfigResult {
    let Some(obj) = raw.as_object() else {
        return Err(format!("{file_label}: config must be a JSON object"));
    };
    let schema_version = match obj.get("schemaVersion").and_then(|v| v.as_u64()) {
        Some(version) if obj["schemaVersion"].is_u64() && version >= 1 => version,
        _ => {
            return Err(format!(
                "{file_label}: schemaVersion must be a positive integer"
            ))
        }
    };
    if obj["schemaVersion"]
        .as_f64()
        .map(|f| f.fract() != 0.0)
        .unwrap_or(false)
    {
        return Err(format!(
            "{file_label}: schemaVersion must be a positive integer"
        ));
    }
    if schema_version > ADE_SCHEMA_VERSION {
        return Err(format!(
            "{file_label}: schemaVersion {schema_version} is newer than this ade understands ({ADE_SCHEMA_VERSION}) — upgrade ade-bootstrapper to work with this repository"
        ));
    }
    let harnesses: Vec<String> = match obj.get("harnesses").and_then(|v| v.as_array()) {
        Some(entries) if entries.iter().all(|entry| entry.is_string()) => entries
            .iter()
            .map(|entry| entry.as_str().unwrap_or_default().to_string())
            .collect(),
        _ => {
            return Err(format!(
                "{file_label}: harnesses must be an array of strings"
            ))
        }
    };
    for harness in &harnesses {
        if !known_harness_ids.contains(&harness.as_str()) {
            return Err(format!(
                "{file_label}: unknown harness id \"{harness}\" (known: {})",
                known_harness_ids.join(", ")
            ));
        }
    }
    let Some(modules_obj) = obj.get("modules").and_then(|v| v.as_object()) else {
        return Err(format!("{file_label}: modules must be an object"));
    };
    let mut parsed_modules: BTreeMap<String, ModuleConfig> = BTreeMap::new();
    for (id, value) in modules_obj {
        if !known_module_ids.contains(&id.as_str()) {
            return Err(format!(
                "{file_label}: unknown module id \"{id}\" (known: {})",
                known_module_ids.join(", ")
            ));
        }
        let Some(module_value) = value.as_object() else {
            return Err(format!("{file_label}: modules.{id} must be an object"));
        };
        let Some(enabled) = module_value.get("enabled").and_then(|v| v.as_bool()) else {
            return Err(format!(
                "{file_label}: modules.{id}.enabled must be a boolean"
            ));
        };
        let options = match module_value.get("options") {
            None | Some(serde_json::Value::Null) => serde_json::json!({}),
            Some(value) if value.is_object() => value.clone(),
            Some(_) => {
                return Err(format!(
                    "{file_label}: modules.{id}.options must be an object"
                ));
            }
        };
        parsed_modules.insert(id.clone(), ModuleConfig { enabled, options });
    }
    Ok(AdeConfig {
        schema_version,
        harnesses,
        modules: parsed_modules,
    })
}

/// Load and validate `<dir>/ade.json`.
pub fn load_config(
    target_dir: &Path,
    known_module_ids: &[&str],
    known_harness_ids: &[&str],
) -> ConfigResult {
    let path = target_dir.join(CONFIG_FILE);
    let Some(text) = read_if_exists(&path) else {
        return Err(format!(
            "{CONFIG_FILE} not found in {} — run `ade init` first",
            target_dir.display()
        ));
    };
    let raw: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            return Err(format!("{CONFIG_FILE} is not valid JSON: {error}"));
        }
    };
    validate_config(&raw, known_module_ids, known_harness_ids, CONFIG_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MODULES: [&str; 3] = ["guardrails", "secrets", "sandbox"];
    const HARNESSES: [&str; 2] = ["claude-code", "codex"];

    #[test]
    fn default_config_enables_everything() {
        let config = default_config(&MODULES, &["claude-code".to_string()]);
        assert_eq!(config.schema_version, ADE_SCHEMA_VERSION);
        assert_eq!(config.modules.len(), 3);
        assert!(config.modules.values().all(|module| module.enabled));
    }

    #[test]
    fn serialization_is_sorted_and_stable() {
        let config = default_config(&MODULES, &["codex".to_string()]);
        let first = serialize_config(&config);
        let second = serialize_config(&config);
        assert_eq!(first, second);
        assert!(first.contains("\"schemaVersion\": 1"));
        let guardrails_at = first.find("guardrails").unwrap();
        let sandbox_at = first.find("sandbox").unwrap();
        let secrets_at = first.find("secrets").unwrap();
        assert!(guardrails_at < sandbox_at && sandbox_at < secrets_at);
    }

    #[test]
    fn rejects_bad_shapes_with_oracle_messages() {
        let label = "ade.json";
        assert_eq!(
            validate_config(&json!([]), &MODULES, &HARNESSES, label).unwrap_err(),
            "ade.json: config must be a JSON object"
        );
        assert_eq!(
            validate_config(&json!({"schemaVersion": 0}), &MODULES, &HARNESSES, label).unwrap_err(),
            "ade.json: schemaVersion must be a positive integer"
        );
        assert!(validate_config(
            &json!({"schemaVersion": 99, "harnesses": [], "modules": {}}),
            &MODULES,
            &HARNESSES,
            label
        )
        .unwrap_err()
        .contains("newer than this ade understands"));
        assert_eq!(
            validate_config(
                &json!({"schemaVersion": 1, "harnesses": "x"}),
                &MODULES,
                &HARNESSES,
                label
            )
            .unwrap_err(),
            "ade.json: harnesses must be an array of strings"
        );
        assert!(validate_config(
            &json!({"schemaVersion": 1, "harnesses": ["vim"], "modules": {}}),
            &MODULES,
            &HARNESSES,
            label
        )
        .unwrap_err()
        .contains("unknown harness id \"vim\""));
        assert!(validate_config(
            &json!({"schemaVersion": 1, "harnesses": [], "modules": {"nope": {"enabled": true}}}),
            &MODULES,
            &HARNESSES,
            label
        )
        .unwrap_err()
        .contains("unknown module id \"nope\""));
        assert_eq!(
            validate_config(
                &json!({"schemaVersion": 1, "harnesses": [], "modules": {"secrets": {"enabled": "yes"}}}),
                &MODULES,
                &HARNESSES,
                label
            )
            .unwrap_err(),
            "ade.json: modules.secrets.enabled must be a boolean"
        );
        assert_eq!(
            validate_config(
                &json!({"schemaVersion": 1, "harnesses": [], "modules": {"secrets": {"enabled": true, "options": []}}}),
                &MODULES,
                &HARNESSES,
                label
            )
            .unwrap_err(),
            "ade.json: modules.secrets.options must be an object"
        );
    }

    #[test]
    fn round_trips_enabled_and_options() {
        let raw = json!({
            "schemaVersion": 1,
            "harnesses": ["claude-code"],
            "modules": {"secrets": {"enabled": false, "options": {"level": 2}}}
        });
        let config = validate_config(&raw, &MODULES, &HARNESSES, "ade.json").unwrap();
        let secrets = config.modules.get("secrets").unwrap();
        assert!(!secrets.enabled);
        assert_eq!(secrets.options, json!({"level": 2}));
    }

    #[test]
    fn load_config_reports_missing_and_invalid_files() {
        let dir = std::env::temp_dir().join(format!("ade-core-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let missing = load_config(&dir, &MODULES, &HARNESSES).unwrap_err();
        assert!(missing.contains("ade.json not found"));
        assert!(missing.contains("run `ade init` first"));
        std::fs::write(dir.join(CONFIG_FILE), "{broken").unwrap();
        assert!(load_config(&dir, &MODULES, &HARNESSES)
            .unwrap_err()
            .contains("not valid JSON"));
        let config = default_config(&MODULES, &["codex".to_string()]);
        std::fs::write(dir.join(CONFIG_FILE), serialize_config(&config)).unwrap();
        let loaded = load_config(&dir, &MODULES, &HARNESSES).unwrap();
        assert_eq!(loaded, config);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
