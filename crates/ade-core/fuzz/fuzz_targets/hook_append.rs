#![no_main]
//! Fuzzes `hook_append`, the runtime-free `ade hook append` entry point that
//! consumes raw stdin from a harness's PostToolUse event — the one place in
//! this codebase that parses genuinely untrusted, externally-supplied text
//! (a tool call's own JSON, potentially shaped by whatever the tool touched).
//! `hook_append`'s own contract is "never blocks the harness, never panics
//! on malformed input" (see its doc comment) — this target holds it to
//! exactly that: any panic here is the real bug, not the fuzzer's problem.

use ade_core::hook::hook_append;
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

static SCRATCH: OnceLock<std::path::PathBuf> = OnceLock::new();

fuzz_target!(|data: &[u8]| {
    let Ok(stdin_text) = std::str::from_utf8(data) else {
        return;
    };

    // One shared scratch dir for the whole fuzz run (not per-input — cargo-fuzz
    // runs millions of inputs; a fresh tempdir per call would dominate runtime
    // with filesystem setup instead of exercising the parser). hook_append's
    // own writes are append-only to one file, so sharing is safe and mirrors
    // how a real, long-lived project directory actually gets used.
    let dir = SCRATCH.get_or_init(|| {
        let path = std::env::temp_dir().join("ade-core-fuzz-hook-append");
        std::fs::create_dir_all(&path).expect("scratch dir");
        path
    });

    hook_append(dir, stdin_text);
});
