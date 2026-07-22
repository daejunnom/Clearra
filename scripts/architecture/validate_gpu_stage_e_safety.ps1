# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Keep functions side-effect free at load time; validation runs only when invoked.

function Invoke-GpuStageEMemorySchedulerSafetyValidation() {
$gpuPipelineDoc = Read-Text "docs/gpu-pipeline.md"
foreach ($requiredMarker in @(
            "GPU Worker Completion Stage E Memory And Scheduler Safety",
            "E1. Every GPU buffer is associated with a memory ticket",
            "E2. GPU buffer release performs the actual free only after the fence epoch",
            "E3. Scheduler hot path code has no raw",
            "E4. Release queue drain leaves the memory leak report clean",
            "E5. If backpressure occurs, throttle reason is preserved",
            "E6. Coverage rows and score cells are never silently dropped",
            "gpu_worker_scheduler_bridge_uses_memory_ticket_and_fence",
            "gpu_buffer_release_before_fence_is_deferred",
            "gpu_buffer_release_before_fence_deferred",
            "gpu_buffer_release_after_fence_is_clean",
            "hybrid_scheduler_no_raw_malloc_in_hot_path",
            "hybrid_scheduler_failure_has_clean_leak_report",
            "autotune_never_drops_coverage_rows_silently",
            "partial_result_reports_truncation_reason"
        )) {
        if ($gpuPipelineDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/gpu-pipeline.md must document GPU worker Stage E memory and scheduler safety marker '$requiredMarker'"
        }
    }
$memoryTests = Read-Text "core-c/tests/memory_tests.c"
foreach ($requiredMarker in @(
            "gpu_buffer_release_before_fence_is_deferred",
            "gpu_buffer_release_before_fence_deferred",
            "gpu_buffer_release_after_fence_is_clean",
            "pending_gpu_buffer_releases",
            "clr_gpu_buffer_set_fence_epoch",
            "clr_release_queue_drain",
            "expect_zero_live_leaks"
        )) {
        if ($memoryTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "C memory tests must verify GPU fence release marker '$requiredMarker'"
        }
    }
$gpuTests = Get-GpuTestsValidationSurface
foreach ($requiredMarker in @(
            "gpu_worker_scheduler_bridge_uses_memory_ticket_and_fence",
            "memory_ticket_id",
            "fence_epoch",
            "live_gpu_buffers",
            "pending_gpu_buffer_releases"
        )) {
        if ($gpuTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "C GPU worker tests must verify memory ticket and fence marker '$requiredMarker'"
        }
    }
$schedulerTests = Get-SchedulerTestsValidationSurface
foreach ($requiredMarker in @(
            "hybrid_scheduler_no_raw_malloc_in_hot_path",
            "hybrid_scheduler_failure_has_clean_leak_report",
            "autotune_never_drops_coverage_rows_silently",
            "partial_result_reports_truncation_reason",
            "throttle_reason",
            "coverage_row_buffer_pressure",
            "partial_result_diagnostic_required"
        )) {
        if ($schedulerTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "C scheduler tests must verify Stage E safety marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @("malloc(", "calloc(", "realloc(", "free(")) {
        foreach ($schedulerSource in @(
                "core-c/src/scheduler/hybrid_scheduler.c",
                "core-c/src/scheduler/hybrid_backpressure.c",
                "core-c/src/scheduler/hybrid_backpressure.c",
                "core-c/src/scheduler/hybrid_backpressure.c",
                "core-c/src/scheduler/hybrid_backpressure.c"
            )) {
            if ((Read-Text $schedulerSource) -like "*$forbiddenMarker*") {
                Add-ArchitectureError "$schedulerSource must not use raw allocation marker '$forbiddenMarker' in the scheduler hot path"
            }
        }
    }
}
