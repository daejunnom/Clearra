# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# G6 keeps generic exact cover as a tiling/assignment layer and requires DLX
# results to flow into BuildUp instead of becoming product solutions.

function Invoke-GenericExactCoverDlxContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-exact-cover/src/model/exact_cover_problem.rs",
            "crates/clearra-exact-cover/src/model/exact_cover_problem_schema.rs",
            "crates/clearra-exact-cover/src/model/generic_exact_cover_candidate.rs",
            "crates/clearra-exact-cover/src/solver/dlx_solver.rs",
            "crates/clearra-exact-cover/src/bridge/generic_exact_cover_bridge.rs",
            "crates/clearra-exact-cover/src/bridge/setup_tiling_bridge.rs",
            "crates/clearra-build-coverage/src/exact_cover/build_exact_cover_problem.rs",
            "crates/clearra-setup-search/src/exact_cover/setup_tiling_exact_cover.rs",
            "crates/clearra-core-ffi/src/problem/dlx_buildup_bridge.rs",
            "crates/clearra-core-ffi/src/problem/dlx_build_up_bridge.rs",
            "docs/architecture.md",
            "docs/algorithms.md",
            "docs/future-custom-pieces.md",
            "docs/mvp-scope.md",
            "scripts/generic-exact-cover-dlx-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "G6 required generic exact-cover/DLX file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-exact-cover/src/model/exact_cover_problem.rs"
        Read-Text "crates/clearra-exact-cover/src/model/exact_cover_problem_schema.rs"
        Read-Text "crates/clearra-exact-cover/src/model/generic_exact_cover_candidate.rs"
        Read-Text "crates/clearra-exact-cover/src/solver/dlx_solver.rs"
        Read-Text "crates/clearra-exact-cover/src/bridge/generic_exact_cover_bridge.rs"
        Read-Text "crates/clearra-exact-cover/src/bridge/setup_tiling_bridge.rs"
        Read-Text "crates/clearra-build-coverage/src/exact_cover/build_exact_cover_problem.rs"
        Read-Text "crates/clearra-setup-search/src/exact_cover/setup_tiling_exact_cover.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/dlx_buildup_bridge.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/dlx_build_up_bridge.rs"
        Read-Text "docs/architecture.md"
        Read-Text "docs/algorithms.md"
        Read-Text "docs/future-custom-pieces.md"
        Read-Text "docs/mvp-scope.md"
        Read-Text "scripts/generic-exact-cover-dlx-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "ExactCoverProblemSchema",
            "cell_universe",
            "PieceUsageConstraint",
            "SlotConstraintColumn",
            "AreaConstraintColumn",
            "ExactCoverColumnKind::Required",
            "ExactCoverColumnKind::Optional",
            "ExactCoverCandidateRow",
            "with_optional_columns",
            "required_column_count",
            "optional_column_count",
            "DlxSearchLimits",
            "max_solutions",
            "max_nodes",
            "complete",
            "searched_nodes",
            "truncation_reason",
            "generic_exact_cover_candidate_schema_validates",
            "dlx_solver_returns_complete_flag",
            "area_infeasible_shape_rejected_before_search",
            "dlx_result_maps_to_buildup_problem",
            "standard_setup_tiling_still_works",
            "dlx_solution_is_not_build_variant",
            "BuildExactCoverProblemBridge",
            "SetupTilingExactCover",
            "DlxSolution -> operation candidates -> BuildUpProblem -> C BuildUp",
            "DLX solution is not a BuildVariant",
            "line clear, hold, queue, and reachability remain BuildUp responsibilities",
            "compile-and-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G6 generic exact-cover/DLX contract must expose marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "BuildVariant::from_dlx",
            "dlx_solution_as_build_variant",
            "line_clear_done_by_exact_cover",
            "hold_done_by_exact_cover",
            "queue_done_by_exact_cover",
            "reachability_done_by_exact_cover",
            "dlx_truncation_marked_complete",
            "truncated_dlx_complete_true"
        )) {
        if ($surface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "G6 must not introduce forbidden DLX shortcut marker '$forbiddenMarker'"
        }
    }
}
