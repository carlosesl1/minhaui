[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter(Mandatory)]
    [ValidateSet('Keep', 'Export', 'Remove')]
    [string]$SettingsAction,

    [string]$ExportPath,

    [string]$AppRoot = $PSScriptRoot,

    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$settingsRoot = [System.IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Minha UI'))
$localRoot = [System.IO.Path]::GetFullPath($env:LOCALAPPDATA).TrimEnd('\') + '\'
if (-not $settingsRoot.StartsWith($localRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'Settings path escaped the current user LocalAppData directory.'
}

& (Join-Path $PSScriptRoot 'Invoke-ObsidianRecovery.ps1') -Hook Uninstall -AppRoot $AppRoot -DryRun:$DryRun

switch ($SettingsAction) {
    'Keep' { Write-Host "Settings preserved at $settingsRoot" }
    'Export' {
        if ([string]::IsNullOrWhiteSpace($ExportPath)) {
            throw 'ExportPath is required when SettingsAction is Export.'
        }
        if ((Test-Path -LiteralPath $settingsRoot) -and -not $DryRun) {
            New-Item -ItemType Directory -Force -Path $ExportPath | Out-Null
            Copy-Item -LiteralPath $settingsRoot -Destination $ExportPath -Recurse -Force
        }
        Write-Host "Settings export target: $ExportPath"
    }
    'Remove' {
        if ((Test-Path -LiteralPath $settingsRoot) -and -not $DryRun -and
            $PSCmdlet.ShouldProcess($settingsRoot, 'Remove application settings')) {
            Remove-Item -LiteralPath $settingsRoot -Recurse -Force
        }
        Write-Host "Settings removal target: $settingsRoot"
    }
}
