# This file is dot-sourced by an architecture validation wrapper.

function Invoke-BackendPolicyFallbackValidation() {
$executionPolicy = Get-RustProductionContents (Read-Text "crates/clearra-pc-graph/src/request/pc_execution_policy.rs")
foreach ($requiredMarker in @(
            "pub enum RequestedSearchBackend",
            "Cpu,",
            "Gpu,",
            "Hybrid,",
            "max_candidates",
            "max_patterns",
            "with_max_candidates",
            "with_max_patterns",
            "requires_gpu",
            "is_frontier_family"
        )) {
        if ($executionPolicy -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 PcExecutionPolicy must own backend policy/budget marker '$requiredMarker'"
        }
    }
$cliParser = Read-Text "crates/clearra-cli/src/args/cli_parser.rs"
foreach ($requiredMarker in @(
            "--backend auto|cpu|gpu|hybrid",
            "--gpu-device auto|N",
            "--allow-backend-fallback",
            "--no-backend-fallback",
            "--max-candidates N",
            "--max-patterns N",
            "--max-memory-mib N"
        )) {
        if ($cliParser -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 CLI help must expose user-facing backend policy option '$requiredMarker'"
        }
    }
$pcParser = Read-Text "crates/clearra-cli/src/args/parse_pc_args.rs"
$scenarioParser = Read-Text "crates/clearra-cli/src/args/parse_pc_scenario_args.rs"
foreach ($requiredMarker in @('"--max-candidates"', '"--max-patterns"')) {
        if ($pcParser -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 pc args parser must pass budget option '$requiredMarker' into args"
        }
        if ($scenarioParser -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 pc-scenario args parser must pass budget option '$requiredMarker' into args"
        }
    }
$executionAssembler = Read-Text "crates/clearra-cli/src/assemble/execution_policy_assembler.rs"
foreach ($requiredMarker in @(
            "ExecutionPolicyInput",
            "RequestedSearchBackend::Gpu",
            "with_max_candidates",
            "with_max_patterns",
            "assembles_m19_user_facing_backend_policy_options",
            "rejects_internal_backend_names_from_cli_surface"
        )) {
        if ($executionAssembler -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 execution policy assembler must lower CLI options into PcExecutionPolicy marker '$requiredMarker'"
        }
    }
$policyValidator = @(
        Read-Text "crates/clearra-validation/src/validators/pc_execution_policy_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/pc_execution_policy_field_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/pc_execution_policy_capability_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/pc_execution_policy_diagnostic_builder.rs"
    ) -join "`n"
$policyValidatorTests = Read-Text "crates/clearra-validation/src/validators/pc_execution_policy_validator_tests.rs"
foreach ($requiredMarker in @(
            "RequestedSearchBackend::Cpu",
            "RequestedSearchBackend::Gpu",
            "RequestedSearchBackend::Hybrid",
            "gpu_backend_capability_is_deferred_to_the_executor",
            "hybrid_backend_is_not_statically_classified_as_fallback",
            "zero_search_limits_select_automatic_demand_growing_budgets",
            "with_max_candidates(0)",
            "with_max_patterns(0)"
        )) {
        if ("$policyValidator`n$policyValidatorTests" -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 validation must block/report backend compatibility marker '$requiredMarker'"
        }
    }
$backendSelector = Read-Text "crates/clearra-core-executor/src/backend/backend_selector.rs"
$backendSelectorTests = Read-Text "crates/clearra-core-executor/src/backend/backend_selector_tests.rs"
$backendTypes = Read-Text "crates/clearra-core-executor/src/backend/backend_types.rs"
$capabilityProvider = Read-Text "crates/clearra-core-executor/src/backend/search_backend_capability_provider.rs"
$nativeCapabilityQuery = Read-Text "crates/clearra-core-ffi/src/gpu/native_search_capability.rs"
$nativeCapabilityBinding = Read-Text "crates/clearra-core-ffi/src/raw/bindings.rs"
$nativeCapabilityImplementation = Read-Text "core-c/src/gpu/gpu_capability.c"
$gpuFailureStateMachine = Read-Text "crates/clearra-core-executor/src/backend/gpu_execution_failure.rs"
$gpuFailureBehavior = Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_contract_behavior/trust_fallback.rs"
$backendRuntimeSurface = @(
    $backendSelector
    $backendSelectorTests
    $backendTypes
    $capabilityProvider
    $nativeCapabilityQuery
    $nativeCapabilityBinding
    $nativeCapabilityImplementation
) -join "`n"
foreach ($requiredMarker in @(
            "RequestedSearchBackend::Cpu",
            "RequestedSearchBackend::Gpu",
            "RequestedSearchBackend::Hybrid",
            "SearchBackendCapabilityProvider",
            "NativeSearchBackendCapabilityProvider",
            "NativeGpuCapabilityQuery::query",
            "clearra_gpu_device_capability_query",
            "GpuDeviceNotFound",
            "GpuKernelUnavailable",
            "max_candidates",
            "gpu_available_selects_gpu",
            "auto_prefers_exact_connected_gpu_without_hardware_assumptions",
            "gpu_device_not_found_falls_back_to_cpu_with_reason",
            "gpu_kernel_unavailable_falls_back_to_cpu_with_reason",
            "no_backend_fallback_returns_error"
        )) {
        if ($backendRuntimeSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 core executor backend selector must own selection/fallback marker '$requiredMarker'"
        }
    }
if ($backendSelector -match 'RequestedSearchBackend::Gpu\s*=>\s*Some\(SearchBackendFallbackReason::GpuFeatureDisabled\)') {
    Add-ArchitectureError "Product backend selector must query SearchBackendCapabilityProvider instead of hardcoding GpuFeatureDisabled"
}
foreach ($requiredMarker in @(
        "pub enum GpuExecutionFailureClass",
        "Unavailable",
        "TransientBeforeCommit",
        "ResourceIncomplete",
        "InvalidRequest",
        "TrustMismatch",
        "FatalInternal",
        "discarded_partial_gpu_result",
        "CpuRerunAfterIncomplete",
        "gpu_backend_no_fallback_returns_error",
        "gpu_worker_fallback_result_carries_reason"
    )) {
    if ("$gpuFailureStateMachine`n$gpuFailureBehavior" -notlike "*$requiredMarker*") {
        Add-ArchitectureError "GPU runtime failure/fallback state machine must own marker '$requiredMarker'"
    }
}
$pcService = Get-PcServiceValidationSurface
$pcBackendReportAdapter = Read-Text "crates/clearra-core-executor/src/service/pc_backend_report_adapter.rs"
$pcOutputSurface = "$pcService`n$pcBackendReportAdapter"
foreach ($requiredMarker in @(
            '"backend_requested"',
            '"backend_selected"',
            '"backend_fallback_reason"',
            '"gpu_confirmed"',
            '"cpu_confirmed"',
            '"candidate_backend"',
            '"buildup_backend"',
            "backend_surface",
            "candidate_backend()",
            "buildup_backend()",
            '"execution_max_candidates"',
            '"execution_max_patterns"',
            '"gpu_device_selected_index"',
            '"gpu_device_selected_name"',
            '"gpu_device_selected_type"',
            '"gpu_device_selected_backend"'
        )) {
        if ($pcOutputSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 PC executor output must expose backend report marker '$requiredMarker'"
        }
    }
$summaryContract = Read-Text "crates/clearra-cli/src/output/summary_render_contract.rs"
$jsonContract = Get-JsonContractValidationSurface
$renderMessage = Read-Text "crates/clearra-output/src/model/render_message.rs"
foreach ($requiredMarker in @(
            "gpu_confirmed",
            "cpu_confirmed",
            "candidate_backend",
            "buildup_backend",
            "execution_max_candidates",
            "execution_max_patterns",
            "pc_backend_report_contract",
            "pc_memory_report_contract",
            "gpu_trust_state",
            "cpu_confirm_required",
            "deterministic_reference_matched",
            "memory_leak_report_clean",
            "memory_pressure_level",
            "pc_contract_exposes_backend_and_memory_reports"
        )) {
        if ("$summaryContract`n$jsonContract`n$renderMessage" -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 output contracts must preserve typed/backend field marker '$requiredMarker'"
        }
    }
$uiExecutionOptions = @(
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/execution_options_schema.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/backend_preset_schema.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/execution_limits_schema.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/execution_options_schema_tests.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "RequestedSearchBackend::Cpu",
            "RequestedSearchBackend::Gpu",
            "RequestedSearchBackend::Hybrid"
        )) {
        if ($uiExecutionOptions -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 UI execution schema must expose backend preset marker '$requiredMarker'"
        }
    }
$processE2E = Read-Text "crates/clearra-cli/tests/process_e2e.rs"
foreach ($requiredMarker in @(
            "process_e2e_m19_backend_policy_reports_fallback_and_backend_split",
            "--backend",
            "gpu",
            "--allow-backend-fallback",
            "--max-candidates",
            "--max-patterns",
            '\"backend_requested\":\"gpu\"',
            '\"backend_selected\":\"cpu\"',
            '\"backend_fallback_reason\":\"gpu_kernel_unavailable\"',
            "expected_candidate_backend",
            "expected_buildup_backend",
            "expected_native_c_core_executed_json"
        )) {
        if ($processE2E -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M19 process E2E must verify backend policy/fallback output marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "M19 Backend Policy and Fallback",
            "auto|cpu|gpu|hybrid",
            "backend_requested",
            "backend_selected",
            "backend_fallback_reason",
            "gpu_confirmed",
            "cpu_confirmed",
            "candidate_backend",
            "buildup_backend",
            "--max-candidates N",
            "--max-patterns N"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M19 backend policy marker '$requiredMarker'"
        }
    }

$artifactArgs = Read-Text "tools/clearra-pc-artifact/src/args.rs"
foreach ($requiredMarker in @(
        'const MEMORY_BOUNDED_INDEX_LIMIT: usize = u32::MAX as usize - 1;',
        'let mut max_candidates = MEMORY_BOUNDED_INDEX_LIMIT;',
        'let mut max_frontier_states = MEMORY_BOUNDED_INDEX_LIMIT;'
    )) {
    if ($artifactArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "full PC artifact runner must use memory-bounded index limit marker '$requiredMarker'"
    }
}
if ($artifactArgs -match '(?m)max_candidates\s*=\s*5_000_000') {
    Add-ArchitectureError "full PC artifact runner must not restore the arbitrary five-million candidate cutoff"
}

$artifactScript = Read-Text "scripts/export-pc-artifact.ps1"
foreach ($requiredMarker in @(
        '[long]$MaxCandidates = 0',
        '[long]$MaxFrontierStates = 0',
        'if ($MaxCandidates -gt 0)',
        'if ($MaxFrontierStates -gt 0)',
        'memory-bounded auto'
    )) {
    if ($artifactScript -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PC artifact PowerShell surface must preserve demand-grown auto limit marker '$requiredMarker'"
    }
}
}



