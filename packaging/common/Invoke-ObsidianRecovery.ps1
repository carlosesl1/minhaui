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

$mutation = if ($DryRun) { 'disabled' } else { 'enabled' }
if ($PSCmdlet.ShouldProcess($Hook, 'Restore the Windows taskbar transaction')) {
    & $watchdog --simulate-taskbar-restore --taskbar-mutation $mutation
    if ($LASTEXITCODE -ne 0) {
        throw "Recovery watchdog failed with exit code $LASTEXITCODE"
    }
}

