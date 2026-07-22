# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-GpuPackingStrengtheningContractValidation() {
foreach ($requiredPath in @(
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_packing_strengthening.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_larger_batch_planner.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_dominance_prefilter.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_candidate_hash.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_readback_compression.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_cpu_exact_confirm_optimizer.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_coverage_bitset_or_helper.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_autotune.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_memory_pressure.rs",
            "scripts/gpu-packing-strengthening-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "X9 GPU Packing Strengthening required file is missing: $requiredPath"
        }
    }
$gpuSurface = @(
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_packing_strengthening.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_larger_batch_planner.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_dominance_prefilter.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_candidate_hash.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_readback_compression.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_cpu_exact_confirm_optimizer.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_coverage_bitset_or_helper.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_autotune.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_memory_pressure.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "GpuPackingStrengthening",
            "GpuPackingStrengtheningReport",
            "GpuLargerBatchPlanner",
            "larger_batch_planner",
            "GpuDominancePrefilter",
            "dominance_prefilter_does_not_drop_required_candidate",
            "GpuCandidateHash",
            "gpu_candidate_hash",
            "GpuReadbackCompression",
            "readback_compression_preserves_candidates",
            "GpuCpuExactConfirmOptimizer",
            "gpu_result_deterministic",
            "gpu_result_cpu_confirmed",
            "cpu_reference_and_gpu_result_match",
            "hash_only_success: false",
            "GpuCoverageBitsetOrHelper",
            "CpuConfirmRequired",
            "GpuWorkerAutotune",
            "GpuWorkerMemoryPressure",
            "fallback_reason",
            "unconfirmed_gpu_coverage_cannot_source_probability"
        )) {
        if ($gpuSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X9 GPU packing strengthening must expose marker '$requiredMarker'"
        }
    }
$modSurface = @(
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/mod.rs"
        Read-Text "crates/clearra-core-executor/src/backend/mod.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "GpuPackingStrengthening",
            "GpuPackingStrengtheningReport",
            "GpuLargerBatchPlanner",
            "GpuDominancePrefilter",
            "GpuCandidateHash",
            "GpuReadbackCompression",
            "GpuCpuExactConfirmOptimizer",
            "GpuCoverageBitsetOrHelper"
        )) {
        if ($modSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X9 public backend module must export marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "X9 GPU Packing Strengthening",
            "larger batch planner",
            "dominance prefilter",
            "GPU candidate hash",
            "readback compression",
            "CPU exact confirm optimization",
            "coverage bitset OR helper",
            "backend autotune",
            "memory pressure handling",
            "fallback reason visible",
            "CPU exact confirm remains mandatory",
            "unconfirmed GPU coverage never sources probability"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document X9 marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "hash_only_success: true",
            "can_source_exact_probability: true",
            "skip_cpu_confirm",
            "unconfirmed_probability"
        )) {
        if ($gpuSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "X9 must not contain forbidden shortcut marker '$forbiddenMarker'"
        }
    }
}
