use clearra_core_domain::{
    ids::setup_id::{BuildVariantId, TilingVariantId},
    piece::piece_kind::PieceKind,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_pc_graph::request::PcCompletionGoal;
use clearra_problem::{ProblemCompiler, SearchProblemPreset};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::{identity::build_identity::BuildIdentity, variant::build_variant::BuildVariant};

use super::*;

#[test]
fn post_pc_scenario_input_builds_clear_to_empty_query() {
    let query = PostPcScenarioInput::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .into_query();

    assert_eq!(query.completion_goal(), PcCompletionGoal::ClearToEmpty);
}

#[test]
fn setup_post_pc_compiles_to_scenario_preset() {
    let setup_mask = 0b11 | (0b11 << 10);
    let variant = BuildVariant::new(
        BuildVariantId::new(1),
        TilingVariantId::new(2),
        BuildIdentity::new(setup_mask, Some(PieceKind::T)),
        PatternBitSet::new(1),
    );
    let input = PostPcScenarioInput::from_build_variant(
        &variant,
        2,
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O, PieceKind::O])),
        2,
    )
    .with_allow_hold(true)
    .with_min_remaining_queue(0);
    let query = input.into_query();
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("post-PC problem");

    assert_eq!(problem.preset(), SearchProblemPreset::ScenarioPc);
    assert_eq!(problem.initial_board().occupied_mask(), setup_mask);
    assert_eq!(problem.supply().hold_piece(), Some(PieceKind::T));
    assert_eq!(problem.piece_window().max_pieces(), 2);
    assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
    assert!(problem.scenario().exact_target_policy().is_none());
}
