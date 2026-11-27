[CmdletBinding()]
param([string] $OutputPath)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# This file is deliberately a reporting shim. The security-critical verifier
# is the Rust test-only runner in src/app_server.rs, which reuses the
# production ContainedChild, bounded readers, fixed exchange and shutdown.
# PowerShell must never provide a second process or protocol implementation.
$report = [ordered]@{
    schema_version = 2
    status = 'NOT EXECUTED'
    runner = 'Rust app_server shared verification tests'
    command = 'cargo test --locked app_server::tests::shared_runner'
    official_codex_executed = $false
    limitation = 'Official Codex execution requires a separate explicit acceptance stage.'
}
$json = $report | ConvertTo-Json -Depth 4
if ($OutputPath) { Set-Content -LiteralPath $OutputPath -Value $json -Encoding utf8 }
$json
