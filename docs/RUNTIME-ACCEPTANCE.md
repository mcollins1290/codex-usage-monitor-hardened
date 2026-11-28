# Runtime acceptance checklist

Production app-server launch is enabled only through the completed fail-closed compatibility gate. This checklist records independent evidence and does not bypass that gate.

The reviewed invocation policy candidate is fixed in `src/app_server.rs` as
`app-server --listen stdio:// --disable plugins -c "mcp_servers={}"`.
The explicit empty-table override replaces configured MCP servers, and the
official `Plugins` feature is disabled.

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
$env:CODEX_VERIFY_INVOCATION = 'app-server --listen stdio:// --disable plugins -c "mcp_servers={}"'
cargo test --locked app_server::tests::externally_selected_codex_unauthenticated_exchange -- --ignored --nocapture
Remove-Item Env:CODEX_VERIFY_EXECUTABLE,Env:CODEX_VERIFY_SHA256,Env:CODEX_VERIFY_VERSION,Env:CODEX_VERIFY_INVOCATION
```

The entry point validates the canonical path and hash, creates a disposable
profile, uses the production creation-time Job Object and bounded readers,
sends only `initialize`, `initialized`, and `account/rateLimits/read`, expects
the sanitized unauthenticated response, and verifies reader completion and
profile cleanup. It does not approve an executable or create a compatibility
record.

For later T2–T6 runs, set `CODEX_VERIFY_CONFIG_FILE` to a separately created
fixture file. The test-only runner copies that bounded file into the synthetic
`CODEX_HOME` before launch; it never reads the user's normal configuration.
The 0.172.0 source uses `[mcp_servers.<name>]` with either `command`/`args` for
stdio or `url` for streamable HTTP, and requires `environment_id`; these types
are defined in `codex-rs/config/src/mcp_types.rs` (`McpServerConfig` and
`McpServerTransportConfig`) at source commit `3d86e43b075adeba2193ec5c22ac92f55afd82e6`.
Hooks use top-level event tables such as `[hooks]` with `SessionStart` or
`PreToolUse` matcher arrays and tagged `type = "command"` handlers, as defined
in `codex-rs/config/src/hook_config.rs` and `codex-rs/protocol/src/hooks.rs`.
Plugins are represented by `[plugins._default]` and exact plugin keys in
`config.toml`, while installed plugin metadata lives under the synthetic
`CODEX_HOME` plugin cache; the version-matched CLI tests in
`codex-rs/cli/tests/plugin_cli.rs` define the manifest layout. These references
are preparation evidence, not proof that app-server activates an integration
during an account-only exchange.

On the approved local CLI 0.172.0 executable, the exact reviewed invocation
completed authenticated quota retrieval with clean bounded shutdown. The
sanitized observation was 89.000% remaining for the five-hour window and
48.000% remaining for the weekly window; both reset timestamps were returned,
and the genuine config.toml hash was unchanged.

## Unauthenticated official-runtime batch results

The authorized batch used the pinned 0.172.0 identity and stayed below the
20-invocation limit. T1 passed. A recognized-looking stdio MCP fixture and a
required-stdio control both completed the fixed protocol without a marker; an
invalid-config control also completed it. Because the observed behavior did
not demonstrate that Codex recognized the fixture configuration, these T2
results are `INCONCLUSIVE`, not proof of suppression. A loopback HTTP MCP
fixture likewise produced no connection, but its recognition was not
established, so T4 is `INCONCLUSIVE`. T3 and T5 were not activated because
doing so safely would require a turn or a version-specific plugin activation
path not demonstrated by this batch; T6 remains `INCONCLUSIVE` because the
loader's effective precedence was not observable through the fixed account-only
exchange. T7 repeated the fixed exchange with identity revalidation and
bounded cleanup; all completed repetitions passed. T8 process/filesystem and
loopback positive controls passed independently.

Observed official-runtime network scope was limited to the loopback listener;
the available observers do not establish comprehensive external-network or
filesystem access absence. No prohibited activity, inference, thread, turn, or
credential access was observed. The compatibility record authorizes polling
only for this exact identity and invocation policy; changed, moved, incomplete,
or revoked identities remain blocked.

## Authenticated official-runtime acceptance

The approved 0.172.0 executable was run once through the production
`read_rate_limits` path using the reviewed invocation. The process returned
both Codex quota windows, completed bounded cleanup, and left the genuine
`config.toml` content hash unchanged. No authentication token, raw response,
account identifier, thread, turn, or inference request was accessed or logged.

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
