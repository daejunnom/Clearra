# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Keep functions side-effect free at load time; validation runs only when invoked.

function Invoke-GpuBatchSourceContractValidation() {
    foreach ($requiredPath in @(
        "core-c/include/clr_gpu.h",
        "crates/clearra-core-ffi/src/gpu/gpu_packing_batch_descriptor_view.rs",
        "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_source.rs",
        "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_from_problem.rs",
        "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_from_candidate_region.rs",
        "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_source_error.rs"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "GPU batch source contract file is missing: $requiredPath"
        }
    }

    $cDescriptor = Read-Text "core-c/include/clr_gpu.h"
    $ffiDescriptor = Read-Text "crates/clearra-core-ffi/src/gpu/gpu_packing_batch_descriptor_view.rs"
    $source = Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_source.rs"
    $descriptorSurface = "$cDescriptor`n$ffiDescriptor`n$source"
    foreach ($requiredMarker in @(
        "piece_source_id",
        "piece_multiset_window",
        "pattern_universe_id",
        "pattern_weight_model_id"
    )) {
        if ($descriptorSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GPU packing descriptor must preserve multiset/source identity marker '$requiredMarker'"
        }
    }

    foreach ($forbiddenMarker in @(
        "legacy_piece_preview",
        "legacy piece preview",
        "pub pieces:",
        "uint8_t pieces[",
        "ClearraGpuBatchDescriptor",
        "StandardGpuBatchDescriptor",
        "CGpuBatchDescriptorView"
    )) {
        if ($descriptorSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "GPU packing descriptor must not retain ordered preview marker '$forbiddenMarker'"
        }
    }

    $fromProblem = Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_from_problem.rs"
    foreach ($requiredMarker in @(
        "coverage_universe_identity",
        "problem.budget().max_results()",
        "packing_batch_source_from_problem"
    )) {
        if ($fromProblem -notlike "*$requiredMarker*") {
            Add-ArchitectureError "packing_batch_from_problem.rs must resolve product source marker '$requiredMarker'"
        }
    }

    $fromCandidateRegion = Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_from_candidate_region.rs"
    foreach ($requiredMarker in @(
        "gpu_piece_multiset_window",
        "active_packing_rows",
        "compact.board.initial_mask",
        "stable_nonzero_batch_hash",
        "C_PIECE_SOURCE_FIXED_QUEUE"
    )) {
        if ($fromCandidateRegion -notlike "*$requiredMarker*") {
            Add-ArchitectureError "packing_batch_from_candidate_region.rs must resolve compact candidate region marker '$requiredMarker'"
        }
    }

    $builder = Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_descriptor_builder.rs"
    foreach ($requiredMarker in @(
        "from_source",
        "PackingBatchSource::from_search_problem",
        "PackingBatchSource::from_compact_problem_with_identity"
    )) {
        if ($builder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "PackingBatchDescriptorBuilder must delegate to source marker '$requiredMarker'"
        }
    }
}
