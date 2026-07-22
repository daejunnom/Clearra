# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Keep U2 hybrid scheduler contract checks split from the older M25 pipeline validator.

function Invoke-HybridSchedulerContractValidation() {
$gpuPipelineDoc = Read-Text "docs/gpu-pipeline.md"
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "U2 Hybrid Scheduler Contract",
            "large packing batch",
            "readback",
            "dominance prefilter",
            "candidate hash",
            "host reducer",
            "exact confirm",
            "BuildUp",
            "coverage row",
            "diagnostics",
            "candidate_queue_len",
            "candidate_queue_capacity",
            "cpu_worker_backlog",
            "gpu_readback_backlog",
            "gpu_batch_in_flight",
            "backpressure_active",
            "deferred_batch_count",
            "truncated_batch_count",
            "memory_pressure_level",
            "throttle_reason",
            "cpu_only_result_equals_hybrid_result",
            "gpu_packing_cpu_buildup_result_equals_cpu_reference",
            "hybrid_backpressure_reports_throttle_reason",
            "hybrid_scheduler_reports_u2_backpressure_contract",
            "memory_pressure_reduces_batch_size",
            "hybrid_result_reports_backend_metrics"
        )) {
        if ($gpuPipelineDoc -notlike "*$requiredMarker*" -and $architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U2 hybrid scheduler docs must expose marker '$requiredMarker'"
        }
    }
$cSchedulerSurface = @(
        Read-Text "core-c/src/scheduler/hybrid_scheduler.h"
        Read-Text "core-c/src/scheduler/hybrid_backpressure.c"
        Read-Text "core-c/src/scheduler/hybrid_backpressure.c"
        Get-SchedulerTestsValidationSurface
    ) -join "`n"
foreach ($requiredMarker in @(
            "ClearraHybridBackpressureReport",
            "candidate_queue_len",
            "candidate_queue_capacity",
            "cpu_worker_backlog",
            "gpu_readback_backlog",
            "gpu_batch_in_flight",
            "backpressure_active",
            "deferred_batch_count",
            "truncated_batch_count",
            "memory_pressure_level",
            "clearra_hybrid_backpressure_report_for",
            "clearra_hybrid_autotune_evaluate",
            "hybrid_scheduler_reports_u2_backpressure_contract",
            "memory_pressure_reduces_batch_size",
            "hybrid_result_reports_backend_metrics"
        )) {
        if ($cSchedulerSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "C U2 hybrid scheduler contract must expose marker '$requiredMarker'"
        }
    }
$rustSurface = @(
        Read-Text "crates/clearra-core-ffi/src/native/hybrid.rs"
        Read-Text "crates/clearra-core-executor/src/backend/hybrid_backpressure_report.rs"
        Read-Text "crates/clearra-core-executor/src/backend/hybrid_scheduler_contract_tests.rs"
        Read-Text "crates/clearra-core-executor/src/packing/hybrid_scheduler_report.rs"
        Read-Text "crates/clearra-core-executor/src/packing/packing_metrics.rs"
        Read-Text "crates/clearra-core-executor/src/service/pc_pipeline_fields.rs"
        Read-Text "crates/clearra-core-executor/src/service/setup_service.rs"
        Read-Text "crates/clearra-core-executor/src/service/cover_service.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/backend_options_schema.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "CNativeHybridBackpressureReport",
            "HybridBackpressureReport",
            "with_u2_contract",
            "candidate_queue_len",
            "candidate_queue_capacity",
            "cpu_worker_backlog",
            "gpu_readback_backlog",
            "gpu_batch_in_flight",
            "backpressure_active",
            "deferred_batch_count",
            "truncated_batch_count",
            "memory_pressure_level",
            "hybrid_candidate_queue_len",
            "hybrid_memory_pressure_level",
            "memory_pressure_reduces_batch_size",
            "hybrid_backpressure_report_exposes_u2_scheduler_fields"
        )) {
        if ($rustSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "Rust/UI U2 hybrid scheduler contract must expose marker '$requiredMarker'"
        }
    }
}
