[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$ShellAppPath,

    [string]$EvidencePath = "artifacts/native-windows-validation/summary.json",

    [ValidateRange(15000, 60000)]
    [int]$QaExitMilliseconds = 45000
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "Native Windows validation can run only on Windows."
}
if (-not [Environment]::UserInteractive) {
    throw "Native Windows validation requires an interactive desktop session."
}
if ((Get-Process -Id $PID).SessionId -eq 0) {
    throw "Native Windows validation refuses Session 0 because UI Automation would be invalid."
}

$shellApp = [System.IO.Path]::GetFullPath($ShellAppPath)
if (-not (Test-Path -LiteralPath $shellApp -PathType Leaf)) {
    throw "shell-app executable was not found at $shellApp"
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class MinhaUiNativeValidation {
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr FindWindow(string className, string windowName);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool IsWindowVisible(IntPtr window);
}
"@

$windowClass = "MinhaUi.NativeShell.Window.v1"
$settingsTitle = "Minha UI Settings"
$existing = [MinhaUiNativeValidation]::FindWindow($windowClass, $settingsTitle)
if ($existing -ne [IntPtr]::Zero) {
    throw "A Minha UI Settings window already exists; the runner image is not isolated."
}

function Wait-NativeSettingsWindow {
    param(
        [Parameter(Mandatory)]
        [bool]$Visible,

        [int]$TimeoutMilliseconds = 15000
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $window = [MinhaUiNativeValidation]::FindWindow($windowClass, $settingsTitle)
        if ($window -ne [IntPtr]::Zero) {
            if (-not $Visible -or [MinhaUiNativeValidation]::IsWindowVisible($window)) {
                return $window
            }
        }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)

    $state = if ($Visible) { "visible" } else { "created" }
    throw "Settings window did not become $state within $TimeoutMilliseconds ms."
}

function Wait-AutomationElement {
    param(
        [Parameter(Mandatory)]
        [System.Windows.Automation.AutomationElement]$Root,

        [Parameter(Mandatory)]
        [string]$Name,

        [int]$TimeoutMilliseconds = 10000
    )

    $condition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $Name
    )
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $element = $Root.FindFirst(
            [System.Windows.Automation.TreeScope]::Descendants,
            $condition
        )
        if ($null -ne $element) {
            return $element
        }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)

    throw "UI Automation element '$Name' was not published within $TimeoutMilliseconds ms."
}

function Wait-RangeValue {
    param(
        [Parameter(Mandatory)]
        [System.Windows.Automation.RangeValuePattern]$Pattern,

        [Parameter(Mandatory)]
        [double]$Reference,

        [Parameter(Mandatory)]
        [ValidateSet("Equal", "Different")]
        [string]$Comparison,

        [int]$TimeoutMilliseconds = 10000
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $value = $Pattern.Current.Value
        $equal = [Math]::Abs($value - $Reference) -le 0.01
        if (($Comparison -eq "Equal" -and $equal) -or
            ($Comparison -eq "Different" -and -not $equal)) {
            return $value
        }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)

    throw "RangeValue did not become $Comparison relative to $Reference within $TimeoutMilliseconds ms."
}

function Stop-TestProcess {
    param([System.Diagnostics.Process]$Process)

    if ($null -eq $Process) {
        return
    }
    try {
        if (-not $Process.HasExited) {
            $Process.Kill()
        }
        $null = $Process.WaitForExit(10000)
    }
    catch {
        Write-Warning "Could not cleanly stop test process $($Process.Id): $($_.Exception.Message)"
    }
    finally {
        $Process.Dispose()
    }
}

$primary = $null
$secondary = $null
try {
    $primaryArguments = @(
        "--showcase",
        "--qa-exit-ms", $QaExitMilliseconds,
        "--force-warp",
        "--safe-mode",
        "--reduced-motion",
        "--simulate-device-loss-once",
        "--simulate-lifecycle-events"
    )
    $primary = Start-Process `
        -FilePath $shellApp `
        -ArgumentList $primaryArguments `
        -PassThru

    $null = Wait-NativeSettingsWindow -Visible $false
    if ($primary.HasExited) {
        throw "Primary shell exited before instance activation with code $($primary.ExitCode)."
    }

    $secondary = Start-Process `
        -FilePath $shellApp `
        -ArgumentList @("--showcase", "--safe-mode") `
        -PassThru
    if (-not $secondary.WaitForExit(15000)) {
        throw "Secondary shell did not forward activation and exit within 15 seconds."
    }
    if ($secondary.ExitCode -ne 0) {
        throw "Secondary shell activation failed with code $($secondary.ExitCode)."
    }

    $settingsWindow = Wait-NativeSettingsWindow -Visible $true
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($settingsWindow)
    if ($null -eq $root) {
        throw "UI Automation did not return a root for the Settings HWND."
    }
    if ($root.Current.Name -ne "Settings") {
        throw "Unexpected Settings UIA root name '$($root.Current.Name)'."
    }
    if ($root.Current.ControlType -ne [System.Windows.Automation.ControlType]::Window) {
        throw "Settings UIA root did not expose the Window control type."
    }
    if ($root.Current.IsOffscreen) {
        throw "Settings UIA root is marked offscreen after activation."
    }
    $bounds = $root.Current.BoundingRectangle
    if ($bounds.Width -le 0 -or $bounds.Height -le 0) {
        throw "Settings UIA root exposed empty screen bounds."
    }

    $sections = Wait-AutomationElement -Root $root -Name "Settings sections"
    if ($sections.Current.ControlType -ne [System.Windows.Automation.ControlType]::List) {
        throw "Settings sections did not expose the List control type."
    }

    $dock = Wait-AutomationElement -Root $root -Name "Dock"
    $invoke = [System.Windows.Automation.InvokePattern]$dock.GetCurrentPattern(
        [System.Windows.Automation.InvokePattern]::Pattern
    )
    $invoke.Invoke()

    $iconSize = Wait-AutomationElement -Root $root -Name "Icon size"
    if ($iconSize.Current.ControlType -ne [System.Windows.Automation.ControlType]::Slider) {
        throw "Icon size did not expose the Slider control type."
    }
    $range = [System.Windows.Automation.RangeValuePattern]$iconSize.GetCurrentPattern(
        [System.Windows.Automation.RangeValuePattern]::Pattern
    )
    $originalValue = $range.Current.Value
    $targetValue = if ($originalValue + $range.Current.LargeChange -le $range.Current.Maximum) {
        $originalValue + $range.Current.LargeChange
    }
    else {
        $originalValue - $range.Current.LargeChange
    }
    $range.SetValue($targetValue)
    $changedValue = Wait-RangeValue `
        -Pattern $range `
        -Reference $originalValue `
        -Comparison Different
    $range.SetValue($originalValue)
    $restoredValue = Wait-RangeValue `
        -Pattern $range `
        -Reference $originalValue `
        -Comparison Equal
    if ($primary.HasExited) {
        throw "Primary shell exited during UI Automation validation with code $($primary.ExitCode)."
    }

    $evidence = [ordered]@{
        schema_version = 1
        validated_at_utc = [DateTime]::UtcNow.ToString("o")
        commit = [Environment]::GetEnvironmentVariable("GITHUB_SHA")
        workflow_run_id = [Environment]::GetEnvironmentVariable("GITHUB_RUN_ID")
        windows_version = [Environment]::OSVersion.VersionString
        powershell_version = $PSVersionTable.PSVersion.ToString()
        session_id = (Get-Process -Id $PID).SessionId
        shell_app = $shellApp
        primary_process_id = $primary.Id
        activation_forwarded = $true
        lifecycle_simulated = $true
        device_loss_simulated = $true
        warp = $true
        safe_mode = $true
        settings_uia = [ordered]@{
            root_name = $root.Current.Name
            root_control_type = $root.Current.ControlType.ProgrammaticName
            bounds_width = $bounds.Width
            bounds_height = $bounds.Height
            navigation_list = $true
            invoke_pattern = $true
            range_value_pattern = $true
            original_range_value = $originalValue
            changed_range_value = $changedValue
            restored_range_value = $restoredValue
        }
    }
    $fullEvidencePath = [System.IO.Path]::GetFullPath($EvidencePath)
    $evidenceDirectory = Split-Path -Parent $fullEvidencePath
    New-Item -ItemType Directory -Path $evidenceDirectory -Force | Out-Null
    $evidence | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $fullEvidencePath -Encoding UTF8
    Write-Output "Native Windows validation passed; evidence: $fullEvidencePath"
}
finally {
    Stop-TestProcess -Process $secondary
    Stop-TestProcess -Process $primary
}
