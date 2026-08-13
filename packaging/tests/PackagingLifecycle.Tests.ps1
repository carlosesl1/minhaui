param(
    [string]$ReleaseDirectory
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
if ([string]::IsNullOrWhiteSpace($ReleaseDirectory)) {
    $ReleaseDirectory = Join-Path $repositoryRoot 'target\x86_64-pc-windows-msvc\release'
}
$ReleaseDirectory = [System.IO.Path]::GetFullPath($ReleaseDirectory)

function Assert-True {
    param(
        [Parameter(Mandatory)] [bool]$Condition,
        [Parameter(Mandatory)] [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Sequence {
    param(
        [Parameter(Mandatory)] [string[]]$Expected,
        [Parameter(Mandatory)] [string[]]$Actual,
        [Parameter(Mandatory)] [string]$Message
    )

    Assert-True `
        -Condition (($Expected -join '|') -eq ($Actual -join '|')) `
        -Message "$Message Expected '$($Expected -join ', ')'; got '$($Actual -join ', ')'."
}

$policyModule = Join-Path $repositoryRoot 'packaging\common\ObsidianLifecyclePolicy.psm1'
Import-Module $policyModule -Force

$recoveryWrapper = Get-Content -Raw -LiteralPath (
    Join-Path $repositoryRoot 'packaging\common\Invoke-ObsidianRecovery.ps1'
)
Assert-True `
    -Condition $recoveryWrapper.Contains('-Wait') `
    -Message 'The packaging recovery contract must wait synchronously for the watchdog.'
Assert-True `
    -Condition $recoveryWrapper.Contains('$process.ExitCode -ne 0') `
    -Message 'The packaging recovery contract must reject a nonzero watchdog exit status.'
Assert-True `
    -Condition ($recoveryWrapper.Contains('MINHA_UI_LIFECYCLE_READY v1') -and
        $recoveryWrapper.Contains('RedirectStandardInput = $true')) `
    -Message 'The MSIX holder must authenticate READY while retaining stdin authority.'

Assert-Sequence `
    -Expected @('InstallOrUpdate') `
    -Actual @(Get-ObsidianMsixLifecyclePlan -Action InstallOrUpdate -IsInstalled $false) `
    -Message 'A first MSIX install must not invent a recovery step.'
Assert-Sequence `
    -Expected @('RecoverUpdate', 'InstallOrUpdate') `
    -Actual @(Get-ObsidianMsixLifecyclePlan -Action InstallOrUpdate -IsInstalled $true) `
    -Message 'An MSIX update must recover before replacing package files.'
Assert-Sequence `
    -Expected @('RecoverUninstall', 'Uninstall') `
    -Actual @(Get-ObsidianMsixLifecyclePlan -Action Uninstall -IsInstalled $true) `
    -Message 'An MSIX uninstall must recover before removing package files.'
Assert-Sequence `
    -Expected @('NoOp') `
    -Actual @(Get-ObsidianMsixLifecyclePlan -Action Uninstall -IsInstalled $false) `
    -Message 'Removing an absent current-user MSIX must be idempotent.'

$steamTemplate = Get-Content -Raw -LiteralPath (
    Join-Path $repositoryRoot 'packaging\steam\installscript.vdf.in'
)
$depotTemplate = Get-Content -Raw -LiteralPath (
    Join-Path $repositoryRoot 'packaging\steam\depot_build_template.vdf'
)
Assert-True `
    -Condition $steamTemplate.Contains('"Run Process On Uninstall"') `
    -Message 'The Steam package must declare its supported uninstall hook.'
Assert-True `
    -Condition $steamTemplate.Contains('-Hook Update') `
    -Message 'The Steam install script must run update recovery before first launch of a build.'
Assert-True `
    -Condition $steamTemplate.Contains('-Hook Uninstall') `
    -Message 'The Steam uninstall hook must invoke the native recovery contract.'
Assert-True `
    -Condition (($steamTemplate | Select-String -Pattern '"AsCurrentUser" "1"' -AllMatches).Matches.Count -eq 2) `
    -Message 'Both Steam lifecycle calls must run in the current user context.'
Assert-True `
    -Condition $depotTemplate.Contains('"InstallScript" "installscript.vdf"') `
    -Message 'The Steam depot must mark the lifecycle file as an InstallScript.'
Assert-True `
    -Condition $depotTemplate.TrimStart().StartsWith('"DepotBuild"') `
    -Message 'The Steam depot template must use the documented DepotBuild root object.'
Assert-True `
    -Condition (-not $depotTemplate.Contains('"DepotBuildConfig"')) `
    -Message 'The obsolete DepotBuildConfig root must not reach steamcmd.'

$msixBuild = Get-Content -Raw -LiteralPath (
    Join-Path $repositoryRoot 'packaging\msix\Build-Msix.ps1'
)
$stagePopulationStart = $msixBuild.IndexOf("New-Item -ItemType Directory -Force -Path (Join-Path `$stage 'Assets')")
$stagePopulationEnd = $msixBuild.IndexOf('$manifest = Get-Content -Raw', $stagePopulationStart)
Assert-True `
    -Condition ($stagePopulationStart -ge 0 -and $stagePopulationEnd -gt $stagePopulationStart) `
    -Message 'The MSIX stage population boundary could not be verified.'
$stagePopulation = $msixBuild.Substring(
    $stagePopulationStart,
    $stagePopulationEnd - $stagePopulationStart
)
Assert-True `
    -Condition (-not $stagePopulation.Contains('Invoke-ObsidianRecovery.ps1')) `
    -Message 'Lifecycle companion scripts must not be presented as automatic in-package MSIX hooks.'
Assert-True `
    -Condition $msixBuild.Contains('Deploy-ObsidianMsix.ps1') `
    -Message 'The MSIX build must publish the recovery-aware companion deployment pipeline.'
Assert-True `
    -Condition ($msixBuild.Contains('shell-watchdog-recovery.exe') -and
        $msixBuild.Contains('$companionWatchdog')) `
    -Message 'The MSIX output must publish and checksum the out-of-package recovery holder.'
$msixDeploy = Get-Content -Raw -LiteralPath (
    Join-Path $repositoryRoot 'packaging\msix\Deploy-ObsidianMsix.ps1'
)
Assert-True `
    -Condition ($msixDeploy.Contains('-WatchdogPath $companionWatchdog') -and
        $msixDeploy.Contains('$recoveryHolder.StandardInput.Close()')) `
    -Message 'The MSIX mutation must remain inside the companion watchdog holder lifetime.'

foreach ($binary in @('shell-app.exe', 'shell-watchdog.exe')) {
    Assert-True `
        -Condition (Test-Path -LiteralPath (Join-Path $ReleaseDirectory $binary) -PathType Leaf) `
        -Message "Release binary required by packaging tests is missing: $binary"
}

$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
    'obsidian-packaging-tests-' + [guid]::NewGuid().ToString('N')
)
$previousLocalAppData = $env:LOCALAPPDATA
try {
    New-Item -ItemType Directory -Force -Path $temporaryRoot | Out-Null
    $env:LOCALAPPDATA = Join-Path $temporaryRoot 'LocalAppData'
    New-Item -ItemType Directory -Force -Path $env:LOCALAPPDATA | Out-Null

    $steamOutput = Join-Path $temporaryRoot 'steam'
    & (Join-Path $repositoryRoot 'packaging\steam\Build-SteamLayout.ps1') `
        -OutputDirectory $steamOutput `
        -Version '9.8.7.6' `
        -SkipCargoBuild

    $generatedInstallScriptPath = Join-Path $steamOutput 'content\installscript.vdf'
    $generatedInstallScript = Get-Content -Raw -LiteralPath $generatedInstallScriptPath
    Assert-True `
        -Condition $generatedInstallScript.Contains('ObsidianGlassRecovery_9_8_7_6') `
        -Message 'The generated Steam run marker must be unique per release version.'
    Assert-True `
        -Condition (-not $generatedInstallScript.Contains('@@VERSION_KEY@@')) `
        -Message 'No Steam install-script template token may reach a release layout.'

    & (Join-Path $repositoryRoot 'packaging\common\Invoke-ObsidianRecovery.ps1') `
        -Hook Update `
        -AppRoot $ReleaseDirectory `
        -DryRun

    $missingWatchdogFailed = $false
    try {
        & (Join-Path $repositoryRoot 'packaging\common\Invoke-ObsidianRecovery.ps1') `
            -Hook Uninstall `
            -AppRoot (Join-Path $temporaryRoot 'missing-watchdog')
    } catch {
        $missingWatchdogFailed = $true
    }
    Assert-True `
        -Condition $missingWatchdogFailed `
        -Message 'A real lifecycle call must fail closed when the watchdog is unavailable.'

    $settingsRoot = Join-Path $env:LOCALAPPDATA 'Minha UI'
    $whatIfExport = Join-Path $temporaryRoot 'whatif-settings-export'
    New-Item -ItemType Directory -Force -Path $settingsRoot | Out-Null
    Set-Content -LiteralPath (Join-Path $settingsRoot 'settings.json') -Value '{}' -Encoding utf8
    & (Join-Path $repositoryRoot 'packaging\common\Remove-ObsidianGlass.ps1') `
        -SettingsAction Export `
        -ExportPath $whatIfExport `
        -AppRoot $ReleaseDirectory `
        -WhatIf
    Assert-True `
        -Condition (-not (Test-Path -LiteralPath $whatIfExport)) `
        -Message 'WhatIf must not create or copy a Settings export.'

    $dummyMsix = Join-Path $temporaryRoot 'ObsidianGlass-test.msix'
    New-Item -ItemType File -Path $dummyMsix | Out-Null
    $deploymentOutput = @(
        & (Join-Path $repositoryRoot 'packaging\msix\Deploy-ObsidianMsix.ps1') `
            -Action InstallOrUpdate `
            -PackagePath $dummyMsix `
            -DryRun 6>&1
    ) -join [Environment]::NewLine
    Assert-True `
        -Condition $deploymentOutput.Contains('MSIX lifecycle step: InstallOrUpdate') `
        -Message 'The MSIX companion pipeline dry-run must expose its planned package step.'
} finally {
    $env:LOCALAPPDATA = $previousLocalAppData
    Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Output 'Packaging lifecycle tests passed.'
