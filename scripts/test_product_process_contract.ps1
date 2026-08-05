param(
    [string]$RepositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-ProductProcessCondition([bool]$Condition, [string]$CaseName) {
    if (-not $Condition) {
        throw "product process contract test failed: $CaseName"
    }
    Write-Output "product_process_contract_test=$CaseName status=passed"
}

. (Join-Path $RepositoryRoot 'scripts/lib/product-process-surface.ps1')
. (Join-Path $RepositoryRoot 'scripts/lib/product-e2e-run.ps1')

$testRoot = Join-Path `
    ([System.IO.Path]::GetTempPath()) `
    "clearra-product-process-contract-$PID-$([guid]::NewGuid().ToString('N'))"
$script:TestCargoTargetDir = Join-Path $testRoot 'cargo-target'
$script:TestExePath = Join-Path $script:TestCargoTargetDir 'debug/clearra.exe'
$script:TestNativeLibraryDir = Join-Path $testRoot 'native'
$previousCargoTargetDir = $env:CARGO_TARGET_DIR
$previousWindowsRustFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS

try {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $script:TestExePath) | Out-Null
    New-Item -ItemType Directory -Force -Path $script:TestNativeLibraryDir | Out-Null
    Set-Content -LiteralPath $script:TestExePath -Value 'test executable placeholder'

    function Get-ClearraBuiltBinaryPath([string]$Root) {
        return $script:TestExePath
    }
    function Get-ClearraCargoTargetDir {
        return $script:TestCargoTargetDir
    }
    function Get-StartTestsPersistentBuildDir([string]$Name) {
        return $script:TestNativeLibraryDir
    }
    function Get-StartTestsCMakeConfigureArgs([object[]]$AdditionalArgs) {
        return @($AdditionalArgs)
    }
    function Invoke-CoreCBuild {
        param(
            [string]$BuildDir,
            [string]$Configuration,
            [object[]]$ConfigureArgs,
            [int]$BuildWorkers
        )
        return [pscustomobject]@{ Status = 'Passed'; Reason = '' }
    }
    function Find-CoreCLibraryDir([string]$BuildDir) {
        return $script:TestNativeLibraryDir
    }
    function New-ClearraProgressScope {
        param(
            [string]$Name,
            [int]$Total,
            [int]$Workers,
            [switch]$VerboseLog
        )
        return [pscustomobject]@{ Name = $Name; VerboseLog = $VerboseLog.IsPresent }
    }
    function Assert-ClearraCanonicalCargoTargetDir([string]$Path) {
        return $Path
    }
    function Sync-ClearraNativeCargoLinkState {
        param(
            [string]$LibraryDirectory,
            [string]$CargoTargetDirectory,
            [string]$CargoPath,
            [string]$WorkspaceRoot
        )
        Write-Output '[native-link-cache] reused | package=clearra-core-ffi'
    }
    function Add-ClearraWindowsNativeRustLinkFlags(
        [AllowNull()][string]$ExistingFlags,
        [string]$LibraryDirectory
    ) {
        return $ExistingFlags
    }
    function Invoke-ClearraProgressCase {
        param($Scope, [string]$Name, [scriptblock]$Body)
        & $Body | Out-Null
    }
    function Invoke-NativeWithProgress {
        param($Scope, [string]$Label, [string]$FileName, [object[]]$Arguments)
        return [pscustomobject]@{ ExitCode = 0; Output = '{"ok":true}' }
    }
    function Complete-ClearraProgressLine {
        param($Scope)
    }
    function Resolve-ProductE2ENativeLibraryDir {
        return $script:TestNativeLibraryDir
    }

    $script:Workers = 1
    $script:CargoPath = 'cargo'
    $script:VerboseLog = [System.Management.Automation.SwitchParameter]::new($false)
    $script:UseBuiltBinary = [System.Management.Automation.SwitchParameter]::new($false)
    $script:Root = $testRoot
    $script:ProductE2EProgressScope = [pscustomobject]@{ Name = 'product-e2e' }
    $script:ProductE2ECurrentCaseName = 'contract probe'

    $builtBinaryResult = @(Ensure-ClearraBuiltBinary $testRoot)
    Assert-ProductProcessCondition `
        ($builtBinaryResult.Count -eq 1 -and $builtBinaryResult[0] -is [string]) `
        'built_binary_returns_one_scalar_value_when_native_link_sync_reports_status'
    Assert-ProductProcessCondition `
        ($builtBinaryResult[0] -eq $script:TestExePath) `
        'built_binary_returns_only_the_executable_path'

    $env:CARGO_TARGET_DIR = $script:TestCargoTargetDir
    $commandResult = @(Invoke-ProductE2EClearra -CommandArgs @('help'))
    Assert-ProductProcessCondition `
        ($commandResult.Count -eq 1 -and $commandResult[0].GetType().Name -eq 'PSCustomObject') `
        'non_built_product_command_returns_one_result_when_native_link_sync_reports_status'
    Assert-ProductProcessCondition `
        ($commandResult[0].ExitCode -eq 0 -and $commandResult[0].Command -eq 'clearra help') `
        'non_built_product_command_preserves_the_typed_result_contract'
} finally {
    if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
        Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $previousCargoTargetDir
    }
    if ([string]::IsNullOrWhiteSpace($previousWindowsRustFlags)) {
        Remove-Item Env:\CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $previousWindowsRustFlags
    }

    $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    $resolvedTempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    if ($resolvedTestRoot.StartsWith($resolvedTempRoot, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Test-Path -LiteralPath $resolvedTestRoot)) {
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
    }
}
