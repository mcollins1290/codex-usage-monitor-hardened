# Final CI readiness — current candidate audit

Status: **NO GO — not ready for final Windows CI**.

This document records the current branch state after the latest local review. It
is an evidence register for the present candidate, not a claim of production
acceptance.

The structured evidence companion is
[`docs/verification/ci-readiness-checklist.json`](verification/ci-readiness-checklist.json).
It records the current evidence boundary and does not represent deferred
Windows or authenticated-runtime acceptance as passed.
The item-level dispositions are in
[`docs/verification/checklist-dispositions.json`](verification/checklist-dispositions.json).

## Candidate identity

- Branch: `feature/security-remediation`
- Starting reviewed remote HEAD: `858cd6dc61f5a67444788a3edceda9a444db3721`
- Current reviewed implementation baseline: `631b888284cd83141744022d2cb94a377e4123aa`
- Subsequent evidence-register identity updates are documentation-only commits.
- Original handover: `168885d0b8616989834e83d38d5899737600776a`
- Windows CI remains manual-only; no CI run was triggered by this review.
- Authenticated app-server launch remains unconditionally frozen in
  `src/app_server.rs` at `require_startup_acceptance()`.

## Verification commands

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed after formatting changes |
| `cargo test --locked` | Exit 0; 86 passed, 0 failed, 4 ignored |
| `cargo build --release --locked` | Exit 0; optimized build passed with no compiler warnings |
| `cargo clippy --locked --all-targets` | Exit 0; no warnings |
| `cargo audit --json` | Exit 0; 0 vulnerabilities; advisory DB `c5279d0b0c9e8b70df666eb6d1edbabcfd1176bd` |
| `cargo deny check` | Exit 0; advisories, bans, licenses and sources passed; duplicate `syn` warning remains |
| Windows CI | Not executed by authorization |

The manual Windows workflow also runs locked clippy with warnings denied. RustSec
and cargo-deny remain documented local acceptance commands because the workflow
does not install third-party audit binaries.

## Verified implementation

- Fixed app-server protocol ordering is implemented in `src/app_server.rs`:
  initialization is awaited before `initialized` and rate-limit retrieval.
- The permitted method set is fixed to `initialize`, `initialized` and
  `account/rateLimits/read`.
- Output frames, total output, stderr and queue behavior are bounded.
- Creation-time Job Object containment, kill-on-close and active-process limits
  are implemented with no uncontained fallback.
- Executable path, canonicalization, SHA-256 and stable version checks are
  implemented.
- Compatibility evidence is schema-versioned and bound to path, hash, version,
  release identity and invocation policy.
- Imported evidence cannot grant its own consent.
- The production launch freeze is unconditional and remains active.
- The local Windows test run exercised the production app-server protocol and
  containment paths: 86 passed, 0 failed, 4 ignored.

## Blocking findings

1. The production Codex-only refactor is implemented and locally verified, but
   Windows CI has not yet run against the current candidate.
2. Some generic service-oriented UI helper names and historical documentation
   remain; source inspection confirms they do not select or execute a secondary
   provider.
3. `cargo deny check` reports a non-security duplicate `syn` version warning.
4. T2–T8 startup side-effect verification and authenticated runtime acceptance
   remain external acceptance gates. The successful T1 protocol exchange does
   not prove absence of plugin, MCP or hook side effects.
5. Windows CI has not yet run against the current branch after the latest
   remediation commits.

## Checklist disposition summary

| Area | Status | Evidence |
| --- | --- | --- |
| Repository controls | IMPLEMENTED — CI PENDING | Git history, clean local branch, manual-only workflow |
| Codex-only architecture | IMPLEMENTED — CI PENDING | `src/codex_poll.rs`, `src/models.rs`, `src/window.rs`, `src/codex_icon.rs`; residual-reference audit |
| Quota presentation and scheduling | IMPLEMENTED — CI PENDING | `src/codex_poll.rs`, `src/quota_refresh.rs`, local 86-test run |
| Identity and compatibility | IMPLEMENTED — CI PENDING | `src/app_server.rs` tests; Windows CI pending |
| Protocol handling | PASS — VERIFIED locally | `src/app_server.rs` protocol tests; 86-test run |
| Process containment | IMPLEMENTED — CI PENDING | Windows fixture tests pass locally; CI rerun pending |
| Startup security | EXTERNAL ACCEPTANCE PENDING | `docs/RUNTIME-ACCEPTANCE.md` |
| Launch freeze | PASS — VERIFIED locally | `require_startup_acceptance()` and regression tests |
| Dependency security | IMPLEMENTED — CI PENDING | `cargo audit --json` and `cargo deny check` passed locally; duplicate `syn` warning remains |
| Documentation | IMPLEMENTED — CI PENDING | Status and acceptance documents; final consistency review pending |
| Final candidate acceptance | IMPLEMENTED — CI PENDING | Local checks pass; Windows CI and runtime acceptance remain |

## Required next work

- Complete the item-level A–T evidence register for the final candidate.
- Run the manually dispatched Windows workflow on the final candidate only.
- Keep the authenticated launch freeze until independent runtime acceptance.

## Final Windows CI procedure

After reviewing the candidate commit locally, dispatch the workflow manually
from GitHub Actions for `feature/security-remediation`:

```text
Actions → Windows build and tests → Run workflow → feature/security-remediation
```

The workflow must run on `windows-2022` with the pinned Rust toolchain and
execute, in order, formatting verification, locked tests, locked Clippy with
warnings denied, and the locked optimized build. A failed step blocks candidate
acceptance; do not bypass or weaken the failed check. RustSec and cargo-deny
must be rerun locally against the exact candidate lockfile if their results are
needed for final acceptance.

Only after final acceptance is separately approved should automatic `push` and
`pull_request` triggers be restored in `.github/workflows/ci.yml`. Review the
workflow diff, dispatch the restored workflow on a disposable branch if needed,
and preserve least-privilege permissions before merging any branch.

No installation, release, merge, tag, remote setting change or authenticated
launch was performed.
