# Development status

**Last updated:** 2026-11-26 (US Eastern)  
**Decision:** GO for development; NO GO for installation or authenticated polling.

## Objective

Deliver a lightweight Windows taskbar monitor showing Codex subscription five-hour and weekly remaining quota, reset countdowns, stale readings and manual refresh. Use only the official Codex app-server for quota retrieval; never read OAuth credentials or implement direct HTTP quota retrieval in the monitor. No inference, secondary providers, automatic updates, MCP listener or automatic executable trust.

## Branch and verification

- Active branch: `feature/security-remediation`
- Preserved handover: `168885d0b8616989834e83d38d5899737600776a`
- Last verified code commit: `e5ef1303c7407af566782950e6cc2b698d0eb2ec`
- [Windows CI run 45932097829](https://github.com/mcollins1290/codex-usage-monitor-hardened/actions/runs/45932097829): success; formatting, locked dependency tests and optimized build passed.
- Tests: 105 passed, 0 failed, 3 ignored (one child fixture invoked by parent tests).
- Release build: 37 compiler warnings; cleanup outstanding.
- Native child containment, descendant rejection, forced shutdown and compatibility regression tests passed. New isolated version probe code is pending CI and is not wired into production or exposed as a user command.

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
| M2 | Independent executable version verification | Isolated bounded version probe and strict parser implemented; CI and approval workflow integration pending |
| M3 | Initialization handshake | Implemented and unit tested |
| M4 | Tests exercise production protocol | Implemented and unit tested |
| M5 | Upgrade-friendly compatibility records | Record schema and identity binding tests passed; approval workflow outstanding |
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

## Next implementation milestone

1. Implement creation-time Windows process containment and bounded cleanup.
2. Add synthetic child-process lifecycle and failure regression tests.
3. Implement explicit executable compatibility records and fail-closed upgrade behavior.
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
