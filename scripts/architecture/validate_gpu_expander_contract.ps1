# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Keep functions side-effect free at load time; validation runs only when invoked.

function Invoke-GpuExpanderContractValidation() {
foreach ($requiredPath in @(
            "core-c/src/gpu/gpu_packing_expander.h",
            "core-c/src/gpu/gpu_packing_expander.c",
            "core-c/src/gpu/gpu_partial_state.h",
            "core-c/src/gpu/gpu_packing_expander.c",
            "core-c/src/gpu/gpu_operation_table_view.h",
            "core-c/src/gpu/gpu_packing_expander.c",
            "core-c/src/gpu/gpu_packing_expander.c",
            "core-c/src/gpu/gpu_packing_expander.c"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "GPU portable expander contract file is missing: $requiredPath"
        }
    }
$gpuPipelineDoc = Read-Text "docs/gpu-pipeline.md"
foreach ($requiredMarker in @(
            "GPU Worker Completion Phase 2 Portable Candidate Expander",
            "level 0 starts from exactly one",
            "expands every partial state",
            "collision checks and active packing row bounds checks",
            "clearra_gpu_kernel_packing_dispatch",
            "clearra_gpu_packing_expander_run",
            "does not perform hold validation",
            "CPU BuildUp",
            "gpu_expander_level_zero_has_initial_state",
            "gpu_expander_expands_one_piece_operations",
            "gpu_expander_rejects_collision",
            "gpu_expander_respects_active_packing_rows",
            "gpu_expander_emits_candidate_after_piece_count"
        )) {
        if ($gpuPipelineDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/gpu-pipeline.md must document GPU expander marker '$requiredMarker'"
        }
    }
$cMake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
            "src/gpu/gpu_packing_expander.c",
            "src/gpu/gpu_packing_expander.c",
            "src/gpu/gpu_packing_expander.c",
            "src/gpu/gpu_packing_expander.c",
            "src/gpu/gpu_packing_expander.c"
        )) {
        if ($cMake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must build GPU expander marker '$requiredMarker'"
        }
    }
$dispatch = Read-Text "core-c/src/gpu/gpu_kernel_launch.c"
foreach ($requiredMarker in @(
            "clearra_gpu_kernel_packing_dispatch",
            "clearra_gpu_packing_expander_run"
        )) {
        if ($dispatch -notlike "*$requiredMarker*") {
            Add-ArchitectureError "gpu_kernel_packing.c must dispatch through expander marker '$requiredMarker'"
        }
    }
if ($dispatch -like "*clearra_portable_packing_kernel_ref_generate(batch, out_buffer)*") {
        Add-ArchitectureError "gpu_kernel_packing.c must not dispatch product kernel generation through CPU reference generator"
    }
$expander = Read-Text "core-c/src/gpu/gpu_packing_expander.c"
foreach ($requiredMarker in @(
            "clearra_gpu_packing_expander_init",
            "clearra_gpu_packing_expander_expand_level",
            "clearra_gpu_packing_expander_run",
            "clr_scratch_alloc",
            "CLR_SCOPE_WORKER",
            "clearra_gpu_capacity_guard_partial_state",
            "clearra_packing_pruner_accepts_static_candidate_with_ledger",
            "expander->pruning_ledger",
            "prune_context.state_layer = depth",
            "clearra_gpu_candidate_emit"
        )) {
        if ($expander -notlike "*$requiredMarker*") {
            Add-ArchitectureError "gpu_packing_expander.c must implement level expansion marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "static CLEARRA_THREAD_LOCAL ClearraGpuPackingExpander",
            "static ClearraGpuPackingExpander scratch",
            "operation->mask & ~expander->active_region_mask",
            "state->occupied_mask & operation->mask"
        )) {
        if ($expander -like "*$forbiddenMarker*") {
            Add-ArchitectureError "gpu_packing_expander.c must not use static expander scratch marker '$forbiddenMarker'"
        }
    }
$tests = Get-GpuTestsValidationSurface
foreach ($requiredMarker in @(
            "gpu_expander_level_zero_has_initial_state",
            "gpu_expander_expands_one_piece_operations",
            "gpu_expander_rejects_collision",
            "gpu_expander_respects_active_packing_rows",
            "gpu_expander_emits_candidate_after_piece_count",
            "gpu_expander_collision_drop_records_actual_batch_and_layer",
            "gpu_prune_digest_uses_operation_rule_and_kick_identity"
        )) {
        if ($tests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c GPU tests must include expander marker '$requiredMarker'"
        }
    }
}
