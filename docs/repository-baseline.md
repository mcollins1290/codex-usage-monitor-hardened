# Hardened fork repository baseline

Recorded from the local checkout on 2026-11-26. This document is a
development baseline, not a release record.

## Repository identity

- Local path: `<repository-root>`
- Branch: `feature/codex-only-hardening`
- Fork remote (`origin`): `https://github.com/mcollins1290/codex-usage-monitor-hardened.git`
- Upstream remote: `https://github.com/upstream-ray/codex-usage-monitor.git`
- Reviewed upstream starting commit:
  `0f17e4a40945e949a9b83bbd58f3da1323c6ff41`
- Current local HEAD at record time:
  `ac1244bb66a097b5e1a88424eb83f917ea6f378f`

The fork was verified as the expected upstream repository before cloning. No
push, merge, release publication, or upstream modification is authorized by
this development process.

## Workflow review

The inherited release workflow was removed. The remaining `ci.yml` workflow:

- runs on pushes, pull requests, and manual dispatch;
- uses read-only contents permission;
- disables checkout credential persistence;
- pins the checkout action revision;
- runs formatting, locked tests, and a locked Windows release build;
- does not publish artifacts, create releases, or run an updater.

Future changes must preserve these properties. Any release or artifact
publishing workflow requires separate review and explicit approval.

## Local tool availability

Available without installing anything: Git, GitHub CLI, Rust, Cargo, rustfmt,
and rustup. Not available: MSVC `cl.exe`, `link.exe`, `msbuild.exe`,
`cargo-audit`, and `cargo-deny`. No missing tool was installed.

The local baseline was checked with `cargo check --locked --offline` and
`cargo test --locked --offline`; the current suite passes with 99 tests passed
and 2 ignored. The application itself has not been launched, and no repository
build script or installer has been executed.

## Remaining gates

- Complete removal of obsolete provider, update, and privacy-policy source
  structures.
- Establish a supported and tested way to suppress unnecessary official
  app-server MCP, plugin, and hook startup integrations, or keep the monitor
  fail-closed until that behavior is proven.
- Perform final locked dependency and independent source review before any
  build or installation proposal.
