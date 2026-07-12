# Support and recovery

## Start normally

Run `shell-app.exe`. No command-line option is required. If graphics initialization fails, use `shell-app.exe --safe-mode --reduced-motion`.

## Restore Windows state

The app never changes taskbar behavior without explicit consent. The separate `shell-watchdog.exe` owns recovery state. Update and uninstall flows call `Invoke-ObsidianRecovery.ps1` before replacing or removing files.

## Uninstall settings choice

Run `Remove-ObsidianGlass.ps1 -SettingsAction Keep`, `Export`, or `Remove`. Export also requires `-ExportPath`. The script restricts removal to the app directory under the current user's LocalAppData.

## Reporting a problem

Include Windows version, display scale, GPU model, reproduction steps, and the redacted diagnostic export. Do not include screenshots containing private notifications unless necessary.

