# Support and recovery

## Start normally

Run `shell-app.exe`. No command-line option is required. If graphics initialization fails, use `shell-app.exe --safe-mode --reduced-motion`.

## Restore Windows state

The app never changes taskbar behavior without explicit consent. The separate `shell-watchdog.exe` owns recovery state. Update and uninstall flows call `Invoke-ObsidianRecovery.ps1` before replacing or removing files.

## Uninstall settings choice

Run `Remove-ObsidianGlass.ps1 -SettingsAction Keep`, `Export`, or `Remove`. Export also requires `-ExportPath`. The script restricts removal to the app directory under the current user's LocalAppData.

## Reporting a problem

Include Windows version, display scale, GPU model, reproduction steps, and the redacted diagnostic export. Do not include screenshots containing private notifications unless necessary.

## Development diagnostics

Diagnostics are disabled by default. Set `MINHA_UI_LOG_MODULES` to a comma-separated list of `app.lifecycle`, `dock.launch`, or `dock.performance` before starting `shell-app.exe`; use `all` to enable all three. The performance module emits only dock render operations that exceed 16 ms. The local, redacted log is stored at `%LOCALAPPDATA%\Minha UI\logs\shell-app.log` and is capped at 4 MiB.

For an A/B test of the experimental lightweight dock material, compare the
normal launch with `shell-app.exe --liquid-glass` while
`MINHA_UI_LOG_MODULES=dock.performance` is enabled. The flag is not persisted,
does not enable desktop capture, and is automatically suppressed by safe mode
and high contrast.

## Background apps menu

The top-bar **Apps** menu is read-only and refreshes when opened. It shows active
applications that still have a matching Explorer notification registration.
Opening a row focuses or launches the application instead of invoking private
tray-icon menus. An unavailable message means the current Windows version did
not expose the expected Explorer registration data; it does not require reset
or recovery.
