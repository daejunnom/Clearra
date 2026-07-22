# This file is dot-sourced by validate_gpu_pipeline.ps1.

function Invoke-HybridSchedulerValidation() {
    $selector = Read-Text 'crates/clearra-core-executor/src/backend/backend_selector.rs'
    foreach ($required in @(
        'prepared_gpu_capability',
        'HybridGpuReady',
        'HybridGpuNotReadyCpu',
        'RequestedSearchBackend::Hybrid'
    )) {
        if ($selector -notlike "*$required*") {
            Add-ArchitectureError "Hybrid selection policy is missing '$required'"
        }
    }
    $executor = Read-Text 'crates/clearra-core-executor/src/backend/native_packing_executors.rs'
    foreach ($forbidden in @(
        'NativeHybridPackingExecutor',
        'gpu_assisted_reference', 'gpu-assisted-reference',
        'portable-gpu-worker', 'CoreCNative::linked()'
    )) {
        if ($executor -like "*$forbidden*") {
            Add-ArchitectureError "Hybrid product path contains fake execution marker '$forbidden'"
        }
    }

    $cScheduler = @(
        Read-Text 'core-c/src/scheduler/hybrid_scheduler.c'
        Read-Text 'core-c/src/scheduler/hybrid_backpressure.c'
        Read-Text 'core-c/src/scheduler/gpu_worker_scheduler_bridge.c'
    ) -join "`n"
    foreach ($required in @(
        'clearra_hybrid_scheduler_run_cpu_fallback', 'fallback_used',
        'clearra_gpu_worker_scheduler_bridge_run', 'gpu_batches_submitted',
        'gpu_readback_pending', 'memory_pressure_level'
    )) {
        if ($cScheduler -notlike "*$required*") {
            Add-ArchitectureError "C scheduler contract is missing '$required'"
        }
    }
    if ($cScheduler -like '*portable*') {
        Add-ArchitectureError 'C scheduler must not label CPU fallback as a portable GPU backend'
    }
}
