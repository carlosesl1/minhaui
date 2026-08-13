[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('Update', 'Uninstall')]
    [string]$Hook,

    [Parameter(Mandatory)]
    [string]$AppRoot,

    [string]$WatchdogPath,

    [switch]$DryRun,

    [switch]$Hold
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($AppRoot)) {
    throw 'AppRoot must not be empty.'
}
if ($DryRun -and $Hold) {
    throw 'DryRun cannot hold a lifecycle mutation gate.'
}

$root = [System.IO.Path]::GetFullPath($AppRoot)
$watchdog = if ([string]::IsNullOrWhiteSpace($WatchdogPath)) {
    Join-Path $root 'shell-watchdog.exe'
} else {
    [System.IO.Path]::GetFullPath($WatchdogPath)
}
if ($Hold -and [string]::IsNullOrWhiteSpace($WatchdogPath)) {
    throw 'Hold requires an explicit companion WatchdogPath outside the package.'
}

if (-not (Test-Path -LiteralPath $watchdog -PathType Leaf)) {
    if ($DryRun) {
        Write-Host "Recovery dry-run: watchdog is not staged at $watchdog"
        return
    }
    throw "Recovery watchdog was not found at $watchdog"
}

$arguments = @('--restore-only', '--hook', $Hook.ToLowerInvariant())
if ($DryRun) {
    $arguments += '--check-only'
}
if ($Hold) {
    $arguments += '--hold-until-stdin-eof'
}

Write-Host "Running synchronous $($Hook.ToLowerInvariant()) recovery contract."
if ($Hold) {
    # MSIX deployment keeps the lifecycle mutex held across the package
    # mutation. All streams are redirected and asynchronously drained so no
    # console EOF or full pipe can release/deadlock the holder.
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $watchdog
    $start.WorkingDirectory = Split-Path -Parent $watchdog
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    # These are fixed, whitespace-free protocol tokens. `ArgumentList` is not
    # available in Windows PowerShell 5.1's .NET Framework.
    $start.Arguments = $arguments -join ' '
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) {
            throw 'Recovery watchdog did not start.'
        }
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $deadline = [System.Diagnostics.Stopwatch]::StartNew()
        $ready = $false
        $lineCount = 0
        while (-not $ready -and $lineCount -lt 64) {
            $remaining = 30000 - [int]$deadline.ElapsedMilliseconds
            if ($remaining -le 0) { break }
            $lineTask = $process.StandardOutput.ReadLineAsync()
            if (-not $lineTask.Wait($remaining)) { break }
            $line = $lineTask.Result
            if ($null -eq $line) { break }
            $lineCount++
            $ready = $line -ceq 'MINHA_UI_LIFECYCLE_READY v1'
        }
        if (-not $ready) {
            if (-not $process.HasExited) { $process.Kill() }
            $process.WaitForExit()
            [void]$stderrTask.Wait(5000)
            $stderrText = if ($stderrTask.IsCompleted) { $stderrTask.Result } else { '' }
            throw "Recovery watchdog did not publish bounded READY. stderr: $stderrText"
        }
        if ($process.HasExited) {
            [void]$stderrTask.Wait(5000)
            $stderrText = if ($stderrTask.IsCompleted) { $stderrTask.Result } else { '' }
            throw "Recovery watchdog exited before gate transfer: $($process.ExitCode); stderr: $stderrText"
        }
        # The holder emits no further stdout after READY and stderr remains
        # asynchronously drained by `stderrTask` until the process exits.
        # Unary comma prevents PowerShell from enumerating Process properties
        # or flattening future collection-like implementations.
        return ,$process
    } catch {
        $process.Dispose()
        throw
    }
}
else {
    $process = Start-Process `
    -FilePath $watchdog `
    -ArgumentList $arguments `
    -WorkingDirectory $root `
    -Wait `
    -PassThru
    try {
        if ($process.ExitCode -ne 0) {
            throw "Recovery watchdog failed with exit code $($process.ExitCode)"
        }
    } finally {
        $process.Dispose()
    }
    Write-Host "Recovery contract completed for $Hook."
}
