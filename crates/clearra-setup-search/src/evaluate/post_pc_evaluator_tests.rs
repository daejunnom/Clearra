use clearra_core_domain::{
    ids::setup_id::{BuildVariantId, TilingVariantId},
    piece::piece_kind::PieceKind,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_pc_graph::request::{
    PcCompletionGoal, PcCountPolicy, PcQueueInput, PcScenarioBoard, PieceWindow,
};
use clearra_scoring::builtin::tetrio_score;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::{
    evaluate::ScoreEvaluationBasis, identity::build_identity::BuildIdentity,
    variant::build_variant::BuildVariant,
};

use super::*;

#[test]
fn post_pc_evaluator_runs_clear_to_empty_scenario_query_from_setup_variant() {
    let setup_o_at_left = 0b11 | (0b11 << 10);
    let variant = BuildVariant::new(
        BuildVariantId::new(1),
        TilingVariantId::new(2),
        BuildIdentity::new(setup_o_at_left, None),
        PatternBitSet::new(1),
    );
    let input = PostPcScenarioInput::from_build_variant(
        &variant,
        2,
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])),
        4,
    )
    .with_count_policy(PcCountPolicy::CountAll)
    .with_min_remaining_queue(0);

    let evaluation = PostPcEvaluator::evaluate_input_with_score_profile(input, &tetrio_score());
    let summary = evaluation.summary().expect("evaluated post-PC");

    assert!(summary.solution_found());
    assert_eq!(summary.completion_goal(), PcCompletionGoal::ClearToEmpty);
    assert_eq!(summary.cleared_lines(), 2);
    assert!(summary.total_solution_count() > 0);
    assert!(summary.retained_trace_count() > 0);
    assert!(summary.score().best_score() > 0);
    assert!(summary.score().best_attack() > 0);
    assert_eq!(summary.score().profile_id(), "tetrio");
    assert_eq!(
        summary.score().score_evaluation_trace_count(),
        summary.retained_trace_count()
    );
    if cfg!(feature = "native-c-core") {
        assert!(summary.score().score_evaluation_complete());
        assert_eq!(
            summary.score().score_evaluation_basis(),
            ScoreEvaluationBasis::AllTraces
        );
    } else {
        assert!(!summary.score().score_evaluation_complete());
        assert_eq!(
            summary.score().score_evaluation_basis(),
            ScoreEvaluationBasis::RetainedTraces
        );
    }
}

#[test]
fn post_pc_score_summary_discloses_sample_basis_when_retained_traces_are_limited() {
    let setup_o_at_left = 0b11 | (0b11 << 10);
    let variant = BuildVariant::new(
        BuildVariantId::new(1),
        TilingVariantId::new(2),
        BuildIdentity::new(setup_o_at_left, None),
        PatternBitSet::new(1),
    );
    let input = PostPcScenarioInput::from_build_variant(
        &variant,
        2,
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])),
        4,
    )
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1);

    let evaluation = PostPcEvaluator::evaluate_input_with_score_profile(input, &tetrio_score());
    let summary = evaluation.summary().expect("evaluated post-PC");

    assert!(summary.solution_found());
    assert_eq!(summary.score_evaluation_trace_count(), 1);
    assert_eq!(
        summary.score_evaluation_basis(),
        ScoreEvaluationBasis::Sample
    );
    assert!(!summary.score_evaluation_complete());
}

#[test]
fn post_pc_continuation_uses_actual_consumed_pieces_not_max_window() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x00000000000003f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::T,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])),
        PieceWindow::new(6),
    )
    .with_min_remaining_queue(5)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1);

    let evaluation = PostPcEvaluator::evaluate_query(&query);
    let summary = evaluation.summary().expect("evaluated post-PC");

    assert!(summary.solution_found());
    assert_eq!(summary.min_queue_consumed(), 2);
    assert_eq!(summary.sample_queue_consumed(), 2);
    assert_eq!(summary.best_remaining_queue_len(), 5);
    assert!(summary.continuation_available());
    assert!(summary.continuation_available_complete());
}

#[test]
fn post_pc_evaluator_reports_unsupported_for_unexpanded_observed_queue() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::observed(clearra_supply::queue::observed_queue::ObservedQueue::new(
            vec![PieceKind::I],
        )),
        PieceWindow::new(1),
    );

    assert_eq!(
        PostPcEvaluator::evaluate_query(&query),
        PostPcEvaluation::Unsupported {
            reason: "observed scenario queues must be expanded before post-PC evaluation"
        }
    );
}
