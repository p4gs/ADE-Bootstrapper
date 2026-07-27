//! Deterministic filesystem + serialization helpers — port of `src/fsutil.ts`.
//!
//! `stable_stringify` must be BYTE-IDENTICAL to the TS oracle's
//! `JSON.stringify(sortValue(v), null, 2) + "\n"` (ISC-160). We deliberately
//! do NOT rely on serde_json's map ordering (a `preserve_order` feature
//! enabled anywhere in the workspace would silently break determinism) —
//! sorting and layout are hand-rolled here, with leaf scalars/strings escaped
//! by serde_json (same escaping rules as JSON.stringify).

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// JSON.stringify-compatible pretty printer with recursively sorted keys.
pub fn stable_stringify(value: &serde_json::Value) -> String {
    let mut out = String::new();
    write_value(value, 0, &mut out);
    out.push('\n');
    out
}

/// Compact variant (no whitespace), sorted keys — used for canonical
/// signatures (audit entries, deep-merge dedup) exactly like the oracle's
/// `JSON.stringify(sortValue(v))`.
pub fn stable_stringify_compact(value: &serde_json::Value) -> String {
    let mut out = String::new();
    write_value_compact(value, &mut out);
    out
}

fn scalar_to_string(value: &serde_json::Value) -> String {
    // serde_json string/number/bool/null rendering matches JSON.stringify for
    // the value domain ADE uses (ASCII keys, integers, short floats).
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn write_value(value: &serde_json::Value, indent: usize, out: &mut String) {
    match value {
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                push_indent(indent + 1, out);
                write_value(item, indent + 1, out);
                if index + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(indent, out);
            out.push(']');
        }
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push_str("{\n");
            for (index, key) in keys.iter().enumerate() {
                push_indent(indent + 1, out);
                out.push_str(&scalar_to_string(&serde_json::Value::String(
                    (*key).clone(),
                )));
                out.push_str(": ");
                write_value(&map[*key], indent + 1, out);
                if index + 1 < keys.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(indent, out);
            out.push('}');
        }
        scalar => out.push_str(&scalar_to_string(scalar)),
    }
}

fn write_value_compact(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value_compact(item, out);
            }
            out.push(']');
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&scalar_to_string(&serde_json::Value::String(
                    (*key).clone(),
                )));
                out.push(':');
                write_value_compact(&map[*key], out);
            }
            out.push('}');
        }
        scalar => out.push_str(&scalar_to_string(scalar)),
    }
}

fn push_indent(level: usize, out: &mut String) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

/// sha256 hex digest of a string (matches Bun.CryptoHasher("sha256").digest("hex")).
pub fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Read a file; None when absent.
pub fn read_if_exists(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// Write a file, creating parent directories. Returns true when content changed.
pub fn write_ensured(path: &Path, content: &str) -> std::io::Result<bool> {
    if let Some(existing) = read_if_exists(path) {
        if existing == content {
            return Ok(false);
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Write-then-rename, not a bare `fs::write`. A plain write truncates first,
    // so a crash — or a reader arriving mid-write — sees an EMPTY or partial
    // file where valid JSON is expected. `ade apply` writes dozens of files;
    // an interruption should leave each one either untouched or complete, never
    // half. Rename within the same directory is atomic on every platform we
    // target. A stray temp file left by a hard kill is caught by verify's
    // unknown-file rule, which is a far better failure than silent truncation.
    static WRITE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = WRITE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "ade".to_string());
    let temp = match path.parent() {
        Some(parent) => parent.join(format!(
            ".{file_name}.ade-tmp-{}-{unique}",
            std::process::id()
        )),
        None => PathBuf::from(format!(".{file_name}.ade-tmp-{unique}")),
    };
    if let Err(error) = fs::write(&temp, content) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(true)
}

/// Idempotently ensure lines exist in a line-oriented file (e.g. .gitignore).
/// Appends only missing lines under a labelled comment; never reorders or
/// removes user content. Returns true when the file changed.
pub fn ensure_lines(path: &Path, lines: &[&str], label: &str) -> std::io::Result<bool> {
    let existing = read_if_exists(path).unwrap_or_default();
    let present: std::collections::BTreeSet<String> = existing
        .split('\n')
        .map(|line| line.trim().to_string())
        .collect();
    let missing: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| !present.contains(line.trim()))
        .collect();
    if missing.is_empty() {
        return Ok(false);
    }
    let prefix = if !existing.is_empty() && !existing.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let block = format!("{prefix}# {label}\n{}\n", missing.join("\n"));
    // Through `write_ensured` so this append gets the same all-or-nothing
    // guarantee as every other write — a half-appended .gitignore is still a
    // corrupted .gitignore.
    write_ensured(path, &format!("{existing}{block}"))?;
    Ok(true)
}

/// Deep-merge `patch` into `base` for ADE-owned JSON surfaces.
/// Objects merge recursively; arrays union with signature-based dedup (user
/// entries preserved, ADE entries appended); scalars from patch win.
pub fn deep_merge(base: &serde_json::Value, patch: &serde_json::Value) -> serde_json::Value {
    match (base, patch) {
        (serde_json::Value::Array(base_items), serde_json::Value::Array(patch_items)) => {
            let mut seen: std::collections::BTreeSet<String> =
                base_items.iter().map(stable_stringify_compact).collect();
            let mut merged = base_items.clone();
            for entry in patch_items {
                let sig = stable_stringify_compact(entry);
                if !seen.contains(&sig) {
                    seen.insert(sig);
                    merged.push(entry.clone());
                }
            }
            serde_json::Value::Array(merged)
        }
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            let mut out = base_map.clone();
            for (key, patch_value) in patch_map {
                let next = match out.get(key) {
                    Some(base_value) => match (base_value, patch_value) {
                        (serde_json::Value::Object(_), serde_json::Value::Object(_))
                        | (serde_json::Value::Array(_), serde_json::Value::Array(_)) => {
                            deep_merge(base_value, patch_value)
                        }
                        _ => patch_value.clone(),
                    },
                    None => patch_value.clone(),
                };
                out.insert(key.clone(), next);
            }
            serde_json::Value::Object(out)
        }
        // Top-level call sites only merge objects; anything else: patch wins.
        (_, other) => other.clone(),
    }
}

/// Undo a `deep_merge` patch, leaving everything the user owns.
///
/// This is what makes `ade remove` safe on files ADE only ever CO-owns
/// (`.claude/settings.json`, `.mcp.json`): ADE's additions are not marked, so
/// the only honest way to withdraw them is to subtract exactly the values it
/// would have written and touch nothing else.
///
/// Rules, all deliberately conservative:
/// - arrays: drop elements deep-equal to a patch element; user elements stay,
///   including ones the user happens to have written identically elsewhere in
///   the file (we only ever look where the patch reached).
/// - objects: recurse; a key is dropped only when its value deep-equals the
///   patch's, or when recursion emptied a container the patch created.
/// - a value the user CHANGED is left alone — a modified value is theirs now.
pub fn subtract_json(current: &serde_json::Value, patch: &serde_json::Value) -> serde_json::Value {
    match (current, patch) {
        (serde_json::Value::Array(current_items), serde_json::Value::Array(patch_items)) => {
            let drop: std::collections::BTreeSet<String> =
                patch_items.iter().map(stable_stringify_compact).collect();
            serde_json::Value::Array(
                current_items
                    .iter()
                    .filter(|item| !drop.contains(&stable_stringify_compact(item)))
                    .cloned()
                    .collect(),
            )
        }
        (serde_json::Value::Object(current_map), serde_json::Value::Object(patch_map)) => {
            let mut out = current_map.clone();
            for (key, patch_value) in patch_map {
                let Some(current_value) = out.get(key) else {
                    continue;
                };
                match (current_value, patch_value) {
                    (serde_json::Value::Object(_), serde_json::Value::Object(_))
                    | (serde_json::Value::Array(_), serde_json::Value::Array(_)) => {
                        let reduced = subtract_json(current_value, patch_value);
                        let empty = match &reduced {
                            serde_json::Value::Object(map) => map.is_empty(),
                            serde_json::Value::Array(items) => items.is_empty(),
                            _ => false,
                        };
                        if empty {
                            out.remove(key);
                        } else {
                            out.insert(key.clone(), reduced);
                        }
                    }
                    _ => {
                        if current_value == patch_value {
                            out.remove(key);
                        }
                    }
                }
            }
            serde_json::Value::Object(out)
        }
        _ => current.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// ISC-220: a write is all-or-nothing, and concurrent readers never observe
    /// a partial file. Before this, `write_ensured` truncated in place, so a
    /// reader arriving mid-write got an empty file — the exact shape of the
    /// intermittent parse failure this suite produced.
    #[test]
    fn write_ensured_is_atomic_under_concurrent_readers() {
        use crate::testutil::make_temp_dir;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::Arc;

        let dir = make_temp_dir("fsutil-atomic");
        let path = dir.join("manifest.json");
        let big = stable_stringify(&json!({
            "tools": (0..400).map(|n| format!("tool-{n}")).collect::<Vec<_>>()
        }));
        let small = stable_stringify(&json!({ "tools": ["one"] }));
        write_ensured(&path, &small).expect("seed");

        let stop = Arc::new(AtomicBool::new(false));
        let torn = Arc::new(AtomicUsize::new(0));
        let reads = Arc::new(AtomicUsize::new(0));
        let reader = {
            let (path, stop, torn, reads) =
                (path.clone(), stop.clone(), torn.clone(), reads.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let Some(text) = read_if_exists(&path) {
                        reads.fetch_add(1, Ordering::Relaxed);
                        // Every observation must be one of the two whole files.
                        if serde_json::from_str::<serde_json::Value>(&text).is_err() {
                            torn.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            })
        };
        for round in 0..60 {
            let content = if round % 2 == 0 { &big } else { &small };
            write_ensured(&path, content).expect("write");
        }
        stop.store(true, Ordering::Relaxed);
        reader.join().expect("reader");

        assert!(reads.load(Ordering::Relaxed) > 0, "reader never ran");
        assert_eq!(
            torn.load(Ordering::Relaxed),
            0,
            "a reader observed a partially-written file"
        );
        // No temp files may survive a clean run.
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .expect("read dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains("ade-tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same tag must never yield the same directory twice — the cause of a
    /// genuine intermittent failure in this suite.
    #[test]
    fn make_temp_dir_never_collides_for_the_same_tag() {
        use crate::testutil::make_temp_dir;
        use std::collections::BTreeSet;

        let handles: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..40)
                        .map(|_| make_temp_dir("collide"))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut all: Vec<std::path::PathBuf> = Vec::new();
        for handle in handles {
            all.extend(handle.join().expect("thread"));
        }
        let unique: BTreeSet<&std::path::PathBuf> = all.iter().collect();
        assert_eq!(unique.len(), all.len(), "temp dir names collided");
        for dir in &all {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn subtract_json_is_the_inverse_of_deep_merge_and_spares_user_content() {
        let user = json!({
            "permissions": { "deny": ["Bash(rm -rf *)"], "allow": ["Read(./src/**)"] },
            "hooks": { "PostToolUse": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "user-thing" }] }] },
            "theme": "dark"
        });
        let patch = json!({
            "permissions": { "deny": ["Read(./.env)", "Read(~/.ssh/**)"] },
            "hooks": { "PostToolUse": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "sh .ade/hooks/audit-log.ts" }] }] }
        });
        let merged = deep_merge(&user, &patch);
        assert_eq!(subtract_json(&merged, &patch), user);

        // A value the user changed after the merge is theirs — never removed.
        let mut edited = merged.clone();
        edited["permissions"]["deny"] = json!(["Read(./.env)", "Read(~/.ssh/**) # mine now"]);
        let reduced = subtract_json(&edited, &patch);
        assert_eq!(
            reduced["permissions"]["deny"],
            json!(["Read(~/.ssh/**) # mine now"])
        );

        // Containers the patch created are pruned when they empty out.
        let fresh = deep_merge(&json!({}), &patch);
        assert_eq!(subtract_json(&fresh, &patch), json!({}));
    }

    #[test]
    fn stable_stringify_matches_js_json_stringify_layout() {
        // Expected strings captured from the TS oracle:
        //   JSON.stringify(sortValue(v), null, 2) + "\n"
        assert_eq!(stable_stringify(&json!({})), "{}\n");
        assert_eq!(stable_stringify(&json!([])), "[]\n");
        assert_eq!(
            stable_stringify(&json!({"b": 1, "a": 2})),
            "{\n  \"a\": 2,\n  \"b\": 1\n}\n"
        );
        assert_eq!(
            stable_stringify(&json!({"z": {"d": [1, {"y": true, "x": null}], "c": "s"}})),
            "{\n  \"z\": {\n    \"c\": \"s\",\n    \"d\": [\n      1,\n      {\n        \"x\": null,\n        \"y\": true\n      }\n    ]\n  }\n}\n"
        );
        assert_eq!(
            stable_stringify(&json!({"e": [], "o": {}, "f": 0.5, "n": -3})),
            "{\n  \"e\": [],\n  \"f\": 0.5,\n  \"n\": -3,\n  \"o\": {}\n}\n"
        );
        // String escaping parity (quotes, backslash, newline, unicode passthrough).
        assert_eq!(
            stable_stringify(&json!({"s": "a\"b\\c\nd — é"})),
            "{\n  \"s\": \"a\\\"b\\\\c\\nd — é\"\n}\n"
        );
    }

    #[test]
    fn compact_signature_matches_js() {
        assert_eq!(
            stable_stringify_compact(&json!({"b": [2, {"z": 1, "a": 2}], "a": "x"})),
            r#"{"a":"x","b":[2,{"a":2,"z":1}]}"#
        );
    }

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(sha256_hex("ade-genesis-v1"), sha256_hex("ade-genesis-v1"));
        assert_eq!(sha256_hex("abc").len(), 64);
    }

    #[test]
    fn write_ensured_and_read_round_trip() {
        let dir = std::env::temp_dir().join(format!("ade-core-test-{}", std::process::id()));
        let path = dir.join("nested").join("file.txt");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(read_if_exists(&path).is_none());
        assert!(write_ensured(&path, "hello\n").unwrap());
        assert!(!write_ensured(&path, "hello\n").unwrap());
        assert!(write_ensured(&path, "changed\n").unwrap());
        assert_eq!(read_if_exists(&path).as_deref(), Some("changed\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_lines_appends_only_missing_under_label() {
        let dir = std::env::temp_dir().join(format!("ade-core-el-{}", std::process::id()));
        let path = dir.join(".gitignore");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "node_modules\n").unwrap();
        assert!(ensure_lines(&path, &[".env", "node_modules"], "ade").unwrap());
        let content = read_if_exists(&path).unwrap();
        assert_eq!(content, "node_modules\n# ade\n.env\n");
        assert!(!ensure_lines(&path, &[".env"], "ade").unwrap());
        // No trailing newline on existing content gets a separating newline.
        std::fs::write(&path, "user").unwrap();
        assert!(ensure_lines(&path, &["x"], "ade").unwrap());
        assert_eq!(read_if_exists(&path).unwrap(), "user\n# ade\nx\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deep_merge_semantics_match_oracle() {
        let base = json!({"keep": 1, "obj": {"a": 1}, "arr": [{"id": 1}, "x"]});
        let patch = json!({"obj": {"b": 2}, "arr": [{"id": 1}, "y"], "new": true});
        let merged = deep_merge(&base, &patch);
        assert_eq!(
            merged,
            json!({"keep": 1, "obj": {"a": 1, "b": 2}, "arr": [{"id": 1}, "x", "y"], "new": true})
        );
        // Scalar conflict: patch wins.
        assert_eq!(
            deep_merge(&json!({"a": 1}), &json!({"a": 2})),
            json!({"a": 2})
        );
    }
}
