# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Keep functions side-effect free at load time; validation runs only when invoked.

function Invoke-DiagnosticsSecurityGateValidation() {
    
foreach ($requiredPath in @(
            "crates/clearra-validation/src/validators/core_security_gate.rs",
            "crates/clearra-validation/src/validators/core_security_gate_tests.rs",
            "crates/clearra-validation/src/diagnostic/diagnostic_code.rs",
            "crates/clearra-validation/src/diagnostic/gpu_worker_diagnostic.rs",
            "crates/clearra-validation/src/validators/security_diagnostic_gate.rs",
            "crates/clearra-validation/src/validators/security_diagnostic_gate_tests.rs",
            "crates/clearra-core-ffi/src/diagnostics/mod.rs",
            "crates/clearra-core-executor/src/diagnostics/mod.rs",
            "crates/clearra-output/src/json/diagnostic_json_contract.rs",
            "crates/clearra-cli/src/output/diagnostic_printer.rs",
            "tests/golden/diagnostics/security_diagnostic_gate.json",
            "tests/golden/diagnostics/security_diagnostic_gate.txt",
            "crates/clearra-validation/src/validators/pc_execution_policy_validator.rs",
            "crates/clearra-coverage/src/matrix/coverage_row_bridge.rs",
            "crates/clearra-core-executor/src/service/pc_service.rs"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M29 diagnostics/security gate requires $requiredPath"
        }
    }
$validationCargo = Read-Text "crates/clearra-validation/Cargo.toml"
if (-not (Test-DependencyLine $validationCargo "clearra-core-ffi")) {
        Add-ArchitectureError "M29 validation diagnostics must depend on clearra-core-ffi to map C status and ABI evidence"
    }
$diagnosticCode = @(
        Read-Text "crates/clearra-validation/src/diagnostic/diagnostic_code.rs"
        Read-Text "crates/clearra-validation/src/diagnostic/diagnostic_code_string.rs"
        Read-Text "crates/clearra-validation/src/diagnostic/diagnostic_severity_mapping.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "E_CORE_ABI_VERSION_MISMATCH",
            "E_CORE_PACKING_FAILED",
            "E_CORE_BUILDUP_FAILED",
            "E_C_MEMORY_SCOPE_INVALID",
            "E_C_MEMORY_LEAK_DETECTED",
            "E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE",
            "E_CORE_MEMORY_SCOPE_INVALID",
            "E_CORE_MEMORY_LEAK_DETECTED",
            "E_CORE_FFI_BUFFER_BOUNDS",
            "E_CORE_INVALID_NATIVE_VIEW",
            "E_KICK_EVIDENCE_BUFFER_EXHAUSTED",
            "E_GPU_WORKER_MISSING_MEMORY_TICKET",
            "E_GPU_FENCE_EPOCH_MISSING",
            "E_GPU_UNCONFIRMED_PROBABILITY_SOURCE",
            "E_BACKEND_GPU_UNAVAILABLE",
            "E_GPU_WORKER_TRUST_MISMATCH",
            "W_BACKEND_FALLBACK_USED",
            "W_TRACE_RETENTION_TRUNCATED",
            "W_OBSERVED_QUEUE_PROBABILITY_INCOMPLETE",
            "E_RENDER_RUNTIME_SVG_FORBIDDEN",
            "E_RENDER_ASSET_PROVENANCE_MISSING",
            "E_GUI_SUBPROCESS_FORBIDDEN",
            "E_FRONTEND_TYPED_REQUEST_REQUIRED",
            "W_GPU_BACKEND_FALLBACK",
            "W_GPU_DEVICE_UNAVAILABLE",
            "W_HYBRID_BACKPRESSURE_ACTIVE",
            "W_GPU_RESULT_CPU_CONFIRM_REQUIRED",
            "E_PACKING_CANDIDATE_USED_AS_SOLUTION"
        )) {
        if ($diagnosticCode -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M29 diagnostic code enum must expose marker '$requiredMarker'"
        }
    }
$securityDiagnosticGate = @(
        Read-Text "crates/clearra-validation/src/validators/security_diagnostic_gate.rs"
        Read-Text "crates/clearra-validation/src/validators/security_diagnostic_gate_tests.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "SecurityDiagnosticGate",
            "memory_context_double_release",
            "memory_scope_status",
            "memory_leak_detected",
            "ffi_buffer_bounds",
            "kick_evidence_buffer_exhausted",
            "gpu_missing_memory_ticket",
            "gpu_fence_epoch_missing",
            "gpu_unconfirmed_probability_source",
            "render_runtime_svg_forbidden",
            "render_asset_provenance_missing",
            "gui_subprocess_forbidden",
            "frontend_typed_request_required",
            "security_diagnostic_gate_maps_all_s_stage_errors",
            "security_errors_are_not_downgraded_to_warnings"
        )) {
        if ($securityDiagnosticGate -notlike "*$requiredMarker*") {
            Add-ArchitectureError "S6 security diagnostic gate must map marker '$requiredMarker'"
        }
    }
$coreFfiDiagnostics = Read-Text "crates/clearra-core-ffi/src/diagnostics/mod.rs"
foreach ($requiredMarker in @(
            "CoreFfiDiagnosticCode",
            "memory_status_diagnostic_code",
            "E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE",
            "E_CORE_MEMORY_SCOPE_INVALID",
            "E_CORE_MEMORY_LEAK_DETECTED",
            "E_CORE_FFI_BUFFER_BOUNDS",
            "E_CORE_INVALID_NATIVE_VIEW",
            "E_KICK_EVIDENCE_BUFFER_EXHAUSTED"
        )) {
        if ($coreFfiDiagnostics -notlike "*$requiredMarker*") {
            Add-ArchitectureError "S6 core-ffi diagnostics must expose marker '$requiredMarker'"
        }
    }
$executorDiagnostics = Read-Text "crates/clearra-core-executor/src/diagnostics/mod.rs"
foreach ($requiredMarker in @(
            "ExecutorSecurityDiagnostic",
            "E_GPU_WORKER_MISSING_MEMORY_TICKET",
            "E_GPU_FENCE_EPOCH_MISSING",
            "E_GPU_UNCONFIRMED_PROBABILITY_SOURCE",
            "W_BACKEND_FALLBACK_USED",
            "W_TRACE_RETENTION_TRUNCATED",
            "W_OBSERVED_QUEUE_PROBABILITY_INCOMPLETE"
        )) {
        if ($executorDiagnostics -notlike "*$requiredMarker*") {
            Add-ArchitectureError "S6 executor diagnostics must expose marker '$requiredMarker'"
        }
    }
$diagnosticOutputSurface = @(
        Read-Text "crates/clearra-output/src/json/diagnostic_json_contract.rs"
        Read-Text "crates/clearra-cli/src/output/diagnostic_printer.rs"
        Read-Text "crates/clearra-cli/src/output/cli_output_dispatcher.rs"
        Read-Text "tests/golden/diagnostics/security_diagnostic_gate.json"
        Read-Text "tests/golden/diagnostics/security_diagnostic_gate.txt"
    ) -join "`n"
foreach ($requiredMarker in @(
            "diagnostics_contract",
            "diagnostic_evidence",
            "suggested_next_step",
            "render_json",
            "validation_failed_with_format",
            "json_validation_failure_uses_stdout_json_contract",
            "E_FRONTEND_TYPED_REQUEST_REQUIRED",
            "E_GPU_WORKER_MISSING_MEMORY_TICKET"
        )) {
        if ($diagnosticOutputSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "S6 diagnostic output must expose marker '$requiredMarker'"
        }
    }
if ($securityDiagnosticGate -like "*unknown error*") {
        Add-ArchitectureError "S6 security diagnostics must not collapse native/security failures to unknown error"
    }
$gpuWorkerDiagnostic = Read-Text "crates/clearra-validation/src/diagnostic/gpu_worker_diagnostic.rs"
foreach ($requiredMarker in @(
            "GpuWorkerDiagnosticInput",
            "gpu_backend_fallback_diagnostic",
            "gpu_device_unavailable_diagnostic",
            "hybrid_backpressure_active_diagnostic",
            "gpu_worker_trust_mismatch_diagnostic",
            "fallback_reason",
            "memory_ticket_id",
            "fence_epoch",
            "diagnostic_reports_gpu_worker_fallback_reason"
        )) {
        if ($gpuWorkerDiagnostic -notlike "*$requiredMarker*") {
            Add-ArchitectureError "gpu_worker_diagnostic.rs must expose Phase 8 diagnostic marker '$requiredMarker'"
        }
    }
$coreGate = Read-Text "crates/clearra-validation/src/validators/core_security_gate.rs"
$coreGateSurface = $coreGate + "`n" + (Read-Text "crates/clearra-validation/src/validators/core_security_gate_tests.rs")
foreach ($requiredMarker in @(
            "CoreSecurityGate",
            "core_abi_version_mismatch",
            "CClrMemStatus",
            "ffi_problem_error",
            "memory_leak_report",
            "gpu_unavailable",
            "backend_fallback_used",
            "gpu_result_cpu_confirm_required",
            "invalid_c_result_buffer",
            "packing_candidate_used_as_solution",
            "invalid_c_result_buffer_is_rejected_as_core_buildup_failure"
        )) {
        if ($coreGateSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M29 core security gate must map core/backend failure marker '$requiredMarker'"
        }
    }
$pcExecutionValidator = @(
        Read-Text "crates/clearra-validation/src/validators/pc_execution_policy_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/pc_execution_policy_capability_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/pc_execution_policy_diagnostic_builder.rs"
    ) -join "`n"
$pcExecutionValidatorSurface = $pcExecutionValidator + "`n" + (Read-Text "crates/clearra-validation/src/validators/pc_execution_policy_validator_tests.rs")
foreach ($requiredMarker in @(
            "CoreSecurityGate::backend_fallback_used",
            "CoreSecurityGate::gpu_unavailable",
            "EBackendGpuUnavailable",
            "WBackendFallbackUsed"
        )) {
        if ($pcExecutionValidatorSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M29 backend validation must connect fallback/GPU unavailable diagnostics marker '$requiredMarker'"
        }
    }
$coverageBridge = Read-Text "crates/clearra-coverage/src/matrix/coverage_row_bridge.rs"
foreach ($requiredMarker in @(
            "WordCountMismatch",
            "WordCountExceedsInput",
            "TailBitsOutsidePatternUniverse",
            "CandidateIdOutOfRange"
        )) {
        if ($coverageBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M29 coverage row bridge must reject invalid C result buffers marker '$requiredMarker'"
        }
    }
$pcService = Get-PcServiceValidationSurface
$pcBackendReportAdapter = Read-Text "crates/clearra-core-executor/src/service/pc_backend_report_adapter.rs"
$pcOutputSurface = "$pcService`n$pcBackendReportAdapter"
foreach ($requiredMarker in @(
            "gpu_unavailable_reason",
            "backend_fallback_reason",
            "hybrid_memory_leak_report_clean",
            "packing_candidate_is_solution"
        )) {
        if ($pcOutputSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M29 executor output must expose backend/memory/candidate gate marker '$requiredMarker'"
        }
    }
if ($pcService -notlike '*field("packing_candidate_is_solution", "false")*') {
        Add-ArchitectureError "M29 executor output must not mark a PackingCandidate as a solution before BuildUp"
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "M29 Diagnostics and Security Gate",
            "C status maps to Rust diagnostic",
            "GPU unavailable maps to diagnostic",
            "memory leak report maps to diagnostic",
            "invalid C result buffer rejected",
            "PackingCandidate as solution attempt rejected",
            "S6 Security Diagnostic Gate",
            "JSON diagnostics include diagnostic evidence",
            "text diagnostics include suggested_next_step",
            "C status is not collapsed to unknown error",
            "backend fallback diagnostic is visible",
            "security error is not downgraded to warning"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M29 diagnostics/security marker '$requiredMarker'"
        }
    }
$diagnosticsDoc = Read-Text "docs/diagnostics.md"
foreach ($requiredMarker in @(
            "E_GPU_WORKER_TRUST_MISMATCH",
            "W_GPU_BACKEND_FALLBACK",
            "W_GPU_DEVICE_UNAVAILABLE",
            "W_HYBRID_BACKPRESSURE_ACTIVE",
            "gpu_worker_state",
            "gpu_worker_trust_state",
            "memory_ticket_id",
            "fence_epoch",
            "diagnostic_reports_gpu_worker_fallback_reason",
            "E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE",
            "E_CORE_MEMORY_SCOPE_INVALID",
            "E_CORE_MEMORY_LEAK_DETECTED",
            "E_CORE_FFI_BUFFER_BOUNDS",
            "E_CORE_INVALID_NATIVE_VIEW",
            "E_GPU_WORKER_MISSING_MEMORY_TICKET",
            "E_GPU_FENCE_EPOCH_MISSING",
            "E_GPU_UNCONFIRMED_PROBABILITY_SOURCE",
            "E_RENDER_RUNTIME_SVG_FORBIDDEN",
            "E_RENDER_ASSET_PROVENANCE_MISSING",
            "E_GUI_SUBPROCESS_FORBIDDEN",
            "E_FRONTEND_TYPED_REQUEST_REQUIRED",
            "contract.diagnostics.items",
            "suggested_next_step",
            "Security Diagnostic Gate"
        )) {
        if ($diagnosticsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/diagnostics.md must document GPU worker diagnostic marker '$requiredMarker'"
        }
    }
$diagnosticCodesDoc = Read-Text "docs/diagnostic-codes.md"
foreach ($requiredMarker in @(
            "Security Diagnostic Gate",
            "E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE",
            "E_CORE_MEMORY_SCOPE_INVALID",
            "E_CORE_MEMORY_LEAK_DETECTED",
            "E_CORE_FFI_BUFFER_BOUNDS",
            "E_CORE_INVALID_NATIVE_VIEW",
            "E_GPU_WORKER_MISSING_MEMORY_TICKET",
            "E_GPU_FENCE_EPOCH_MISSING",
            "E_GPU_UNCONFIRMED_PROBABILITY_SOURCE",
            "E_RENDER_RUNTIME_SVG_FORBIDDEN",
            "E_RENDER_ASSET_PROVENANCE_MISSING",
            "E_GUI_SUBPROCESS_FORBIDDEN",
            "E_FRONTEND_TYPED_REQUEST_REQUIRED",
            "contract.diagnostics.items"
        )) {
        if ($diagnosticCodesDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/diagnostic-codes.md must document S6 diagnostic marker '$requiredMarker'"
        }
    }
}


