param(
    [Parameter(Mandatory)]
    [string]$CargoTargetDirectory,
    [Parameter(Mandatory, ValueFromRemainingArguments)]
    [string[]]$CargoArguments
)

# Ad-hoc diagnostics must use the same bounded debug retention as the normal
# progress runner, rather than accumulating an untracked series of Cargo caches.
$ErrorActionPreference = 'Stop'
if ($CargoArguments.Count -eq 0 -or
    $CargoArguments[0] -notin @('build', 'test', 'check', 'run', 'rustc', 'clippy') -or
    '--release' -in $CargoArguments -or
    @($CargoArguments | Where-Object { $_ -match '^--profile(?:=|$)' }).Count -gt 0) {
    throw 'This entry point accepts ordinary debug Cargo commands only.'
}
$debugTargetRoot = [IO.Path]::GetFullPath($CargoTargetDirectory)
if ([IO.Path]::GetFileName($debugTargetRoot.TrimEnd('\', '/')) -notmatch '^(?:cargo-)?target(?:-[A-Za-z0-9_-]+)?$') {
    throw 'An explicit Cargo target directory is required.'
}
$previousTargetDirectory = $env:CARGO_TARGET_DIR
$previousIncrementalPolicy = $env:CARGO_INCREMENTAL
$debugExitCode = 1
try {
    $env:CARGO_TARGET_DIR = $debugTargetRoot
    $env:CARGO_INCREMENTAL = '0'
    & cargo @CargoArguments
    $debugExitCode = $LASTEXITCODE
} finally {
    if ($null -eq $previousTargetDirectory) { Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue }
    else { $env:CARGO_TARGET_DIR = $previousTargetDirectory }
    if ($null -eq $previousIncrementalPolicy) { Remove-Item Env:\CARGO_INCREMENTAL -ErrorAction SilentlyContinue }
    else { $env:CARGO_INCREMENTAL = $previousIncrementalPolicy }
    try {
        $retention = & (Join-Path $PSScriptRoot 'retain-clearra-debug-builds.ps1') -CargoTargetDirectory $debugTargetRoot -Apply
        Write-Output "debug_retention=$($retention.Status) deleted=$($retention.DeletedCount) freed_bytes=$($retention.FreedBytes)"
    } catch { Write-Warning "Debug retention skipped: $($_.Exception.Message)" }
}
exit $debugExitCode
