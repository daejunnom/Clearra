function New-ArchitectureValidationResult($Status, $Errors, $Warnings) {
    return [pscustomobject]@{
        Status = $Status
        ErrorCount = $Errors.Count
        WarningCount = $Warnings.Count
        Errors = @($Errors.ToArray())
        Warnings = @($Warnings.ToArray())
    }
}function New-ArchitectureValidationTaskResult(
    [string]$Name,
    [string]$Status,
    [string[]]$Errors,
    [string[]]$Warnings,
    [int64]$DurationMs
) {
    return [pscustomobject]@{
        Name = $Name
        Status = $Status
        Errors = @($Errors)
        Warnings = @($Warnings)
        DurationMs = $DurationMs
    }
}