# Development status

**Last updated:** 2026-11-26 (US Eastern)  
**Decision:** GO for development; NO GO for installation or authenticated polling.

## Objective

Deliver a lightweight Windows taskbar monitor showing Codex subscription five-hour and weekly remaining quota, reset countdowns, stale readings and manual refresh. Use only the official Codex app-server for quota retrieval; never read OAuth credentials or implement direct HTTP quota retrieval in the monitor. No inference, secondary providers, automatic updates, MCP listener or automatic executable trust.

## Branch and verification

- Active branch: `feature/security-remediation`
- Preserved handover: `168885d0b8616989834e83d38d5899737600776a`
- Last verified code commit: `6f58cbb97bb911e04cca79b8844301457bd027ac`
- [Windows CI run 45932899764](https://github.com/mcollins1290/codex-usage-monitor-hardened/actions/runs/45932899764): success; formatting, locked dependency tests and optimized build passed.
- Tests: 108 passed, 0 failed, 4 ignored (two child fixtures invoked by parent tests).
- Release build: 41 compiler warnings; cleanup outstanding.
- Native child containment, descendant rejection, forced shutdown, compatibility, isolated profile and negative version-probe tests passed. Schema 2 now binds evidence to the executable path as well as version/hash/policy; tests pending CI. No version probe or approval action is exposed to users yet.

**Temporary CI policy (owner authorized):** Windows CI is manual-only on this development branch until final verification. Intermediate commits are unverified; restore push and pull-request triggers and require a successful full Windows CI run before acceptance, merging or installation. All checks remain intact.

## Completed implementation (not necessarily runtime accepted)

- [x] Upstream security review, fork and independent review.
- [x] Remove active direct OAuth/HTTP quota path, inference fallback, updater and MCP listener.
- [x] Enforce unconditional authenticated app-server launch freeze.
- [x] Isolate primary settings, startup registry key, window classes, mutex and executable name.
- [x] Remove upstream settings/startup migration and inherited installer/uninstaller.
- [x] Wait for initialization response before sending quota request.
- [x] Bound stdout frames and output queue; drain stderr without logging contents.
- [x] Release UI state lock before polling.
- [x] Add protocol regression tests, pin Rust toolchain, pass Windows CI.

## Open security review findings

| ID | Finding | Status |
| --- | --- | --- |
| H1 | Output queue deadlock | Remediated in source; runtime tests outstanding |
| H2 | Unbounded output allocation | Remediated in source; runtime tests outstanding |
| H3 | Job Object assignment after process creation | Creation-time JOB_LIST and HANDLE_LIST implemented; synthetic lifecycle tests passed on Windows |
| H4 | Missing runtime security gate | Launch freeze enforced; accepted policy still required |
| H5 | Application identity isolation | Partial; audit remaining metadata, paths and scripts |
| M1 | Canonical path / hash-to-launch race | Read-only file sharing lock held and hash rechecked through launch; directory alias races still require review |
| M2 | Independent executable version verification | Isolated profile, strict parser and negative probe tests passed in CI; real Codex positive control and approval workflow integration pending |
| M3 | Initialization handshake | Implemented and unit tested |
| M4 | Tests exercise production protocol | Implemented and unit tested |
| M5 | Upgrade-friendly compatibility records | Schema 2 binds approval to path, version, hash and policy; new regression tests pending CI; external evidence collection and explicit approval UI outstanding |
| M6 | Child environment and working directory | Partial; authenticated configuration policy unresolved |
| M7 | Sanitized actionable error categories | Approval, executable change and authentication classifications implemented; UI review outstanding |
| M8 | UI lock during polling | Remediated in source |
| L1 | Residual provider/updater code | Open |
| L2 | Toolchain pinning | Implemented |

## App-server evidence

- CLI 0.166.0: earlier local authenticated quota feasibility succeeded; executable subsequently replaced.
- CLI 0.172.0: local synthetic verifier report confirms initialize, initialized and account/rateLimits/read; expected authentication-required error (-32600); clean exit in 508 ms. This was reported by local Codex, not independently reproduced in CI.
- Startup integration side effects, authenticated behavior on 0.172.0 and full T2–T8 synthetic tests remain unverified. Do not infer safety from protocol success alone.
- Never publish local executable paths, personal configuration, account data, credentials or private logs.

## Standing development mandate (owner authorized)

Complete autonomously as far as available evidence and permissions permit:

- Path-bound compatibility approval: schema 2 committed; final CI pending.
- User-facing approval workflow: read-only status entry added to Settings and tray menu; selection/import and explicit approval still pending.
- Legacy code removal: dead updater functions, updater/privacy preference machinery and helper argument handling removed; residual multi-provider renderer/scheduler remains.
- Dependency security audit: inventory and advisory assessment required; final locked build and audit required.
- Authenticated runtime verification: synthetic tests and acceptance harness can be developed autonomously; genuine local authenticated evidence and startup safety acceptance remain required. Never substitute fabricated or CI credentials.

Continue without intermediate automatic CI; restore full CI at final verification. No release, main merge, local install, or live launch gate change without acceptance.

## Next implementation milestone

1. Verify schema 2 compatibility and approval validation tests on Windows.
2. Complete version verification and approval workflow, including release provenance and fresh-profile ownership.
3. Review remaining executable alias races and output lifecycle failure cases.
4. Improve sanitized errors, remove legacy code and resolve warnings.
5. Audit dependencies, repeat Windows CI and review release candidate.
6. Complete synthetic startup verification and separately authorized local authenticated acceptance.
7. Obtain explicit approval before merge, release or installation.

## Acceptance and operating boundaries

- Every candidate must pass formatting, locked tests and Windows release compilation.
- Production launch remains blocked until startup policy and runtime verification are accepted.
- No automatic approval of new executable versions or hashes.
- No merge to main, release publication, repository security-setting changes or local installation without explicit approval.
- GitHub commits and CI are the durable development evidence. Update this document at each substantive milestone, recording verified results separately from pending work.
- Development in chat is turn-based; no background development is implied by this document.

## Links

- [Active branch](https://github.com/mcollins1290/codex-usage-monitor-hardened/tree/feature/security-remediation)
- [Commit history](https://github.com/mcollins1290/codex-usage-monitor-hardened/commits/feature/security-remediation/)
- [CI runs](https://github.com/mcollins1290/codex-usage-monitor-hardened/actions)
- [Changes since handover](https://github.com/mcollins1290/codex-usage-monitor-hardened/compare/168885d0b8616989834e83d38d5899737600776a...feature/security-remediation)
