param(
    [string] $RepositoryRoot = (Split-Path -Parent $PSScriptRoot),
    [datetime] $Today = (Get-Date)
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

Import-Module (Join-Path $PSScriptRoot "ArchitecturePolicy.psm1") -Force

$metadataJson = & cargo metadata --format-version 1 --no-deps 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "cargo metadata failed:`n$($metadataJson -join [Environment]::NewLine)"
}
$metadata = ($metadataJson -join [Environment]::NewLine) | ConvertFrom-Json

$exceptionsPath = Join-Path $RepositoryRoot "docs/architecture/exceptions.json"
$exceptionRegistry = Get-Content -Raw -LiteralPath $exceptionsPath | ConvertFrom-Json
$exceptions = @($exceptionRegistry.exceptions)
$allows = @(Get-ArchitectureAllows -RepositoryRoot $RepositoryRoot)

$violations = @(
    Test-WorkspaceDependencyPolicy -Metadata $metadata
    Test-ArchitectureExceptionPolicy -Exceptions $exceptions -Allows $allows -Today $Today
    Test-NativeSurfaceOwnershipPolicy -RepositoryRoot $RepositoryRoot
    Test-PublicFacadePolicy -RepositoryRoot $RepositoryRoot
    Test-DiagnosticsPolicyOwnership -RepositoryRoot $RepositoryRoot
)

if ($violations.Count -gt 0) {
    Write-Error ("Architecture policy violations:`n- " + ($violations -join "`n- "))
    exit 1
}

Write-Output (
    "Architecture policy passed: {0} workspace crates, {1} registered exception(s)." -f `
        @($metadata.workspace_members).Count,
        $exceptions.Count
)
