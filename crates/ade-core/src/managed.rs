//! Managed-block engine for user-owned files (CLAUDE.md, AGENTS.md, …).
//! Faithful port of `src/managed.ts` — the audit-hardened surface (ISC-163):
//! provenance-line ownership proof, in-block hand-edit refusal, corrupt-marker
//! abort. Message strings are kept oracle-identical.

use crate::fsutil::sha256_hex;
use crate::version::{MARKER_BEGIN, MARKER_END};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpsertResult {
    Ok { content: String, changed: bool },
    Err { error: String },
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// Render the full managed block (markers + hash-bearing provenance + body).
pub fn render_managed_block(body: &str) -> String {
    let normalized = body.trim_end();
    let provenance = format!(
        "<!-- Managed by ADE Bootstrapper — generated from .ade/instructions.md; do not hand-edit this block; run `ade translate` to regenerate. content-hash:{} -->",
        sha256_hex(normalized)
    );
    format!("{MARKER_BEGIN}\n{provenance}\n\n{normalized}\n{MARKER_END}")
}

struct ParsedBlock {
    begin_index: usize,
    end_index: usize,
    body: String,
    declared_hash: Option<String>,
}

fn find_hash(inner: &str) -> Option<String> {
    // Port of /content-hash:([0-9a-f]{64})/
    let marker = "content-hash:";
    let mut search_from = 0;
    while let Some(position) = inner[search_from..].find(marker) {
        let start = search_from + position + marker.len();
        let candidate: String = inner[start..]
            .chars()
            .take(64)
            .filter(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            .collect();
        if candidate.len() == 64 && inner[start..].starts_with(candidate.as_str()) {
            return Some(candidate);
        }
        search_from = start;
    }
    None
}

fn parse_block(existing: &str) -> Option<ParsedBlock> {
    let begin_index = existing.find(MARKER_BEGIN)?;
    let end_index = existing.find(MARKER_END)?;
    if begin_index >= end_index {
        return None;
    }
    let inner = &existing[begin_index + MARKER_BEGIN.len()..end_index];
    let declared_hash = find_hash(inner);
    let body = match inner.find("-->") {
        None => inner.trim().to_string(),
        Some(provenance_end) => inner[provenance_end + "-->".len()..].trim().to_string(),
    };
    Some(ParsedBlock {
        begin_index,
        end_index,
        body,
        declared_hash,
    })
}

/// Insert or replace the managed block in `existing`.
/// - no markers → append (with separating blank line)
/// - one balanced block, body hash intact → replace
/// - one balanced block, body hash MISMATCH → error (user edited inside; refuse)
/// - markers with NO provenance hash → error (not our content; refuse)
/// - anything else (unbalanced, duplicated, reversed) → error, file untouched
pub fn upsert_managed_block(existing: &str, body: &str) -> UpsertResult {
    let block = render_managed_block(body);
    let begins = count_occurrences(existing, MARKER_BEGIN);
    let ends = count_occurrences(existing, MARKER_END);

    if begins == 0 && ends == 0 {
        let separator = if existing.is_empty() {
            ""
        } else if existing.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        return UpsertResult::Ok {
            content: format!("{existing}{separator}{block}\n"),
            changed: true,
        };
    }

    if begins == 1 && ends == 1 {
        let Some(parsed) = parse_block(existing) else {
            return UpsertResult::Err {
                error: "managed markers are reversed (end before begin) — refusing to rewrite; fix the markers manually".to_string(),
            };
        };
        let Some(declared_hash) = parsed.declared_hash else {
            // Every block ADE writes carries a content-hash provenance line. Markers with
            // no hash are NOT our block — replacing them would destroy content outside
            // our ownership, the one thing this engine must never do.
            return UpsertResult::Err {
                error: "found ade markers with no ADE provenance line — this content was not written by ade; refusing to overwrite it. Remove or rename the markers if you want ade to manage this file.".to_string(),
            };
        };
        if sha256_hex(&parsed.body) != declared_hash {
            return UpsertResult::Err {
                error: "managed block was hand-edited (content-hash mismatch) — refusing to overwrite; move your changes outside the ade markers (or into .ade/instructions.local.md) and re-run `ade translate`".to_string(),
            };
        }
        let before = &existing[..parsed.begin_index];
        let after = &existing[parsed.end_index + MARKER_END.len()..];
        let next = format!("{before}{block}{after}");
        let changed = next != existing;
        return UpsertResult::Ok {
            content: next,
            changed,
        };
    }

    UpsertResult::Err {
        error: format!(
            "managed markers are corrupt ({begins} begin, {ends} end) — refusing to rewrite; fix the markers manually"
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveResult {
    /// Block excised; `content` is what remains (empty when the block was the whole file).
    Removed { content: String },
    /// No ADE markers in this file.
    Absent,
    /// Refused for the same reasons `upsert_managed_block` refuses.
    Refused { error: String },
}

/// Excise the managed block, leaving everything else.
///
/// This is the inverse of `upsert_managed_block` and inherits its ownership
/// rules exactly: a block with no ADE provenance line, a hand-edited block, or
/// corrupt markers is REFUSED rather than cut out. Uninstalling is not a
/// licence to delete content we cannot prove we wrote — a wrong excision here
/// destroys user text, which is the one thing this engine must never do.
///
/// One documented normalization: `upsert` inserts a blank-line separator when
/// it appends, and the separator it chose cannot be recovered from the final
/// text (a file that ended with a newline and one that did not both produce
/// the same bytes once the block is in). Removal keeps a single trailing
/// newline, so a file that had NO trailing newline before ADE touched it gains
/// one. Only whitespace is ever affected; no user byte is dropped.
pub fn remove_managed_block(existing: &str) -> RemoveResult {
    let begins = count_occurrences(existing, MARKER_BEGIN);
    let ends = count_occurrences(existing, MARKER_END);

    if begins == 0 && ends == 0 {
        return RemoveResult::Absent;
    }
    if begins != 1 || ends != 1 {
        return RemoveResult::Refused {
            error: format!(
                "managed markers are corrupt ({begins} begin, {ends} end) — refusing to rewrite; fix the markers manually"
            ),
        };
    }
    let Some(parsed) = parse_block(existing) else {
        return RemoveResult::Refused {
            error: "managed markers are reversed (end before begin) — refusing to rewrite; fix the markers manually".to_string(),
        };
    };
    let Some(declared_hash) = parsed.declared_hash else {
        return RemoveResult::Refused {
            error: "found ade markers with no ADE provenance line — this content was not written by ade; refusing to remove it. Delete the markers by hand if they are yours.".to_string(),
        };
    };
    if sha256_hex(&parsed.body) != declared_hash {
        return RemoveResult::Refused {
            error: "managed block was hand-edited (content-hash mismatch) — refusing to remove it; your edits would be lost. Move them outside the ade markers, then re-run `ade remove`.".to_string(),
        };
    }

    let before = &existing[..parsed.begin_index];
    let after = &existing[parsed.end_index + MARKER_END.len()..];
    let mut content = before.to_string();
    // Drop the blank-line separator `upsert` introduced when it appended.
    if content.ends_with("\n\n") {
        content.pop();
    }
    content.push_str(after.strip_prefix('\n').unwrap_or(after));
    if content.trim().is_empty() {
        content = String::new();
    }
    RemoveResult::Removed { content }
}

/// Extract the current managed block text (markers inclusive), or None when absent/corrupt.
pub fn extract_managed_block(existing: &str) -> Option<String> {
    let begins = count_occurrences(existing, MARKER_BEGIN);
    let ends = count_occurrences(existing, MARKER_END);
    if begins != 1 || ends != 1 {
        return None;
    }
    let parsed = parse_block(existing)?;
    Some(existing[parsed.begin_index..parsed.end_index + MARKER_END.len()].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_to_empty_and_nonempty() {
        let UpsertResult::Ok { content, changed } = upsert_managed_block("", "BODY") else {
            panic!("expected ok");
        };
        assert!(changed);
        assert!(content.starts_with(MARKER_BEGIN));
        assert!(content.contains("content-hash:"));
        assert!(content.ends_with(&format!("{MARKER_END}\n")));

        let UpsertResult::Ok {
            content: with_user, ..
        } = upsert_managed_block("user text\n", "BODY")
        else {
            panic!("expected ok");
        };
        assert!(with_user.starts_with("user text\n\n<!-- ade:begin -->"));

        let UpsertResult::Ok {
            content: no_newline,
            ..
        } = upsert_managed_block("user text", "BODY")
        else {
            panic!("expected ok");
        };
        assert!(no_newline.starts_with("user text\n\n<!-- ade:begin -->"));
    }

    #[test]
    fn replace_intact_block_preserves_user_content_and_is_idempotent() {
        let UpsertResult::Ok { content: first, .. } = upsert_managed_block("above\n", "BODY-1")
        else {
            panic!("expected ok");
        };
        let seeded = format!("{first}below\n");
        let UpsertResult::Ok {
            content: second,
            changed,
        } = upsert_managed_block(&seeded, "BODY-2")
        else {
            panic!("expected ok");
        };
        assert!(changed);
        assert!(second.starts_with("above\n"));
        assert!(second.ends_with("below\n"));
        assert!(second.contains("BODY-2"));
        assert!(!second.contains("BODY-1"));
        let UpsertResult::Ok {
            content: third,
            changed: changed_again,
        } = upsert_managed_block(&second, "BODY-2")
        else {
            panic!("expected ok");
        };
        assert_eq!(third, second);
        assert!(!changed_again);
    }

    #[test]
    fn hand_edit_inside_block_is_refused() {
        let UpsertResult::Ok { content, .. } = upsert_managed_block("", "BODY") else {
            panic!("expected ok");
        };
        let tampered = content.replace("BODY", "BODY plus my edit");
        let result = upsert_managed_block(&tampered, "NEW");
        let UpsertResult::Err { error } = result else {
            panic!("expected refusal")
        };
        assert!(error.contains("hand-edited"));
    }

    #[test]
    fn markers_without_provenance_are_not_ours_and_are_refused() {
        let foreign = format!(
            "docs about markers:\n{MARKER_BEGIN}\nNever deploy on Fridays.\n{MARKER_END}\n"
        );
        let result = upsert_managed_block(&foreign, "NEW");
        let UpsertResult::Err { error } = result else {
            panic!("expected refusal")
        };
        assert!(error.contains("no ADE provenance line"));
    }

    #[test]
    fn corrupt_and_reversed_markers_are_refused() {
        let duplicated = format!("{MARKER_BEGIN}\nx\n{MARKER_END}\n{MARKER_BEGIN}\n");
        let UpsertResult::Err { error } = upsert_managed_block(&duplicated, "NEW") else {
            panic!("expected refusal");
        };
        assert!(error.contains("corrupt"));

        let reversed = format!("{MARKER_END}\ncontent\n{MARKER_BEGIN}\n");
        let UpsertResult::Err {
            error: reversed_error,
        } = upsert_managed_block(&reversed, "NEW")
        else {
            panic!("expected refusal");
        };
        assert!(reversed_error.contains("reversed"));
    }

    #[test]
    fn extract_returns_block_or_none() {
        let UpsertResult::Ok { content, .. } = upsert_managed_block("above\n", "BODY") else {
            panic!("expected ok");
        };
        let extracted = extract_managed_block(&content).expect("block");
        assert!(extracted.starts_with(MARKER_BEGIN));
        assert!(extracted.ends_with(MARKER_END));
        assert!(extract_managed_block("no markers").is_none());
        assert!(extract_managed_block(&format!("{MARKER_END}{MARKER_BEGIN}")).is_none());
    }

    #[test]
    fn render_parity_with_oracle_hash_semantics() {
        // The body is trimEnd()'d before hashing — trailing whitespace never
        // changes the hash; leading whitespace does.
        let a = render_managed_block("BODY\n\n");
        let b = render_managed_block("BODY");
        assert_eq!(a, b);
    }
}
