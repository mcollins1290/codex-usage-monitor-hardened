# Codex-only hardening implementation plan

This plan is for the `feature/codex-only-hardening` branch. It preserves the
upstream MIT license and attribution and assumes local builds from a reviewed,
pinned commit. No release publication or automatic update mechanism is part of
the plan.

## Milestones

1. **Trust boundary and process client**
   - Keep the approved absolute Codex executable path, expected version, and
     SHA-256 in user settings.
   - Validate path canonicalization, file identity, and version before every
     launch.
   - Launch directly with `Command`; never use `cmd.exe`, PowerShell, WSL, or a
     shell fallback.
   - Send only `initialize`, `initialized`, and
     `account/rateLimits/read` JSON-RPC messages.
   - Resolve the documented app-server startup behavior for MCP, plugins, and
     hooks before permitting production use. If suppression cannot be proven,
     fail closed or require explicit user approval.

2. **Quota and lifecycle behavior**
   - Use one short-lived app-server process per refresh initially.
   - Enforce five-minute default polling, no overlapping polls, bounded retry,
     bounded request/startup timeouts, and deterministic child cleanup.
   - Preserve both five-hour and weekly windows, reset times, missing-window
     handling, stale markers, manual refresh, and taskbar rendering.

3. **Surface reduction**
   - Remove legacy provider data models, scheduler branches, icons, settings,
     localization, and tests that are not used by Codex mode.
   - Remove dormant updater code, update-policy state, local MCP remnants, and
     inference or credential-watch paths.
   - Retain only taskbar UI, appearance customization, user-level startup, and
     Codex quota presentation.

4. **Dependency and build review**
   - Keep `Cargo.lock` authoritative and build with `--locked`.
   - Review the final dependency graph for removed HTTP, TLS, updater, MCP, and
     provider-specific dependencies.
   - Run static source scans for credential-file access, network clients,
     shell execution, listeners, telemetry, and automatic downloads.

5. **Independent verification**
   - Review the final diff from the pinned upstream baseline.
   - Re-run formatting, locked offline tests, static security scans, and
     identity/protocol/lifecycle tests.
   - Verify that the reviewed source commit, local build inputs, executable
     identity policy, and runtime verification results are recorded without
     personal paths, credentials, account identifiers, or quota values.

## Acceptance criteria

### Credential isolation

- No source path reads, parses, logs, copies, or writes Codex or ChatGPT
  credential files, browser cookies, Windows Credential Manager data, access
  tokens, refresh tokens, or account identifiers.
- The monitor cannot refresh, replace, or delete credentials.
- Authentication and network access are performed only by the approved local
  Codex executable.

### Executable identity and process safety

- A refresh succeeds only when the configured path is absolute, canonicalizes
  consistently, has the approved version, and matches the approved SHA-256.
- Missing, replaced, reparse-point-changed, or hash-mismatched executables fail
  closed; no PATH discovery or replacement trust is attempted.
- Process creation uses direct arguments and no shell wrapper.
- Startup and request timeouts are bounded; stdout lines and total protocol
  messages are bounded; stderr is drained and never logged.
- The child is terminated and reaped on success, protocol error, timeout, and
  caller cancellation. A Windows Job Object with kill-on-close is used where
  available and its limitations are tested.

### Protocol restriction

- Automated tests assert the exact outbound method set and reject any thread,
  turn, model, login, logout, settings, or arbitrary method.
- No model inference is requested and no usage quota is consumed by the
  monitor's query itself.
- Responses are validated for the Codex bucket, both expected window durations,
  finite percentages, and reset timestamps. Missing windows are represented as
  unavailable rather than fabricated.

### Integration suppression

- The official app-server startup path is documented from source/protocol
  evidence.
- MCP servers, plugins, hooks, and unrelated integrations are either disabled
  through a supported, tested mechanism or the monitor refuses to operate when
  their startup cannot be ruled out.

### Polling and presentation

- Default polling is five minutes; adaptive polling is disabled.
- Polls never overlap. Retries use bounded exponential backoff and do not
  create a busy loop.
- Failed retrieval preserves the last value only when it is visibly marked
  stale; missing windows remain unavailable.
- The native Windows taskbar widget continues to show remaining percentages,
  reset countdowns/times, appearance settings, dark mode, scaling behavior,
  manual refresh, and optional user-level startup.

### Supply chain and verification

- Dependencies are locked and reviewed; no unpinned Git or runtime-downloaded
  dependency is accepted.
- CI does not publish releases or execute an updater.
- Final verification is performed from a clean local checkout at a pinned
  commit, with no credentials, personal paths, account identifiers, or quota
  responses committed.
