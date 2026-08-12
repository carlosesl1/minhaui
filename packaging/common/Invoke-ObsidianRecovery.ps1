[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter(Mandatory)]
    [ValidateSet('Update', 'Uninstall')]
    [string]$Hook,

    [Parameter(Mandatory)]
    [string]$AppRoot,

    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath($AppRoot)
$watchdog = Join-Path $root 'shell-watchdog.exe'

if (-not (Test-Path -LiteralPath $watchdog -PathType Leaf)) {
    if ($DryRun) {
        Write-Host "Recovery dry-run: watchdog is not staged at $watchdog"
        exit 0
    }
    throw "Recovery watchdog was not found at $watchdog"
}

$arguments = @('--restore-only', '--hook', $Hook.ToLowerInvariant())
if ($DryRun) {
    $arguments += '--check-only'
}

if ($PSCmdlet.ShouldProcess($Hook, 'Run the watchdog recovery contract')) {
    $process = Start-Process `
        -FilePath $watchdog `
        -ArgumentList $arguments `
        -Wait `
        -PassThru
    if ($process.ExitCode -ne 0) {
        throw "Recovery watchdog failed with exit code $($process.ExitCode)"
    }
}
