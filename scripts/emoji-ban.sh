#!/bin/bash
# Emoji are banned from every UI surface (owner directive 2026-08-01: emoji
# read as AI-generated). This gate fails on any emoji-range codepoint or the
# specific text-presentation status glyphs the redesign retired from UI code.
# Phase F removed the last sanctioned exception (the tray's U+25CF colored
# dot — now a drawn template NSImage), so the geometric status dots
# (U+25CF/U+25CB/U+25D0) are banned too.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v rg >/dev/null 2>&1; then
  echo "emoji-ban: ripgrep (rg) is required but not installed" >&2
  exit 2
fi

pattern='[\x{1F300}-\x{1FAFF}\x{2600}-\x{27BF}\x{2B00}-\x{2BFF}\x{FE0F}\x{25CF}\x{25CB}\x{25D0}]|⚠|✓|✗|✔|✖|●'
# Scope: the shipped Rust surface. src/ is the frozen TS reference
# implementation the parity gate compares against — its CLI glyphs are
# reference-fixture output, not product UI. Snapshot PNGs are excluded by
# glob (rg skips binaries regardless).
set +e
matches="$(rg --pcre2 -n "$pattern" crates/ scripts/bundle-apps.sh \
  --glob '!**/tests/snapshots/**' 2>&1)"
status=$?
set -e

case "$status" in
  0)
    echo "emoji-ban: FAIL — emoji-range codepoints found:" >&2
    printf '%s\n' "$matches" >&2
    exit 1
    ;;
  1)
    echo "emoji-ban: PASS (no emoji-range codepoints in source)"
    ;;
  *)
    echo "emoji-ban: rg failed (exit $status):" >&2
    printf '%s\n' "$matches" >&2
    exit "$status"
    ;;
esac
