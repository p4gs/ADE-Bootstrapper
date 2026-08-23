# Files

- [The tamper-evident audit chain](audit-chain.md) - How each entry hashes its predecessor, why the checkpoint must live outside the log, and the residual limit the project states rather than papers over.
- [Configuration and the module contract surface](configuration.md) - How ade.json is validated once in the core, why module and harness ids are a closed vocabulary, and the type-level choices that make generated output deterministic.
- [Detection and the environment layer](detection-and-environment.md) - How ADE resolves PATH so a Finder-launched app sees the same tools as a terminal, how tools and coding agents are detected, and why the CLI and desktop apps cannot disagree.
- [Determinism and atomic I/O](determinism-and-io.md) - Why stable_stringify is hand-rolled, why every artifact is written through a temp file and renamed, and how deep_merge and subtract_json make co-owned files reversible.
- [The generated artifact tree](generated-artifacts.md) - Every path ade init writes, classified as generated, user-owned or co-owned, with the module that owns it and the mechanism that protects it.
- [Coding-agent integration](harness-integration.md) - Seven agent adapters, which of them can actually enforce anything, and the single merge function every Claude-side JSON mutation passes through.
- [Instructions and managed blocks](instructions-and-managed-blocks.md) - How ADE owns exactly one region of your instruction files, proves that ownership with a content hash, and refuses rather than clobbers whenever it cannot prove it.
- [The lockfile and what verify actually checks](lockfile-and-verify.md) - Why ade.lock.json is timestamp-free, why its scope is path-derived rather than authorship-derived, and the unknown-file rule that catches planted artifacts.
- [The apply pipeline](pipeline.md) - How ade init, plan, apply and verify actually run — module ordering, fault isolation, the core-owned steps no module can disable, and what ok does and does not prove.
- [Withdrawing ADE from a repository](removal.md) - The refusal rule that governs removal, why it is lockfile-driven rather than path-driven, and how a chained git hook gets restored rather than orphaned.
