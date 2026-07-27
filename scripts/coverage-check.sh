#!/usr/bin/env bash
# Rust coverage gate: 95% line AND 95% function over the product logic.
#
# The two UI crates are excluded on purpose. `ade-control-center` is an egui
# render loop and `ade-status` is an AppKit run loop + NSTimer; both are the
# structurally-untestable entry-point class. Every decision they display is
# computed in `ade-core::gui` (inventory/jobs/menubar/state), which IS covered —
# so excluding the shells does not exclude any judgement.
set -euo pipefail

IGNORE='crates/(ade-control-center|ade-status)/'

exec cargo llvm-cov --workspace \
  --ignore-filename-regex "$IGNORE" \
  --fail-under-lines 95 \
  --fail-under-functions 95 \
  "$@"
