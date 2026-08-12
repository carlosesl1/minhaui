Set-StrictMode -Version Latest

$script:AllowedWorkspaceDependencies = @{
    "shell-core" = @()
    "shell-config" = @("shell-core")
    "shell-diagnostics" = @()
    "shell-renderer" = @("shell-core")
    "shell-platform-windows" = @(
        "shell-config", "shell-core", "shell-diagnostics", "shell-renderer"
    )
    "shell-app" = @("shell-platform-windows")
    "shell-watchdog" = @("shell-core", "shell-diagnostics")
}

function Test-WorkspaceDependencyPolicy {
    [CmdletBinding()]
    param([Parameter(Mandatory)] $Metadata)

    $workspaceIds = @{}
    foreach ($member in @($Metadata.workspace_members)) {
        $workspaceIds[[string]$member] = $true
    }

    $workspaceNames = @{}
    foreach ($package in @($Metadata.packages)) {
        if ($workspaceIds.ContainsKey([string]$package.id)) {
            $workspaceNames[[string]$package.name] = $true
        }
    }

    $violations = New-Object System.Collections.Generic.List[string]
    foreach ($package in @($Metadata.packages)) {
        $packageName = [string]$package.name
        if (-not $workspaceNames.ContainsKey($packageName)) {
            continue
        }
        if (-not $script:AllowedWorkspaceDependencies.ContainsKey($packageName)) {
            $violations.Add("workspace crate has no architecture rule: $packageName")
            continue
        }

        $allowed = @($script:AllowedWorkspaceDependencies[$packageName])
        foreach ($dependency in @($package.dependencies)) {
            $dependencyName = [string]$dependency.name
            if ($workspaceNames.ContainsKey($dependencyName) -and $dependencyName -notin $allowed) {
                $violations.Add("forbidden workspace dependency: $packageName -> $dependencyName")
            }
        }
    }

    @($violations | Sort-Object -Unique)
}

function Test-ArchitectureExceptionPolicy {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [AllowEmptyCollection()] [object[]] $Exceptions,
        [Parameter(Mandatory)] [AllowEmptyCollection()] [object[]] $Allows,
        [Parameter(Mandatory)] [datetime] $Today
    )

    $violations = New-Object System.Collections.Generic.List[string]
    $registered = @{}
    $ids = @{}
    $requiredFields = @(
        "id", "file", "rule", "symbol", "owner", "reason", "risk",
        "compensation", "expires", "removal_plan"
    )

    foreach ($exception in @($Exceptions)) {
        $id = [string]$exception.id
        if ($ids.ContainsKey($id)) {
            $violations.Add("duplicate architecture exception id: $id")
        }
        $ids[$id] = $true

        foreach ($field in $requiredFields) {
            $property = $exception.PSObject.Properties[$field]
            if ($null -eq $property -or [string]::IsNullOrWhiteSpace([string]$property.Value)) {
                $violations.Add("architecture exception $id is missing field: $field")
            }
        }

        $key = "{0}|{1}|{2}" -f $exception.file, $exception.rule, $exception.symbol
        $registered[$key] = $exception

        $expires = [datetime]::MinValue
        $parsed = [datetime]::TryParseExact(
            [string]$exception.expires,
            "yyyy-MM-dd",
            [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::None,
            [ref]$expires
        )
        if (-not $parsed) {
            $violations.Add("architecture exception $id has invalid expiration: $($exception.expires)")
        }
        elseif ($Today.Date -gt $expires.Date) {
            $violations.Add("expired architecture exception: $id expired $($exception.expires)")
        }
    }

    $observed = @{}
    foreach ($allow in @($Allows)) {
        $key = "{0}|{1}|{2}" -f $allow.file, $allow.rule, $allow.symbol
        $observed[$key] = $true
        if (-not $registered.ContainsKey($key)) {
            $violations.Add(
                "unregistered architecture allow: $($allow.file) $($allow.rule) $($allow.symbol)"
            )
        }
    }

    foreach ($key in $registered.Keys) {
        if (-not $observed.ContainsKey($key)) {
            $exception = $registered[$key]
            $violations.Add("stale architecture exception: $($exception.id) has no matching allow")
        }
    }

    @($violations | Sort-Object -Unique)
}

function Get-ArchitectureAllows {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [string] $RepositoryRoot)

    $cratesRoot = Join-Path $RepositoryRoot "crates"
    $allows = New-Object System.Collections.Generic.List[object]
    foreach ($file in Get-ChildItem -LiteralPath $cratesRoot -Recurse -File -Filter "*.rs") {
        $lines = @(Get-Content -LiteralPath $file.FullName)
        for ($index = 0; $index -lt $lines.Count; $index++) {
            $match = [regex]::Match(
                $lines[$index],
                '#\s*\[\s*allow\((clippy::[A-Za-z0-9_]+)\)\s*\]'
            )
            if (-not $match.Success) {
                continue
            }

            $symbol = $null
            $lastCandidate = [Math]::Min($index + 8, $lines.Count - 1)
            for ($candidate = $index + 1; $candidate -le $lastCandidate; $candidate++) {
                $functionMatch = [regex]::Match($lines[$candidate], '\bfn\s+([A-Za-z0-9_]+)')
                if ($functionMatch.Success) {
                    $symbol = $functionMatch.Groups[1].Value
                    break
                }
            }
            if ($null -eq $symbol) {
                $symbol = "<unknown>"
            }

            $relative = $file.FullName.Substring($RepositoryRoot.Length).TrimStart('\', '/')
            $allows.Add([pscustomobject]@{
                file = $relative.Replace('\', '/')
                rule = $match.Groups[1].Value
                symbol = $symbol
            })
        }
    }

    foreach ($allow in $allows) {
        $allow
    }
}

function Test-NativeSurfaceOwnershipPolicy {
    [CmdletBinding(DefaultParameterSetName = "Repository")]
    param(
        [Parameter(Mandatory, ParameterSetName = "Repository")]
        [string] $RepositoryRoot,
        [Parameter(Mandatory, ParameterSetName = "Sources")]
        [AllowEmptyCollection()]
        [object[]] $Sources
    )

    if ($PSCmdlet.ParameterSetName -eq "Repository") {
        $sourceRoot = Join-Path $RepositoryRoot "crates/shell-platform-windows/src"
        $Sources = @(
            foreach ($file in Get-ChildItem -LiteralPath $sourceRoot -File -Filter "*.rs") {
                $relative = $file.FullName.Substring($RepositoryRoot.Length).TrimStart('\', '/')
                [pscustomobject]@{
                    file = $relative.Replace('\', '/')
                    content = Get-Content -Raw -LiteralPath $file.FullName
                }
            }
        )
    }

    $violations = New-Object System.Collections.Generic.List[string]
    $structPattern = [regex]::new(
        '\bstruct\s+(?<name>[A-Za-z_][A-Za-z0-9_]*)[^\{;]*\{(?<body>.*?)^\s*\}',
        [Text.RegularExpressions.RegexOptions]::Multiline -bor `
            [Text.RegularExpressions.RegexOptions]::Singleline
    )
    $fieldPattern = [regex]::new(
        '^\s*(?:pub(?:\([^\)]*\))?\s+)?(?<name>[A-Za-z_][A-Za-z0-9_]*)\s*:\s*(?<type>[^,\r\n]+)',
        [Text.RegularExpressions.RegexOptions]::Multiline
    )
    $helperPattern = [regex]::new(
        '\bfn\s+(?<name>[A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>]*>)?\s*\((?<params>[^\)]*)\)',
        [Text.RegularExpressions.RegexOptions]::Singleline
    )

    foreach ($source in @($Sources)) {
        $file = ([string]$source.file).Replace('\', '/')
        $content = [string]$source.content
        $isRuntimeModule = $file.EndsWith("/win32_surface_runtime.rs")

        if (-not $isRuntimeModule) {
            foreach ($structMatch in $structPattern.Matches($content)) {
                foreach ($fieldMatch in $fieldPattern.Matches($structMatch.Groups["body"].Value)) {
                    $fieldType = $fieldMatch.Groups["type"].Value
                    $nativeType = [regex]::Match(
                        $fieldType,
                        '\b(CompositionRenderer|WindowSurface)\b'
                    )
                    if ($nativeType.Success) {
                        $violations.Add(
                            "native surface ownership outside win32_surface_runtime.rs: " +
                            "$file $($structMatch.Groups['name'].Value)." +
                            "$($fieldMatch.Groups['name'].Value): $($nativeType.Groups[1].Value)"
                        )
                    }
                }
            }
        }

        foreach ($helperMatch in $helperPattern.Matches($content)) {
            $tailStart = $helperMatch.Index + $helperMatch.Length
            $tail = $content.Substring($tailStart)
            $nextFunction = [regex]::Match(
                $tail,
                '(?m)^\s*(?:pub(?:\([^\)]*\))?\s+)?fn\s+[A-Za-z_]'
            )
            $functionBody = if ($nextFunction.Success) {
                $tail.Substring(0, $nextFunction.Index)
            }
            else {
                $tail
            }
            if (
                $helperMatch.Groups["params"].Value -match '&\s*(?:''[A-Za-z_][A-Za-z0-9_]*\s+)?mut\s+RuntimeSurfaces\b' -and
                $functionBody -match '\brebuild_native_surfaces\s*\('
            ) {
                $violations.Add(
                    "presentation helper depends on mutable RuntimeSurfaces: " +
                    "$file $($helperMatch.Groups['name'].Value)"
                )
            }
        }
    }

    @($violations | Sort-Object -Unique)
}

$script:AllowedPublicFacadeItems = @{
    "crates/shell-diagnostics/src/lib.rs" = @(
        "DiagnosticPolicy",
        "RetentionPolicy",
        "STANDARD_DIAGNOSTIC_POLICY"
    )
    "crates/shell-platform-windows/src/lib.rs" = @(
        "ShowcaseRunConfig",
        "activate_existing_instance",
        "crate_identity",
        "current_session_id",
        "local_app_data_path",
        "run_showcase"
    )
    "crates/shell-renderer/src/lib.rs" = @(
        "ContextMenuEntry",
        "ContextMenuLaidOutRow",
        "ContextMenuLayout",
        "ContextMenuScene",
        "ContextMenuSeparator",
        "DipPoint",
        "DipRect",
        "DockAlignment",
        "DockIcon",
        "DockItemVisual",
        "DockItemVisualKind",
        "DockLaidOutItem",
        "DockLayout",
        "DockLayoutConfig",
        "DockScene",
        "Dpi",
        "PhysicalRect",
        "PopoverContentState",
        "PopoverLaidOutRow",
        "PopoverLayout",
        "PopoverLayoutStyle",
        "PopoverNotch",
        "PopoverPlacement",
        "PopoverRow",
        "PopoverScene",
        "PopoverSeparator",
        "PopoverSurfaceSize",
        "PreviewCardLayout",
        "PreviewCardVisual",
        "PreviewPanelSize",
        "PreviewPlacementInput",
        "PreviewSourceSize",
        "PreviewUnavailableReason",
        "QUICK_SETTINGS_BODY_TOP",
        "QUICK_SETTINGS_WIDTH",
        "QuickControlSettingsRow",
        "QuickSettingsAudioOutput",
        "QuickSettingsAudioOutputId",
        "QuickSettingsAudioPanel",
        "QuickSettingsAudioSession",
        "QuickSettingsAudioSessionId",
        "QuickSettingsChoice",
        "QuickSettingsChoiceId",
        "QuickSettingsDisplay",
        "QuickSettingsEnergy",
        "QuickSettingsFocus",
        "QuickSettingsHit",
        "QuickSettingsLaidOutAudioOutput",
        "QuickSettingsLaidOutAudioSession",
        "QuickSettingsLaidOutChoice",
        "QuickSettingsLaidOutMedia",
        "QuickSettingsLaidOutMediaChoice",
        "QuickSettingsLaidOutSlider",
        "QuickSettingsLaidOutTile",
        "QuickSettingsLayout",
        "QuickSettingsMediaAction",
        "QuickSettingsMediaArtwork",
        "QuickSettingsMediaChoice",
        "QuickSettingsMediaPlayer",
        "QuickSettingsMediaSessionId",
        "QuickSettingsPlaybackState",
        "QuickSettingsScene",
        "QuickSettingsSlider",
        "QuickSettingsSound",
        "QuickSettingsSubmenu",
        "QuickSettingsTile",
        "RunningIndicator",
        "SETTINGS_CONTENT_MAX_WIDTH",
        "SETTINGS_RAIL_WIDTH",
        "SETTINGS_SPLIT_BREAKPOINT",
        "SettingsAccessibilityControlType",
        "SettingsAccessibilityError",
        "SettingsAccessibilityNode",
        "SettingsAccessibilityNodeId",
        "SettingsAccessibilityPattern",
        "SettingsAccessibilitySnapshot",
        "SettingsControl",
        "SettingsControlId",
        "SettingsControlKind",
        "SettingsFocus",
        "SettingsHit",
        "SettingsLaidOutControl",
        "SettingsLaidOutNavigation",
        "SettingsLayout",
        "SettingsLayoutMode",
        "SettingsNavigationItem",
        "SettingsRow",
        "SettingsScene",
        "SettingsSectionId",
        "ShellMetrics",
        "TopbarDensity",
        "TopbarLaidOutItem",
        "TopbarLayout",
        "TopbarModuleStatus",
        "TopbarModuleVisual",
        "TopbarOverflow",
        "TopbarScene",
        "VisualPreferences",
        "WINDOW_PREVIEW_THUMBNAIL_RADIUS",
        "WindowPreviewCapture",
        "WindowPreviewPanelLayout",
        "WindowPreviewScene",
        "WindowPreviewVisual",
        "context_menu_anchor_rect",
        "context_menu_height_for_entries",
        "context_menu_surface_width",
        "crate_identity",
        "dock_scene_max_width",
        "dock_showcase_rect",
        "layout_context_menu_scene",
        "layout_dock_scene",
        "layout_popover_scene",
        "layout_quick_settings",
        "layout_settings_scene",
        "layout_topbar_scene",
        "layout_window_preview",
        "native",
        "physical_from_dip",
        "place_window_preview",
        "popover_anchor_rect",
        "popover_anchor_rect_with_height",
        "popover_height_for_rows",
        "popover_placement",
        "popover_surface_size",
        "preview_panel_size_for_scene",
        "preview_panel_size",
        "quick_settings_surface_size",
        "rounded_content_hit",
        "settings_accessibility_snapshot",
        "topbar_height_for_text_scale",
        "topbar_rect"
    )
}

function Test-PublicFacadePolicy {
    [CmdletBinding(DefaultParameterSetName = "Repository")]
    param(
        [Parameter(Mandatory, ParameterSetName = "Repository")]
        [string] $RepositoryRoot,
        [Parameter(Mandatory, ParameterSetName = "Sources")]
        [AllowEmptyCollection()]
        [object[]] $Sources
    )

    if ($PSCmdlet.ParameterSetName -eq "Repository") {
        $Sources = @(
            foreach ($relative in $script:AllowedPublicFacadeItems.Keys) {
                $path = Join-Path $RepositoryRoot $relative
                [pscustomobject]@{
                    file = $relative
                    content = Get-Content -Raw -LiteralPath $path
                }
            }
        )
    }

    $violations = New-Object System.Collections.Generic.List[string]
    foreach ($source in @($Sources)) {
        $file = ([string]$source.file).Replace('\', '/')
        if (-not $script:AllowedPublicFacadeItems.ContainsKey($file)) {
            continue
        }
        $allowed = @($script:AllowedPublicFacadeItems[$file])
        $content = [string]$source.content
        $observed = New-Object System.Collections.Generic.List[string]

        foreach ($match in [regex]::Matches($content, '(?ms)^pub use\s+.*?;')) {
            $statement = $match.Value
            if ($statement.Contains('{')) {
                $inner = $statement.Substring($statement.IndexOf('{') + 1)
                $inner = $inner.Substring(0, $inner.LastIndexOf('}'))
                foreach ($part in $inner.Split(',')) {
                    $name = $part.Trim()
                    if (-not [string]::IsNullOrWhiteSpace($name)) {
                        $observed.Add(($name -split '\s+as\s+')[-1])
                    }
                }
            }
            else {
                $path = ($statement -replace '^pub use\s+', '' -replace ';$', '').Trim()
                $observed.Add($path.Split('::')[-1])
            }
        }

        foreach ($match in [regex]::Matches(
            $content,
            '(?m)^pub\s+(?:(?:const\s+)?fn|struct|enum|trait|type|mod|const)\s+(?<name>[A-Za-z_][A-Za-z0-9_]*)'
        )) {
            $observed.Add($match.Groups['name'].Value)
        }

        foreach ($name in @($observed | Sort-Object -Unique)) {
            if ($name -notin $allowed) {
                $violations.Add("unapproved public facade item: $file $name")
            }
        }
    }

    @($violations | Sort-Object -Unique)
}

function Test-DiagnosticsPolicyOwnership {
    [CmdletBinding(DefaultParameterSetName = "Repository")]
    param(
        [Parameter(Mandatory, ParameterSetName = "Repository")]
        [string] $RepositoryRoot,
        [Parameter(Mandatory, ParameterSetName = "Sources")]
        [AllowEmptyCollection()]
        [object[]] $Sources
    )

    $owner = "crates/shell-diagnostics/src/lib.rs"
    $adapters = @(
        "crates/shell-platform-windows/src/diagnostics.rs",
        "crates/shell-watchdog/src/diagnostics.rs"
    )
    if ($PSCmdlet.ParameterSetName -eq "Repository") {
        $Sources = @(
            foreach ($relative in @($owner) + $adapters) {
                [pscustomobject]@{
                    file = $relative
                    content = Get-Content -Raw -LiteralPath (Join-Path $RepositoryRoot $relative)
                }
            }
        )
    }

    $violations = New-Object System.Collections.Generic.List[string]
    foreach ($source in @($Sources)) {
        $file = ([string]$source.file).Replace('\', '/')
        $content = [string]$source.content
        if ($file -eq $owner) {
            continue
        }

        foreach ($match in [regex]::Matches(
            $content,
            '(?m)^\s*(?:pub(?:\(crate\))?\s+)?fn\s+(?<name>redact(?:_value)?|is_sensitive_key|contains_ascii_case_insensitive)\b'
        )) {
            $violations.Add(
                "diagnostics privacy policy outside shared owner: $file $($match.Groups['name'].Value)"
            )
        }
        foreach ($match in [regex]::Matches(
            $content,
            '(?m)^\s*(?:pub(?:\(crate\))?\s+)?struct\s+(?<name>RotationPolicy|RetentionPolicy)\b'
        )) {
            $violations.Add(
                "diagnostics retention policy outside shared owner: $file $($match.Groups['name'].Value)"
            )
        }
        if ($file -in $adapters -and $content -notmatch 'shell_diagnostics') {
            $violations.Add("diagnostics adapter bypasses shared policy: $file")
        }
    }

    @($violations | Sort-Object -Unique)
}

function Test-WatchdogNativeRecoveryBoundary {
    [CmdletBinding(DefaultParameterSetName = "Repository")]
    param(
        [Parameter(Mandatory, ParameterSetName = "Repository")]
        [string] $RepositoryRoot,
        [Parameter(Mandatory, ParameterSetName = "Sources")]
        [AllowEmptyCollection()]
        [object[]] $Sources
    )

    $watchdogRoot = "crates/shell-watchdog/src"
    $lib = "$watchdogRoot/lib.rs"
    $safePlanner = "$watchdogRoot/taskbar_restore.rs"
    $nativeAdapter = "$watchdogRoot/taskbar_restore_win32.rs"
    if ($PSCmdlet.ParameterSetName -eq "Repository") {
        $sourceRoot = Join-Path $RepositoryRoot $watchdogRoot
        $Sources = @(
            foreach ($file in Get-ChildItem -LiteralPath $sourceRoot -File -Filter "*.rs") {
                $relative = $file.FullName.Substring($RepositoryRoot.Length).TrimStart('\', '/')
                [pscustomobject]@{
                    file = $relative.Replace('\', '/')
                    content = Get-Content -Raw -LiteralPath $file.FullName
                }
            }
        )
    }

    $violations = New-Object System.Collections.Generic.List[string]
    $unsafeAllows = New-Object System.Collections.Generic.List[string]
    $sourceByFile = @{}
    foreach ($source in @($Sources)) {
        $file = ([string]$source.file).Replace('\', '/')
        $content = [string]$source.content
        $sourceByFile[$file] = $content
        foreach ($match in [regex]::Matches(
            $content,
            '(?s)#\s*\[\s*allow\s*\(\s*unsafe_code\b.*?\)\s*\]'
        )) {
            $unsafeAllows.Add($file)
        }
        if (
            $file -ne $nativeAdapter -and
            $content -match '\bunsafe\s*(?:\{|extern\b|fn\b|impl\b|trait\b)'
        ) {
            $violations.Add("watchdog unsafe operation outside native recovery adapter: $file")
        }
    }

    if (-not $sourceByFile.ContainsKey($nativeAdapter)) {
        $violations.Add("watchdog native recovery adapter is missing: $nativeAdapter")
    }
    if ($unsafeAllows.Count -ne 1 -or $unsafeAllows[0] -ne $lib) {
        $violations.Add(
            "watchdog must have exactly one unsafe_code allow on its native recovery module"
        )
    }
    if (
        -not $sourceByFile.ContainsKey($lib) -or
        $sourceByFile[$lib] -notmatch '(?s)#\s*\[\s*cfg\s*\(\s*windows\s*\)\s*\]\s*#\s*\[\s*allow\s*\(\s*unsafe_code\b.*?\)\s*\]\s*mod\s+taskbar_restore_win32\s*;'
    ) {
        $violations.Add(
            "watchdog native recovery adapter must be cfg(windows) with the sole unsafe allow"
        )
    }
    if (
        $sourceByFile.ContainsKey($safePlanner) -and
        $sourceByFile[$safePlanner] -match '\bwindows\s*::'
    ) {
        $violations.Add("watchdog recovery planner must not import the Windows API")
    }

    @($violations | Sort-Object -Unique)
}

Export-ModuleMember -Function @(
    "Get-ArchitectureAllows",
    "Test-ArchitectureExceptionPolicy",
    "Test-DiagnosticsPolicyOwnership",
    "Test-NativeSurfaceOwnershipPolicy",
    "Test-PublicFacadePolicy",
    "Test-WatchdogNativeRecoveryBoundary",
    "Test-WorkspaceDependencyPolicy"
)
