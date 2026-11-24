//! The supply consumer receives an exact language here; this test does not
//! enumerate or solve a geometry family. It checks the full-to-scalar boundary.

use super::*;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy,
    PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
use clearra_supply::pattern_universe::PieceMultisetKey;

pub(super) fn problem(probabilities: PcSolutionProbabilityPolicy) -> SearchProblem {
    ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 63 | (63 << 10)),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(7),
        )
        .with_allow_hold(true)
        .with_exact_pieces(Some(7))
        .with_supply_window_size(SupplyWindowSize::new(7))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_objective(ObjectivePolicy::unique())
        .with_solution_probability_policy(probabilities)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_workers(1)
                .with_tablebase_requested(false),
        ),
    )
    .unwrap()
    .with_output_policy(SearchOutputPolicy::Summary)
}

pub(super) fn all_one_bag_words() -> BuildOrderGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for subset in 0_usize..128 {
        let first = edges.len();
        for (operation, piece) in PieceKind::STANDARD_TETROMINOES.into_iter().enumerate() {
            if subset & (1 << operation) == 0 {
                edges.push(BuildOrderGraph::edge(
                    (subset | (1 << operation)) as u32,
                    operation as u8,
                    piece,
                    RotationState::Zero,
                    0,
                    0,
                    0,
                ));
            }
        }
        nodes.push(BuildOrderNodeSpec {
            edge_start: first as u32,
            edge_count: (edges.len() - first) as u32,
            depth: subset.count_ones() as u8,
            accepting: subset == 127,
        });
    }
    BuildOrderGraph::from_topological_parts(nodes, edges, 0, 0, true).unwrap()
}

#[test]
fn product_membership_requires_full_original_coverage_and_no_retained_rows() {
    let problem = problem(PcSolutionProbabilityPolicy::Omit);
    assert!(problem.pc_placement_dependency_eligible());
    let universe = problem.piece_source().materialized_universe().unwrap();
    assert_eq!(universe.pattern_count(), 5_040);
    assert!(StandardBagCoverage::supports(
        universe,
        problem.initial_hold()
    ));
    let target = TargetGroup {
        key: PieceMultisetKey::from_pieces(PieceKind::STANDARD_TETROMINOES),
        pattern_index_id: 0,
        possible_patterns: Arc::new(PatternBitSet::all(5_040)),
        pattern_index: None,
    };
    let forbidden = || panic!("ineligible membership must not scan global coverage");
    let mut workspace = BuildUpWorkspace::default();
    assert_eq!(
        CandidateWitnessMode::for_candidate(&problem, &target, forbidden, false, false),
        CandidateWitnessMode::Disabled,
    );
    let root = workspace
        .piece_order_languages
        .canonicalize(&all_one_bag_words())
        .unwrap();
    let control = ExecutionControl::default();
    let mut evaluator = CoverageProductEvaluator::default();
    let full = workspace
        .cover_piece_language(
            &problem,
            &target,
            root,
            CandidateWitnessMode::Disabled,
            false,
            false,
            &mut evaluator,
            &control,
        )
        .unwrap();
    assert_eq!(full.symbolic_covered_pattern_count, 5_040);
    workspace
        .standard_bag_coverage
        .as_mut()
        .unwrap()
        .set_boolean_membership_enabled_for_test(true);
    assert!(!workspace.can_use_first_pattern_witness());
    workspace
        .merge_standard_bag_coverage(full.symbolic_coverage_root.unwrap())
        .unwrap();
    assert!(workspace.can_use_first_pattern_witness());
    let original_words = workspace
        .materialize_standard_bag_coverage()
        .unwrap()
        .unwrap()
        .to_owned_words();

    // Capture rows and explicit per-solution probabilities still need their
    // exact full coverage. Typed probability evidence is an independent owner.
    assert_eq!(
        CandidateWitnessMode::for_candidate(&problem, &target, forbidden, true, true),
        CandidateWitnessMode::Disabled,
    );
    let included = self::problem(PcSolutionProbabilityPolicy::Include);
    assert_eq!(
        CandidateWitnessMode::for_candidate(&included, &target, forbidden, false, true),
        CandidateWitnessMode::Disabled,
    );
    let probability = problem
        .clone()
        .with_pc_chance_probability_v2_evidence()
        .with_output_policy(SearchOutputPolicy::CoverageSummary);
    assert!(probability.pc_placement_dependency_eligible());
    assert_eq!(
        CandidateWitnessMode::for_candidate(&probability, &target, forbidden, false, true),
        CandidateWitnessMode::Disabled,
    );

    let mode = CandidateWitnessMode::for_candidate(&problem, &target, || true, false, true);
    assert_eq!(mode, CandidateWitnessMode::MembershipOnly);
    let scalar = workspace
        .cover_piece_language(
            &problem,
            &target,
            root,
            mode,
            true,
            false,
            &mut evaluator,
            &control,
        )
        .unwrap();
    assert!(scalar.buildable);
    assert_eq!(scalar.witness_pattern_id, Some(0));
    assert!(scalar.covered_patterns.is_none());
    assert!(scalar.symbolic_coverage_root.is_none());
    assert_eq!(scalar.symbolic_covered_pattern_count, 0);
    assert!(scalar.coverage_product_states > 0);
    assert!(scalar.coverage_product_edge_checks > 0);
    let accounting = workspace
        .standard_bag_coverage
        .as_ref()
        .unwrap()
        .memo_accounting();
    assert_eq!(accounting.first_witness_calls, 1);
    assert_eq!(accounting.first_witness_mode_transitions, 1);
    assert_eq!(
        workspace
            .materialize_standard_bag_coverage()
            .unwrap()
            .unwrap()
            .to_owned_words(),
        original_words,
    );

    let boolean = workspace
        .cover_piece_language(
            &problem,
            &target,
            root,
            mode,
            false,
            false,
            &mut evaluator,
            &control,
        )
        .unwrap();
    assert!(boolean.buildable);
    assert_eq!(boolean.witness_pattern_id, None);
    assert!(boolean.covered_patterns.is_none());
    assert!(boolean.symbolic_coverage_root.is_none());
    assert_eq!(boolean.symbolic_covered_pattern_count, 0);
    assert!(boolean.coverage_product_states > 0);
    assert!(boolean.coverage_product_edge_checks > 0);
    let accounting = workspace
        .standard_bag_coverage
        .as_ref()
        .unwrap()
        .memo_accounting();
    assert_eq!(accounting.boolean_membership_calls, 1);
    assert_eq!(accounting.boolean_membership_mode_transitions, 1);
    assert_eq!(accounting.boolean_certified_membership_calls, 0);
    assert_eq!(accounting.product_entries, 0);
    assert_eq!(accounting.product_capacity, 0);
    assert_eq!(accounting.union_capacity, 0);
    assert_eq!(accounting.boolean_negative_roots, 0);

    // The caller's positive source proof is only consumed by Summary bulk.
    // This fixture independently proved every original queue above; no
    // certificate result is used as the Boolean visitor's oracle.
    let certified = workspace
        .cover_piece_language(
            &problem,
            &target,
            root,
            mode,
            false,
            true,
            &mut evaluator,
            &control,
        )
        .unwrap();
    assert!(certified.buildable);
    assert_eq!(certified.witness_pattern_id, None);
    assert_eq!(certified.coverage_product_states, 0);
    assert_eq!(certified.coverage_product_edge_checks, 0);
    assert_eq!(
        workspace
            .standard_bag_coverage
            .as_ref()
            .unwrap()
            .memo_accounting()
            .boolean_certified_membership_calls,
        1
    );

    let trace = problem
        .clone()
        .with_output_policy(SearchOutputPolicy::Trace);
    let trace_result = workspace
        .cover_piece_language(
            &trace,
            &target,
            root,
            mode,
            false,
            true,
            &mut evaluator,
            &control,
        )
        .unwrap();
    assert!(trace_result.buildable);
    assert_eq!(trace_result.witness_pattern_id, Some(0));
    assert_eq!(
        workspace
            .standard_bag_coverage
            .as_ref()
            .unwrap()
            .memo_accounting()
            .boolean_certified_membership_calls,
        1
    );

    workspace
        .standard_bag_coverage
        .as_mut()
        .unwrap()
        .set_boolean_membership_enabled_for_test(false);
    let ablated = workspace
        .cover_piece_language(
            &problem,
            &target,
            root,
            mode,
            false,
            true,
            &mut evaluator,
            &control,
        )
        .unwrap();
    assert_eq!(ablated.witness_pattern_id, Some(0));
    assert!(ablated.buildable);
    assert_eq!(
        workspace
            .standard_bag_coverage
            .as_ref()
            .unwrap()
            .memo_accounting()
            .boolean_certified_membership_calls,
        1
    );
    assert_eq!(
        workspace
            .materialize_standard_bag_coverage()
            .unwrap()
            .unwrap()
            .to_owned_words(),
        original_words,
    );
}
