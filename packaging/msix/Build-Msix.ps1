[CmdletBinding()]
param(
    [ValidatePattern('^\d+\.\d+\.\d+\.\d+$')]
    [string]$Version = '0.1.0.0',

    [string]$Publisher = 'CN=REPLACE-WITH-YOUR-CERTIFICATE-SUBJECT',

    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'out'),

    [switch]$SkipCargoBuild,

    [string]$CertificatePath,

    [string]$CertificatePasswordEnvironmentVariable
)

$ErrorActionPreference = 'Stop'
$repo = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$output = [System.IO.Path]::GetFullPath($OutputDirectory)
$stage = Join-Path $output 'layout'
$release = Join-Path $repo 'target\x86_64-pc-windows-msvc\release'

if (-not $SkipCargoBuild) {
    & cargo build --release -p shell-app -p shell-watchdog
    if ($LASTEXITCODE -ne 0) { throw 'Cargo release build failed.' }
}

Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path (Join-Path $stage 'Assets') | Out-Null
Copy-Item -LiteralPath (Join-Path $release 'shell-app.exe') -Destination $stage
Copy-Item -LiteralPath (Join-Path $release 'shell-watchdog.exe') -Destination $stage
Copy-Item -LiteralPath (Join-Path $repo 'packaging\common\Invoke-ObsidianRecovery.ps1') -Destination $stage
Copy-Item -LiteralPath (Join-Path $repo 'packaging\common\Remove-ObsidianGlass.ps1') -Destination $stage

$manifest = Get-Content -Raw (Join-Path $PSScriptRoot 'AppxManifest.xml.in')
$manifest = $manifest.Replace('@@VERSION@@', $Version).Replace('@@PUBLISHER@@', $Publisher)
[System.IO.File]::WriteAllText((Join-Path $stage 'AppxManifest.xml'), $manifest, [System.Text.UTF8Encoding]::new($false))

Add-Type -AssemblyName System.Drawing
function New-BrandAsset([string]$Path, [int]$Width, [int]$Height) {
    $bitmap = [System.Drawing.Bitmap]::new($Width, $Height)
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.Clear([System.Drawing.Color]::FromArgb(17, 21, 27))
            $pen = [System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(45, 125, 255), [Math]::Max(2, $Width / 18))
            try {
                $margin = [Math]::Max(4, [int]($Width * 0.18))
                $size = [Math]::Min($Width, $Height) - (2 * $margin)
                $graphics.DrawEllipse($pen, ($Width - $size) / 2, ($Height - $size) / 2, $size, $size)
            } finally { $pen.Dispose() }
        } finally { $graphics.Dispose() }
        $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $bitmap.Dispose() }
}

New-BrandAsset (Join-Path $stage 'Assets\StoreLogo.png') 50 50
New-BrandAsset (Join-Path $stage 'Assets\Square44x44Logo.png') 44 44
New-BrandAsset (Join-Path $stage 'Assets\Square150x150Logo.png') 150 150
New-BrandAsset (Join-Path $stage 'Assets\Wide310x150Logo.png') 310 150

$makeAppx = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter makeappx.exe |
    Where-Object FullName -Match '\\x64\\makeappx\.exe$' |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $makeAppx) { throw 'MakeAppx.exe was not found. Install the Windows SDK.' }

$package = Join-Path $output "ObsidianGlass-$Version-x64.msix"
& $makeAppx pack /d $stage /p $package /o
if ($LASTEXITCODE -ne 0) { throw 'MakeAppx failed.' }

if ($CertificatePath) {
    $signTool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter signtool.exe |
        Where-Object FullName -Match '\\x64\\signtool\.exe$' |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
    if (-not $signTool) { throw 'SignTool.exe was not found.' }
    $password = if ($CertificatePasswordEnvironmentVariable) {
        [Environment]::GetEnvironmentVariable($CertificatePasswordEnvironmentVariable)
    } else { $null }
    $arguments = @('sign', '/fd', 'SHA256', '/f', $CertificatePath)
    if ($password) { $arguments += @('/p', $password) }
    $arguments += $package
    & $signTool @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Package signing failed.' }
}

Get-FileHash -Algorithm SHA256 -LiteralPath $package |
    Format-List Algorithm, Hash, Path |
    Out-File -Encoding utf8 (Join-Path $output 'SHA256SUMS.txt')
Write-Host "MSIX ready: $package"

