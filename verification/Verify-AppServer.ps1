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

function Stop-ProcessSafely([System.Diagnostics.Process] $Process) {
    if ($null -eq $Process) { return $false }
    try { $Process.Refresh() } catch { }
    if ($Process.HasExited) { return $true }
    try { $Process.Kill($true) } catch { try { $Process.Kill() } catch { } }
    try { $null = $Process.WaitForExit(2000) } catch { }
    try { $Process.Refresh() } catch { }
    return $Process.HasExited
}

function New-ControlledStartInfo([string] $FileName, [string] $Arguments, [string] $WorkingDirectory, [string] $SyntheticHome) {
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $FileName
    $psi.Arguments = $Arguments
    $psi.WorkingDirectory = $WorkingDirectory
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.StandardOutputEncoding = [Text.Encoding]::UTF8
    $psi.StandardErrorEncoding = [Text.Encoding]::UTF8
    $psi.EnvironmentVariables.Clear()
    foreach ($name in @('SystemRoot','WINDIR','PATH')) {
        $value = [Environment]::GetEnvironmentVariable($name)
        if ($value) { $psi.EnvironmentVariables[$name] = $value }
    }
    $psi.EnvironmentVariables['TEMP'] = $SyntheticHome
    $psi.EnvironmentVariables['TMP'] = $SyntheticHome
    $psi.EnvironmentVariables['HOME'] = $SyntheticHome
    $psi.EnvironmentVariables['USERPROFILE'] = $SyntheticHome
    $psi.EnvironmentVariables['CODEX_HOME'] = $SyntheticHome
    return $psi
}

function Read-BoundedLine([System.IO.StreamReader] $Reader, [int] $TimeoutMs, [int] $MaxChars) {
    $task = $Reader.ReadLineAsync()
    if (-not $task.Wait($TimeoutMs)) { return @{ TimedOut = $true; Line = $null } }
    $line = $task.Result
    if ($null -ne $line -and $line.Length -gt $MaxChars) { throw 'Protocol frame exceeded the configured character limit.' }
    return @{ TimedOut = $false; Line = $line }
}

function Invoke-VersionProbe([string] $FileName, [string] $WorkingDirectory, [string] $SyntheticHome, [int] $TimeoutMs) {
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = New-ControlledStartInfo $FileName '--version' $WorkingDirectory $SyntheticHome
    try {
        if (-not $process.Start()) { throw 'Version probe process did not start.' }
        $out = Read-BoundedLine $process.StandardOutput $TimeoutMs 4096
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutMs)) { throw 'Version probe exceeded its deadline.' }
        if (-not $stderr.Wait(1000)) { throw 'Version probe stderr drain exceeded its deadline.' }
        if ($process.ExitCode -ne 0 -or $out.TimedOut -or [string]::IsNullOrWhiteSpace($out.Line)) { throw 'Version probe failed or produced no bounded output.' }
        return $out.Line.Trim()
    } finally {
        if (-not (Stop-ProcessSafely $process)) { throw 'Version probe process could not be confirmed terminated.' }
        $process.Dispose()
    }
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

$root = Join-Path ([System.IO.Path]::GetTempPath()) ('codex-app-server-' + [guid]::NewGuid().ToString('N'))
$codexHome = Join-Path $root 'codex-home'
$workdir = Join-Path $root 'work'
New-Item -ItemType Directory -Path $codexHome, $workdir -Force | Out-Null

$file = Get-Item -LiteralPath $ExecutablePath -ErrorAction Stop
if (-not $file.PSIsContainer -and $file.Extension -ieq '.exe') { } else {
    Fail 'ExecutablePath must identify an .exe file.'
}
$canonical = $file.FullName
$actualHash = (Get-FileHash -LiteralPath $canonical -Algorithm SHA256).Hash.ToUpperInvariant()
if ($actualHash -ne $ExpectedSha256.ToUpperInvariant()) {
    Fail 'Executable SHA-256 does not match the explicitly approved fingerprint.'
}

$versionText = Invoke-VersionProbe $canonical $workdir $codexHome ($TimeoutSeconds * 1000)
if ($versionText -notmatch [regex]::Escape($ExpectedVersion)) {
    Fail 'Reported executable version does not match ExpectedVersion.'
}

$result = [ordered]@{
    schema_version = 1
    executable_path = $canonical
    sha256 = $actualHash
    expected_version = $ExpectedVersion
    reported_version = $versionText
    tests = @()
    limitations = @(
        'Synthetic unauthenticated execution does not establish authenticated startup safety.',
        'T2-T8 fixture scenarios are implemented by verification/Verify-Fixtures.ps1; official-runtime execution remains separate.'
    )
}

function Add-TestResult([string] $Name, [string] $Status, [string] $Detail) {
    $result.tests += [ordered]@{ id = $Name; status = $Status; detail = $Detail }
}

$psi = New-ControlledStartInfo $canonical 'app-server --listen stdio://' $workdir $codexHome

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

    $initRead = Read-BoundedLine $process.StandardOutput ($TimeoutSeconds * 1000) 65536
    if ($initRead.TimedOut -or [string]::IsNullOrWhiteSpace($initRead.Line)) {
        Add-TestResult 'T1' 'INCONCLUSIVE' 'No initialize response was received.'
    } else {
        $init = $initRead.Line | ConvertFrom-Json
        if ($init.PSObject.Properties.Name -notcontains 'id' -or $init.id -ne 1) {
            Add-TestResult 'T1' 'FAIL' 'Initialize response id did not match request id 1.'
        } elseif ($init.PSObject.Properties.Name -contains 'error') {
            Add-TestResult 'T1' 'INCONCLUSIVE' 'Initialize returned a sanitized JSON-RPC error; later requests were not sent.'
        } else {
            $process.StandardInput.WriteLine($initialized)
            $process.StandardInput.Flush()
            $process.StandardInput.WriteLine($rateLimits)
            $process.StandardInput.Flush()
            $quotaRead = Read-BoundedLine $process.StandardOutput ($TimeoutSeconds * 1000) 65536
            if ($quotaRead.TimedOut -or [string]::IsNullOrWhiteSpace($quotaRead.Line)) {
                Add-TestResult 'T1' 'INCONCLUSIVE' 'Initialize succeeded but no rate-limit response was received.'
            } else {
                $quota = $quotaRead.Line | ConvertFrom-Json
                if ($quota.PSObject.Properties.Name -notcontains 'id' -or $quota.id -ne 2) {
                    Add-TestResult 'T1' 'FAIL' 'Rate-limit response id did not match request id 2.'
                } elseif ($quota.PSObject.Properties.Name -contains 'error') {
                    if (($quota.error.PSObject.Properties.Name -contains 'code') -and $quota.error.code -in @('unauthorized','not_authenticated','authentication_required')) {
                        Add-TestResult 'T1' 'PASS' 'Expected unauthenticated rate-limit response received with matching id.'
                    } else {
                        Add-TestResult 'T1' 'INCONCLUSIVE' 'Rate-limit request returned an unexpected sanitized JSON-RPC error.'
                    }
                } elseif ($quota.PSObject.Properties.Name -contains 'result' -and $null -ne $quota.result) {
                    Add-TestResult 'T1' 'INCONCLUSIVE' 'Authenticated quota result was returned; no raw quota data was retained.'
                } else {
                    Add-TestResult 'T1' 'FAIL' 'Rate-limit response lacked both a valid result and an expected authentication error.'
                }
            }
        }
    }
    $process.StandardInput.Close()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        Stop-ProcessSafely $process | Out-Null
        Add-TestResult 'shutdown' 'FAIL' 'Process exceeded bounded shutdown deadline and was terminated.'
    } elseif ($stderrTask.Wait(1000)) {
        Add-TestResult 'shutdown' 'PASS' 'Process exited within the bounded shutdown deadline.'
    } else {
        Add-TestResult 'shutdown' 'FAIL' 'Stderr drain exceeded its deadline.'
    }
} catch {
    Add-TestResult 'T1' 'INCONCLUSIVE' 'Harness error was sanitized; protocol evidence is incomplete.'
    Stop-ProcessSafely $process | Out-Null
} finally {
    if ($process) { Stop-ProcessSafely $process | Out-Null; $process.Dispose() }
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction Stop }
}

$json = $result | ConvertTo-Json -Depth 8
if ($OutputPath) { Set-Content -LiteralPath $OutputPath -Value $json -Encoding utf8 }
$json
if (@($result.tests | Where-Object { $_.status -eq 'FAIL' }).Count -gt 0) { exit 2 }
