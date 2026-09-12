param(
    [string]$CargoTargetDirectory,
    [Parameter(Mandatory, ValueFromRemainingArguments)]
    [string[]]$CargoArguments
)

# A whole experimental generation is owned by the common build transaction.
$ErrorActionPreference = 'Stop'
if ($CargoArguments.Count -eq 0 -or
    $CargoArguments[0] -notin @('build', 'test', 'check', 'run', 'rustc', 'clippy') -or
    '--release' -in $CargoArguments -or
    @($CargoArguments | Where-Object { $_ -match '^--profile(?:=|$)' }).Count -gt 0) {
    throw 'This entry point accepts ordinary debug Cargo commands only.'
}
. (Join-Path $PSScriptRoot '../lib/clearra-path-helpers.ps1')
$debugSourceRoot = Resolve-ClearraRoot
if ($CargoTargetDirectory) {
    # This check is pure: an outside target must not trigger even cache setup.
    $expected = Join-Path (Get-ClearraBuildTransactionRoot -RepositoryRoot $debugSourceRoot -Purpose experiment) 'cargo-target'
    if (-not ([IO.Path]::GetFullPath($CargoTargetDirectory)).Equals($expected, (Get-ClearraBuildPathComparison))) {
        throw "Debug Cargo must use the one experimental target: $expected"
    }
}
foreach ($argument in $CargoArguments) {
    if ($argument -eq '--') { break }
    if ($argument -match '^(?:--target-dir|--build-dir|--artifact-dir|--out-dir|--config)(?:=|$)') { throw 'Cargo build path/config overrides are forbidden.' }
}
$debugExitCode = 1
try {
    Ensure-ClearraBuildArtifactCache -RepositoryRoot $debugSourceRoot -Purpose experiment
    & cargo @CargoArguments
    $debugExitCode = $LASTEXITCODE
    if ($debugExitCode -eq 0 -and (Test-ClearraBuildTransactionOwner)) { Complete-ClearraBuildTransaction }
} finally {
    Exit-ClearraBuildArtifactCacheUsage
}
exit $debugExitCode
