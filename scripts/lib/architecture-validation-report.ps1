# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Result and summary helpers stay outside the dispatcher.
function New-ArchitectureValidationResult(
    $Status,
    $Errors,
    $Warnings,
    [int]$TaskCount = 0,
    [timespan]$Duration = [timespan]::Zero
) {
    return [pscustomobject]@{
        Status = $Status
        ErrorCount = $Errors.Count
        WarningCount = $Warnings.Count
        TaskCount = $TaskCount
        DurationMs = [int][math]::Round($Duration.TotalMilliseconds)
        Errors = @($Errors.ToArray())
        Warnings = @($Warnings.ToArray())
    }
}
function Format-ArchitectureValidationElapsed([timespan]$Elapsed) {
    if ($Elapsed.TotalSeconds -lt 1) {
        return "$([math]::Round($Elapsed.TotalMilliseconds))ms"
    }
    return "$([math]::Round($Elapsed.TotalSeconds, 2))s"
}
function Get-ArchitectureWarningCategoryCounts([string[]]$WarningMessages) {
    $largeFileCount = 0
    $otherCount = 0
    foreach ($warningMessage in @($WarningMessages)) {
        if ($warningMessage -match 'large-file:' -or
            $warningMessage -match 'has \d+ lines; consider splitting') {
            $largeFileCount += 1
        } else {
            $otherCount += 1
        }
    }
    return [pscustomobject]@{
        LargeFile = $largeFileCount
        Other = $otherCount
    }
}
function Write-ArchitectureValidationSummary(
    [object]$Result,
    [switch]$ShowWarnings,
    [int]$WarningDetailLimit
) {
    $elapsed = [timespan]::FromMilliseconds([double]$Result.DurationMs)
    [Console]::Out.WriteLine("[validate] passed | tasks=$($Result.TaskCount) | warnings=$($Result.WarningCount) | errors=$($Result.ErrorCount) | $(Format-ArchitectureValidationElapsed $elapsed)")

    if ($Result.WarningCount -le 0) {
        return
    }

    $categories = Get-ArchitectureWarningCategoryCounts ([string[]]$Result.Warnings)
    [Console]::Out.WriteLine("[validate] warning summary | large-file=$($categories.LargeFile) | other=$($categories.Other) | use -ShowWarnings or -VerboseLog for details")

    if (-not $ShowWarnings.IsPresent) {
        return
    }

    $limit = [Math]::Max(1, $WarningDetailLimit)
    foreach ($warningMessage in @($Result.Warnings | Select-Object -First $limit)) {
        Write-Warning "architecture warning: $warningMessage"
    }
    if ($Result.WarningCount -gt $limit) {
        [Console]::Out.WriteLine("[validate] ... $($Result.WarningCount - $limit) more warning(s)")
    }
}