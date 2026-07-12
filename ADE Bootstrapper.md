# ADE Bootstrapper
**ADE Bootstrapper** is a toolkit for bootstrapping a secure, high-quality, and operationally trustworthy **Agentic Development Environment (ADE)** for AI coding harnesses such as **Claude Code, Codex, Antigravity, Cursor, Hermes, Pi, and OpenCode**. It is purpose-built for harness-based agentic software development workflows, not for custom agent frameworks or ADK-built agents.

It bootstraps an ADE with opinionated mechanisms and components for **quality, performance, security, safety, reproducibility, observability, governance, memory, and token efficiency**.
## Core components
- **Secure-by-default coding guardrails**, including secure code generation, secure planning/specification support, policy-driven code review, and automated vulnerability remediation workflows (e.g. Project CodeGuard).[1][2][3]
- **Software supply chain security** to ensure only trustworthy and benign dependencies can be used, including both traditional software dependencies and AI-native dependencies such as skills, plugins, MCP servers, instruction packs, and agent configuration artifacts.[2][1]
- **AI-native sandboxing**, including permission controls, secure credential injection, network controls, filesystem isolation, rollback/snapshots, and tamper-evident execution trails; more than a microVM and purpose-built for coding harnesses (e.g. nono).[4]
- **Codebase context management**, including automatically maintained code wiki/documentation, call graphs, dependency graphs, hierarchies, symbol tables, semantic indexes, and other continuously refreshed retrieval/context artifacts for coding harnesses. (e.g. Cocoindex and OpenWiki)
- **Opinionated performance and quality scaffolding**, including skills, plugins, workflows, hooks, templates, execution policies, and harness-specific conventions that strongly steer agent behavior toward consistent, high-quality, and trustworthy outcomes.
- **Network-syncable agent memory** so that agents across devices and environments can securely share persistent memory, preferences, project context, and prior decisions over time (e.g. Mem0 / OpenMemory MCP).[5][6]
- **Prompt injection and context poisoning defenses** for harness workflows, including protections against malicious instructions embedded in README files, docs, issues, comments, web content, dependencies, and other untrusted context sources before they can influence harness behavior.
- **Harness configuration governance**, including standardized and version-controlled management of files such as `CLAUDE.md`, `AGENTS.md`, `.cursorrules`, and similar harness instruction/configuration files, with translation or synchronization across supported harnesses to prevent drift and inconsistency.
- **Tamper-evident observability and audit logging**, including structured logs of prompts, tool invocations, file mutations, approvals, policy decisions, sandbox events, and relevant agent actions for traceability, forensics, and compliance.
- **Human-in-the-loop approval gates** for high-risk actions such as destructive shell commands, credential use, external network access, dependency installation, branch operations, pull request creation, merges, and production-affecting changes.
- **Secrets and credential hygiene**, including secure bootstrap-time secret provisioning, scoped credential access, secret leak prevention, environment scrubbing, and protection against accidental inclusion of secrets in code, logs, prompts, or commits. (e.g. TruffleHog with pre-commit hook, pre-commit package being installed if it isn't already, etc.)
- **Git and repository hygiene enforcement**, including safe defaults for branching, commit signing, commit/PR policies, path protections, pre-commit checks, protected-branch awareness, and controls that reduce the chance of agents damaging repository integrity. (e.g. using https://github.com/grcengineering/OCEAN to do this)
- **Cost and token budget governance**, including model routing controls, per-session and per-project token/cost limits, budget alerts, and policies to prevent runaway harness activity and uncontrolled spend.
- **Reproducible environment and lockfile management**, including pinned toolchain versions, pinned harness versions, deterministic bootstrap outputs, environment manifest generation, lockfile validation, and mechanisms that keep ADE behavior consistent across machines and sessions.
- **Lossless or minimally lossy token efficiency mechanisms**, including compression, filtering, deduplication, grouping, truncation, and reversible or semantically lossless transformation of high-volume context such as terminal output, logs, file listings, tool results, codebase context, retrieval results, and model responses, in order to reduce token spend and preserve effective context window capacity without materially degrading coding quality or agent reliability (e.g. RTK).[7][8][9]

## Design goals

The bootstrapper should be:

- **Opinionated** enough to provide secure and high-quality defaults out of the box.
- **Modular** enough that teams can enable, disable, or swap components.
- **Composable** enough to integrate best-in-class open-source projects rather than reimplement everything from scratch.
- **Harness-native** in its abstractions, meaning it should optimize for real coding harnesses and their config surfaces, tool boundaries, and workflows rather than generic agent runtimes.
- **Local-first and self-hostable where practical**, especially for sensitive memory, policy, audit, and security functions.
- **Secure-by-default**, with explicit opt-outs rather than insecure defaults with opt-ins.
- **Cross-platform** across macOS, Linux, Windows (native), and WSL where feasible.

## What it should provide

The bootstrapper should ideally provide:

- A **single installation/bootstrap flow** for standing up a fresh ADE.
- A **unified policy and configuration model** across supported harnesses.
- A **consistent integration layer** for sandboxing, guardrails, logging, memory, and token-efficiency tools.
- A **deterministic and reproducible setup experience** across machines and sessions.
- A **clear extension model** for adding future harnesses, policies, hooks, and integrations.
- A design optimized for **individual developers, power users, and small engineering teams** using harness-based AI coding tools in real repositories.

## Non-goals

Non-goals:

- Building a general-purpose agent framework.
- Building a custom multi-agent runtime or orchestration SDK.
- Building an ADK for creating bespoke agents from scratch.
- Replacing the coding harness itself.
- Replacing existing best-of-breed open-source tools when integration is the better path.

## Mental model

A useful mental model is:

> **ADE Bootstrapper** is to AI coding harnesses what a hardened, opinionated developer platform bootstrapper is to traditional software engineering: it installs, wires together, and governs the environment so the harness can operate with stronger security, safety, consistency, auditability, efficiency, and effectiveness by default.

## Implementation bias

The implementation philosophy should be:

- **Integrate before rebuilding**.
- **Prefer open standards and plain files** over opaque proprietary control planes.
- **Treat harness config, policy, memory, and logs as first-class artifacts**.
- **Push controls as close as possible to the boundary where risk or waste occurs**, such as the shell boundary, dependency boundary, prompt/context boundary, git boundary, and network boundary.
- **Prefer reversible and benchmark-validated efficiency mechanisms** over naive summarization, especially for token-efficiency features like CLI/output compression.[8][9]

Project CodeGuard is an open-source, model-agnostic framework for embedding secure-by-default practices into AI coding workflows, with rules, translators, and validators for popular coding agents. nono provides kernel-enforced, capability-based sandboxing with network filtering, atomic rollbacks, secret injection at the boundary, and tamper-evident auditing for terminal agents. RTK frames token efficiency as an infrastructure layer at the shell boundary by compressing common command output before it reaches the model, using filtering, grouping, truncation, and deduplication.[3][9][1][2][4][7][8]

Sources
[1] Project CodeGuard https://github.com/project-codeguard
[2] Announcing a New Framework for Securing AI-Generated ... https://blogs.cisco.com/ai/announcing-new-framework-securing-ai-generated-code
[3] Getting Started https://project-codeguard.org/getting-started/
[4] Sandbox for AI Agents — Kernel-Level Isolation | nono https://nono.sh/
[5] Introducing OpenMemory MCP - Mem0 https://mem0.ai/blog/introducing-openmemory-mcp
[6] OpenMemory - AI Memory MCP Server for Coding Agents - Mem0 https://mem0.ai/openmemory
[7] RTK: Cut Your AI Coding Bill by 80% With One CLI Tool https://dev.to/arshtechpro/how-rtk-reduces-llm-token-usage-for-ai-coding-agents-2kfd
[8] rtk: A Rust CLI Proxy That Cuts AI Agent Token Usage 60-90% https://themenonlab.blog/blog/rtk-cli-proxy-token-reduction-ai-agents
[9] What Is RTK and Why Token Efficiency Matters https://wavespeed.ai/blog/posts/what-is-rtk-token-efficiency/
[10] RTK CLI Proxy for Claude Code and LLMs - LinkedIn https://www.linkedin.com/posts/ajeetsraina_github-rtk-airtk-cli-proxy-that-reduces-activity-7470757360454705152-s0_x
[11] Stop Wasting Tokens: This CLI Proxy Cuts Claude Code Usage by 90% [2026] https://www.youtube.com/watch?v=mpmxsARsmcI
[12] Optimize LLM token spend with RTK CLI proxy https://www.linkedin.com/posts/lucas-garcia-rubio_github-rtk-airtk-cli-proxy-that-reduces-activity-7445190925066252288-XYU9
[13] RTK Review: Slash Your AI Coding Token Costs by 70% https://www.xiaoxinsoft.com/en/rtk-ai-token-optimizer-for-coding-agents
[14] I Only Compressed CLI Output, Yet Tokens Dropped by 80%? https://madplay.github.io/en/post/rtk-reduce-ai-coding-agent-token-usage
[15] GitHub - rtk-ai/rtk: CLI proxy that reduces LLM token consumption by 60-90% on common dev command... https://www.youtube.com/watch?v=lFhZsoiM6ww
[16] Owasp-Style Prompts https://community.cisco.com/t5/security-blogs/can-security-rules-make-ai-generated-code-safer/bc-p/5354397/highlight/true
[17] Project CodeGuard: Security Skills and Rules for AI Coding ... https://github.com/cosai-oasis/project-codeguard