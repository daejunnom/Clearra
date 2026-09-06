param()

# Pure mocked policy regressions. No CIM query, compiler, generated executable,
# runtime fallback, signing, or policy modification is performed by this test.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$previousSurface = $env:CLEARRA_EXECUTION_SURFACE
$script:PolicyQueryCount = 0
$script:PolicyDisposition = 'enforced'
$script:PassedCases = 0

function Test-StartTestsWindows { return $true }
. (Join-Path $PSScriptRoot 'lib/clearra-execution-surface.ps1')
function Get-ClearraApplicationControlStatus {
    $script:PolicyQueryCount++
    return [pscustomobject]@{
        query_status = if ($script:PolicyDisposition -eq 'unknown') { 'failed' } else { 'ok' }
        generated_executable_policy = if ($script:PolicyDisposition -eq 'off') { 'allow' } else { 'deny' }
        user_mode_code_integrity_policy = $script:PolicyDisposition
    }
}
function Assert-PolicyValue([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Assert-PolicyReject([scriptblock]$Body, [string]$Expected) {
    $caught = $null
    try { & $Body } catch { $caught = $_.Exception.Message }
    Assert-PolicyValue ($null -ne $caught -and $caught.Contains($Expected)) "Expected policy rejection '$Expected', got '$caught'"
}
function Invoke-PolicyCase([string]$Name, [scriptblock]$Body) {
    & $Body
    $script:PassedCases++
    Write-Output "execution_surface_case=$Name status=passed"
}

try {
    Invoke-PolicyCase 'trusted_full_enforced_rejects_before_dispatch' {
        $script:PolicyDisposition = 'enforced'
        $script:PolicyQueryCount = 0
        $script:DispatchCount = 0
        Assert-PolicyReject {
            Assert-ClearraRequestedTaskSurfaces @('ReleaseAcceptance') 'Trusted' 'windows'
            $script:DispatchCount++
        } 'E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE'
        Assert-PolicyValue ($script:DispatchCount -eq 0) 'A denied full gate reached dispatch'
        Assert-PolicyValue ($script:PolicyQueryCount -eq 1) 'Trusted entry did not consult the policy owner'
    }
    Invoke-PolicyCase 'trusted_full_allowed_dispatches' {
        $script:PolicyDisposition = 'off'
        $script:PolicyQueryCount = 0
        Assert-ClearraRequestedTaskSurfaces @('ReleaseAcceptance') 'Trusted' 'windows'
        Assert-PolicyValue ($script:PolicyQueryCount -eq 1) 'Allowed full gate skipped the same policy owner'
    }
    Invoke-PolicyCase 'trusted_full_unknown_fails_closed' {
        $script:PolicyDisposition = 'unknown'
        Assert-PolicyReject {
            Assert-ClearraRequestedTaskSurfaces @('ReleaseAcceptance') 'Trusted' 'windows'
        } 'E_WINDOWS_APPLICATION_CONTROL_PREFLIGHT_UNKNOWN'
    }
    Invoke-PolicyCase 'source_validation_remains_available' {
        $script:PolicyDisposition = 'enforced'
        $script:PolicyQueryCount = 0
        Assert-ClearraRequestedTaskSurfaces @('Validate') 'Trusted' 'windows'
        Assert-ClearraRequestedTaskSurfaces @('Validate') 'ManagedLocal' 'windows'
        Assert-PolicyValue ($script:PolicyQueryCount -eq 0) 'Source-only validation was treated as generated execution'
    }
    Invoke-PolicyCase 'managed_generated_tasks_reject_without_policy_query' {
        $script:PolicyQueryCount = 0
        Assert-PolicyReject {
            Assert-ClearraRequestedTaskSurfaces @('ReleaseAcceptance') 'ManagedLocal' 'windows'
        } 'requires -ExecutionSurface Trusted'
        Assert-PolicyValue ($script:PolicyQueryCount -eq 0) 'Managed denial unnecessarily queried platform policy'
    }
    Invoke-PolicyCase 'comma_separated_tasks_cannot_hide_generated_execution' {
        $script:PolicyDisposition = 'enforced'
        Assert-PolicyReject {
            Assert-ClearraRequestedTaskSurfaces @('Validate, ReleaseAcceptance') 'Trusted' 'windows'
        } 'E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE'
    }
    Invoke-PolicyCase 'explicit_other_runtimes_are_not_selected_as_fallback' {
        $script:PolicyQueryCount = 0
        foreach ($runtime in @('wsl', 'wasm')) {
            Assert-ClearraRequestedTaskSurfaces @('ReleaseAcceptance') 'Trusted' $runtime
        }
        Assert-PolicyValue ($script:PolicyQueryCount -eq 0) 'Explicit runtime selection changed the Windows admission owner'
        Assert-PolicyReject {
            Assert-ClearraRequestedTaskSurfaces @('ReleaseAcceptance') 'Trusted' 'auto'
        } 'Unknown Clearra runtime environment'
    }
    Invoke-PolicyCase 'entry_normalizes_auto_and_admits_before_workspace_work' {
        $entry = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'clearra.ps1') -Raw
        $normalize = $entry.IndexOf('$resolvedRequestedRuntime = Resolve-ClearraRuntimeEnvironment $RuntimeEnvironment')
        $admit = $entry.IndexOf('Assert-ClearraRequestedTaskSurfaces')
        $workspace = $entry.IndexOf('$Root = Resolve-ClearraRoot')
        Assert-PolicyValue ($normalize -ge 0 -and $normalize -lt $admit -and $admit -lt $workspace) 'Runtime normalization/admission no longer precedes workspace work'
        Assert-PolicyValue ($entry.Substring($admit, $workspace - $admit).Contains('$resolvedRequestedRuntime')) 'Normalized runtime is not passed to admission'
    }
    Invoke-PolicyCase 'invalid_surface_still_rejects_source_only_call' {
        Assert-PolicyReject {
            Assert-ClearraRequestedTaskSurfaces @('Validate') 'UnknownSurface' 'windows'
        } 'Unknown Clearra execution surface'
    }
    Write-Output "execution_surface_policy=passed cases=$script:PassedCases native_execution=false"
} finally {
    if ($null -eq $previousSurface) { Remove-Item Env:CLEARRA_EXECUTION_SURFACE -ErrorAction SilentlyContinue }
    else { $env:CLEARRA_EXECUTION_SURFACE = $previousSurface }
}
