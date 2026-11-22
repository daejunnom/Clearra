//! A kind-word union is exact for an oracle existential language, but is not
//! an action quotient for a policy that must commit before the next reveal.

use clearra_core_domain::{
    execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
    probability::probability_value::ProbabilityValue,
};
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};
use clearra_supply::{
    pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
    QueueObservationPolicy,
};

use super::{
    piece_code, ObservationLanguageNode, ObservationPieceLanguage, PhysicalActionEdge,
    PhysicalActionLanguage, QueueObservationCoverage, QueueObservationPolicyEvaluator,
    RootedPieceLanguageUnion,
};
use crate::backend::wasm_cpu::piece_order_language::{
    ConditionalLanguageCompilation, ConditionalTransitionProof, PieceOrderLanguageCache,
};

/// Ten labelled operations and two complete operation paths. Both first
/// operations are I, but each commits to a different suffix. This is an
/// exact finite relation fixture, not a claim that a geometric tiling with
/// precisely these guards has been found.
struct CommittedOperationLanguage {
    pieces: [PieceKind; 10],
    allowed: Vec<u16>,
}

impl CommittedOperationLanguage {
    fn hidden_eighth_choice() -> Self {
        use PieceKind::{I, J, L, O, S, T, Z};

        let pieces = [I, I, O, O, T, T, S, Z, J, L];
        let mut allowed = vec![0_u16; 1 << pieces.len()];
        for path in [
            [0, 2, 4, 6, 7, 8, 9, 1, 3, 5],
            [1, 2, 4, 6, 7, 8, 9, 3, 0, 5],
        ] {
            let mut prefix = 0;
            for operation in path {
                allowed[prefix] |= 1 << operation;
                prefix |= 1 << operation;
            }
            assert_eq!(prefix, (1 << pieces.len()) - 1);
        }
        Self { pieces, allowed }
    }

    fn terminal(&self) -> u32 {
        (1 << self.pieces.len()) - 1
    }
}

impl ObservationPieceLanguage for CommittedOperationLanguage {
    fn root(&self) -> u32 {
        0
    }

    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        let allowed = *self.allowed.get(node as usize)?;
        (node == self.terminal() || allowed != 0).then_some(ObservationLanguageNode {
            accepting: node == self.terminal(),
            depth: node.count_ones() as u8,
        })
    }

    fn edge_count(&self, node: u32) -> Option<usize> {
        self.node(node)?;
        Some(self.allowed[node as usize].count_ones() as usize)
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        self.node(node)?;
        let mut allowed = self.allowed[node as usize];
        for _ in 0..index {
            if allowed == 0 {
                return None;
            }
            allowed &= allowed - 1;
        }
        if allowed == 0 {
            return None;
        }
        let operation = allowed.trailing_zeros() as usize;
        Some((piece_code(self.pieces[operation]), node | (1 << operation)))
    }
}

fn p7p4_hidden_eighth_universe(
    weights: [f64; 2],
) -> (MaterializedPatternUniverse, PatternPiecePositionIndex) {
    use PieceKind::{I, J, L, O, S, T, Z};

    // Each queue is a legal P7P4 word. The last J is unplaced lookahead.
    // These are two weighted samples, not a complete standard-bag universe.
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(73),
        PatternWeightModelId::new(73),
        vec![
            vec![I, O, T, S, Z, J, L, I, O, T, J],
            vec![I, O, T, S, Z, J, L, O, I, T, J],
        ],
        weights
            .into_iter()
            .map(|weight| ProbabilityValue::new(weight).expect("weight"))
            .collect(),
        2,
        true,
        None,
    )
    .expect("two hidden-suffix queues");
    let index = PatternPiecePositionIndex::compile(&universe).expect("index");
    (universe, index)
}

fn evaluate_no_hold(
    universe: &MaterializedPatternUniverse,
    index: &PatternPiecePositionIndex,
    language: &impl ObservationPieceLanguage,
) -> QueueObservationCoverage {
    QueueObservationPolicyEvaluator::new(
        universe,
        index,
        QueueObservationPolicy::VisibleSeven,
        0,
        None,
        false,
        true,
        false,
        None,
    )
    .expect("see-seven evaluator")
    .evaluate(language, &ExecutionControl::default())
    .expect("exact policy")
}

fn operation_cells(pieces: &[PieceKind; 10]) -> [u64; 10] {
    use clearra_core_domain::piece::rotation::RotationState;
    use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

    // Valid, distinct four-cell action keys. This fixture qualifies the
    // language quotient; its abstract relation is not a physical tiling.
    std::array::from_fn(|operation| {
        let shape = standard_tetromino_registry()
            .get(pieces[operation])
            .unwrap()
            .shape(RotationState::Zero);
        let x = operation % 5;
        let y = (operation / 5) * 3;
        shape.cells().iter().fold(0, |mask, cell| {
            mask | (1_u64 << ((y + cell.y() as usize) * 10 + x + cell.x() as usize))
        })
    })
}

fn concrete_physical_language(concrete: &CommittedOperationLanguage) -> PhysicalActionLanguage {
    let control = ExecutionControl::default();
    let cells = operation_cells(&concrete.pieces);
    let mut owner = PhysicalActionLanguage::default();
    let mut nodes = vec![None; concrete.allowed.len()];
    for prefix in (0..concrete.allowed.len()).rev() {
        let Some(node) = concrete.node(prefix as u32) else {
            continue;
        };
        let mut edges = Vec::new();
        for (operation, &piece) in concrete.pieces.iter().enumerate() {
            let bit = 1 << operation;
            if concrete.allowed[prefix] & bit != 0 {
                let child = nodes[prefix | usize::from(bit)].expect("child first");
                edges.push(PhysicalActionEdge::new(piece, cells[operation], child).unwrap());
            }
        }
        nodes[prefix] = Some(
            owner
                .intern_node(10 - node.depth, &mut edges, &control)
                .expect("exact physical continuation"),
        );
    }
    owner.insert_root(nodes[0].unwrap()).unwrap();
    owner
}

#[test]
fn see_seven_same_kind_determinization_changes_policy_probability() {
    let concrete = CommittedOperationLanguage::hidden_eighth_choice();
    let mut cache = PieceOrderLanguageCache::default();
    let compiled = cache
        .compile_conditional_language(&concrete.pieces, 64, 256, |prefix, operation| {
            if concrete.allowed[usize::from(prefix)] & (1 << operation) != 0 {
                ConditionalTransitionProof::Allowed
            } else {
                ConditionalTransitionProof::Forbidden
            }
        })
        .expect("exact finite operation relation");
    let ConditionalLanguageCompilation::Certified(compiled) = compiled else {
        panic!("fixture refused: {compiled:?}");
    };
    let root = compiled.root.expect("nonempty language");
    assert_eq!(
        cache
            .edges_for_piece(root, piece_code(PieceKind::I))
            .expect("root I choices")
            .len(),
        2,
        "same kind leaves two different concrete residuals"
    );
    let kind_union = RootedPieceLanguageUnion::new(&cache, &[root]).expect("kind-word union");
    assert_eq!(kind_union.edge_count(kind_union.root()), Some(1));
    let mut physical = concrete_physical_language(&concrete);
    let control = ExecutionControl::default();
    let physical = physical.view(&control).unwrap().unwrap();
    assert_eq!(physical.edge_count(physical.root()), Some(2));

    for (weights, selected_word, selected_weight) in
        [([0.5, 0.5], 0b01, 0.5), ([0.25, 0.75], 0b10, 0.75)]
    {
        let (universe, index) = p7p4_hidden_eighth_universe(weights);
        let committed = evaluate_no_hold(&universe, &index, &concrete);
        let physical = evaluate_no_hold(&universe, &index, &physical);
        let determinized = evaluate_no_hold(&universe, &index, &kind_union);
        assert_eq!(committed.covered_patterns.words(), &[selected_word]);
        assert_eq!(committed.covered_pattern_count, 1);
        assert_eq!(committed.covered_weight, selected_weight);
        assert_eq!(committed.min_accepted_depth, Some(10));
        assert_eq!(committed.max_accepted_depth, Some(10));
        assert_eq!(
            physical.covered_pattern_count,
            committed.covered_pattern_count
        );
        assert_eq!(physical.covered_weight, committed.covered_weight);
        if weights[0] != weights[1] {
            assert_eq!(
                physical.covered_patterns.words(),
                committed.covered_patterns.words()
            );
        } else {
            // A different canonical node numbering can choose the other
            // equally good committed policy. It must still choose exactly
            // one coherent branch, never both hidden suffixes.
            assert_eq!(physical.covered_patterns.count_ones(), 1);
        }

        // The oracle language correctly contains both complete words, but
        // determinizing by kind postpones choosing the first I operation
        // until the hidden eighth piece has been revealed. That is not a
        // legal concrete policy action.
        assert_eq!(determinized.covered_patterns.words(), &[0b11]);
        assert_eq!(determinized.covered_pattern_count, 2);
        assert_eq!(determinized.covered_weight, 1.0);
        assert_eq!(determinized.min_accepted_depth, Some(10));
        assert_eq!(determinized.max_accepted_depth, Some(10));
    }
}

#[test]
fn see_seven_root_union_retains_only_histories_of_the_same_physical_action() {
    let concrete = CommittedOperationLanguage::hidden_eighth_choice();
    let cells = operation_cells(&concrete.pieces);
    let paths = [
        [0, 2, 4, 6, 7, 8, 9, 1, 3, 5],
        [1, 2, 4, 6, 7, 8, 9, 3, 0, 5],
    ];
    let control = ExecutionControl::default();
    let (universe, index) = p7p4_hidden_eighth_universe([0.5, 0.5]);
    for same_first_action in [false, true] {
        let mut owner = PhysicalActionLanguage::default();
        for path in paths {
            let mut edges = Vec::new();
            let mut root = owner.intern_node(0, &mut edges, &control).unwrap();
            for depth in (0..path.len()).rev() {
                let operation = path[depth];
                let mask = if same_first_action && depth == 0 {
                    cells[0]
                } else {
                    cells[operation]
                };
                edges
                    .push(PhysicalActionEdge::new(concrete.pieces[operation], mask, root).unwrap());
                root = owner
                    .intern_node((path.len() - depth) as u8, &mut edges, &control)
                    .unwrap();
                edges.clear();
            }
            owner.insert_root(root).unwrap();
        }
        let view = owner.view(&control).unwrap().unwrap();
        let coverage = evaluate_no_hold(&universe, &index, &view);
        if same_first_action {
            // Both candidate hypotheses survived the same physical move.
            // The next reveal may now select their different continuations.
            assert_eq!(view.edge_count(view.root()), Some(1));
            assert_eq!(coverage.covered_patterns.words(), &[0b11]);
            assert_eq!(coverage.covered_weight, 1.0);
        } else {
            // Distinct physical first moves must be selected before reveal.
            assert_eq!(view.edge_count(view.root()), Some(2));
            assert_eq!(coverage.covered_pattern_count, 1);
            assert_eq!(coverage.covered_patterns.count_ones(), 1);
            assert_eq!(coverage.covered_weight, 0.5);
        }
    }
}
