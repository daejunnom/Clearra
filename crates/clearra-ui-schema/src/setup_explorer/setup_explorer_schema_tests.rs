use clearra_i18n::{LanguageId, TranslationCatalog};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::disabled_reason::UiDisabledReason;

use super::{
    setup_explorer_schema::SetupExplorerSchema, setup_result_column_schema::SetupResultColumnSchema,
};

#[test]
fn setup_explorer_schema_selects_pc_scenario_fixtures_with_disabled_reason() {
    let schema = SetupExplorerSchema::mvp2();
    let fixture_values = schema
        .scenario_fixtures()
        .iter()
        .map(|fixture| fixture.value())
        .collect::<Vec<_>>();

    assert!(fixture_values.contains(&"tests/fixtures/pc/example.json"));
    assert!(fixture_values.contains(&"tests/fixtures/pc/requires_180_unsupported.json"));

    let unsupported_fixture = schema
        .scenario_fixtures()
        .iter()
        .find(|fixture| fixture.value() == "tests/fixtures/pc/requires_180_unsupported.json")
        .expect("unsupported fixture option");
    assert!(unsupported_fixture.is_disabled());
    let reason: &UiDisabledReason = unsupported_fixture
        .disabled_reason()
        .expect("disabled reason");
    assert_eq!(reason.code(), DiagnosticCode::EPcQueryInvalid);
    assert_eq!(reason.reason(), "scenario_requires_180_unsupported");
}

#[test]
fn setup_explorer_schema_exposes_mvp2_result_columns() {
    let schema = SetupExplorerSchema::mvp2();
    let columns = schema
        .result_columns()
        .iter()
        .map(SetupResultColumnSchema::id)
        .collect::<Vec<_>>();

    for column in [
        "shape_family_id",
        "tiling_variant_count",
        "packing_candidate_count",
        "build_variant_count",
        "covered_pattern_count",
        "coverage_probability",
        "post_pc_solution_count",
        "total_solution_count",
        "count_complete",
        "solution_trace_mode",
        "backend_selection_reason",
        "backend_fallback_reason",
        "state_count",
        "multiplicity_count",
        "score_expectation",
        "attack_expectation",
        "score_evaluation_trace_count",
        "score_evaluation_complete",
        "score_evaluation_basis",
        "score_basis",
        "score_accuracy_level",
        "setup_raw_metrics",
        "setup_raw_metrics_schema_version",
        "metrics_kind",
        "setup_raw_coverage_export",
        "raw_coverage_schema_version",
        "raw_coverage_export_kind",
        "raw_coverage_export_path",
        "pattern_universe_id",
        "pattern_weight_model_id",
        "pattern_count",
        "rows",
        "family_unions",
        "overlap_report",
        "coverage_overlap_report",
        "backend_report",
        "search_unsupported_reason",
        "build_variant_metrics_required_hold",
        "diagnostic_evidence_rule_profile",
        "continuation_available",
        "continuation_available_complete",
    ] {
        assert!(
            columns.contains(&column),
            "missing setup result column {column}"
        );
    }
}

#[test]
fn gui_setup_explorer_consumes_raw_metrics_schema() {
    let schema = SetupExplorerSchema::mvp2();
    let raw_metrics = schema.setup_raw_metrics_schema();
    let raw_coverage = schema.setup_raw_coverage_export_schema();

    assert_eq!(raw_metrics.schema_version(), 2);
    assert_eq!(raw_metrics.metrics_kind(), "setup_raw_metrics");
    assert!(raw_metrics.requires_field("shape_family_id"));
    assert!(raw_metrics.requires_field("coverage_overlap_report"));
    assert!(raw_metrics.requires_field("build_variant_metrics"));
    assert!(raw_metrics.forbids_field("condition_summary"));

    assert_eq!(raw_coverage.schema_version(), 2);
    assert_eq!(raw_coverage.export_kind(), "setup_raw_coverage_export");
    assert!(raw_coverage.requires_field("pattern_universe_id"));
    assert!(raw_coverage.requires_field("pattern_weight_model_id"));
    assert!(raw_coverage.requires_field("pattern_count"));
    assert!(raw_coverage.requires_field("rows"));
    assert!(raw_coverage.requires_field("family_unions"));
    assert!(raw_coverage.requires_field("overlap_report"));
}

#[test]
fn setup_explorer_schema_exposes_pc_scenario_result_columns() {
    let schema = SetupExplorerSchema::mvp2();
    let columns = schema
        .scenario_result_columns()
        .iter()
        .map(SetupResultColumnSchema::id)
        .collect::<Vec<_>>();

    for column in [
        "total_solution_count",
        "unique_solution_count",
        "packing_candidate_count",
        "build_variant_count",
        "coverage_probability",
        "min_queue_consumed",
        "max_queue_consumed",
        "sample_queue_consumed",
        "placed_piece_count",
        "best_remaining_queue_len",
        "count_mode",
        "count_requested",
        "count_complete",
        "retained_trace_limit",
        "retained_trace_count",
        "solution_trace_mode",
        "backend_selection_reason",
        "backend_fallback_reason",
        "state_count_available",
        "state_count",
        "multiplicity_count_available",
        "multiplicity_count",
        "trace_retention_truncated",
        "trace_retention_reason",
        "score_evaluation_trace_count",
        "score_evaluation_complete",
        "score_evaluation_basis",
        "score_accuracy_level",
        "search_unsupported_reason",
        "next_pc_available",
        "next_pc_candidate",
        "continuation_token_available",
        "continuation_token_unavailable_reason",
        "continuation_basis",
        "continuation_queue_consumed",
        "continue_available",
        "continuation_available_complete",
        "continuation_token",
        "scenario_replay_token",
    ] {
        assert!(
            columns.contains(&column),
            "missing scenario result column {column}"
        );
    }
}

#[test]
fn setup_explorer_schema_exposes_canonical_execution_options() {
    let schema = SetupExplorerSchema::mvp2();
    let execution = schema.execution_options();
    let backend_values = execution
        .backend_options()
        .iter()
        .map(|option| option.value())
        .collect::<Vec<_>>();

    assert_eq!(backend_values, ["auto", "cpu", "gpu", "hybrid"]);
    assert!(execution.deterministic_default());
    assert!(execution.allow_backend_fallback_default());
    assert_eq!(execution.worker_options().len(), 5);
    assert_eq!(execution.gpu_device_options().len(), 1);

    let gpu = execution
        .backend_options()
        .iter()
        .find(|option| option.value() == "gpu")
        .expect("gpu backend option");
    assert!(!gpu.is_disabled());
    assert!(gpu.disabled_reason().is_none());
}

#[test]
fn setup_explorer_schema_exposes_language_selector_and_localized_columns() {
    let schema = SetupExplorerSchema::mvp2();

    assert_eq!(
        schema.language_selector().default_language(),
        LanguageId::En
    );
    assert_eq!(schema.language_selector().options().len(), 2);
    assert!(schema
        .language_selector()
        .options()
        .iter()
        .any(|option| option.id() == LanguageId::Ko && option.native_label() == "한국어"));

    let total_solution_count = schema
        .scenario_result_columns()
        .iter()
        .find(|column| column.id() == "total_solution_count")
        .expect("total solution column");
    assert_eq!(
        total_solution_count.localized_label().key().as_str(),
        "ui.setup.result.total_solution_count"
    );
    assert_eq!(
        total_solution_count
            .localized_label()
            .resolve(TranslationCatalog::new(LanguageId::Ko))
            .text(),
        "전체 해법 수"
    );
}

#[test]
fn setup_explorer_schema_exposes_m28_schema_surfaces() {
    let schema = SetupExplorerSchema::mvp2();
    let backend_values = schema
        .backend_options()
        .options()
        .iter()
        .map(|option| option.value())
        .collect::<Vec<_>>();
    let preset_values = schema
        .problem_preset_options()
        .options()
        .iter()
        .map(|option| option.id())
        .collect::<Vec<_>>();
    let scenario_fields = schema
        .scenario_editor()
        .fields()
        .iter()
        .map(|field| field.id())
        .collect::<Vec<_>>();

    assert_eq!(backend_values, ["auto", "cpu", "gpu", "hybrid"]);
    assert!(schema
        .backend_options()
        .result_contract_fields()
        .iter()
        .any(|field| field == "backend_fallback_reason"));
    assert_eq!(
        preset_values,
        ["opening-pc", "scenario-pc", "setup", "build"]
    );
    assert!(scenario_fields.contains(&"initial_board_mask"));
    assert!(scenario_fields.contains(&"remaining_queue"));
    assert!(schema
        .scenario_editor()
        .result_contract_fields()
        .iter()
        .any(|field| field == "packing_candidate_count"));
    assert_eq!(
        schema.scenario_editor().unsupported_reason_field(),
        "search_unsupported_reason"
    );
}
