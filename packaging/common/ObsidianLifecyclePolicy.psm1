Set-StrictMode -Version Latest

function Get-ObsidianMsixLifecyclePlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet('InstallOrUpdate', 'Uninstall')]
        [string]$Action,

        [Parameter(Mandatory)]
        [bool]$IsInstalled
    )

    if ($Action -eq 'InstallOrUpdate') {
        if ($IsInstalled) {
            'RecoverUpdate'
        }
        'InstallOrUpdate'
        return
    }

    if ($IsInstalled) {
        'RecoverUninstall'
        'Uninstall'
    } else {
        'NoOp'
    }
}

Export-ModuleMember -Function Get-ObsidianMsixLifecyclePlan
