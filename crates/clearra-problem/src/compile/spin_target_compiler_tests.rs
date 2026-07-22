use clearra_core_domain::{
    ids::SpinTargetId, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
    probability::probability_value::ProbabilityValue,
};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;
use crate::{
    goal::{RequiredSpinKind, SearchGoal, SpinTargetRequest},
    query::{SetupSearchQuery, SpinTargetQuery, SpinTargetTraceRequirement},
};

fn tsd_target() -> SpinTargetRequest {
    SpinTargetRequest::tsd("tsd")
}

#[test]
fn percent_spin_target_query_compiles_to_search_problem() {
    let scenario = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::T])),
        PieceWindow::new(1),
    );
    let query = SpinTargetQuery::percent_goal_spin(scenario, tsd_target());

    let problem = SpinTargetCompiler::compile(&query).expect("spin target problem");

    assert_eq!(problem.preset().as_str(), "scenario-pc");
    assert!(matches!(problem.search_goal(), SearchGoal::SpinTarget(_)));
    assert_eq!(
        problem
            .search_goal()
            .spin_target()
            .expect("spin target")
            .id()
            .as_str(),
        "tsd"
    );
}

#[test]
fn spin_target_query_compiles_to_search_problem() {
    let scenario = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::T])),
        PieceWindow::new(1),
    );
    let query = SpinTargetQuery::percent_goal_spin(scenario, tsd_target())
        .with_trace_requirement(SpinTargetTraceRequirement::KickEvidenceRequired);

    let problem = SpinTargetCompiler::compile(&query).expect("spin target problem");

    assert!(matches!(problem.search_goal(), SearchGoal::SpinTarget(_)));
    assert_eq!(query.trace_requirement().as_str(), "kick-evidence-required");
    assert_eq!(query.score_profile_id(), None);
}

#[test]
fn setup_spin_target_query_preserves_threshold() {
    let threshold = ProbabilityValue::new(0.42).expect("threshold");
    let spin_target = tsd_target().with_target_probability_threshold(threshold);
    let query = SpinTargetQuery::setup_goal_spin(SetupSearchQuery::default(), spin_target);

    let problem = SpinTargetCompiler::compile(&query).expect("setup spin target problem");

    assert_eq!(problem.preset().as_str(), "setup");
    assert_eq!(
        problem
            .search_goal()
            .spin_target()
            .expect("spin target")
            .target_probability_threshold(),
        Some(threshold)
    );
}

#[test]
fn pc_then_spin_compiles_to_composite_goal() {
    let pc = OpeningPcSearchQuery::new(PcTarget::two_lines());
    let query = SpinTargetQuery::pc_then_spin(pc, tsd_target());

    let problem = SpinTargetCompiler::compile(&query).expect("pc then spin problem");

    let SearchGoal::Composite(composite) = problem.search_goal() else {
        panic!("expected composite goal");
    };
    assert_eq!(problem.preset().as_str(), "opening-pc");
    assert!(matches!(composite.goals()[0], SearchGoal::ClearToEmpty));
    assert!(matches!(composite.goals()[1], SearchGoal::SpinTarget(_)));
}

#[test]
fn spin_target_query_requires_score_profile_when_profile_specific() {
    let target = SpinTargetRequest::new(
        SpinTargetId::new("fin"),
        RequiredSpinKind::ProfileSpecific("fin"),
    );
    let scenario = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::default(),
        PieceWindow::new(1),
    );
    let query = SpinTargetQuery::percent_goal_spin(scenario, target);

    let result = SpinTargetCompiler::compile(&query);

    assert_eq!(
        result,
        Err(ProblemCompileError::ProfileSpecificSpinTargetRequiresScoreProfile)
    );
}
