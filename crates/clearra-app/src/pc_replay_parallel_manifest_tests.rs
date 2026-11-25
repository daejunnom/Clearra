use super::*;
use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionProgress, ProgressSink},
    piece::piece_kind::PieceKind,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    SupplyWindowSize,
};
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

const CAP: u128 = 128 * 1024 * 1024;

#[allow(clippy::too_many_arguments)]
fn source(
    expression: &str,
    draws: usize,
    hold: bool,
    held: Option<PieceKind>,
    board: u64,
    height: u16,
    pieces: usize,
) -> (SearchProblem, CoreExecutionResult) {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(height, board),
        PcQueueInput::pattern_expression(QueuePatternExpression::parse(expression, 1024).unwrap()),
        PieceWindow::new(pieces),
    )
    .with_exact_pieces(Some(pieces))
    .with_supply_window_size(SupplyWindowSize::new(draws))
    .with_allow_hold(hold)
    .with_hold_piece(held)
    .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_objective(ObjectivePolicy::all());
    let problem = clearra_problem::ProblemCompiler::compile_scenario_pc(&query)
        .unwrap()
        .with_pc_path_v2_evidence();
    let result = clearra_core_executor::WasmCpuSearchBackend::execute_with_control(
        &problem,
        &ExecutionControl::default(),
    )
    .unwrap();
    assert!(result.pc_path_supply_evidence().is_some());
    assert!(!result.exact_scoring_execution_batches()[0]
        .graphs()
        .is_empty());
    assert_eq!(
        result.exact_scoring_execution_batches()[0].patterns()[0].len(),
        draws
    );
    (problem, result)
}

fn o_source(hold: bool, held: Option<PieceKind>) -> (SearchProblem, CoreExecutionResult) {
    // 625 original queues cross the 1/2/8-worker wave boundaries. Only the
    // source suffix varies; the producer still owns all original ordinals.
    source(
        "OOI[IJLST][IJLST][IJLST][IJLST]",
        7,
        hold,
        held,
        0xfc3f0,
        2,
        2,
    )
}

fn serial(problem: &SearchProblem, result: &CoreExecutionResult) -> Arc<PcReplayPageSource> {
    let mut session = PcReplaySourceBuildSession::new(problem, result, CAP).unwrap();
    for _ in 0..10_000 {
        if session.advance(8192, &ExecutionControl::default()).unwrap() {
            return session.complete().unwrap();
        }
    }
    panic!("bounded two-piece manifest did not finish");
}

fn assert_manifest(actual: &PcReplayPageSource, expected: &PcReplayPageSource) {
    assert_eq!(
        actual.materialized_pattern_count(),
        expected.materialized_pattern_count()
    );
    assert_eq!(actual.witness_count(), expected.witness_count());
    assert_eq!(actual.identity_sha256(), expected.identity_sha256());
    assert_eq!(actual.geometries, expected.geometries);
    assert_eq!(actual.first_members, expected.first_members);
}

#[test]
fn replay_prefix_counts_cross_waves_preserve_original_manifest_and_witnesses() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    for (hold, held) in [(false, None), (true, None), (true, Some(PieceKind::O))] {
        let (problem, result) = o_source(hold, held);
        let expected = serial(&problem, &result);
        assert_eq!(expected.materialized_pattern_count(), 625);
        assert!(expected.witness_count() > 0);
        for workers in [1, 2, 8] {
            let mut session = PcReplaySourceBuildSession::new(&problem, &result, CAP).unwrap();
            let geometries = session.pending.len();
            assert!(session.same_count_prefix(0, 624));
            assert!(
                !session.same_count_prefix(0, 625),
                "a receipt cannot cross geometry"
            );
            let computed = session
                .drain_count_waves(&ExecutionControl::default().with_parallelism(workers))
                .unwrap();
            assert_eq!(
                computed, geometries,
                "one count per geometry, workers {workers}"
            );
            let actual = session.complete().unwrap();
            assert_manifest(&actual, &expected);
            assert!(actual
                .geometries
                .iter()
                .all(|geometry| geometry.patterns.len() == 625));
            assert_eq!(
                actual.checked_retained_capacity_bytes(),
                actual.checked_full_capacity_recount()
            );
        }
    }
}

#[test]
fn replay_prefix_counts_require_the_bound_producer_proof() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let (problem, result) = o_source(true, None);
    let expected = serial(&problem, &result);
    let mut session = PcReplaySourceBuildSession::new(&problem, &result, CAP).unwrap();
    session.source.supply_evidence = None;
    assert!(!session.same_count_prefix(0, 1));
    let cells = session.pending.len() * session.source.materialized_pattern_count;
    let computed = session
        .drain_count_waves(&ExecutionControl::default().with_parallelism(8))
        .unwrap();
    assert_eq!(
        computed, cells,
        "unknown evidence must use each original language"
    );
    assert_manifest(&session.complete().unwrap(), &expected);
}

#[allow(clippy::too_many_arguments)]
fn with_supply(
    base: &ExactScoringExecutionBatch,
    patterns: &[&str],
    cursor: u16,
    held: Option<PieceKind>,
    hold: bool,
    projected: bool,
    standard_bag: bool,
) -> ExactScoringExecutionBatch {
    ExactScoringExecutionBatch::new(
        base.layout(),
        base.initial_occupied(),
        patterns
            .iter()
            .map(|pattern| {
                pattern
                    .chars()
                    .map(|piece| PieceKind::from_ascii(piece).unwrap())
                    .collect()
            })
            .collect(),
        cursor,
        held,
        hold,
        projected,
        standard_bag,
        base.kick_table_id(),
        base.rule_profile_id(),
        base.graphs().to_vec(),
        true,
    )
}

fn count_and_keys(batch: &ExactScoringExecutionBatch, pattern: usize) -> (usize, Vec<String>) {
    let mut language = ExactReplayLanguageSession::new(
        Arc::from(vec![batch.clone()]),
        vec![ExactReplayGraphLocation { batch: 0, graph: 0 }],
        pattern,
        ExactReplayMaterializationLimits::new(usize::MAX, 256, CAP),
        &mut |_| Ok(()),
    )
    .unwrap();
    let mut complete = false;
    for _ in 0..10_000 {
        if language
            .advance(8192, &ExecutionControl::default(), &mut |_| Ok(()))
            .unwrap()
        {
            complete = true;
            break;
        }
    }
    assert!(complete, "bounded replay language must finish");
    let count = language.count().unwrap();
    assert!(
        count < 1000,
        "fixture must not enumerate a large witness set"
    );
    let keys = (0..count)
        .map(|rank| {
            let selected = language
                .select(rank, &ExecutionControl::default(), &mut |_| Ok(()))
                .unwrap();
            assert_eq!(
                selected.pattern_id(),
                pattern,
                "selection retains the actual ordinal"
            );
            selected.trace_identity().to_owned()
        })
        .collect();
    (count, keys)
}

#[test]
fn replay_prefix_equivalence_matches_exact_zero_and_nonzero_languages_with_cursor_and_hold() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let (_, result) = source("OOIJ", 4, true, None, 0xfc3f0, 2, 2);
    let base = &result.exact_scoring_execution_batches()[0];
    for (hold, held) in [(false, None), (true, None), (true, Some(PieceKind::O))] {
        for cursor in [0, 1] {
            for zero in [false, true] {
                let patterns = match (cursor, zero) {
                    (0, false) => ["OOIJ", "OOIL"],
                    (1, false) => ["TOOIJ", "TOOIL"],
                    (0, true) => ["IITJ", "IITL"],
                    (1, true) => ["TIITJ", "TIITL"],
                    _ => unreachable!(),
                };
                let batch = with_supply(base, &patterns, cursor, held, hold, false, false);
                assert!(same_unprojected_count_prefix(&batch, 2, 0, 1));
                let left = count_and_keys(&batch, 0);
                let right = count_and_keys(&batch, 1);
                assert_eq!(left, right, "cursor {cursor}, hold {hold}, held {held:?}");
                assert_eq!(left.0 == 0, zero);
            }
        }
    }
    let changed = with_supply(base, &["OOIJ", "OIIL"], 0, None, true, false, false);
    assert!(!same_unprojected_count_prefix(&changed, 2, 0, 1));
    assert_ne!(count_and_keys(&changed, 0).0, count_and_keys(&changed, 1).0);
    let different_length = with_supply(base, &["OOIJ", "OOI"], 0, None, true, false, false);
    assert!(!same_unprojected_count_prefix(&different_length, 2, 0, 1));
    assert!(!same_unprojected_count_prefix(base, 2, 0, usize::MAX));
}

#[test]
fn replay_prefix_reuse_declines_terminal_release_and_actual_bag_lookahead() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let (_, result) = source("OOIJ", 4, true, None, 0xfc3f0, 2, 2);
    let base = &result.exact_scoring_execution_batches()[0];
    for standard_bag in [false, true] {
        let release = with_supply(
            base,
            &["O", "O"],
            0,
            Some(PieceKind::O),
            true,
            true,
            standard_bag,
        );
        assert!(!same_unprojected_count_prefix(&release, 2, 0, 1));
        assert!(
            count_and_keys(&release, 0).0 > 0,
            "terminal held release is a real path"
        );
    }
    let all = (1_u64 << 40) - 1;
    let i_hole = 1 | (1 << 10) | (1 << 20) | (1 << 30);
    let (_, result) = source("I", 1, false, None, all ^ i_hole, 4, 1);
    let base = &result.exact_scoring_execution_batches()[0];
    // Six pieces exclude I. The real standard-bag terminal transition draws
    // that missing I at cursor6, beyond the original sequence, and clears4L.
    let lookahead = with_supply(
        base,
        &["OTLSZJ", "OTLSJZ"],
        6,
        Some(PieceKind::O),
        true,
        true,
        true,
    );
    assert!(!same_unprojected_count_prefix(&lookahead, 1, 0, 1));
    let left = count_and_keys(&lookahead, 0);
    assert!(left.0 > 0);
    assert_eq!(left, count_and_keys(&lookahead, 1));
}

struct CancelAfterFirstWave(ExecutionCancellationToken);
impl ProgressSink for CancelAfterFirstWave {
    fn report(&self, progress: ExecutionProgress) {
        if progress.stage == "complete-replay-pattern" && progress.completed >= 256 {
            self.0.handle().cancel();
        }
    }
}

#[test]
fn replay_prefix_wave_reuse_preserves_memory_rejection_and_cross_wave_cancellation() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let (problem, result) = o_source(true, None);
    let mut limited = PcReplaySourceBuildSession::new(&problem, &result, CAP).unwrap();
    let exact = limited.checked_retained_capacity_bytes().unwrap()
        + limited.source.retained_external_bytes().unwrap();
    limited.source.maximum_bytes = exact;
    let error = limited
        .drain_count_waves(&ExecutionControl::default().with_parallelism(8))
        .unwrap_err();
    assert!(
        matches!(error, PcReplayPageError::MemoryLimit { required, maximum } if required > maximum && maximum == exact)
    );
    assert_eq!(limited.next_pattern, 0);
    assert_eq!(limited.source.witness_count(), 0);

    let cancellation = ExecutionCancellationToken::new();
    let mut control = ExecutionControl::new(cancellation.clone()).with_parallelism(8);
    control.progress_sink = Some(Arc::new(CancelAfterFirstWave(cancellation)));
    let mut session = PcReplaySourceBuildSession::new(&problem, &result, CAP).unwrap();
    let error = session.drain_count_waves(&control).unwrap_err();
    assert_eq!(error.code(), "complete_replay_cancelled");
    assert_eq!(session.next_pattern, 256);
    assert!(!session.complete);
    assert!(session.source.identity_sha256().is_empty());
    assert!(
        session.ready_counts.is_empty(),
        "the last admitted wave reduced completely"
    );
}
