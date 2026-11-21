//! One physical graph and two equivalent ordinal backings. This focused target
//! exercises the production replay reader without linking the App unit suite.
use std::sync::Arc;

use clearra_app::{PcReplayPageSource, PcReplayPageStore, PcReplaySourceBuildSession};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{CoreExecutionResult, WasmCpuSearchBackend};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_replay::{ExactScoringExecutionBatch, ReplayPatternRows};

fn source(
    problem: &SearchProblem,
    result: &CoreExecutionResult,
    control: &ExecutionControl,
) -> (Arc<PcReplayPageSource>, usize) {
    let mut session = PcReplaySourceBuildSession::new(problem, result, u128::MAX).unwrap();
    for steps in 1..=200_000 {
        if session.advance(1, control).unwrap() {
            return (session.complete().unwrap(), steps);
        }
    }
    panic!("bounded one-lock replay did not finish");
}

#[test]
fn exact_p7_prefix_pruning_preserves_digest_and_pages() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(1),
    )
    .with_supply_window_size(SupplyWindowSize::new(7))
    .with_allow_hold(true)
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_objective(ObjectivePolicy::all())
    .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1));
    let problem = ProblemCompiler::compile_scenario_pc(&query)
        .unwrap()
        .with_pc_path_v2_evidence();
    let control = ExecutionControl::default();
    let result = WasmCpuSearchBackend::execute_with_control(&problem, &control).unwrap();
    let [batch] = result.exact_scoring_execution_batches() else {
        panic!("one replay source expected");
    };
    assert_eq!(batch.graphs().len(), 1);
    assert_eq!(batch.standard_bag_ordinals().unwrap().len(), 5_040);
    // Identical ordered queues in dense form cannot claim the authenticated
    // P7 block proof; the comparison arm uses the original exact cursor.
    let dense = ExactScoringExecutionBatch::new_with_pattern_rows(
        batch.layout(),
        batch.initial_occupied(),
        ReplayPatternRows::Dense(
            (0..batch.pattern_count())
                .map(|id| batch.pattern_at(id).unwrap().as_ref().to_vec())
                .collect(),
        ),
        batch.initial_cursor(),
        batch.initial_hold(),
        batch.hold_enabled(),
        batch.projects_unplaced_lookahead(),
        batch.projects_standard_bag_lookahead(),
        batch.kick_table_id(),
        batch.rule_profile_id(),
        batch.graphs().to_vec(),
        batch.complete(),
    );
    let (indexed, indexed_steps) = source(&problem, &result, &control);
    let (dense, dense_steps) = source(
        &problem,
        &result.with_exact_scoring_execution_batch(Some(dense)),
        &control,
    );
    assert!(
        indexed_steps < dense_steps,
        "the identical manifest must require fewer primitive turns: {indexed_steps} vs {dense_steps}"
    );
    assert!(indexed.witness_count() > 0);
    assert_eq!(indexed.witness_count(), dense.witness_count());
    assert_eq!(indexed.identity_sha256(), dense.identity_sha256());
    let mut indexed = PcReplayPageStore::new(indexed);
    let mut dense = PcReplayPageStore::new(dense);
    for page in [1, 2] {
        assert_eq!(
            indexed.page(1, page, &control).unwrap(),
            dense.page(1, page, &control).unwrap()
        );
    }
}
