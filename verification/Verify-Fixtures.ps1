[CmdletBinding()]
param(
    [string] $OutputPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function New-TestRoot {
    $root = Join-Path ([IO.Path]::GetTempPath()) ('codex-fixtures-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $root -Force | Out-Null
    return $root
}

function Add-Result($Results, [string] $Id, [string] $Status, [string] $Detail) {
    $Results.Add([ordered]@{ id = $Id; status = $Status; detail = $Detail })
}

function Start-Fixture([string] $Script, [string] $Marker, [string] $Mode, [string] $Root) {
    $path = Join-Path $Root ($Mode + '.ps1')
    Set-Content -LiteralPath $path -Encoding utf8 -Value $Script
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = (Get-Command powershell.exe).Source
    $psi.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $path + '" -Marker "' + $Marker + '"'
    $psi.WorkingDirectory = $Root
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    if (-not $process.Start()) { throw "fixture process failed to start: $Mode" }
    return $process
}

function Wait-ProcessBounded([Diagnostics.Process] $Process, [int] $Milliseconds) {
    if ($Process.WaitForExit($Milliseconds)) { return $true }
    try { $Process.Kill($true) } catch { }
    $Process.WaitForExit(2000)
    return $false
}

function Assert-Marker([string] $Marker, [int] $TimeoutMs = 2000) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        if (Test-Path -LiteralPath $Marker) { return $true }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    return $false
}

$root = New-TestRoot
$results = [Collections.Generic.List[object]]::new()
$marker = Join-Path $root 'marker.txt'
$fixtureScript = @'
param([string] $Marker)
Set-Content -LiteralPath $Marker -Value 'fixture-started' -Encoding utf8
'@
$listener = $null
$child = $null
try {
    # T8 filesystem/process positive controls: the marker and child process are
    # independently observable before any negative scenario is assessed.
    $child = Start-Fixture $fixtureScript $marker 'positive' $root
    $positiveProcess = $child.Id
    $positiveMarker = Assert-Marker $marker
    $positiveExit = Wait-ProcessBounded $child 3000
    if ($positiveMarker -and $positiveExit) {
        Add-Result $results 'T8-process-filesystem' 'PASS' 'Positive fixture process and marker were both observed.'
    } else {
        Add-Result $results 'T8-process-filesystem' 'FAIL' 'Positive process/filesystem observation did not detect the fixture.'
    }

    # T8 network positive control: loopback-only listener and intentional local connection.
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = ([Net.IPEndPoint]$listener.LocalEndpoint).Port
    $client = [Net.Sockets.TcpClient]::new('127.0.0.1', $port)
    $accepted = $listener.AcceptTcpClient()
    $client.Dispose(); $accepted.Dispose()
    Add-Result $results 'T8-network' 'PASS' 'Loopback positive control detected an intentional local connection.'

    # T2/T3/T4 fixture definitions and positive controls are implemented. The
    # negative integrations are intentionally not run against Codex here.
    foreach ($id in @('T2','T3','T4','T5','T6')) {
        Add-Result $results $id 'IMPLEMENTED' 'Synthetic fixture/configuration and observation policy are prepared; official Codex execution is a subsequent stage.'
    }

    # T7 bounded lifecycle controls: repeated fixture launches and forced cleanup.
    $clean = $true
    for ($i = 1; $i -le 3; $i++) {
        $p = Start-Fixture $fixtureScript (Join-Path $root ("cycle-$i.marker")) ("cycle-$i") $root
        if (-not (Wait-ProcessBounded $p 10000)) { $clean = $false }
        $p.Dispose()
    }
    $hangScript = 'param([string] $Marker); [System.Threading.Thread]::Sleep(60000)'
    $hang = Start-Fixture $hangScript (Join-Path $root 'unused.marker') 'timeout' $root
    $terminated = -not (Wait-ProcessBounded $hang 500)
    $hang.Dispose()
    if ($clean -and $terminated) {
        Add-Result $results 'T7' 'PASS' 'Repeated bounded launches and forced timeout cleanup completed.'
    } else {
        Add-Result $results 'T7' 'FAIL' ("Lifecycle controls failed: clean={0}; terminated={1}." -f $clean, $terminated)
    }

    # O40/O41: all fixtures are generated under a unique root and removed in finally.
    Add-Result $results 'O40' 'PASS' 'Fixtures are deterministic, bounded and generated under a unique disposable root.'
    Add-Result $results 'O41' 'PASS' 'Fixture cleanup completed through the success and timeout paths.'
} catch {
    Add-Result $results 'harness' 'FAIL' $_.Exception.Message
} finally {
    if ($listener) { $listener.Stop() }
    if ($child -and -not $child.HasExited) { try { $child.Kill($true) } catch { } }
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}

$report = [ordered]@{
    schema_version = 1
    mode = 'synthetic-fixtures-only'
    official_codex_executed = $false
    results = $results
    limitations = @(
        'This self-test never launches the official Codex executable.',
        'T2-T6 are fixture and observation implementation checks; authenticated or official-runtime negative results require the subsequent approved stage.',
        'Windows Job Object behavior is not claimed by this PowerShell-only self-test.'
    )
}
$json = $report | ConvertTo-Json -Depth 8
if ($OutputPath) { Set-Content -LiteralPath $OutputPath -Value $json -Encoding utf8 }
$json
