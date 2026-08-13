# Packaging lifecycle contracts

Packaging must never imply that copying a recovery script into a layout makes
the operating system or store execute it. This matrix is the release boundary:

| Distribution path | Lifecycle actually provided | Repository wiring | Limit |
| --- | --- | --- | --- |
| Steam update | InstallScript before first launch of a mounted build | Versioned `Run Process`, current-user PowerShell, synchronous watchdog wrapper | It is not a pre-download/pre-file-replacement hook; Steam does not document that a nonzero prerequisite cancels application launch. |
| Steam uninstall | `Run Process On Uninstall` | Current-user synchronous watchdog wrapper | Steam documents invocation, not rollback of depot removal after a nonzero child status. |
| Managed MSIX sideload | The companion script owns `Add-AppxPackage` and `Remove-AppxPackage` | Recovery is the first step for an existing package; a failure stops the script before deployment mutation. | Users must invoke `Deploy-ObsidianMsix.ps1`; the MSIX manifest cannot register this script. |
| Store, Windows Settings, normal App Installer | No arbitrary pre-update/pre-uninstall process in the supported package lifecycle | None | Supported only while stable builds leave experimental taskbar replacement disabled. |
| Portable test ZIP | No installer lifecycle | Manual `Invoke-ObsidianRecovery.ps1` / `Remove-ObsidianGlass.ps1` | Not a managed release updater. |

The Steam behavior is based on Valve's documented
[InstallScript integration, `Run Process`, and uninstall process](https://partner.steamgames.com/doc/sdk/installscripts?l=english).
The MSIX boundary follows Microsoft's documented
[package update model](https://learn.microsoft.com/windows/msix/app-package-updates)
and [non-Store deployment APIs](https://learn.microsoft.com/windows/msix/non-store-developer-updates).
The absence of an arbitrary manifest lifecycle process is deliberately treated
as an unsupported capability, not filled with a private API or a best-effort
background task.

## Managed MSIX commands

Build and sign the package, then distribute the `.msix`, the three companion
files, and `SHA256SUMS.txt` from the same output directory.

```powershell
.\Deploy-ObsidianMsix.ps1 -Action InstallOrUpdate -PackagePath .\ObsidianGlass-0.1.0.0-x64.msix
.\Deploy-ObsidianMsix.ps1 -Action Uninstall
```

`InstallOrUpdate` is a normal first install when the package is absent. When it
is already installed for the current user, the plan is strictly
`RecoverUpdate -> InstallOrUpdate`. Uninstall is strictly
`RecoverUninstall -> Uninstall`, and is idempotent when the package is absent.

## Verification

The Windows CI runs the policy and executable contract after compiling release
binaries:

```powershell
powershell -NoProfile -File packaging/tests/PackagingLifecycle.Tests.ps1
```

The test validates plan ordering, Steam depot registration, per-version run
markers, current-user context, generated layout contents, the real watchdog
check-only path, and fail-closed behavior when the watchdog is missing.
