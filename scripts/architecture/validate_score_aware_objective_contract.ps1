# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# X4 keeps score-aware objectives layered over coverage probability invariants.

function Invoke-ScoreAwareObjectiveContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-scoring/src/stats/per_build_variant_score.rs",
            "crates/clearra-scoring/src/stats/per_candidate_score_expectation.rs",
            "crates/clearra-scoring/src/stats/average_score_report.rs",
            "crates/clearra-scoring/src/stats/mod.rs",
            "crates/clearra-scoring/src/model/score_model_evaluator.rs",
            "crates/clearra-objectives/src/max_score/max_score_cover.rs",
            "crates/clearra-objectives/src/max_score/max_score_selection.rs",
            "crates/clearra-objectives/src/max_score/materialized_score_matrix.rs",
            "crates/clearra-objectives/src/max_score/scored_coverage_candidate.rs",
            "crates/clearra-objectives/src/max_score/score_aware_objective_invariant.rs",
            "crates/clearra-objectives/src/reducer/objective_reducer.rs",
            "crates/clearra-postprocess/src/score_batch/score_matrix.rs",
            "crates/clearra-postprocess/src/score_batch/candidate_execution_aggregate.rs",
            "crates/clearra-postprocess/src/pc_scoring/pc_scoring_postprocessor.rs",
            "crates/clearra-core-executor/src/buildup/buildup_objective_bridge.rs",
            "crates/clearra-coverage/src/matrix/score_cell_matrix.rs",
            "crates/clearra-output/src/scoring/score_aware_objective_output_contract.rs",
            "crates/clearra-output/src/json/pc_json_contract.rs",
            "scripts/score-aware-objective-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            Add-ArchitectureError "X4 required score-aware objective file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-scoring/src/stats/per_build_variant_score.rs"
        Read-Text "crates/clearra-scoring/src/stats/per_candidate_score_expectation.rs"
        Read-Text "crates/clearra-scoring/src/stats/average_score_report.rs"
        Read-Text "crates/clearra-scoring/src/stats/mod.rs"
        Read-Text "crates/clearra-scoring/src/model/candidate_score_stats.rs"
        Read-Text "crates/clearra-scoring/src/model/pattern_score_contribution.rs"
        Read-Text "crates/clearra-scoring/src/model/score_model_evaluator.rs"
        Read-Text "crates/clearra-scoring/src/model/spin_interpretation_evaluator.rs"
        Read-Text "crates/clearra-objectives/src/max_score/max_score_cover.rs"
        Read-Text "crates/clearra-objectives/src/max_score/max_score_selection.rs"
        Read-Text "crates/clearra-objectives/src/max_score/materialized_score_matrix.rs"
        Read-Text "crates/clearra-objectives/src/max_score/scored_coverage_candidate.rs"
        Read-Text "crates/clearra-objectives/src/max_score/score_aware_objective_invariant.rs"
        Read-Text "crates/clearra-objectives/src/reducer/objective_reducer.rs"
        Read-Text "crates/clearra-postprocess/src/score_batch/score_matrix.rs"
        Read-Text "crates/clearra-postprocess/src/score_batch/candidate_execution_aggregate.rs"
        Read-Text "crates/clearra-postprocess/src/pc_scoring/pc_scoring_postprocessor.rs"
        Read-Text "crates/clearra-core-executor/src/buildup/buildup_objective_bridge.rs"
        Read-Text "crates/clearra-coverage/src/matrix/score_cell_matrix.rs"
        Read-Text "crates/clearra-output/src/scoring/score_aware_objective_output_contract.rs"
        Read-Text "crates/clearra-output/src/json/pc_json_contract.rs"
        Read-Text "crates/clearra-output/src/json/json_contract_tests.rs"
        Read-Text "scripts/score-aware-objective-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "PerBuildVariantScore",
            "PerCandidateConditionalAverage",
            "PerCandidateUnconditionalExpectation",
            "CandidateScoreStats",
            "PatternScoreContribution",
            "AverageScoreReport",
            "score_profile_evaluates_replay",
            "evaluate_replay_trace",
            "MaxScoreCover",
            "MaterializedScoreMatrix",
            "CandidateExecutionAggregate",
            "ScoreCell",
            "trace_identity",
            "select_matrix",
            "score_matrix_not_materialized",
            "connected-approximate",
            "connected-exact",
            "not_requested",
            "ScoreCellMatrix",
            "pattern_id",
            "best_score_by_pattern",
            "max_score_cover_uses_best_score_by_pattern",
            "max_score_cover_selects_best_score_by_pattern",
            "score_aware_cover_selects_best_candidate_per_pattern",
            "score_probability_no_double_count",
            "score_does_not_change_probability_union",
            "score_does_not_modify_coverage_probability",
            "objective_score_does_not_modify_coverage_probability",
            "spin_award_profile_separate_from_score_profile",
            "coverage_probability_before_scoring",
            "coverage_probability_after_scoring",
            "sample_vs_full_evaluation_distinguished",
            "retained_trace_sample_is_not_reported_as_full_expected_score",
            "score_output_states_accuracy_level",
            "ScoreAwareObjectiveInvariantReport",
            "ScoreAwareObjectiveOutputContract"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X4 score-aware objective contract must expose marker '$requiredMarker'"
        }
    }
$maxScoreCover = Read-Text "crates/clearra-objectives/src/max_score/max_score_cover.rs"
foreach ($forbiddenMarker in @(
            "sum_variant_probability",
            "variant_probability_sum",
            "CoverageRow::new",
            "TypedCoverageMatrix::new"
        )) {
        if ($maxScoreCover -like "*$forbiddenMarker*") {
            Add-ArchitectureError "X4 MaxScoreCover must not reinterpret CoverageRow or sum variant probabilities marker '$forbiddenMarker'"
        }
    }
$objectiveBridge = Read-Text "crates/clearra-core-executor/src/buildup/buildup_objective_bridge.rs"
if ($objectiveBridge -like "*ObjectiveCandidate::new*") {
        Add-ArchitectureError "X4 product buildup objective bridge must not synthesize score/attack values"
    }
if ($objectiveBridge -notlike "*ObjectiveCandidate::unscored*") {
        Add-ArchitectureError "X4 product buildup objective bridge must preserve the unscored state until PostProcess"
    }
$postProcessor = Read-Text "crates/clearra-postprocess/src/pc_scoring/pc_scoring_postprocessor.rs"
foreach ($requiredMarker in @(
            "ScoreMatrix::materialize",
            "MaxScoreCover::select_matrix",
            "score_matrix_not_materialized",
            "objective_max_score_accuracy_level",
            "score_does_not_change_probability_union"
        )) {
        if ($postProcessor -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X4 product postprocess must materialize and guard max-score marker '$requiredMarker'"
        }
    }
$processE2E = Read-Text "crates/clearra-cli/tests/process_e2e.rs"
foreach ($runtimeMarker in @(
            "--objective",
            "max-score-cover",
            "objective_max_score_cover: connected-approximate",
            "score_matrix_complete: true"
        )) {
        if ($processE2E -notlike "*$runtimeMarker*") {
            Add-ArchitectureError "X4 process E2E must exercise materialized max-score behavior marker '$runtimeMarker'"
        }
    }
$scoringStats = @(
        Read-Text "crates/clearra-scoring/src/stats/per_build_variant_score.rs"
        Read-Text "crates/clearra-scoring/src/stats/per_candidate_score_expectation.rs"
        Read-Text "crates/clearra-scoring/src/stats/average_score_report.rs"
    ) -join "`n"
if ($scoringStats -notlike "*RetainedTraceSample*" -or $scoringStats -notlike "*FullPatternUniverseExpected*") {
        Add-ArchitectureError "X4 score stats must distinguish retained sample and full universe evaluation scopes"
    }
}
