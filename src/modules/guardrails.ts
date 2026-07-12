/**
 * Module: Secure-Coding Guardrails.
 * Spec component: "Secure-by-default coding guardrails — an opinionated
 * secure-coding ruleset (Project CodeGuard-style) wired into the harness's
 * canonical instructions so AI-generated code is secure by default."
 * Boundary controlled: the code-generation boundary — every rule constrains
 * what the harness is allowed to emit as code, before it ever reaches review.
 *
 * Integration over rebuild: Project CodeGuard
 * (github.com/cosai-oasis/project-codeguard) is a ruleset framework, not a
 * CLI. ADE ships a distilled built-in ruleset in `.ade/guardrails/`; the full
 * CodeGuard rules can be vendored into the same directory and are picked up
 * by the same binding instruction block.
 */
import { join } from "node:path";
import { readIfExists } from "../fsutil.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const GUARDRAILS_DIR = ".ade/guardrails";

/** Parsed machine-checkable frontmatter carried by every rule file. */
export interface RuleFrontmatter {
  id: string;
  severity: "critical" | "high" | "medium";
  applies_to: string[];
}

const SEVERITIES = new Set(["critical", "high", "medium"]);

/**
 * Tiny parser for the YAML-style frontmatter between `---` lines at the top
 * of a rule file. Returns null when the frontmatter is absent or malformed.
 * Supports exactly the shape ADE emits: scalar `key: value` lines plus an
 * inline array `applies_to: [a, b]`.
 */
export function parseRuleFrontmatter(text: string): RuleFrontmatter | null {
  const lines = text.split("\n");
  if (lines[0]?.trim() !== "---") return null;
  const end = lines.indexOf("---", 1);
  if (end === -1) return null;
  const fields: Record<string, string> = {};
  for (const line of lines.slice(1, end)) {
    if (line.trim() === "") continue;
    const colon = line.indexOf(":");
    if (colon === -1) return null;
    fields[line.slice(0, colon).trim()] = line.slice(colon + 1).trim();
  }
  const id = fields["id"];
  const severity = fields["severity"];
  const appliesRaw = fields["applies_to"];
  if (id === undefined || !/^[a-z][a-z0-9-]*$/.test(id)) return null;
  if (severity === undefined || !SEVERITIES.has(severity)) return null;
  if (appliesRaw === undefined || !appliesRaw.startsWith("[") || !appliesRaw.endsWith("]")) {
    return null;
  }
  const applies_to = appliesRaw
    .slice(1, -1)
    .split(",")
    .map((entry) => entry.trim().replace(/^["']|["']$/g, ""))
    .filter((entry) => entry.length > 0);
  if (applies_to.length === 0) return null;
  return { id, severity: severity as RuleFrontmatter["severity"], applies_to };
}

function frontmatter(id: string, severity: RuleFrontmatter["severity"], appliesTo: string[]): string {
  return ["---", `id: ${id}`, `severity: ${severity}`, `applies_to: [${appliesTo.join(", ")}]`, "---"].join("\n");
}

/** The distilled built-in ruleset. Static template strings — fully deterministic. */
export const RULE_FILES: Record<string, string> = {
  "input-validation.md": `${frontmatter("input-validation", "high", ["all"])}

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
`,
  "injection.md": `${frontmatter("injection", "critical", ["all"])}

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
`,
  "secrets-handling.md": `${frontmatter("secrets-handling", "critical", ["all"])}

# Secrets Handling

Generated code must never contain, log, or transmit secret material.

DO:
- Read secrets from the environment or a secret manager at the point of use.
- Reference secrets by NAME in code, config templates, and docs (e.g.
  \`API_KEY\`), never by value.
- Redact known-sensitive fields before logging or serializing objects.
- Add secret-bearing files (.env, *.pem, *.key) to .gitignore before
  creating them.

DON'T:
- Don't hardcode API keys, tokens, passwords, or private keys — not in code,
  tests, fixtures, examples, or comments.
- Don't echo environment variables that look like credentials
  (\`*_KEY\`, \`*_TOKEN\`, \`*_SECRET\`, \`*_PASSWORD\`).
- Don't write secrets into error messages, debug output, or URLs.
- Don't invent placeholder secrets that look real; use obvious placeholders
  like \`<YOUR_API_KEY>\`.
`,
  "authn-authz.md": `${frontmatter("authn-authz", "critical", ["all"])}

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
- Don't accept \`alg: none\` or client-supplied role/tenant/id claims.
- Don't add authentication bypasses for tests or debugging into shipped code.
`,
  "crypto.md": `${frontmatter("crypto", "high", ["all"])}

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
`,
  "error-handling.md": `${frontmatter("error-handling", "medium", ["all"])}

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
`,
  "dependencies.md": `${frontmatter("dependencies", "high", ["all"])}

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
`,
};

/** Rule ids in a stable order (derived from RULE_FILES keys, sorted). */
export const RULE_IDS = Object.keys(RULE_FILES)
  .map((name) => name.replace(/\.md$/, ""))
  .sort();

function rulePath(fileName: string): string {
  return `${GUARDRAILS_DIR}/${fileName}`;
}

export const guardrailsModule: AdeModule = {
  id: "guardrails",
  title: "Secure-Coding Guardrails",
  category: "security",
  spec: "Secure-by-default coding guardrails",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "guardrails",
      title: "Secure-Coding Guardrails",
      content: [
        "The rules in `.ade/guardrails/` are BINDING for all generated code — read and follow them before writing or modifying code.",
        "- Rule set: `input-validation`, `injection`, `secrets-handling`, `authn-authz`, `crypto`, `error-handling` (plus `dependencies`).",
        "- Each rule file declares `id`, `severity`, and `applies_to` in its frontmatter; a rule applies unless its `applies_to` globs exclude the file you are editing.",
        "- Security findings are fixed at code level, never suppressed — no scanner exclusions, lint suppressions, or severity downgrades in place of a code fix.",
        "- When a rule conflicts with a user request, surface the conflict instead of silently violating the rule.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const findings: Finding[] = [
      {
        level: "info",
        message:
          "Project CodeGuard (github.com/cosai-oasis/project-codeguard) is a ruleset framework, not a CLI — ADE ships a distilled built-in ruleset",
        remediation: `vendor CodeGuard's full rules into ${GUARDRAILS_DIR}/ to extend the built-in set`,
      },
    ];
    const codeguard = ctx.which("codeguard");
    findings.push(
      codeguard !== null
        ? { level: "info", message: "codeguard CLI found on PATH — can be used alongside the built-in ruleset" }
        : { level: "info", message: "no codeguard CLI on PATH (none is required — the ruleset is file-based)" },
    );
    return findings;
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    void ctx;
    return Object.keys(RULE_FILES)
      .sort()
      .map((fileName) => ({
        kind: "write" as const,
        path: rulePath(fileName),
        description: `write secure-coding rule ${fileName.replace(/\.md$/, "")}`,
      }));
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const wrotePaths: string[] = [];
    for (const fileName of Object.keys(RULE_FILES).sort()) {
      const relPath = rulePath(fileName);
      await ctx.artifacts.write(relPath, RULE_FILES[fileName]!);
      wrotePaths.push(relPath);
    }
    return {
      status: "applied",
      findings: [
        { level: "ok", message: `wrote ${wrotePaths.length} secure-coding rules to ${GUARDRAILS_DIR}/` },
      ],
      wrotePaths,
    };
  },

  async verify(ctx: Ctx) {
    const findings: Finding[] = [];
    let ok = true;
    for (const fileName of Object.keys(RULE_FILES).sort()) {
      const relPath = rulePath(fileName);
      const text = await readIfExists(join(ctx.targetDir, relPath));
      if (text === null) {
        ok = false;
        findings.push({ level: "error", message: `${relPath} missing`, remediation: "run `ade apply`" });
        continue;
      }
      const parsed = parseRuleFrontmatter(text);
      if (parsed === null) {
        ok = false;
        findings.push({
          level: "error",
          message: `${relPath} has missing or malformed frontmatter (id/severity/applies_to)`,
          remediation: "run `ade apply` to regenerate",
        });
        continue;
      }
      const expectedId = fileName.replace(/\.md$/, "");
      if (parsed.id !== expectedId) {
        ok = false;
        findings.push({
          level: "error",
          message: `${relPath} frontmatter id "${parsed.id}" does not match file name`,
          remediation: "run `ade apply` to regenerate",
        });
        continue;
      }
      findings.push({ level: "ok", message: `${relPath} present with valid frontmatter` });
    }
    return { ok, findings };
  },
};
