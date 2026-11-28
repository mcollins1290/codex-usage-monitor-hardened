# Installation and personal acceptance

This branch supplies a portable executable, not an installer or updater. Do
not enable startup until completing personal acceptance.

## Install

1. Copy `target/release/codex-usage-hardened.exe` to a user-owned folder such
   as `%LOCALAPPDATA%\CodexUsageHardened\`.
2. Launch it manually once.
3. In Settings, import the private approval evidence file at
   `%LOCALAPPDATA%\CodexUsageMonitor\approval-evidence-0.172.0.json`, review
   the displayed identity and policy, then explicitly confirm approval.
4. Configure remaining-percentage display, appearance, monitor placement and
   refresh interval. The default refresh interval is five minutes and startup
   with Windows is disabled.

Approval binds the canonical path, exact SHA-256, stable version, reviewed
invocation policy, completed runtime evidence and explicit consent. The monitor
delegates authentication to official Codex and never reads credentials.

## Verify and operate

Compare five-hour and weekly remaining percentages and reset times with the
official Codex interface. Small changes are expected as time passes or ordinary
Codex use continues. Stale or unavailable states are explicit; cached values
remain after transient failures. Manual refresh is available from the controls.

If Codex is upgraded, moved or changed, approval fails closed. Do not reuse old
evidence; independently verify the new official executable first.

## Remove or roll back

1. Exit the monitor.
2. Disable Windows startup if it was enabled.
3. Revoke approval in Settings.
4. Delete the executable and `%APPDATA%\CodexUsageHardened\` settings if
   desired. No upstream Codex settings are migrated or removed.

To roll back, revoke approval, delete the monitor executable, and restore a
previous build only after verifying its own hash and evidence.

## Personal desktop acceptance checklist

- Taskbar placement and coexistence look correct on each intended monitor.
- Values, reset countdowns and warning colors are legible.
- Manual refresh works without duplicate processes.
- Low, critical, exhausted, stale and unavailable states are understandable.
- Sleep/resume and Explorer restart recover normally.
- Normal exit leaves no monitor or Codex child process.
- Startup remains disabled unless explicitly wanted.
