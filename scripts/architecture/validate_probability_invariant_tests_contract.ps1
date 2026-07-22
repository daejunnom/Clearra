function Invoke-CoverageProbabilityInvariantTestsContractValidation() {
foreach ($requiredPath in @(
            "crates/clearra-coverage/src/coverage_contract_tests.rs",
            "crates/clearra-coverage/src/reducer/coverage_probability_reducer.rs",
            "crates/clearra-setup-search/src/coverage/setup_probability.rs",
            "crates/clearra-build-coverage/src/coverage/build_coverage_result.rs",
            "crates/clearra-core-executor/src/spin/spin_target_runner_tests.rs",
            "crates/clearra-objectives/src/reducer/objective_reducer.rs",
            "crates/clearra-output/tests/output_golden_contract_tests.rs",
            "crates/clearra-validation/src/validators/core_security_gate_tests.rs"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "T3 coverage probability invariant required file is missing: $requiredPath"
        }
    }
$coverageContractTests = Read-Text "crates/clearra-coverage/src/coverage_contract_tests.rs"
foreach ($requiredMarker in @(
            "coverage_row_rejects_universe_mismatch",
            "coverage_row_rejects_weight_model_mismatch",
            "coverage_row_rejects_piece_source_mismatch",
            "spin_probability_uses_pattern_bitset_union",
            "score_does_not_change_coverage_probability",
            "PatternBitSet",
            "TypedCoverageMatrix",
            "union_probability",
            "count_ones"
        )) {
        if ($coverageContractTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "coverage_contract_tests.rs must keep T3 invariant marker '$requiredMarker'"
        }
    }
$coverageReducer = Read-Text "crates/clearra-coverage/src/reducer/coverage_probability_reducer.rs"
foreach ($requiredMarker in @(
            "CoverageProbabilityReducer",
            "family_probability",
            "matrix.union_all",
            "union_probability",
            "coverage_union_does_not_sum_variant_probability",
            "family_probability_uses_pattern_bitset_or",
            "probability_never_exceeds_one"
        )) {
        if ($coverageReducer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "coverage_probability_reducer.rs must keep T3 union reducer marker '$requiredMarker'"
        }
    }
$setupProbability = Read-Text "crates/clearra-setup-search/src/coverage/setup_probability.rs"
foreach ($requiredMarker in @(
            "SetupProbability::from_union",
            "union_probability",
            "SetupUnionCoverage",
            "setup_probability_uses_pattern_bitset_union",
            "variant_probability_sum=forbidden"
        )) {
        if ($setupProbability -notlike "*$requiredMarker*") {
            Add-ArchitectureError "setup_probability.rs must keep T3 setup union marker '$requiredMarker'"
        }
    }
$buildCoverageResult = Read-Text "crates/clearra-build-coverage/src/coverage/build_coverage_result.rs"
foreach ($requiredMarker in @(
            "BuildCoverageResult::from_union",
            "BuildUnionCoverage",
            "union_probability",
            "build_coverage_uses_union_probability",
            "build_coverage_result_uses_union_probability"
        )) {
        if ($buildCoverageResult -notlike "*$requiredMarker*") {
            Add-ArchitectureError "build_coverage_result.rs must keep T3 build coverage union marker '$requiredMarker'"
        }
    }
$spinRunnerTests = Read-Text "crates/clearra-core-executor/src/spin/spin_target_runner_tests.rs"
foreach ($requiredMarker in @(
            "spin_probability_uses_pattern_bitset_union",
            "spin_probability_result_uses_union_probability",
            "covered_pattern_count",
            "probability_result"
        )) {
        if ($spinRunnerTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "spin_target_runner_tests.rs must keep T3 spin union marker '$requiredMarker'"
        }
    }
$objectiveReducer = Read-Text "crates/clearra-objectives/src/reducer/objective_reducer.rs"
foreach ($requiredMarker in @(
            "score_does_not_change_coverage_probability",
            "max_score_objective_reports_best_score_by_pattern_without_changing_coverage_probability",
            "coverage().probability",
            "max_score().covered_probability",
            "best_score_by_pattern_count"
        )) {
        if ($objectiveReducer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "objective_reducer.rs must keep T3 score/coverage separation marker '$requiredMarker'"
        }
    }
$outputGoldenTests = Read-Text "crates/clearra-output/tests/output_golden_contract_tests.rs"
foreach ($requiredMarker in @(
            "observed_queue_truncation_not_renormalized",
            "observed_queue_truncation_is_not_renormalized",
            "probability_complete",
            "materialized_probability_mass",
            "renormalized",
            "observed_queue_truncated"
        )) {
        if ($outputGoldenTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "output_golden_contract_tests.rs must keep T3 observed queue marker '$requiredMarker'"
        }
    }
$securityGateTests = Read-Text "crates/clearra-validation/src/validators/core_security_gate_tests.rs"
foreach ($requiredMarker in @(
            "observed_queue_truncation_not_renormalized",
            "observed_queue_truncation_is_not_renormalized",
            "WObservedQueueProbabilityIncomplete",
            "renormalized",
            "probability_complete"
        )) {
        if ($securityGateTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core_security_gate_tests.rs must keep T3 truncation diagnostic marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "T3 Coverage / Probability Invariant Tests",
            "PatternBitSet union",
            "probability never exceeds 1.0",
            "same pattern covered by multiple variants counted once",
            "score-aware objective does not modify probability"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document T3 marker '$requiredMarker'"
        }
    }
$testPolicyDoc = Read-Text "docs/test-policy.md"
foreach ($requiredMarker in @(
            "T3 coverage probability invariants",
            "coverage_row_rejects_universe_mismatch",
            "coverage_row_rejects_piece_source_mismatch",
            "build_coverage_uses_union_probability",
            "observed_queue_truncation_not_renormalized"
        )) {
        if ($testPolicyDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/test-policy.md must document T3 marker '$requiredMarker'"
        }
    }
}
