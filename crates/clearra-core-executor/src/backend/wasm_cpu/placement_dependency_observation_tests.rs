//! Product-route qualification and a separate sampled ten-lock consumer check.
//! The complete P7 test reaches the ordinary executor. The second test uses
//! one real geometric identity and an explicitly finite weighted queue sample;
//! it makes no completeness claim about the full P7P4 geometry or supply family.

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
use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
use clearra_rules::{kicks::KickTableProfileId, profile::builtin_rules::srs_plus};
use clearra_supply::{
    pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
    QueueObservationPolicy,
};

use super::{
    piece_code, ObservationLanguageNode, ObservationPieceLanguage, PhysicalActionLanguage,
    QueueObservationPolicyEvaluator,
};
use crate::backend::wasm_cpu::{
    buildup::with_product_verifier_counts,
    catalog::GeometryCatalog,
    geometry::GeometryCandidate,
    reachability::ReachabilityWorkspace,
    symbolic_prefix_orders::{SymbolicPrefixOrders, SymbolicPrefixWork},
    with_pc_placement_dependency_reference,
};
use crate::{CoreExecutionResult, WasmCpuSearchBackend};

#[test]
fn dependency_product_p7_oracle_and_see_seven_match_without_opt_in() {
    let make_problem = |observation| {
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
            .with_queue_observation_policy(observation)
            .with_retained_trace_limit(0)
            .with_execution_policy(
                PcExecutionPolicy::mvp_default()
                    .with_requested_backend(RequestedSearchBackend::Cpu)
                    .with_allow_backend_fallback(false)
                    .with_workers(1)
                    .with_tablebase_requested(false)
                    .with_precompute_build_dependencies(false),
            ),
        )
        .expect("qualified six-lock P7 request")
        // This qualification compares the complete physical solution set as
        // well as queue coverage. CoverageSummary intentionally reports its
        // solution count as not-calculated and cannot supply that evidence.
        .with_output_policy(SearchOutputPolicy::Summary)
    };
    let oracle_problem = make_problem(QueueObservationPolicy::FullQueueOracle);
    let see_seven_problem = make_problem(QueueObservationPolicy::VisibleSeven);
    let execute = |problem| {
        with_product_verifier_counts(|| {
            WasmCpuSearchBackend::execute_with_control(problem, &ExecutionControl::default())
                .expect("complete bounded P7 consumer")
        })
    };
    let (reference, (reference_build_up, reference_proof)) =
        with_pc_placement_dependency_reference(|| execute(&oracle_problem));
    let (oracle, (oracle_build_up, oracle_proof)) = execute(&oracle_problem);
    let (see_seven, (see_seven_build_up, see_seven_proof)) = execute(&see_seven_problem);

    assert!(!oracle_problem
        .backend_policy()
        .precompute_build_dependencies());
    assert!(!see_seven_problem
        .backend_policy()
        .precompute_build_dependencies());
    assert!(reference_build_up > 0);
    assert_eq!(reference_proof, 0);
    assert_eq!((oracle_build_up, see_seven_build_up), (0, 0));
    assert!(oracle_proof > 0 && see_seven_proof > 0);
    for result in [&reference, &oracle, &see_seven] {
        assert_complete_nonempty(result);
        assert_eq!(result.usize_field("coverage_pattern_count"), Some(5_040));
        assert_eq!(
            result.usize_field("normalized_unique_solution_count"),
            Some(246)
        );
        assert_eq!(result.normalized_solution_identities().len(), 246);
    }
    assert_eq!(
        oracle.normalized_solution_identities(),
        reference.normalized_solution_identities()
    );
    assert_eq!(
        see_seven.normalized_solution_identities(),
        oracle.normalized_solution_identities()
    );
    assert_eq!(
        oracle.coverage_pattern_words(),
        reference.coverage_pattern_words()
    );
    // All seven actual draws are visible initially. Holding changes the
    // order of use but does not hide any source draw, so this policy has
    // exactly the oracle's success set, not merely the same cardinality.
    assert_eq!(
        see_seven.coverage_pattern_words(),
        oracle.coverage_pattern_words()
    );
    assert_eq!(
        see_seven.field("coverage_probability"),
        oracle.field("coverage_probability")
    );
    assert_eq!(oracle.usize_field("total_build_order_nodes"), Some(0));
    assert_eq!(see_seven.usize_field("total_build_order_nodes"), Some(0));
    assert_eq!(oracle.usize_field("total_reachability_states"), Some(0));
    assert_eq!(see_seven.usize_field("total_reachability_states"), Some(0));
    assert!(see_seven.usize_field("observation_policy_states").unwrap() > 0);
    assert!(
        see_seven
            .usize_field("observation_policy_action_checks")
            .unwrap()
            > 0
    );
}

fn assert_complete_nonempty(result: &CoreExecutionResult) {
    assert!(result.solution_found());
    assert!(result
        .coverage_pattern_words()
        .iter()
        .any(|&word| word != 0));
    assert_eq!(result.bool_field("count_complete"), Some(true));
    assert_eq!(result.bool_field("probability_complete"), Some(true));
    assert_eq!(result.bool_field("resource_truncated"), Some(false));
}

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
            .map(|(operation, piece)| (piece_code(piece), node | (1 << operation)))
    }
}

#[test]
fn dependency_real_jstris_ten_lock_relation_preserves_sampled_see_seven_policy() {
    use PieceKind::{I, J, L, O, S, T, Z};

    // The same real empty-origin identity used by the existing independent
    // Jstris P7P4 coverage qualification. IOTZSLJZTI is a no-hold witness.
    let placements = [
        (I, 0xf),
        (I, 0xf0),
        (O, 0xc0300),
        (T, 0x201c00),
        (Z, 0xc06000),
        (Z, 0x1006008000),
        (S, 0x18030000),
        (L, 0x1c0100000),
        (T, 0xe01000000),
        (J, 0xe020000000),
    ];
    let required = (1_u64 << 40) - 1;
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, required).unwrap();
    let rows = placements.map(|(piece, cells)| catalog.skeleton_id(piece, cells).unwrap());
    let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
    let control = ExecutionControl::default();
    let orders = SymbolicPrefixOrders::compile_product_with_workspace(
        &catalog,
        &candidate,
        KickTableProfileId::Jstris180,
        &control,
        &mut ReachabilityWorkspace::default(),
        &mut SymbolicPrefixWork::default(),
        0,
    )
    .expect("complete concrete operation relation");
    let mut physical = PhysicalActionLanguage::default();
    orders
        .compile_observation_language(&mut physical, &control)
        .expect("exact physical action quotient");
    let view = physical.view(&control).unwrap().unwrap();

    let suffix = [I, O, T, Z];
    let mut sequences = Vec::new();
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    if a == b || a == c || a == d || b == c || b == d || c == d {
                        continue;
                    }
                    let mut sequence = vec![I, O, T, Z, S, L, J];
                    sequence.extend([suffix[a], suffix[b], suffix[c], suffix[d]]);
                    sequences.push(sequence);
                }
            }
        }
    }
    assert_eq!(sequences.len(), 24);
    // Distinct powers of two give every subset a distinct positive weight.
    // This excludes policy-tie differences from the selected-pattern check.
    let weights = (0..24)
        .map(|index| ProbabilityValue::new(f64::from(1_u32 << index) / 16_777_215.0).unwrap())
        .collect();
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(74),
        PatternWeightModelId::new(74),
        sequences,
        weights,
        24,
        true,
        None,
    )
    .unwrap();
    let index = PatternPiecePositionIndex::compile(&universe).unwrap();
    for hold in [false, true] {
        let evaluator = || {
            QueueObservationPolicyEvaluator::new(
                &universe,
                &index,
                QueueObservationPolicy::VisibleSeven,
                0,
                None,
                hold,
                true,
                false,
                None,
            )
            .unwrap()
        };
        let concrete = evaluator()
            .evaluate(&ConcretePrefixPolicy(&orders), &control)
            .unwrap();
        let projected = evaluator().evaluate(&view, &control).unwrap();
        assert!(
            concrete.covered_pattern_count > 0,
            "nonvacuous physical fixture"
        );
        assert_eq!(projected.covered_patterns, concrete.covered_patterns);
        assert_eq!(
            projected.covered_pattern_count,
            concrete.covered_pattern_count
        );
        assert_eq!(projected.covered_weight, concrete.covered_weight);
        assert_eq!(projected.min_accepted_depth, Some(10));
        assert_eq!(projected.max_accepted_depth, Some(10));
    }
}
