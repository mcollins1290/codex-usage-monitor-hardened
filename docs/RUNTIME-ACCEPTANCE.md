# Runtime acceptance checklist

Production app-server launch remains frozen. This checklist does not authorize an authenticated launch or replace independent evidence.

## Reusable harness

The committed `verification/Verify-AppServer.ps1` file is a reporting shim; it
does not launch a process. The security-critical runner is the Rust test-only
implementation in `src/app_server.rs`, which reuses production containment,
bounded readers, protocol exchange and shutdown. Run the synthetic shared
runner with:

```powershell
cargo test --locked app_server::tests::shared_runner
```

The Rust runner validates synthetic process containment, bounded frame and total
output limits, deadlines, fixed protocol sequencing and cleanup. It is not a
production launcher, does not handle credentials, and does not execute T2–T8
against the official Codex executable;
those scenarios remain subject to official-runtime execution and independent
review; the synthetic fixture and observation controls are implemented below.

An explicitly selected external executable can be tested through the ignored
Rust test-only entry point (the production launch freeze is not consulted or
modified by this development-only path):

```powershell
$env:CODEX_VERIFY_EXECUTABLE = 'C:\path\to\approved\codex.exe'
$env:CODEX_VERIFY_SHA256 = '<64-hex SHA-256>'
$env:CODEX_VERIFY_VERSION = '<stable semver>'
cargo test --locked app_server::tests::externally_selected_codex_unauthenticated_exchange -- --ignored --nocapture
Remove-Item Env:CODEX_VERIFY_EXECUTABLE,Env:CODEX_VERIFY_SHA256,Env:CODEX_VERIFY_VERSION
```

The entry point validates the canonical path and hash, creates a disposable
profile, uses the production creation-time Job Object and bounded readers,
sends only `initialize`, `initialized`, and `account/rateLimits/read`, expects
the sanitized unauthenticated response, and verifies reader completion and
profile cleanup. It does not approve an executable or create a compatibility
record.

On the approved local CLI 0.172.0 executable, this T1 exchange completed with
the expected authentication-required result and clean bounded shutdown. This
is not evidence that configured MCP, hooks, plugins, or other startup
integrations are absent; T2–T6 require version-specific configuration fixtures
and observer coverage and remain separate acceptance cases.

The synthetic fixture suite is `verification/Verify-Fixtures.ps1`. It runs
without the official executable and covers positive process/filesystem/network
observation, T2–T6 fixture execution, normal and premature exits, timeout
termination and cleanup. Ten consecutive self-test repetitions passed; measured
elapsed times were approximately 3.1–3.3 seconds per run. Official Codex
execution remains a separate acceptance stage.

## Synthetic phase (no real account)

Record the exact executable canonical path, SHA-256, independently verified stable version and official release source revision locally. Never commit personal paths or private diagnostic logs. Use a fresh disposable profile and working directory, no real credentials, fixed stdio protocol, bounded output/timeouts, creation-time process containment and positive observation controls.

| Test | Acceptance evidence |
| --- | --- |
| T1 | Initialize response, then initialized notification, then account/rateLimits/read; expected unauthenticated response and bounded clean shutdown. |
| T2 | Synthetic stdio MCP command never starts. |
| T3 | Synthetic hook command never executes. |
| T4 | Synthetic loopback MCP endpoint receives no connection. |
| T5 | Synthetic plugin cannot trigger command execution or unrelated network activity. |
| T6 | Configuration override semantics demonstrated; no reliance on an empty table removing inherited settings. |
| T7 | Repeated bounded lifecycle, output floods and cleanup leave no descendants or leaked processes. |
| T8 | Positive controls demonstrate that process, filesystem and network observation actually detects test side effects. |

## Authenticated phase (requires separate local authorization and accepted startup policy)

Use the exact approved binary and invocation policy. Compare five-hour and weekly remaining quota and reset times with the official Codex UI. Observe process creation, network destinations and configuration writes. Only official Codex authentication and quota activity is acceptable; no inference, unrelated integrations or monitor credential reads. Record pass/fail and sanitized evidence, never tokens, account IDs or raw authentication responses. Repeat identity validation after any upgrade. Do not fabricate evidence or lift the production gate merely because an evidence file is imported.
