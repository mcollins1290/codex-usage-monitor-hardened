# Codex Usage installation model

Codex Usage remains a single native Windows executable. Installation only places the executable and an uninstall helper in a stable per-user directory; it does not add a runtime, service, driver, telemetry component, or machine-wide dependency.

## Installation status

No installer or release package is part of this development baseline. Any
future installation procedure must be separately reviewed, use a pinned local
build, remain per-user, request no elevation, and must not download or replace
the executable automatically.

## Portable mode

`codex-usage.exe` can be run from any user-writable directory without installation. Portable mode uses the isolated `%APPDATA%\CodexUsageHardened\settings.json` settings path and has no in-app update mechanism.

## Settings and startup behavior

- Upgrades preserve `%APPDATA%\CodexUsage\settings.json`.
- Normal uninstall preserves settings so a later reinstall restores preferences.
- `uninstall.ps1 -RemoveSettings` explicitly deletes the settings directory.
- The installer does not enable startup automatically. If startup was already enabled, installation preserves that choice and updates the registry value to the stable installed executable. Users otherwise control startup under **Settings → General → Start with Windows**.
- If Windows has disabled the startup entry in **Settings → Apps → Startup**, Codex Usage detects the blocked state and shows an **Open Startup apps** button on **Settings → General** so you can re-enable it directly in Windows Settings.
- Uninstall removes the `CodexUsage` startup entry because its executable no longer exists.

## Managed installations

The hardened build is Codex-only. It does not read credential files, use direct HTTP clients, invoke model inference, run secondary providers, expose an MCP server, or perform automatic update checks. Authentication and network access are delegated to the approved local Codex executable through its app-server interface.

For an offline, managed installation, supply a trusted local executable and its approved SHA256.

```powershell
.\install.ps1 -Managed -SourcePath .\codex-usage.exe -ExpectedSha256 <approved-sha256> -NoLaunch
```

The installer saves the hardened settings path and approved Codex executable identity before launch. Other settings are copied from the existing JSON object. For an organization-wide policy, administrators can place a JSON object at `%ProgramData%\CodexUsage\policy.json` and restrict writes to authorized administrators.

```json
{}
```

The application does not use the policy file to enable network or update behavior; an unreadable or invalid policy is treated conservatively.
