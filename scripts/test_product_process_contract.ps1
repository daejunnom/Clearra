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
. (Join-Path $RepositoryRoot 'scripts/lib/progress/native_progress_runner.ps1')

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
    New-Item -ItemType Directory -Force -Path (Join-Path $testRoot 'scripts') | Out-Null
    Set-Content -LiteralPath $script:TestExePath -Value 'test executable placeholder'
    Set-Content `
        -LiteralPath (Join-Path $testRoot 'scripts/product-e2e.ps1') `
        -Value "Write-Output 'product_e2e_fixture=passed'"

    function Write-ClearraProgressLine {
        param($Scope, [string]$Label)
    }

    $nativePowerShellCommand = Get-Command 'pwsh' -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($null -eq $nativePowerShellCommand) {
        $nativePowerShellCommand = Get-Command 'powershell' -ErrorAction Stop |
            Select-Object -First 1
    }
    $nativeCaptureScope = [pscustomobject]@{ Name = 'native-capture-contract' }
    $nativeSuccess = Invoke-NativeWithProgress `
        -Scope $nativeCaptureScope `
        -Label 'native stderr success probe' `
        -FileName $nativePowerShellCommand.Name `
        -Arguments @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-Command',
            "[Console]::Out.WriteLine('native-stdout-success'); [Console]::Error.WriteLine('native-stderr-success'); exit 0"
        )
    Assert-ProductProcessCondition `
        ($nativeSuccess.ExitCode -eq 0) `
        'native_capture_keeps_stderr_with_zero_exit_successful'
    Assert-ProductProcessCondition `
        ($nativeSuccess.Output.Contains('native-stdout-success') -and
            $nativeSuccess.Output.Contains('native-stderr-success')) `
        'native_capture_preserves_stdout_and_stderr_on_success'

    if ($env:OS -eq 'Windows_NT') {
        $nativeCmdPath = Join-Path $testRoot 'native-stderr-success.cmd'
        Set-Content -LiteralPath $nativeCmdPath -Encoding Ascii -Value @(
            '@echo off',
            'echo native-cmd-stdout-success',
            'echo native-cmd-stderr-success 1>&2',
            'exit /b 0'
        )
        $nativeCmdSuccess = Invoke-NativeWithProgress `
            -Scope $nativeCaptureScope `
            -Label 'native cmd stderr success probe' `
            -FileName $nativeCmdPath
        Assert-ProductProcessCondition `
            ($nativeCmdSuccess.ExitCode -eq 0 -and
                $nativeCmdSuccess.Output.Contains('native-cmd-stdout-success') -and
                $nativeCmdSuccess.Output.Contains('native-cmd-stderr-success')) `
            'native_capture_launches_windows_cmd_and_keeps_zero_exit_stderr_successful'
    }

    $nativeFailure = Invoke-NativeWithProgress `
        -Scope $nativeCaptureScope `
        -Label 'native stderr failure probe' `
        -FileName $nativePowerShellCommand.Name `
        -Arguments @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-Command',
            "[Console]::Out.WriteLine('native-stdout-failure'); [Console]::Error.WriteLine('native-stderr-failure'); exit 23"
        )
    Assert-ProductProcessCondition `
        ($nativeFailure.ExitCode -eq 23) `
        'native_capture_preserves_nonzero_exit_status'
    Assert-ProductProcessCondition `
        ($nativeFailure.Output.Contains('native-stdout-failure') -and
            $nativeFailure.Output.Contains('native-stderr-failure')) `
        'native_capture_preserves_stdout_and_stderr_on_failure'

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
        $scope = [pscustomobject]@{
            Name = $Name
            Total = $Total
            Done = 0
            Running = 0
            Pending = $Total
            Failed = 0
            VerboseLog = $VerboseLog.IsPresent
        }
        if ($Name -eq 'terminal-supply-product') {
            $script:TerminalSupplyScope = $scope
        }
        return $scope
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
        $Scope.Running = 1
        $Scope.Pending = [Math]::Max(0, $Scope.Total - $Scope.Done - 1)
        try {
            & $Body | Out-Null
            $Scope.Done += 1
            $Scope.Running = 0
            $Scope.Pending = [Math]::Max(0, $Scope.Total - $Scope.Done)
        } catch {
            $Scope.Failed += 1
            $Scope.Running = 0
            $Scope.Pending = [Math]::Max(0, $Scope.Total - $Scope.Done - 1)
            throw
        }
    }
    $script:NativeProgressCalls = [System.Collections.Generic.List[object]]::new()
    $script:NativeProgressFailureLabel = ''
    function Invoke-NativeWithProgress {
        param($Scope, [string]$Label, [string]$FileName, [object[]]$Arguments)
        $script:NativeProgressCalls.Add([pscustomobject]@{
            Label = $Label
            FileName = $FileName
            Arguments = @($Arguments)
        })
        if ($Label -eq $script:NativeProgressFailureLabel) {
            return [pscustomobject]@{ ExitCode = 19; Output = 'native-stderr-failure' }
        }
        return [pscustomobject]@{ ExitCode = 0; Output = 'native-stderr-success' }
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
    $script:ExecutionSurface = 'Trusted'
    $script:OutputExcerptLines = 10
    $script:ReportPath = ''
    $script:ProductE2EProgressScope = [pscustomobject]@{ Name = 'product-e2e' }
    $script:ProductE2ECurrentCaseName = 'contract probe'
    $script:TerminalSupplyScope = $null

    function Assert-ClearraTrustedExecutionSurface {
        param([string]$Surface, [string]$Label)
    }

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

    $nativeCallCountBeforeBuiltProduct = $script:NativeProgressCalls.Count
    $builtProductOutput = @(Invoke-ProductE2EBuiltTask $testRoot)
    $builtProductCalls = @($script:NativeProgressCalls |
        Select-Object -Skip $nativeCallCountBeforeBuiltProduct)
    $terminalSupplyCalls = @($builtProductCalls | Where-Object {
        $_.Label -in @(
            'npm build ctk3',
            'Discord terminal-supply product probe',
            'UI terminal-supply product probe'
        )
    })
    Assert-ProductProcessCondition `
        ($terminalSupplyCalls.Count -eq 3) `
        'built_product_routes_all_terminal_supply_commands_through_native_capture'
    Assert-ProductProcessCondition `
        ($builtProductOutput -contains 'native-stderr-success') `
        'built_product_accepts_captured_stderr_when_exit_status_is_zero'
    Assert-ProductProcessCondition `
        ($null -ne $script:TerminalSupplyScope -and
            $script:TerminalSupplyScope.Done -eq 3 -and
            $script:TerminalSupplyScope.Running -eq 0 -and
            $script:TerminalSupplyScope.Pending -eq 0 -and
            $script:TerminalSupplyScope.Failed -eq 0) `
        'built_product_terminal_supply_progress_closes_three_of_three_cases'

    $script:NativeProgressFailureLabel = 'npm build ctk3'
    $capturedBuildFailure = ''
    try {
        Invoke-ProductE2EBuiltTask $testRoot | Out-Null
    } catch {
        $capturedBuildFailure = $_.Exception.Message
    } finally {
        $script:NativeProgressFailureLabel = ''
    }
    Assert-ProductProcessCondition `
        ($capturedBuildFailure.Contains('exit 19') -and
            $capturedBuildFailure.Contains('native-stderr-failure')) `
        'built_product_reports_captured_output_only_for_nonzero_exit'
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
