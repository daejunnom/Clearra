param(
    [Parameter(Mandatory)] [string]$InputSvg,
    [Parameter(Mandatory)] [string]$OutputSvg,
    [string]$CargoPath = "cargo"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# Build-time only. Runtime raw SVG rendering and limit-disable modes are forbidden.
$root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "../..")
. (Join-Path $root "scripts/lib/clearra-path-helpers.ps1")
$previousTargetDir = $env:CARGO_TARGET_DIR
$previousCargoIncremental = $env:CARGO_INCREMENTAL
$previousBuildCacheSessionKey = $env:CLEARRA_BUILD_CACHE_SESSION_KEY
$previousBuildCacheOwnerPid = $env:CLEARRA_BUILD_CACHE_OWNER_PID
$targetDir = Get-ClearraCargoTargetDir
$env:CARGO_TARGET_DIR = $targetDir
try {
    Push-Location $root
    & $CargoPath build -q -p clearra-render --features asset-import --bin clearra-asset-import
    if ($LASTEXITCODE -ne 0) { throw "clearra-asset-import build failed" }
    $tool = Join-Path $targetDir "debug/clearra-asset-import.exe"
    & $tool sanitize --input (Resolve-Path -LiteralPath $InputSvg) --output $OutputSvg
    if ($LASTEXITCODE -ne 0) { throw "SVG sanitize failed" }
} finally {
    Pop-Location
    Exit-ClearraBuildArtifactCacheUsage
    if ([string]::IsNullOrWhiteSpace($previousTargetDir)) {
        Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $previousTargetDir
    }
    if ([string]::IsNullOrWhiteSpace($previousCargoIncremental)) {
        Remove-Item Env:\CARGO_INCREMENTAL -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_INCREMENTAL = $previousCargoIncremental
    }
    if ([string]::IsNullOrWhiteSpace($previousBuildCacheSessionKey)) {
        Remove-Item Env:\CLEARRA_BUILD_CACHE_SESSION_KEY -ErrorAction SilentlyContinue
    } else {
        $env:CLEARRA_BUILD_CACHE_SESSION_KEY = $previousBuildCacheSessionKey
    }
    if ([string]::IsNullOrWhiteSpace($previousBuildCacheOwnerPid)) {
        Remove-Item Env:\CLEARRA_BUILD_CACHE_OWNER_PID -ErrorAction SilentlyContinue
    } else {
        $env:CLEARRA_BUILD_CACHE_OWNER_PID = $previousBuildCacheOwnerPid
    }
}
