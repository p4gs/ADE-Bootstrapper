# Security Policy

## Reporting a Vulnerability

Please report security vulnerabilities privately through GitHub's built-in
reporting flow rather than a public issue:

**[Report a vulnerability](https://github.com/p4gs/ADE-Bootstrapper/security/advisories/new)**

(Repository → Security tab → "Report a vulnerability". Private vulnerability
reporting is enabled for this repository.)

You should expect an initial response within 5 business days. Please include:

- A description of the vulnerability and its potential impact
- Steps to reproduce, or a proof-of-concept if you have one
- Affected version(s) / commit

## Supported Versions

ADE Bootstrapper is pre-1.0 (`0.2.x`). Security fixes land on `main` and the
latest tagged release; there is no separate LTS branch at this stage.

## Scope

In scope: the `ade` CLI, `ade-core`, `ade-control-center`, `ade-status`, and
the `.github/workflows/` CI/CD pipeline for this repository.

Out of scope: vulnerabilities in third-party dependencies — please report
those directly to the upstream project (and feel free to open a normal issue
here noting the advisory so we can track the dependency bump).
