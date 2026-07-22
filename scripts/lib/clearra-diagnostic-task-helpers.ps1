# This file is dot-sourced by clearra-start-helpers.ps1.
function Invoke-ClearraValidationTask([string]$Root) {
    $result = Invoke-ArchitectureValidation `
        -Workers $Workers `
        -QuietProgress:$false `
        -ShowWarnings:($ShowWarnings.IsPresent -or $VerboseLog.IsPresent) `
        -WarningDetailLimit $WarningDetailLimit

    if ($result.Status -eq "Failed") {
        throw "Architecture validation failed."
    }
}
function Invoke-DiagnoseCArtifactsTask([string]$Root) {
    $arguments = @{}
    if (-not [string]::IsNullOrWhiteSpace($CoreCBuildDir)) {
        $arguments["BuildDir"] = $CoreCBuildDir
    } elseif ($KeepBuildCache.IsPresent) {
        $arguments["BuildDir"] = Get-StartTestsPersistentBuildDir "core-c-test-cache"
    }
    if (-not [string]::IsNullOrWhiteSpace($ReportPath)) {
        $arguments["ReportPath"] = $ReportPath
    }
    & (Join-Path $Root "scripts/diagnose-c-core-test-artifacts.ps1") @arguments
    if (-not $?) {
        throw "C core artifact diagnosis failed."
    }
}
function Invoke-CollectWindowsBlockEventsTask([string]$Root) {
    $arguments = @{
        Minutes = $Minutes
    }
    if (-not [string]::IsNullOrWhiteSpace($ReportPath)) {
        $arguments["ReportPath"] = $ReportPath
    }
    & (Join-Path $Root "scripts/collect-windows-block-events.ps1") @arguments
    if (-not $?) {
        throw "Windows block event collection failed."
    }
}
function Invoke-RunnerSecurityTask([string]$Root) {
    & (Join-Path $Root "scripts/test_verify_security.ps1")
    if (-not $?) {
        throw "Runner security meta-test failed."
    }
}
