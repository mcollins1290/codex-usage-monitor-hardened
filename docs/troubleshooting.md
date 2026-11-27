# Troubleshooting Codex Usage

## Taskbar error labels

The hardened monitor reports app-server and quota-read failures without exposing authentication details:

| Simplified Chinese | Other languages | Meaning | Recommended action |
|---|---|---|---|
| `!` | `!` | Approved Codex executable is missing, changed, or unavailable | Verify the configured absolute path and SHA-256, then use the official Codex client to restore authentication if needed |
| `网络` | `NET` | Official Codex app-server or account service unavailable | Check connectivity, VPN, proxy, and firewall settings in the official Codex client |
| `限流` | `429` | Rate-limit data unavailable or request bounded by the monitor | Wait for the next scheduled poll or use manual refresh |
| `服务` | `5XX` | Unsupported or failed app-server response | Check the installed Codex version against the approved version |
| `错误` | `ERR` | Invalid or unsupported response | Enable diagnostics and inspect the log |

The monitor never reads or modifies credential files. Failed reads preserve the last value only as explicitly stale and use bounded retry behavior.

## Diagnostic log

Run:

```powershell
# Per-user installation
& "$env:LOCALAPPDATA\Programs\CodexUsage\codex-usage.exe" --diagnose

# Portable copy (from the folder containing codex-usage.exe)
.\codex-usage.exe --diagnose
```

The log is written to `%TEMP%\codex-usage.log`. It includes:

- application version and executable path
- approved Codex executable identity status and retry delay
- window creation, taskbar placement, and relaunch events

The log does not include access tokens, refresh tokens, credential file contents, or API response bodies.

## Updates

The hardened build has no automatic update or self-replacement mechanism. Review and install future builds separately from a pinned, reviewed source commit.

## Reset local position without deleting settings

Right-click the taskbar widget or tray icon, open **Settings → General**, and click **Reset position** under **Widget position**. Settings are stored at `%APPDATA%\CodexUsage\settings.json`.

## Start with Windows did not launch after sign-in

If **Start with Windows** was enabled but Codex Usage did not launch after signing in, check whether Windows disabled the entry in **Settings → Apps → Startup**. Open **Codex Usage → Settings → General**: when the entry is blocked by Windows, the **Start with Windows** row displays a warning and an **Open Startup apps** button that opens Windows Startup settings directly.

## Reinstall while preserving settings

Normal uninstall keeps the isolated settings file. Reinstalling restores the saved language, refresh interval, Codex executable identity, widget visibility, and taskbar position. Remove the settings directory only when a full reset is intended.
