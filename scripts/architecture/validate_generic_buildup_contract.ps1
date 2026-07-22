# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# G7 exposes only the connected Board64 scope and explicit unsupported results.

function Invoke-GenericBuildUpContractValidation() {
foreach ($requiredFile in @(
            "core-c/include/clr_problem.h",
            "core-c/src/buildup/generic_buildup.h",
            "core-c/src/buildup/generic_buildup.c",
            "core-c/src/buildup/buildup_worker.c",
            "core-c/tests/buildup_problem_tests.c",
            "crates/clearra-core-ffi/src/problem/generic_buildup.rs",
            "crates/clearra-core-executor/src/buildup/generic_buildup.rs",
            "docs/architecture.md",
            "docs/algorithms.md",
            "docs/future-custom-pieces.md",
            "docs/mvp-scope.md",
            "scripts/generic-buildup-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "G7 required generic BuildUp file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "core-c/include/clr_problem.h"
        Read-Text "core-c/src/buildup/generic_buildup.h"
        Read-Text "core-c/src/buildup/generic_buildup.c"
        Read-Text "core-c/src/buildup/buildup_worker.c"
        Read-Text "core-c/tests/buildup_problem_tests.c"
        Read-Text "crates/clearra-core-ffi/src/problem/generic_buildup.rs"
        Read-Text "crates/clearra-core-executor/src/buildup/generic_buildup.rs"
        Read-Text "docs/architecture.md"
        Read-Text "docs/algorithms.md"
        Read-Text "docs/future-custom-pieces.md"
        Read-Text "docs/mvp-scope.md"
        Read-Text "scripts/generic-buildup-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE",
            "clearra_buildup_runtime_status_for_board",
            "clearra_buildup_operation_set_runtime_status",
            "mvp1_buildup_15_operation_fast_path_unchanged",
            "operation_count_above_runtime_limit_is_unsupported",
            "board128_buildup_guard_reports_unsupported",
            "unsupported_buildup_scope_does_not_claim_solution",
            "BuildUpCapability",
            "ConnectedExact",
            "Unsupported",
            "C_BUILDUP_MAX_OPERATIONS",
            "operation_count > 15 is guarded, not truncated",
            "Board128/Wide BuildUp is unsupported",
            "compile-c-and-rust-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G7 generic BuildUp contract must expose marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "Board64BuildUpState",
            "Board128BuildUpState",
            "WideBoardBuildUpState",
            "GenericLineClearState",
            "DynamicOperationBitSet",
            "DynamicDeletedRowMap",
            "requires_future_dynamic_runtime",
            "dynamic_operation_bitset_schema_exists",
            "operation_count_gt_15_silently_truncated",
            "operation_count > 15 silently truncate",
            "Board128BuildUpFallbackToBoard64",
            "board128_buildup_board64_fallback",
            "wide_buildup_board64_fallback",
            "generic_piece_y_adjustment_uses_standard_tetromino_shape",
            "generic_buildup_claims_solution_before_connected"
        )) {
        if ($surface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "G7 must not introduce forbidden generic BuildUp shortcut marker '$forbiddenMarker'"
        }
    }
}
