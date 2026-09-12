param(
    [string]$SourceRoot = (Join-Path $PSScriptRoot '../..'),
    [ValidateSet('experiment', 'product')][string]$Purpose = 'experiment',
    [Parameter(Mandatory)][string]$Command,
    [string]$ArgumentsJson = '[]'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '../lib/clearra-path-helpers.ps1')
$source = [IO.Path]::GetFullPath($SourceRoot)
if (-not (Test-Path -LiteralPath (Join-Path $source 'Cargo.toml') -PathType Leaf)) { throw 'A Clearra source root is required.' }
$arguments = @(ConvertFrom-Json $ArgumentsJson)
if (@($arguments | Where-Object { $_ -isnot [string] }).Count -ne 0 -or !$ArgumentsJson.TrimStart().StartsWith('[')) { throw 'Build arguments must be a JSON string array.' }
if ([IO.Path]::GetFileNameWithoutExtension($Command) -eq 'cargo') {
    foreach ($argument in $arguments) {
        if ($argument -eq '--') { break }
        if ($argument -match '^(?:--target-dir|--build-dir|--artifact-dir|--out-dir|--config)(?:=|$)') { throw 'Cargo output/config overrides are forbidden; the managed owner chooses the build root.' }
    }
}
$exitCode = 1
$locationPushed = $false
try {
    Ensure-ClearraBuildArtifactCache -RepositoryRoot $source -Purpose $Purpose
    Push-Location $source
    $locationPushed = $true
    $global:LASTEXITCODE = 0
    & $Command @arguments
    $exitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
    if ($exitCode -eq 0 -and (Test-ClearraBuildTransactionOwner)) { Complete-ClearraBuildTransaction }
} finally {
    if ($locationPushed) { Pop-Location }
    Exit-ClearraBuildArtifactCacheUsage
}
exit $exitCode
