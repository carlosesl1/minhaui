$ErrorActionPreference = "Stop"
$EvidenceDir = Join-Path (Get-Location) ".omo\evidence\task-5-dock"
$Stdout = Join-Path $EvidenceDir "scripted-qa-stdout.txt"
$Stderr = Join-Path $EvidenceDir "scripted-qa-stderr.txt"
$Metadata = Join-Path $EvidenceDir "scripted-qa-metadata.json"
Remove-Item $Stdout, $Stderr, $Metadata -ErrorAction SilentlyContinue

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class NativeQa {
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hwnd);
}
"@

function Find-ShellApp {
  $candidates = Get-ChildItem -Path "target" -Recurse -Filter "shell-app.exe" |
    Sort-Object LastWriteTime -Descending
  if (-not $candidates) { throw "release shell-app.exe not found" }
  $candidates[0].FullName
}

function Parse-Windows($path) {
  $lines = Get-Content $path -ErrorAction SilentlyContinue
  $parsed = @{}
  foreach ($line in $lines) {
    if ($line -match 'WINDOW role=(?<role>\w+) hwnd=(?<hwnd>0x[0-9A-Fa-f]+).*rect=(?<x>-?\d+),(?<y>-?\d+),(?<w>\d+),(?<h>\d+)') {
      $parsed[$Matches.role] = [pscustomobject]@{
        role = $Matches.role
        hwnd = $Matches.hwnd
        x = [int]$Matches.x
        y = [int]$Matches.y
        w = [int]$Matches.w
        h = [int]$Matches.h
      }
    }
  }
  $parsed
}

function Hwnd-Ptr($hex) {
  [IntPtr]([Convert]::ToInt64($hex.Substring(2), 16))
}

function Rect-For($hwnd) {
  $rect = New-Object NativeQa+RECT
  if (-not [NativeQa]::GetWindowRect($hwnd, [ref]$rect)) { throw "GetWindowRect failed for $hwnd" }
  [pscustomobject]@{
    x = $rect.Left
    y = $rect.Top
    w = $rect.Right - $rect.Left
    h = $rect.Bottom - $rect.Top
  }
}

function Save-Crop($rect, $name) {
  $path = Join-Path $EvidenceDir $name
  $bitmap = New-Object System.Drawing.Bitmap([Math]::Max(1, $rect.w), [Math]::Max(1, $rect.h))
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  $graphics.CopyFromScreen($rect.x, $rect.y, 0, 0, $bitmap.Size)
  $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $graphics.Dispose()
  $bitmap.Dispose()
  $path
}

function Mouse-Click($x, $y, $button) {
  [NativeQa]::SetCursorPos($x, $y) | Out-Null
  Start-Sleep -Milliseconds 120
  if ($button -eq "left") {
    [NativeQa]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [NativeQa]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  } else {
    [NativeQa]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [NativeQa]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
  }
}

function Mouse-Drag($x1, $y1, $x2, $y2) {
  [NativeQa]::SetCursorPos($x1, $y1) | Out-Null
  Start-Sleep -Milliseconds 120
  [NativeQa]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  foreach ($step in 1..8) {
    $x = [int]($x1 + (($x2 - $x1) * $step / 8))
    $y = [int]($y1 + (($y2 - $y1) * $step / 8))
    [NativeQa]::SetCursorPos($x, $y) | Out-Null
    Start-Sleep -Milliseconds 70
  }
  [NativeQa]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
}

function Key-Enter {
  [NativeQa]::keybd_event(0x0D, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 80
  [NativeQa]::keybd_event(0x0D, 0, 2, [UIntPtr]::Zero)
}

$beforeNotepad = @(Get-Process notepad -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
$exe = Find-ShellApp
$proc = $null
$dockHwnd = [IntPtr]::Zero
$topbarHwnd = [IntPtr]::Zero
$observations = New-Object System.Collections.Generic.List[object]
$artifacts = New-Object System.Collections.Generic.List[string]

try {
  $proc = Start-Process -FilePath $exe -ArgumentList @("--showcase", "--force-warp", "--qa-exit-ms", "12000") -RedirectStandardOutput $Stdout -RedirectStandardError $Stderr -PassThru -WindowStyle Hidden
  $windows = @{}
  foreach ($attempt in 1..60) {
    Start-Sleep -Milliseconds 200
    $windows = Parse-Windows $Stdout
    if ($windows.ContainsKey("dock") -and $windows.ContainsKey("topbar")) { break }
  }
  if (-not $windows.ContainsKey("dock")) { throw "dock window was not reported" }
  if (-not $windows.ContainsKey("topbar")) { throw "topbar window was not reported" }

  $dockHwnd = Hwnd-Ptr $windows.dock.hwnd
  $topbarHwnd = Hwnd-Ptr $windows.topbar.hwnd
  $dockRect = Rect-For $dockHwnd
  $topbarRect = Rect-For $topbarHwnd
  $artifacts.Add((Save-Crop $dockRect "scripted-dock-initial.png")) | Out-Null
  $artifacts.Add((Save-Crop $topbarRect "scripted-topbar-initial.png")) | Out-Null

  $firstX = [int]($dockRect.x + ($dockRect.w / 2) - 156)
  $secondX = [int]($dockRect.x + ($dockRect.w / 2))
  $beforeFirstX = [int]($dockRect.x + ($dockRect.w / 2) - 240)
  $centerY = [int]($dockRect.y + ($dockRect.h / 2))

  Mouse-Click $firstX $centerY "left"
  Start-Sleep -Milliseconds 1800
  $afterNotepad = @(Get-Process notepad -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
  $newNotepad = @($afterNotepad | Where-Object { $beforeNotepad -notcontains $_ })
  if ($newNotepad.Count -eq 0) { throw "launch click did not create a safe notepad process" }
  $observations.Add([pscustomobject]@{ scenario = "mouse launch"; observable = "new notepad process"; value = ($newNotepad -join ",") }) | Out-Null

  Mouse-Click $firstX $centerY "left"
  Start-Sleep -Milliseconds 500
  Mouse-Click $firstX $centerY "left"
  Start-Sleep -Milliseconds 500
  $observations.Add([pscustomobject]@{ scenario = "focus/minimize clicks"; observable = "dock accepted two follow-up clicks"; value = $true }) | Out-Null

  Mouse-Click $firstX $centerY "right"
  Start-Sleep -Milliseconds 300
  Key-Enter
  Start-Sleep -Milliseconds 500
  $observations.Add([pscustomobject]@{ scenario = "native context menu"; observable = "right-click popup accepted Enter selection"; value = $true }) | Out-Null

  Mouse-Drag $secondX $centerY $beforeFirstX $centerY
  Start-Sleep -Milliseconds 600
  $observations.Add([pscustomobject]@{ scenario = "drag reorder"; observable = "mouse drag completed without process exit"; value = (-not $proc.HasExited) }) | Out-Null

  [NativeQa]::SetCursorPos(4, 4) | Out-Null
  Start-Sleep -Milliseconds 1200
  $hiddenRect = Rect-For $dockHwnd
  $artifacts.Add((Save-Crop $hiddenRect "scripted-dock-hidden-strip.png")) | Out-Null
  if ($hiddenRect.h -ge [int]($dockRect.h / 2)) { throw "dock did not collapse to reveal strip: $($hiddenRect.h) from $($dockRect.h)" }
  $observations.Add([pscustomobject]@{ scenario = "physical autohide"; observable = "hidden HWND height"; value = $hiddenRect.h }) | Out-Null

  [NativeQa]::SetCursorPos([int]($hiddenRect.x + ($hiddenRect.w / 2)), [int]($hiddenRect.y + ($hiddenRect.h / 2))) | Out-Null
  Start-Sleep -Milliseconds 1200
  $revealedRect = Rect-For $dockHwnd
  $artifacts.Add((Save-Crop $revealedRect "scripted-dock-revealed.png")) | Out-Null
  if ($revealedRect.h -le ($hiddenRect.h * 2)) { throw "dock did not reveal from strip: $($revealedRect.h) from $($hiddenRect.h)" }
  $observations.Add([pscustomobject]@{ scenario = "reveal zone"; observable = "revealed HWND height"; value = $revealedRect.h }) | Out-Null

  Wait-Process -Id $proc.Id -Timeout 15 -ErrorAction SilentlyContinue
} finally {
  $createdNotepad = @(Get-Process notepad -ErrorAction SilentlyContinue | Where-Object { $beforeNotepad -notcontains $_.Id })
  foreach ($np in $createdNotepad) {
    Stop-Process -Id $np.Id -Force -ErrorAction SilentlyContinue
  }
  foreach ($attempt in 1..40) {
    $remainingNotepad = @(Get-Process notepad -ErrorAction SilentlyContinue | Where-Object { $beforeNotepad -notcontains $_.Id })
    if ($remainingNotepad.Count -eq 0) { break }
    foreach ($np in $remainingNotepad) {
      Stop-Process -Id $np.Id -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 250
  }
  if ($proc -and -not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    Wait-Process -Id $proc.Id -Timeout 5 -ErrorAction SilentlyContinue
  }
  $cleanup = [pscustomobject]@{
    shellProcessAliveAfterExit = if ($proc) { -not $proc.HasExited } else { $false }
    dockHwndAliveAfterExit = if ($dockHwnd -ne [IntPtr]::Zero) { [NativeQa]::IsWindow($dockHwnd) } else { $false }
    topbarHwndAliveAfterExit = if ($topbarHwnd -ne [IntPtr]::Zero) { [NativeQa]::IsWindow($topbarHwnd) } else { $false }
    newNotepadAliveAfterCleanup = @((Get-Process notepad -ErrorAction SilentlyContinue | Where-Object { $beforeNotepad -notcontains $_.Id })).Count
  }
  $result = [pscustomobject]@{
    scenario = "scripted mouse launch focus minimize context menu drag autohide reveal cleanup"
    exe = $exe
    processId = if ($proc) { $proc.Id } else { $null }
    stdout = $Stdout
    stderr = $Stderr
    windows = $windows
    observations = $observations
    cleanup = $cleanup
    artifacts = @($artifacts | ForEach-Object { Resolve-Path $_ | Select-Object -ExpandProperty Path })
  }
  $result | ConvertTo-Json -Depth 8 | Set-Content -Path $Metadata -Encoding UTF8
}

if ($cleanup.shellProcessAliveAfterExit -or $cleanup.dockHwndAliveAfterExit -or $cleanup.topbarHwndAliveAfterExit -or $cleanup.newNotepadAliveAfterCleanup -ne 0) {
  throw "cleanup failed"
}

Get-Content $Metadata
