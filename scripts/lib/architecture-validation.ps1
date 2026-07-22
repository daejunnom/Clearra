$ArchitectureValidationLibRoot = Split-Path -Parent $PSCommandPath
. (Join-Path $ArchitectureValidationLibRoot "architecture-validation-common.ps1")
. (Join-Path $ArchitectureValidationLibRoot "architecture-validation-tasks.ps1")
. (Join-Path $ArchitectureValidationLibRoot "validation-task-runner.ps1")
. (Join-Path $ArchitectureValidationLibRoot "architecture-validation-report.ps1")

function Invoke-ArchitectureValidation {
param(
    [string]$TaskName = "",
    [int]$Workers = [Math]::Max(1, [Environment]::ProcessorCount),
    [switch]$QuietProgress,
    [switch]$ShowWarnings,
    [int]$WarningDetailLimit = 5
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ArchitectureLibraryRoot = Split-Path -Parent $PSCommandPath
$ArchitectureScriptsRoot = Split-Path -Parent $ArchitectureLibraryRoot
$Root = Resolve-Path -LiteralPath (Join-Path $ArchitectureScriptsRoot "..")
$Errors = New-Object System.Collections.Generic.List[string]
$Warnings = New-Object System.Collections.Generic.List[string]
$ValidationStarted = Get-Date

. (Join-Path $ArchitectureLibraryRoot "architecture-validation-repository.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_dependencies.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_product_boundary.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_forbidden_algorithms.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_proof_carrying_pruning_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_app_boundary_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_cli_boundaries.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_test_policy.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_runner_progress_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_security.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_unsafe_boundary.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_release_static_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_no_product_debt.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_rust_ffi_safety_tests_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_probability_invariant_tests_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_product_golden_tests_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_security_regression_tests_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_file_size.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_srp_policy.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_build_system.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_c_core_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_c_core_test_matrix_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_coverage_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_output_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_workspace_dependencies.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_workspace_surface_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_buildup_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_board_backend_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_pipeline.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_packing_backend_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_backend_adapter_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_batch_source_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_expander_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_real_kernel_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_reference_equivalence_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_host_reducer_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_buildup_product_path_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_product_equivalence_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_scheduler_metrics_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_hybrid_scheduler_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gui_host_boundary_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_render_exactness_gate_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_asset_import_security_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_tauri_svelte_desktop_host_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_wasm_command_runtime_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_webgpu_backend_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_job_worker_progress_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_guarded_expansion_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_mvp2_scope_gate_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_mvp3_scope_gate_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_custom_piece_domain_model_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_mixed_supply_generalization_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_generic_operation_candidate_reachability_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_area_multiset_feasibility_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_generic_exact_cover_dlx_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_generic_buildup_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_custom_rule_editor_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_generic_gpu_descriptor_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_custom_skin_theme_editor_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_rule_kick_expansion_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_score_profile_object_model_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_spin_target_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_score_aware_objective_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_setup_raw_metrics_v2_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_path_percent_cover_cli_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_fumen_render_product_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gui_editor_schema_v2_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_packing_strengthening_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_mvp2_acceptance_gate_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_mvp2_acceptance_tests_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_mvp3_acceptance_gate_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_mvp3_acceptance_tests_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_stage_e_safety.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_gpu_stage_f_visibility.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_worker_e2e_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_product_e2e_closure_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_spin_scoring_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_postprocess_pipeline_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_host_runtime_contract.ps1")
. (Join-Path $ArchitectureScriptsRoot "architecture\validate_diagnostics_contract.ps1")

function Invoke-ArchitectureValidationTaskInCurrentRunspace([object]$Task) {
    $taskStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $previousErrors = $Errors
    $previousWarnings = $Warnings
    $taskErrors = New-Object System.Collections.Generic.List[string]
    $taskWarnings = New-Object System.Collections.Generic.List[string]

    try {
        $Errors = $taskErrors
        $Warnings = $taskWarnings
        if ($Task.RequiresWorkspaceDependencyGraph) {
            $taskWorkspaceDependencyGraph = Get-WorkspaceDependencyGraph
            & $Task.FunctionName $taskWorkspaceDependencyGraph
        } else {
            & $Task.FunctionName
        }
    }
    catch {
        $taskErrors.Add("$($Task.Name) failed: $($_.Exception.Message)")
    }
    finally {
        $Errors = $previousErrors
        $Warnings = $previousWarnings
        $taskStopwatch.Stop()
    }

    $status = if ($taskErrors.Count -gt 0) { "Failed" } else { "Passed" }
    return New-ArchitectureValidationTaskResult `
        -Name $Task.Name `
        -Status $status `
        -Errors @($taskErrors.ToArray()) `
        -Warnings @($taskWarnings.ToArray()) `
        -DurationMs ([int64]$taskStopwatch.Elapsed.TotalMilliseconds)
}

function Invoke-ArchitectureValidationTaskByName([string]$RequestedTaskName) {
    $task = @(New-AllArchitectureValidationTasks | Where-Object { $_.Name -eq $RequestedTaskName } | Select-Object -First 1)
    if ($task.Count -eq 0) {
        return New-ArchitectureValidationTaskResult `
            -Name $RequestedTaskName `
            -Status "Failed" `
            -Errors @("unknown architecture validation task '$RequestedTaskName'") `
            -Warnings @() `
            -DurationMs 0
    }

    return Invoke-ArchitectureValidationTaskInCurrentRunspace $task[0]
}

function Merge-ArchitectureValidationTaskResults([object[]]$TaskResults) {
    foreach ($taskResult in $TaskResults) {
        foreach ($errorMessage in @($taskResult.Errors)) {
            $Errors.Add($errorMessage)
        }
        foreach ($warningMessage in @($taskResult.Warnings)) {
            $Warnings.Add($warningMessage)
        }
    }
}

if (-not [string]::IsNullOrWhiteSpace($TaskName)) {
    return Invoke-ArchitectureValidationTaskByName $TaskName
}

$architectureValidationTasks = @(New-CurrentArchitectureValidationTasks)
$taskResults = Invoke-ValidationTaskRunner `
    -Tasks $architectureValidationTasks `
    -ArchitectureValidationScript $PSCommandPath `
    -Workers $Workers `
    -QuietProgress:$QuietProgress.IsPresent
Merge-ArchitectureValidationTaskResults $taskResults

if ($Errors.Count -gt 0) {
    foreach ($errorMessage in $Errors) {
        [Console]::Error.WriteLine("architecture error: $errorMessage")
    }
    return New-ArchitectureValidationResult "Failed" $Errors $Warnings $architectureValidationTasks.Count ((Get-Date) - $ValidationStarted)
}

$result = New-ArchitectureValidationResult "Passed" $Errors $Warnings $architectureValidationTasks.Count ((Get-Date) - $ValidationStarted)
Write-ArchitectureValidationSummary `
    -Result $result `
    -ShowWarnings:$ShowWarnings.IsPresent `
    -WarningDetailLimit $WarningDetailLimit
return $result
}

