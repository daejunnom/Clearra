# This file is dot-sourced by clearra-start-helpers.ps1.
function Write-CoreCTestStartSummary([string]$ModeName, [object]$Result) {
    if ($Result.Status -eq "Passed") {
        $ctestSummary = "ctest=$($Result.CTestCount)/$($Result.CTestCount)"
        $internalSummary = if (($Result.TestLayout -eq "aggregate" -or $Result.TestLayout -eq "split-build") -and $Result.InternalTestCount -gt 0) {
            " | internal=$($Result.InternalTestCount)/$($Result.InternalTestCount)"
        } else {
            ""
        }
        Write-Output "[core-c] $($Result.TestLayout) passed | $ctestSummary$internalSummary | executed=$($Result.TestExecuted) | compiled=$($Result.TestCompiled)"
    } elseif ($Result.Status -eq "BuiltOnly") {
        $internalSummary = if ($Result.InternalTestCount -gt 0) {
            " | internal=$($Result.InternalTestCount)/$($Result.InternalTestCount)"
        } else {
            ""
        }
        Write-Output "[core-c] $($Result.TestLayout) build-only | ctest=not-run$internalSummary | reason=$($Result.Reason) | executed=$($Result.TestExecuted) | compiled=$($Result.TestCompiled)"
    } elseif ($Result.Status -eq "Degraded") {
        Write-Output "[core-c] degraded | reason=$($Result.Reason) | compiled=$($Result.TestCompiled) | executed=$($Result.TestExecuted)"
    } else {
        Write-Output "[core-c] failed | reason=$($Result.Reason)"
    }
    if ($Json.IsPresent -or $VerboseLog.IsPresent) {
        $Result | ConvertTo-Json -Depth 8
    }
}
function Get-StartTestsCMakeConfigureArgs([string[]]$ExtraArgs = @()) {
    $args = New-Object System.Collections.Generic.List[string]
    $buildType = Get-Variable -Name CMakeBuildType -ValueOnly -ErrorAction SilentlyContinue
    $args.Add("-DCLEARRA_CORE_SPLIT_TESTS=OFF")
    if ($null -ne $ExtraArgs) {
        foreach ($arg in $ExtraArgs) {
            if (-not [string]::IsNullOrWhiteSpace($arg)) {
                $args.Add($arg)
            }
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($buildType)) {
        $args.Add("-DCMAKE_BUILD_TYPE=$buildType")
    }
    return [string[]]$args.ToArray()
}
function Invoke-CoreCTestStartMode(
    [string]$Root,
    [string]$ModeName,
    [string[]]$ConfigureArgs = @(),
    [string]$PersistentBuildName,
    [switch]$AggregateOnly,
    [string]$TestRegex
) {
    $trustedExecution = Test-ClearraTrustedExecutionSurface $ExecutionSurface
    $useNamedPersistentBuild = -not [string]::IsNullOrWhiteSpace($PersistentBuildName)
    $buildDir = if ($useNamedPersistentBuild) {
        Get-StartTestsPersistentBuildDir $PersistentBuildName
    } elseif ($KeepBuildCache.IsPresent -and [string]::IsNullOrWhiteSpace($CoreCBuildDir)) {
        $cacheName = if ($trustedExecution) { "core-c-test-cache" } else { "core-c-library-cache" }
        Get-StartTestsPersistentBuildDir $cacheName
    } else {
        Resolve-CoreCBuildDirForStartTests $Root $KeepBuildCache.IsPresent $CoreCBuildDir
    }
    $ephemeral = (-not $useNamedPersistentBuild -and
        -not $KeepBuildCache.IsPresent -and
        [string]::IsNullOrWhiteSpace($CoreCBuildDir))
    $script:ClearraVerboseLog = $VerboseLog.IsPresent
    $script:ClearraOutputExcerptLines = [Math]::Max(1, $OutputExcerptLines)
    $runtimeVariable = Get-Variable `
        -Name ClearraRuntimeEnvironment `
        -Scope Script `
        -ErrorAction SilentlyContinue
    $runtimeEnvironment = if ($null -eq $runtimeVariable) {
        'auto'
    } else {
        [string]$runtimeVariable.Value
    }
    $distributionVariable = Get-Variable `
        -Name ClearraWslDistribution `
        -Scope Script `
        -ErrorAction SilentlyContinue
    $wslDistribution = if ($null -eq $distributionVariable) {
        'Ubuntu'
    } else {
        [string]$distributionVariable.Value
    }
    try {
        $result = Invoke-CoreCTest `
            -BuildDir $buildDir `
            -Configuration "Debug" `
            -ConfigureArgs $ConfigureArgs `
            -BuildOnly:(-not $trustedExecution) `
            -AggregateOnly:$AggregateOnly.IsPresent `
            -TestRegex $TestRegex `
            -Workers $Workers `
            -RuntimeEnvironment $runtimeEnvironment `
            -WslDistribution $wslDistribution

        Write-CoreCTestStartSummary $ModeName $result

        if ($result.Status -eq "Failed") {
            throw "$ModeName CTest failed"
        }
        if ($trustedExecution -and -not $result.TestExecuted) {
            throw "$ModeName requires executed CTest evidence; status=$($result.Status) reason=$($result.Reason)"
        }
    } finally {
        if ($ephemeral -and (Test-Path -LiteralPath $buildDir)) {
            Remove-TransientBuildDir $buildDir
        }
    }
}
