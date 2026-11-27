# Final CI readiness — current candidate audit

Status: **NO GO — not ready for final Windows CI**.

This document records the current branch state after the latest local review. It
is an evidence register for the present candidate, not a claim of production
acceptance.

## Candidate identity

- Branch: `feature/security-remediation`
- Starting reviewed remote HEAD: `858cd6dc61f5a67444788a3edceda9a444db3721`
- Current local candidate: recorded after the final commit is created
- Original handover: `168885d0b8616989834e83d38d5899737600776a`
- Windows CI remains manual-only; no CI run was triggered by this review.
- Authenticated app-server launch remains unconditionally frozen in
  `src/app_server.rs` at `require_startup_acceptance()`.

## Verification commands

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed after formatting changes |
| `cargo test --locked` | Exit 0; 104 passed, 0 failed, 4 ignored |
| `cargo build --release --locked` | Exit 0; 23 compiler warnings remain |
| `cargo clippy --locked --all-targets` | Exit 0; dead-code and lint warnings remain |
| `cargo audit` | Not executed; tool unavailable |
| `cargo deny` | Not executed; tool unavailable |
| Windows CI | Not executed by authorization |

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
  containment paths: 104 passed, 0 failed, 4 ignored.

## Blocking findings

1. Production still retains generalized three-slot provider scheduling and
   provider-specific rendering/models/assets. `src/provider_poll.rs`,
   `src/models.rs`, `src/window.rs`, `src/provider_icons.rs` and localization
   modules contain Claude/Antigravity-era structures. These are not yet fully
   removed or justified for a Codex-only product.
2. Release compilation succeeds but emits 23 warnings, including unused
   version-probe functions, update/local-MCP localization fields and legacy
   provider error variants.
3. Automated RustSec and registry-yank checks were not executable because
   `cargo-audit` and `cargo-deny` are unavailable. The manual audit is recorded
   in `docs/DEPENDENCY-AUDIT.md` but is not equivalent to an automated pass.
4. T2–T8 startup side-effect verification and authenticated runtime acceptance
   remain external acceptance gates. The successful T1 protocol exchange does
   not prove absence of plugin, MCP or hook side effects.
5. Windows CI has not yet run against the current branch after the latest
   remediation commits.

## Checklist disposition summary

| Area | Status | Evidence |
| --- | --- | --- |
| Repository controls | IMPLEMENTED — CI PENDING | Git history, clean local branch, manual-only workflow |
| Codex-only architecture | FAIL / INCOMPLETE | Legacy production models and rendering remain |
| Quota presentation and scheduling | IMPLEMENTED — CI PENDING | Existing unit tests and `src/provider_poll.rs` |
| Identity and compatibility | IMPLEMENTED — CI PENDING | `src/app_server.rs` tests; Windows CI pending |
| Protocol handling | PASS — VERIFIED locally | `src/app_server.rs` protocol tests; 104-test run |
| Process containment | IMPLEMENTED — CI PENDING | Windows fixture tests pass locally; CI rerun pending |
| Startup security | EXTERNAL ACCEPTANCE PENDING | `docs/RUNTIME-ACCEPTANCE.md` |
| Launch freeze | PASS — VERIFIED locally | `require_startup_acceptance()` and regression tests |
| Dependency security | FAIL / INCOMPLETE | Manual review only; audit tools unavailable |
| Documentation | IMPLEMENTED — CI PENDING | Status and acceptance documents; final consistency review pending |
| Final candidate acceptance | FAIL / INCOMPLETE | Actionable legacy and warning cleanup remains |

## Required next work

- Remove or decisively isolate the remaining provider-specific production
  structures and assets.
- Remove unused localization, error and version-probe code or make its test-only
  role explicit without leaving production warnings.
- Perform the dependency audit in an environment with reviewed audit tooling.
- Reconcile the full item-level A–T checklist after those changes.
- Run the manually dispatched Windows workflow on the final candidate only.
- Keep the authenticated launch freeze until independent runtime acceptance.

No installation, release, merge, tag, remote setting change or authenticated
launch was performed.
