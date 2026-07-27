//! Module: Secure-Coding Guardrails — port of `src/modules/guardrails.ts`.
//! Spec component: "Secure-by-default coding guardrails — an opinionated
//! secure-coding ruleset (Project CodeGuard-style) wired into the harness's
//! canonical instructions so AI-generated code is secure by default."
//! Boundary controlled: the code-generation boundary — every rule constrains
//! what the harness is allowed to emit as code, before it ever reaches review.
//!
//! Integration over rebuild: Project CodeGuard
//! (github.com/cosai-oasis/project-codeguard) is a ruleset framework, not a
//! CLI. ADE ships a distilled built-in ruleset in `.ade/guardrails/`; the full
//! CodeGuard rules can be vendored into the same directory and are picked up
//! by the same binding instruction block.

use crate::fsutil::read_if_exists;
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};

pub const GUARDRAILS_DIR: &str = ".ade/guardrails";

/// Parsed machine-checkable frontmatter carried by every rule file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFrontmatter {
    pub id: String,
    /// One of "critical" | "high" | "medium" (validated by the parser).
    pub severity: String,
    pub applies_to: Vec<String>,
}

const SEVERITIES: [&str; 3] = ["critical", "high", "medium"];

fn is_kebab_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Strip at most one leading and one trailing quote character — the port of
/// the oracle's `replace(/^["']|["']$/g, "")`.
fn strip_quotes(entry: &str) -> &str {
    let entry = entry
        .strip_prefix('"')
        .or_else(|| entry.strip_prefix('\''))
        .unwrap_or(entry);
    entry
        .strip_suffix('"')
        .or_else(|| entry.strip_suffix('\''))
        .unwrap_or(entry)
}

/// Tiny parser for the YAML-style frontmatter between `---` lines at the top
/// of a rule file. Returns None when the frontmatter is absent or malformed.
/// Supports exactly the shape ADE emits: scalar `key: value` lines plus an
/// inline array `applies_to: [a, b]`.
pub fn parse_rule_frontmatter(text: &str) -> Option<RuleFrontmatter> {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.first().map(|line| line.trim()) != Some("---") {
        return None;
    }
    let end = lines
        .iter()
        .skip(1)
        .position(|line| *line == "---")
        .map(|position| position + 1)?;
    let mut fields: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for line in &lines[1..end] {
        if line.trim().is_empty() {
            continue;
        }
        let colon = line.find(':')?;
        fields.insert(
            line[..colon].trim().to_string(),
            line[colon + 1..].trim().to_string(),
        );
    }
    let id = fields.get("id")?;
    if !is_kebab_id(id) {
        return None;
    }
    let severity = fields.get("severity")?;
    if !SEVERITIES.contains(&severity.as_str()) {
        return None;
    }
    let applies_raw = fields.get("applies_to")?;
    if !applies_raw.starts_with('[') || !applies_raw.ends_with(']') {
        return None;
    }
    let applies_to: Vec<String> = applies_raw[1..applies_raw.len() - 1]
        .split(',')
        .map(|entry| strip_quotes(entry.trim()).to_string())
        .filter(|entry| !entry.is_empty())
        .collect();
    if applies_to.is_empty() {
        return None;
    }
    Some(RuleFrontmatter {
        id: id.clone(),
        severity: severity.clone(),
        applies_to,
    })
}

fn frontmatter(id: &str, severity: &str, applies_to: &[&str]) -> String {
    format!(
        "---\nid: {id}\nseverity: {severity}\napplies_to: [{}]\n---",
        applies_to.join(", ")
    )
}

/// The distilled built-in ruleset. Static template strings — fully
/// deterministic. Entries are in the oracle's declaration order; call sites
/// sort by file name exactly like the TS (`Object.keys(RULE_FILES).sort()`).
pub fn rule_files() -> Vec<(&'static str, String)> {
    vec![
        (
            "input-validation.md",
            format!(
                "{}{}",
                frontmatter("input-validation", "high", &["all"]),
                r##"

# Input Validation

All data crossing a trust boundary (HTTP requests, CLI args, file contents,
environment, IPC, LLM output) is untrusted until validated.

DO:
- Validate at the boundary, immediately on receipt, before any other use.
- Use allowlists (known-good shapes) — schema validation, strict enums, typed parsers.
- Enforce length, range, and type limits on every field; reject, don't truncate.
- Canonicalize (decode, normalize unicode/paths) BEFORE validating, never after.
- Treat deserialized data (JSON, YAML, pickle-like formats) as untrusted input.

DON'T:
- Don't use denylists or regex "bad character" filters as the primary control.
- Don't validate on the client only — server-side validation is the control.
- Don't pass unvalidated input into interpreters, templates, or file APIs
  (see injection.md).
- Don't accept unbounded collections or payloads; cap sizes explicitly.
"##
            ),
        ),
        (
            "injection.md",
            format!(
                "{}{}",
                frontmatter("injection", "critical", &["all"]),
                r##"

# Injection (SQL / Command / Path / XSS)

Never build executable syntax by string concatenation with untrusted data.

DO:
- SQL: use parameterized queries / prepared statements for EVERY query;
  identifiers that must vary come from a hardcoded allowlist.
- Commands: use argv-array process APIs (execFile/spawn with arg lists);
  if a shell is truly unavoidable, allowlist-validate every argument first.
- Paths: resolve to an absolute path, then verify it is inside the intended
  root directory before reading or writing; reject on failure.
- XSS: rely on the framework's contextual auto-escaping; sanitize any
  unavoidable raw-HTML sink with a maintained sanitizer library.
- Set a restrictive Content-Security-Policy on web surfaces.

DON'T:
- Don't interpolate user input into SQL, shell strings, eval, or templates —
  no exceptions, including "internal" or "trusted" values.
- Don't strip ../ sequences and call it path validation; check containment
  after full resolution.
- Don't use innerHTML/dangerouslySetInnerHTML with any user-influenced value.
- Don't disable framework escaping to "fix" rendering problems.
"##
            ),
        ),
        (
            "secrets-handling.md",
            format!(
                "{}{}",
                frontmatter("secrets-handling", "critical", &["all"]),
                r##"

# Secrets Handling

Generated code must never contain, log, or transmit secret material.

DO:
- Read secrets from the environment or a secret manager at the point of use.
- Reference secrets by NAME in code, config templates, and docs (e.g.
  `API_KEY`), never by value.
- Redact known-sensitive fields before logging or serializing objects.
- Add secret-bearing files (.env, *.pem, *.key) to .gitignore before
  creating them.

DON'T:
- Don't hardcode API keys, tokens, passwords, or private keys — not in code,
  tests, fixtures, examples, or comments.
- Don't echo environment variables that look like credentials
  (`*_KEY`, `*_TOKEN`, `*_SECRET`, `*_PASSWORD`).
- Don't write secrets into error messages, debug output, or URLs.
- Don't invent placeholder secrets that look real; use obvious placeholders
  like `<YOUR_API_KEY>`.
"##
            ),
        ),
        (
            "authn-authz.md",
            format!(
                "{}{}",
                frontmatter("authn-authz", "critical", &["all"]),
                r##"

# Authentication & Authorization

Every non-public operation checks WHO is calling and WHETHER they may.

DO:
- Enforce authentication and authorization on the server for every request;
  deny by default when no rule matches.
- Perform object-level checks: verify the caller may access the specific
  resource id in the request (prevent IDOR/BOLA).
- Use the platform's vetted auth framework and session management; store
  session tokens in HttpOnly, Secure cookies where applicable.
- Derive privileged fields (user id, role, tenant) from the verified session,
  never from the request body.
- Validate JWTs fully: signature, algorithm allowlist, expiry, issuer,
  and audience.

DON'T:
- Don't roll your own authentication, session, or password storage; use
  argon2/bcrypt/scrypt via a maintained library when you must hash passwords.
- Don't rely on hiding UI elements or "unguessable" URLs as access control.
- Don't accept `alg: none` or client-supplied role/tenant/id claims.
- Don't add authentication bypasses for tests or debugging into shipped code.
"##
            ),
        ),
        (
            "crypto.md",
            format!(
                "{}{}",
                frontmatter("crypto", "high", &["all"]),
                r##"

# Cryptography

Use vetted primitives from the platform's standard library — never invent.

DO:
- Use high-level, misuse-resistant APIs (e.g. libsodium-style, WebCrypto,
  language-standard AEAD) with authenticated encryption (AES-GCM,
  ChaCha20-Poly1305).
- Generate keys, IVs, tokens, and salts with a cryptographically secure RNG
  (crypto.getRandomValues / secrets module equivalents).
- Use a fresh, unique nonce/IV per encryption; never reuse with the same key.
- Hash passwords with argon2id/bcrypt/scrypt (dedicated KDFs), not fast hashes.
- Use TLS for data in transit with certificate verification left ON.

DON'T:
- Don't implement your own ciphers, protocols, padding, or comparisons;
  use constant-time comparison helpers for secret material.
- Don't use MD5/SHA-1 for security purposes or ECB mode anywhere.
- Don't seed security decisions from Math.random()-style PRNGs.
- Don't disable TLS verification, even "temporarily" in dev code paths.
"##
            ),
        ),
        (
            "error-handling.md",
            format!(
                "{}{}",
                frontmatter("error-handling", "medium", &["all"]),
                r##"

# Error Handling & Logging

Fail closed, tell the user little, tell the operator enough.

DO:
- Fail closed: on unexpected errors in a security decision, deny.
- Return generic error messages to callers; log the detailed cause
  server-side with a correlation id.
- Handle every error path explicitly — no empty catch blocks; either
  recover meaningfully or propagate.
- Log security-relevant events (authn failures, authz denials, validation
  rejects) at a consistent level for monitoring.

DON'T:
- Don't leak stack traces, file paths, SQL, or dependency versions in
  responses to callers.
- Don't log secrets, session tokens, or full request bodies containing
  personal data (redact first).
- Don't swallow exceptions to make tests or demos pass.
- Don't use error messages to enumerate state (e.g. "user exists but wrong
  password") — keep authn errors uniform.
"##
            ),
        ),
        (
            "dependencies.md",
            format!(
                "{}{}",
                frontmatter("dependencies", "high", &["all"]),
                r##"

# Dependencies

Every dependency is third-party code running with your privileges.

DO:
- Prefer the standard library; add a dependency only when it earns its keep.
- Verify a package EXISTS and is the well-known one before adding it —
  AI-suggested names are untrusted (slopsquatting/typosquatting risk).
- Pin versions via the lockfile and commit it; upgrade deliberately.
- Run the project's vulnerability scanner after changing dependencies and
  fix HIGH/CRITICAL findings before proceeding.

DON'T:
- Don't add dependencies for trivial one-liners.
- Don't fetch code or install scripts from URLs at build/run time.
- Don't ignore or suppress vulnerability findings to get a build green —
  fix the code or upgrade the dependency.
- Don't use abandoned packages for security-critical functions.
"##
            ),
        ),
    ]
}

/// Rule file names sorted — the iteration order every call site uses
/// (the port of `Object.keys(RULE_FILES).sort()`).
fn sorted_rule_files() -> Vec<(&'static str, String)> {
    let mut files = rule_files();
    files.sort_by_key(|(name, _)| *name);
    files
}

/// Rule ids in a stable order (derived from the rule file names, sorted).
pub fn rule_ids() -> Vec<String> {
    let mut ids: Vec<String> = rule_files()
        .iter()
        .map(|(name, _)| name.strip_suffix(".md").unwrap_or(name).to_string())
        .collect();
    ids.sort();
    ids
}

fn rule_path(file_name: &str) -> String {
    format!("{GUARDRAILS_DIR}/{file_name}")
}

pub struct GuardrailsModule;

pub static MODULE: GuardrailsModule = GuardrailsModule;

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let mut wrote_paths: Vec<String> = Vec::new();
    for (file_name, content) in sorted_rule_files() {
        let rel_path = rule_path(file_name);
        ctx.artifacts.write(&rel_path, &content)?;
        wrote_paths.push(rel_path);
    }
    Ok(ModuleResult {
        status: ModuleStatus::Applied,
        findings: vec![Finding::ok(format!(
            "wrote {} secure-coding rules to {GUARDRAILS_DIR}/",
            wrote_paths.len()
        ))],
        wrote_paths,
    })
}

impl AdeModule for GuardrailsModule {
    fn id(&self) -> &'static str {
        "guardrails"
    }
    fn title(&self) -> &'static str {
        "Secure-Coding Guardrails"
    }
    fn category(&self) -> &'static str {
        "security"
    }
    fn spec(&self) -> &'static str {
        "Secure-by-default coding guardrails"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "guardrails",
            title: "Secure-Coding Guardrails",
            content: [
                "The rules in `.ade/guardrails/` are BINDING for all generated code — read and follow them before writing or modifying code.",
                "- Rule set: `input-validation`, `injection`, `secrets-handling`, `authn-authz`, `crypto`, `error-handling` (plus `dependencies`).",
                "- Each rule file declares `id`, `severity`, and `applies_to` in its frontmatter; a rule applies unless its `applies_to` globs exclude the file you are editing.",
                "- Security findings are fixed at code level, never suppressed — no scanner exclusions, lint suppressions, or severity downgrades in place of a code fix.",
                "- When a rule conflicts with a user request, surface the conflict instead of silently violating the rule.",
            ]
            .join("\n"),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let mut findings = vec![Finding {
            level: FindingLevel::Info,
            message:
                "Project CodeGuard (github.com/cosai-oasis/project-codeguard) is a ruleset framework, not a CLI — ADE ships a distilled built-in ruleset"
                    .to_string(),
            remediation: Some(format!(
                "vendor CodeGuard's full rules into {GUARDRAILS_DIR}/ to extend the built-in set"
            )),
        }];
        let codeguard = (ctx.which)("codeguard");
        findings.push(match codeguard {
            Some(_) => Finding::info(
                "codeguard CLI found on PATH — can be used alongside the built-in ruleset",
            ),
            None => Finding::info(
                "no codeguard CLI on PATH (none is required — the ruleset is file-based)",
            ),
        });
        findings
    }

    fn plan(&self, _ctx: &Ctx) -> Vec<PlannedAction> {
        sorted_rule_files()
            .iter()
            .map(|(file_name, _)| PlannedAction {
                kind: ActionKind::Write,
                path: Some(rule_path(file_name)),
                description: format!(
                    "write secure-coding rule {}",
                    file_name.strip_suffix(".md").unwrap_or(file_name)
                ),
            })
            .collect()
    }

    fn apply(&self, ctx: &Ctx) -> ModuleResult {
        match apply_inner(ctx) {
            Ok(result) => result,
            Err(err) => ModuleResult {
                status: ModuleStatus::Failed,
                findings: vec![Finding::error(format!("module threw: {err}"))],
                wrote_paths: vec![],
            },
        }
    }

    fn verify(&self, ctx: &Ctx) -> VerifyResult {
        let mut findings: Vec<Finding> = Vec::new();
        let mut ok = true;
        for (file_name, _) in sorted_rule_files() {
            let rel_path = rule_path(file_name);
            let text = match read_if_exists(&ctx.target_dir.join(&rel_path)) {
                Some(text) => text,
                None => {
                    ok = false;
                    findings.push(Finding::error_with(
                        format!("{rel_path} missing"),
                        "run `ade apply`",
                    ));
                    continue;
                }
            };
            let parsed = match parse_rule_frontmatter(&text) {
                Some(parsed) => parsed,
                None => {
                    ok = false;
                    findings.push(Finding::error_with(
                        format!(
                            "{rel_path} has missing or malformed frontmatter (id/severity/applies_to)"
                        ),
                        "run `ade apply` to regenerate",
                    ));
                    continue;
                }
            };
            let expected_id = file_name.strip_suffix(".md").unwrap_or(file_name);
            if parsed.id != expected_id {
                ok = false;
                findings.push(Finding::error_with(
                    format!(
                        "{rel_path} frontmatter id \"{}\" does not match file name",
                        parsed.id
                    ),
                    "run `ade apply` to regenerate",
                ));
                continue;
            }
            findings.push(Finding::ok(format!(
                "{rel_path} present with valid frontmatter"
            )));
        }
        VerifyResult { ok, findings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::sha256_hex;
    use crate::testutil::{fake_which, make_temp_dir, make_test_ctx, TestCtxOptions};
    use std::path::Path;

    const REQUIRED_FILES: [&str; 6] = [
        "input-validation.md",
        "injection.md",
        "secrets-handling.md",
        "authn-authz.md",
        "crypto.md",
        "error-handling.md",
    ];

    fn read_rule(dir: &Path, file_name: &str) -> String {
        std::fs::read_to_string(dir.join(GUARDRAILS_DIR).join(file_name)).unwrap()
    }

    fn contains_timestamp(text: &str) -> bool {
        let bytes = text.as_bytes();
        let digit = |index: usize| {
            bytes
                .get(index)
                .map(|byte| byte.is_ascii_digit())
                .unwrap_or(false)
        };
        (0..bytes.len()).any(|i| {
            digit(i)
                && digit(i + 1)
                && digit(i + 2)
                && digit(i + 3)
                && bytes.get(i + 4) == Some(&b'-')
                && digit(i + 5)
                && digit(i + 6)
                && bytes.get(i + 7) == Some(&b'-')
                && digit(i + 8)
                && digit(i + 9)
                && bytes.get(i + 10) == Some(&b'T')
                && digit(i + 11)
                && digit(i + 12)
                && bytes.get(i + 13) == Some(&b':')
                && digit(i + 14)
                && digit(i + 15)
        })
    }

    #[test]
    fn isc_59_apply_writes_required_rule_files_with_real_do_dont_content() {
        let dir = make_temp_dir("guardrails-59");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(rule_files().len() >= 6);
        for file_name in REQUIRED_FILES {
            let rel_path = format!("{GUARDRAILS_DIR}/{file_name}");
            assert!(result.wrote_paths.contains(&rel_path));
            let text = read_rule(&dir, file_name);
            assert!(text.contains("DO:"));
            assert!(text.contains("DON'T:"));
            // Real content, not placeholders: ~15-40 lines of body after frontmatter.
            let body_lines = text
                .split('\n')
                .skip(5)
                .filter(|line| !line.trim().is_empty())
                .count();
            assert!(body_lines >= 15, "{file_name}: {body_lines} body lines");
            assert!(body_lines <= 40, "{file_name}: {body_lines} body lines");
            let lower = text.to_lowercase();
            assert!(!lower.contains("lorem"));
            assert!(!lower.contains("tbd"));
            assert!(!lower.contains("todo"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_59_injection_rule_concretely_covers_sql_command_path_and_xss() {
        let dir = make_temp_dir("guardrails-inj");
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let text = read_rule(&dir, "injection.md");
        assert!(text.contains("parameterized"));
        assert!(text.to_lowercase().contains("command"));
        assert!(text.to_lowercase().contains("path"));
        assert!(text.contains("XSS"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_62_every_rule_file_starts_with_parseable_frontmatter() {
        let dir = make_temp_dir("guardrails-62");
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        for (file_name, _) in rule_files() {
            let text = read_rule(&dir, file_name);
            assert!(text.starts_with("---\n"));
            let parsed = parse_rule_frontmatter(&text).expect("frontmatter must parse");
            assert_eq!(parsed.id, file_name.strip_suffix(".md").unwrap());
            assert!(super::is_kebab_id(&parsed.id));
            assert!(["critical", "high", "medium"].contains(&parsed.severity.as_str()));
            assert!(!parsed.applies_to.is_empty());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_62_parse_rule_frontmatter_rejects_malformed_frontmatter() {
        assert_eq!(parse_rule_frontmatter("# no frontmatter\n"), None);
        // unterminated
        assert_eq!(parse_rule_frontmatter("---\nid: x\nseverity: high\n"), None);
        assert_eq!(
            parse_rule_frontmatter("---\nid: Not-Kebab\nseverity: high\napplies_to: [all]\n---\n"),
            None
        );
        assert_eq!(
            parse_rule_frontmatter("---\nid: x\nseverity: nuclear\napplies_to: [all]\n---\n"),
            None
        );
        assert_eq!(
            parse_rule_frontmatter("---\nid: x\nseverity: high\napplies_to: []\n---\n"),
            None
        );
        // not an array
        assert_eq!(
            parse_rule_frontmatter("---\nid: x\nseverity: high\napplies_to: all\n---\n"),
            None
        );
        // missing applies_to
        assert_eq!(
            parse_rule_frontmatter("---\nid: x\nseverity: high\n---\n"),
            None
        );
        let ok = parse_rule_frontmatter(
            "---\nid: my-rule\nseverity: medium\napplies_to: [\"*.ts\", *.py]\n---\nbody\n",
        );
        assert_eq!(
            ok,
            Some(RuleFrontmatter {
                id: "my-rule".to_string(),
                severity: "medium".to_string(),
                applies_to: vec!["*.ts".to_string(), "*.py".to_string()],
            })
        );
    }

    #[test]
    fn isc_60_instruction_block_binds_ruleset_lists_rule_ids_and_forbids_suppression() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        let block = &blocks[0];
        assert!(block.content.contains("BINDING"));
        assert!(block.content.contains(".ade/guardrails/"));
        for id in [
            "input-validation",
            "injection",
            "secrets-handling",
            "authn-authz",
            "crypto",
            "error-handling",
        ] {
            assert!(block.content.contains(id), "missing {id}");
        }
        assert!(block
            .content
            .contains("fixed at code level, never suppressed"));
    }

    #[test]
    fn isc_61_detect_reports_codeguard_framework_status_as_info_cli_absent() {
        let dir = make_temp_dir("guardrails-61a");
        let findings = MODULE.detect(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(findings.len() >= 2);
        assert!(findings
            .iter()
            .all(|finding| finding.level == FindingLevel::Info));
        assert!(findings
            .iter()
            .any(|finding| finding.message.contains("cosai-oasis/project-codeguard")));
        assert!(findings.iter().any(|finding| finding
            .remediation
            .as_deref()
            .map(|remediation| remediation.contains(GUARDRAILS_DIR))
            .unwrap_or(false)));
        assert!(findings
            .iter()
            .any(|finding| finding.message.contains("no codeguard CLI")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_61_detect_notices_codeguard_cli_when_on_path() {
        let dir = make_temp_dir("guardrails-61b");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                which: Some(fake_which(&["codeguard"])),
                ..Default::default()
            },
        );
        let findings = MODULE.detect(&ctx);
        assert!(findings
            .iter()
            .any(|finding| finding.message.contains("codeguard CLI found")));
        assert!(findings
            .iter()
            .all(|finding| finding.level == FindingLevel::Info));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_116_plan_lists_one_write_per_rule_file_but_writes_nothing() {
        let dir = make_temp_dir("guardrails-116");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let actions = MODULE.plan(&ctx);
        assert_eq!(actions.len(), rule_files().len());
        assert!(actions
            .iter()
            .all(|action| action.kind == ActionKind::Write));
        assert!(!dir.join(GUARDRAILS_DIR).join("injection.md").exists());
        assert_eq!(ctx.artifacts.written().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_117_apply_is_idempotent_second_run_byte_identical() {
        let dir = make_temp_dir("guardrails-117");
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let first: Vec<(String, String)> = rule_files()
            .iter()
            .map(|(file_name, _)| {
                (
                    file_name.to_string(),
                    sha256_hex(&read_rule(&dir, file_name)),
                )
            })
            .collect();
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        for (file_name, first_hash) in first {
            let second = sha256_hex(&read_rule(&dir, &file_name));
            assert_eq!(second, first_hash, "{file_name} changed across applies");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn determinism_no_timestamps_or_absolute_paths_in_generated_rules() {
        let dir = make_temp_dir("guardrails-det");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        let dir_text = dir.to_string_lossy().to_string();
        for (file_name, _) in rule_files() {
            let text = read_rule(&dir, file_name);
            assert!(!text.contains(&dir_text));
            assert!(!contains_timestamp(&text));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_passes_after_apply() {
        let dir = make_temp_dir("guardrails-vok");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        let result = MODULE.verify(&ctx);
        assert!(result.ok);
        assert!(result
            .findings
            .iter()
            .all(|finding| finding.level == FindingLevel::Ok));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_a_rule_file_is_deleted() {
        let dir = make_temp_dir("guardrails-vdel");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        std::fs::remove_file(dir.join(GUARDRAILS_DIR).join("crypto.md")).unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result.findings.iter().any(|finding| {
            finding.level == FindingLevel::Error
                && finding.message.contains("crypto.md")
                && finding
                    .remediation
                    .as_deref()
                    .map(|remediation| remediation.contains("ade apply"))
                    .unwrap_or(false)
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_frontmatter_is_tampered_invalid_severity() {
        let dir = make_temp_dir("guardrails-vsev");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        let path = dir.join(GUARDRAILS_DIR).join("injection.md");
        let tampered = std::fs::read_to_string(&path)
            .unwrap()
            .replace("severity: critical", "severity: whatever");
        std::fs::write(&path, tampered).unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Error
                && finding.message.contains("injection.md")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_frontmatter_id_diverges_from_file_name() {
        let dir = make_temp_dir("guardrails-vid");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        let path = dir.join(GUARDRAILS_DIR).join("crypto.md");
        let tampered = std::fs::read_to_string(&path)
            .unwrap()
            .replace("id: crypto", "id: something-else");
        std::fs::write(&path, tampered).unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.message.contains("does not match")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rule_ids_covers_the_six_required_rule_ids() {
        let ids = rule_ids();
        for id in [
            "input-validation",
            "injection",
            "secrets-handling",
            "authn-authz",
            "crypto",
            "error-handling",
        ] {
            assert!(ids.iter().any(|entry| entry == id), "missing {id}");
        }
    }
}
