# This file is dot-sourced by Invoke-WorkspaceSurfaceArchitectureValidation.
# It intentionally contains ordered validation statements, not a standalone entrypoint.

$outputModelMod = Read-Text "crates/clearra-output/src/model/mod.rs"
$outputJsonContract = Read-Text "crates/clearra-output/src/json/json_contract.rs"
$outputPcJsonContract = Read-Text "crates/clearra-output/src/json/pc_json_contract.rs"
$outputJsonWriter = Read-Text "crates/clearra-output/src/json/json_writer.rs"
$outputRender = Read-Text "crates/clearra-output/src/render.rs"
$outputRenderMessage = Read-Text "crates/clearra-output/src/model/render_message.rs"
$outputRenderFieldValue = Read-Text "crates/clearra-output/src/model/render_field_value.rs"
$outputGoldenContractTests = Read-Text "crates/clearra-output/tests/output_golden_contract_tests.rs"
$spinProbabilityOutputGolden = Read-Text "tests/golden/output/spin_probability_result.json"
$scoreExpectationOutputGolden = Read-Text "tests/golden/output/score_expectation_result.json"
$specialSpinOutputGolden = Read-Text "tests/golden/output/special_spin_disabled_reason.json"
$cliCommandRenderer = Read-Text "crates/clearra-cli/src/output/command_renderer.rs"
$cliSummaryRenderContract = Read-Text "crates/clearra-cli/src/output/summary_render_contract.rs"
foreach ($requiredMarker in @(
    "RenderFieldValue",
    "RenderField",
    "RenderCoverSelection",
    "RenderCoverSelectionStrategy",
    "RenderCoverSelectionOptimality",
    "RenderExactSearchBudget",
    "cover_selection_render_model_is_exported_from_model_surface"
)) {
    if ($outputModelMod -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-output model/mod.rs must export and test cover selection render model marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "JSON_SCHEMA_VERSION",
    "pub enum JsonValue",
    "Bool(bool)",
    "Number(String)",
    "from_render_message",
    "schema_version",
    "summary",
    "contract",
    "pc_contract",
    "pc_search_contract",
    "pc_backend_contract",
    '"backend"',
    '"requested"',
    '"selected"',
    '"compute"',
    '"traversal"',
    '"selection_reason"',
    '"fallback_used"',
    '"fallback_reason"',
    '"workers_requested"',
    '"workers_used"',
    '"deterministic"',
    '"device_selected"',
    '"device_label"',
    '"wgpu"',
    "search_result_model",
    "backend_selection_reason",
    "user-requested",
    "frontier-count",
    "solution_trace_mode",
    "sample-only",
    "state_count_available",
    "state_count",
    "multiplicity_count_available",
    "multiplicity_count",
    '("replay", prefixed_object(fields, "scenario_replay_"))',
    "min_queue_consumed",
    "max_queue_consumed",
    "sample_queue_consumed",
    "placed_piece_count",
    "best_remaining_queue_len",
    "continuation_available_complete",
    "pc_contract_separates_scenario_replay_from_continuation",
    "supply_contract",
    "spin_probability_contract",
    "score_expectation_contract",
    "special_spin_diagnostic_contract",
    "RenderField",
    "to_json_value",
    "render_message_contract_uses_explicit_typed_summary_values",
    "render_message_contract_does_not_infer_numeric_looking_strings"
)) {
    if ($outputJsonContract -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-output JsonContract must own MVP2 typed nested JSON marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("pub enum RenderFieldValue", "String(String)", "Bool(bool)", "Number(String)", "Array(Vec<RenderFieldValue>)", "Object(Vec<RenderField>)", "to_json_value", "render_field_value_keeps_string_ids_distinct_from_numbers")) {
    if ($outputRenderFieldValue -notlike "*$requiredMarker*") {
        Add-ArchitectureError "RenderFieldValue must carry explicit typed output field marker '$requiredMarker'"
    }
}
if ($outputJsonContract -like "*infer_scalar*" -or $outputJsonContract -like "*fn is_json_number*") {
    Add-ArchitectureError "clearra-output JsonContract must not infer bool/number types from string field contents"
}
foreach ($requiredMarker in @("write_value", "JsonValue::Bool", "JsonValue::Number", "JsonValue::Object", "writes_typed_nested_json_values")) {
    if ($outputJsonWriter -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-output JsonWriter must serialize typed JSON values marker '$requiredMarker'"
    }
}
if ($outputRenderMessage -notlike "*pub fn json_contract(&self) -> JsonContract*" -or
    $outputRenderMessage -notlike "*with_value*" -or
    $outputRenderMessage -notlike "*with_field_preserves_numeric_looking_values_as_strings_for_json*" -or
    $outputRenderMessage -like "*pub fn json_fields*") {
    Add-ArchitectureError "RenderMessage must expose JSON contract separately from flat text summary fields"
}
if ($outputRender -notlike "*JsonWriter::write(&message.json_contract())*") {
    Add-ArchitectureError "RenderFormatDispatcher must render JSON through RenderMessage::json_contract"
}
foreach ($requiredMarker in @(
    "json_spin_probability_includes_universe_identity",
    "json_score_result_distinguishes_evaluation_scope",
    "json_special_spin_diagnostic_reports_disabled_reason",
    "json_probability_not_renormalized_after_observed_truncation",
    "json_output_contains_count_and_trace_separation",
    "text_output_marks_representative_trace",
    "verbose_output_contains_backend_report",
    "diagnostics_output_contains_evidence",
    "retained_trace_truncation_does_not_mark_count_incomplete",
    "json_retained_trace_average_not_labeled_expected_score",
    "spin_probability_result.json",
    "score_expectation_result.json",
    "special_spin_disabled_reason.json"
)) {
    if ($outputGoldenContractTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-output golden contract tests must lock Spin/Score JSON output marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    '"total_solution_count"',
    '"unique_solution_count"',
    '"retained_trace_count"',
    '"count_complete"',
    '"count_truncated_reason"',
    '"trace_retention_truncated"',
    '"trace_retention_reason"',
    '"packing_candidate_count"',
    '"build_variant_count"',
    '"coverage_pattern_count"',
    '"covered_pattern_count"',
    '"coverage_probability"',
    '"probability_complete"',
    '"backend_requested"',
    '"backend_selected"',
    '"backend_fallback_reason"',
    '"gpu_confirmed"',
    '"gpu_trust_state"',
    '"continuation_available"',
    '"continuation_token"',
    '"continue_hint"',
    '"checkpoint_results"',
    '"chain_labels"',
    '"exact_target_policy"'
)) {
    if ($outputPcJsonContract -notlike "*$requiredMarker*") {
        Add-ArchitectureError "pc_json_contract.rs must expose M8 result/output count semantic marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    '"spin_target_id"',
    '"pattern_universe_id"',
    '"pattern_weight_model_id"',
    '"probability_complete"',
    '"materialized_probability_mass"',
    '"renormalized"',
    '"spin_accuracy"',
    '"trace_completeness"'
)) {
    if ($spinProbabilityOutputGolden -notlike "*$requiredMarker*") {
        Add-ArchitectureError "tests/golden/output/spin_probability_result.json must expose SpinProbabilityResult field marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    '"evaluation_scope"',
    '"retained_trace_average_score"',
    '"covered_pattern_conditional_average_score"',
    '"unconditional_expected_score"',
    '"score_does_not_change_probability_union"'
)) {
    if ($scoreExpectationOutputGolden -notlike "*$requiredMarker*") {
        Add-ArchitectureError "tests/golden/output/score_expectation_result.json must expose ScoreResult field marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    '"special_spin_case_id"',
    '"verification_state"',
    '"kick_evidence_required"',
    '"classification_accuracy"',
    '"disabled_reason"'
)) {
    if ($specialSpinOutputGolden -notlike "*$requiredMarker*") {
        Add-ArchitectureError "tests/golden/output/special_spin_disabled_reason.json must expose SpecialSpinDiagnosticOutput field marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("IntoIterator<Item = RenderField>", "message.with_value", "delegates_command_rendering_to_output_crate", "command_renderer_does_not_infer_numeric_looking_strings")) {
    if ($cliCommandRenderer -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CommandRenderer must accept explicit RenderField values and delegate rendering marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("typed_value_for_key", "is_bool_key", "is_number_key", "is_json_number", "ends_with(")) {
    if ($cliCommandRenderer -like "*$forbiddenMarker*") {
        Add-ArchitectureError "CommandRenderer must not infer typed JSON values from key names marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @("SummaryRenderContract", "render_fields", "backend_selection_reason", "solution_trace_mode", "state_count_available", "state_count", "multiplicity_count_available", "multiplicity_count", "contract_uses_exact_keys_without_suffix_inference", "contract_exposes_retained_trace_keys_as_array")) {
    if ($cliSummaryRenderContract -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SummaryRenderContract must own explicit flat summary field typing marker '$requiredMarker'"
    }
}
if ($cliSummaryRenderContract -like "*ends_with(*" -or
    $cliSummaryRenderContract -like "*fn is_json_number*") {
    Add-ArchitectureError "SummaryRenderContract must not use suffix or numeric-content inference; use exact field contracts"
}

