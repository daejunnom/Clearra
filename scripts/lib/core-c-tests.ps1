$script:ClearraCoreCTestsLibRoot = Split-Path -Parent $PSCommandPath
. (Join-Path $script:ClearraCoreCTestsLibRoot "core-c-build.ps1")
. (Join-Path $script:ClearraCoreCTestsLibRoot "clearra-application-control.ps1")
. (Join-Path $script:ClearraCoreCTestsLibRoot "clearra-runtime-environment.ps1")
function New-CoreCTestResult(
    [string]$Status,
    [string]$Reason,
    [bool]$TestExecuted,
    [bool]$TestCompiled,
    [int]$TestCount,
    [string]$TestLayout = "unknown",
    [int]$InternalTestCount = 0,
    [string]$BuildDir,
    [string]$Output,
    [string]$Command
) {
    return [pscustomobject]@{
        Status = $Status
        Reason = $Reason
        TestExecuted = $TestExecuted
        TestCompiled = $TestCompiled
        TestCount = $TestCount
        CTestCount = $TestCount
        TestLayout = $TestLayout
        InternalTestCount = $InternalTestCount
        BuildDir = $BuildDir
        Command = $Command
        OutputExcerpt = if ($Status -eq "Passed") { $null } else { Get-CoreCOutputExcerpt $Output }
    }
}
function Test-CoreCTestConfigureArgEnabled([string[]]$ConfigureArgs, [string]$Name) {
    foreach ($arg in @($ConfigureArgs)) {
        if ($arg -match "^-D$([regex]::Escape($Name))=(ON|TRUE|1)$") {
            return $true
        }
    }
    return $false
}
function Get-CoreCTestLayout([string[]]$ConfigureArgs) {
    if (Test-CoreCTestConfigureArgEnabled $ConfigureArgs "CLEARRA_CORE_SPLIT_TESTS") {
        return "split"
    }
    return "aggregate"
}
function Get-CoreCTestInternalAggregateCount() {
    $repoRoot = Resolve-Path -LiteralPath (Join-Path $script:ClearraCoreCTestsLibRoot "..\..")
    $cmakePath = Join-Path $repoRoot "core-c\cmake\test_targets.cmake"
    if (-not (Test-Path -LiteralPath $cmakePath)) {
        return 0
    }

    $contents = Get-Content -LiteralPath $cmakePath -Raw
    $match = [regex]::Match(
        $contents,
        '(?s)set\(CLEARRA_CORE_TEST_NAMES\s+(?<body>.*?)\)\s*set\(CLEARRA_CORE_TEST_SOURCES'
    )
    if (-not $match.Success) {
        return 0
    }

    $bodyWithoutComments = [regex]::Replace(
        $match.Groups["body"].Value,
        '(?m)#.*$',
        ''
    )
    return @(
        $bodyWithoutComments -split '\s+' |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    ).Count
}
function Get-CoreCTestRegisteredTestCount([string]$BuildDirectory, [string]$BuildConfiguration) {
    $result = Invoke-CoreCNativeCapture "ctest" @("--test-dir", $BuildDirectory, "--build-config", $BuildConfiguration, "-N") "ctest discovery" -QuietOnSuccess
    if ($result.ExitCode -ne 0) {
        Write-CoreCFailureExcerpt $result.Output
        throw "core-c CTest discovery failed with exit code $($result.ExitCode)"
    }

    foreach ($line in @($result.Output -split "`r?`n")) {
        if ($line -match "Total Tests:\s*(\d+)") {
            return [int]$Matches[1]
        }
    }

    return @($result.Output -split "`r?`n" | Where-Object { $_ -match "^\s*Test\s+#\d+:" }).Count
}
function Invoke-CoreCTestWsl(
    [string[]]$ConfigureArgs,
    [string]$TestRegex,
    [int]$Workers,
    [string]$WslDistribution
) {
    $allowedConfigureArgs = @(
        '^-DBUILD_TESTING=(ON|OFF)$',
        '^-DCLEARRA_CORE_SPLIT_TESTS=(ON|OFF)$',
        '^-DCLEARRA_CORE_ENABLE_ASAN=(ON|OFF)$',
        '^-DCLEARRA_CORE_ENABLE_UBSAN=(ON|OFF)$',
        '^-DCLEARRA_ENABLE_STAGE_PROFILING=(ON|OFF)$',
        '^-DCMAKE_BUILD_TYPE=[A-Za-z0-9_-]+$'
    )
    foreach ($arg in @($ConfigureArgs)) {
        if (-not ($allowedConfigureArgs | Where-Object { $arg -match $_ })) {
            throw "WSL aggregate C tests do not silently ignore configure argument '$arg'."
        }
    }

    $testName = $null
    if (-not [string]::IsNullOrWhiteSpace($TestRegex)) {
        $match = [regex]::Match($TestRegex, '^\^?(?<name>[A-Za-z0-9_]+)\$?$')
        if (-not $match.Success) {
            throw "WSL aggregate C tests require an exact test-group selector, not regex '$TestRegex'."
        }
        $testName = $match.Groups['name'].Value
    }

    $repositoryRoot = [string](Resolve-Path -LiteralPath (Join-Path $script:ClearraCoreCTestsLibRoot '..\..'))
    $sync = Sync-ClearraWslExt4Workspace $repositoryRoot $WslDistribution
    $workerCount = [Math]::Max(1, $Workers)
    $arguments = @(
        '-d', $WslDistribution, '--',
        'env', "CLEARRA_WSL_WORKSPACE=$($sync.workspace)",
        'bash', "$($sync.workspace)/scripts/tools/wsl-core-c-tests.sh",
        '--workers', [string]$workerCount
    )
    if (Test-CoreCTestConfigureArgEnabled $ConfigureArgs 'CLEARRA_CORE_ENABLE_ASAN') {
        $arguments += @('--sanitizer', 'address')
    } elseif (Test-CoreCTestConfigureArgEnabled $ConfigureArgs 'CLEARRA_CORE_ENABLE_UBSAN') {
        $arguments += @('--sanitizer', 'undefined')
    }
    if (Test-CoreCTestConfigureArgEnabled $ConfigureArgs 'CLEARRA_ENABLE_STAGE_PROFILING') {
        $arguments += '--profile'
    }
    if (-not [string]::IsNullOrWhiteSpace($testName)) {
        $arguments += @('--test', $testName)
    }

    $result = Invoke-CoreCNativeCapture 'wsl.exe' $arguments 'WSL aggregate C tests'
    if ($result.ExitCode -ne 0) {
        Write-CoreCFailureExcerpt $result.Output
        throw "WSL aggregate C tests failed with exit code $($result.ExitCode)"
    }
    $internalTestCount = Get-CoreCTestInternalAggregateCount
    return New-CoreCTestResult `
        -Status 'Passed' `
        -Reason $null `
        -TestExecuted $true `
        -TestCompiled $true `
        -TestCount 1 `
        -TestLayout 'wsl-aggregate' `
        -InternalTestCount $internalTestCount `
        -BuildDir $sync.workspace `
        -Output $result.Output `
        -Command "wsl.exe $($arguments -join ' ')"
}
function Invoke-CoreCTest(
    [string]$BuildDir,
    [string]$Configuration = "Debug",
    [string[]]$ConfigureArgs = @(),
    [switch]$AllowMissingCompiler,
    [switch]$BuildOnly,
    [switch]$AggregateOnly,
    [string]$TestRegex,
    [int]$Workers = 1,
    [ValidateSet('auto', 'windows', 'wsl')]
    [string]$RuntimeEnvironment = 'auto',
    [string]$WslDistribution = 'Ubuntu'
) {
    $workerCount = [Math]::Min(
        [Math]::Max(1, $Workers),
        [Math]::Max(1, [Environment]::ProcessorCount)
    )
    if ([string]::IsNullOrWhiteSpace($BuildDir)) {
        $BuildDir = if ($BuildOnly.IsPresent) {
            "core-c-library-cache"
        } else {
            "core-c-test-cache"
        }
    }
    $progressScope = New-CoreCProgressScope 3 $workerCount
    $effectiveConfigureArgs = @($ConfigureArgs)
    if ($BuildOnly.IsPresent -and
        -not ($effectiveConfigureArgs | Where-Object { $_ -match '^-DBUILD_TESTING=' })) {
        $effectiveConfigureArgs += '-DBUILD_TESTING=OFF'
    }
    $runtime = Assert-ClearraRuntimeEnvironmentAvailable $RuntimeEnvironment $WslDistribution
    if ($runtime -eq 'wsl') {
        if ($BuildOnly.IsPresent) {
            throw 'WSL C-test execution requires -ExecutionSurface Trusted; no test executable was generated.'
        }
        return Invoke-CoreCTestWsl `
            -ConfigureArgs $effectiveConfigureArgs `
            -TestRegex $TestRegex `
            -Workers $workerCount `
            -WslDistribution $WslDistribution
    }
    if ($runtime -ne 'windows') {
        throw "C tests are unavailable in independent runtime '$runtime'."
    }
    if (-not $BuildOnly.IsPresent) {
        Assert-ClearraWindowsGeneratedExecutionAllowed 'core-c CTest' | Out-Null
    }
    $testLayout = if ($BuildOnly.IsPresent) {
        'library-only'
    } else {
        Get-CoreCTestLayout $effectiveConfigureArgs
    }
    $internalTestCount = if ($testLayout -eq "aggregate" -or $AggregateOnly.IsPresent) {
        Get-CoreCTestInternalAggregateCount
    } else {
        0
    }
    $resolvedBuildDir = Resolve-CoreCBuildDir $BuildDir
    $buildResult = Invoke-CoreCBuild `
        -BuildDir $resolvedBuildDir `
        -Configuration $Configuration `
        -ConfigureArgs $effectiveConfigureArgs `
        -BuildWorkers $workerCount `
        -AllowMissingCompiler:$AllowMissingCompiler.IsPresent `
        -ProgressScope $progressScope

    if ($buildResult.Status -ne "Passed") {
        Complete-CoreCProgressLine $progressScope
        [Console]::Out.WriteLine("==> core-c CMake tests degraded: CMake or C compiler unavailable; CTest not executed")
        $ctestCommand = "ctest --test-dir $resolvedBuildDir --build-config $Configuration --output-on-failure"
        if ($workerCount -gt 1) {
            $ctestCommand = "$ctestCommand -j $workerCount"
        }
        return New-CoreCTestResult `
            -Status "Degraded" `
            -Reason $buildResult.Reason `
            -TestExecuted $false `
            -TestCompiled $false `
            -TestCount 0 `
            -TestLayout $testLayout `
            -InternalTestCount $internalTestCount `
            -BuildDir $resolvedBuildDir `
            -Output $buildResult.OutputExcerpt `
            -Command $ctestCommand
    }

    if ($BuildOnly.IsPresent) {
        Start-CoreCProgressStep $progressScope "process-free"
        Complete-CoreCProgressStep $progressScope
        Complete-CoreCProgressLine $progressScope
        [Console]::Out.WriteLine("==> core-c library compiled with BUILD_TESTING=OFF; no CTest executable was generated by this run")
        return New-CoreCTestResult `
            -Status "BuiltOnly" `
            -Reason "ManagedLocalProcessFree" `
            -TestExecuted $false `
            -TestCompiled $false `
            -TestCount 0 `
            -TestLayout "library-only" `
            -InternalTestCount 0 `
            -BuildDir $resolvedBuildDir `
            -Output "" `
            -Command "cmake --build $resolvedBuildDir --target clearra_core"
    }

    if (-not (Test-Path -LiteralPath (Join-Path $resolvedBuildDir "CTestTestfile.cmake"))) {
        Complete-CoreCProgressLine $progressScope
        throw "core-c CTest root file is missing after a successful CMake configure: $resolvedBuildDir"
    }

    Start-CoreCProgressStep $progressScope "ctest"
    $testCount = Get-CoreCTestRegisteredTestCount $resolvedBuildDir $Configuration
    if ($testCount -le 0) {
        Fail-CoreCProgressStep $progressScope "ctest"
        throw "core-c CTest registered zero tests; root CMake/CTest configuration is invalid"
    }
    $effectiveTestLayout = $testLayout
    $effectiveTestCount = $testCount
    $ctestArgs = @("--test-dir", $resolvedBuildDir, "--build-config", $Configuration, "--output-on-failure")
    if ($AggregateOnly.IsPresent) {
        $aggregateNames = @("clearra_core_all_tests")
        $ctestArgs += @("-R", "^($($aggregateNames -join '|'))$")
        $effectiveTestLayout = if ($testLayout -eq "split") {
            "split-build"
        } else {
            "aggregate"
        }
        $effectiveTestCount = $aggregateNames.Count
    }
    if (-not [string]::IsNullOrWhiteSpace($TestRegex)) {
        if ($AggregateOnly.IsPresent) {
            throw "Invoke-CoreCTest cannot combine AggregateOnly with TestRegex"
        }
        $ctestArgs += @("-R", $TestRegex)
        $effectiveTestLayout = "filtered"
        $effectiveTestCount = 1
    }
    if ($workerCount -gt 1) {
        $ctestArgs += @("-j", [string]$workerCount)
    }
    $command = "ctest $($ctestArgs -join ' ')"
    Assert-ClearraWindowsGeneratedExecutionAllowed 'core-c CTest execution' | Out-Null
    $ctestStarted = Get-Date
    $ctestResult = Invoke-CoreCNativeCapture "ctest" $ctestArgs "ctest"
    if ($ctestResult.ExitCode -ne 0) {
        Fail-CoreCProgressStep $progressScope "ctest"
        Write-CoreCFailureExcerpt $ctestResult.Output
        $blockEvidence = Wait-ClearraGeneratedExecutableBlockEvidence `
            -Since $ctestStarted `
            -ParentProcessName 'ctest.exe'
        if ($blockEvidence.query_status -eq 'ok' -and
            $blockEvidence.matched_event_count -gt 0) {
            throw (New-ClearraLocalSourceBuildBlockedMessage `
                'core-c CTest execution' `
                (Get-ClearraApplicationControlStatus) `
                $blockEvidence)
        }
        throw "core-c CTest failed with exit code $($ctestResult.ExitCode)"
    }

    Complete-CoreCProgressStep $progressScope
    Complete-CoreCProgressLine $progressScope
    return New-CoreCTestResult `
        -Status "Passed" `
        -Reason $null `
        -TestExecuted $true `
        -TestCompiled $true `
        -TestCount $effectiveTestCount `
        -TestLayout $effectiveTestLayout `
        -InternalTestCount $internalTestCount `
        -BuildDir $resolvedBuildDir `
        -Output $ctestResult.Output `
        -Command $command
}
