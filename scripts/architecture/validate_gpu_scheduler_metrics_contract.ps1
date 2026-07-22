# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Keep functions side-effect free at load time; validation runs only when invoked.

function Invoke-GpuSchedulerMetricsContractValidation() {
$gpuPipelineDoc = Read-Text "docs/gpu-pipeline.md"
foreach ($requiredMarker in @(
            "Scheduler metrics are derived from queue stats",
            "hybrid_candidate_queue.c",
            "GPU queue records submitted and completed batches",
            "readback queue records pending readback batches",
            "CPU confirm queue records confirm queue depth",
            "ClearraHybridBackendMetrics",
            "ClearraHybridAutotuneMetrics",
            "hybrid_gpu_queue_tracks_submitted_completed_and_latency",
            "hybrid_readback_queue_tracks_pending_and_candidate_pressure",
            "hybrid_cpu_confirm_queue_tracks_confirm_and_buildup_depth",
            "hybrid_scheduler_metrics_are_derived_from_queue_stats"
        )) {
        if ($gpuPipelineDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/gpu-pipeline.md must document scheduler metrics marker '$requiredMarker'"
        }
    }
foreach ($requiredPath in @(
            "core-c/src/scheduler/hybrid_candidate_queue.c",
            "core-c/src/scheduler/hybrid_scheduler.c"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "GPU scheduler metrics required file is missing: $requiredPath"
        }
    }
$schedulerHeader = Read-Text "core-c/src/scheduler/hybrid_scheduler.h"
foreach ($requiredMarker in @(
            "ClearraHybridGpuQueueStats",
            "ClearraHybridReadbackQueueStats",
            "ClearraHybridCpuConfirmQueueStats",
            "gpu_batches_submitted",
            "gpu_batches_completed",
            "gpu_readback_pending",
            "cpu_confirm_queue_depth",
            "cpu_buildup_queue_depth",
            "candidate_buffer_pressure",
            "memory_ticket_live_count",
            "pending_release_queue_depth",
            "average_batch_latency_ms",
            "average_cpu_confirm_latency_ms"
        )) {
        if ($schedulerHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "hybrid_scheduler.h must expose scheduler metrics marker '$requiredMarker'"
        }
    }
$queueSources = @(
        Read-Text "core-c/src/scheduler/hybrid_candidate_queue.c"
    ) -join "`n"
foreach ($requiredMarker in @(
            "clearra_hybrid_gpu_queue_submit",
            "clearra_hybrid_gpu_queue_complete",
            "clearra_hybrid_readback_queue_enqueue",
            "clearra_hybrid_readback_queue_complete",
            "clearra_hybrid_cpu_confirm_queue_enqueue",
            "clearra_hybrid_cpu_confirm_queue_complete",
            "clearra_hybrid_cpu_confirm_queue_apply_metrics"
        )) {
        if ($queueSources -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scheduler queue sources must preserve marker '$requiredMarker'"
        }
    }
$schedulerSource = Read-Text "core-c/src/scheduler/hybrid_scheduler.c"
foreach ($requiredMarker in @(
            "clearra_hybrid_gpu_queue_apply_metrics",
            "clearra_hybrid_readback_queue_apply_metrics",
            "clearra_hybrid_cpu_confirm_queue_apply_metrics",
            "clearra_hybrid_autotune_evaluate",
            "elapsed_ms_since"
        )) {
        if ($schedulerSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "hybrid_scheduler.c must derive metrics from queue stats marker '$requiredMarker'"
        }
    }
$cMake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
            "src/scheduler/hybrid_candidate_queue.c",
            "src/scheduler/hybrid_scheduler.c"
        )) {
        if ($cMake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must compile scheduler metric source '$requiredMarker'"
        }
    }
$schedulerTests = Get-SchedulerTestsValidationSurface
foreach ($requiredMarker in @(
            "hybrid_gpu_queue_tracks_submitted_completed_and_latency",
            "hybrid_readback_queue_tracks_pending_and_candidate_pressure",
            "hybrid_cpu_confirm_queue_tracks_confirm_and_buildup_depth",
            "hybrid_scheduler_metrics_are_derived_from_queue_stats"
        )) {
        if ($schedulerTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scheduler tests must verify metrics queue marker '$requiredMarker'"
        }
    }
}
