/**
 * Module: Quality & Performance Scaffolding.
 * Spec component: "Opinionated performance and quality scaffolding — PR
 * checklists, testing conventions, and commit conventions written into
 * `.ade/templates/` so every agent-authored change is held to the same
 * quality bar."
 * Boundary controlled: the quality boundary (inconsistent agent output) —
 * without shared conventions each harness session invents its own standards,
 * so review quality, test rigor, and commit hygiene drift run to run.
 */
import { join } from "node:path";
import { readIfExists } from "../fsutil.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const PR_CHECKLIST_PATH = ".ade/templates/pr-checklist.md";
export const TESTING_CONVENTIONS_PATH = ".ade/templates/testing-conventions.md";
export const COMMIT_CONVENTIONS_PATH = ".ade/templates/commit-conventions.md";

export const DEFAULT_COVERAGE_FLOOR_PCT = 95;

/** Template path → required h1 header, used by both apply and verify. */
export const TEMPLATE_HEADERS: Record<string, string> = {
  [PR_CHECKLIST_PATH]: "# PR Checklist",
  [TESTING_CONVENTIONS_PATH]: "# Testing Conventions",
  [COMMIT_CONVENTIONS_PATH]: "# Commit Conventions",
};

/** Validate scaffolding options; returns errors (empty = valid). */
export function validateScaffoldingOptions(options: Record<string, unknown>): string[] {
  const errors: string[] = [];
  const floor = options["coverageFloorPct"];
  if (
    floor !== undefined &&
    (typeof floor !== "number" || !Number.isFinite(floor) || floor <= 0 || floor > 100)
  ) {
    errors.push("options.coverageFloorPct must be a number between 0 (exclusive) and 100");
  }
  return errors;
}

function coverageFloor(options: Record<string, unknown>): number {
  const floor = options["coverageFloorPct"];
  return typeof floor === "number" ? floor : DEFAULT_COVERAGE_FLOOR_PCT;
}

export function prChecklistTemplate(floorPct: number): string {
  return `${TEMPLATE_HEADERS[PR_CHECKLIST_PATH]}

Work through every item before requesting review. An unchecked item is a
blocker, not a suggestion.

## Tests
- [ ] Tests added or updated for every behavior change in this PR.
- [ ] Full test suite passes locally — paste the command and exit code in the PR description.
- [ ] Coverage floor met (${floorPct}% line and function minimum) — the CI gate is not lowered.

## Security
- [ ] Security review done on every touched surface (inputs, auth, subprocess, file, and network boundaries).
- [ ] No secrets, tokens, or credentials in code, config, tests, or fixtures.
- [ ] No scanner findings suppressed — every true positive is fixed at the code level.

## Hygiene
- [ ] Docs updated for any changed behavior, options, or public interfaces.
- [ ] No generated artifacts, lockfile noise, or unrelated changes bundled in.
- [ ] PR description states WHAT changed, WHY, and how it was verified.
`;
}

export function testingConventionsTemplate(floorPct: number): string {
  return `${TEMPLATE_HEADERS[TESTING_CONVENTIONS_PATH]}

These conventions define what "tested" means in this repository.

## Test-first
- Write the failing test BEFORE the behavior change; the test defines done.
- A bug fix starts with a regression test that reproduces the bug.

## Meaningful assertions only
- Every test asserts an observable outcome that maps to an intended use case.
- Coverage padding is a defect: no assertion-free tests, no tests that only
  execute a line without verifying its effect, no contrived inputs whose only
  purpose is touching a branch.
- If reachable code cannot be covered by a meaningful test, treat the code as
  suspect — fix or delete it rather than padding around it.

## Coverage
- The coverage floor (${floorPct}% line and function) is a hard gate; never lower
  it to make a change pass — write the real test instead.

## Integration over mocks
- Prefer exercising real components (temp dirs, real files, real subprocess
  contracts) when the real thing is cheap; mock only true external boundaries.
`;
}

export function commitConventionsTemplate(): string {
  return `${TEMPLATE_HEADERS[COMMIT_CONVENTIONS_PATH]}

Every commit in this repository follows these rules.

## Format
- Conventional-commit style: \`type(scope): subject\` with types
  \`feat\`, \`fix\`, \`docs\`, \`test\`, \`refactor\`, \`perf\`, \`chore\`, \`ci\`, \`build\`.
- Subject is imperative mood ("add", not "added" or "adds") and at most 72 characters.
- Body explains WHY the change was made — the diff already shows what.

## Scope
- One logical change per commit; split unrelated changes into separate commits.
- Never mix a refactor with a behavior change in the same commit.

## Never commit
- Secrets, tokens, credentials, or private keys of any kind.
- Generated artifacts, build output, or local tooling state that belongs in
  \`.gitignore\`.
- Commented-out code or debugging leftovers.
`;
}

function moduleOptions(ctx: Ctx): Record<string, unknown> {
  return ctx.config.modules["scaffolding"]?.options ?? {};
}

/** All three templates, rendered from options — the single source for apply. */
function renderTemplates(options: Record<string, unknown>): Record<string, string> {
  const floor = coverageFloor(options);
  return {
    [PR_CHECKLIST_PATH]: prChecklistTemplate(floor),
    [TESTING_CONVENTIONS_PATH]: testingConventionsTemplate(floor),
    [COMMIT_CONVENTIONS_PATH]: commitConventionsTemplate(),
  };
}

export const scaffoldingModule: AdeModule = {
  id: "scaffolding",
  title: "Quality & Performance Scaffolding",
  category: "governance",
  spec: "Opinionated performance and quality scaffolding",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "scaffolding",
      title: "Quality & Performance Scaffolding",
      content: [
        "This project ships quality conventions in `.ade/templates/` — follow them on every change.",
        "- Test-first: write the failing test before the behavior change; a bug fix starts with a regression test.",
        "- The coverage floor is a HARD gate — never lower it to make a change pass; write the real test.",
        "- Fix security findings at the code level; NEVER suppress, exclude, or annotate them away.",
        "- Follow `.ade/templates/pr-checklist.md` before requesting review and `.ade/templates/commit-conventions.md` for every commit.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const errors = validateScaffoldingOptions(moduleOptions(ctx));
    if (errors.length > 0) {
      return errors.map((message) => ({ level: "error" as const, message }));
    }
    return [{ level: "ok", message: "scaffolding options valid" }];
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    void ctx;
    return [
      { kind: "write", path: PR_CHECKLIST_PATH, description: "write PR checklist template" },
      { kind: "write", path: TESTING_CONVENTIONS_PATH, description: "write testing conventions template" },
      { kind: "write", path: COMMIT_CONVENTIONS_PATH, description: "write commit conventions template" },
    ];
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const options = moduleOptions(ctx);
    const errors = validateScaffoldingOptions(options);
    if (errors.length > 0) {
      return {
        status: "failed",
        findings: errors.map((message) => ({ level: "error" as const, message })),
        wrotePaths: [],
      };
    }
    const templates = renderTemplates(options);
    const wrotePaths: string[] = [];
    for (const [relPath, content] of Object.entries(templates)) {
      await ctx.artifacts.write(relPath, content);
      wrotePaths.push(relPath);
    }
    return {
      status: "applied",
      findings: [{ level: "ok", message: `wrote ${wrotePaths.length} quality templates to .ade/templates/` }],
      wrotePaths,
    };
  },

  async verify(ctx: Ctx) {
    const findings: Finding[] = [];
    let ok = true;
    for (const [relPath, header] of Object.entries(TEMPLATE_HEADERS)) {
      const text = await readIfExists(join(ctx.targetDir, relPath));
      if (text === null || text.trim().length === 0) {
        ok = false;
        findings.push({
          level: "error",
          message: `${relPath} missing or empty`,
          remediation: "run `ade apply`",
        });
      } else if (!text.split("\n").some((line) => line.trim() === header)) {
        ok = false;
        findings.push({
          level: "error",
          message: `${relPath} missing its \`${header}\` heading`,
          remediation: "run `ade apply` to regenerate",
        });
      } else {
        findings.push({ level: "ok", message: `${relPath} present with expected heading` });
      }
    }
    return { ok, findings };
  },
};
