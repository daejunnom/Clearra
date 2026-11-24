use super::super::product_membership_tests::{all_one_bag_words, problem};
use super::*;
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
};
use clearra_pc_graph::request::{
    PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy,
    PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
use clearra_supply::{pattern_universe::PieceMultisetKey, QueueObservationPolicy};

fn target() -> TargetGroup {
    TargetGroup {
        key: PieceMultisetKey::from_pieces(PieceKind::STANDARD_TETROMINOES),
        pattern_index_id: 0,
        possible_patterns: Arc::new(PatternBitSet::all(5_040)),
        pattern_index: None,
    }
}

fn one_bag_word() -> BuildOrderGraph {
    let pieces = PieceKind::STANDARD_TETROMINOES;
    let nodes = (0..=pieces.len())
        .map(|index| BuildOrderNodeSpec {
            edge_start: index.min(pieces.len()) as u32,
            edge_count: u32::from(index < pieces.len()),
            depth: index as u8,
            accepting: index == pieces.len(),
        })
        .collect();
    let edges = pieces
        .into_iter()
        .enumerate()
        .map(|(index, piece)| {
            BuildOrderGraph::edge(
                (index + 1) as u32,
                index as u8,
                piece,
                RotationState::Zero,
                0,
                0,
                0,
            )
        })
        .collect();
    BuildOrderGraph::from_topological_parts(nodes, edges, 0, 0, true).unwrap()
}

fn initialized_workspace(
    problem: &SearchProblem,
    request: &Arc<SharedStandardBagRequest>,
    full: bool,
) -> (BuildUpWorkspace, u32, CandidateBuildResult) {
    let mut workspace = BuildUpWorkspace::default();
    workspace
        .set_shared_standard_bag_request(Arc::clone(request))
        .unwrap();
    let graph = if full {
        all_one_bag_words()
    } else {
        one_bag_word()
    };
    let root = workspace
        .piece_order_languages
        .canonicalize(&graph)
        .unwrap();
    let result = workspace
        .cover_piece_language(
            problem,
            &target(),
            root,
            CandidateWitnessMode::Disabled,
            false,
            false,
            &mut CoverageProductEvaluator::default(),
            &ExecutionControl::default(),
        )
        .unwrap();
    let owner = workspace.standard_bag_coverage.as_mut().unwrap();
    owner.set_boolean_membership_enabled_for_test(true);
    owner.set_shared_saturation_enabled_for_test(true);
    (workspace, root, result)
}

#[test]
fn shared_summary_bridge_publishes_once_adopts_partial_and_reverifies_representative_with_fresh_owner(
) {
    let problem = problem(PcSolutionProbabilityPolicy::Omit);
    assert!(BuildUpWorkspace::shared_saturation_request_eligible(
        &problem, false
    ));
    let control = ExecutionControl::default();
    let request = Arc::new(SharedStandardBagRequest::default());
    let shared = SharedCoverage::for_standard_bag_request(5_040, Arc::clone(&request));
    let (mut publisher, _, full) = initialized_workspace(&problem, &request, true);
    let (mut receiver, root, partial) = initialized_workspace(&problem, &request, false);
    assert_eq!(full.symbolic_covered_pattern_count, 5_040);
    assert!(
        partial.symbolic_covered_pattern_count > 0
            && partial.symbolic_covered_pattern_count < 5_040
    );
    receiver
        .merge_standard_bag_coverage(partial.symbolic_coverage_root.unwrap())
        .unwrap();
    let original = receiver
        .materialize_standard_bag_coverage()
        .unwrap()
        .unwrap();
    assert!(!publisher
        .publish_early_standard_bag_saturation(&shared, &control)
        .unwrap());
    assert!(!receiver
        .adopt_shared_standard_bag_saturation(&shared, &control)
        .unwrap());
    publisher
        .merge_standard_bag_coverage(full.symbolic_coverage_root.unwrap())
        .unwrap();
    assert!(publisher
        .publish_early_standard_bag_saturation(&shared, &control)
        .unwrap());
    assert!(!publisher
        .publish_early_standard_bag_saturation(&shared, &control)
        .unwrap());
    assert_eq!(
        publisher
            .standard_bag_memo_accounting()
            .shared_early_publications,
        1
    );
    assert!(receiver
        .adopt_shared_standard_bag_saturation(&shared, &control)
        .unwrap());
    assert!(!receiver
        .adopt_shared_standard_bag_saturation(&shared, &control)
        .unwrap());
    assert!(!receiver.standard_bag_coverage_complete());
    assert!(!receiver.can_use_first_pattern_witness());
    assert!(receiver.shared_boolean_saturation_available());
    let mode = CandidateWitnessMode::for_candidate(
        &problem,
        &target(),
        || true,
        false,
        receiver.can_use_first_pattern_witness() || receiver.shared_boolean_saturation_available(),
    );
    assert_eq!(mode, CandidateWitnessMode::MembershipOnly);
    let boolean = receiver
        .cover_piece_language(
            &problem,
            &target(),
            root,
            mode,
            false,
            false,
            &mut CoverageProductEvaluator::default(),
            &control,
        )
        .unwrap();
    assert_eq!(boolean.buildable, partial.buildable);
    assert!(boolean.witness_pattern_id.is_none());
    assert!(boolean.symbolic_coverage_root.is_none());
    assert_eq!(
        receiver
            .materialize_standard_bag_coverage()
            .unwrap()
            .unwrap(),
        original
    );
    assert_eq!(
        receiver
            .standard_bag_memo_accounting()
            .shared_saturation_adoptions,
        1
    );
    assert_eq!(
        receiver
            .standard_bag_memo_accounting()
            .shared_retire_transitions,
        1
    );
    assert_eq!(
        receiver
            .standard_bag_memo_accounting()
            .boolean_membership_calls,
        1
    );

    // The native representative consumer keeps the shared request lease but
    // uses a fresh workspace, Disabled mode and retain_trace=true. A shared Ω
    // must not replace its real original minimum ordinal with first success.
    let mut representative = BuildUpWorkspace::default();
    representative
        .set_shared_standard_bag_request(Arc::clone(&request))
        .unwrap();
    let representative_root = representative
        .piece_order_languages
        .canonicalize(&one_bag_word())
        .unwrap();
    let reverified = representative
        .cover_piece_language(
            &problem,
            &target(),
            representative_root,
            CandidateWitnessMode::Disabled,
            true,
            false,
            &mut CoverageProductEvaluator::default(),
            &control,
        )
        .unwrap();
    assert_eq!(reverified.witness_pattern_id, partial.witness_pattern_id);
    assert_eq!(
        reverified.symbolic_covered_pattern_count,
        partial.symbolic_covered_pattern_count
    );
    assert!(reverified.symbolic_coverage_root.is_some());
    assert_eq!(
        representative
            .standard_bag_memo_accounting()
            .shared_saturation_adoptions,
        0
    );
    assert_eq!(
        representative
            .standard_bag_memo_accounting()
            .boolean_membership_calls,
        0
    );
    receiver
        .publish_worker_standard_bag_coverage(&shared)
        .unwrap();
    assert_eq!(shared.to_bitset().unwrap(), PatternBitSet::all(5_040));
}

#[test]
fn shared_summary_scope_rejects_trace_capture_rows_countall_observation_and_constraints() {
    let ordinary = problem(PcSolutionProbabilityPolicy::Omit);
    assert!(!BuildUpWorkspace::shared_saturation_request_eligible(
        &ordinary, true
    )); // Capture/rows.
    for output in [
        SearchOutputPolicy::Trace,
        SearchOutputPolicy::CoverageSummary,
    ] {
        assert!(!BuildUpWorkspace::shared_saturation_request_eligible(
            &ordinary.clone().with_output_policy(output),
            false
        ));
    }
    assert!(!BuildUpWorkspace::shared_saturation_request_eligible(
        &problem(PcSolutionProbabilityPolicy::Include),
        false
    ));
    assert!(!BuildUpWorkspace::shared_saturation_request_eligible(
        &ordinary.clone().with_pc_chance_probability_v2_evidence(),
        false
    ));
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 63 | (63 << 10)),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(7),
    )
    .with_allow_hold(true)
    .with_exact_pieces(Some(7))
    .with_supply_window_size(SupplyWindowSize::new(7))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::unique())
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1)
            .with_tablebase_requested(false),
    );
    for query in [
        query.clone().with_count_policy(PcCountPolicy::CountAll),
        query
            .clone()
            .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven),
        query.clone().with_objective(
            ObjectivePolicy::unique().with_back_to_back_preservation(SpinProfileSelection::TSpins),
        ),
        query.with_objective(ObjectivePolicy::minimum_cover()),
    ] {
        let problem = ProblemCompiler::compile_scenario_pc(&query)
            .unwrap()
            .with_output_policy(SearchOutputPolicy::Summary);
        assert!(!BuildUpWorkspace::shared_saturation_request_eligible(
            &problem, false
        ));
    }
}

#[test]
fn shared_summary_bridge_disables_both_directions_and_propagates_cancelled_publication() {
    let problem = problem(PcSolutionProbabilityPolicy::Omit);
    let control = ExecutionControl::default();
    let request = Arc::new(SharedStandardBagRequest::default());
    let shared = SharedCoverage::for_standard_bag_request(5_040, Arc::clone(&request));
    let (mut publisher, _, full) = initialized_workspace(&problem, &request, true);
    publisher
        .merge_standard_bag_coverage(full.symbolic_coverage_root.unwrap())
        .unwrap();
    let (mut receiver, _, _) = initialized_workspace(&problem, &request, false);
    for (boolean, enabled) in [(false, true), (true, false)] {
        for workspace in [&mut publisher, &mut receiver] {
            let owner = workspace.standard_bag_coverage.as_mut().unwrap();
            owner.set_boolean_membership_enabled_for_test(boolean);
            owner.set_shared_saturation_enabled_for_test(enabled);
        }
        assert!(!publisher
            .publish_early_standard_bag_saturation(&shared, &control)
            .unwrap());
        assert!(!receiver
            .adopt_shared_standard_bag_saturation(&shared, &control)
            .unwrap());
        assert!(shared.to_bitset().unwrap().is_empty());
    }
    for workspace in [&mut publisher, &mut receiver] {
        let owner = workspace.standard_bag_coverage.as_mut().unwrap();
        owner.set_boolean_membership_enabled_for_test(true);
        owner.set_shared_saturation_enabled_for_test(true);
    }
    let cancelled = ExecutionControl::default();
    cancelled.cancellation.handle().cancel();
    assert!(matches!(
        publisher.publish_early_standard_bag_saturation(&shared, &cancelled),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert!(shared.to_bitset().unwrap().is_empty());
    assert_eq!(
        publisher
            .standard_bag_memo_accounting()
            .shared_early_publications,
        0
    );
    publisher
        .publish_early_standard_bag_saturation(&shared, &control)
        .unwrap();
    assert!(matches!(
        receiver.adopt_shared_standard_bag_saturation(&shared, &cancelled),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert_eq!(
        receiver
            .standard_bag_memo_accounting()
            .shared_saturation_adoptions,
        0
    );
    let foreign_request = Arc::new(SharedStandardBagRequest::default());
    let (mut foreign, _, _) = initialized_workspace(&problem, &foreign_request, false);
    assert!(matches!(
        foreign.adopt_shared_standard_bag_saturation(&shared, &control),
        Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_shared_saturation_request_mismatch"
        ))
    ));
}
