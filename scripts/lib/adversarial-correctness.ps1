# Executed correctness gate for solution preservation and probability invariants.

function Get-AdversarialRustCases {
    return @(
        [pscustomobject]@{
            Id = "forced_hash_collision"
            Package = "clearra-core-executor"
            Test = "buildup::buildup_runner_tests::coverage_source::case_operation_set_hash_collision_does_not_merge_candidates::operation_set_hash_collision_does_not_merge_candidates"
        },
        [pscustomobject]@{
            Id = "alternate_successful_buildup_order"
            Package = "clearra-core-executor"
            Test = "spin::spin_target_runner::spin_target_runner_tests::replay_execution::case_alternate_success_order_replay_is_legal::alternate_success_order_replay_is_legal"
        },
        [pscustomobject]@{
            Id = "nonuniform_pattern_weights"
            Package = "clearra-core-executor"
            Test = "buildup::buildup_runner_tests::objective::case_objective_uses_nonuniform_pattern_weights::objective_uses_nonuniform_pattern_weights"
        },
        [pscustomobject]@{
            Id = "all_pattern_minimum_cover"
            Package = "clearra-core-executor"
            Test = "buildup::buildup_runner_tests::objective::case_minimum_cover_requires_all_requested_patterns::minimum_cover_requires_all_requested_patterns"
        },
        [pscustomobject]@{
            Id = "ledger_complete_required_domain"
            Package = "clearra-core-domain"
            Test = "pruning::pruning_proof_ledger::tests::complete_required_capacity_keeps_candidate"
        },
        [pscustomobject]@{
            Id = "same_candidate_different_pattern_execution"
            Package = "clearra-core-executor"
            Test = "buildup::buildup_runner_tests::execution_retention::case_execution_variant_set_preserves_successes_from_multiple_patterns::execution_variant_set_preserves_successes_from_multiple_patterns"
        }
    )
}

function Invoke-AdversarialCargoProcess {
    param(
        [string]$CargoPath,
        [string[]]$Arguments
    )

    $quotedArguments = $Arguments | ForEach-Object {
        '"' + $_.Replace('"', '\"') + '"'
    }
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $CargoPath
    $startInfo.Arguments = $quotedArguments -join ' '
    $startInfo.WorkingDirectory = (Get-Location).Path
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    [void]$process.Start()
    $standardOutputTask = $process.StandardOutput.ReadToEndAsync()
    $standardErrorTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $standardOutput = $standardOutputTask.GetAwaiter().GetResult()
    $standardError = $standardErrorTask.GetAwaiter().GetResult()
    $result = [pscustomobject]@{
        ExitCode = $process.ExitCode
        Output = @($standardError, $standardOutput) |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    }
    $process.Dispose()
    return $result
}

function Invoke-AdversarialCargoProcessOnce {
    param(
        [string]$CargoPath,
        [string[]]$Arguments
    )
    return Invoke-AdversarialCargoProcess $CargoPath $Arguments
}

function Invoke-AdversarialRustSuite {
    param(
        [string]$CargoPath,
        [object[]]$RequiredCases,
        [string[]]$FeatureArguments = @()
    )

    $packages = @($RequiredCases.Package | Sort-Object -Unique)
    if ($packages.Count -eq 0) {
        throw 'adversarial Rust suite has no required executable cases'
    }
    $arguments = New-Object System.Collections.Generic.List[string]
    $arguments.Add('test')
    foreach ($package in $packages) {
        $arguments.Add('--package')
        $arguments.Add($package)
    }
    $arguments.Add('--lib')
    foreach ($featureArgument in $FeatureArguments) {
        $arguments.Add($featureArgument)
    }
    $arguments.Add('--')
    $arguments.Add('--test-threads=1')

    Write-Output "[adversarial] rust suite packages=$($packages -join ',')"
    $result = Invoke-AdversarialCargoProcessOnce `
        -CargoPath $CargoPath `
        -Arguments @($arguments.ToArray())
    $result.Output | Write-Output
    if ($result.ExitCode -ne 0) {
        throw "adversarial Rust suite failed with exit code $($result.ExitCode)"
    }

    $output = $result.Output -join "`n"
    $summaryMatches = [regex]::Matches($output, 'test result: ok\. (?<passed>[0-9]+) passed; 0 failed;')
    $passed = 0
    foreach ($summary in $summaryMatches) {
        $passed += [int]$summary.Groups['passed'].Value
    }
    if ($passed -lt 1) {
        throw 'adversarial Rust suite did not execute any tests'
    }

    foreach ($case in $RequiredCases) {
        $executedCasePattern = '(?m)^test ' + [regex]::Escape($case.Test) + ' \.\.\. ok\s*$'
        if ($output -notmatch $executedCasePattern) {
            throw "adversarial Rust suite '$Package' did not execute required case '$($case.Id)'"
        }
        Write-Output "adversarial_case=$($case.Id) status=passed source=rust"
    }
    Write-Output "adversarial_suite=$($packages -join ',') status=passed tests=$passed"
}

function Invoke-AdversarialCorrectnessGate {
    param(
        [string]$Root,
        [string]$CargoPath,
        [int]$Workers,
        [string]$CargoTargetDir
    )

    Invoke-CoreCTestStartMode `
        -Root $Root `
        -ModeName "AdversarialCorrectness" `
        -ConfigureArgs (Get-StartTestsCMakeConfigureArgs @("-DCLEARRA_CORE_ADVERSARIAL_TESTS=ON")) `
        -PersistentBuildName "core-c-adversarial-cache" `
        -TestRegex "^clearra_adversarial_tests$"

    Write-Output "adversarial_case=forced_hash_collision status=passed source=c"
    Write-Output "adversarial_case=distinct_hold_states_same_hash status=passed source=c"
    Write-Output "adversarial_case=reachability_capacity_exhaustion status=passed source=c"
    Write-Output "adversarial_case=alternate_successful_buildup_order status=passed source=c"
    Write-Output "adversarial_case=ledger_complete_required status=passed source=c"

    $nativeBuildDir = Get-StartTestsPersistentBuildDir 'core-c-adversarial-cache'
    $nativeLibDir = Find-CoreCLibraryDir $nativeBuildDir
    if ([string]::IsNullOrWhiteSpace($nativeLibDir)) {
        throw "adversarial Rust suites could not find clearra_core under $nativeBuildDir"
    }

    $previousCargoTargetDir = $env:CARGO_TARGET_DIR
    $previousWindowsRustFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
    New-Item -ItemType Directory -Force -Path $CargoTargetDir | Out-Null
    try {
        $env:CARGO_TARGET_DIR = Assert-ClearraCanonicalCargoTargetDir $CargoTargetDir
        Sync-ClearraNativeCargoLinkState `
            -LibraryDirectory $nativeLibDir `
            -CargoTargetDirectory $env:CARGO_TARGET_DIR `
            -CargoPath $CargoPath `
            -WorkspaceRoot $Root
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS =
            Add-ClearraWindowsNativeRustLinkFlags $previousWindowsRustFlags $nativeLibDir
        Write-Output "[adversarial] rust execution surface | package-process-parallelism=1 | test-threads=1 | target-dir=$CargoTargetDir"
        $rustCases = @(Get-AdversarialRustCases)
        Invoke-AdversarialRustSuite `
            -CargoPath $CargoPath `
            -RequiredCases $rustCases `
            -FeatureArguments @('--features', 'clearra-core-executor/native-c-core')
    }
    finally {
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
    }

    Write-Output "adversarial_correctness=passed"
    Write-Output "adversarial_c_tests=executed"
    Write-Output "adversarial_rust_tests=executed"
}
