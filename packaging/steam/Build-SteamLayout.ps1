[CmdletBinding()]
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'out'),

    [ValidatePattern('^\d+\.\d+\.\d+\.\d+$')]
    [string]$Version = '0.1.0.0',

    [switch]$SkipCargoBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$release = Join-Path $repo 'target\x86_64-pc-windows-msvc\release'
$content = Join-Path ([System.IO.Path]::GetFullPath($OutputDirectory)) 'content'

if (-not $SkipCargoBuild) {
    & cargo build --release -p shell-app -p shell-watchdog
    if ($LASTEXITCODE -ne 0) { throw 'Cargo release build failed.' }
}

Remove-Item -LiteralPath $content -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $content | Out-Null
Copy-Item -LiteralPath (Join-Path $release 'shell-app.exe') -Destination $content
Copy-Item -LiteralPath (Join-Path $release 'shell-watchdog.exe') -Destination $content
Copy-Item -LiteralPath (Join-Path $repo 'packaging\common\Invoke-ObsidianRecovery.ps1') -Destination $content
Copy-Item -LiteralPath (Join-Path $repo 'packaging\common\Remove-ObsidianGlass.ps1') -Destination $content
Copy-Item -LiteralPath (Join-Path $repo 'PRIVACY.md') -Destination $content
Copy-Item -LiteralPath (Join-Path $repo 'SUPPORT.md') -Destination $content
Copy-Item -LiteralPath (Join-Path $repo 'THIRD_PARTY_LICENSES.md') -Destination $content

$installScriptTemplate = Get-Content -Raw (Join-Path $PSScriptRoot 'installscript.vdf.in')
$installScript = $installScriptTemplate.Replace('@@VERSION_KEY@@', $Version.Replace('.', '_'))
[System.IO.File]::WriteAllText(
    (Join-Path $content 'installscript.vdf'),
    $installScript,
    [System.Text.UTF8Encoding]::new($false)
)

Get-ChildItem -LiteralPath $content -File | Get-FileHash -Algorithm SHA256 |
    Sort-Object Path |
    ForEach-Object { "$($_.Hash)  $([System.IO.Path]::GetFileName($_.Path))" } |
    Set-Content -Encoding utf8 (Join-Path $content 'SHA256SUMS.txt')
$portable = Join-Path ([System.IO.Path]::GetFullPath($OutputDirectory)) "ObsidianGlass-$Version-portable-win-x64.zip"
Remove-Item -LiteralPath $portable -Force -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $content '*') -DestinationPath $portable -CompressionLevel Optimal
Write-Host "Steam content layout ready: $content"
Write-Host "Portable test package ready: $portable"
