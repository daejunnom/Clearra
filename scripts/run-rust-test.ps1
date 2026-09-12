param(
    [Parameter(Mandatory)]
    [string]$Package,

    [switch]$Lib,
    [string]$Test,
    [string[]]$Features = @(),
    [string]$Filter,
    [switch]$NoRun,

    [string]$CargoPath = "cargo",
    [string]$CargoTargetDir,
    [int]$OutputExcerptLines = 60,
    [string]$ExecutionSurface = "",
    [switch]$VerboseLog
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
. (Join-Path $PSScriptRoot "lib/progress.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-path-helpers.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")
$Root = Resolve-ClearraBuildSourceRoot
Assert-ClearraBuildEnvironmentBeforeMutation $Root
Assert-ClearraTrustedExecutionSurface $ExecutionSurface "targeted Rust test"
function Get-ClearraRustTestTargetDir {
    param([string]$RequestedTargetDir)

    if (-not [string]::IsNullOrWhiteSpace($RequestedTargetDir)) {
        return (Assert-ClearraCanonicalCargoTargetDir $RequestedTargetDir)
    }

    return (Get-ClearraCargoTargetDir)
}function Assert-ClearraRustTestTargetDir {
    param([string]$TargetDir)

    Assert-ClearraCanonicalCargoTargetDir $TargetDir | Out-Null
}function Get-OutputExcerpt {
    param([string]$Output, [int]$LineLimit)

    if ([string]::IsNullOrWhiteSpace($Output)) {
        return ""
    }

    return (($Output -split "`r?`n" |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            Select-Object -Last ([Math]::Max(1, $LineLimit))) -join "`n")
}
function Invoke-SerializedRustTestOnce {
    param(
        [object]$Scope,
        [string]$CargoPath,
        [string[]]$Arguments
    )

    return Invoke-NativeWithProgress `
        -Scope $Scope `
        -Label "cargo $($Arguments -join ' ')" `
        -FileName $CargoPath `
        -Arguments $Arguments
}
if (-not $Lib.IsPresent -and [string]::IsNullOrWhiteSpace($Test)) {
    throw "Specify -Lib or -Test <name> so the targeted Rust test surface is explicit."
}
if ($OutputExcerptLines -lt 1) {
    throw "-OutputExcerptLines must be at least 1."
}
if (-not [string]::IsNullOrWhiteSpace($CargoTargetDir)) {
    Assert-ClearraCanonicalCargoTargetDir $CargoTargetDir | Out-Null
}
if (-not [string]::IsNullOrWhiteSpace($Filter) -and $Filter.StartsWith('-')) {
    throw 'A Rust test filter must not be a Cargo option.'
}

$cargoArguments = @("test", "-p", $Package)
if ($Lib.IsPresent) {
    $cargoArguments += "--lib"
}
if (-not [string]::IsNullOrWhiteSpace($Test)) {
    $cargoArguments += @("--test", $Test)
}
if ($Features.Count -gt 0) {
    $cargoArguments += @("--features", ($Features -join ","))
}
if ($NoRun.IsPresent) {
    $cargoArguments += "--no-run"
}
if (-not [string]::IsNullOrWhiteSpace($Filter)) {
    $cargoArguments += $Filter
}
if (-not $NoRun.IsPresent) {
    $cargoArguments += @("--", "--test-threads=1")
}

$scope = New-ClearraProgressScope `
    -Name "rust-test" `
    -Total 1 `
    -Workers 1 `
    -VerboseLog:$VerboseLog.IsPresent

$rustTestLocationPushed = $false
try {
    Ensure-ClearraBuildArtifactCache -RepositoryRoot $Root
    $resolvedCargoTargetDir = Get-ClearraRustTestTargetDir $CargoTargetDir
    Assert-ClearraRustTestTargetDir $resolvedCargoTargetDir
    Push-Location $Root
    $rustTestLocationPushed = $true
    Invoke-ClearraProgressCase `
        -Scope $scope `
        -Name "cargo $($cargoArguments -join ' ')" `
        -Body {
            $result = Invoke-SerializedRustTestOnce `
                -Scope $scope `
                -CargoPath $CargoPath `
                -Arguments $cargoArguments

            if ($VerboseLog.IsPresent -and -not [string]::IsNullOrWhiteSpace($result.Output)) {
                Complete-ClearraProgressLine $scope
                Write-Output $result.Output
            }

            if ($result.ExitCode -ne 0) {
                $excerpt = Get-OutputExcerpt $result.Output $OutputExcerptLines
                throw "Rust test failed with exit $($result.ExitCode): cargo $($cargoArguments -join ' ')`n---- last $OutputExcerptLines output line(s) ----`n$excerpt`n---- end output excerpt ----"
            }
        }

    Complete-ClearraProgressLine $scope
    Write-Output "[rust-test] passed | package=$Package | package-process-parallelism=1 | test-threads=1 | target-dir=$resolvedCargoTargetDir"
    if (Test-ClearraBuildTransactionOwner) { Complete-ClearraBuildTransaction }
}
finally {
    if ($rustTestLocationPushed) { Pop-Location }
    Exit-ClearraBuildArtifactCacheUsage
}
