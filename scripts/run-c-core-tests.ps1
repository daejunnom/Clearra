param(
    [string]$BuildDir,
    [string]$Configuration = "Debug",
    [string]$CMakeBuildType,
    [switch]$Split,
    [switch]$EnableAsan,
    [switch]$EnableUbsan,
    [switch]$AllowMissingCompiler,
    [ValidateSet("ManagedLocal", "Trusted")]
    [string]$ExecutionSurface = "ManagedLocal",
    [ValidateSet("auto", "windows", "wsl")]
    [string]$RuntimeEnvironment = "auto",
    [string]$WslDistribution = "Ubuntu",
    [switch]$VerboseLog,
    [switch]$Json,
    [int]$OutputExcerptLines = 40,
    [int]$Workers = 1
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# M0 marker: build-core-c.ps1, ctest --test-dir, Total Tests:,
# registered zero tests, CMake tests degraded, AllowMissingCompiler.
. (Join-Path $PSScriptRoot "lib/core-c-tests.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")

$script:ClearraVerboseLog = $VerboseLog.IsPresent
$script:ClearraOutputExcerptLines = [Math]::Max(1, $OutputExcerptLines)

$configureArgs = @()
if (-not [string]::IsNullOrWhiteSpace($CMakeBuildType)) {
    $configureArgs += "-DCMAKE_BUILD_TYPE=$CMakeBuildType"
}
if ($Split.IsPresent) {
    $configureArgs += "-DCLEARRA_CORE_SPLIT_TESTS=ON"
}
if ($EnableAsan.IsPresent) {
    $configureArgs += "-DCLEARRA_CORE_ENABLE_ASAN=ON"
}
if ($EnableUbsan.IsPresent) {
    $configureArgs += "-DCLEARRA_CORE_ENABLE_UBSAN=ON"
}

$result = Invoke-CoreCTest `
    -BuildDir $BuildDir `
    -Configuration $Configuration `
    -ConfigureArgs $configureArgs `
    -AllowMissingCompiler:$AllowMissingCompiler.IsPresent `
    -BuildOnly:(-not (Test-ClearraTrustedExecutionSurface $ExecutionSurface)) `
    -RuntimeEnvironment $RuntimeEnvironment `
    -WslDistribution $WslDistribution `
    -Workers $Workers

if ($result.Status -eq "Passed") {
    $ctestSummary = "ctest=$($result.CTestCount)/$($result.CTestCount)"
    $internalSummary = if ($result.TestLayout -eq "aggregate" -and $result.InternalTestCount -gt 0) {
        " | internal=$($result.InternalTestCount)/$($result.InternalTestCount)"
    } else {
        ""
    }
    Write-Output "[ctest] $($result.TestLayout) passed | $ctestSummary$internalSummary | executed=$($result.TestExecuted) | compiled=$($result.TestCompiled)"
} elseif ($result.Status -eq "BuiltOnly") {
    Write-Output "[ctest] not-built | reason=$($result.Reason) | execution_surface=$ExecutionSurface"
} elseif ($result.Status -eq "Degraded") {
    Write-Output "[ctest] degraded | reason=$($result.Reason) | executed=$($result.TestExecuted) | compiled=$($result.TestCompiled)"
} else {
    Write-Output "[ctest] failed | reason=$($result.Reason)"
}

if ($Json.IsPresent -or $VerboseLog.IsPresent) {
    $result | ConvertTo-Json -Depth 8
}

if ($result.Status -eq "Failed" -or
    ((Test-ClearraTrustedExecutionSurface $ExecutionSurface) -and -not $result.TestExecuted)) {
    exit 1
}

exit 0
