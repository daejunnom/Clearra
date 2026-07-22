# This file is dot-sourced by Invoke-WorkspaceSurfaceArchitectureValidation.
# It intentionally contains ordered validation statements, not a standalone entrypoint.

$setupSearchCargo = Read-Text "crates/clearra-setup-search/Cargo.toml"
$setupSearchDoc = Read-Text "docs/setup-search.md"
foreach ($crateName in @("clearra-pc-graph", "clearra-problem", "clearra-core-executor", "clearra-scoring", "clearra-objectives", "clearra-rules")) {
    if (-not (Test-DependencyLine $setupSearchCargo $crateName)) {
        Add-ArchitectureError "clearra-setup-search MVP2 must depend on $crateName for scenario post-PC and score-aware setup aggregation"
    }
}
foreach ($requiredMarker in @("execution_scope=mvp2", "queue-pattern-shape-tiling-build-post-pc", "post_pc_mode=scenario-clear-to-empty", "post_pc_evaluation_attached=true", "PatternBitSet union coverage", "Score aggregation must never sum duplicate variant probabilities")) {
    if ($setupSearchDoc -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/setup-search.md must disclose setup MVP2 execution marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("shape_family_id", "tiling_variant_count", "build_variant_count", "covered_pattern_count", "coverage_probability", "post_pc_solution_count", "score_basis", "backend_report", "raw_coverage_export_path", "setup explorer schema consumes these fields")) {
    if ($setupSearchDoc -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/setup-search.md must disclose X3 setup raw metric marker '$requiredMarker'"
    }
}
$setupRawMetrics = Read-Text "crates/clearra-setup-search/src/evaluate/setup_raw_metrics.rs"
foreach ($requiredMarker in @("queue_prefix", "hold_required", "bag_boundary_offsets", "Requires180Evidence", "RuleProfileEvidence", "not-modeled", "default-rule-profile", "post_pc_rule_profile", "requires_180_evidence", "rule_profile_evidence", "post_pc_solution_found", "setup_raw_metrics_reports_queue_hold_boundary_rule_180_and_post_pc")) {
    if ($setupRawMetrics -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SetupRawMetrics must expose MVP2 setup raw metrics marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("rule_profile_evidence: RuleProfileId", "requires_180: bool", "rule_profile_evidence = RuleProfileId::SrsPlus")) {
    if ($setupRawMetrics -like "*$forbiddenMarker*") {
        Add-ArchitectureError "SetupRawMetrics must not collapse unmodeled setup rule/180 dependency into hard-coded marker '$forbiddenMarker'"
    }
}
$postPcEvaluator = Read-Text "crates/clearra-setup-search/src/evaluate/post_pc_evaluator.rs"
$postPcScenarioInput = Read-Text "crates/clearra-setup-search/src/evaluate/post_pc_scenario_input.rs"
$postPcEvaluationSummary = Read-Text "crates/clearra-setup-search/src/evaluate/post_pc_evaluation_summary.rs"
$postPcScoreSummary = Read-Text "crates/clearra-setup-search/src/evaluate/post_pc_score_summary.rs"
$postPcScoreEvaluator = Read-Text "crates/clearra-setup-search/src/evaluate/post_pc_score_evaluator.rs"
$postPcContinuationStatus = Read-Text "crates/clearra-setup-search/src/evaluate/post_pc_continuation_status.rs"
$postPcErrorReason = Read-Text "crates/clearra-setup-search/src/evaluate/post_pc_error_reason.rs"
foreach ($requiredMarker in @("PostPcScenarioInput", "PcScenarioQuery", "ProblemCompiler::compile_scenario_pc", "CoreExecutor::execute", "evaluate_query_with_score_profile", "PostPcScoreEvaluator::score_retained_traces", "PostPcEvaluationSummary::from_query_result", "post_pc_evaluator_runs_clear_to_empty_scenario_query_from_setup_variant", "post_pc_score_summary_discloses_sample_basis_when_retained_traces_are_limited", "post_pc_continuation_uses_actual_consumed_pieces_not_max_window")) {
    if ($postPcEvaluator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PostPcEvaluator must stay a thin scenario ProblemCompiler/CoreExecutor facade marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("ScoreModelEvaluator", "ScoreEvaluationSummary::new", "best_remaining_queue_len() >= query.min_remaining_queue()", "fn scenario_error_reason")) {
    if ($postPcEvaluator -like "*$forbiddenMarker*") {
        Add-ArchitectureError "PostPcEvaluator must delegate score/continuation/error detail marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @("PostPcScenarioInput", "PcScenarioQuery", "PcCompletionGoal::ClearToEmpty", "effective_rule_profile_id", "requires_180_modeled", "into_query")) {
    if ($postPcScenarioInput -notlike "*$requiredMarker*") {
        Add-ArchitectureError "post_pc_scenario_input.rs must own post-PC scenario input-to-query contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PostPcEvaluationSummary", "from_query_result", "PostPcContinuationStatus::from_query_result", "min_queue_consumed", "max_queue_consumed", "sample_queue_consumed", "placed_piece_count", "best_remaining_queue_len", "continuation_available", "continuation_available_complete", "score_evaluation_trace_count", "score_evaluation_complete", "score_evaluation_basis")) {
    if ($postPcEvaluationSummary -notlike "*$requiredMarker*") {
        Add-ArchitectureError "post_pc_evaluation_summary.rs must own post-PC result summary marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PostPcScoreSummary", "ScoreEvaluationSummary", "ScoreEvaluationBasis", "score_evaluation_trace_count", "score_evaluation_complete", "score_evaluation_basis")) {
    if ($postPcScoreSummary -notlike "*$requiredMarker*") {
        Add-ArchitectureError "post_pc_score_summary.rs must own post-PC score summary marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PostPcScoreEvaluator", "score_retained_traces", "CoreExecutionResult", "ScoreEvaluationSummary::new", "ScoreEvaluationBasis::AllTraces", "ScoreEvaluationBasis::Sample", "ScoreEvaluationBasis::RetainedTraces", "trace_retention_truncated", "total_solution_count")) {
    if ($postPcScoreEvaluator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "post_pc_score_evaluator.rs must own retained trace score evaluation marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PostPcContinuationStatus", "from_query_result", 'usize_field("best_remaining_queue_len")', "query.min_remaining_queue()", 'bool_field("count_complete")', "|| available")) {
    if ($postPcContinuationStatus -notlike "*$requiredMarker*") {
        Add-ArchitectureError "post_pc_continuation_status.rs must own continuation availability marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("scenario_error_reason", "CoreExecutionError", "UnsupportedProblem")) {
    if ($postPcErrorReason -notlike "*$requiredMarker*") {
        Add-ArchitectureError "post_pc_error_reason.rs must own post-PC error reason marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("preflight_unsupported_reason", "observed scenario queues must be expanded before post-PC evaluation", "bag-aligned scenario patterns must be expanded before post-PC evaluation", "empty scenario queue")) {
    if ($postPcEvaluator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PostPcEvaluator must own pre-executor scenario input guard marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("PcTarget", "CheckpointDag", "OpeningPcSearchQuery")) {
    if ($postPcEvaluator -like "*$forbiddenMarker*") {
        Add-ArchitectureError "PostPcEvaluator must not route setup completion through opening PC concepts marker '$forbiddenMarker'"
    }
}
foreach ($forbiddenMarker in @(".saturating_sub(query.piece_window().max_pieces())", "remaining_queue.len() - piece_window.max_pieces()", "remaining_queue.len().saturating_sub(piece_window.max_pieces())")) {
    if ($postPcEvaluator -like "*$forbiddenMarker*") {
        Add-ArchitectureError "PostPcEvaluator continuation availability must use actual solver consumed-piece results, not max window marker '$forbiddenMarker'"
    }
}
$setupScoreAggregation = @(
    Read-Text "crates/clearra-setup-search/src/result/setup_score_aggregation.rs"
    Read-Text "crates/clearra-setup-search/src/result/setup_score_input.rs"
    Read-Text "crates/clearra-setup-search/src/result/setup_build_score.rs"
    Read-Text "crates/clearra-setup-search/src/result/setup_score_hierarchy.rs"
    Read-Text "crates/clearra-setup-search/src/result/setup_score_aggregation_tests.rs"
) -join "`n"
foreach ($requiredMarker in @("SetupScoreAggregation", "SetupBuildScoreInput", "SetupFamilyScore", "SetupTilingScore", "SetupBuildScore", "MaxScoreCover::select", "post_pc_probability", "expected_score", "expected_attack", "total_solution_count", "score_evaluation_trace_count", "score_evaluation_complete", "score_evaluation_basis", "continuation_available", "continuation_available_complete", "setup_score_aggregation_preserves_family_tiling_build_layers", "setup_score_aggregation_does_not_double_count_duplicate_pattern_probability", "setup_score_aggregation_discloses_sample_basis_when_any_build_is_sampled")) {
    if ($setupScoreAggregation -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SetupScoreAggregation must preserve setup layers and union probability marker '$requiredMarker'"
    }
}
if ($setupScoreAggregation -like "*CoverageMatrix*") {
    Add-ArchitectureError "SetupScoreAggregation must aggregate from build candidates, not by adding score metadata to CoverageMatrix"
}

$buildEditor = Read-Text "crates/clearra-ui-schema/src/build_editor/build_editor_schema.rs"
$buildSlotSchema = Read-Text "crates/clearra-ui-schema/src/build_editor/build_slot_schema.rs"
foreach ($requiredMarker in @("BuildTemplate", "from_template", "custom_domains_enabled: true", "BuildFieldSchema", "build_editor_schema_exposes_template_and_slot_field_schema")) {
    if ($buildEditor -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildEditorSchema must expose MVP2 build template field schema from canonical BuildTemplate marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("from_build_slot", "cells", "allowed_pieces", "required_piece", "hold_constraint", "order_constraint", "symmetry", "canonicalization", "BuildFieldSchema")) {
    if ($buildSlotSchema -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildSlotSchema must expose editable MVP2 build slot fields marker '$requiredMarker'"
    }
}

$setupFilterSchema = Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_filter_schema.rs"
foreach ($requiredMarker in @("GroupingMode::MVP1_SUPPORTED", "SetupLimits::from(SearchDefaults::MVP1)", "setup_filter_schema_uses_canonical_grouping_modes_and_limits")) {
    if ($setupFilterSchema -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SetupFilterSchema must derive setup explorer defaults from setup-search/profiles canonical marker '$requiredMarker'"
    }
}
$setupExplorerSchema = @(
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_explorer_schema.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_result_columns.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_explorer_schema_tests.rs"
) -join "`n"
foreach ($requiredMarker in @("scenario_fixtures", "tests/fixtures/pc/example.json", "tests/fixtures/pc/requires_180_unsupported.json", "scenario_requires_180_unsupported", "scenario_result_columns", "total_solution_count", "count_mode", "count_requested", "count_complete", "solution_trace_mode", "backend_selection_reason", "state_count_available", "state_count", "multiplicity_count_available", "multiplicity_count", "min_queue_consumed", "max_queue_consumed", "sample_queue_consumed", "placed_piece_count", "best_remaining_queue_len", "retained_trace_limit", "retained_trace_count", "trace_retention_truncated", "trace_retention_reason", "score_evaluation_trace_count", "score_evaluation_complete", "score_evaluation_basis", "next_pc_available", "next_pc_candidate", "continuation_token_available", "continuation_token_unavailable_reason", "continuation_basis", "continuation_queue_consumed", "continue_available", "continuation_available_complete", "continuation_token", "scenario_replay_token", "setup_explorer_schema_selects_pc_scenario_fixtures_with_disabled_reason")) {
    if ($setupExplorerSchema -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SetupExplorerSchema must expose GUI scenario fixture/result contract marker '$requiredMarker'"
    }
}
foreach ($forbiddenLiteral in @('"shape-family"', '"tiling"', '"tiling-variant"', '"build-variant"', "max_results: 256")) {
    if ($setupFilterSchema -like "*$forbiddenLiteral*") {
        Add-ArchitectureError "SetupFilterSchema must not hard-code setup grouping/default literal $forbiddenLiteral"
    }
}

Invoke-CliCommandSurfaceArchitectureValidation
