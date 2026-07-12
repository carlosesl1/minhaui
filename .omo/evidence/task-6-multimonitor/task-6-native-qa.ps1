param(
    [string]$RepoRoot,
    [ValidateSet("Lifecycle", "Suppressed", "PreviewFallback")]
    [string]$Scenario = "Lifecycle"
)

$ErrorActionPreference = "Stop"
$process = $null
$fullscreen = $null
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NativeQaUser32 {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc proc, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll", SetLastError=true)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll", SetLastError=true)] public static extern int GetClassName(IntPtr hWnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
}
"@

$evidence = Join-Path $RepoRoot ".omo/evidence/task-6-multimonitor"
$prefix = switch ($Scenario) {
    "Lifecycle" { "task-6-qa-lifecycle" }
    "Suppressed" { "task-6-qa-fullscreen8" }
    "PreviewFallback" { "task-6-qa-preview-fallback" }
}
$stdoutPath = Join-Path $evidence "$prefix-stdout-redacted.txt"
$stderrPath = Join-Path $evidence "$prefix-stderr.txt"
$windowPath = Join-Path $evidence "$prefix-windows.txt"
$cleanupPath = Join-Path $evidence "$prefix-cleanup.txt"
$arguments = switch ($Scenario) {
    "Lifecycle" { "--window-smoke --force-warp --simulate-lifecycle-events --qa-exit-ms 6000" }
    default { "--window-smoke --force-warp --qa-exit-ms 3000" }
}
$captureDelayMs = switch ($Scenario) {
    "Lifecycle" { 1800 }
    default { 1100 }
}
$needsFullscreen = $Scenario -in @("Lifecycle", "Suppressed")

try {
    if ($needsFullscreen) {
        $fullscreen = New-Object System.Windows.Forms.Form
        $screen = [System.Windows.Forms.Screen]::PrimaryScreen
        $fullscreen.Text = "Task6SafeFullscreen"
        $fullscreen.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::None
        $fullscreen.StartPosition = [System.Windows.Forms.FormStartPosition]::Manual
        $fullscreen.Bounds = $screen.Bounds
        $fullscreen.TopMost = $true
        $fullscreen.BackColor = [System.Drawing.Color]::Black
        $fullscreen.ShowInTaskbar = $true
        $fullscreen.Show()
        [System.Windows.Forms.Application]::DoEvents()
    }

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = Join-Path $RepoRoot "target/x86_64-pc-windows-msvc/release/shell-app.exe"
    $psi.WorkingDirectory = $RepoRoot
    $psi.Arguments = $arguments
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.Environment["MINHA_UI_QA_TRACE"] = "1"
    if ($Scenario -eq "PreviewFallback") {
        $psi.Environment["MINHA_UI_QA_RESTRICTED_PREVIEW"] = "1"
    }
    $process = [System.Diagnostics.Process]::Start($psi)
    Start-Sleep -Milliseconds $captureDelayMs
    [System.Windows.Forms.Application]::DoEvents()

    $windows = [System.Collections.Generic.List[object]]::new()
    [NativeQaUser32]::EnumWindows({
        param([IntPtr]$hwnd, [IntPtr]$lparam)
        if (-not [NativeQaUser32]::IsWindowVisible($hwnd)) { return $true }
        $title = [System.Text.StringBuilder]::new(256)
        $class = [System.Text.StringBuilder]::new(256)
        [void][NativeQaUser32]::GetWindowText($hwnd, $title, $title.Capacity)
        [void][NativeQaUser32]::GetClassName($hwnd, $class, $class.Capacity)
        if ($title.ToString().StartsWith("Minha UI")) {
            $rect = New-Object NativeQaUser32+RECT
            [void][NativeQaUser32]::GetWindowRect($hwnd, [ref]$rect)
            $windows.Add([pscustomobject]@{
                Hwnd = ("0x{0:x}" -f $hwnd.ToInt64())
                Title = $title.ToString()
                Class = $class.ToString()
                X = $rect.Left
                Y = $rect.Top
                Width = $rect.Right - $rect.Left
                Height = $rect.Bottom - $rect.Top
            })
        }
        return $true
    }, [IntPtr]::Zero) | Out-Null

    $index = 0
    $windowLines = [System.Collections.Generic.List[string]]::new()
    foreach ($window in $windows) {
        $windowLines.Add(("WINDOW hwnd={0} title=""{1}"" class=""{2}"" rect={3},{4},{5},{6}" -f $window.Hwnd, $window.Title, $window.Class, $window.X, $window.Y, $window.Width, $window.Height))
        if ($window.Width -gt 0 -and $window.Height -gt 0) {
            $bitmap = [System.Drawing.Bitmap]::new($window.Width, $window.Height)
            $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
            $graphics.CopyFromScreen($window.X, $window.Y, 0, 0, $bitmap.Size)
            $pngPath = Join-Path $evidence ("$prefix-window-{0}.png" -f $index)
            $bitmap.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
            $graphics.Dispose()
            $bitmap.Dispose()
            $windowLines.Add(("SCREENSHOT {0}" -f (Split-Path $pngPath -Leaf)))
        }
        $index += 1
    }
    if ($windowLines.Count -eq 0) { $windowLines.Add("NO_WINDOWS_CAPTURED") }
    $windowLines | Set-Content -Encoding UTF8 $windowPath

    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    $redacted = ($stdout -split "\r?\n" | ForEach-Object {
        if ($_ -like "DOCK_STATE*") { "DOCK_STATE [redacted]" } else { $_ }
    }) -join [Environment]::NewLine
    if ([string]::IsNullOrWhiteSpace($redacted)) { $redacted = "NO_STDOUT" }
    if ([string]::IsNullOrWhiteSpace($stderr)) { $stderr = "NO_STDERR" }
    $redacted | Set-Content -Encoding UTF8 $stdoutPath
    $stderr | Set-Content -Encoding UTF8 $stderrPath
}
finally {
    if ($fullscreen -ne $null) {
        $fullscreen.Close()
        $fullscreen.Dispose()
    }
    if ($process -ne $null -and -not $process.HasExited) {
        $process.Kill()
        $process.WaitForExit()
    }
    $remaining = Get-Process shell-app -ErrorAction SilentlyContinue
    if ($remaining) {
        $remaining | ForEach-Object { "STILL_RUNNING pid=$($_.Id)" } | Set-Content -Encoding UTF8 $cleanupPath
    } else {
        "NO_SHELL_APP_PROCESS" | Set-Content -Encoding UTF8 $cleanupPath
    }
}
if ($process -eq $null) { exit 1 }
exit $process.ExitCode
