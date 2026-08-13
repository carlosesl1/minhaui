[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('InstallOrUpdate', 'Uninstall')]
    [string]$Action,

    [string]$PackagePath,

    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$packageIdentityName = 'Carlos.ObsidianGlass'
$policyModule = Join-Path $PSScriptRoot 'ObsidianLifecyclePolicy.psm1'
$recoveryScript = Join-Path $PSScriptRoot 'Invoke-ObsidianRecovery.ps1'
$companionWatchdog = Join-Path $PSScriptRoot 'shell-watchdog-recovery.exe'
$checksumFile = Join-Path $PSScriptRoot 'SHA256SUMS.txt'

$requiredFiles = @($policyModule, $recoveryScript)
if (-not $DryRun) {
    $requiredFiles += @($companionWatchdog, $checksumFile)
}
foreach ($requiredFile in $requiredFiles) {
    if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
        throw "Required deployment file is missing: $requiredFile"
    }
}
if (-not $DryRun) {
    $checksumPattern = '^([0-9A-Fa-f]{64})\s{2}shell-watchdog-recovery\.exe$'
    $expectedCompanionHashes = @(
        Get-Content -LiteralPath $checksumFile | ForEach-Object {
            if ($_ -match $checksumPattern) { $Matches[1].ToUpperInvariant() }
        }
    )
    if ($expectedCompanionHashes.Count -ne 1) {
        throw 'SHA256SUMS.txt must contain exactly one companion watchdog hash.'
    }
    $actualCompanionHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $companionWatchdog).Hash
    if ($actualCompanionHash -cne $expectedCompanionHashes[0]) {
        throw 'Companion watchdog checksum validation failed.'
    }
}
Import-Module $policyModule -Force

$resolvedPackagePath = $null
if ($Action -eq 'InstallOrUpdate') {
    if ([string]::IsNullOrWhiteSpace($PackagePath)) {
        throw 'PackagePath is required for InstallOrUpdate.'
    }
    $resolvedPackagePath = [System.IO.Path]::GetFullPath($PackagePath)
    if ([System.IO.Path]::GetExtension($resolvedPackagePath) -ne '.msix') {
        throw 'PackagePath must point to an .msix file.'
    }
    if (-not (Test-Path -LiteralPath $resolvedPackagePath -PathType Leaf)) {
        throw "MSIX package was not found: $resolvedPackagePath"
    }
}

$installedPackages = @(Get-AppxPackage -Name $packageIdentityName -ErrorAction Stop)
if ($installedPackages.Count -gt 1) {
    throw "More than one current-user package matched $packageIdentityName."
}
$installedPackage = $installedPackages | Select-Object -First 1
$installedRoot = if ($null -ne $installedPackage) {
    [string]$installedPackage.InstallLocation
} else {
    $PSScriptRoot
}
$plan = @(
    Get-ObsidianMsixLifecyclePlan `
        -Action $Action `
        -IsInstalled ($null -ne $installedPackage)
)
$recoveryHolder = $null

try {
    foreach ($step in $plan) {
        Write-Host "MSIX lifecycle step: $step"
        if ($DryRun) {
            continue
        }

        switch ($step) {
            'RecoverUpdate' {
                if ([string]::IsNullOrWhiteSpace($installedRoot)) {
                    throw 'The installed package has no usable InstallLocation.'
                }
                $recoveryHolder = & $recoveryScript `
                    -Hook Update `
                    -AppRoot $installedRoot `
                    -WatchdogPath $companionWatchdog `
                    -Hold
            }
            'InstallOrUpdate' {
                Add-AppxPackage `
                    -Path $resolvedPackagePath `
                    -ForceApplicationShutdown `
                    -ErrorAction Stop
            }
            'RecoverUninstall' {
                if ([string]::IsNullOrWhiteSpace($installedRoot)) {
                    throw 'The installed package has no usable InstallLocation.'
                }
                $recoveryHolder = & $recoveryScript `
                    -Hook Uninstall `
                    -AppRoot $installedRoot `
                    -WatchdogPath $companionWatchdog `
                    -Hold
            }
            'Uninstall' {
                Remove-AppxPackage `
                    -Package ([string]$installedPackage.PackageFullName) `
                    -ErrorAction Stop
            }
            'NoOp' {
                Write-Host 'Obsidian Glass is not installed for the current user.'
            }
            default {
                throw "Unknown MSIX lifecycle step: $step"
            }
        }
    }
} finally {
    if ($null -ne $recoveryHolder) {
        $holderFailure = $null
        try {
            $recoveryHolder.StandardInput.Close()
            if (-not $recoveryHolder.WaitForExit(30000)) {
                $recoveryHolder.Kill()
                $holderFailure = 'Recovery holder did not release after the package mutation.'
            }
            elseif ($recoveryHolder.ExitCode -ne 0) {
                $holderFailure = "Recovery holder failed with exit code $($recoveryHolder.ExitCode)"
            }
        } finally {
            $recoveryHolder.Dispose()
        }
        if ($null -ne $holderFailure) {
            Write-Error $holderFailure
        }
    }
}

Write-Host "MSIX lifecycle action completed: $Action"
