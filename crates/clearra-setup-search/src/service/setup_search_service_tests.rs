use clearra_core_domain::piece::piece_kind::PieceKind;
#[cfg(feature = "native-c-core")]
use clearra_scoring::builtin::tetrio_score;
use clearra_supply::queue::fixed_sequence::FixedSequence;

#[cfg(feature = "native-c-core")]
use crate::query::SetupLimits;
use crate::query::{SetupQueueInput, SetupSearchQuery};

use super::*;

#[test]
#[cfg(feature = "native-c-core")]
fn setup_search_service_executes_query_into_union_probability_results() {
    let query = SetupSearchQuery::default().with_queue(SetupQueueInput::fixed_sequence(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O]),
    ));

    let result = SetupSearchService::execute(&query).expect("setup search");
    let fields = result.summary_fields();

    assert!(result.results().len() > 1);
    assert_eq!(result.family_scores().len(), result.results().len());
    assert!(fields.contains(&("status".to_owned(), "setup-searched".to_owned())));
    assert!(fields.contains(&("execution_scope".to_owned(), "mvp2".to_owned())));
    assert!(fields.contains(&(
        "enumeration_strategy".to_owned(),
        "queue-pattern-shape-tiling-build-post-pc".to_owned()
    )));
    assert!(fields.contains(&(
        "post_pc_mode".to_owned(),
        "scenario-clear-to-empty".to_owned()
    )));
    assert!(fields.contains(&("post_pc_evaluation_attached".to_owned(), "true".to_owned())));
    assert!(fields.contains(&(
        "setup_foundation_reason".to_owned(),
        "core_packing_buildup_build_variants_attached".to_owned()
    )));
    assert!(fields.contains(&(
        "executor_flow".to_owned(),
        "SetupQuery->SearchProblem->C PackingProblem->C PackingResult->C BuildUpResult".to_owned()
    )));
    assert!(fields.contains(&("build_variant_source".to_owned(), "C BuildUp".to_owned())));
    assert!(fields.contains(&("packing_candidate_count".to_owned(), "1".to_owned())));
    assert!(fields.contains(&("core_buildup_variant_count".to_owned(), "1".to_owned())));
    assert!(fields.contains(&("core_coverage_row_count".to_owned(), "1".to_owned())));
    assert!(fields.contains(&(
        "coverage_source".to_owned(),
        "fixed-single-pattern".to_owned()
    )));
    assert!(fields.contains(&("coverage_pattern_count".to_owned(), "1".to_owned())));
    assert!(fields.contains(&("verified_pattern_count".to_owned(), "1".to_owned())));
    assert!(fields.contains(&("materialized_pattern_count".to_owned(), "1".to_owned())));
    assert!(fields.contains(&(
        "covered_pattern_count_basis".to_owned(),
        "complete_pattern_universe".to_owned()
    )));
    assert!(fields
        .iter()
        .all(|(key, _)| !key.contains("condition_summary")));
    assert!(fields.contains(&("backend_report".to_owned(), "per-result".to_owned())));
    assert!(fields.contains(&("queue_mode".to_owned(), "fixed".to_owned())));
    assert!(fields.contains(&("result_0_probability".to_owned(), "1".to_owned())));
    assert!(fields.contains(&("result_0_shape_family_id".to_owned(), "0".to_owned())));
    assert!(fields.contains(&("result_0_shape_family_count".to_owned(), "1".to_owned())));
    assert!(fields.contains(&("result_0_tiling_variant_count".to_owned(), "1".to_owned())));
    assert!(fields.contains(&("result_0_build_variant_count".to_owned(), "1".to_owned())));
    assert!(fields.contains(&("result_0_coverage_probability".to_owned(), "1".to_owned())));
    assert!(fields.contains(&("result_0_covered_pattern_count".to_owned(), "1".to_owned())));
    assert!(fields.contains(&("result_0_queue_prefix".to_owned(), "IO".to_owned())));
    assert!(fields.contains(&("result_0_queue_prefix_len".to_owned(), "2".to_owned())));
    assert!(fields.iter().any(|(key, value)| {
        key == "result_0_hold_required" && matches!(value.as_str(), "true" | "false")
    }));
    assert!(fields.iter().any(|(key, _)| key == "result_0_hold_piece"));
    assert!(fields
        .iter()
        .any(|(key, _)| key == "result_0_bag_boundary_offsets"));
    assert!(fields.iter().any(|(key, value)| {
        key == "result_0_bag_boundary_ambiguous" && matches!(value.as_str(), "true" | "false")
    }));
    assert!(fields.contains(&("result_0_requires_180".to_owned(), "false".to_owned())));
    assert!(fields.contains(&(
        "result_0_requires_180_evidence".to_owned(),
        "not-modeled".to_owned()
    )));
    assert!(fields.contains(&(
        "result_0_rule_profile_evidence".to_owned(),
        "default-rule-profile".to_owned()
    )));
    assert!(fields.iter().any(|(key, value)| {
        key == "result_0_post_pc_solution_count" && value.parse::<usize>().is_ok()
    }));
    assert!(fields.contains(&(
        "result_0_score_basis".to_owned(),
        "retained-traces".to_owned()
    )));
    assert!(fields.contains(&("result_0_backend_report".to_owned(), "attached".to_owned())));
    assert!(fields.contains(&(
        "result_0_raw_coverage_export_path".to_owned(),
        "inline://clearra/setup/raw-coverage/0/union".to_owned()
    )));
    assert!(fields.contains(&(
        "result_0_setup_raw_metrics".to_owned(),
        "attached".to_owned()
    )));
    assert!(fields.contains(&(
        "result_0_setup_raw_coverage_export".to_owned(),
        "inline".to_owned()
    )));
    assert!(fields.contains(&(
        "result_0_diagnostic_evidence_rule_profile".to_owned(),
        "default-rule-profile".to_owned()
    )));
    assert!(fields.contains(&(
        "result_0_backend_report_post_pc_rule_profile".to_owned(),
        "srs-plus".to_owned()
    )));
    assert!(fields.contains(&(
        "result_0_raw_condition_data_requires_180".to_owned(),
        "not-modeled".to_owned()
    )));
    assert!(!fields.contains(&(
        "result_0_raw_condition_data_requires_180_required".to_owned(),
        "false".to_owned()
    )));
    assert!(fields.contains(&("setup_raw_metrics".to_owned(), "per-result".to_owned())));
    assert!(fields.contains(&(
        "setup_raw_coverage_export".to_owned(),
        "union-coverage-fields".to_owned()
    )));
    assert!(fields.contains(&(
        "coverage_overlap_report".to_owned(),
        "union-probability-no-variant-sum".to_owned()
    )));
    assert!(fields.contains(&("diagnostic_evidence".to_owned(), "per-result".to_owned())));
    assert!(fields.contains(&("build_variant_metrics".to_owned(), "per-result".to_owned())));
    assert!(fields.contains(&("score_aggregation_attached".to_owned(), "false".to_owned())));
    assert!(fields
        .iter()
        .any(|(key, value)| key.ends_with("_post_pc_status") && value == "evaluated"));
    assert!(result
        .results()
        .iter()
        .all(|result| result.probability().get() <= 1.0));
}

#[test]
#[cfg(feature = "native-c-core")]
fn setup_search_service_attaches_post_pc_and_score_profile_to_build_results() {
    let query = SetupSearchQuery::default()
        .with_queue(SetupQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_limits(SetupLimits::new(4096, 1024, 1024, 256, 4096, 1).expect("limits"));

    let result = SetupSearchService::execute_with_score_profile(&query, &tetrio_score())
        .expect("setup search");
    let fields = result.summary_fields();

    assert!(fields.contains(&("score_aggregation_attached".to_owned(), "true".to_owned())));
    assert!(result.family_scores().iter().any(|score| {
        score.post_pc_probability().get() > 0.0
            && score.total_solution_count() > 0
            && score.score_evaluation_trace_count() > 0
    }));
    assert!(fields
        .iter()
        .any(|(key, value)| key.ends_with("_score_evaluation_basis") && value == "sample"));
}

#[test]
#[cfg(feature = "native-c-core")]
fn condition_summary_field_absent() {
    let query = SetupSearchQuery::default().with_queue(SetupQueueInput::fixed_sequence(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O]),
    ));

    let result = SetupSearchService::execute(&query).expect("setup search");

    assert!(result
        .summary_fields()
        .iter()
        .all(|(key, _)| !key.contains("condition_summary")));
}

#[test]
#[cfg(feature = "native-c-core")]
fn setup_raw_coverage_export_is_machine_readable() {
    let query = SetupSearchQuery::default().with_queue(SetupQueueInput::fixed_sequence(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O]),
    ));

    let result = SetupSearchService::execute(&query).expect("setup search");
    let fields = result.summary_fields();

    assert!(fields.iter().any(|(key, value)| {
        key.ends_with("_raw_coverage_export_path")
            && value.starts_with("inline://clearra/setup/raw-coverage/")
            && value.ends_with("/union")
    }));
    assert!(fields.contains(&(
        "setup_raw_coverage_export".to_owned(),
        "union-coverage-fields".to_owned()
    )));
}

#[test]
#[cfg(not(feature = "native-c-core"))]
fn setup_search_service_rejects_portable_preview_as_core_buildup_proof() {
    let query = SetupSearchQuery::default().with_queue(SetupQueueInput::fixed_sequence(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O]),
    ));

    assert!(matches!(
        SetupSearchService::execute(&query),
        Err(SetupSearchExecutionError::CoreBuildUp)
    ));
}
