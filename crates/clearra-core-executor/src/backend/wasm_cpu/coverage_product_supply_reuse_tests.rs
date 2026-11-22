//! Boolean idempotence preserves original queue identities, not action counts.

use std::cell::Cell;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::piece::rotation::RotationState;
use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_coverage::pattern::pattern_id::PatternId;
use clearra_coverage::universe::pattern_universe_id::PatternUniverseId;
use clearra_coverage::universe::pattern_weight_model_id::PatternWeightModelId;
use clearra_supply::execution_automaton::{
    SupplyBranchKind, SupplyExecutionAutomaton, SupplyObservationIdentity, SupplyProvenanceId,
};
use clearra_supply::hold::hold_policy::HoldPolicy;
use clearra_supply::hold_automaton::HoldAutomatonState;
use clearra_supply::pattern_universe::{
    MaterializedPatternUniverse, PatternPiecePositionIndex, PatternUniverseMaterializer,
};
use clearra_supply::piece_source::{PieceSourceId, PieceSourceKind};
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

use super::super::buildup::{BuildOrderGraph, BuildOrderNodeSpec};
use super::super::piece_order_language::PieceOrderLanguageCache;
use super::{CoverageProductEvaluator, CoverageProductResult};

#[derive(Clone, Copy)]
enum SupplyReuse {
    Enabled,
    TransitionsOnly,
    Reference,
}

#[derive(Clone, Copy, Debug, Default)]
struct SupplyWork {
    position_word_reads: usize,
    transition_calls: usize,
    nonempty_transitions: usize,
}

thread_local! {
    static SUPPLY_REUSE: Cell<SupplyReuse> = const { Cell::new(SupplyReuse::Enabled) };
    static SUPPLY_WORK: Cell<SupplyWork> = const {
        Cell::new(SupplyWork {
            position_word_reads: 0,
            transition_calls: 0,
            nonempty_transitions: 0,
        })
    };
}

pub(super) fn reuse_supply_steps() -> bool {
    SUPPLY_REUSE.with(|mode| !matches!(mode.get(), SupplyReuse::Reference))
}

pub(super) fn reuse_layer_source_words() -> bool {
    SUPPLY_REUSE.with(|mode| matches!(mode.get(), SupplyReuse::Enabled))
}

pub(super) fn record_position_word_read() {
    SUPPLY_WORK.with(|work| {
        let mut value = work.get();
        value.position_word_reads = value.position_word_reads.saturating_add(1);
        work.set(value);
    });
}

pub(super) fn record_transition_activation(bits: u64) {
    SUPPLY_WORK.with(|work| {
        let mut value = work.get();
        value.transition_calls = value.transition_calls.saturating_add(1);
        value.nonempty_transitions = value
            .nonempty_transitions
            .saturating_add(usize::from(bits != 0));
        work.set(value);
    });
}

fn measure<T>(mode: SupplyReuse, operation: impl FnOnce() -> T) -> (T, SupplyWork) {
    struct Restore(SupplyReuse);
    impl Drop for Restore {
        fn drop(&mut self) {
            SUPPLY_REUSE.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(SUPPLY_REUSE.with(|cell| cell.replace(mode)));
    SUPPLY_WORK.with(|work| work.set(SupplyWork::default()));
    let value = operation();
    (value, SUPPLY_WORK.with(Cell::get))
}

fn chain(pieces: &[PieceKind]) -> BuildOrderGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for (depth, &piece) in pieces.iter().enumerate() {
        nodes.push(BuildOrderNodeSpec {
            edge_start: depth as u32,
            edge_count: 1,
            depth: depth as u8,
            accepting: false,
        });
        edges.push(BuildOrderGraph::edge(
            depth as u32 + 1,
            depth as u8,
            piece,
            RotationState::Zero,
            0,
            0,
            0,
        ));
    }
    nodes.push(BuildOrderNodeSpec {
        edge_start: edges.len() as u32,
        edge_count: 0,
        depth: pieces.len() as u8,
        accepting: true,
    });
    BuildOrderGraph::from_topological_parts(nodes, edges, 0, 0, true).unwrap()
}

fn initial(cursor: u16, held: Option<PieceKind>, enabled: bool) -> HoldAutomatonState {
    HoldAutomatonState::with_contract(
        PieceSourceId::new(98),
        PieceSourceKind::MaterializedPatternUniverse,
        cursor,
        held,
        if enabled {
            HoldPolicy::Allowed
        } else {
            HoldPolicy::Forbidden
        },
        0,
        0,
        SupplyObservationIdentity::full_queue_oracle(),
        SupplyProvenanceId(98),
    )
}

fn explicit_source(sequences: Vec<Vec<PieceKind>>) -> MaterializedPatternUniverse {
    let count = sequences.len();
    MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(98),
        PatternWeightModelId::new(98),
        sequences,
        (0..count)
            .map(|ordinal| {
                // Zero weight does not remove original ordinal membership.
                ProbabilityValue::new(if count == 1 {
                    1.0
                } else if ordinal == 0 {
                    0.0
                } else {
                    1.0 / (count - 1) as f64
                })
                .unwrap()
            })
            .collect(),
        count as u128,
        true,
        None,
    )
    .unwrap()
}

// Count labeled canonical transitions separately, including equal Current and
// Swap destinations. This finite oracle has no product bitmap or memo reuse.
fn finite_histories(
    queue: &[PieceKind],
    word: &[PieceKind],
    initial: HoldAutomatonState,
    terminal_release: bool,
) -> u128 {
    let automaton = SupplyExecutionAutomaton::sequence();
    let mut states = vec![(initial, 1_u128)];
    for (depth, &desired) in word.iter().enumerate() {
        let mut next = Vec::<(HoldAutomatonState, u128)>::new();
        for (state, count) in states {
            let cursor = usize::from(state.cursor());
            if cursor == queue.len() {
                if terminal_release
                    && depth + 1 == word.len()
                    && state.hold_policy() != HoldPolicy::Forbidden
                    && state.hold_piece() == Some(desired)
                {
                    let mut released = state;
                    released.hold_piece = None;
                    released.hold_empty = true;
                    next.push((released, count));
                }
                continue;
            }
            for branch in [
                SupplyBranchKind::Current,
                SupplyBranchKind::SwapHeld,
                SupplyBranchKind::StoreCurrent,
            ] {
                let Ok(step) = automaton.transition(
                    state,
                    branch,
                    queue[cursor],
                    queue.get(cursor + 1).copied(),
                ) else {
                    continue;
                };
                if step.used_piece == desired {
                    next.push((step.next_state, count));
                }
            }
        }
        states = next;
    }
    states.into_iter().map(|(_, count)| count).sum()
}

fn assert_same_result(a: &CoverageProductResult, b: &CoverageProductResult) {
    assert_eq!(a.coverage_bits, b.coverage_bits);
    assert_eq!(a.path_count, b.path_count);
    assert_eq!(a.count_complete, b.count_complete);
    assert_eq!(a.processed_words, b.processed_words);
    assert_eq!(a.active_states, b.active_states);
    assert_eq!(a.edge_checks, b.edge_checks);
}

#[allow(clippy::too_many_arguments)]
fn compare(
    source: &MaterializedPatternUniverse,
    index: &PatternPiecePositionIndex,
    ordinals: &[usize],
    word: &[PieceKind],
    initial: HoldAutomatonState,
    terminal_release: bool,
    standard_complement: bool,
) -> [(SupplyWork, SupplyWork); 2] {
    let graph = chain(word);
    let control = ExecutionControl::default();
    let mut expected = PatternBitSet::new(source.pattern_count());
    let mut expected_histories = 0;
    for &ordinal in ordinals {
        let mut queue = source.sequence_at(ordinal).to_vec();
        if standard_complement && queue.len() == 6 {
            let missing = PieceKind::STANDARD_TETROMINOES
                .into_iter()
                .find(|piece| !queue.contains(piece))
                .unwrap();
            queue.push(missing);
        }
        let histories = finite_histories(&queue, word, initial, terminal_release);
        if histories != 0 {
            expected.insert(PatternId::new(ordinal)).unwrap();
        }
        expected_histories += histories;
    }
    std::array::from_fn(|mode| {
        let count_paths = mode != 0;
        let execute = |strategy| {
            let mut evaluator = CoverageProductEvaluator::default();
            let (result, work) = measure(strategy, || {
                evaluator
                    .evaluate(
                        &graph,
                        index,
                        initial,
                        initial.hold_policy() != HoldPolicy::Forbidden,
                        terminal_release,
                        standard_complement,
                        count_paths,
                        false,
                        &control,
                    )
                    .unwrap()
            });
            (result, work, evaluator.retained_bytes())
        };
        let (reference, old_work, old_bytes) = execute(SupplyReuse::Reference);
        let (reused, new_work, new_bytes) = execute(SupplyReuse::Enabled);
        assert_same_result(&reference, &reused);
        assert_eq!(reused.coverage_bits, expected);
        assert!(reused.count_complete);
        assert_eq!(
            reused.path_count,
            if count_paths { expected_histories } else { 0 }
        );
        assert_eq!(new_bytes, old_bytes);
        assert!(new_work.position_word_reads <= old_work.position_word_reads);
        assert!(new_work.transition_calls <= old_work.transition_calls);
        if count_paths {
            assert_eq!(new_work.nonempty_transitions, old_work.nonempty_transitions);
        }

        // The rooted suffix adapter shares this Boolean kernel but retains
        // its own dynamic-node storage. Its original ordinal result must match.
        if !count_paths {
            let mut language = PieceOrderLanguageCache::default();
            let root = language.canonicalize(&graph).unwrap();
            let projected = CoverageProductEvaluator::default()
                .evaluate_language(
                    &language,
                    root,
                    index,
                    initial,
                    initial.hold_policy() != HoldPolicy::Forbidden,
                    terminal_release,
                    standard_complement,
                    &control,
                )
                .unwrap();
            assert_eq!(projected.coverage_bits, expected);
        }
        (old_work, new_work)
    })
}

#[test]
fn supply_step_reuse_keeps_sparse_ordinals_initial_hold_and_labeled_counts() {
    use PieceKind::{I, O, T};

    let tails = [[O, O], [O, T], [T, O], [I, I]];
    let source = explicit_source(
        (0..130)
            .map(|ordinal| [I, tails[ordinal % 4][0], tails[ordinal % 4][1]].to_vec())
            .collect(),
    );
    let ordinals = [0, 1, 2, 63, 64, 65, 127, 129];
    let subset = PatternBitSet::from_pattern_indices(
        source.pattern_count(),
        ordinals.iter().map(|&ordinal| ordinal as u32).collect(),
    )
    .unwrap();
    let index = PatternPiecePositionIndex::compile_subset_before(&source, &subset, 130).unwrap();
    let measured = compare(
        &source,
        &index,
        &ordinals,
        &[O, O],
        initial(1, Some(O), true),
        false,
        false,
    );
    assert!(measured[0].1.position_word_reads < measured[0].0.position_word_reads);
    assert!(measured[0].1.nonempty_transitions < measured[0].0.nonempty_transitions);
    assert_eq!(
        measured[1].1.nonempty_transitions,
        measured[1].0.nonempty_transitions
    );
    assert_eq!(
        finite_histories(&[O, O], &[O, O], initial(0, Some(O), true), false),
        4
    );
    eprintln!(
        "supply_step_reuse sparse Boolean {:?} -> {:?}; CountAll {:?} -> {:?}",
        measured[0].0, measured[0].1, measured[1].0, measured[1].1
    );
}

#[test]
fn supply_step_reuse_keeps_fixed_partial_atom_store_and_finite_eof() {
    use PieceKind::{I, O, T};

    let mut saved_reads = 0;
    for (queue, word, held, terminal) in [
        (vec![I, O, T], vec![O, T], None, false),
        (vec![O], vec![O, T], Some(T), false),
        (vec![O], vec![O, T], Some(T), true),
        (vec![O, T], vec![T, O], None, true),
    ] {
        let source = explicit_source(vec![queue]);
        let index = PatternPiecePositionIndex::compile(&source).unwrap();
        let measured = compare(
            &source,
            &index,
            &[0],
            &word,
            initial(0, held, true),
            terminal,
            false,
        );
        for (a, b) in measured {
            saved_reads += a.position_word_reads - b.position_word_reads;
        }
    }
    let source = PatternUniverseMaterializer::queue_pattern_expression(
        &QueuePatternExpression::parse("[IOT]2[LI]1", 0).unwrap(),
        98,
    )
    .unwrap();
    let index = PatternPiecePositionIndex::compile(&source).unwrap();
    let ordinals = (0..source.pattern_count()).collect::<Vec<_>>();
    for enabled in [false, true] {
        for (a, b) in compare(
            &source,
            &index,
            &ordinals,
            &[I, O],
            initial(0, None, enabled),
            false,
            false,
        ) {
            saved_reads += a.position_word_reads - b.position_word_reads;
        }
    }
    assert!(saved_reads > 0);
    eprintln!("supply_step_reuse fixed/partial atom saved position reads={saved_reads}");
}

#[test]
fn supply_step_reuse_keeps_standard_bag_projected_complement() {
    use PieceKind::{I, J, L, O, S, T, Z};

    let source = PatternUniverseMaterializer::standard_7_bag(6, 0, 98).unwrap();
    let index = PatternPiecePositionIndex::compile(&source).unwrap();
    let ordinals = (0..source.pattern_count()).collect::<Vec<_>>();

    // A six-draw standard source projects one additional current piece for
    // six locks with empty Hold, or seven locks with an initially held piece.
    // The complement participates in normal Current/Swap/Store transitions;
    // it does not change the source into a finite seven-draw EOF contract.
    // Neither supported horizon below can release Hold after consuming that
    // projected seventh piece. The independent oracle can therefore use its
    // explicit seven-piece completion without manufacturing an eighth draw.
    for (word, held) in [
        (&[I, O, T, S, Z, J][..], None),
        (&[I, O, T, S, Z, J, L][..], Some(L)),
    ] {
        let measured = compare(
            &source,
            &index,
            &ordinals,
            word,
            initial(0, held, true),
            true,
            true,
        );
        assert!(measured[0].1.position_word_reads < measured[0].0.position_word_reads);
        eprintln!(
            "supply_step_reuse projected complement held={held:?} {:?} -> {:?}",
            measured[0].0, measured[0].1
        );
    }

    // Full seven-draw inputs have a different terminal boundary. Keep that
    // finite EOF release covered separately, including a concrete queue whose
    // target word requires the terminal held piece after one initial Store.
    let queue = [O, I, T, S, Z, J, L];
    let word = [I, O, T, S, Z, J, L];
    assert_eq!(
        finite_histories(&queue, &word, initial(0, None, true), false),
        0
    );
    assert!(finite_histories(&queue, &word, initial(0, None, true), true) > 0);
    let source = PatternUniverseMaterializer::standard_7_bag(7, 0, 98).unwrap();
    let index = PatternPiecePositionIndex::compile(&source).unwrap();
    let ordinals = (0..source.pattern_count()).collect::<Vec<_>>();
    compare(
        &source,
        &index,
        &ordinals,
        &word,
        initial(0, None, true),
        true,
        false,
    );
}

#[test]
fn layer_source_words_share_supply_without_merging_dependency_paths() {
    use PieceKind::{I, J, O, T};

    // Two dependency orders converge before T. The first I placement also
    // has two concrete realizations: Boolean coverage may share their supply
    // factor, but CountAll still owns both physical execution histories.
    let edges = vec![
        BuildOrderGraph::edge(1, 0, I, RotationState::Zero, 0, 0, 0),
        BuildOrderGraph::edge(1, 0, I, RotationState::Right, 0, 0, 0),
        BuildOrderGraph::edge(2, 1, O, RotationState::Zero, 0, 0, 0),
        BuildOrderGraph::edge(3, 1, O, RotationState::Zero, 0, 0, 0),
        BuildOrderGraph::edge(3, 0, I, RotationState::Zero, 0, 0, 0),
        BuildOrderGraph::edge(4, 2, T, RotationState::Zero, 0, 0, 0),
    ];
    let nodes = [(0, 3, 0), (3, 1, 1), (4, 1, 1), (5, 1, 2), (6, 0, 3)]
        .into_iter()
        .map(|(edge_start, edge_count, depth)| BuildOrderNodeSpec {
            edge_start,
            edge_count,
            depth,
            accepting: depth == 3,
        })
        .collect();
    let graph = BuildOrderGraph::from_topological_parts(nodes, edges, 0, 0, false).unwrap();
    let queues = [[I, O, T, J], [O, I, T, I], [T, I, O, J], [J, T, O, I]];
    let sources = [
        explicit_source(
            (0..130)
                .map(|index| queues[index % queues.len()].to_vec())
                .collect(),
        ),
        PatternUniverseMaterializer::queue_pattern_expression(
            &QueuePatternExpression::parse("[IOT]3[IJ]1", 0).unwrap(),
            99,
        )
        .unwrap(),
    ];
    let mut evaluator = CoverageProductEvaluator::default();
    let mut reference_reads = 0;
    let mut reused_reads = 0;
    for source in sources {
        let index = PatternPiecePositionIndex::compile(&source).unwrap();
        for cursor in [0, 1] {
            for held in [None, Some(O), Some(T)] {
                for enabled in [false, true] {
                    if !enabled && held.is_some() {
                        // The public supply contract forbids a populated
                        // Hold slot when the Hold policy is disabled.
                        continue;
                    }
                    let initial = initial(cursor, held, enabled);
                    let mut expected = PatternBitSet::new(source.pattern_count());
                    let mut expected_histories = 0;
                    for ordinal in 0..source.pattern_count() {
                        let queue = source.sequence_at(ordinal);
                        let histories = 2 * finite_histories(&queue, &[I, O, T], initial, true)
                            + finite_histories(&queue, &[O, I, T], initial, true);
                        if histories != 0 {
                            expected.insert(PatternId::new(ordinal)).unwrap();
                        }
                        expected_histories += histories;
                    }
                    for count_paths in [false, true] {
                        let mut execute = |strategy| {
                            let (result, work) = measure(strategy, || {
                                evaluator
                                    .evaluate(
                                        &graph,
                                        &index,
                                        initial,
                                        enabled,
                                        true,
                                        false,
                                        count_paths,
                                        false,
                                        &ExecutionControl::default(),
                                    )
                                    .unwrap()
                            });
                            (result, work, evaluator.retained_bytes())
                        };
                        let (reference, old_work, old_bytes) =
                            execute(SupplyReuse::TransitionsOnly);
                        let (reused, new_work, new_bytes) = execute(SupplyReuse::Enabled);
                        assert_same_result(&reference, &reused);
                        assert_eq!(old_bytes, new_bytes);
                        assert_eq!(reused.coverage_bits, expected);
                        assert_eq!(
                            reused.path_count,
                            if count_paths { expected_histories } else { 0 }
                        );
                        assert!(reused.count_complete);
                        assert_eq!(new_work.transition_calls, old_work.transition_calls);
                        assert_eq!(new_work.nonempty_transitions, old_work.nonempty_transitions);
                        assert!(new_work.position_word_reads <= old_work.position_word_reads);
                        assert!(
                            new_work.position_word_reads <= 14 * 3 * index.word_count(),
                            "each requested kind is loaded at most once per source position/layer"
                        );
                        reference_reads += old_work.position_word_reads;
                        reused_reads += new_work.position_word_reads;
                    }
                }
            }
        }
    }
    assert!(reused_reads < reference_reads);
    eprintln!("layer_source_words position reads {reference_reads} -> {reused_reads}");
}
