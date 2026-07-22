param(
    [Parameter(Mandatory)] [string]$InputSvg,
    [Parameter(Mandatory)] [string]$OutputDirectory,
    [Parameter(Mandatory)] [string]$SkinId,
    [Parameter(Mandatory)] [string]$DisplayName,
    [Parameter(Mandatory)] [string]$License,
    [Parameter(Mandatory)] [string]$Redistribution,
    [ValidateSet("external-svg", "builtin-reviewed", "human-reviewed-svg")]
    [string]$OriginKind = "human-reviewed-svg",
    [int]$TileWidth = 16,
    [int]$TileHeight = 16,
    [string]$CargoPath = "cargo"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# One build-time importer owns sanitize, bounded resvg rasterization, manifest and provenance.
$root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "../..")
$source = Resolve-Path -LiteralPath $InputSvg
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
    & $tool import `
        --input $source `
        --output-dir $OutputDirectory `
        --source-label $source `
        --origin-kind $OriginKind `
        --skin-id $SkinId `
        --display-name $DisplayName `
        --license $License `
        --redistribution $Redistribution `
        --tile-width $TileWidth `
        --tile-height $TileHeight
    if ($LASTEXITCODE -ne 0) { throw "skin import failed" }
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
