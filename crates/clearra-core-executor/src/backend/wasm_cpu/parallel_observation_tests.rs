//! Native producer handoff must preserve one global observation policy.
//! Seven actual draws are initially visible in this complete P7 fixture,
//! so the concrete oracle is an independent policy reference as well.

use clearra_core_domain::{
    execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
    probability::probability_value::ProbabilityValue,
};
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblem};
use clearra_rules::{kicks::KickTableProfileId, profile::builtin_rules::srs_plus};
use clearra_supply::{
    pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
    QueueObservationPolicy,
};

use super::{
    catalog::GeometryCatalog,
    geometry::GeometryCandidate,
    piece_order_language::{
        ConditionalLanguageCompilation, ConditionalTransitionProof, PieceOrderLanguageCache,
    },
    queue_observation_policy::{
        ObservationLanguageNode, ObservationPieceLanguage, PhysicalActionLanguage,
        QueueObservationCoverage, QueueObservationPolicyEvaluator,
    },
    reachability::ReachabilityWorkspace,
    symbolic_prefix_orders::{SymbolicPrefixOrders, SymbolicPrefixWork},
    with_pc_placement_dependency_reference,
};
use crate::{CoreExecutionResult, WasmCpuSearchBackend};

fn complete_p7_problem(policy: QueueObservationPolicy, workers: usize) -> SearchProblem {
    ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0x3c0f_03c0f),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(6),
        )
        .with_rule(srs_plus())
        .with_allow_hold(true)
        .with_exact_pieces(Some(6))
        .with_supply_window_size(SupplyWindowSize::new(7))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_queue_observation_policy(policy)
        .with_retained_trace_limit(0)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(false)
                .with_workers(workers)
                .with_tablebase_requested(false)
                .with_exact_legal_board_enabled(false)
                .with_conditioned_reachability_enabled(false)
                .with_precompute_build_dependencies(false),
        ),
    )
    .expect("complete native P7 request")
    .with_output_policy(SearchOutputPolicy::Summary)
}

fn execute(problem: &SearchProblem) -> CoreExecutionResult {
    WasmCpuSearchBackend::execute_with_control(problem, &ExecutionControl::default())
        .expect("complete native observation result")
}

#[test]
fn native_parallel_observation_producers_preserve_the_complete_p7_policy() {
    let oracle_problem = complete_p7_problem(QueueObservationPolicy::FullQueueOracle, 1);
    let serial_problem = complete_p7_problem(QueueObservationPolicy::VisibleSeven, 1);
    let parallel_problem = complete_p7_problem(QueueObservationPolicy::VisibleSeven, 2);

    let reference = with_pc_placement_dependency_reference(|| execute(&oracle_problem));
    let serial = execute(&serial_problem);
    let parallel = execute(&parallel_problem);

    // The reference is concrete oracle execution. A kind-only see-seven
    // reference would already erase action commitment on longer supplies.
    assert!(reference.usize_field("total_build_order_nodes").unwrap() > 0);
    for result in [&reference, &serial, &parallel] {
        assert!(result.solution_found());
        assert_eq!(result.bool_field("count_complete"), Some(true));
        assert_eq!(result.bool_field("probability_complete"), Some(true));
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
        assert_eq!(result.usize_field("packing_candidate_count"), Some(2_260));
        assert_eq!(result.usize_field("coverage_pattern_count"), Some(5_040));
        assert_eq!(result.normalized_solution_identities().len(), 246);
        assert_eq!(
            result.normalized_solution_identities(),
            reference.normalized_solution_identities()
        );
        assert_eq!(
            result.coverage_pattern_words(),
            reference.coverage_pattern_words()
        );
        assert_eq!(
            result.field("coverage_probability"),
            reference.field("coverage_probability")
        );
    }
    assert_eq!(serial.usize_field("workers_used"), Some(1));
    assert_eq!(parallel.usize_field("workers_used"), Some(2));
    assert_eq!(parallel.bool_field("cpu_parallel_execution"), Some(true));
    assert!(parallel.usize_field("parallel_active_workers").unwrap() >= 1);
    for result in [&serial, &parallel] {
        assert_eq!(result.usize_field("total_build_order_nodes"), Some(0));
        assert_eq!(result.usize_field("total_reachability_states"), Some(0));
        assert!(result.usize_field("observation_policy_states").unwrap() > 0);
        assert!(
            result
                .usize_field("observation_policy_action_checks")
                .unwrap()
                > 0
        );
    }
    // An owner lost at worker return would leave the coordinator with no
    // observation roots and turn this nonempty policy coverage into empty.
    assert!(parallel
        .coverage_pattern_words()
        .iter()
        .any(|&word| word != 0));
}

/// Concrete operation identities remain the decisions in this reference;
/// it performs neither a kind union nor an oracle suffix conversion.
struct ConcretePrefixPolicy<'a>(&'a SymbolicPrefixOrders);

impl ObservationPieceLanguage for ConcretePrefixPolicy<'_> {
    fn root(&self) -> u32 {
        0
    }

    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        let full = u32::from(self.0.terminal_prefix());
        if node > full {
            return None;
        }
        let accepting = node == full;
        (accepting || self.0.live_operations_after(node as u16).next().is_some()).then_some(
            ObservationLanguageNode {
                accepting,
                depth: node.count_ones() as u8,
            },
        )
    }

    fn edge_count(&self, node: u32) -> Option<usize> {
        self.node(node)?;
        Some(self.0.live_operations_after(node as u16).count())
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        self.node(node)?;
        self.0
            .live_operations_after(node as u16)
            .nth(index)
            .map(|(operation, piece)| {
                let code = match piece {
                    PieceKind::I => 1,
                    PieceKind::O => 2,
                    _ => panic!("the eleven-lock fixture contains only I and O"),
                };
                (code, node | (1 << operation))
            })
    }
}

fn sampled_policy(
    universe: &MaterializedPatternUniverse,
    index: &PatternPiecePositionIndex,
    language: &impl ObservationPieceLanguage,
    hold: bool,
) -> QueueObservationCoverage {
    QueueObservationPolicyEvaluator::new(
        universe,
        index,
        QueueObservationPolicy::VisibleSeven,
        0,
        None,
        hold,
        false,
        false,
        None,
    )
    .unwrap()
    .evaluate(language, &ExecutionControl::default())
    .unwrap()
}

#[test]
fn imported_eleven_lock_relation_preserves_repeated_pieces_and_clear_frames() {
    use PieceKind::{I, J, L, O, T};

    // The first I completes row zero. Ten O operations fill the four rows
    // above it, so their physical action coordinates change immediately,
    // while five source draws are still outside the initial observation.
    let initial = 0x3f0;
    let required = ((1_u64 << 50) - 1) ^ initial;
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(10, 5, initial, required)
            .unwrap();
    let mut placements = vec![(I, 0x0f)];
    for y in [1, 3] {
        for x in [0, 2, 4, 6, 8] {
            placements.push((O, (3_u64 << (10 * y + x)) | (3_u64 << (10 * (y + 1) + x))));
        }
    }
    let rows = placements
        .iter()
        .map(|&(piece, cells)| catalog.skeleton_id(piece, cells).unwrap())
        .collect::<Vec<_>>();
    let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
    let control = ExecutionControl::default();
    let orders = SymbolicPrefixOrders::compile_product_with_workspace(
        &catalog,
        &candidate,
        KickTableProfileId::SrsPlus,
        &control,
        &mut ReachabilityWorkspace::default(),
        &mut SymbolicPrefixWork::default(),
        0,
    )
    .expect("complete five-line eleven-operation relation");
    let first_i = orders
        .live_operations_after(0)
        .find_map(|(operation, piece)| (piece == I).then_some(operation))
        .expect("the floor I can clear row zero first");
    assert_eq!(
        orders
            .live_operations_after(1 << first_i)
            .filter(|&(_, piece)| piece == O)
            .count(),
        5,
        "only the five newly projected floor O placements are grounded"
    );

    let mut ordinary = vec![I];
    ordinary.extend([O; 10]);
    ordinary.push(J);
    let mut other_lookahead = ordinary.clone();
    *other_lookahead.last_mut().unwrap() = L;
    let mut hidden_hold = vec![I];
    hidden_hold.extend([O; 6]);
    hidden_hold.push(T);
    hidden_hold.extend([O; 4]);
    // The first two words retain hidden lookahead uncertainty after the
    // first I clears a line, so the physical adapter must use that new frame.
    // The third needs hold to skip its hidden T. This is an exact finite
    // sample, not a complete bag family or repeated-O product admission.
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(711),
        PatternWeightModelId::new(711),
        vec![ordinary, other_lookahead, hidden_hold],
        vec![
            ProbabilityValue::new(0.25).unwrap(),
            ProbabilityValue::new(0.25).unwrap(),
            ProbabilityValue::new(0.5).unwrap(),
        ],
        3,
        true,
        None,
    )
    .unwrap();
    let index = PatternPiecePositionIndex::compile(&universe).unwrap();
    let reference = [false, true]
        .map(|hold| sampled_policy(&universe, &index, &ConcretePrefixPolicy(&orders), hold));
    assert_eq!(reference[0].covered_patterns.words(), &[3]);
    assert_eq!(reference[0].covered_weight, 0.5);
    assert_eq!(reference[1].covered_patterns.words(), &[7]);
    assert_eq!(reference[1].covered_weight, 1.0);

    let mut source_kinds = PieceOrderLanguageCache::default();
    let mut source = PhysicalActionLanguage::default();
    orders
        .compile_product_language_with_residuals(
            &mut source_kinds,
            &mut SymbolicPrefixWork::default(),
            source.prefix_suffix_scratch(),
        )
        .unwrap()
        .expect("nonempty exact suffix language");
    source.register_relation(orders, 5, &control).unwrap();

    // A different pre-existing arena makes numerical source IDs unsafe to
    // copy. Import the complete DAG and rebind every retained suffix root.
    let mut destination_kinds = PieceOrderLanguageCache::default();
    assert!(matches!(
        destination_kinds
            .compile_conditional_language(&[T], 2, 1, |_, _| ConditionalTransitionProof::Allowed)
            .unwrap(),
        ConditionalLanguageCompilation::Certified(_)
    ));
    let rebound = destination_kinds.import_node_map(&source_kinds).unwrap();
    assert!(rebound
        .iter()
        .enumerate()
        .any(|(old, &new)| old != new as usize));
    let mut destination = PhysicalActionLanguage::default();
    destination
        .merge_factored(source, &rebound, &control, &destination_kinds)
        .unwrap();
    drop(source_kinds);
    let view = destination
        .factored_view(&destination_kinds, &control)
        .unwrap()
        .unwrap();
    for (reference, hold) in reference.iter().zip([false, true]) {
        let imported = sampled_policy(&universe, &index, &view, hold);
        assert_eq!(imported.covered_patterns, reference.covered_patterns);
        assert_eq!(imported.covered_weight, reference.covered_weight);
        assert_eq!(
            imported.covered_pattern_count,
            reference.covered_pattern_count
        );
        assert_eq!(imported.min_accepted_depth, Some(11));
        assert_eq!(imported.max_accepted_depth, Some(11));
    }
}
