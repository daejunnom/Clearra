param(
    [string]$BuildDir,
    [string]$Configuration = "Debug",
    [string]$CMakeBuildType,
    [switch]$AllowMissingCompiler,
    [switch]$VerboseLog,
    [int]$OutputExcerptLines = 40
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# M0 marker: $SourceDir = $Root, cmake -S, -B, --build, AllowMissingCompiler.
. (Join-Path $PSScriptRoot "lib/core-c-build.ps1")

$script:ClearraVerboseLog = $VerboseLog.IsPresent
$script:ClearraOutputExcerptLines = [Math]::Max(1, $OutputExcerptLines)

$configureArgs = @("-DBUILD_TESTING=OFF")
if (-not [string]::IsNullOrWhiteSpace($CMakeBuildType)) {
    $configureArgs += "-DCMAKE_BUILD_TYPE=$CMakeBuildType"
}

$result = Invoke-CoreCBuild `
    -BuildDir $BuildDir `
    -Configuration $Configuration `
    -ConfigureArgs $configureArgs `
    -AllowMissingCompiler:$AllowMissingCompiler.IsPresent

$result | ConvertTo-Json -Depth 8

if ($result.Status -eq "Passed" -or $result.Status -eq "Degraded") {
    exit 0
}

exit 1
