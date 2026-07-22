# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# G5 keeps area decomposition as a necessary-condition guard only.

function Invoke-AreaMultisetFeasibilityContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-geometry/src/area/standard_tetromino_area_rule.rs",
            "crates/clearra-geometry/src/area/area_multiset_feasibility.rs",
            "crates/clearra-geometry/src/area/area_scope.rs",
            "crates/clearra-core-executor/src/area/area_tileability.rs",
            "crates/clearra-core-executor/src/area/scenario_area_pruner.rs",
            "crates/clearra-exact-cover/src/area/area_multiset_bridge.rs",
            "crates/clearra-validation/src/validators/area_feasibility_validator.rs",
            "crates/clearra-problem/src/compile/area_pruner.rs",
            "docs/architecture.md",
            "docs/algorithms.md",
            "docs/future-custom-pieces.md",
            "docs/mvp-scope.md",
            "scripts/area-multiset-feasibility-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "G5 required area multiset feasibility file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-geometry/src/area/standard_tetromino_area_rule.rs"
        Read-Text "crates/clearra-geometry/src/area/area_multiset_feasibility.rs"
        Read-Text "crates/clearra-geometry/src/area/area_scope.rs"
        Read-Text "crates/clearra-core-executor/src/area/area_tileability.rs"
        Read-Text "crates/clearra-core-executor/src/area/scenario_area_pruner.rs"
        Read-Text "crates/clearra-exact-cover/src/area/area_multiset_bridge.rs"
        Read-Text "crates/clearra-validation/src/validators/area_feasibility_validator.rs"
        Read-Text "crates/clearra-problem/src/compile/area_pruner.rs"
        Read-Text "docs/architecture.md"
        Read-Text "docs/algorithms.md"
        Read-Text "docs/future-custom-pieces.md"
        Read-Text "docs/mvp-scope.md"
        Read-Text "scripts/area-multiset-feasibility-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "StandardTetrominoAreaRule",
            "AreaMultisetFeasibility",
            "active_piece_area_multiset",
            "bounded_area_subset_sum",
            "AreaScopeDescriptor",
            "TargetRows",
            "InterpretedTargetCells",
            "WholeBoardTarget",
            "scenario_area_pruner_requires_explicit_area_scope",
            "area_decomposition_is_necessary_condition_not_solver",
            "standard_area4_fast_path_unchanged",
            "area_multiset_feasibility_uses_piece_area_multiset",
            "AreaFeasibilityValidator",
            "EAreaInfeasible",
            "IAreaNecessaryConditionPassed",
            "CompileAreaPruner",
            "SearchMayContinue",
            "area_feasible_is_solution_found",
            "compile-and-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G5 area multiset feasibility contract must expose marker '$requiredMarker'"
        }
    }
$sourceOnlySurface = @(
        Read-Text "crates/clearra-geometry/src/area/area_multiset_feasibility.rs"
        Read-Text "crates/clearra-core-executor/src/area/area_tileability.rs"
        Read-Text "crates/clearra-exact-cover/src/area/area_multiset_bridge.rs"
        Read-Text "crates/clearra-validation/src/validators/area_feasibility_validator.rs"
        Read-Text "crates/clearra-problem/src/compile/area_pruner.rs"
    ) -join "`n"
if ($sourceOnlySurface -match "missing_cells\s*%\s*4" -or
        $sourceOnlySurface -match "missing_cells\.rem_euclid\(\s*4\s*\)") {
        Add-ArchitectureError "G5 generic area feasibility must not decide feasibility with missing_cells % 4"
    }
foreach ($forbiddenMarker in @(
            "whole_board_empty_sky_default_target",
            "area_feasible_solution_found_true",
            "area_feasible_means_solution_found"
        )) {
        if ($sourceOnlySurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "G5 must not treat area feasibility as solver success marker '$forbiddenMarker'"
        }
    }
}
