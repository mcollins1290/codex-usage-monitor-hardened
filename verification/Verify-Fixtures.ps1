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
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = (Get-Command powershell.exe).Source
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Script))
    $psi.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ' + $encoded
    $psi.WorkingDirectory = $Root
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $false
    $psi.RedirectStandardError = $false
    $psi.Environment['FIXTURE_MARKER'] = $Marker
    $psi.Environment['FIXTURE_MODE'] = $Mode
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    if (-not $process.Start()) { throw "fixture process failed to start: $Mode" }
    $script:children.Add($process)
    return $process
}

function Wait-ProcessBounded([Diagnostics.Process] $Process, [int] $Milliseconds) {
    if ($Process.WaitForExit($Milliseconds)) { return $true }
    try {
        $Process.Kill($true)
    } catch {
        try { $Process.Kill() } catch { }
    }
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

function Start-TimeoutFixture([string] $Marker) {
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = Join-Path $env:SystemRoot 'System32\PING.EXE'
    $psi.Arguments = '-t 127.0.0.1'
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $false
    $psi.RedirectStandardError = $false
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    if (-not $process.Start()) { throw 'timeout fixture failed to start' }
    if ($process.HasExited) { throw 'timeout fixture exited before observation' }
    Set-Content -LiteralPath $Marker -Value ('fixture-started:' + $process.Id) -Encoding utf8
    $script:children.Add($process)
    return $process
}

$root = New-TestRoot
$results = [Collections.Generic.List[object]]::new()
$children = [Collections.Generic.List[Diagnostics.Process]]::new()
$listeners = [Collections.Generic.List[Net.Sockets.TcpListener]]::new()
$marker = Join-Path $root 'marker.txt'
$fixtureScript = @'
$marker = $env:FIXTURE_MARKER
Set-Content -LiteralPath $marker -Value ('fixture-started:' + $env:FIXTURE_MODE) -Encoding utf8
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
    $listeners.Add($listener)
    $port = ([Net.IPEndPoint]$listener.LocalEndpoint).Port
    $client = [Net.Sockets.TcpClient]::new('127.0.0.1', $port)
    $accepted = $listener.AcceptTcpClient()
    $client.Dispose(); $accepted.Dispose()
    Add-Result $results 'T8-network' 'PASS' 'Loopback positive control detected an intentional local connection.'

    # T2 stdio-MCP-shaped command fixture: execution is observable and bounded.
    $mcpMarker = Join-Path $root 'mcp.marker'
    $mcp = Start-Fixture $fixtureScript $mcpMarker 'mcp-stdio' $root
    $mcpObserved = Assert-Marker $mcpMarker -TimeoutMs 2000
    $mcpExited = Wait-ProcessBounded $mcp 10000
    if ($mcpObserved -and $mcpExited) {
        Add-Result $results 'T2' 'PASS' 'Synthetic stdio command fixture started, marked execution and exited.'
    } else { Add-Result $results 'T2' 'FAIL' 'Synthetic stdio command observation failed.' }

    # T3 hook-shaped command fixture: the marker is the observable hook effect.
    $hookMarker = Join-Path $root 'hook.marker'
    $hook = Start-Fixture $fixtureScript $hookMarker 'hook' $root
    $hookObserved = Assert-Marker $hookMarker -TimeoutMs 2000
    $hookExited = Wait-ProcessBounded $hook 10000
    if ($hookObserved -and $hookExited) {
        Add-Result $results 'T3' 'PASS' 'Synthetic hook fixture execution was independently observable.'
    } else { Add-Result $results 'T3' 'FAIL' 'Synthetic hook observation failed.' }

    # T4 negative loopback control: after the positive control above, a quiet
    # bounded interval proves that the listener is not reporting phantom traffic.
    $quietListener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $quietListener.Start()
    $listeners.Add($quietListener)
    Start-Sleep -Milliseconds 200
    $phantomConnection = $quietListener.Pending()
    $quietListener.Stop()
    if (-not $phantomConnection) {
        Add-Result $results 'T4' 'PASS' 'Loopback observer detected no unsolicited connection during the bounded quiet interval.'
    } else { Add-Result $results 'T4' 'FAIL' 'Loopback observer reported an unexpected connection.' }

    # T5 plugin-shaped fixture: metadata is represented as data and the
    # synthetic side effect is observable only when the fixture is launched.
    $pluginManifest = Join-Path $root 'plugin.json'
    Set-Content -LiteralPath $pluginManifest -Encoding utf8 -Value '{"name":"synthetic-plugin","version":"1.0.0","command":"fixture"}'
    $pluginMarker = Join-Path $root 'plugin.marker'
    $plugin = Start-Fixture $fixtureScript $pluginMarker 'plugin' $root
    $pluginObserved = Assert-Marker $pluginMarker -TimeoutMs 2000
    $pluginExited = Wait-ProcessBounded $plugin 10000
    $manifest = Get-Content -LiteralPath $pluginManifest -Raw | ConvertFrom-Json
    if ($manifest.name -eq 'synthetic-plugin' -and $pluginObserved -and $pluginExited) {
        Add-Result $results 'T5' 'PASS' 'Synthetic plugin metadata and its controlled side effect were observed.'
    } else { Add-Result $results 'T5' 'FAIL' 'Synthetic plugin control failed.' }

    # T6 configuration inheritance matrix: parse inherited and replacement
    # tables separately and assert that replacement, not empty-table merging,
    # is the policy being tested.
    $inheritedPath = Join-Path $root 'inherited.json'
    $replacementPath = Join-Path $root 'replacement.json'
    Set-Content -LiteralPath $inheritedPath -Encoding utf8 -Value '{"mcp":{"synthetic":{"command":"fixture"}},"hooks":{"enabled":true}}'
    Set-Content -LiteralPath $replacementPath -Encoding utf8 -Value '{"mcp":{},"hooks":{"enabled":false}}'
    $inherited = Get-Content $inheritedPath -Raw | ConvertFrom-Json
    $replacement = Get-Content $replacementPath -Raw | ConvertFrom-Json
    if ($inherited.mcp.synthetic.command -eq 'fixture' -and @($replacement.mcp.PSObject.Properties).Count -eq 0 -and -not $replacement.hooks.enabled) {
        Add-Result $results 'T6' 'PASS' 'Inheritance and explicit replacement fixtures are distinguishable.'
    } else { Add-Result $results 'T6' 'FAIL' 'Configuration inheritance control failed.' }

    # T7 bounded lifecycle controls: normal exit, premature exit, repeated
    # launches and a deliberate timeout. Every outcome requires a marker or an
    # explicit exit observation; early exit is never treated as a timeout pass.
    $normalMarker = Join-Path $root 'normal.marker'
    $normal = Start-Fixture $fixtureScript $normalMarker 'normal' $root
    $normalStarted = Assert-Marker $normalMarker
    $normalExited = Wait-ProcessBounded $normal 10000
    if ($normalStarted -and $normalExited) {
        Add-Result $results 'T7-normal-exit' 'PASS' 'Startup marker and bounded normal exit were observed.'
    } else {
        Add-Result $results 'T7-normal-exit' 'FAIL' ("Normal control failed: started={0}; exited={1}." -f $normalStarted, $normalExited)
    }

    $earlyScript = '$marker = $env:FIXTURE_MARKER; Set-Content -LiteralPath $marker -Value (''fixture-started:'' + $env:FIXTURE_MODE) -Encoding utf8; exit 23'
    $earlyMarker = Join-Path $root 'early.marker'
    $early = Start-Fixture $earlyScript $earlyMarker 'early' $root
    $earlyStarted = Assert-Marker $earlyMarker
    $earlyExited = Wait-ProcessBounded $early 10000
    if ($earlyStarted -and $earlyExited -and $early.ExitCode -eq 23) {
        Add-Result $results 'T7-premature-exit' 'PASS' 'Startup marker, early exit and exit code were observed.'
    } else {
        Add-Result $results 'T7-premature-exit' 'FAIL' ("Premature-exit control failed: started={0}; exited={1}; code={2}." -f $earlyStarted, $earlyExited, $early.ExitCode)
    }

    $clean = $true
    for ($i = 1; $i -le 3; $i++) {
        $cycleMarker = Join-Path $root ("cycle-$i.marker")
        $p = Start-Fixture $fixtureScript $cycleMarker ("cycle-$i") $root
        if (-not (Assert-Marker $cycleMarker) -or -not (Wait-ProcessBounded $p 10000)) { $clean = $false }
    }
    $hangMarker = Join-Path $root 'timeout.marker'
    $hang = Start-TimeoutFixture $hangMarker
    $hangStarted = Assert-Marker $hangMarker 2000
    $deadline = [DateTime]::UtcNow.AddMilliseconds(500)
    while ($hangStarted -and -not $hang.HasExited -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 25
        $hang.Refresh()
    }
    $timedOut = $hangStarted -and -not $hang.HasExited -and [DateTime]::UtcNow -ge $deadline
    if ($timedOut) {
        Stop-Process -Id $hang.Id -Force -ErrorAction SilentlyContinue
        $null = $hang.WaitForExit(2000)
    }
    $hang.Refresh()
    $terminated = $timedOut -and $hang.HasExited
    if ($clean -and $terminated) {
        Add-Result $results 'T7' 'PASS' 'Repeated bounded launches and forced timeout cleanup completed.'
    } else {
        Add-Result $results 'T7' 'FAIL' ("Lifecycle controls failed: clean={0}; hangStarted={1}; timedOut={2}; terminated={3}; exited={4}." -f $clean, $hangStarted, $timedOut, $terminated, $hang.HasExited)
    }

} catch {
    Add-Result $results 'harness' 'FAIL' $_.Exception.Message
} finally {
    $cleanupFailures = [Collections.Generic.List[string]]::new()
    foreach ($trackedListener in $listeners) {
        try { $trackedListener.Stop() } catch { $cleanupFailures.Add('listener') }
    }
    foreach ($trackedChild in $children) {
        try {
            $trackedChild.Refresh()
            if (-not $trackedChild.HasExited) { Wait-ProcessBounded $trackedChild 500 | Out-Null }
            $trackedChild.Refresh()
            if (-not $trackedChild.HasExited) { $cleanupFailures.Add("process:$($trackedChild.Id)") }
            $trackedChild.Dispose()
        } catch { $cleanupFailures.Add('process-cleanup') }
    }
    if (Test-Path -LiteralPath $root) {
        try { Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction Stop } catch { $cleanupFailures.Add('temporary-root') }
    }
    if (Test-Path -LiteralPath $root) { $cleanupFailures.Add('temporary-root-remains') }
    if ($cleanupFailures.Count -eq 0) {
        Add-Result $results 'O40' 'PASS' 'Fixtures were deterministic, bounded and generated under a unique disposable root.'
        Add-Result $results 'O41' 'PASS' 'All tracked processes/listeners terminated and the temporary root was independently confirmed removed.'
    } else {
        Add-Result $results 'O40' 'FAIL' 'Fixture cleanup verification failed.'
        Add-Result $results 'O41' 'FAIL' ('Cleanup failures: ' + ($cleanupFailures -join ', '))
    }
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
