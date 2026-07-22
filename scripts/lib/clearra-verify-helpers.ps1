# This file is dot-sourced by clearra-start-helpers.ps1.
function Add-VerifyCommonArgs(
    [hashtable]$Arguments
) {
    if ($VerboseLog.IsPresent) {
        $Arguments["VerboseLog"] = $true
    }
    $Arguments["Workers"] = $Workers
    $Arguments["OutputExcerptLines"] = $OutputExcerptLines
    $Arguments["CargoPath"] = $CargoPath
    $Arguments["PowerShellPath"] = $PowerShellPath

    $Arguments["ExecutionSurface"] = $ExecutionSurface

    if ($KeepBuildCache.IsPresent) {
        $buildName = if (Test-ClearraTrustedExecutionSurface $ExecutionSurface) {
            "core-c-test-cache"
        } else {
            "core-c-library-cache"
        }
        $Arguments["CoreCBuildDir"] = Get-StartTestsPersistentBuildDir $buildName
    } elseif (-not [string]::IsNullOrWhiteSpace($CoreCBuildDir)) {
        $Arguments["CoreCBuildDir"] = $CoreCBuildDir
    }
}
function Build-VerifyArgs(
    [string]$Root,
    [bool]$StrictMode,
    [bool]$RunSecurity
) {
    $args = @{}
    if ($StrictMode) {
        $args["Strict"] = $true
    }
    if ($RunSecurity) {
        $args["RunVerifySecurity"] = $true
    }

    Add-VerifyCommonArgs $args

    if (-not [string]::IsNullOrWhiteSpace($ReportDir)) {
        $resolvedReportDir = Resolve-ClearraReportPath $ReportDir $Root
        if (-not (Test-Path -LiteralPath $resolvedReportDir)) {
            New-Item -ItemType Directory -Force -Path $resolvedReportDir | Out-Null
        }
        $reportName = if ($StrictMode) {
            "verify-strict.json"
        } elseif ($RunSecurity) {
            "verify-security.json"
        } else {
            "verify-local.json"
        }
        $args["ReportPath"] = (Join-Path $resolvedReportDir $reportName)
    } elseif (-not [string]::IsNullOrWhiteSpace($ReportPath)) {
        $args["ReportPath"] = Resolve-ClearraReportPath $ReportPath $Root
    }

    return $args
}
