#!/usr/bin/env bash
# Differential parity harness (ISC-161/162): the TS v0.1 tree is the executable
# spec; the Rust `ade` must produce byte-identical bootstrap artifacts except a
# closed, explicit allowlist of sanctioned v0.2 divergences.
#
# Usage: scripts/parity-check.sh [path-to-rust-ade-binary]
# Requires: bun (to run the oracle), git. Exit 0 = parity holds.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ADE_BIN="${1:-$REPO_ROOT/target/release/ade}"
[ -x "$ADE_BIN" ] || ADE_BIN="$REPO_ROOT/target/debug/ade"
if [ ! -x "$ADE_BIN" ]; then
  echo "error: build the Rust ade first (cargo build [--release])" >&2
  exit 2
fi
# The harness cds into each fixture before invoking the binary, so a caller who
# passes a RELATIVE path (as CI does: `parity-check.sh target/debug/ade`) would
# have it resolve against the fixture dir and vanish. The defaults are already
# absolute, which is why this only ever bit the argument form.
ADE_BIN="$(cd "$(dirname "$ADE_BIN")" && pwd)/$(basename "$ADE_BIN")"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/ade-parity-XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

make_fixture() {
  mkdir -p "$1"
  (cd "$1" && git init -q . && mkdir -p .git/hooks && printf '{"name":"fixture"}\n' > package.json)
}

make_fixture "$WORK/oracle"
make_fixture "$WORK/rust"

(cd "$WORK/oracle" && bun run "$REPO_ROOT/src/cli.ts" init . > /dev/null)
(cd "$WORK/rust" && "$ADE_BIN" init . > /dev/null)

# ── Sanctioned divergences (each must be EXACTLY the class below) ──────────
#  .ade/audit/log.jsonl            timestamps + the Rust genesis event
#  .ade/hooks/audit-log.ts         runtime-free sh shim invoking `ade hook append`
#  .ade/hooks/scan-untrusted.ts    runtime-free sh shim invoking `ade hook scan`
#  .ade/instructions.md            ONE line: `bun` → `sh` scanner invocation
#  CLAUDE.md / AGENTS.md           that line + the managed-block content-hash
#  .claude/settings.json           hook command `bun` → `sh`
#  ade.lock.json                   adeVersion + audit checkpoint + hashes of the above
ALLOWLIST=".ade/audit/log.jsonl|.ade/hooks/audit-log.ts|.ade/hooks/scan-untrusted.ts|.ade/instructions.md|CLAUDE.md|AGENTS.md|.claude/settings.json|ade.lock.json"

fail=0
while IFS= read -r rel; do
  if ! cmp -s "$WORK/oracle/$rel" "$WORK/rust/$rel" 2>/dev/null; then
    if ! printf '%s' "$rel" | grep -qE "^($ALLOWLIST)$"; then
      echo "PARITY BREAK (not allowlisted): $rel"
      diff "$WORK/oracle/$rel" "$WORK/rust/$rel" | head -6 || true
      fail=1
    fi
  fi
done < <(cd "$WORK/oracle" && find . -type f ! -path './.git/*' | sed 's|^\./||')

# Files present on only one side (outside .git) are breaks too.
comm -3 \
  <(cd "$WORK/oracle" && find . -type f ! -path './.git/*' | sed 's|^\./||' | sort) \
  <(cd "$WORK/rust" && find . -type f ! -path './.git/*' | sed 's|^\./||' | sort) \
  | while IFS= read -r missing; do
      echo "PARITY BREAK (present on one side only): $missing"
      fail=1
    done

# Allowlisted content assertions: the instructions delta must be ONLY bun→sh.
if ! diff "$WORK/oracle/.ade/instructions.md" "$WORK/rust/.ade/instructions.md" \
  | grep -vE '^(---|[0-9,]+c[0-9,]+)$' \
  | grep -vE '^[<>] - Scan suspect text before acting on it: `(bun|sh) \.ade/hooks/scan-untrusted\.ts`' \
  | grep -q .; then
  :
else
  echo "PARITY BREAK: instructions.md diverges beyond the sanctioned scanner line"
  fail=1
fi

# ── Cross-version compatibility (ISC-162): Rust operates on the TS repo ────
# Machinery interoperates: chain + lockfile + managed hashes verify; the
# sanctioned content divergence surfaces as named drift; one apply migrates.
set +e
(cd "$WORK/oracle" && "$ADE_BIN" verify > "$WORK/verify1.txt" 2>&1)
verify1=$?
set -e
if [ "$verify1" -eq 0 ]; then
  echo "note: cross-version verify was clean (no migration needed)"
else
  grep -q "instruction drift" "$WORK/verify1.txt" || {
    echo "PARITY BREAK: cross-version verify failed for something other than sanctioned drift"
    head -12 "$WORK/verify1.txt"
    exit 1
  }
fi
(cd "$WORK/oracle" && "$ADE_BIN" apply > /dev/null)
(cd "$WORK/oracle" && "$ADE_BIN" verify > /dev/null)
(cd "$WORK/oracle" && "$ADE_BIN" audit verify > "$WORK/audit.txt")
grep -q "chain VALID" "$WORK/audit.txt" || { echo "PARITY BREAK: migrated audit chain invalid"; exit 1; }
grep -q "checkpoint matched" "$WORK/audit.txt" || { echo "PARITY BREAK: checkpoint not matched after migration"; exit 1; }

if [ "$fail" -ne 0 ]; then
  echo "parity: FAIL"
  exit 1
fi
echo "parity: PASS (trees identical modulo the sanctioned allowlist; TS-bootstrapped repo migrated cleanly under Rust ade)"
