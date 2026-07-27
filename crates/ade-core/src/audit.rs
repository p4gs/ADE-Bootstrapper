//! Tamper-evident audit log — hash-chained JSONL at `.ade/audit/log.jsonl`.
//! Port of `src/audit.ts`.
//!
//! Each entry embeds `hash = sha256(prev + canonical(entry-without-hash))`,
//! anchored at a fixed genesis value. Any modification or deletion of a
//! historical entry breaks every subsequent link.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

use crate::fsutil::{read_if_exists, sha256_hex};
use crate::version::AUDIT_GENESIS;

/// The event fields callers supply — port of the TS `AuditEventInput`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventInput {
    pub ts: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub result: String,
}

/// A chained log entry — the TS `AuditEntry` (`AuditEventInput` + prev/hash).
/// Field order matters: serialization must be byte-identical to the oracle's
/// `JSON.stringify({...input, prev, hash})`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEntry {
    pub ts: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub result: String,
    pub prev: String,
    pub hash: String,
}

/// Canonical serialization of the hashed portion of an entry (fixed key order).
/// Mirrors the TS `canonical()` — `JSON.stringify` with keys in declaration
/// order (ts, actor, action, target, result, prev), compact.
#[derive(Serialize)]
struct CanonicalEntry<'a> {
    ts: &'a str,
    actor: &'a str,
    action: &'a str,
    target: &'a str,
    result: &'a str,
    prev: &'a str,
}

fn canonical(fields: &CanonicalEntry<'_>) -> String {
    // Serializing a struct of &str fields cannot fail; fallback mirrors the
    // fsutil scalar convention rather than panicking in a production path.
    serde_json::to_string(fields).unwrap_or_else(|_| "null".to_string())
}

pub fn compute_entry(input: &AuditEventInput, prev: &str) -> AuditEntry {
    let hash = sha256_hex(&format!(
        "{prev}{}",
        canonical(&CanonicalEntry {
            ts: &input.ts,
            actor: &input.actor,
            action: &input.action,
            target: &input.target,
            result: &input.result,
            prev,
        })
    ));
    AuditEntry {
        ts: input.ts.clone(),
        actor: input.actor.clone(),
        action: input.action.clone(),
        target: input.target.clone(),
        result: input.result.clone(),
        prev: prev.to_string(),
        hash,
    }
}

/// Parse a JSONL log. The TS oracle throws on malformed lines; here that is a
/// contained `Err` for callers to surface.
pub fn parse_log(text: &str) -> Result<Vec<AuditEntry>, serde_json::Error> {
    text.split('\n')
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<AuditEntry>)
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChainVerdict {
    pub valid: bool,
    pub length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broken_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A commitment to the chain state at a point in time, recorded in the lockfile
/// (which is git-committed). Internal consistency alone cannot detect truncation
/// or a re-forged-from-genesis chain — the genesis anchor is public. The
/// checkpoint is what makes those detectable: the head hash must still be present
/// in the chain, and the chain may only have grown.
///
/// (TS `ChainCheckpoint`; named `AuditCheckpoint` here — the cross-file contract
/// `lockfile.rs` depends on.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditCheckpoint {
    pub length: u64,
    pub head_hash: String,
}

/// Checkpoint for the current chain (head hash = last entry's hash, or genesis when empty).
pub fn checkpoint_of(entries: &[AuditEntry]) -> AuditCheckpoint {
    let head = entries
        .last()
        .map(|entry| entry.hash.clone())
        .unwrap_or_else(|| AUDIT_GENESIS.to_string());
    AuditCheckpoint {
        length: entries.len() as u64,
        head_hash: head,
    }
}

/// Verify the chain from genesis. When a checkpoint is supplied, ALSO require that
/// the committed head is still present and that the log has only grown — this is
/// what catches truncation-to-empty, tail-dropping, and wholesale replacement.
pub fn verify_chain(entries: &[AuditEntry], checkpoint: Option<&AuditCheckpoint>) -> ChainVerdict {
    let mut prev: String = AUDIT_GENESIS.to_string();
    for (index, entry) in entries.iter().enumerate() {
        if entry.prev != prev {
            return ChainVerdict {
                valid: false,
                length: entries.len(),
                broken_index: Some(index),
                reason: Some("prev-mismatch".to_string()),
            };
        }
        let expected = sha256_hex(&format!(
            "{prev}{}",
            canonical(&CanonicalEntry {
                ts: &entry.ts,
                actor: &entry.actor,
                action: &entry.action,
                target: &entry.target,
                result: &entry.result,
                prev: &prev,
            })
        ));
        if entry.hash != expected {
            return ChainVerdict {
                valid: false,
                length: entries.len(),
                broken_index: Some(index),
                reason: Some("hash-mismatch".to_string()),
            };
        }
        prev = entry.hash.clone();
    }

    if let Some(checkpoint) = checkpoint {
        if (entries.len() as u64) < checkpoint.length {
            return ChainVerdict {
                valid: false,
                length: entries.len(),
                broken_index: Some(entries.len()),
                reason: Some("truncated".to_string()),
            };
        }
        let head_present = checkpoint.head_hash == AUDIT_GENESIS
            || entries
                .iter()
                .any(|entry| entry.hash == checkpoint.head_hash);
        if !head_present {
            return ChainVerdict {
                valid: false,
                length: entries.len(),
                broken_index: None,
                reason: Some("checkpoint-head-missing".to_string()),
            };
        }
    }

    ChainVerdict {
        valid: true,
        length: entries.len(),
        broken_index: None,
        reason: None,
    }
}

/// Append events to the log file, extending the chain. Creates the file with a genesis event when absent.
pub fn append_events(log_path: &Path, events: &[AuditEventInput]) -> io::Result<Vec<AuditEntry>> {
    let existing_text = read_if_exists(log_path).unwrap_or_default();
    let existing =
        parse_log(&existing_text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let mut prev = existing
        .last()
        .map(|entry| entry.hash.clone())
        .unwrap_or_else(|| AUDIT_GENESIS.to_string());
    let mut appended: Vec<AuditEntry> = Vec::new();
    for event in events {
        let entry = compute_entry(event, &prev);
        prev = entry.hash.clone();
        appended.push(entry);
    }
    let mut lines = String::new();
    for entry in &appended {
        let line = serde_json::to_string(entry)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        lines.push_str(&line);
        lines.push('\n');
    }
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(log_path, format!("{existing_text}{lines}"))?;
    Ok(appended)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::make_temp_dir;

    fn event(action: &str) -> AuditEventInput {
        AuditEventInput {
            ts: "2026-07-12T00:00:00Z".to_string(),
            actor: "ade".to_string(),
            action: action.to_string(),
            target: "test".to_string(),
            result: "ok".to_string(),
        }
    }

    fn is_lower_hex_64(hash: &str) -> bool {
        hash.len() == 64
            && hash
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    }

    // ── tests/audit.test.ts ────────────────────────────────────────────────

    #[test]
    fn isc_42_entries_chain_from_the_fixed_genesis_value() {
        let first = compute_entry(&event("one"), AUDIT_GENESIS);
        assert_eq!(first.prev, AUDIT_GENESIS);
        assert!(
            is_lower_hex_64(&first.hash),
            "hash must match /^[0-9a-f]{{64}}$/: {}",
            first.hash
        );
        let second = compute_entry(&event("two"), &first.hash);
        assert_eq!(second.prev, first.hash);
    }

    #[test]
    fn isc_41_43_append_events_builds_a_valid_chain_on_disk() {
        let dir = make_temp_dir("audit-chain");
        let log_path = dir.join("log.jsonl");
        append_events(&log_path, &[event("a"), event("b")]).unwrap();
        append_events(&log_path, &[event("c")]).unwrap();
        let entries = parse_log(&std::fs::read_to_string(&log_path).unwrap()).unwrap();
        assert_eq!(entries.len(), 3);
        let verdict = verify_chain(&entries, None);
        assert!(verdict.valid);
        assert_eq!(verdict.length, 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn isc_44_modifying_a_historical_entry_breaks_the_chain_at_that_index() {
        let dir = make_temp_dir("audit-tamper");
        let log_path = dir.join("log.jsonl");
        append_events(&log_path, &[event("a"), event("b"), event("c")]).unwrap();
        let mut entries = parse_log(&std::fs::read_to_string(&log_path).unwrap()).unwrap();
        entries[1].result = "tampered".to_string();
        let verdict = verify_chain(&entries, None);
        assert!(!verdict.valid);
        assert_eq!(verdict.broken_index, Some(1));
        assert_eq!(verdict.reason.as_deref(), Some("hash-mismatch"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn isc_45_deleting_a_mid_chain_entry_breaks_the_chain() {
        let dir = make_temp_dir("audit-delete");
        let log_path = dir.join("log.jsonl");
        append_events(&log_path, &[event("a"), event("b"), event("c")]).unwrap();
        let mut entries = parse_log(&std::fs::read_to_string(&log_path).unwrap()).unwrap();
        entries.remove(1);
        let verdict = verify_chain(&entries, None);
        assert!(!verdict.valid);
        assert_eq!(verdict.broken_index, Some(1));
        assert_eq!(verdict.reason.as_deref(), Some("prev-mismatch"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn empty_log_verifies_as_valid_genesis_state() {
        assert!(verify_chain(&[], None).valid);
        assert_eq!(parse_log("").unwrap(), Vec::<AuditEntry>::new());
    }

    #[test]
    fn forged_prev_pointer_is_caught_even_with_recomputed_hash_shape() {
        let dir = make_temp_dir("audit-forge-prev");
        let log_path = dir.join("log.jsonl");
        append_events(&log_path, &[event("a"), event("b")]).unwrap();
        let mut entries = parse_log(&std::fs::read_to_string(&log_path).unwrap()).unwrap();
        entries[1].prev = AUDIT_GENESIS.to_string();
        let verdict = verify_chain(&entries, None);
        assert!(!verdict.valid);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ── cross-language parity anchors (ISC-163/164) ────────────────────────
    // Expected values captured from the TS oracle:
    //   computeEntry({ts:"2026-07-12T00:00:00Z",actor:"ade",action:"one",
    //                 target:"test",result:"ok"}, AUDIT_GENESIS)

    #[test]
    fn hashes_and_serialized_lines_match_the_ts_oracle_byte_for_byte() {
        let first = compute_entry(&event("one"), AUDIT_GENESIS);
        assert_eq!(
            first.hash,
            "b45f177b7de63f343313dbe5f43abe2f90d51f0ef901f9e7b0b969f62689011e"
        );
        let second = compute_entry(&event("two"), &first.hash);
        assert_eq!(
            second.hash,
            "323ae71eea1c41c489c0420b371f2bfe8462679ce817bb1ffcf9c585477d92f4"
        );
        // The on-disk JSONL line must be byte-identical to JSON.stringify(entry).
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            r#"{"ts":"2026-07-12T00:00:00Z","actor":"ade","action":"one","target":"test","result":"ok","prev":"ade-genesis-v1","hash":"b45f177b7de63f343313dbe5f43abe2f90d51f0ef901f9e7b0b969f62689011e"}"#
        );
    }

    // ── tests/audit-findings.test.ts (chain-level ISC-142/143 probes) ──────
    // The TS versions drive `ade init`/`ade verify` through the CLI; the CLI
    // is ported separately, so these exercise the same attacks at the chain
    // layer the CLI delegates to.

    #[test]
    fn isc_142_truncating_the_log_to_empty_fails_against_the_checkpoint() {
        let dir = make_temp_dir("audit-truncate");
        let log_path = dir.join("log.jsonl");
        let events: Vec<AuditEventInput> = ["a", "b", "c", "d", "e", "f", "g"]
            .iter()
            .map(|action| event(action))
            .collect();
        append_events(&log_path, &events).unwrap();
        let original = parse_log(&std::fs::read_to_string(&log_path).unwrap()).unwrap();
        assert!(original.len() > 5);
        let checkpoint = checkpoint_of(&original);

        // Emptied log: internally a valid genesis state…
        assert!(verify_chain(&[], None).valid);
        // …but the committed checkpoint catches the truncation.
        let verdict = verify_chain(&[], Some(&checkpoint));
        assert!(!verdict.valid);
        assert_eq!(verdict.reason.as_deref(), Some("truncated"));
        assert_eq!(verdict.broken_index, Some(0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn isc_142_dropping_tail_entries_fails_verification() {
        let dir = make_temp_dir("audit-taildrop");
        let log_path = dir.join("log.jsonl");
        let events: Vec<AuditEventInput> = ["a", "b", "c", "d", "e", "f", "g"]
            .iter()
            .map(|action| event(action))
            .collect();
        append_events(&log_path, &events).unwrap();
        let original = parse_log(&std::fs::read_to_string(&log_path).unwrap()).unwrap();
        let checkpoint = checkpoint_of(&original);

        let dropped = &original[..original.len() - 3];
        // The shortened chain is internally consistent — the pre-fix code
        // accepted exactly this.
        assert!(verify_chain(dropped, None).valid);
        // Checkpoint length + head commitment catch it.
        let verdict = verify_chain(dropped, Some(&checkpoint));
        assert!(!verdict.valid);
        assert_eq!(verdict.reason.as_deref(), Some("truncated"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn isc_142_internally_consistent_chain_fails_against_a_checkpoint_it_does_not_contain() {
        let entry = compute_entry(
            &AuditEventInput {
                ts: "2026-07-12T00:00:00Z".to_string(),
                actor: "attacker".to_string(),
                action: "module.apply".to_string(),
                target: "secrets".to_string(),
                result: "applied".to_string(),
            },
            AUDIT_GENESIS,
        );
        let forged = vec![entry];
        // Self-consistent from genesis — the old code accepted exactly this.
        assert!(verify_chain(&forged, None).valid);
        // With the lockfile's committed checkpoint, the forgery is caught.
        let real_checkpoint = AuditCheckpoint {
            length: 12,
            head_hash: "a".repeat(64),
        };
        let verdict = verify_chain(&forged, Some(&real_checkpoint));
        assert!(!verdict.valid);
        assert_eq!(verdict.reason.as_deref(), Some("truncated"));
    }

    #[test]
    fn isc_143_a_chain_reforged_from_the_public_genesis_anchor_is_rejected_by_the_checkpoint() {
        let dir = make_temp_dir("audit-reforge");
        let log_path = dir.join("log.jsonl");
        let events: Vec<AuditEventInput> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|action| event(action))
            .collect();
        append_events(&log_path, &events).unwrap();
        let real = parse_log(&std::fs::read_to_string(&log_path).unwrap()).unwrap();
        let checkpoint = checkpoint_of(&real);

        // Attacker rebuilds a fully self-consistent chain of the SAME length that
        // hides what actually happened. Internally valid, but the head differs.
        let mut prev = AUDIT_GENESIS.to_string();
        let forged: Vec<AuditEntry> = real
            .iter()
            .map(|entry| {
                let next = compute_entry(
                    &AuditEventInput {
                        ts: entry.ts.clone(),
                        actor: entry.actor.clone(),
                        action: entry.action.clone(),
                        target: entry.target.clone(),
                        result: "clean".to_string(),
                    },
                    &prev,
                );
                prev = next.hash.clone();
                next
            })
            .collect();
        assert!(verify_chain(&forged, None).valid);
        let verdict = verify_chain(&forged, Some(&checkpoint));
        assert!(!verdict.valid);
        assert_eq!(verdict.reason.as_deref(), Some("checkpoint-head-missing"));
        assert_eq!(verdict.broken_index, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn checkpoint_semantics_growth_is_allowed_and_genesis_head_anchors_the_empty_chain() {
        let dir = make_temp_dir("audit-checkpoint");
        let log_path = dir.join("log.jsonl");
        append_events(&log_path, &[event("a"), event("b"), event("c")]).unwrap();
        let entries = parse_log(&std::fs::read_to_string(&log_path).unwrap()).unwrap();

        // Empty chain → genesis-anchored checkpoint.
        let empty_checkpoint = checkpoint_of(&[]);
        assert_eq!(
            empty_checkpoint,
            AuditCheckpoint {
                length: 0,
                head_hash: AUDIT_GENESIS.to_string()
            }
        );
        // A grown chain still verifies against the genesis checkpoint.
        assert!(verify_chain(&entries, Some(&empty_checkpoint)).valid);

        // Non-empty chain → head = last entry's hash.
        let checkpoint = checkpoint_of(&entries);
        assert_eq!(checkpoint.length, 3);
        assert_eq!(checkpoint.head_hash, entries[2].hash);

        // Growth past an earlier checkpoint is fine: the committed head is
        // still present and the log only got longer.
        let earlier = checkpoint_of(&entries[..2]);
        assert!(verify_chain(&entries, Some(&earlier)).valid);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
