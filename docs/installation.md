# Installation status

**Not approved for installation or release.** The hardened branch remains under development; authenticated polling is intentionally disabled.

No installer, uninstaller, automatic update service or machine-wide policy is supplied. Do not use upstream binaries or installation scripts as substitutes for this project.

The intended executable is `codex-usage-hardened.exe`, with separate settings under `%APPDATA%\CodexUsageHardened\settings.json` and a distinct startup registry value `CodexUsageHardened`. No migration from the upstream application's settings or startup entry is intended.

A future installation procedure must be separately reviewed after final CI, dependency audit and runtime acceptance. See [development status](DEVELOPMENT-STATUS.md) and [runtime acceptance](RUNTIME-ACCEPTANCE.md).
