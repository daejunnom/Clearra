use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_problem::{
    query::{PieceBudget, SetupQueueInput, SetupSearchQuery},
    ProblemCompiler,
};
use clearra_supply::queue::{fixed_sequence::FixedSequence, observed_queue::ObservedQueue};

use super::*;

#[test]
fn setup_raw_metrics_reports_queue_hold_boundary_rule_180_and_post_pc() {
    let query = SetupSearchQuery::default()
        .with_queue(SetupQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
        ])))
        .with_piece_budget(
            PieceBudget::new(vec![PieceKind::I, PieceKind::O, PieceKind::T], 3)
                .expect("piece budget"),
        );
    let problem = ProblemCompiler::compile_setup(&query).expect("setup problem");

    let result = SetupService::execute(&problem).expect("setup service");

    assert_eq!(result.field("shape_family_count"), Some("1"));
    assert_eq!(result.field("queue_prefix"), Some("IOT"));
    assert_eq!(result.field("queue_prefix_len"), Some("3"));
    assert!(matches!(
        result.field("hold_required"),
        Some("true") | Some("false")
    ));
    assert!(result.field("hold_piece").is_some());
    assert!(result.field("bag_boundary_offsets").is_some());
    assert!(matches!(
        result.field("bag_boundary_ambiguous"),
        Some("true") | Some("false")
    ));
    assert_eq!(result.field("requires_180"), Some("false"));
    assert_eq!(result.field("requires_180_evidence"), Some("not-modeled"));
    assert_eq!(result.field("rule_profile_evidence"), Some("srs-plus"));
    assert_eq!(result.field("post_pc_solution_count"), Some("0"));
    assert_eq!(result.field("score_basis"), Some("none"));
    assert_eq!(result.field("backend_report"), Some("attached"));
    assert_eq!(result.field("setup_raw_coverage_export"), Some("inline"));
}

#[test]
fn observed_verified_pattern_count_reported() {
    let query = SetupSearchQuery::default()
        .with_queue(SetupQueueInput::observed(ObservedQueue::new(vec![
            PieceKind::I,
            PieceKind::O,
        ])))
        .with_piece_budget(PieceBudget::new(vec![PieceKind::I], 1).expect("piece budget"));
    let problem = ProblemCompiler::compile_setup(&query).expect("setup problem");

    let result = SetupService::execute(&problem).expect("observed setup executes");

    assert_eq!(
        result.field("verified_pattern_count"),
        result.field("materialized_pattern_count")
    );
    assert!(result.usize_field("verified_pattern_count").unwrap_or(0) > 0);
    assert_eq!(
        result.field("coverage_source"),
        Some("pattern-specific-exact-buildability")
    );
}
