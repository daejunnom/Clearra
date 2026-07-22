# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# X3 keeps SpinTarget probability on BuildVariant replay evidence and PatternBitSet union.

function Invoke-SpinTargetContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-problem/src/query/spin_target_query.rs",
            "crates/clearra-problem/src/compile/spin_target_compiler.rs",
            "crates/clearra-core-executor/src/spin/spin_target_runner.rs",
            "crates/clearra-core-executor/src/spin/spin_input_from_replay.rs",
            "crates/clearra-core-executor/src/spin/build_variant_mapper.rs",
            "crates/clearra-core-executor/src/spin/spin_target_coverage_bridge.rs",
            "crates/clearra-core-executor/src/spin/spin_target_result_reducer.rs",
            "crates/clearra-core-executor/src/spin/spin_target_execution_report.rs",
            "crates/clearra-core-executor/src/spin/spin_target_runner_tests.rs",
            "crates/clearra-scoring/src/spin/spin_target_predicate.rs",
            "crates/clearra-scoring/src/spin/spin_classifier.rs",
            "crates/clearra-scoring/src/spin/t_spin_corner_rule.rs",
            "crates/clearra-scoring/src/spin/all_spin_rule.rs",
            "crates/clearra-scoring/src/spin/all_mini_rule.rs",
            "crates/clearra-scoring/src/spin/kick_sensitive_spin_rule.rs",
            "crates/clearra-scoring/src/spin/special_spin_case_registry.rs",
            "crates/clearra-coverage/src/matrix/spin_coverage_matrix.rs",
            "crates/clearra-output/src/spin/spin_target_output_contract.rs",
            "crates/clearra-output/src/json/pc_json_contract.rs",
            "scripts/spin-target-contract-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            Add-ArchitectureError "X3 required spin target contract file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-problem/src/query/spin_target_query.rs"
        Read-Text "crates/clearra-problem/src/compile/spin_target_compiler.rs"
        Read-Text "crates/clearra-core-executor/src/spin/spin_target_runner.rs"
        Read-Text "crates/clearra-core-executor/src/spin/spin_input_from_replay.rs"
        Read-Text "crates/clearra-core-executor/src/spin/build_variant_mapper.rs"
        Read-Text "crates/clearra-core-executor/src/spin/spin_target_coverage_bridge.rs"
        Read-Text "crates/clearra-core-executor/src/spin/spin_target_result_reducer.rs"
        Read-Text "crates/clearra-core-executor/src/spin/spin_target_execution_report.rs"
        Read-Text "crates/clearra-core-executor/src/spin/spin_target_runner_tests.rs"
        Read-Text "crates/clearra-scoring/src/spin/spin_target_predicate.rs"
        Read-Text "crates/clearra-scoring/src/spin/spin_classifier.rs"
        Read-Text "crates/clearra-scoring/src/spin/t_spin_corner_rule.rs"
        Read-Text "crates/clearra-scoring/src/spin/all_spin_rule.rs"
        Read-Text "crates/clearra-scoring/src/spin/all_mini_rule.rs"
        Read-Text "crates/clearra-scoring/src/spin/kick_sensitive_spin_rule.rs"
        Read-Text "crates/clearra-scoring/src/spin/special_spin_case_registry.rs"
        Read-Text "crates/clearra-coverage/src/matrix/spin_coverage_matrix.rs"
        Read-Text "crates/clearra-output/src/spin/spin_target_output_contract.rs"
        Read-Text "crates/clearra-output/src/json/pc_json_contract.rs"
        Read-Text "scripts/spin-target-contract-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "SpinTargetRequest",
            "SpinTargetPredicate",
            "target_probability_threshold",
            "score_profile_id",
            "SpinTargetTraceRequirement",
            "trace_requirement",
            "spin_target_query_compiles_to_search_problem",
            "SpinClassifier",
            "TSpinCornerRule",
            "AllSpinRule",
            "AllMiniRule",
            "KickSensitiveSpinRule",
            "SpecialSpinCaseRegistry",
            "board_before",
            "board_after_placement",
            "board_after_clear",
            "piece",
            "rotation",
            "cleared_lines",
            "kick_evidence",
            "trace_completeness",
            "BuildVariantMapper::to_replay_trace",
            "spin_input_from_replay",
            "SpinTargetCoverageBridge::row_from_build_variant",
            "CoverageRowKind::SpinTarget",
            "SpinTargetResultReducer::reduce_uniform",
            "PatternBitSet",
            "union_probability",
            "spin_target_predicate_applies_after_replay_before_coverage_row",
            "kick_evidence_flows_from_build_variant_to_spin_classifier",
            "missing_kick_evidence_is_incomplete_not_exact_spin",
            "W_SPIN_TARGET_PROBABILITY_INCOMPLETE",
            "probability_complete",
            "exact",
            "spin_probability_uses_pattern_bitset_union",
            "spin_target_runner_rejects_missing_spin_classifier",
            "SpinTargetOutputContract",
            "compile-and-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X3 spin target/classifier/kick evidence contract must expose marker '$requiredMarker'"
        }
    }
$runner = Read-PhysicalText "crates/clearra-core-executor/src/spin/spin_target_runner.rs"
foreach ($forbiddenMarker in @(
            "CPackingCandidate",
            "PackingCandidate",
            "packing_candidate"
        )) {
        if ($runner -like "*$forbiddenMarker*") {
            Add-ArchitectureError "X3 SpinTargetRunner must not classify spin from PackingCandidate marker '$forbiddenMarker'; use BuildVariant replay evidence"
        }
    }
$reducer = Read-Text "crates/clearra-core-executor/src/spin/spin_target_result_reducer.rs"
if ($reducer -like "*variant_probability*" -or $reducer -like "*sum_variant*") {
        Add-ArchitectureError "X3 SpinTargetResultReducer must not sum variant probabilities; use PatternBitSet OR union"
    }
}
