# Files

- [Codebase context management](context-management.md) - A two-tier model — an always-present structural codemap plus detected engines — and the honest limits of both, including a real defect this repository demonstrates.
- [Governance modules](governance.md) - Configuration governance, approval gates and cost governance — three declarative contracts, and an honest account of how little of it is mechanically enforced.
- [Secure-coding guardrails](guardrails.md) - Seven rule files with machine-checkable frontmatter, why verify re-parses them, and the vendoring seam that collides with the lockfile.
- [Prompt-injection defense](injection-defense.md) - A trust policy that classifies external content as read-only data, a heuristic scanner that cannot block, and the fail-open choice behind it.
- [The module contract](module-contract.md) - What the four lifecycle methods each guarantee, why registry order is load-bearing twice, and the integrate-never-install convention that shapes every module.
- [Observability and git hygiene](observability-and-git-hygiene.md) - Where the audit chain gets initialized and wired, and a module that writes a git contract it cannot enforce.
- [Reproducibility and token efficiency](reproducibility-and-token-efficiency.md) - Two modules at the tail of the registry with exactly opposite stances on environment drift, and neither one enforcing what its name suggests.
- [Sandboxing](sandbox.md) - A declarative policy that says honestly whether anything enforces it, and the one place ADE maps its own contract onto a coding agent's real permission system.
- [Scaffolding and agent memory](scaffolding-and-memory.md) - Three quality templates that are documents rather than gates, and a memory module whose MCP registration is a deliberate credential boundary.
- [Secrets and credential hygiene](secrets.md) - The scan-nothing invocation this project found and enforces against, how an existing git hook is chained rather than destroyed, and where the guarantee actually stops.
- [Supply chain security](supply-chain.md) - One policy artifact, a hardcoded registry allowlist, AI-native dependencies as first-class governed classes, and a lockfile heuristic worth knowing.
