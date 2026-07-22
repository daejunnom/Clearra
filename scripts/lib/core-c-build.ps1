$script:ClearraCoreCBuildLibRoot = Split-Path -Parent $PSCommandPath
$script:ClearraScriptsRoot = Split-Path -Parent $script:ClearraCoreCBuildLibRoot
$script:ClearraRoot = Resolve-Path -LiteralPath (Join-Path $script:ClearraScriptsRoot "..")
. (Join-Path $script:ClearraCoreCBuildLibRoot "clearra-path-helpers.ps1")
function Resolve-CoreCBuildDir([string]$BuildDir) {
    if ([string]::IsNullOrWhiteSpace($BuildDir)) {
        $BuildDir = "core-c-cache"
    }
    $path = Resolve-ClearraArtifactPath $BuildDir $script:ClearraRoot
    New-Item -ItemType Directory -Force -Path $path | Out-Null
    return $path
}function Test-CoreCVerboseLog() {
    $variable = Get-Variable -Name ClearraVerboseLog -Scope Script -ErrorAction SilentlyContinue
    return $null -ne $variable -and [bool]$variable.Value
}function Get-CoreCOutputExcerptLineLimit() {
    $variable = Get-Variable -Name ClearraOutputExcerptLines -Scope Script -ErrorAction SilentlyContinue
    if ($null -eq $variable -or $null -eq $variable.Value) {
        return 40
    }
    return [Math]::Max(1, [int]$variable.Value)
}function Get-CoreCOutputExcerpt([string]$Output) {
    if ([string]::IsNullOrWhiteSpace($Output)) {
        return $null
    }
    $lines = @($Output -split "`r?`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($lines.Count -eq 0) {
        return $null
    }
    $lineLimit = Get-CoreCOutputExcerptLineLimit
    $excerpt = ($lines | Select-Object -Last $lineLimit) -join "`n"
    $charLimit = [Math]::Max(2000, $lineLimit * 200)
    if ($excerpt.Length -gt $charLimit) {
        return $excerpt.Substring($excerpt.Length - $charLimit)
    }
    return $excerpt
}function New-CoreCProgressScope([int]$Total, [int]$Workers) {
    return [pscustomobject]@{
        Total = [Math]::Max(1, $Total)
        Workers = [Math]::Max(1, $Workers)
        Done = 0
        Running = 0
        Pending = [Math]::Max(1, $Total)
        Failed = 0
        StartedAt = Get-Date
        LastLineLength = 0
    }
}function Format-CoreCProgressElapsed([timespan]$Elapsed) {
    if ($Elapsed.TotalSeconds -lt 1) {
        return "$([math]::Round($Elapsed.TotalMilliseconds))ms"
    }
    return "$([math]::Round($Elapsed.TotalSeconds, 2))s"
}function Write-CoreCProgressLine($Scope, [string]$Current = "") {
    if ($null -eq $Scope -or (Test-CoreCVerboseLog)) {
        return
    }

    $suffix = if ([string]::IsNullOrWhiteSpace($Current)) {
        ""
    } else {
        " | $Current"
    }
    $message = "[core-c] $($Scope.Done)/$($Scope.Total) done | running $($Scope.Running) | pending $($Scope.Pending) | failed $($Scope.Failed) | workers $($Scope.Workers) | $(Format-CoreCProgressElapsed ((Get-Date) - $Scope.StartedAt))$suffix"
    $paddingLength = [Math]::Max(0, $Scope.LastLineLength - $message.Length)
    $padding = " " * $paddingLength
    [Console]::Out.Write("`r$message$padding")
    $Scope.LastLineLength = $message.Length
}function Start-CoreCProgressStep($Scope, [string]$Name) {
    if ($null -eq $Scope) {
        return
    }
    $Scope.Running = 1
    $Scope.Pending = [Math]::Max(0, $Scope.Total - $Scope.Done - 1)
    Write-CoreCProgressLine $Scope $Name
}function Complete-CoreCProgressStep($Scope) {
    if ($null -eq $Scope) {
        return
    }
    $Scope.Done += 1
    $Scope.Running = 0
    $Scope.Pending = [Math]::Max(0, $Scope.Total - $Scope.Done)
    Write-CoreCProgressLine $Scope
}function Fail-CoreCProgressStep($Scope, [string]$Name) {
    if ($null -eq $Scope) {
        return
    }
    $Scope.Failed += 1
    $Scope.Running = 0
    $Scope.Pending = [Math]::Max(0, $Scope.Total - $Scope.Done - 1)
    Write-CoreCProgressLine $Scope $Name
    Complete-CoreCProgressLine $Scope
}function Complete-CoreCProgressLine($Scope) {
    if ($null -ne $Scope -and -not (Test-CoreCVerboseLog) -and $Scope.LastLineLength -gt 0) {
        [Console]::Out.WriteLine("")
    }
    if ($null -ne $Scope) {
        $Scope.LastLineLength = 0
    }
}function Write-CoreCFailureExcerpt([string]$Output) {
    $excerpt = Get-CoreCOutputExcerpt $Output
    if (-not [string]::IsNullOrWhiteSpace($excerpt)) {
        [Console]::Out.WriteLine("---- last $(Get-CoreCOutputExcerptLineLimit) output line(s) ----")
        [Console]::Out.WriteLine($excerpt)
        [Console]::Out.WriteLine("---- end output excerpt ----")
    }
}function Test-CoreCCMakeToolUnavailable([string]$Output) {
    return $Output -match "No CMAKE_C_COMPILER could be found" -or
        $Output -match "CMAKE_C_COMPILER.*not set" -or
        $Output -match "is not a full path and was not found in the PATH" -or
        $Output -match "No CMAKE_MAKE_PROGRAM could be found" -or
        $Output -match "CMake was unable to find a build program"
}function Invoke-CoreCNativeCapture(
    [string]$Name,
    [string[]]$Arguments,
    [string]$Label,
    [switch]$QuietOnSuccess,
    [switch]$QuietOnFailure
) {
    $commandText = "$Name $($Arguments -join ' ')"
    $summaryLabel = if ([string]::IsNullOrWhiteSpace($Label)) { $Name } else { $Label }
    if (Test-CoreCVerboseLog) {
        [Console]::Out.WriteLine("==> $commandText")
    }
    $started = Get-Date
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = @(& $Name @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $elapsed = (Get-Date) - $started
    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($line in $output) {
        $textLine = $line.ToString()
        $lines.Add($textLine)
        if (Test-CoreCVerboseLog) {
            [Console]::Out.WriteLine($textLine)
        }
    }
    if ($exitCode -eq 0) {
        if ((Test-CoreCVerboseLog) -and -not $QuietOnSuccess.IsPresent) {
            [Console]::Out.WriteLine("[$summaryLabel] passed | $([math]::Round($elapsed.TotalSeconds, 2))s")
        }
    } else {
        if (-not $QuietOnFailure.IsPresent) {
            [Console]::Out.WriteLine("[$summaryLabel] failed | exit=$exitCode | $([math]::Round($elapsed.TotalSeconds, 2))s")
        }
    }
    return [pscustomobject]@{
        ExitCode = $exitCode
        Output = ($lines -join "`n")
        ElapsedSeconds = $elapsed.TotalSeconds
        Command = $commandText
    }
}function New-CoreCBuildResult(
    [string]$Status,
    [string]$Reason,
    [bool]$BuildExecuted,
    [string]$BuildDir,
    [string]$Output
) {
    return [pscustomobject]@{
        Status = $Status
        Reason = $Reason
        BuildExecuted = $BuildExecuted
        BuildDir = $BuildDir
        OutputExcerpt = if ($Status -eq "Passed") { $null } else { Get-CoreCOutputExcerpt $Output }
    }
}function Invoke-CoreCCMakeStep(
    [string[]]$Arguments,
    [string]$StepName,
    [string]$BuildDir,
    [bool]$AllowMissingCompiler,
    $ProgressScope = $null
) {
    Start-CoreCProgressStep $ProgressScope "cmake $StepName"
    $stepResult = Invoke-CoreCNativeCapture "cmake" $Arguments "cmake $StepName"
    if ($stepResult.ExitCode -eq 0) {
        Complete-CoreCProgressStep $ProgressScope
        return New-CoreCBuildResult "Passed" $null $true $BuildDir $stepResult.Output
    }

    $message = "CMake $StepName failed with exit code $($stepResult.ExitCode)"
    if ($AllowMissingCompiler -and (Test-CoreCCMakeToolUnavailable $stepResult.Output)) {
        Fail-CoreCProgressStep $ProgressScope "cmake $StepName"
        [Console]::Out.WriteLine("==> core-c CMake build skipped: $message")
        return New-CoreCBuildResult "Degraded" "CompilerUnavailable" $false $BuildDir $stepResult.Output
    }

    Fail-CoreCProgressStep $ProgressScope "cmake $StepName"
    Write-CoreCFailureExcerpt $stepResult.Output
    throw $message
}function Invoke-CoreCBuild(
    [string]$BuildDir,
    [string]$Configuration = "Debug",
    [string[]]$ConfigureArgs = @(),
    [int]$BuildWorkers = 1,
    [switch]$AllowMissingCompiler,
    $ProgressScope = $null
) {
    $sourceDir = $script:ClearraRoot
    $resolvedBuildDir = Resolve-CoreCBuildDir $BuildDir

    if ($null -eq (Get-Command cmake -ErrorAction SilentlyContinue)) {
        $message = "CMake was not found. Install CMake to build core-c."
        if ($AllowMissingCompiler.IsPresent) {
            [Console]::Out.WriteLine("==> core-c CMake build skipped: $message")
            return New-CoreCBuildResult "Degraded" "CMakeUnavailable" $false $resolvedBuildDir $message
        }
        throw $message
    }

    New-Item -ItemType Directory -Force -Path $resolvedBuildDir | Out-Null

    $configureArguments = @("-S", $sourceDir, "-B", $resolvedBuildDir)
    if ($null -ne $ConfigureArgs -and $ConfigureArgs.Count -gt 0) {
        $configureArguments += $ConfigureArgs
    }

    $configureResult = Invoke-CoreCCMakeStep $configureArguments "configure" $resolvedBuildDir $AllowMissingCompiler.IsPresent $ProgressScope
    if ($configureResult.Status -ne "Passed") {
        return $configureResult
    }

    $buildArguments = @("--build", $resolvedBuildDir, "--config", $Configuration)
    if ($BuildWorkers -gt 1) {
        $buildArguments += @("--parallel", [string]$BuildWorkers)
    }

    return Invoke-CoreCCMakeStep $buildArguments "build" $resolvedBuildDir $AllowMissingCompiler.IsPresent $ProgressScope
}
