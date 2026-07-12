$ErrorActionPreference = "Stop"

$EvidenceRoot = Join-Path (Get-Location) ".omo\evidence\task-5-dock"
$PackDir = Join-Path $EvidenceRoot "canonical"
$RunId = "task5-" + (Get-Date -Format "yyyyMMdd-HHmmss")
if (Test-Path $PackDir) {
  Remove-Item -LiteralPath $PackDir -Recurse -Force
}
New-Item -ItemType Directory -Path $PackDir | Out-Null

$AppStdout = Join-Path $PackDir "app-stdout.txt"
$AppStderr = Join-Path $PackDir "app-stderr.txt"
$Manifest = Join-Path $PackDir "manifest.json"
$QaLog = Join-Path $PackDir "qa-run.txt"
$TempRoot = Join-Path $env:TEMP "Minha UI QA Space"
$DropScript = Join-Path $TempRoot "drop-target.cmd"
$UnpinnedExe = (Get-Command charmap.exe -ErrorAction Stop).Source

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class NativeQa {
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("kernel32.dll")] public static extern IntPtr GlobalAlloc(uint flags, UIntPtr bytes);
  [DllImport("kernel32.dll")] public static extern IntPtr GlobalLock(IntPtr hmem);
  [DllImport("kernel32.dll")] public static extern bool GlobalUnlock(IntPtr hmem);
  [DllImport("kernel32.dll")] public static extern IntPtr GlobalFree(IntPtr hmem);
  public static void PostDrop(IntPtr hwnd, string path) {
    byte[] encoded = Encoding.Unicode.GetBytes(path + "\0\0");
    int header = 20;
    IntPtr handle = GlobalAlloc(0x42, (UIntPtr)(header + encoded.Length));
    if (handle == IntPtr.Zero) throw new InvalidOperationException("GlobalAlloc failed");
    IntPtr memory = GlobalLock(handle);
    if (memory == IntPtr.Zero) {
      GlobalFree(handle);
      throw new InvalidOperationException("GlobalLock failed");
    }
    byte[] block = new byte[header + encoded.Length];
    BitConverter.GetBytes(header).CopyTo(block, 0);
    BitConverter.GetBytes(1).CopyTo(block, 16);
    encoded.CopyTo(block, header);
    Marshal.Copy(block, 0, memory, block.Length);
    GlobalUnlock(handle);
    SendMessage(hwnd, 0x0233, handle, IntPtr.Zero);
  }
}
"@

function Write-Log($message) {
  $line = "$(Get-Date -Format o) $message"
  Add-Content -Path $QaLog -Value $line
}

function Find-ShellApp {
  $candidate = Get-ChildItem -Path "target\x86_64-pc-windows-msvc\release", "target\release" -Filter "shell-app.exe" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
  if (-not $candidate) { throw "release shell-app.exe not found" }
  $candidate.FullName
}

function Parse-Windows {
  $parsed = @{}
  foreach ($line in (Get-Content $AppStdout -ErrorAction SilentlyContinue)) {
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

function Parse-DockStates {
  $states = @()
  foreach ($line in (Get-Content $AppStdout -ErrorAction SilentlyContinue)) {
    if ($line -notlike "DOCK_STATE *") { continue }
    $itemsText = ($line -split "items=", 2)[1]
    $items = @()
    if ($itemsText) {
      foreach ($entry in ($itemsText -split "\|")) {
        $parts = $entry -split ":", 5
        if ($parts.Count -eq 5) {
          $items += [pscustomobject]@{
            index = [int]$parts[0]
            id = $parts[1]
            app = $parts[2]
            pin = $parts[3]
            running = $parts[4]
          }
        }
      }
    }
    $states += [pscustomobject]@{ raw = $line; items = $items }
  }
  $states
}

function Wait-State($predicate, $label, $timeoutMs = 6000) {
  $deadline = [DateTime]::UtcNow.AddMilliseconds($timeoutMs)
  do {
    Start-Sleep -Milliseconds 200
    $states = Parse-DockStates
    $match = @($states | Where-Object $predicate | Select-Object -Last 1)
    if ($match.Count -gt 0) { return $match[-1] }
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "timed out waiting for state: $label"
}

function Hwnd-Ptr($hex) {
  [IntPtr]([Convert]::ToInt64($hex.Substring(2), 16))
}

function Rect-For($hwnd) {
  $rect = New-Object NativeQa+RECT
  if (-not [NativeQa]::GetWindowRect($hwnd, [ref]$rect)) { throw "GetWindowRect failed for $hwnd" }
  [pscustomobject]@{ x = $rect.Left; y = $rect.Top; w = $rect.Right - $rect.Left; h = $rect.Bottom - $rect.Top }
}

function Save-Crop($rect, $name) {
  $path = Join-Path $PackDir $name
  $bitmap = New-Object System.Drawing.Bitmap([Math]::Max(1, $rect.w), [Math]::Max(1, $rect.h))
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  $graphics.CopyFromScreen($rect.x, $rect.y, 0, 0, $bitmap.Size)
  $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $graphics.Dispose()
  $bitmap.Dispose()
  $path
}

function Menu-Crop($dockRect) {
  [pscustomobject]@{
    x = [Math]::Max(0, $dockRect.x - 24)
    y = [Math]::Max(0, $dockRect.y - 180)
    w = $dockRect.w + 220
    h = $dockRect.h + 220
  }
}

function Mouse-Click($x, $y, $button) {
  [NativeQa]::SetCursorPos($x, $y) | Out-Null
  Start-Sleep -Milliseconds 140
  if ($button -eq "left") {
    [NativeQa]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 90
    [NativeQa]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  } else {
    [NativeQa]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 90
    [NativeQa]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
  }
}

function Mouse-DragWithCapture($x1, $y1, $x2, $y2, $dockRect) {
  [NativeQa]::SetCursorPos($x1, $y1) | Out-Null
  Start-Sleep -Milliseconds 140
  [NativeQa]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  foreach ($step in 1..8) {
    $x = [int]($x1 + (($x2 - $x1) * $step / 8))
    $y = [int]($y1 + (($y2 - $y1) * $step / 8))
    [NativeQa]::SetCursorPos($x, $y) | Out-Null
    Start-Sleep -Milliseconds 90
    if ($step -eq 4) {
      Save-Crop $dockRect "drag-insertion.png" | Out-Null
    }
  }
  [NativeQa]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
}

function Press-Key($virtualKey) {
  [NativeQa]::keybd_event([byte]$virtualKey, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 80
  [NativeQa]::keybd_event([byte]$virtualKey, 0, 2, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 120
}

function Visual-Centers($state, $dockRect) {
  $itemSize = 52.0
  $separatorSize = $itemSize * 0.28
  $spacing = 8.0
  $padding = 14.0
  $visuals = @()
  $insertedSeparator = $false
  $hasPinned = @($state.items | Where-Object { $_.pin -eq "pinned" }).Count -gt 0
  foreach ($item in $state.items) {
    if (-not $insertedSeparator -and $hasPinned -and $item.pin -eq "unpinned") {
      $visuals += [pscustomobject]@{ type = "separator"; app = ""; id = ""; size = $separatorSize }
      $insertedSeparator = $true
    }
    $visuals += [pscustomobject]@{ type = "app"; app = $item.app; id = $item.id; size = $itemSize }
  }
  $total = ($visuals | Measure-Object -Property size -Sum).Sum + ([Math]::Max(0, $visuals.Count - 1) * $spacing) + ($padding * 2)
  $x = $dockRect.x + (($dockRect.w - $total) / 2.0) + $padding
  $centers = @{}
  foreach ($visual in $visuals) {
    if ($visual.type -eq "app") {
      $centers[$visual.app] = [pscustomobject]@{ x = [int]($x + ($visual.size / 2.0)); y = [int]($dockRect.y + ($dockRect.h / 2.0)) }
    }
    $x += $visual.size + $spacing
  }
  $centers
}

function Process-ByPath($path) {
  @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $path })
}

function Process-ByCommandLine($needle) {
  @(Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like "*$needle*" })
}

function Window-ForProcessPath($path) {
  $deadline = [DateTime]::UtcNow.AddSeconds(8)
  do {
    foreach ($process in Process-ByPath $path) {
      $p = Get-Process -Id $process.ProcessId -ErrorAction SilentlyContinue
      if ($p -and $p.MainWindowHandle -ne 0) { return $p }
    }
    Start-Sleep -Milliseconds 250
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "window process not found for $path"
}

function Artifact-Record($path) {
  $item = Get-Item -LiteralPath $path
  [pscustomobject]@{
    path = $item.FullName
    bytes = $item.Length
    mtimeUtc = $item.LastWriteTimeUtc.ToString("o")
    sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash
  }
}

$shellProc = $null
$unpinnedProc = $null
$dropCmdProcess = $null
$beforeNotepadIds = @(Get-Process notepad -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
$observations = New-Object System.Collections.Generic.List[object]

try {
  New-Item -ItemType Directory -Path $TempRoot -Force | Out-Null
  Set-Content -Path $DropScript -Encoding ASCII -Value "@echo off`r`ntitle MinhaUI-QA-Drop`r`ntimeout /t 25 >nul`r`n"
  Write-Log "prepared drop script $DropScript and external unpinned app $UnpinnedExe"

  $unpinnedProc = Start-Process -FilePath $UnpinnedExe -PassThru
  foreach ($attempt in 1..30) {
    Start-Sleep -Milliseconds 250
    $candidate = Get-Process -Id $unpinnedProc.Id -ErrorAction SilentlyContinue
    if ($candidate -and $candidate.MainWindowHandle -ne 0) { break }
  }
  $unpinnedWindow = Get-Process -Id $unpinnedProc.Id -ErrorAction SilentlyContinue
  if (-not $unpinnedWindow -or $unpinnedWindow.MainWindowHandle -eq 0) { throw "charmap.exe unpinned window did not appear" }
  Write-Log "started external unpinned app pid=$($unpinnedWindow.Id)"

  $exe = Find-ShellApp
  $oldTrace = $env:MINHA_UI_QA_TRACE
  $env:MINHA_UI_QA_TRACE = "1"
  $shellProc = Start-Process -FilePath $exe -ArgumentList @("--showcase", "--force-warp", "--qa-exit-ms", "30000") -RedirectStandardOutput $AppStdout -RedirectStandardError $AppStderr -PassThru -WindowStyle Hidden
  $env:MINHA_UI_QA_TRACE = $oldTrace
  Write-Log "started shell pid=$($shellProc.Id)"

  $windows = @{}
  foreach ($attempt in 1..80) {
    Start-Sleep -Milliseconds 200
    $windows = Parse-Windows
    if ($windows.ContainsKey("dock") -and $windows.ContainsKey("topbar")) { break }
  }
  if (-not $windows.ContainsKey("dock")) { throw "dock window was not reported" }
  $dockHwnd = Hwnd-Ptr $windows.dock.hwnd
  $topbarHwnd = Hwnd-Ptr $windows.topbar.hwnd
  $dockRect = Rect-For $dockHwnd
  $topbarRect = Rect-For $topbarHwnd
  Save-Crop $dockRect "dock-initial.png" | Out-Null
  Save-Crop $topbarRect "topbar-initial.png" | Out-Null

  $runningUnpinned = Wait-State { @($_.items | Where-Object { $_.app -eq "charmap.exe" -and $_.pin -eq "unpinned" -and $_.running -like "running:*" }).Count -gt 0 } "running unpinned charmap.exe"
  Save-Crop (Rect-For $dockHwnd) "running-unpinned.png" | Out-Null
  $observations.Add([pscustomobject]@{ scenario = "running unpinned appears"; observable = "DOCK_STATE"; value = $runningUnpinned.raw }) | Out-Null

  $centers = Visual-Centers $runningUnpinned (Rect-For $dockHwnd)
  $unpinnedPoint = $centers["charmap.exe"]
  [NativeQa]::SetCursorPos($unpinnedPoint.x, $unpinnedPoint.y) | Out-Null
  Start-Sleep -Milliseconds 250
  $postedPin = [NativeQa]::PostMessage($dockHwnd, 0x0111, [IntPtr]2, [IntPtr]::Zero)
  Write-Log "posted WM_COMMAND Pin result=$postedPin"
  $pinnedUnpinned = Wait-State { @($_.items | Where-Object { $_.app -eq "charmap.exe" -and $_.pin -eq "pinned" }).Count -gt 0 } "native menu Pin effect"
  $observations.Add([pscustomobject]@{ scenario = "native context Pin"; observable = "DOCK_STATE pin=pinned"; value = $pinnedUnpinned.raw }) | Out-Null

  [NativeQa]::PostDrop($dockHwnd, $DropScript)
  $dropped = Wait-State { @($_.items | Where-Object { $_.app -eq "drop-target.cmd" -and $_.pin -eq "pinned" }).Count -gt 0 } "drop path pinned"
  $centers = Visual-Centers $dropped (Rect-For $dockHwnd)
  $dropPoint = $centers["drop-target.cmd"]
  Mouse-Click $dropPoint.x $dropPoint.y "left"
  foreach ($attempt in 1..30) {
    Start-Sleep -Milliseconds 250
    $dropCmdProcess = @(Process-ByCommandLine "drop-target.cmd" | Select-Object -First 1)
    if ($dropCmdProcess) { break }
  }
  if (-not $dropCmdProcess) { throw "drop-target.cmd did not launch through ShellExecute" }
  $observations.Add([pscustomobject]@{ scenario = "drop path launch"; observable = "Win32 command line"; value = "$($dropCmdProcess.ProcessId):$($dropCmdProcess.CommandLine)" }) | Out-Null

  $latest = Parse-DockStates | Select-Object -Last 1
  $centers = Visual-Centers $latest (Rect-For $dockHwnd)
  $notePoint = $centers["notepad.exe"]
  Mouse-Click $notePoint.x $notePoint.y "left"
  $newNotepad = $null
  $deadline = [DateTime]::UtcNow.AddSeconds(8)
  do {
    $newNotepad = @(Get-Process notepad -ErrorAction SilentlyContinue | Where-Object { $beforeNotepadIds -notcontains $_.Id -and $_.MainWindowHandle -ne 0 } | Select-Object -First 1)
    if ($newNotepad) { break }
    Start-Sleep -Milliseconds 250
  } while ([DateTime]::UtcNow -lt $deadline)
  if (-not $newNotepad) { throw "pinned notepad launch did not create a window" }

  Mouse-Click $notePoint.x $notePoint.y "left"
  Start-Sleep -Milliseconds 900
  $foreground = [NativeQa]::GetForegroundWindow()
  if ($foreground -ne $newNotepad.MainWindowHandle) { throw "GetForegroundWindow did not match launched notepad" }
  $observations.Add([pscustomobject]@{ scenario = "focus"; observable = "GetForegroundWindow"; value = $foreground.ToString() }) | Out-Null

  Mouse-Click $notePoint.x $notePoint.y "left"
  Start-Sleep -Milliseconds 900
  if (-not [NativeQa]::IsIconic($newNotepad.MainWindowHandle)) { throw "IsIconic did not report minimized notepad" }
  Save-Crop (Rect-For $dockHwnd) "focus-minimized-indicators.png" | Out-Null
  $observations.Add([pscustomobject]@{ scenario = "minimize"; observable = "IsIconic"; value = $true }) | Out-Null

  $preDrag = Parse-DockStates | Select-Object -Last 1
  $centers = Visual-Centers $preDrag (Rect-For $dockHwnd)
  Mouse-DragWithCapture $centers["calc.exe"].x $centers["calc.exe"].y ($centers["notepad.exe"].x - 70) $centers["notepad.exe"].y (Rect-For $dockHwnd)
  $reordered = Wait-State { ($_.items[0].app -eq "calc.exe") } "drag reorder persisted"
  Save-Crop (Rect-For $dockHwnd) "drag-reordered.png" | Out-Null
  $observations.Add([pscustomobject]@{ scenario = "drag reorder"; observable = "DOCK_STATE first app"; value = $reordered.raw }) | Out-Null

  [NativeQa]::SetCursorPos(4, 4) | Out-Null
  Start-Sleep -Milliseconds 1400
  $hiddenRect = Rect-For $dockHwnd
  if ($hiddenRect.h -gt 12) { throw "hidden dock strip expected <=12 px, got $($hiddenRect.h)" }
  Save-Crop $hiddenRect "hidden-8px.png" | Out-Null
  $observations.Add([pscustomobject]@{ scenario = "autohide"; observable = "GetWindowRect height"; value = $hiddenRect.h }) | Out-Null

  [NativeQa]::SetCursorPos([int]($hiddenRect.x + ($hiddenRect.w / 2)), [int]($hiddenRect.y + ($hiddenRect.h / 2))) | Out-Null
  Start-Sleep -Milliseconds 1400
  $revealedRect = Rect-For $dockHwnd
  if ($revealedRect.h -lt 100) { throw "revealed dock expected >=100 px, got $($revealedRect.h)" }
  Save-Crop $revealedRect "revealed-180px.png" | Out-Null
  $observations.Add([pscustomobject]@{ scenario = "reveal"; observable = "GetWindowRect height"; value = $revealedRect.h }) | Out-Null

  $menuState = Parse-DockStates | Select-Object -Last 1
  $centers = Visual-Centers $menuState (Rect-For $dockHwnd)
  $menuPoint = $centers["charmap.exe"]
  Mouse-Click $menuPoint.x $menuPoint.y "right"
  Start-Sleep -Milliseconds 350
  Save-Crop (Menu-Crop (Rect-For $dockHwnd)) "menu-open.png" | Out-Null
  Mouse-Click 5 5 "left"

  Wait-Process -Id $shellProc.Id -Timeout 35 -ErrorAction SilentlyContinue
} finally {
  foreach ($procInfo in (Process-ByCommandLine "drop-target.cmd")) {
    Stop-Process -Id $procInfo.ProcessId -Force -ErrorAction SilentlyContinue
  }
  if ($unpinnedProc) {
    Stop-Process -Id $unpinnedProc.Id -Force -ErrorAction SilentlyContinue
  }
  foreach ($np in (Get-Process notepad -ErrorAction SilentlyContinue | Where-Object { $beforeNotepadIds -notcontains $_.Id })) {
    Stop-Process -Id $np.Id -Force -ErrorAction SilentlyContinue
  }
  if ($shellProc -and -not $shellProc.HasExited) {
    Stop-Process -Id $shellProc.Id -Force -ErrorAction SilentlyContinue
    Wait-Process -Id $shellProc.Id -Timeout 5 -ErrorAction SilentlyContinue
  }
}

$cleanup = [pscustomobject]@{
  shellProcessAliveAfterExit = if ($shellProc) { -not $shellProc.HasExited } else { $false }
  dropProcessAliveAfterCleanup = @((Process-ByCommandLine "drop-target.cmd")).Count
  unpinnedProcessAliveAfterCleanup = if ($unpinnedProc) { @(Get-Process -Id $unpinnedProc.Id -ErrorAction SilentlyContinue).Count } else { 0 }
  newNotepadAliveAfterCleanup = @((Get-Process notepad -ErrorAction SilentlyContinue | Where-Object { $beforeNotepadIds -notcontains $_.Id })).Count
}
if ($cleanup.shellProcessAliveAfterExit -or $cleanup.dropProcessAliveAfterCleanup -ne 0 -or $cleanup.unpinnedProcessAliveAfterCleanup -ne 0 -or $cleanup.newNotepadAliveAfterCleanup -ne 0) {
  throw "cleanup failed: $($cleanup | ConvertTo-Json -Compress)"
}

$artifactFiles = Get-ChildItem -LiteralPath $PackDir -File | Where-Object { $_.Name -ne "manifest.json" }
$manifestObject = [pscustomobject]@{
  runId = $RunId
  commit = (git rev-parse HEAD)
  invocation = "powershell -NoProfile -ExecutionPolicy Bypass -File .omo\evidence\task-5-dock\scripted-qa.ps1"
  binary = (Find-ShellApp)
  scenarios = @(
    "running unpinned app appears from Win32 discovery",
    "native menu Pin changes unpinned item to pinned",
    "WM_DROPFILES path with spaces pins and launches copied executable",
    "GetForegroundWindow verifies focus",
    "IsIconic verifies minimize",
    "drag reorder persists in DOCK_STATE order",
    "hidden HWND strip and revealed HWND height are measured"
  )
  observations = $observations
  cleanup = $cleanup
  artifacts = @($artifactFiles | ForEach-Object { Artifact-Record $_.FullName })
}
$manifestObject | ConvertTo-Json -Depth 8 | Set-Content -Path $Manifest -Encoding UTF8
Get-Content $Manifest
