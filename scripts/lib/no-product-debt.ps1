# Executed release gate for Finish-or-Remove product debt.

function Invoke-NoProductDebtStaticGate {
    param(
        [string]$Root,
        [string]$PowerShellPath
    )

    Write-Output '[no-product-debt] static product source validation'
    $result = Invoke-AdversarialCargoProcess `
        -CargoPath $PowerShellPath `
        -Arguments @(
            '-NoProfile',
            '-File', (Join-Path $Root 'scripts/validate_architecture.ps1')
        )
    $result.Output | Write-Output
    if ($result.ExitCode -ne 0) {
        throw "NoProductDebt static validation failed with exit code $($result.ExitCode)"
    }
    Write-Output 'no_product_debt_static=passed allowlist=test-fixtures,docs-history'
}

function Invoke-NoProductDebtRustCase {
    param(
        [string]$CargoPath,
        [string]$Package,
        [string[]]$TargetArguments,
        [string]$Filter,
        [string]$RequiredTest,
        [string]$EvidenceId
    )

    $arguments = @('test', '--package', $Package) +
        @($TargetArguments) +
        @($Filter, '--', '--test-threads=1')
    $result = Invoke-AdversarialCargoProcessOnce `
        -CargoPath $CargoPath `
        -Arguments $arguments
    $result.Output | Write-Output
    if ($result.ExitCode -ne 0) {
        throw "NoProductDebt evidence '$EvidenceId' failed with exit code $($result.ExitCode)"
    }

    $output = $result.Output -join "`n"
    $testPattern = '(?m)^test .*' + [regex]::Escape($RequiredTest) + ' \.\.\. ok\s*$'
    if ($output -notmatch $testPattern) {
        throw "NoProductDebt evidence '$EvidenceId' did not execute required test '$RequiredTest'"
    }
    $summary = [regex]::Match($output, 'test result: ok\. (?<passed>[0-9]+) passed; 0 failed;')
    if (-not $summary.Success -or [int]$summary.Groups['passed'].Value -lt 1) {
        throw "NoProductDebt evidence '$EvidenceId' did not execute a passing Rust test"
    }
    Write-Output "no_product_debt_evidence=$EvidenceId status=passed source=rust-test"
}

function Invoke-NoProductDebtMaxScoreProbe {
    param(
        [string]$Root,
        [string]$CargoPath,
        [int]$Workers
    )

    Write-Output '[no-product-debt] native max-score library product probe'
    $libDir = Get-NoProductDebtNativeCoreLibraryDir $Workers

    $previousWindowsRustFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
    try {
        Sync-ClearraNativeCargoLinkState `
            -LibraryDirectory $libDir `
            -CargoTargetDirectory (Get-ClearraCargoTargetDir) `
            -CargoPath $CargoPath `
            -WorkspaceRoot $Root
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS =
            Add-ClearraWindowsNativeRustLinkFlags $previousWindowsRustFlags $libDir
        Invoke-NoProductDebtRustCase $CargoPath 'clearra-cli' `
            @('--features', 'native-c-core,webgpu-search', '--lib') `
            'library_route_max_score_materializes_profile_specific_nonzero_matrix' `
            'library_route_max_score_materializes_profile_specific_nonzero_matrix' `
            'max_score_nonzero_profile_matrix'
    }
    finally {
        if ([string]::IsNullOrWhiteSpace($previousWindowsRustFlags)) {
            Remove-Item Env:\CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $previousWindowsRustFlags
        }
    }
    Write-Output 'no_product_debt_execution_surface=max-score route=clearra_cli::run_with_args process-launch=False'
}

function Get-NoProductDebtNativeCoreLibraryDir {
    param([int]$Workers)

    $buildDir = Get-StartTestsPersistentBuildDir 'core-c-library-cache'
    $coreBuild = Invoke-CoreCBuild `
        -BuildDir $buildDir `
        -Configuration 'Debug' `
        -ConfigureArgs (Get-StartTestsCMakeConfigureArgs @('-DBUILD_TESTING=OFF')) `
        -BuildWorkers ([Math]::Max(1, $Workers))
    if ($coreBuild.Status -ne 'Passed') {
        throw "NoProductDebt could not build native C core: $($coreBuild.Reason)"
    }
    $libDir = Find-CoreCLibraryDir $buildDir
    if ([string]::IsNullOrWhiteSpace($libDir)) {
        throw "NoProductDebt could not find clearra_core under $buildDir"
    }
    return $libDir
}

function Invoke-NoProductDebtDesktopProbe {
    param([string]$CargoPath)

    Write-Output '[no-product-debt] WASM CPU desktop AppRequest probe'
    Invoke-NoProductDebtRustCase $CargoPath 'clearra-gui-host' `
        @('--features', 'wasm-cpu-runtime,webgpu-search', '--lib') `
        'tauri_command_calls_clearra_gui_host_only' `
        'tauri_command_calls_clearra_gui_host_only' `
        'desktop_real_app_request'
    Write-Output 'no_product_debt_execution_surface=desktop route=tauri-gui-host-app-wasm-cpu process-launch=False'
}

function Invoke-NoProductDebtHoldLanguageProofProbe {
    param(
        [string]$Root,
        [string]$CargoPath,
        [string]$CargoTargetDir
    )

    $probeRoot = New-TransientBuildDir 'clearra-no-product-debt-proof'
    $sourceRoot = Join-Path $probeRoot 'src'
    New-Item -ItemType Directory -Force -Path $sourceRoot | Out-Null
    $dependencyPath = (Join-Path $Root 'crates/clearra-core-executor').Replace('\', '/')
    try {
        @"
[package]
name = "clearra-no-product-debt-proof-probe"
version = "0.0.0"
edition = "2021"

[workspace]

[dependencies]
clearra-core-executor = { path = "$dependencyPath" }
"@ | Set-Content -LiteralPath (Join-Path $probeRoot 'Cargo.toml') -Encoding utf8
        @'
use clearra_core_executor::pruning::AuthorizedPrune;

fn main() {
    let _ = core::mem::size_of::<AuthorizedPrune>();
}
'@ | Set-Content -LiteralPath (Join-Path $sourceRoot 'main.rs') -Encoding utf8

        $result = Invoke-AdversarialCargoProcess `
            -CargoPath $CargoPath `
            -Arguments @(
                'check', '--quiet', '--manifest-path', (Join-Path $probeRoot 'Cargo.toml')
            )
        $output = $result.Output -join "`n"
        if ($result.ExitCode -eq 0) {
            throw 'NoProductDebt independent-proof probe constructed HoldLanguageEmpty pruning without an engine proof'
        }
        if ($output -notmatch '(could not find `pruning`|unresolved import)' -or
            $output -notmatch 'AuthorizedPrune') {
            $result.Output | Write-Output
            throw 'NoProductDebt independent-proof probe failed for an unrelated compiler reason'
        }
        Write-Output 'no_product_debt_evidence=hold_language_empty_requires_independent_proof status=passed source=compile-fail-probe'
    }
    finally {
        Remove-TransientBuildDir $probeRoot
    }
}

function Invoke-NoProductDebtGate {
    param(
        [string]$Root,
        [string]$CargoPath,
        [string]$PowerShellPath,
        [string]$CargoTargetDir,
        [int]$Workers = 1
    )

    Assert-ClearraRepositoryArtifactPolicy $Root
    & (Join-Path $Root 'scripts/test_artifact_path_policy.ps1') -RepositoryRoot $Root
    if (-not $?) {
        throw 'NoProductDebt artifact path policy tests failed'
    }
    Write-Output 'no_product_debt_evidence=artifact_path_policy status=passed source=powershell-test'

    & (Join-Path $Root 'scripts/test_product_process_contract.ps1') -RepositoryRoot $Root
    if (-not $?) {
        throw 'NoProductDebt product process contract tests failed'
    }
    Write-Output 'no_product_debt_evidence=product_process_contract status=passed source=powershell-test'

    Invoke-NoProductDebtStaticGate $Root $PowerShellPath

    $previousCargoTargetDir = $env:CARGO_TARGET_DIR
    New-Item -ItemType Directory -Force -Path $CargoTargetDir | Out-Null
    try {
        $env:CARGO_TARGET_DIR = Assert-ClearraCanonicalCargoTargetDir $CargoTargetDir
        Invoke-NoProductDebtRustCase $CargoPath 'clearra-core-executor' @('--lib') `
            'default_build_reports_native_runtime_unavailable' `
            'default_build_reports_native_runtime_unavailable' `
            'native_unavailable_explicit_error'
        Invoke-NoProductDebtRustCase $CargoPath 'clearra-wasm' @('--lib') `
            'wasm_output_keys_are_not_localized' `
            'wasm_output_keys_are_not_localized' `
            'wasm_real_app_response'
        Invoke-NoProductDebtRustCase $CargoPath 'clearra-core-domain' @('--lib') `
            'complete_required_capacity_keeps_candidate' `
            'complete_required_capacity_keeps_candidate' `
            'complete_required_keeps_candidate'
        Invoke-NoProductDebtRustCase $CargoPath 'clearra-render' @('--lib') `
            'png_board_render_golden' `
            'png_board_render_golden' `
            'renderer_png_artifact'
        Invoke-NoProductDebtRustCase $CargoPath 'clearra-render' @('--lib') `
            'gif_timeline_render_golden' `
            'gif_timeline_render_golden' `
            'renderer_gif_artifact'

        Invoke-NoProductDebtHoldLanguageProofProbe $Root $CargoPath $CargoTargetDir
        Invoke-NoProductDebtDesktopProbe $CargoPath
        Invoke-NoProductDebtMaxScoreProbe $Root $CargoPath $Workers
    }
    finally {
        if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
            Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousCargoTargetDir
        }
    }

    Write-Output 'no_product_debt=passed'
}
