# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Keep functions side-effect free at load time; validation runs only when invoked.
function Invoke-ScoringPostProcessingValidation() {
foreach ($requiredPath in @(
            "core-c/src/scoring_events/scoring_event_basis.h",
            "core-c/src/scoring_events/scoring_events.c",
            "core-c/tests/scoring_event_tests.c"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M27 scoring post-processing requires $requiredPath"
        }
    }
$cmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
            "src/scoring_events/scoring_events.c",
            "scoring_event_tests"
        )) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 CMake must compile scoring event basis marker '$requiredMarker'"
        }
    }
$replayEvent = Read-Text "crates/clearra-replay/src/replay/replay_event.rs"
foreach ($requiredMarker in @(
            "ReplayDropEvent",
            "ReplaySpinBasisEvent",
            "Drop(ReplayDropEvent)",
            "SpinBasis(ReplaySpinBasisEvent)"
        )) {
        if ($replayEvent -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 replay event contract must expose marker '$requiredMarker'"
        }
    }
$replayEngine = @(
        Read-Text "crates/clearra-replay/src/replay/replay_engine.rs"
        Read-Text "crates/clearra-replay/src/replay/replay_event_builder.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "ReplayDropEvent::new",
            "ReplaySpinBasisEvent::new",
            "replay_events_from_trace",
            "trace_drop_from_y"
        )) {
        if ($replayEngine -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 replay engine must build scoring basis replay event marker '$requiredMarker'"
        }
    }
$scoringTrace = Read-Text "crates/clearra-scoring/src/trace/solution_trace_events.rs"
if ($scoringTrace -notlike "*from_replay_trace*") {
        Add-ArchitectureError "M27 scoring trace adapter must evaluate from replay traces"
    }
$scoreEvent = Read-Text "crates/clearra-scoring/src/event/score_event.rs"
foreach ($requiredMarker in @(
            "score_event_from_step_postprocess_only",
            "score_must_not_prune_packing_candidate",
            "SpinDetector::detect"
        )) {
        if ($scoreEvent -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 ScoreEvent::from_step must remain post-processing only marker '$requiredMarker'"
        }
    }
$spinDetector = Read-Text "crates/clearra-scoring/src/event/spin_detector.rs"
foreach ($requiredMarker in @(
            "spin_detector_postprocess_only",
            "accepted replay evidence",
            "unknown_spin_not_false_for_pc_pruning"
        )) {
        if ($spinDetector -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 SpinDetector must be guarded as post-processing only marker '$requiredMarker'"
        }
    }
$scoringEvaluator = Read-Text "crates/clearra-scoring/src/model/score_model_evaluator.rs"
foreach ($requiredMarker in @(
            "evaluate_replay_trace",
            "SolutionTraceEvents::from_replay_trace",
            "evaluate_events"
        )) {
        if ($scoringEvaluator -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 ScoreModelEvaluator must evaluate replay post-processing marker '$requiredMarker'"
        }
    }
$coreExecutorManifest = Read-Text "crates/clearra-core-executor/Cargo.toml"
$coreExecutorRuntimeDependencies = ($coreExecutorManifest -split "\[dev-dependencies\]", 2)[0]
if ($coreExecutorRuntimeDependencies -like "*clearra-scoring*") {
        Add-ArchitectureError "M27 core executor runtime must stay scoring-free; clearra-app owns the post-processing handoff"
    }
$pcService = Get-PcServiceValidationSurface
foreach ($requiredMarker in @(
            "ScoreModelEvaluator::evaluate_replay_trace",
            "score_post_processing",
            "score_core_hot_path",
            "score_accuracy_level",
            "score_profile_accuracy_mode",
            "score_event_basis",
            "score_matrix_complete",
            "score_matrix_accuracy_level",
            "score_does_not_change_probability_union",
            "score_evaluation_scope",
            "objective_max_score_cover",
            "objective_best_score_by_pattern_count",
            "objective_score_probability_no_double_count",
            "objective_score_does_not_modify_coverage_probability",
            "placement_event_available",
            "clear_event_available",
            "drop_event_basis_available",
            "spin_event_basis_available",
            "score_probability_before",
            "score_probability_after"
        )) {
        if ($pcService -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 PcService must report replay scoring post-processing marker '$requiredMarker'"
        }
    }
$summaryContract = Read-Text "crates/clearra-cli/src/output/summary_render_contract.rs"
foreach ($requiredMarker in @(
            "score_post_processing",
            "score_core_hot_path",
            "score_accuracy_level",
            "score_profile_accuracy_mode",
            "score_event_basis",
            "score_evaluation_basis",
            "score_evaluation_scope",
            "score_matrix_complete",
            "objective_max_score_cover",
            "objective_best_score_by_pattern_count",
            "objective_score_probability_no_double_count",
            "score_does_not_change_probability_union",
            "drop_event_basis_available",
            "spin_event_basis_available"
        )) {
        if ($summaryContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 summary contract must type scoring marker '$requiredMarker'"
        }
    }
$jsonContract = Get-JsonContractValidationSurface
$replayJsonContract = Read-Text "crates/clearra-output/src/json/replay_json_contract.rs"
$jsonSurface = "$jsonContract`n$replayJsonContract"
foreach ($requiredMarker in @(
            "pc_scoring_contract",
            "score_post_processing",
            "score_accuracy_level",
            "score_profile_accuracy_mode",
            "score_event_basis",
            "score_matrix_complete",
            "score_evaluation_scope",
            "score_does_not_change_probability_union",
            "objective_max_score_cover",
            "objective_best_score_by_pattern_count",
            "objective_score_probability_no_double_count",
            "ReplayEvent::Drop",
            "ReplayEvent::SpinBasis"
        )) {
        if ($jsonSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 JSON contract must expose scoring/replay event marker '$requiredMarker'"
        }
    }
$pathCommand = Read-Text "crates/clearra-app/src/commands/path_app_command.rs"
foreach ($requiredMarker in @(
            "score_post_processing",
            "score_accuracy_level",
            "score_does_not_change_probability_union"
        )) {
        if ($pathCommand -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 PathAppCommand must forward scoring post-processing marker '$requiredMarker'"
        }
    }
$processE2E = Read-Text "crates/clearra-cli/tests/process_e2e.rs"
foreach ($requiredMarker in @(
            "process_e2e_m27_scoring_post_processing_reports_accuracy_and_probability_invariant",
            "score_post_processing",
            "score_core_hot_path",
            "score_accuracy_level",
            "score_profile_accuracy_mode",
            "score_event_basis",
            "score_evaluation_scope",
            "score_matrix_complete",
            "score_does_not_change_probability_union",
            "objective_max_score_cover",
            "objective_best_score_by_pattern_count",
            "objective_score_probability_no_double_count",
            "placement_event_available",
            "clear_event_available",
            "drop_event_basis_available",
            "spin_event_basis_available"
        )) {
        if ($processE2E -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M27 process E2E must verify scoring post-processing marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "M27 Scoring Post-Processing",
            "Core search hot path stays scoring-free",
            "score profile evaluates replay",
            "placement event available",
            "clear event available",
            "drop event basis available",
            "spin event basis available",
            "score does not change probability union",
            "score output states accuracy level",
            "core-c/src/scoring_events",
            "X4 MVP2 Scoring / Objective",
            "score_event_basis=c-replay",
            "score_profile_accuracy_mode",
            "score_evaluation_scope",
            "MaxScoreCover",
            "best_score_by_pattern",
            "objective_max_score_cover",
            "objective_best_score_by_pattern_count",
            "objective_score_probability_no_double_count",
            "sample vs full evaluation distinguished"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M27 scoring post-processing marker '$requiredMarker'"
        }
    }
}
function Invoke-GuiSchemaValidation() {
foreach ($requiredPath in @(
            "crates/clearra-ui-schema/src/setup_explorer/backend_options_schema.rs",
            "crates/clearra-ui-schema/src/setup_explorer/problem_preset_options_schema.rs",
            "crates/clearra-ui-schema/src/setup_explorer/scenario_editor_schema.rs",
            "crates/clearra-ui-schema/src/setup_explorer/setup_explorer_schema.rs",
            "crates/clearra-ui-schema/src/i18n/language_selector_schema.rs",
            "crates/clearra-ui-schema/src/i18n/localized_label_schema.rs",
            "crates/clearra-ui-schema/src/build_editor/build_editor_schema.rs",
            "crates/clearra-ui-schema/src/rule_editor/rule_editor_schema.rs",
            "crates/clearra-ui-schema/src/score_editor/score_profile_editor_schema.rs",
            "crates/clearra-ui-schema/src/score_editor/spin_target_schema.rs",
            "crates/clearra-ui-schema/src/score_editor/spin_classifier_schema.rs",
            "crates/clearra-ui-schema/src/score_editor/special_spin_case_schema.rs",
            "crates/clearra-ui-schema/src/score_editor/score_expectation_schema.rs",
            "crates/clearra-ui-schema/src/score_editor/max_score_cover_schema.rs",
            "crates/clearra-ui-schema/src/setup_explorer/spin_target_filter_schema.rs",
            "crates/clearra-ui-schema/src/setup_explorer/spin_probability_columns.rs"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M28 GUI schema product path requires $requiredPath"
        }
    }
$uiManifest = Read-Text "crates/clearra-ui-schema/Cargo.toml"
if ($uiManifest -notlike "*clearra-problem*") {
        Add-ArchitectureError "M28 UI schema must depend on clearra-problem for canonical SearchProblemPreset ids"
    }
if ($uiManifest -notlike "*clearra-i18n*") {
        Add-ArchitectureError "M28 UI schema must depend on clearra-i18n for canonical translation keys and language selector schema"
    }
$i18nManifest = Read-Text "crates/clearra-i18n/Cargo.toml"
$i18nLib = Read-Text "crates/clearra-i18n/src/lib.rs"
foreach ($requiredMarker in @("language", "catalog", "export", "LanguageId", "TranslationCatalog", "UiTranslationExport")) {
        if ($i18nManifest -notlike "*clearra-i18n*" -or $i18nLib -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-i18n must expose language/catalog/export marker '$requiredMarker'"
        }
    }
$setupMod = Read-Text "crates/clearra-ui-schema/src/setup_explorer/mod.rs"
foreach ($requiredMarker in @(
            "backend_options_schema",
            "problem_preset_options_schema",
            "scenario_editor_schema",
            "BackendOptionsSchema",
            "ProblemPresetOptionsSchema",
            "ScenarioEditorSchema",
            "SpinTargetFilterSchema",
            "SpinProbabilityColumnSchema"
        )) {
        if ($setupMod -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 setup_explorer module must export schema marker '$requiredMarker'"
        }
    }
$backendOptions = Read-Text "crates/clearra-ui-schema/src/setup_explorer/backend_options_schema.rs"
foreach ($requiredMarker in @(
            "BackendOptionsSchema",
            "ExecutionOptionsSchema::mvp2",
            "backend_fallback_reason",
            "candidate_backend",
            "buildup_backend"
        )) {
        if ($backendOptions -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 backend_options schema must expose backend/result marker '$requiredMarker'"
        }
    }
$problemPresets = Read-Text "crates/clearra-ui-schema/src/setup_explorer/problem_preset_options_schema.rs"
foreach ($requiredMarker in @(
            "SearchProblemPreset::OpeningPc",
            "SearchProblemPreset::ScenarioPc",
            "SearchProblemPreset::Setup",
            "SearchProblemPreset::Build",
            "packing_candidate_count",
            "build_variant_count",
            "coverage_probability"
        )) {
        if ($problemPresets -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 problem preset schema must use canonical preset/result marker '$requiredMarker'"
        }
    }
$scenarioEditor = Read-Text "crates/clearra-ui-schema/src/setup_explorer/scenario_editor_schema.rs"
foreach ($requiredMarker in @(
            "ScenarioEditorSchema",
            "initial_board_mask",
            "remaining_queue",
            "max_pieces",
            "retained_trace_limit",
            "search_unsupported_reason",
            "scenario_requires_180_unsupported",
            "packing_candidate_count",
            "coverage_probability"
        )) {
        if ($scenarioEditor -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 scenario editor schema must expose query/result marker '$requiredMarker'"
        }
    }
$setupExplorer = Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_explorer_schema.rs"
foreach ($requiredMarker in @(
            "backend_options",
            "language_selector",
            "problem_preset_options",
            "scenario_editor",
            "BackendOptionsSchema::from_execution_options",
            "LanguageSelectorSchema::mvp",
            "ProblemPresetOptionsSchema::m28",
            "ScenarioEditorSchema::m28",
            "SpinTargetFilterSchema::mvp2",
            "spin_probability_columns"
        )) {
        if ($setupExplorer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 SetupExplorerSchema must aggregate product path schema marker '$requiredMarker'"
        }
    }
$setupColumns = @(
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_result_columns.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_probability_columns.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_backend_columns.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_score_columns.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_diagnostic_columns.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_continuation_columns.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/scenario_result_columns.rs"
    ) -join "`n"
$setupColumnSchema = Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_result_column_schema.rs"
foreach ($requiredMarker in @(
            "packing_candidate_count",
            "shape_family_id",
            "tiling_variant_count",
            "build_variant_count",
            "covered_pattern_count",
            "backend_fallback_reason",
            "post_pc_solution_count",
            "total_solution_count",
            "retained_trace_count",
            "coverage_probability",
            "score_basis",
            "setup_raw_metrics",
            "setup_raw_coverage_export",
            "raw_coverage_export_path",
            "backend_report",
            "score_evaluation_basis",
            "search_unsupported_reason"
        )) {
        if ($setupColumns -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 setup result columns must expose result marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @("LocalizedLabelSchema", "TranslationKey::ui_setup_result", "localized_label")) {
        if ($setupColumnSchema -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 setup result column schema must expose localized label marker '$requiredMarker'"
        }
    }
$languageSelector = Read-Text "crates/clearra-ui-schema/src/i18n/language_selector_schema.rs"
$localizedLabel = Read-Text "crates/clearra-ui-schema/src/i18n/localized_label_schema.rs"
foreach ($requiredMarker in @("LanguageSelectorSchema", "default_language", "detected_language", "selected_language", "LanguageOptionSchema", "한국어")) {
        if ($languageSelector -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 UI i18n language selector must expose marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @("LocalizedLabelSchema", "TranslationKey", "fallback_en", "resolve")) {
        if ($localizedLabel -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 UI i18n localized label schema must expose marker '$requiredMarker'"
        }
    }
foreach ($editorSpec in @(
            @{
                Path = "crates/clearra-ui-schema/src/score_editor/spin_target_schema.rs";
                Markers = @("SpinTargetSchema", "T-spin Double", "T-spin Triple", "T-spin Mini Double", "All-spin Single", "All-spin Double", "All-spin Triple", "Profile-specific spin", "Regular only", "Mini allowed", "Mini only", "All-spin as mini", "Exact only", "Allow estimate", "Require kick evidence", "ui_schema_exposes_spin_target_options")
            },
            @{
                Path = "crates/clearra-ui-schema/src/score_editor/special_spin_case_schema.rs";
                Markers = @("SpecialSpinCaseSchema", "SpecialSpinCaseId::Fin", "SpecialSpinCaseId::Iso", "SpecialSpinCaseId::Neo", "verified_fixture_required", "ui_schema_exposes_special_spin_disabled_reason")
            },
            @{
                Path = "crates/clearra-ui-schema/src/score_editor/score_expectation_schema.rs";
                Markers = @("ScoreExpectationSchema", "Retained trace sample", "Covered patterns conditional", "Full universe expected", "ui_schema_distinguishes_score_evaluation_scope")
            },
            @{
                Path = "crates/clearra-ui-schema/src/setup_explorer/spin_probability_columns.rs";
                Markers = @("SpinProbabilityColumnSchema", "spin_target_id", "probability", "pattern_universe_id", "pattern_weight_model_id", "materialized_probability_mass", "spin_accuracy", "trace_completeness", "ui_schema_does_not_localize_json_contract_keys")
            },
            @{
                Path = "crates/clearra-ui-schema/src/build_editor/build_editor_schema.rs";
                Markers = @("result_contract_fields", "packing_candidate_count", "build_variant_count", "coverage_probability", "backend_fallback_reason")
            },
            @{
                Path = "crates/clearra-ui-schema/src/rule_editor/rule_editor_schema.rs";
                Markers = @("capability_result_fields", "search_backend_supported", "search_unsupported_reason", "unsupported_reason_field")
            },
            @{
                Path = "crates/clearra-ui-schema/src/score_editor/score_profile_editor_schema.rs";
                Markers = @("result_contract_fields", "score_evaluation_basis", "score_accuracy_level", "score_does_not_change_probability_union")
            }
        )) {
        $contents = Read-Text $editorSpec.Path
        if ($editorSpec.Path -eq "crates/clearra-ui-schema/src/score_editor/score_profile_editor_schema.rs") {
            $contents = @(
                $contents
                Read-Text "crates/clearra-ui-schema/src/score_editor/score_profile_editor_fields.rs"
                Read-Text "crates/clearra-ui-schema/src/score_editor/score_profile_import_export_schema.rs"
                Read-Text "crates/clearra-ui-schema/src/score_editor/score_profile_result_contract_fields.rs"
            ) -join "`n"
        }
        foreach ($requiredMarker in $editorSpec.Markers) {
            if ($contents -notlike "*$requiredMarker*") {
                Add-ArchitectureError "M28 $($editorSpec.Path) must expose editor contract marker '$requiredMarker'"
            }
        }
    }
$schemaSnapshot = Read-Text "crates/clearra-ui-schema/src/schema_snapshot.rs"
foreach ($requiredMarker in @(
            "backend_result_contract_field_count",
            "language_option_count",
            "problem_preset_option_count",
            "scenario_editor_field_count",
            "spin_target_option_count",
            "special_spin_case_count",
            "score_expectation_scope_count",
            "spin_probability_column_count"
        )) {
        if ($schemaSnapshot -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 schema snapshot must pin product path surface marker '$requiredMarker'"
        }
    }
$uiTests = Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_explorer_schema_tests.rs"
foreach ($requiredMarker in @(
            "setup_explorer_schema_exposes_m28_schema_surfaces",
            "setup_explorer_schema_exposes_language_selector_and_localized_columns",
            "backend_fallback_reason",
            "packing_candidate_count",
            "build_variant_count",
            "search_unsupported_reason"
        )) {
        if ($uiTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M28 UI schema tests must verify product path marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "M28 GUI Schema",
            "language selector",
            "localized label schema",
            "backend auto/cpu/gpu/hybrid",
            "fallback reason",
            "packing candidate count",
            "BuildVariant count",
            "total_solution_count",
            "retained_trace_count",
            "coverage_probability",
            "raw metrics export",
            "score basis",
            "unsupported reason"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M28 GUI schema marker '$requiredMarker'"
        }
    }
}
