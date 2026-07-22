# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Keep functions side-effect free at load time; validation runs only when invoked.

function Invoke-GpuStageFVisibilityValidation() {
$gpuPipelineDoc = Read-Text "docs/gpu-pipeline.md"
foreach ($requiredMarker in @(
            'GPU Worker Completion Stage F Output Diagnostic And GUI Visibility',
            'F1. JSON output includes',
            'backend_report.gpu_worker',
            'F2.',
            'trust_state',
            'memory_ticket_id',
            'fence_epoch',
            'cpu_confirm_required',
            'can_source_exact_probability',
            'F3.',
            'fallback_reason',
            'unavailable_reason',
            'F4. Default text shows only compact backend',
            'F5. GUI shows GPU disabled, running, or fallback status',
            'json_backend_report_includes_gpu_worker_trust_state',
            'json_gpu_worker_report_shows_connected_confirmed_state',
            'json_backend_report_includes_memory_ticket_and_fence_epoch',
            'json_gpu_worker_report_shows_memory_ticket_and_fence',
            'json_backend_report_includes_gpu_worker_unavailable_reason',
            'text_default_summarizes_gpu_worker_without_internal_noise',
            'text_verbose_includes_gpu_worker_backpressure',
            'diagnostic_reports_gpu_worker_fallback_reason',
            'diagnostic_reports_gpu_worker_unavailable_reason',
            'gui_gpu_status_view_reports_unavailable_before_execution',
            'gui_does_not_execute_solver_for_gpu_status_preview',
            'U0 Backend Capability Report',
            'backend_requested',
            'backend_selected',
            'candidate_backend',
            'buildup_backend',
            'gpu_available',
            'gpu_disabled_reason',
            'cpu_reference_matched',
            'fallback_used',
            'fallback_backend',
            'backend_fallback_reason',
            'hybrid_status',
            'hybrid_disabled_reason',
            'memory_pressure_level',
            'backpressure',
            'backend_report_present_in_json',
            'backend_report_present_in_verbose_text',
            'gpu_unavailable_reports_reason',
            'hybrid_disabled_reports_reason',
            'fallback_used_reports_reason'
        )) {
        if ($gpuPipelineDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/gpu-pipeline.md must document GPU worker Stage F visibility marker '$requiredMarker'"
        }
    }
$pcJsonContract = Read-Text "crates/clearra-output/src/json/backend_report_contract.rs"
foreach ($requiredMarker in @(
            "pc_backend_report_contract",
            "backend_requested",
            "backend_selected",
            "candidate_backend",
            "buildup_backend",
            "gpu_available",
            "gpu_disabled_reason",
            "gpu_trust_state",
            "cpu_confirm_required",
            "cpu_reference_matched",
            "fallback_used",
            "fallback_backend",
            "backend_fallback_reason",
            "hybrid_status",
            "hybrid_disabled_reason",
            "memory_pressure_level",
            "backpressure"
        )) {
        if ($pcJsonContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "backend_report_contract.rs must expose U0 backend capability report marker '$requiredMarker'"
        }
    }
$backendGpuWorkerContract = Read-Text "crates/clearra-output/src/json/backend_gpu_worker_contract.rs"
foreach ($requiredMarker in @(
            "backend_gpu_worker_contract",
            "gpu_worker_state",
            "gpu_trust_state",
            "gpu_memory_ticket_id",
            "gpu_fence_epoch",
            "cpu_confirm_required",
            "gpu_can_source_exact_probability",
            "gpu_worker_fallback_reason",
            "gpu_worker_unavailable_reason",
            "json_backend_report_includes_gpu_worker_trust_state",
            "json_gpu_worker_report_shows_connected_confirmed_state",
            "json_backend_report_includes_memory_ticket_and_fence_epoch",
            "json_gpu_worker_report_shows_memory_ticket_and_fence",
            "json_backend_report_includes_gpu_worker_unavailable_reason"
        )) {
        if ($backendGpuWorkerContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "backend_gpu_worker_contract.rs must expose Stage F JSON marker '$requiredMarker'"
        }
    }
$backendSummaryText = Read-Text "crates/clearra-output/src/text/backend_summary_text.rs"
foreach ($requiredMarker in @(
            "BackendSummaryText",
            "default_lines",
            "verbose_lines",
            "gpu: unavailable",
            "memory: clean",
            "gpu_worker_state",
            "gpu_memory_ticket_id",
            "gpu_backpressure_gpu_queue_depth",
            "candidate_backend",
            "buildup_backend",
            "gpu_available",
            "gpu_disabled_reason",
            "cpu_reference_matched",
            "fallback_used",
            "fallback_backend",
            "backend_fallback_reason",
            "hybrid_status",
            "hybrid_disabled_reason",
            "memory_pressure_level",
            "backpressure",
            "text_default_summarizes_gpu_worker_without_internal_noise",
            "text_verbose_includes_gpu_worker_backpressure",
            "backend_report_present_in_verbose_text",
            "gpu_default_summary_ignores_non_actionable_reason"
        )) {
        if ($backendSummaryText -notlike "*$requiredMarker*") {
            Add-ArchitectureError "backend_summary_text.rs must expose Stage F text visibility marker '$requiredMarker'"
        }
    }
$productContractE2E = Read-Text "crates/clearra-cli/tests/product_contract_e2e.rs"
foreach ($requiredMarker in @(
            "backend_report_present_in_json",
            "gpu_unavailable_reports_reason",
            "hybrid_disabled_reports_reason",
            "fallback_used_reports_reason",
            "assert_u0_backend_capability_report",
            "backend_report_string",
            "backend_report_bool"
        )) {
        if ($productContractE2E -notlike "*$requiredMarker*") {
            Add-ArchitectureError "product_contract_e2e.rs must test U0 backend capability marker '$requiredMarker'"
        }
    }
$productE2EScript = Read-Text "scripts/product-e2e.ps1"
foreach ($requiredMarker in @(
            "Invoke-ProductE2EBackendCapabilityReportCase",
            "backend_report_present_in_json",
            "Assert-ProductE2EU0BackendCapabilityReport"
        )) {
        if ($productE2EScript -notlike "*$requiredMarker*") {
            Add-ArchitectureError "product-e2e.ps1 must execute U0 backend capability marker '$requiredMarker'"
        }
    }
$gpuWorkerDiagnostic = Read-Text "crates/clearra-validation/src/diagnostic/gpu_worker_diagnostic.rs"
foreach ($requiredMarker in @(
            "GpuWorkerDiagnosticInput",
            "fallback_reason",
            "unavailable_reason",
            "throttle_reason",
            "memory_ticket_id",
            "fence_epoch",
            "diagnostic_reports_gpu_worker_fallback_reason",
            "diagnostic_reports_gpu_worker_unavailable_reason"
        )) {
        if ($gpuWorkerDiagnostic -notlike "*$requiredMarker*") {
            Add-ArchitectureError "gpu_worker_diagnostic.rs must expose Stage F diagnostic visibility marker '$requiredMarker'"
        }
    }
$guiBridgeTests = Read-Text "crates/clearra-app/tests/gui_bridge_contract.rs"
foreach ($requiredMarker in @(
            "gui_gpu_status_view_reports_unavailable_before_execution",
            "gui_gpu_status_view_reports_cpu_confirmed_result",
            "gui_does_not_execute_solver_for_gpu_status_preview",
            "gpu_status",
            "gpu_trust_state",
            "memory_ticket_status",
            "solver_executed"
        )) {
        if ($guiBridgeTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GUI bridge tests must expose Stage F GPU visibility marker '$requiredMarker'"
        }
    }
}
