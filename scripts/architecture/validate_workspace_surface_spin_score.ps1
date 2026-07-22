# Ordered SRP validators share the caller function scope.

$scoringCargo = Read-Text "crates/clearra-scoring/Cargo.toml"
$scoreProfile = Read-Text "crates/clearra-scoring/src/profile/score_profile.rs"
$scoreProfileRegistry = Read-Text "crates/clearra-scoring/src/profile/score_profile_registry.rs"
$scoreEvent = Read-Text "crates/clearra-scoring/src/event/score_event.rs"
$spinDetector = Read-Text "crates/clearra-scoring/src/event/spin_detector.rs"
$kickSensitiveSpinRule = Read-Text "crates/clearra-scoring/src/spin/kick_sensitive_spin_rule.rs"
$specialSpinCase = Read-Text "crates/clearra-scoring/src/spin/special_spin_case.rs"
$specialSpinCaseRegistry = Read-Text "crates/clearra-scoring/src/spin/special_spin_case_registry.rs"
$solutionTraceEvents = Read-Text "crates/clearra-scoring/src/trace/solution_trace_events.rs"
$scoreEvaluation = Read-Text "crates/clearra-scoring/src/model/score_evaluation.rs"
$scoreModelEvaluator = Read-Text "crates/clearra-scoring/src/model/score_model_evaluator.rs"
$scoreTable = Read-Text "crates/clearra-scoring/src/model/score_table.rs"
$attackModelEvaluator = Read-Text "crates/clearra-scoring/src/model/attack_model_evaluator.rs"
$scoreProfileImport = Read-Text "crates/clearra-scoring/src/import/score_profile_import.rs"
$scoreProfileExport = Read-Text "crates/clearra-scoring/src/export/score_profile_export.rs"
$scoreProfileValidator = Read-Text "crates/clearra-validation/src/validators/score_profile_validator.rs"
$spinTargetValidator = Read-Text "crates/clearra-validation/src/validators/spin_target_validator.rs"
$specialSpinProfileValidator = Read-Text "crates/clearra-validation/src/validators/special_spin_profile_validator.rs"
$scoreProfileObjectValidator = @(
    Read-Text "crates/clearra-validation/src/validators/score_profile_object_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/score_profile_object_diagnostic_builder.rs"
    Read-Text "crates/clearra-validation/src/validators/score_profile_object_field_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/score_profile_object_policy_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/score_profile_object_registry_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/score_profile_object_validator_tests.rs"
) -join "`n"
$spinClassifierContractTests = Read-Text "crates/clearra-scoring/tests/spin_classifier_contract.rs"
$specialSpinCaseContractTests = Read-Text "crates/clearra-scoring/tests/special_spin_case_contract.rs"
$scoreExpectationScopeContractTests = Read-Text "crates/clearra-scoring/tests/score_expectation_scope_contract.rs"
$scoreProfileObjectValidatorContractTests = Read-Text "crates/clearra-scoring/tests/score_profile_object_validator_contract.rs"
$spinTargetRunnerTests = Read-Text "crates/clearra-core-executor/src/spin/spin_target_runner_tests.rs"
$scoringDoc = Read-Text "docs/scoring.md"
$scoringProfilesDoc = Read-Text "docs/scoring-profiles.md"
$buildCoverageDoc = Read-Text "docs/build-coverage.md"
$rulesAndKicksDocForSpin = Read-Text "docs/rules-and-kicks.md"
$buildupDocForSpin = Read-Text "docs/buildup.md"
$algorithmsDocForSpin = Read-Text "docs/algorithms.md"
$gpuPipelineDocForSpin = Read-Text "docs/gpu-pipeline.md"
$outputFormatsDocForSpin = Read-Text "docs/output-formats.md"
$diagnosticsDocForSpin = Read-Text "docs/diagnostics.md"
$mvpScopeDocForSpin = Read-Text "docs/mvp-scope.md"
$architectureDocForSpin = Read-Text "docs/architecture.md"
if (-not (Test-DependencyLine $scoringCargo "clearra-replay")) {
    Add-ArchitectureError "clearra-scoring must depend on clearra-replay so scoring interprets SolutionTrace as a post-processing layer"
}
if (Test-DependencyLine $scoringCargo "clearra-search") {
    Add-ArchitectureError "clearra-scoring must not depend on a search implementation crate; use clearra-replay trace contracts"
}
foreach ($requiredMarker in @("ScoreModelId", "AttackModelId", "SpinRuleId", "TSpinCornerBased", "ScoringAccuracyLevel", "BASIC_APPROXIMATION_REASON", "profile_specific_exact", "accuracy_reason", "ComboPolicy", "B2BPolicy", "SpinAwardPolicy", "AllSpinScoreMapping", "DropScorePolicy", "profile_ids_and_models_are_stable_contracts", "scoring_accuracy_level_parses_stable_contract_strings")) {
    if ($scoreProfile -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreProfile must own MVP2 score/attack/spin/combo/B2B profile contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("jstris_ultra", "ppt_profile", "tetrio_score_with_spin_profile", "SpinProfileRegistry", "ScoreProfileRegistry", "builtin_profiles_disclose_basic_approximation_accuracy", "tetrio_default_and_all_spin_options_are_selectable", "tetrio_profile_disables_all_spin_by_default")) {
    if ($scoreProfileRegistry -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreProfileRegistry must expose canonical builtin score profiles marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("ScoreEvent", "ClearEvent", "SpinEvent", "SpinDetector", "ComboState", "b2b_before", "b2b_after", "score_event_extracts_perfect_clear_and_combo_state_from_step")) {
    if ($scoreEvent -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreEvent must extract line clear, perfect clear, spin, combo, and B2B state marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("SpinDetector", "SpinClassifier", "detect_with_classifier", "SpinClassificationInput", "TSpinSimple", "TSpinCornerBased", "board_before", "after_placement", "t_center_for_step", "corner_is_blocked", "spin_detector_can_delegate_to_spin_classifier", "corner_based_t_spin_requires_three_blocked_corners", "corner_based_t_spin_does_not_flag_open_t_line_clear")) {
    if ($spinDetector -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpinDetector must delegate profile-aware spin detection through SpinClassifier while keeping score-event entrypoints marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("SolutionTrace", "SolutionTraceEvents", "from_trace", "solution_trace_events_extract_line_clear_and_perfect_clear_sequence")) {
    if ($solutionTraceEvents -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-scoring must adapt search SolutionTrace into score events marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("ScoreEvaluationSummary", "ScoreEvaluationBasis", "evaluated_trace_count", "evaluation_complete", "evaluation_basis", "retained-traces", "all-traces", "sample", "score_evaluation_summary_discloses_trace_basis_and_completeness")) {
    if ($scoreEvaluation -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreEvaluationSummary must disclose scoring aggregation trace basis marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("GuidelineScoreTable", "JstrisUltraScoreTable", "TetrioScoreTable", "ScoreModelTable", "SOURCE_NOTE", "quad=800", "tetrio_score_table_matches_source_pinned_values", "score_model_tables_are_profile_specific", "score_model_tables_score_t_spins_separately_from_line_clears")) {
    if ($scoreTable -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreModelTable must split MVP2 profile-specific basic score tables marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("evaluate_trace", "ScoreEvaluation", "ScoreModelTable", "AttackModelEvaluator::evaluate_event", "replay_drop_score", "HardDrop2SoftDrop1", "score_model_evaluator_scores_solution_trace_as_post_processing", "score_model_evaluator_uses_profile_specific_score_tables", "score_model_evaluator_adds_hard_drop_2_soft_drop_1_from_replay_drop_events")) {
    if ($scoreModelEvaluator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreModelEvaluator must evaluate whole traces as post-processing marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("evaluate_event", "combo_attack_bonus", "attack_model_adds_pc_combo_and_b2b_bonuses_from_score_event")) {
    if ($attackModelEvaluator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "AttackModelEvaluator must consume ScoreEvent and profile policy marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("from_json", "UnknownScoringField", "UnsupportedSpinRule", "UnsupportedAccuracyLevel", "accuracy_level", "profile_specific_exact", "reject_profile_specific_exact", "InvalidComboSetting", "InvalidB2BSetting", "imported_score_profile_rejects_unknown_scoring_field", "imported_score_profile_rejects_unsupported_spin_rule_and_invalid_policies")) {
    if ($scoreProfileImport -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreProfileImport must own strict MVP2 JSON validation marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("to_json", "accuracy_level", "profile_specific_exact", "accuracy_reason", "score_profile_export_roundtrips_through_import_contract")) {
    if ($scoreProfileExport -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreProfileExport must write the MVP2 JSON profile contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("validate_score_profile_json", "profile_specific_exact", "unsupported accuracy level", "basic-approximation", "EScoreProfileInvalid", "IScoreProfileMvp2Supported", "score_profile_json_rejects_unknown_fields_and_unsupported_spin_rules", "score_profile_rejects_profile_specific_exact_until_exact_models_exist", "score_profile_json_rejects_invalid_combo_and_b2b_settings")) {
    if ($scoreProfileValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreProfileValidator must map score profile import/schema errors into diagnostics marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "SpinTarget",
    "SpinTargetPredicate",
    "SpinClassifier",
    "SpecialSpinCaseRegistry",
    "KickEvidence",
    "CandidateScoreStats",
    "ScoreProfileObjectValidator",
    "AllSpinScoreMapping",
    "SpinAwardPolicy::AllSpinAsTSpinMini",
    "DropScorePolicy::HardDrop2SoftDrop1",
    "tetrio_profile_disables_all_spin_by_default"
)) {
    if ($scoringProfilesDoc -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/scoring-profiles.md must document spin/scoring profile marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "pattern_universe_id",
    "pattern_weight_model_id",
    "CoverageRowKind",
    "SpinTarget",
    "ScoreCell",
    "OwnedCorePatternBitSetSnapshot",
    "PatternBitSet Dynamic Word Allocation",
    "SpinCoverageMatrixBudget",
    "ScoreCellMatrixBudget",
    "WordCapacityExceeded",
    "CLR_SCORE_MATRIX_CAPACITY_EXCEEDED"
)) {
    if ($buildCoverageDoc -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/build-coverage.md must document coverage universe/row-kind marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "Forbidden:",
    "KickTableProfileId::FinSpecial",
    "KickTableProfileId::IsoSpecial",
    "KickTableProfileId::NeoSpecial"
)) {
    if ($rulesAndKicksDocForSpin -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/rules-and-kicks.md must reject special-spin-as-kick-table marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "first-success evidence",
    "search_backend_supported=false",
    "Rule/Kick Ownership Boundary",
    "raw kick property text"
)) {
    if ($rulesAndKicksDocForSpin -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/rules-and-kicks.md must document kick evidence/ownership marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "SearchProblemBudget",
    "CLR_BUILDUP_ENUMERATION_TRUNCATED",
    "CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED",
    "FFI Lifetime And Variant Buffers",
    "buildup_enumerate_variant_limit_comes_from_problem_budget",
    "ffi_build_variant_view_copies_kick_evidence_to_block_pointer_escape"
)) {
    if ($buildupDocForSpin -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/buildup.md must document BuildUp memory/variant marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "ffi_pattern_bitset_pointer_escape_is_blocked_by_owned_snapshot",
    "PatternBitSet::new_with_word_budget",
    "SpinCoverageMatrix",
    "ScoreCellMatrix",
    "CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED"
)) {
    if ($algorithmsDocForSpin -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/algorithms.md must document memory/FFI algorithm marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "GpuComputedUnconfirmed",
    "CLR_SCORE_MATRIX_CAPACITY_EXCEEDED",
    "E_SCORE_MATRIX_CAPACITY_EXCEEDED",
    "throttled_backend",
    "score matrix capacity overflow without a diagnostic"
)) {
    if ($gpuPipelineDocForSpin -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/gpu-pipeline.md must document GPU trust/budget marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "json_spin_probability_includes_universe_identity",
    "json_score_result_distinguishes_evaluation_scope",
    "json_special_spin_diagnostic_reports_disabled_reason",
    "json_retained_trace_average_not_labeled_expected_score",
    "json_score_matrix_capacity_exceeded_reports_budget_evidence",
    "kick_evidence_limit",
    "FFI pointer identity",
    "probability_complete=false"
)) {
    if ($outputFormatsDocForSpin -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/output-formats.md must document output completeness/budget marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "E_SPIN_COVERAGE_CAPACITY_EXCEEDED",
    "E_KICK_EVIDENCE_BUFFER_EXHAUSTED",
    "W_BUILDUP_ENUMERATION_TRUNCATED",
    "FFI And Budget Diagnostics",
    "score_matrix_capacity_exceeded_diagnostic"
)) {
    if ($diagnosticsDocForSpin -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/diagnostics.md must document memory/FFI diagnostic marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "C/Rust FFI owned snapshots",
    "dynamic word allocation budget",
    "SpinCoverageMatrix",
    "ScoreCellMatrix",
    "BuildUp enumerate variant limit",
    "KickEvidence buffer budget",
    "Score matrix capacity exceeded diagnostic"
)) {
    if ($mvpScopeDocForSpin -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/mvp-scope.md must document MVP memory/FFI scope marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "Architecture marker registry",
    "docs/scoring-profiles.md",
    "FFI pointer escape guard",
    "PatternBitSet dynamic word allocation scope",
    "Score matrix capacity exceeded diagnostic"
)) {
    if ($architectureDocForSpin -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/architecture.md must keep marker-only docs contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("SpinTargetValidator", "SpinTargetCapability", "SpinTargetValidationMode", "required_score_profile", "clear_lines", "ESpinClassifierIncompatible", "ESpinProfileUnverified", "ESpinKickEvidenceMissing", "spin_target_validator_rejects_missing_classifier", "spin_target_validator_rejects_unverified_special_spin_exact")) {
    if ($spinTargetValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpinTargetValidator must guard spin target capability/score-profile/exactness marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("SpecialSpinProfileValidator", "SpecialSpinProfileValidationMode", "Fin/ISO/NEO", "verified_fixture_required", "ESpinProfileUnverified", "ESpinKickEvidenceMissing", "WSpinClassificationEstimated", "special_spin_profile_validator_requires_verified_import")) {
    if ($specialSpinProfileValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpecialSpinProfileValidator must keep Fin/ISO/NEO as verified special spin cases marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("ScoreProfileObjectValidator", "ScoreProfileObjectDescriptor", "ScoreModelId::parse", "AttackModelId::parse", "spin_classifier_id", "DropScorePolicy", "TraceCompleteness", "SpinAwardPolicy::AllSpins", "SpinAwardPolicy::AllMini", "SpinAwardPolicy::AllSpinAsTSpinMini", "unknown_fields", "score_profile_object_validator_rejects_all_spin_in_default_tetrio_profile", "score_profile_object_validator_allows_custom_all_spin_options_with_classifier", "score_profile_object_validator_rejects_all_spin_without_all_piece_classifier", "score_profile_object_validator_requires_trace_completeness_for_drop_score")) {
    if ($scoreProfileObjectValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreProfileObjectValidator must validate registry ids, trace requirements, all-spin policy, and unknown fields marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "special_spin_case_is_not_kick_table_profile",
    "special_spin_case_requires_kick_evidence",
    "special_spin_case_matches_profile_kick_signature_and_board_signature",
    "unverified_fin_iso_neo_profile_is_disabled",
    "verified_special_spin_profile_enables_kick_sensitive_classifier"
)) {
    if ($specialSpinCaseContractTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "special spin contract tests must keep validation marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "kick_sensitive_spin_requires_kick_evidence",
    "kick_sensitive_rule_uses_verified_special_spin_registry",
    "kick_sensitive_rule_does_not_enable_unverified_special_case",
    "kick_sensitive_rule_checks_profile_kick_signature_and_board_predicate",
    "kick_sensitive_rule_falls_back_to_all_spin_for_immobile_non_t"
)) {
    if ($spinClassifierContractTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "spin classifier contract tests must keep validation marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "special_case_registry",
    "cases_for_piece",
    "exact_enabled(true)",
    "allowed_for_profile(profile.id())",
    "required_kick_signature_matches(kick_evidence)",
    "board_signature_matches(&input)",
    "fallback_to_corner_or_immobility_rule"
)) {
    if ($kickSensitiveSpinRule -notlike "*$requiredMarker*") {
        Add-ArchitectureError "KickSensitiveSpinRule must use verified SpecialSpinCaseRegistry before fallback marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "with_required_kick_signature",
    "with_board_signature_predicate",
    "with_mini_override",
    "allowed_for_profile",
    "required_kick_signature_matches",
    "board_signature_matches",
    "SpinKind::ProfileSpecific"
)) {
    if ($specialSpinCase -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpecialSpinCase must classify verified exact special spin marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "cases_for_piece",
    "case.piece() == piece"
)) {
    if ($specialSpinCaseRegistry -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpecialSpinCaseRegistry must expose piece-filtered classifier cases marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "retained_trace_average_is_not_unconditional_expected_score"
)) {
    if ($scoreExpectationScopeContractTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "score expectation scope tests must keep validation marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "score_profile_object_validator"
)) {
    if ($scoreProfileObjectValidatorContractTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "score profile object validator contract tests must keep validation marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "spin_probability_uses_pattern_bitset_union"
)) {
    if ($spinTargetRunnerTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpinTarget runner tests must keep validation marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("basic approximation", "profile-specific basic score/attack tables", "jstris-ultra", "tetrio", "t-spins", "t-spins-plus", "all-spin", "all-spin-plus", "all-mini", "all-mini-plus", "HardDrop2SoftDrop1", "profile_specific_exact=false", "t-spin-simple", "profile-specific-exact", "ScoreEvaluationSummary", "evaluated_trace_count", "score_evaluation_trace_count", "score_evaluation_complete", "score_evaluation_basis", "retained-traces", "all-traces", "sample", "score_event_basis=c-replay", "score_evaluation_scope", "MaxScoreCover", "best_score_by_pattern", "objective_score_probability_no_double_count")) {
    if ($scoringDoc -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/scoring.md must disclose scoring accuracy contract marker '$requiredMarker'"
    }
}
$maxScoreCover = Read-Text "crates/clearra-objectives/src/max_score/max_score_cover.rs"
$objectiveReducer = Read-Text "crates/clearra-objectives/src/reducer/objective_reducer.rs"
$scoredCoverageCandidate = Read-Text "crates/clearra-objectives/src/max_score/scored_coverage_candidate.rs"
$maxScoreSelection = Read-Text "crates/clearra-objectives/src/max_score/max_score_selection.rs"
foreach ($requiredMarker in @("ScoredCoverageCandidate", "PatternBitSet", "score", "attack")) {
    if ($scoredCoverageCandidate -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoredCoverageCandidate must keep score metadata outside CoverageMatrix marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("pub fn select", "WeightedPatternSet", "union_probability", "PatternScoreContribution", "best_score_by_pattern", "score_aware_cover_uses_pattern_union_probability_not_variant_sum", "score_aware_cover_selects_best_candidate_per_pattern", "score_aware_cover_reports_incomplete_required_patterns", "max_score_cover_uses_best_score_by_pattern")) {
    if ($maxScoreCover -notlike "*$requiredMarker*") {
        Add-ArchitectureError "MaxScoreCover must compute score-aware cover from pattern union measure marker '$requiredMarker'"
    }
}
if ($objectiveReducer -notlike "*score_does_not_change_coverage_probability*") {
    Add-ArchitectureError "ObjectiveReducer tests must keep score/probability separation marker 'score_does_not_change_coverage_probability'"
}
foreach ($requiredMarker in @("MaxScoreCoverPolicy", "PatternScoreContribution", "expected_score", "expected_attack", "best_score_by_pattern", "MaxScoreCoverResult", "MaxScoreCoverPolicyError")) {
    if ($maxScoreSelection -notlike "*$requiredMarker*") {
        Add-ArchitectureError "MaxScoreCover selection model must expose score/attack expectation contract marker '$requiredMarker'"
    }
}
foreach ($file in Get-RustFiles "crates/clearra-objectives/src/max_score") {
    $relativePath = Resolve-Path -LiteralPath $file.FullName -Relative
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    if ($contents -like "*CoverageMatrix*") {
        Add-ArchitectureError "$relativePath must not couple score-aware objectives to CoverageMatrix; use ScoredCoverageCandidate"
    }
}
