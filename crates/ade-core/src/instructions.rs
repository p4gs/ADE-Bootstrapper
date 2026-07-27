//! Canonical instructions composer — `.ade/instructions.md`.
//! Port of `src/instructions.ts` (content strings byte-identical to the oracle).
//!
//! `.ade/instructions.md` is a GENERATED artifact (module blocks, registry order,
//! deterministic). The user-writable surface is `.ade/instructions.local.md`:
//! ADE creates it once, never overwrites it, never hash-locks it, and appends its
//! content to every harness's managed block. That separation is what lets the
//! generated file be lock-verified while project-specific rules still survive
//! `ade apply`.

use crate::types::{AdeModule, InstructionBlock};
use std::collections::BTreeSet;

pub const INSTRUCTIONS_PATH: &str = ".ade/instructions.md";
pub const LOCAL_INSTRUCTIONS_PATH: &str = ".ade/instructions.local.md";

const HEADER: &str = r#"# ADE Baseline Instructions

> GENERATED FILE — do not edit. Regenerated from the enabled modules on every `ade apply` / `ade translate`; edits here are overwritten and fail `ade verify`.
> Project-specific instructions belong in `.ade/instructions.local.md` — that file is yours, is never overwritten, and its content is appended to every harness's managed block.
"#;

/// The stub written once at init; never overwritten if the user has edited it.
pub const LOCAL_INSTRUCTIONS_STUB: &str = r#"# Project Instructions (user-owned)

<!-- This file is YOURS. ADE creates it once and never overwrites it.
     Everything below is appended verbatim to every harness's managed block
     (CLAUDE.md, AGENTS.md, .cursor/rules/ade.mdc) on `ade apply` / `ade translate`.
     Delete this comment and write your project's rules here. -->
"#;

pub fn collect_blocks(
    modules: &[&dyn AdeModule],
    enabled_ids: &BTreeSet<String>,
) -> Vec<InstructionBlock> {
    let mut blocks: Vec<InstructionBlock> = Vec::new();
    for module in modules {
        if !enabled_ids.contains(module.id()) {
            continue;
        }
        blocks.extend(module.instruction_blocks());
    }
    blocks
}

/// The generated canonical file's content (module blocks only).
pub fn compose_instructions(blocks: &[InstructionBlock]) -> String {
    let sections: Vec<String> = blocks
        .iter()
        .map(|block| format!("### {}\n\n{}\n", block.title, block.content.trim()))
        .collect();
    format!("{HEADER}\n{}", sections.join("\n"))
}

/// The body rendered into every harness managed block: the generated baseline
/// plus the user's own project instructions, when they wrote any.
pub fn compose_managed_body(generated: &str, local: Option<&str>) -> String {
    match strip_stub(local) {
        None => generated.to_string(),
        Some(user_content) => format!(
            "{}\n\n### Project-Specific Instructions\n\n{user_content}\n",
            generated.trim_end()
        ),
    }
}

/// Port of the oracle's `/<!--[\s\S]*?-->/g` removal: each `<!--` is dropped
/// together with everything up to the NEAREST following `-->` (lazy match);
/// an unterminated `<!--` is left in place, exactly like the regex.
fn strip_html_comments(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + "<!--".len()..];
        match after_open.find("-->") {
            Some(end) => rest = &after_open[end + "-->".len()..],
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// None when the local file is absent, empty, or still the untouched stub.
fn strip_stub(local: Option<&str>) -> Option<String> {
    let local = local?;
    let without_comments = strip_html_comments(local);
    let joined = without_comments
        .split('\n')
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && trimmed != "# Project Instructions (user-owned)"
        })
        .collect::<Vec<&str>>()
        .join("\n");
    let meaningful = joined.trim();
    if meaningful.is_empty() {
        None
    } else {
        Some(meaningful.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Ctx, Finding, ModuleResult, ModuleStatus, PlannedAction, VerifyResult};

    /// Test fixture standing in for a real module (the real `secrets` /
    /// `cost-governance` ports are separate files owned by other fan-out
    /// agents); ids and block titles match the oracle modules so the ported
    /// assertions stay meaningful.
    struct FixtureModule {
        id: &'static str,
        block_title: &'static str,
        block_content: &'static str,
    }

    impl AdeModule for FixtureModule {
        fn id(&self) -> &'static str {
            self.id
        }
        fn title(&self) -> &'static str {
            self.id
        }
        fn category(&self) -> &'static str {
            "test"
        }
        fn spec(&self) -> &'static str {
            "test"
        }
        fn instruction_blocks(&self) -> Vec<InstructionBlock> {
            vec![InstructionBlock {
                id: self.id,
                title: self.block_title,
                content: self.block_content.to_string(),
            }]
        }
        fn detect(&self, _ctx: &Ctx) -> Vec<Finding> {
            Vec::new()
        }
        fn plan(&self, _ctx: &Ctx) -> Vec<PlannedAction> {
            Vec::new()
        }
        fn apply(&self, _ctx: &Ctx) -> ModuleResult {
            ModuleResult {
                status: ModuleStatus::Skipped,
                findings: Vec::new(),
                wrote_paths: Vec::new(),
            }
        }
        fn verify(&self, _ctx: &Ctx) -> VerifyResult {
            VerifyResult {
                ok: true,
                findings: Vec::new(),
            }
        }
    }

    const SECRETS: FixtureModule = FixtureModule {
        id: "secrets",
        block_title: "Secrets & Credential Hygiene",
        block_content: "Never print or commit secrets.",
    };
    const COST: FixtureModule = FixtureModule {
        id: "cost-governance",
        block_title: "Cost & Token Budget",
        block_content: "Stay within the configured token budget.",
    };

    fn enabled(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn isc_53_composition_includes_header_edit_guidance_and_enabled_module_blocks_in_order() {
        let blocks = collect_blocks(
            &[&SECRETS, &COST],
            &enabled(&["secrets", "cost-governance"]),
        );
        let composed = compose_instructions(&blocks);
        assert!(composed.contains("# ADE Baseline Instructions"));
        assert!(composed.contains("ade translate"));
        let secrets_index = composed
            .find("Secrets & Credential Hygiene")
            .expect("secrets block");
        let cost_index = composed.find("Cost & Token Budget").expect("cost block");
        assert!(secrets_index > 0);
        assert!(cost_index > secrets_index);
    }

    #[test]
    fn isc_31_adjacent_disabled_modules_contribute_no_blocks() {
        let blocks = collect_blocks(&[&SECRETS, &COST], &enabled(&["cost-governance"]));
        let composed = compose_instructions(&blocks);
        assert!(!composed.contains("Secrets & Credential Hygiene"));
        assert!(composed.contains("Cost & Token Budget"));
    }

    #[test]
    fn composition_is_deterministic() {
        let enabled_ids = enabled(&["secrets", "cost-governance"]);
        let first = compose_instructions(&collect_blocks(&[&SECRETS, &COST], &enabled_ids));
        let second = compose_instructions(&collect_blocks(&[&SECRETS, &COST], &enabled_ids));
        assert_eq!(second, first);
    }

    #[test]
    fn managed_body_without_user_content_is_the_generated_text() {
        let generated = compose_instructions(&collect_blocks(&[&SECRETS], &enabled(&["secrets"])));
        // Absent, empty, and untouched-stub local files all mean "no user content".
        assert_eq!(compose_managed_body(&generated, None), generated);
        assert_eq!(compose_managed_body(&generated, Some("")), generated);
        assert_eq!(
            compose_managed_body(&generated, Some(LOCAL_INSTRUCTIONS_STUB)),
            generated
        );
    }

    #[test]
    fn managed_body_appends_project_specific_instructions() {
        let generated = compose_instructions(&collect_blocks(&[&SECRETS], &enabled(&["secrets"])));
        let local = format!("{LOCAL_INSTRUCTIONS_STUB}\nAlways use bun.\n");
        let body = compose_managed_body(&generated, Some(&local));
        assert_eq!(
            body,
            format!(
                "{}\n\n### Project-Specific Instructions\n\nAlways use bun.\n",
                generated.trim_end()
            )
        );
    }

    #[test]
    fn byte_parity_with_the_ts_oracle() {
        // Hashes captured from the TS oracle (Bun.CryptoHasher sha256 over the
        // exact same inputs) — proves HEADER, the stub, and the managed-body
        // appending are byte-identical (ISC-160/163).
        use crate::fsutil::sha256_hex;
        let body = compose_instructions(&[InstructionBlock {
            id: "t",
            title: "Test Block",
            content: "Do the test thing.".to_string(),
        }]);
        assert_eq!(
            sha256_hex(&body),
            "59837ee69324e71b17c1b024cc362dc09b04cc675fa8420e8e8fad8764658e68"
        );
        assert_eq!(
            sha256_hex(LOCAL_INSTRUCTIONS_STUB),
            "c7eb5321ad5750806d52b864416cfb36b996b303264554dbb3940cc7e7147456"
        );
        let local = format!("{LOCAL_INSTRUCTIONS_STUB}\nAlways use bun.\n");
        assert_eq!(
            sha256_hex(&compose_managed_body(&body, Some(&local))),
            "41c83443003e59dcddc301520d65ad82cb3815fb9ff71a3355f285561f33e6ed"
        );
    }

    #[test]
    fn html_comments_are_stripped_lazily_like_the_oracle_regex() {
        assert_eq!(strip_html_comments("a<!-- x -->b<!-- y -->c"), "abc");
        // Unterminated comment stays (the regex would not match it).
        assert_eq!(strip_html_comments("a<!-- open"), "a<!-- open");
        // Lazy match: first `<!--` pairs with the NEAREST `-->`.
        assert_eq!(strip_html_comments("a<!-- <!-- -->b"), "ab");
        assert_eq!(strip_html_comments("multi<!--\nline\n-->end"), "multiend");
    }
}
