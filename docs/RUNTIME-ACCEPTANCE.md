# Runtime acceptance checklist

Production app-server launch remains frozen. This checklist does not authorize an authenticated launch or replace independent evidence.

## Reusable harness

The committed T1 harness is `verification/Verify-AppServer.ps1`. It requires an
explicit absolute executable path, expected SHA-256 and expected stable version;
it never discovers or approves replacements. Example usage, which must be run
only under the separately approved synthetic-test procedure, is:

```powershell
& .\verification\Verify-AppServer.ps1 `
  -ExecutablePath 'C:\Approved\codex.exe' `
  -ExpectedSha256 '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef' `
  -ExpectedVersion '0.172.0' `
  -OutputPath .\verification-result.json
```

The harness validates the executable identity, creates a disposable synthetic
profile, sends only the fixed T1 protocol sequence, bounds shutdown, and emits
structured sanitized results. It is intentionally not a production launcher,
does not handle credentials, and does not implement T2–T8 fixture observation;
those scenarios remain explicitly unimplemented until their observation and
containment requirements are added and independently reviewed.

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
