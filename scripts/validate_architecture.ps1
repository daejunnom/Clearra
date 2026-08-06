param(
    [string]$TaskName = "",
    [int]$Workers = [Math]::Max(1, [Environment]::ProcessorCount),
    [switch]$QuietProgress,
    [switch]$ShowWarnings,
    [switch]$VerboseLog,
    [int]$WarningDetailLimit = 5
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot "lib/architecture-validation.ps1")

# Delegation markers live in scripts/lib/architecture-validation.ps1:
# architecture\validate_dependencies.ps1
# architecture\validate_cli_boundaries.ps1
# architecture\validate_test_policy.ps1
# architecture\validate_security.ps1
# architecture\validate_file_size.ps1

$result = Invoke-ArchitectureValidation `
    -TaskName $TaskName `
    -Workers $Workers `
    -QuietProgress:$QuietProgress.IsPresent `
    -ShowWarnings:($ShowWarnings.IsPresent -or $VerboseLog.IsPresent) `
    -WarningDetailLimit $WarningDetailLimit

if ($result.Status -eq "Failed") {
    if (-not [string]::IsNullOrWhiteSpace($TaskName)) {
        foreach ($errorMessage in @($result.Errors)) {
            [Console]::Error.WriteLine("architecture error: $errorMessage")
        }
    }
    exit 1
}

exit 0
