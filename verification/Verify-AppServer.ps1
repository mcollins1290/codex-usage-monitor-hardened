[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $ExecutablePath,
    [Parameter(Mandatory)] [string] $ExpectedSha256,
    [Parameter(Mandatory)] [string] $ExpectedVersion,
    [int] $TimeoutSeconds = 15,
    [string] $OutputPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Fail([string] $Message) {
    throw $Message
}

if (-not [System.IO.Path]::IsPathFullyQualified($ExecutablePath)) {
    Fail 'ExecutablePath must be absolute.'
}
if ($ExpectedSha256 -notmatch '^[0-9A-Fa-f]{64}$') {
    Fail 'ExpectedSha256 must contain exactly 64 hexadecimal characters.'
}
if ($TimeoutSeconds -lt 1 -or $TimeoutSeconds -gt 120) {
    Fail 'TimeoutSeconds must be between 1 and 120.'
}

$file = Get-Item -LiteralPath $ExecutablePath -ErrorAction Stop
if (-not $file.PSIsContainer -and $file.Extension -ieq '.exe') { } else {
    Fail 'ExecutablePath must identify an .exe file.'
}
$canonical = $file.FullName
$actualHash = (Get-FileHash -LiteralPath $canonical -Algorithm SHA256).Hash.ToUpperInvariant()
if ($actualHash -ne $ExpectedSha256.ToUpperInvariant()) {
    Fail 'Executable SHA-256 does not match the explicitly approved fingerprint.'
}

$versionOutput = & $canonical --version 2>$null
if ($LASTEXITCODE -ne 0) { Fail 'Bounded version probe failed.' }
$versionText = ($versionOutput -join "`n").Trim()
if ($versionText -notmatch [regex]::Escape($ExpectedVersion)) {
    Fail 'Reported executable version does not match ExpectedVersion.'
}

$root = Join-Path ([System.IO.Path]::GetTempPath()) ('codex-app-server-' + [guid]::NewGuid().ToString('N'))
$codexHome = Join-Path $root 'codex-home'
$workdir = Join-Path $root 'work'
New-Item -ItemType Directory -Path $codexHome, $workdir -Force | Out-Null

$result = [ordered]@{
    schema_version = 1
    executable_path = $canonical
    sha256 = $actualHash
    expected_version = $ExpectedVersion
    reported_version = $versionText
    tests = @()
    limitations = @(
        'Synthetic unauthenticated execution does not establish authenticated startup safety.',
        'T2-T8 fixture and observation scenarios are not implemented by this bounded T1 harness.'
    )
}

function Add-TestResult([string] $Name, [string] $Status, [string] $Detail) {
    $result.tests += [ordered]@{ id = $Name; status = $Status; detail = $Detail }
}

$psi = [System.Diagnostics.ProcessStartInfo]::new()
$psi.FileName = $canonical
$psi.Arguments = 'app-server --listen stdio://'
$psi.WorkingDirectory = $workdir
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.Environment['CODEX_HOME'] = $codexHome
$psi.Environment['HOME'] = $codexHome

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $psi
$started = $false
try {
    $started = $process.Start()
    if (-not $started) { Fail 'App-server process did not start.' }

    $initialize = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"codex-usage-monitor-hardened-verifier","version":"1.0.0"},"capabilities":{}}}'
    $initialized = '{"jsonrpc":"2.0","method":"initialized","params":{}}'
    $rateLimits = '{"jsonrpc":"2.0","id":2,"method":"account/rateLimits/read","params":{}}'
    $process.StandardInput.WriteLine($initialize)
    $process.StandardInput.Flush()

    $initLine = $process.StandardOutput.ReadLine()
    if ([string]::IsNullOrWhiteSpace($initLine)) {
        Add-TestResult 'T1' 'INCONCLUSIVE' 'No initialize response was received.'
    } else {
        $init = $initLine | ConvertFrom-Json
        if ($init.id -ne 1) {
            Add-TestResult 'T1' 'FAIL' 'Initialize response id did not match request id 1.'
        } elseif ($null -ne $init.error) {
            Add-TestResult 'T1' 'INCONCLUSIVE' 'Initialize returned a sanitized JSON-RPC error; later requests were not sent.'
        } else {
            $process.StandardInput.WriteLine($initialized)
            $process.StandardInput.Flush()
            $process.StandardInput.WriteLine($rateLimits)
            $process.StandardInput.Flush()
            $quotaLine = $process.StandardOutput.ReadLine()
            if ([string]::IsNullOrWhiteSpace($quotaLine)) {
                Add-TestResult 'T1' 'INCONCLUSIVE' 'Initialize succeeded but no rate-limit response was received.'
            } else {
                $quota = $quotaLine | ConvertFrom-Json
                if ($quota.id -ne 2) {
                    Add-TestResult 'T1' 'FAIL' 'Rate-limit response id did not match request id 2.'
                } else {
                    Add-TestResult 'T1' 'PASS' 'Fixed initialize, initialized and account/rateLimits/read sequence completed.'
                }
            }
        }
    }
    $process.StandardInput.Close()
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        $process.Kill($true)
        Add-TestResult 'shutdown' 'FAIL' 'Process exceeded bounded shutdown deadline and was terminated.'
    } else {
        Add-TestResult 'shutdown' 'PASS' 'Process exited within the bounded shutdown deadline.'
    }
} catch {
    Add-TestResult 'T1' 'INCONCLUSIVE' 'Harness error: ' + $_.Exception.Message
    if ($process.HasExited -eq $false) { $process.Kill($true) }
} finally {
    if ($process -and -not $process.HasExited) { $process.Kill($true) }
    $process.Dispose()
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}

$json = $result | ConvertTo-Json -Depth 8
if ($OutputPath) { Set-Content -LiteralPath $OutputPath -Value $json -Encoding utf8 }
$json
