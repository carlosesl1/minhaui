$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$scriptsRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scriptsRoot "ArchitecturePolicy.psm1") -Force

function Assert-Equal {
    param(
        [Parameter(Mandatory)]
        $Expected,
        [Parameter(Mandatory)]
        $Actual,
        [Parameter(Mandatory)]
        [string] $Message
    )

    if ($Expected -ne $Actual) {
        throw "$Message`nExpected: $Expected`nActual: $Actual"
    }
}

function Read-Fixture {
    param([Parameter(Mandatory)][string] $Name)

    $path = Join-Path $PSScriptRoot "fixtures/$Name"
    Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
}

$validMetadata = Read-Fixture "valid-workspace-metadata.json"
$validViolations = @(Test-WorkspaceDependencyPolicy -Metadata $validMetadata)
Assert-Equal 0 $validViolations.Count "The approved workspace graph must be accepted."

$invalidMetadata = Read-Fixture "invalid-workspace-metadata.json"
$invalidViolations = @(Test-WorkspaceDependencyPolicy -Metadata $invalidMetadata)
Assert-Equal 1 $invalidViolations.Count "One forbidden dependency must be reported."
Assert-Equal `
    "forbidden workspace dependency: shell-core -> shell-platform-windows" `
    $invalidViolations[0] `
    "The dependency violation must identify both crates."

$fixtureRepository = Join-Path $PSScriptRoot "fixtures/repository"
$observedAllows = @(Get-ArchitectureAllows -RepositoryRoot $fixtureRepository)
Assert-Equal 1 $observedAllows.Count "The scanner must return a function-level allow."
Assert-Equal `
    "crates/sample/src/lib.rs" `
    $observedAllows[0].file `
    "The scanner must normalize repository paths."
Assert-Equal `
    "fixture_function" `
    $observedAllows[0].symbol `
    "The scanner must associate the allow with its function."

$allow = [pscustomobject]@{
    file = "crates/example.rs"
    rule = "clippy::too_many_arguments"
    symbol = "example"
}
$expiredException = [pscustomobject]@{
    id = "ARCH-EX-001"
    file = "crates/example.rs"
    rule = "clippy::too_many_arguments"
    symbol = "example"
    owner = "maintainer"
    reason = "fixture"
    risk = "fixture"
    compensation = "fixture"
    expires = "2026-07-14"
    removal_plan = "fixture"
}
$exceptionViolations = @(
    Test-ArchitectureExceptionPolicy `
        -Exceptions @($expiredException) `
        -Allows @($allow) `
        -Today ([datetime]::ParseExact("2026-07-15", "yyyy-MM-dd", $null))
)
Assert-Equal 1 $exceptionViolations.Count "An expired exception must fail governance."
Assert-Equal `
    "expired architecture exception: ARCH-EX-001 expired 2026-07-14" `
    $exceptionViolations[0] `
    "The expired exception must be actionable."

function New-NativeSurfaceSource {
    param(
        [Parameter(Mandatory)] [string] $File,
        [Parameter(Mandatory)] [string] $Content
    )

    [pscustomobject]@{
        file = $File
        content = $Content
    }
}

$nativeSurfaceFixtures = @(
    [pscustomobject]@{
        name = "renderer field on RuntimeSurfaces"
        sources = @(
            New-NativeSurfaceSource `
                -File "crates/shell-platform-windows/src/win32_owner.rs" `
                -Content @'
struct RuntimeSurfaces {
    renderer: CompositionRenderer,
}
'@
        )
        expected = 1
    },
    [pscustomobject]@{
        name = "surface field on RuntimeSurfaces"
        sources = @(
            New-NativeSurfaceSource `
                -File "crates/shell-platform-windows/src/win32_owner.rs" `
                -Content @'
struct RuntimeSurfaces {
    dock_surface: WindowSurface,
}
'@
        )
        expected = 1
    },
    [pscustomobject]@{
        name = "native ownership outside the runtime module"
        sources = @(
            New-NativeSurfaceSource `
                -File "crates/shell-platform-windows/src/win32_overlay.rs" `
                -Content @'
struct OverlayResources {
    renderer: Option<CompositionRenderer>,
    surface: WindowSurface,
}
'@
        )
        expected = 2
    },
    [pscustomobject]@{
        name = "presentation helper coupled to RuntimeSurfaces"
        sources = @(
            New-NativeSurfaceSource `
                -File "crates/shell-platform-windows/src/win32_surface_present.rs" `
                -Content @'
fn handle_present(
    update: SurfaceUpdate,
    runtime: &mut RuntimeSurfaces,
) -> Result<()> {
    runtime.rebuild_native_surfaces(update)
}
'@
        )
        expected = 1
    },
    [pscustomobject]@{
        name = "role-level runtime interface"
        sources = @(
            New-NativeSurfaceSource `
                -File "crates/shell-platform-windows/src/win32_owner.rs" `
                -Content @'
struct RuntimeSurfaces {
    surface_runtime: Win32NativeSurfaceRuntime,
}

fn present(runtime: &mut Win32NativeSurfaceRuntime, role: ShowcaseRole) {
    runtime.redraw(role, scenes());
}

fn runtime_preview_scene(runtime: &mut RuntimeSurfaces) {
    runtime.refresh_preview_scene();
}
'@
        )
        expected = 0
    }
)

foreach ($fixture in $nativeSurfaceFixtures) {
    $violations = @(Test-NativeSurfaceOwnershipPolicy -Sources $fixture.sources)
    Assert-Equal `
        $fixture.expected `
        $violations.Count `
        "Native-surface policy fixture failed: $($fixture.name)."
}

$publicFacadeSources = @(
    (New-NativeSurfaceSource `
        -File "crates/shell-platform-windows/src/lib.rs" `
        -Content @'
pub use dock_controller::DockController;
pub use win32::{ShowcaseRunConfig, run_showcase};
pub enum PlatformEvent { DisplayChanged }
pub const fn crate_identity() -> &'static str { "shell-platform-windows" }
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-renderer/src/lib.rs" `
        -Content @'
pub mod native;
pub use geometry::{DipPoint, PhysicalRect};
pub use color::Rgba8;
pub const INTERNAL_LIMIT: usize = 8;
pub const fn crate_identity() -> &'static str { "shell-renderer" }
'@)
)
$publicFacadeViolations = @(Test-PublicFacadePolicy -Sources $publicFacadeSources)
Assert-Equal 4 $publicFacadeViolations.Count "Facade policy must reject four internal exports."

$compliantFacadeSources = @(
    (New-NativeSurfaceSource `
        -File "crates/shell-platform-windows/src/lib.rs" `
        -Content @'
pub use win32::{ShowcaseRunConfig, run_showcase};
pub const fn crate_identity() -> &'static str { "shell-platform-windows" }
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-renderer/src/lib.rs" `
        -Content @'
pub mod native;
pub use geometry::{DipPoint, PhysicalRect};
pub const fn crate_identity() -> &'static str { "shell-renderer" }
'@)
)
$compliantFacadeViolations = @(Test-PublicFacadePolicy -Sources $compliantFacadeSources)
Assert-Equal 0 $compliantFacadeViolations.Count "Approved facades must remain valid."

$validWatchdogBoundary = @(
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/lib.rs" `
        -Content @'
#![deny(unsafe_code)]
mod taskbar_restore;
#[cfg(windows)]
#[allow(unsafe_code, reason = "ADR-0011")]
mod taskbar_restore_win32;
#[cfg(windows)]
#[allow(unsafe_code, reason = "ADR-0011")]
mod lifecycle_win32;
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/taskbar_restore.rs" `
        -Content @'
#![deny(unsafe_code)]
fn plan() {}
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/taskbar_restore_win32.rs" `
        -Content @'
unsafe extern "system" fn callback() {}
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/lifecycle_win32.rs" `
        -Content @'
unsafe fn named_gate() {}
'@)
)
$watchdogBoundaryViolations = @(
    Test-WatchdogNativeRecoveryBoundary -Sources $validWatchdogBoundary
)
Assert-Equal 0 $watchdogBoundaryViolations.Count "The isolated watchdog FFI boundary must pass."

$invalidWatchdogBoundary = @(
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/lib.rs" `
        -Content @'
#![deny(unsafe_code)]
#[cfg(windows)]
#[allow(unsafe_code, reason = "ADR-0011")]
mod taskbar_restore_win32;
#[cfg(windows)]
#[allow(unsafe_code, reason = "ADR-0011")]
mod lifecycle_win32;
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/taskbar_restore.rs" `
        -Content @'
use windows::Win32::Foundation::HWND;
fn plan() { unsafe {} }
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/taskbar_restore_win32.rs" `
        -Content @'
unsafe extern "system" fn callback() {}
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/lifecycle_win32.rs" `
        -Content @'
unsafe fn named_gate() {}
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/third_win32.rs" `
        -Content @'
unsafe fn forbidden_third_adapter() {}
'@)
)
$invalidWatchdogBoundaryViolations = @(
    Test-WatchdogNativeRecoveryBoundary -Sources $invalidWatchdogBoundary
)
Assert-Equal 3 $invalidWatchdogBoundaryViolations.Count "Boundary drift must be rejected."

$duplicatedDiagnosticsPolicySources = @(
    (New-NativeSurfaceSource `
        -File "crates/shell-platform-windows/src/diagnostics.rs" `
        -Content @'
fn redact_value(key: &str, value: &str) -> String { value.to_owned() }
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/diagnostics.rs" `
        -Content @'
struct RotationPolicy { max_files: usize, max_file_bytes: u64 }
fn redact(key: &str, value: &str) -> String { value.to_owned() }
'@)
)
$duplicatedDiagnosticsPolicyViolations = @(
    Test-DiagnosticsPolicyOwnership -Sources $duplicatedDiagnosticsPolicySources
)
Assert-Equal `
    5 `
    $duplicatedDiagnosticsPolicyViolations.Count `
    "Diagnostics privacy and retention policy must have one owner."

$sharedDiagnosticsPolicySources = @(
    (New-NativeSurfaceSource `
        -File "crates/shell-diagnostics/src/lib.rs" `
        -Content @'
pub struct RetentionPolicy;
fn is_sensitive_key(key: &str) -> bool { false }
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-platform-windows/src/diagnostics.rs" `
        -Content @'
use shell_diagnostics::STANDARD_DIAGNOSTIC_POLICY;
'@),
    (New-NativeSurfaceSource `
        -File "crates/shell-watchdog/src/diagnostics.rs" `
        -Content @'
use shell_diagnostics::{RetentionPolicy, STANDARD_DIAGNOSTIC_POLICY};
fn retained_log_segments(policy: RetentionPolicy, sizes: &[u64]) -> Vec<usize> {
    policy.retained_segments(sizes)
}
'@)
)
$sharedDiagnosticsPolicyViolations = @(
    Test-DiagnosticsPolicyOwnership -Sources $sharedDiagnosticsPolicySources
)
Assert-Equal 0 $sharedDiagnosticsPolicyViolations.Count "Shared diagnostics policy must pass."

Write-Output "Architecture policy tests passed."
