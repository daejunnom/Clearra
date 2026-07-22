# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Runtime equivalence belongs to executable GPU tests. This task validates only
# the product boundary and the isolation of the old materializing checkpoint.

function Invoke-GpuHostReducerContractValidation() {
    $gpuPipelineDoc = Read-Text "docs/gpu-pipeline.md"
    foreach ($requiredMarker in @(
            "Windows WebGPU geometry exact cover",
            "embedded_geometry_exact_cover.wgsl",
            "exact host reduction",
            "per-dispatch CPU transition samples",
            "Queue, hold, score, spin, Fumen, and render state never enter a GPU batch",
            "RejectedTrustMismatch",
            "candidate hash without exact payload comparison"
        )) {
        if ($gpuPipelineDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/gpu-pipeline.md must document connected exact WebGPU marker '$requiredMarker'"
        }
    }

    foreach ($requiredPath in @(
            "crates/clearra-core-executor/src/backend/native_webgpu_packing_executor.rs",
            "crates/clearra-core-executor/src/backend/buildable_geometry_task_reducer.rs",
            "crates/clearra-webgpu/src/embedded_geometry_exact_cover.wgsl",
            "core-c/src/packing/geometry_buildable_stream.c"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "connected WebGPU geometry path is missing: $requiredPath"
        }
    }

    $webGpuExecutor = Read-Text "crates/clearra-core-executor/src/backend/native_webgpu_packing_executor.rs"
    foreach ($requiredMarker in @(
            "execute_webgpu_buildable_unique",
            "reduce_buildable_geometry_paths",
            "stream_partition_paths",
            "consumer.consume_row_ids",
            "result.can_claim_exact()",
            "RejectedTrustMismatch",
            "GpuExecutionFailure::trust_mismatch",
            "GpuExecutionFailure::resource_incomplete"
        )) {
        if ($webGpuExecutor -notlike "*$requiredMarker*") {
            Add-ArchitectureError "WebGPU product executor must preserve exact buildability/trust marker '$requiredMarker'"
        }
    }

    $buildableReducer = Read-Text "crates/clearra-core-executor/src/backend/buildable_geometry_task_reducer.rs"
    foreach ($requiredMarker in @(
            "NativeCandidateReducer::new",
            "stream_buildable_rows",
            "BuildabilitySourceSelection::ConcretePatterns",
            "NativeBuildUpWorkspace",
            "source_pattern_bits"
        )) {
        if ($buildableReducer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GPU and CPU must share the pattern-specific BuildUp reducer marker '$requiredMarker'"
        }
    }

    $manifest = Read-Text "core-c/cmake/source_manifest.cmake"
    $testingBoundary = $manifest.IndexOf("if(BUILD_TESTING)")
    foreach ($checkpointSource in @(
            "src/gpu/gpu_backend.c",
            "src/gpu/gpu_readback_reduce.c",
            "src/gpu/gpu_host_confirm.c",
            "src/packing/cpu_packing_reference.c"
        )) {
        $sourceIndex = $manifest.IndexOf($checkpointSource)
        if ($testingBoundary -lt 0 -or $sourceIndex -lt $testingBoundary) {
            Add-ArchitectureError "legacy GPU materializing checkpoint must be BUILD_TESTING-only: $checkpointSource"
        }
    }

    foreach ($forbiddenProductMarker in @(
            "clearra_gpu_readback_reduce_result(",
            "clearra_gpu_confirmed_candidate_queue_from_result("
        )) {
        if ($webGpuExecutor -like "*$forbiddenProductMarker*" -or
            $buildableReducer -like "*$forbiddenProductMarker*") {
            Add-ArchitectureError "WebGPU product path must not call legacy C checkpoint '$forbiddenProductMarker'"
        }
    }
}
