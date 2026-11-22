use clearra_core_domain::{
    piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
};
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};

use super::*;

#[derive(Clone, Default)]
struct Node {
    accepting: bool,
    depth: u8,
    edges: Vec<(u8, u64, u32)>,
}

#[derive(Clone)]
struct Language {
    nodes: Vec<Node>,
    root: u32,
    horizon: Option<u8>,
    physical: bool,
}

impl Language {
    fn paths(paths: &[(u64, Vec<PieceKind>)]) -> Self {
        let mut language = Self {
            nodes: vec![Node::default()],
            root: 0,
            horizon: Some(paths[0].1.len() as u8),
            physical: true,
        };
        for (first_cells, path) in paths {
            let mut node = 0;
            for (depth, piece) in path.iter().copied().enumerate() {
                let child = language.nodes.len() as u32;
                language.nodes.push(Node {
                    depth: depth as u8 + 1,
                    ..Node::default()
                });
                language.nodes[node].edges.push((
                    piece_code(piece),
                    if depth == 0 { *first_cells } else { 0xf },
                    child,
                ));
                node = child as usize;
            }
            language.nodes[node].accepting = true;
        }
        language
    }

    fn one_lock(edges: &[(PieceKind, u64, u32)], accepting: bool) -> Self {
        Self {
            nodes: vec![
                Node {
                    edges: edges
                        .iter()
                        .map(|&(piece, cells, child)| (piece_code(piece), cells, child))
                        .collect(),
                    ..Node::default()
                },
                Node {
                    accepting,
                    depth: 1,
                    ..Node::default()
                },
                Node {
                    accepting,
                    depth: 1,
                    ..Node::default()
                },
            ],
            root: 0,
            horizon: Some(1),
            physical: true,
        }
    }

    fn reverse_storage(&mut self) {
        let last = self.nodes.len() as u32 - 1;
        self.nodes.reverse();
        self.root = last - self.root;
        for node in &mut self.nodes {
            for (_, _, child) in &mut node.edges {
                *child = last - *child;
            }
        }
    }
}

impl ObservationPieceLanguage for Language {
    fn root(&self) -> u32 {
        self.root
    }

    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        self.nodes
            .get(node as usize)
            .map(|node| ObservationLanguageNode {
                accepting: node.accepting,
                depth: node.depth,
            })
    }

    fn edge_count(&self, node: u32) -> Option<usize> {
        self.nodes.get(node as usize).map(|node| node.edges.len())
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        let &(piece, _, child) = self.nodes.get(node as usize)?.edges.get(index)?;
        Some((piece, child))
    }

    fn has_physical_actions(&self, node: u32) -> bool {
        self.physical && self.nodes.get(node as usize).is_some()
    }

    fn physical_action_cells(&self, node: u32, index: usize) -> Option<u64> {
        self.nodes
            .get(node as usize)?
            .edges
            .get(index)
            .map(|edge| edge.1)
    }

    fn fixed_horizon(&self) -> Option<u8> {
        self.horizon
    }
}

fn universe(
    sequences: Vec<Vec<PieceKind>>,
    weights: Vec<f64>,
) -> (MaterializedPatternUniverse, PatternPiecePositionIndex) {
    let count = sequences.len();
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(907),
        PatternWeightModelId::new(908),
        sequences,
        weights
            .into_iter()
            .map(|weight| ProbabilityValue::new(weight).expect("weight"))
            .collect(),
        count as u128,
        true,
        None,
    )
    .expect("universe");
    let index = PatternPiecePositionIndex::compile(&universe).expect("index");
    (universe, index)
}

fn evaluator(
    universe: &MaterializedPatternUniverse,
    index: &PatternPiecePositionIndex,
    cursor: u16,
    held: Option<PieceKind>,
    hold_enabled: bool,
) -> QueueObservationPolicyEvaluator {
    QueueObservationPolicyEvaluator::new(
        universe,
        index,
        QueueObservationPolicy::VisibleSeven,
        cursor,
        held,
        hold_enabled,
        true,
        false,
        None,
    )
    .expect("evaluator")
}

fn first_transition(
    evaluator: &mut QueueObservationPolicyEvaluator,
    language: &Language,
    hold_code: Option<u8>,
) -> (PolicyValue, Option<PolicyTransition>) {
    let control = ExecutionControl::default();
    evaluator.prepare_trie(&control).expect("prepared trie");
    assert_eq!(evaluator.trie.initial_observations.len(), 1);
    let state = PolicyState {
        language_node: language.root(),
        observation_node: evaluator.trie.initial_observations[0],
        source_cursor: evaluator.initial_cursor as u16,
        hold_code: hold_code.unwrap_or(evaluator.initial_hold_code),
    };
    evaluator
        .best_transition(language, state, &control, true)
        .expect("transition")
}

fn covered_ids(coverage: &QueueObservationCoverage) -> Vec<usize> {
    coverage
        .covered_patterns
        .covered_patterns()
        .into_iter()
        .map(|id| id.index())
        .collect()
}

fn branch_bound_hidden_fixture(
    weights: Vec<f64>,
) -> (
    MaterializedPatternUniverse,
    PatternPiecePositionIndex,
    Language,
) {
    use PieceKind::{I, J, L, O, S, T, Z};
    let prefix = vec![I, O, T, S, Z, J, L];
    let mut low = prefix.clone();
    low.push(I);
    let mut middle = prefix.clone();
    middle.push(O);
    let mut high = prefix;
    high.push(T);
    let (source, index) = universe(
        vec![low.clone(), low.clone(), low.clone(), middle, high.clone()],
        weights,
    );
    let language = Language::paths(&[(0xf, low), (0xf0, high)]);
    (source, index, language)
}

#[test]
fn branch_bound_skips_disjoint_reveals_and_preserves_duplicate_ids_and_replay() {
    let control = ExecutionControl::default();
    let (source, index, language) = branch_bound_hidden_fixture(vec![0.2; 5]);
    for collect_full in [false, true] {
        for reverse in [false, true] {
            let mut language = language.clone();
            if reverse {
                language.reverse_storage();
            }
            let results = [false, true].map(|enabled| {
                let mut evaluator = evaluator(&source, &index, 0, None, false);
                evaluator.set_branch_bound_enabled(enabled);
                evaluator.collect_full = collect_full;
                let result = evaluator.evaluate(&language, &control).unwrap();
                assert_eq!(
                    result.metrics.branch_bound,
                    evaluator.branch_bound_metrics()
                );
                result
            });
            same_collection_coverage(&results[0], &results[1]);
            assert_eq!(covered_ids(&results[1]), vec![0, 1, 2]);
            assert_eq!(results[1].covered_pattern_count, 3);
            assert_eq!(results[0].metrics.branch_bound, [0; 4]);
            assert!(results[1].metrics.branch_bound[0] > 0);
            assert!(results[1].metrics.branch_bound[1] > 0);
            assert!(results[1].metrics.branch_bound[2] >= 2);
            assert!(results[1].metrics.branch_bound[3] > 0);
            assert!(results[1].metrics.action_checks < results[0].metrics.action_checks);
        }
    }
}

#[test]
fn branch_bound_equal_count_keeps_an_earlier_raw_tie_and_first_zero_action() {
    use PieceKind::{I, J, L, O, S, T, Z};
    let control = ExecutionControl::default();
    let first = vec![I, O, T, S, Z, J, L, I];
    let second = vec![I, O, T, S, Z, J, L, O];
    let (source, index) = universe(
        vec![first.clone(), first.clone(), second.clone(), second.clone()],
        vec![0.25; 4],
    );
    for physical in [false, true] {
        let mut language = Language::paths(&[(0xf, first.clone()), (0xf0, second.clone())]);
        language.physical = physical;
        language.reverse_storage();
        let results = [false, true].map(|enabled| {
            let mut evaluator = evaluator(&source, &index, 0, None, false);
            evaluator.set_branch_bound_enabled(enabled);
            evaluator.collect_full = false;
            evaluator.evaluate(&language, &control).unwrap()
        });
        same_collection_coverage(&results[0], &results[1]);
        assert_eq!(
            covered_ids(&results[1]),
            if physical { vec![0, 1] } else { vec![2, 3] }
        );
        if physical {
            assert!(results[1].metrics.branch_bound[1] > 0);
        } else {
            // After its first failed reveal, the later action can only tie
            // two IDs. Its smaller raw child wins that tie, so it must finish.
            assert!(results[1].metrics.branch_bound[0] > 0);
            assert_eq!(results[1].metrics.branch_bound[1], 0);
        }
    }

    let mut rejected_first = first;
    *rejected_first.last_mut().unwrap() = S;
    let mut rejected_second = second;
    *rejected_second.last_mut().unwrap() = Z;
    let rejected = Language::paths(&[(0xf, rejected_first), (0xf0, rejected_second)]);
    let mut evaluator = evaluator(&source, &index, 0, None, false);
    evaluator.set_branch_bound_enabled(true);
    let (value, transition) = first_transition(&mut evaluator, &rejected, None);
    assert_eq!(value.pattern_count, 0);
    assert_eq!(
        transition.unwrap().physical_action.unwrap().cells,
        0xf,
        "the first legal exact-zero action is retained without an incumbent"
    );
    assert_eq!(evaluator.branch_bound_metrics()[1], 0);
}

#[test]
fn branch_bound_keeps_partial_actions_out_of_memos_and_resets_forks() {
    use PieceKind::T;
    let control = ExecutionControl::default();
    let (source, index, language) = branch_bound_hidden_fixture(vec![0.2; 5]);
    let mut evaluator = evaluator(&source, &index, 0, None, false);
    evaluator.set_branch_bound_enabled(true);
    evaluator.collect_full = false;
    evaluator.raw_memo_keys = true;
    let first = evaluator.evaluate(&language, &control).unwrap();
    let observations = evaluator.trie.initial_observations.clone();
    assert_eq!(observations.len(), 1);
    let mut revealed = [NO_NODE; MAX_REVEAL_BRANCHES];
    let mut count = 0;
    evaluator
        .trie
        .collect_revealed_descendants(observations[0], 8, &mut revealed, &mut count)
        .unwrap();
    assert_eq!(count, 3);
    let observation_node = revealed[..count]
        .iter()
        .copied()
        .find(|&node| evaluator.trie.piece_at(node, 7) == Some(piece_code(T)))
        .unwrap();
    let child = language.edge(language.root(), 1).unwrap().1;
    let skipped = PolicyState {
        language_node: child,
        observation_node,
        source_cursor: 1,
        hold_code: 0,
    };
    let key = evaluator.memo_state(&language, skipped).unwrap();
    assert!(!evaluator.memo.contains_key(&key));
    assert!(!evaluator.zero_scores.contains(&key));
    assert!(!evaluator.pre_reveal_memo.values.contains_key(&key));
    // The skipped branch succeeds if independently requested. A partial
    // action's losing bound must never have memoized it as a rejection.
    assert_eq!(
        evaluator
            .solve(&language, skipped, &control)
            .unwrap()
            .pattern_count,
        1
    );

    #[cfg(not(target_family = "wasm"))]
    {
        let mut fork = evaluator.fork_empty().unwrap();
        assert!(fork.branch_bound_enabled);
        assert_eq!(fork.branch_bound_metrics(), [0; 4]);
        let result = fork.evaluate(&language, &control).unwrap();
        same_collection_coverage(&first, &result);
        assert_eq!(first.metrics.branch_bound, result.metrics.branch_bound);
    }
    let again = evaluator.evaluate(&language, &control).unwrap();
    same_collection_coverage(&first, &again);
    assert_eq!(first.metrics.branch_bound, again.metrics.branch_bound);
    evaluator.set_branch_bound_enabled(false);
    let baseline = evaluator.evaluate(&language, &control).unwrap();
    same_collection_coverage(&first, &baseline);
    assert_eq!(baseline.metrics.branch_bound, [0; 4]);
}

#[test]
fn branch_bound_retains_weighted_variable_and_terminal_target_fallbacks() {
    use clearra_problem::{compile_setup_search_conditions, SetupSearchQuery};
    use PieceKind::{I, O, S, T};
    let control = ExecutionControl::default();
    for weights in [vec![0.1, 0.2, 0.3, 0.1, 0.3], vec![0.0; 5]] {
        let (source, index, language) = branch_bound_hidden_fixture(weights);
        let results = [false, true].map(|enabled| {
            let mut evaluator = evaluator(&source, &index, 0, None, false);
            assert!(evaluator.trie.uniform_weight.is_none());
            evaluator.set_branch_bound_enabled(enabled);
            evaluator.collect_full = false;
            evaluator.evaluate(&language, &control).unwrap()
        });
        same_collection_coverage(&results[0], &results[1]);
        assert_eq!(results[1].metrics.branch_bound, [0; 4]);
    }

    let (source, index, mut language) = branch_bound_hidden_fixture(vec![0.2; 5]);
    language.horizon = None;
    let results = [false, true].map(|enabled| {
        let mut evaluator = evaluator(&source, &index, 0, None, false);
        evaluator.set_branch_bound_enabled(enabled);
        evaluator.evaluate(&language, &control).unwrap()
    });
    same_collection_coverage(&results[0], &results[1]);
    assert_eq!(results[1].metrics.branch_bound, [0; 4]);

    let target = compile_setup_search_conditions(
        &SetupSearchQuery::default()
            .with_remaining_pieces(vec![I, T, S, O])
            .with_next_cycle_remaining_pieces(vec![O]),
    )
    .unwrap()
    .remove(0)
    .terminal_supply_target()
    .unwrap();
    language.horizon = Some(8);
    let results = [false, true].map(|enabled| {
        let mut evaluator = evaluator(&source, &index, 0, None, false);
        evaluator.set_branch_bound_enabled(enabled);
        evaluator.set_terminal_supply_target(Some(target));
        evaluator.evaluate(&language, &control).unwrap()
    });
    same_collection_coverage(&results[0], &results[1]);
    assert_eq!(results[1].metrics.branch_bound, [0; 4]);
}

#[test]
fn branch_bound_keeps_hold_eof_and_projected_last_draw_paths() {
    use PieceKind::{I, J, L, O, S, T, Z};
    let control = ExecutionControl::default();
    let (source, index) = universe(vec![vec![I]], vec![1.0]);
    let language = Language::paths(&[(0xf, vec![I, O])]);
    for held in [None, Some(O)] {
        for collect_full in [false, true] {
            let results = [false, true].map(|enabled| {
                let mut evaluator = evaluator(&source, &index, 0, held, true);
                evaluator.set_branch_bound_enabled(enabled);
                evaluator.collect_full = collect_full;
                evaluator.evaluate(&language, &control).unwrap()
            });
            same_collection_coverage(&results[0], &results[1]);
            assert_eq!(
                results[1].covered_pattern_count,
                usize::from(held.is_some())
            );
            assert_eq!(results[1].metrics.branch_bound, [0; 4]);
        }
    }
    let (source, index) = universe(vec![vec![I, O]], vec![1.0]);
    let language = Language::one_lock(&[(O, 0xf, 1)], true);
    for held in [None, Some(O)] {
        let transitions = [false, true].map(|enabled| {
            let mut evaluator = evaluator(&source, &index, 0, held, true);
            evaluator.set_branch_bound_enabled(enabled);
            first_transition(&mut evaluator, &language, None).1.unwrap()
        });
        assert_eq!(transitions[0], transitions[1]);
        assert_eq!(
            transitions[1].action,
            if held.is_some() {
                SupplyAction::SwapHeld
            } else {
                SupplyAction::StoreCurrentUseNext
            }
        );
    }
    let (source, _) = universe(vec![vec![I, O, T, S, Z, J]], vec![1.0]);
    let language = Language::paths(&[(0xf, vec![I, O, T, S, Z, J, L])]);
    let results = [false, true].map(|enabled| {
        let mut evaluator = QueueObservationPolicyEvaluator::for_original_universe(
            &source,
            QueueObservationPolicy::VisibleSeven,
            0,
            None,
            false,
            false,
            true,
            None,
        )
        .unwrap();
        evaluator.set_branch_bound_enabled(enabled);
        evaluator.collect_full = false;
        evaluator.evaluate(&language, &control).unwrap()
    });
    same_collection_coverage(&results[0], &results[1]);
    assert_eq!(results[1].covered_pattern_count, 1);
    assert_eq!(results[1].metrics.branch_bound, [0; 4]);
}

#[test]
fn branch_bound_checks_child_and_cancellation_before_dominated_return() {
    let (source, index, language) = branch_bound_hidden_fixture(vec![0.2; 5]);
    let mut evaluator = evaluator(&source, &index, 0, None, false);
    evaluator
        .prepare_trie(&ExecutionControl::default())
        .unwrap();
    let state = PolicyState {
        language_node: language.root(),
        observation_node: evaluator.trie.initial_observations[0],
        source_cursor: 0,
        hold_code: 0,
    };
    let transition = PolicyTransition {
        child: language.edge(language.root(), 1).unwrap().1,
        source_cursor: 1,
        hold_code: 0,
        action: SupplyAction::UseCurrent,
        physical_action: Some(PhysicalPolicyAction {
            kind: 1,
            cells: 0xf0,
        }),
    };
    let bound = Some(TransitionBound {
        incumbent_count: 5,
        wins_equal_count: false,
    });
    assert!(matches!(
        evaluator.transition_value(
            &language,
            state,
            PolicyTransition {
                child: NO_NODE,
                ..transition
            },
            bound,
            &ExecutionControl::default()
        ),
        Err(WasmExactSearchError::InvalidProblem(
            "wasm_observation_language_node_out_of_range"
        ))
    ));
    let cancelled = ExecutionControl::default();
    cancelled.cancellation.handle().cancel();
    assert!(matches!(
        evaluator.transition_value(&language, state, transition, bound, &cancelled),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert_eq!(evaluator.branch_bound_metrics()[1], 0);
    assert!(evaluator.memo.is_empty());
    assert!(evaluator.zero_scores.is_empty());
    #[cfg(not(target_family = "wasm"))]
    {
        evaluator.set_peer_abort(Arc::new(AtomicBool::new(true)));
        assert!(matches!(
            evaluator.transition_value(
                &language,
                state,
                transition,
                bound,
                &ExecutionControl::default()
            ),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert_eq!(evaluator.branch_bound_metrics()[1], 0);
    }
}

#[test]
fn physical_tie_preserves_original_ids_under_node_storage_permutation() {
    use PieceKind::{I, J, L, O, S, T, Z};
    let low = vec![I, O, T, S, Z, J, L, I];
    let high = vec![I, O, T, S, Z, J, L, O];
    for weights in [vec![0.25; 4], vec![0.125, 0.375, 0.125, 0.375]] {
        let (universe, index) = universe(
            vec![low.clone(), low.clone(), high.clone(), high.clone()],
            weights,
        );
        for reverse in [false, true] {
            let mut language = Language::paths(&[(0xf, low.clone()), (0xf0, high.clone())]);
            if reverse {
                language.reverse_storage();
            }
            for raw_memo_keys in [false, true] {
                let mut evaluator = evaluator(&universe, &index, 0, None, false);
                evaluator.raw_memo_keys = raw_memo_keys;
                let coverage = evaluator
                    .evaluate(&language, &ExecutionControl::default())
                    .expect("physical coverage");
                assert_eq!(covered_ids(&coverage), vec![0, 1]);
                assert_eq!(coverage.covered_pattern_count, 2);
                assert_eq!(coverage.covered_weight, 0.5);
                assert_eq!(coverage.min_accepted_depth, Some(8));
                assert_eq!(coverage.max_accepted_depth, Some(8));
            }

            // The new contract intentionally differs from the legacy numeric
            // child tie. Kind-only providers retain that old selection rule.
            language.physical = false;
            let coverage = evaluator(&universe, &index, 0, None, false)
                .evaluate(&language, &ExecutionControl::default())
                .expect("kind-only coverage");
            assert_eq!(
                covered_ids(&coverage),
                if reverse { vec![2, 3] } else { vec![0, 1] }
            );
            assert_eq!(coverage.covered_pattern_count, 2);
            assert_eq!(coverage.covered_weight, 0.5);
        }
    }
}

#[test]
fn physical_supply_action_order_precedes_edge_and_child_order_at_full_count_cutoff() {
    use PieceKind::{I, O};
    let (universe, index) = universe(vec![vec![I, O]], vec![1.0]);
    // O has the first edge, smaller cells and smaller child. Current I still
    // precedes both empty-hold storage and an occupied-hold swap.
    let language = Language::one_lock(&[(O, 0xf, 1), (I, 0xf0, 2), (I, 0xf00, 1)], true);
    for held in [None, Some(O)] {
        let mut evaluator = evaluator(&universe, &index, 0, held, true);
        let (value, transition) = first_transition(&mut evaluator, &language, None);
        let transition = transition.expect("current action");
        assert_eq!(value.pattern_count, 1);
        assert_eq!(transition.action, SupplyAction::UseCurrent);
        assert_eq!(transition.child, 2);
        assert_eq!(transition.source_cursor, 1);
        assert_eq!(transition.hold_code, held.map_or(0, piece_code));
        assert_eq!(
            transition.physical_action.expect("physical key").cells,
            0xf0
        );
        assert_eq!(evaluator.action_checks, 1);
    }
}

#[test]
fn physical_actions_keep_distinct_cells_when_children_coincide_including_zero_value() {
    use PieceKind::I;
    let (universe, index) = universe(vec![vec![I]], vec![1.0]);
    for accepting in [false, true] {
        let mut language = Language::one_lock(&[(I, 0xf, 1), (I, 0xf0, 1)], accepting);
        language.horizon = None;
        let mut evaluator = evaluator(&universe, &index, 0, None, false);
        let (value, transition) = first_transition(&mut evaluator, &language, None);
        let transition = transition.expect("even a zero-valued legal action is selected");
        assert_eq!(value.pattern_count, u32::from(accepting));
        assert_eq!(transition.physical_action.expect("physical key").cells, 0xf);
        assert_eq!(language.edge_count(language.root()), Some(2));
        assert_eq!(evaluator.action_checks, 2);
    }
}

#[test]
fn physical_supply_kernel_keeps_empty_occupied_and_terminal_hold_transitions() {
    use PieceKind::{I, O};
    let language = Language::one_lock(&[(O, 0xf, 1)], true);
    let (universe, index) = universe(vec![vec![I, O]], vec![1.0]);
    let mut empty = evaluator(&universe, &index, 0, None, true);
    let (_, transition) = first_transition(&mut empty, &language, None);
    let stored = transition.expect("empty-hold store");
    assert_eq!(stored.action, SupplyAction::StoreCurrentUseNext);
    assert_eq!(stored.source_cursor, 2);
    assert_eq!(stored.hold_code, piece_code(I));

    let mut occupied = evaluator(&universe, &index, 0, Some(O), true);
    let (_, transition) = first_transition(&mut occupied, &language, None);
    let swapped = transition.expect("occupied-hold swap");
    assert_eq!(swapped.action, SupplyAction::SwapHeld);
    assert_eq!(swapped.source_cursor, 1);
    assert_eq!(swapped.hold_code, piece_code(I));

    let mut terminal = evaluator(&universe, &index, 2, Some(O), true);
    let (_, transition) = first_transition(&mut terminal, &language, None);
    let released = transition.expect("terminal-held release");
    assert_eq!(released.action, SupplyAction::ReleaseHeldAtTerminal);
    assert_eq!(released.source_cursor, 2);
    assert_eq!(released.hold_code, TERMINAL_USED_HELD_CODE);

    let mut disabled_release = evaluator(&universe, &index, 2, Some(O), true);
    disabled_release.projects_unplaced_lookahead = false;
    assert!(first_transition(&mut disabled_release, &language, None)
        .1
        .is_none());
    for hold_code in [0, TERMINAL_USED_HELD_CODE] {
        let mut terminal = evaluator(&universe, &index, 2, None, true);
        let (value, transition) = first_transition(&mut terminal, &language, Some(hold_code));
        assert_eq!(value.pattern_count, 0);
        assert!(transition.is_none());
    }
}

#[test]
fn physical_empty_hold_at_last_draw_cannot_store_without_a_next_piece() {
    use PieceKind::{I, O};
    let (universe, index) = universe(vec![vec![I]], vec![1.0]);
    let language = Language::one_lock(&[(O, 0xf, 1)], true);
    let mut evaluator = evaluator(&universe, &index, 0, None, true);
    let (value, transition) = first_transition(&mut evaluator, &language, None);
    assert_eq!(value.pattern_count, 0);
    assert!(transition.is_none());
    assert_eq!(evaluator.action_checks, 0);
}

#[test]
fn an_empty_physical_node_is_not_inferred_to_be_a_kind_only_alias() {
    use PieceKind::I;
    let (universe, index) = universe(vec![vec![I]], vec![1.0]);
    let language = Language::one_lock(&[], false);
    assert!(language.has_physical_actions(language.root()));
    let coverage = evaluator(&universe, &index, 0, None, false)
        .evaluate(&language, &ExecutionControl::default())
        .expect("empty physical language");
    assert_eq!(coverage.covered_pattern_count, 0);
    assert_eq!(coverage.metrics.action_checks, 0);
    assert_eq!(QueueObservationMetrics::default().board_actions, [0; 14]);
    assert_eq!(
        QueueObservationMetrics::default().physical_representation,
        "none"
    );
}

fn collect_mode_pair(
    universe: &MaterializedPatternUniverse,
    index: &PatternPiecePositionIndex,
    language: &Language,
    terminal: Option<SetupTerminalSupplyTarget>,
) -> [QueueObservationCoverage; 2] {
    [false, true].map(|enabled| {
        let mut evaluator = evaluator(universe, index, 0, None, false);
        evaluator.collect_full = enabled;
        evaluator.set_terminal_supply_target(terminal);
        evaluator
            .evaluate(language, &ExecutionControl::default())
            .unwrap()
    })
}

fn same_collection_coverage(left: &QueueObservationCoverage, right: &QueueObservationCoverage) {
    assert_eq!(
        left.covered_patterns.words(),
        right.covered_patterns.words()
    );
    assert_eq!(left.covered_pattern_count, right.covered_pattern_count);
    assert_eq!(
        left.covered_weight.to_bits(),
        right.covered_weight.to_bits()
    );
    assert_eq!(left.min_accepted_depth, right.min_accepted_depth);
    assert_eq!(left.max_accepted_depth, right.max_accepted_depth);
}

#[test]
fn full_information_collection_preserves_weighted_duplicate_ids_without_reveal() {
    use PieceKind::{I, J, L, O, S, T, Z};
    let first = vec![I, O, T, S, Z, J, L, I];
    let second = vec![I, O, T, S, Z, J, L, O];
    let (universe, index) = universe(
        vec![first.clone(), first.clone(), second.clone(), second],
        vec![0.0, 0.2, 0.3, 0.5],
    );
    let mut language = Language::paths(&[(0xf, first)]);
    // Share the first seven physical choices. Only the last action depends
    // on the originally hidden eighth piece, after its actual reveal.
    let alternative = language.nodes.len() as u32;
    language.nodes.push(Node {
        accepting: true,
        depth: 8,
        edges: Vec::new(),
    });
    language.nodes[7]
        .edges
        .push((piece_code(O), 0xf0, alternative));
    let mut inspection = evaluator(&universe, &index, 0, None, false);
    inspection
        .prepare_trie(&ExecutionControl::default())
        .unwrap();
    assert!(inspection.trie.uniform_weight.is_none());
    assert_eq!(inspection.trie.initial_observations.len(), 1);
    assert!(!inspection
        .trie
        .future_is_determined(inspection.trie.initial_observations[0])
        .unwrap());
    let [baseline, optimized] = collect_mode_pair(&universe, &index, &language, None);
    same_collection_coverage(&baseline, &optimized);
    assert_eq!(optimized.covered_pattern_count, 4);
    assert_eq!(optimized.covered_patterns.words()[0], 0b1111);
    assert_eq!(optimized.min_accepted_depth, Some(8));
    assert_eq!(optimized.max_accepted_depth, Some(8));
    assert_eq!(optimized.metrics.collection, [1, 1]);
    assert!(baseline.metrics.collection[0] > optimized.metrics.collection[0]);
    assert_eq!(baseline.metrics.collection[1], 0);
}

#[test]
fn full_information_collection_uses_solved_terminal_target_not_intermediate_supply() {
    use clearra_problem::{compile_setup_search_conditions, SetupSearchQuery};
    use PieceKind::{I, O, S, T};
    let target = compile_setup_search_conditions(
        &SetupSearchQuery::default()
            .with_remaining_pieces(vec![I, T, S, O])
            .with_next_cycle_remaining_pieces(vec![O]),
    )
    .unwrap()
    .remove(0)
    .terminal_supply_target()
    .unwrap();
    let (universe, index) = universe(vec![vec![I, T, S, O], vec![I, T, S, O]], vec![0.25, 0.75]);
    let language = Language::paths(&[(0xf, vec![I, T, S])]);
    let inspection = evaluator(&universe, &index, 0, None, false);
    let observation = inspection.trie.initial_observations[0];
    assert!(!inspection
        .trie
        .terminal_supply_target_accepts(observation, 0, 0, target));
    assert!(inspection
        .trie
        .terminal_supply_target_accepts(observation, 3, 0, target));
    let [baseline, optimized] = collect_mode_pair(&universe, &index, &language, Some(target));
    same_collection_coverage(&baseline, &optimized);
    assert_eq!(optimized.covered_pattern_count, 2);
    assert_eq!(optimized.metrics.collection, [1, 1]);
    assert_eq!(optimized.min_accepted_depth, Some(3));
}

#[test]
fn full_information_collection_retains_partial_info_sets_and_variable_depths() {
    use PieceKind::{I, J, L, O, S, T, Z};
    let first = vec![I, O, T, S, Z, J, L, I];
    let second = vec![I, O, T, S, Z, J, L, O];
    let (partial, index) = universe(
        vec![first.clone(), first.clone(), second.clone(), second],
        vec![0.25; 4],
    );
    let language = Language::paths(&[(0xf, first)]);
    let [baseline, optimized] = collect_mode_pair(&partial, &index, &language, None);
    same_collection_coverage(&baseline, &optimized);
    assert_eq!(optimized.covered_pattern_count, 2);
    assert_eq!(optimized.covered_patterns.words()[0], 0b11);
    assert!(
        optimized.metrics.collection[0] > 1,
        "a partial initial information set must be replayed"
    );

    let (variable, index) = universe(vec![vec![I, O], vec![O, I]], vec![0.4, 0.6]);
    let mut language = Language::paths(&[(0xf, vec![I]), (0xf0, vec![O, I])]);
    language.horizon = None;
    let [baseline, optimized] = collect_mode_pair(&variable, &index, &language, None);
    same_collection_coverage(&baseline, &optimized);
    assert_eq!(optimized.covered_pattern_count, 2);
    assert_eq!(optimized.min_accepted_depth, Some(1));
    assert_eq!(optimized.max_accepted_depth, Some(2));
    assert_eq!(optimized.metrics.collection[1], 0);
    assert_eq!(
        optimized.metrics.collection[0],
        baseline.metrics.collection[0]
    );
}

struct RevealedSupplyProbe {
    language: Language,
    enabled: bool,
    supplied: std::cell::RefCell<Vec<[u8; 7]>>,
    unfiltered: std::cell::Cell<usize>,
}

impl RevealedSupplyProbe {
    fn new(horizon: usize) -> Self {
        Self {
            language: Language::paths(&[(0xf, vec![PieceKind::I; horizon])]),
            enabled: true,
            supplied: std::cell::RefCell::new(Vec::new()),
            unfiltered: std::cell::Cell::new(0),
        }
    }
}

impl ObservationPieceLanguage for RevealedSupplyProbe {
    fn root(&self) -> u32 {
        self.language.root()
    }
    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        self.language.node(node)
    }
    fn edge_count(&self, node: u32) -> Option<usize> {
        self.language.edge_count(node)
    }
    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        self.language.edge(node, index)
    }
    fn fixed_horizon(&self) -> Option<u8> {
        self.language.fixed_horizon()
    }
    fn wants_revealed_supply(&self, node: u32) -> bool {
        self.enabled && self.node(node).is_some_and(|node| !node.accepting)
    }
    fn fully_revealed_node(
        &self,
        node: u32,
        _: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        self.unfiltered.set(self.unfiltered.get() + 1);
        Ok(node)
    }
    fn fully_revealed_node_with_supply(
        &self,
        node: u32,
        available: [u8; 7],
        _: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        self.supplied.borrow_mut().push(available);
        Ok(node)
    }
}

fn initial_probe_state(evaluator: &QueueObservationPolicyEvaluator) -> PolicyState {
    PolicyState {
        language_node: 0,
        observation_node: evaluator.trie.initial_observations[0],
        source_cursor: evaluator.initial_cursor as u16,
        hold_code: evaluator.initial_hold_code,
    }
}

#[test]
fn supply_multiset_requires_actual_draws_and_respects_provider_interest() {
    use PieceKind::{I, J, L, O, S, T, Z};
    let control = ExecutionControl::default();
    let (source, _) = universe(vec![vec![I, O, T, S, Z, J, L, I]], vec![1.0]);
    let mut empty_hold = QueueObservationPolicyEvaluator::for_original_universe(
        &source,
        QueueObservationPolicy::VisibleSeven,
        0,
        None,
        true,
        false,
        false,
        None,
    )
    .unwrap();
    empty_hold.prepare_trie(&control).unwrap();
    let state = initial_probe_state(&empty_hold);
    assert!(empty_hold
        .trie
        .future_is_determined(state.observation_node)
        .unwrap());
    assert_eq!(
        empty_hold.trie.nodes[state.observation_node as usize].depth,
        7
    );
    let probe = RevealedSupplyProbe::new(7);
    empty_hold
        .observation_language_state(&probe, state, &control)
        .unwrap();
    assert!(
        probe.supplied.borrow().is_empty(),
        "the possible empty-hold draw remains hidden"
    );
    assert_eq!(
        probe.unfiltered.get(),
        1,
        "future-only reveal uses the established path"
    );

    let (ambiguous_source, _) = universe(
        vec![vec![I, O, T, S, Z, J, L, I], vec![I, O, T, S, Z, J, L, O]],
        vec![0.5, 0.5],
    );
    let mut ambiguous = QueueObservationPolicyEvaluator::for_original_universe(
        &ambiguous_source,
        QueueObservationPolicy::VisibleSeven,
        0,
        None,
        true,
        false,
        false,
        None,
    )
    .unwrap();
    ambiguous.prepare_trie(&control).unwrap();
    let probe = RevealedSupplyProbe::new(7);
    ambiguous
        .observation_language_state(&probe, initial_probe_state(&ambiguous), &control)
        .unwrap();
    assert!(probe.supplied.borrow().is_empty());
    assert_eq!(
        probe.unfiltered.get(),
        0,
        "no reveal certificate exists for the hidden choice"
    );

    let mut occupied = QueueObservationPolicyEvaluator::for_original_universe(
        &source,
        QueueObservationPolicy::VisibleSeven,
        0,
        Some(T),
        true,
        false,
        false,
        None,
    )
    .unwrap();
    occupied.prepare_trie(&control).unwrap();
    let probe = RevealedSupplyProbe::new(7);
    occupied
        .observation_language_state(&probe, initial_probe_state(&occupied), &control)
        .unwrap();
    assert_eq!(*probe.supplied.borrow(), vec![[1, 1, 2, 1, 1, 1, 1]]);
    assert_eq!(probe.unfiltered.get(), 0);
    let mut ignored = RevealedSupplyProbe::new(7);
    ignored.enabled = false;
    occupied
        .observation_language_state(&ignored, initial_probe_state(&occupied), &control)
        .unwrap();
    assert!(ignored.supplied.borrow().is_empty());
    assert_eq!(ignored.unfiltered.get(), 1);
}

#[test]
fn supply_multiset_includes_projected_draws_and_only_usable_held_piece() {
    use PieceKind::{I, J, O, S, T, Z};
    let control = ExecutionControl::default();
    let (source, _) = universe(vec![vec![I, O, T, S, Z, J]], vec![1.0]);
    let projected = QueueObservationPolicyEvaluator::for_original_universe(
        &source,
        QueueObservationPolicy::VisibleSeven,
        0,
        None,
        false,
        false,
        true,
        None,
    )
    .unwrap();
    assert_eq!(projected.trie.materialized_sequence_len, 6);
    assert_eq!(projected.trie.sequence_len, 7);
    let probe = RevealedSupplyProbe::new(7);
    projected
        .observation_language_state(&probe, initial_probe_state(&projected), &control)
        .unwrap();
    assert_eq!(
        *probe.supplied.borrow(),
        vec![[1; 7]],
        "the projected L is an observed usable draw"
    );

    let (source, _) = universe(vec![vec![I]], vec![1.0]);
    let held = QueueObservationPolicyEvaluator::for_original_universe(
        &source,
        QueueObservationPolicy::VisibleSeven,
        0,
        Some(O),
        true,
        true,
        false,
        None,
    )
    .unwrap();
    let probe = RevealedSupplyProbe::new(2);
    held.observation_language_state(&probe, initial_probe_state(&held), &control)
        .unwrap();
    let eof = PolicyState {
        language_node: 1,
        source_cursor: 1,
        ..initial_probe_state(&held)
    };
    held.observation_language_state(&probe, eof, &control)
        .unwrap();
    held.observation_language_state(
        &probe,
        PolicyState {
            hold_code: TERMINAL_USED_HELD_CODE,
            ..eof
        },
        &control,
    )
    .unwrap();
    assert_eq!(
        *probe.supplied.borrow(),
        vec![[1, 1, 0, 0, 0, 0, 0], [0, 1, 0, 0, 0, 0, 0], [0; 7]]
    );

    let disabled = QueueObservationPolicyEvaluator::for_original_universe(
        &source,
        QueueObservationPolicy::VisibleSeven,
        0,
        Some(O),
        false,
        true,
        false,
        None,
    )
    .unwrap();
    let probe = RevealedSupplyProbe::new(2);
    disabled
        .observation_language_state(&probe, initial_probe_state(&disabled), &control)
        .unwrap();
    assert_eq!(*probe.supplied.borrow(), vec![[1, 0, 0, 0, 0, 0, 0]]);
}

/// Raw and normalized references have disjoint namespaces, as in the board
/// view. Count normalization calls to verify real reuse, rather than merely
/// reducing a work counter while repeating the computation.
struct PreRevealProbe {
    language: Language,
    interested: bool,
    reusable: bool,
    normalizations: std::cell::Cell<usize>,
    supplied: std::cell::RefCell<Vec<[u8; 7]>>,
    fail_reveal: std::cell::Cell<bool>,
}

impl PreRevealProbe {
    fn new(language: Language) -> Self {
        Self {
            language,
            interested: true,
            reusable: false,
            normalizations: std::cell::Cell::new(0),
            supplied: std::cell::RefCell::new(Vec::new()),
            fail_reveal: std::cell::Cell::new(false),
        }
    }

    fn offset(&self) -> u32 {
        self.language.nodes.len() as u32
    }

    fn raw(&self, node: u32) -> Option<u32> {
        (node < self.offset() * 2).then(|| node % self.offset())
    }

    fn normalize(&self, node: u32) -> Result<u32, WasmExactSearchError> {
        if node >= self.offset() {
            return Ok(node);
        }
        self.normalizations.set(self.normalizations.get() + 1);
        if self.fail_reveal.get() {
            return Err(WasmExactSearchError::InvalidProblem(
                "pre_reveal_probe_failure",
            ));
        }
        Ok(node + self.offset())
    }
}

impl ObservationPieceLanguage for PreRevealProbe {
    fn root(&self) -> u32 {
        self.language.root()
    }

    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        self.language.node(self.raw(node)?)
    }

    fn edge_count(&self, node: u32) -> Option<usize> {
        self.language.edge_count(self.raw(node)?)
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        let (piece, child) = self.language.edge(self.raw(node)?, index)?;
        Some((
            piece,
            child
                + if node >= self.offset() {
                    self.offset()
                } else {
                    0
                },
        ))
    }

    fn has_physical_actions(&self, node: u32) -> bool {
        node < self.offset() && self.language.has_physical_actions(node)
    }

    fn physical_action_cells(&self, node: u32, index: usize) -> Option<u64> {
        self.has_physical_actions(node)
            .then(|| self.language.physical_action_cells(node, index))
            .flatten()
    }

    fn fixed_horizon(&self) -> Option<u8> {
        self.language.fixed_horizon()
    }

    fn wants_revealed_supply(&self, node: u32) -> bool {
        self.interested
            && node < self.offset()
            && self.node(node).is_some_and(|node| !node.accepting)
    }

    fn fully_revealed_node(
        &self,
        node: u32,
        _: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        self.normalize(node)
    }

    fn fully_revealed_node_with_supply(
        &self,
        node: u32,
        available: [u8; 7],
        _: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        self.supplied.borrow_mut().push(available);
        self.normalize(node)
    }

    fn memo_reusable(&self, _: u32) -> bool {
        self.reusable
    }
}

fn same_policy_value(left: PolicyValue, right: PolicyValue) {
    assert_eq!(left.score.to_bits(), right.score.to_bits());
    assert_eq!(left.pattern_count, right.pattern_count);
}

#[test]
fn pre_reveal_memo_reuses_completed_value_without_repeating_normalization() {
    use PieceKind::{I, O};
    let (source, index) = universe(vec![vec![I, O], vec![I, O]], vec![0.25, 0.75]);
    let probe = PreRevealProbe::new(Language::paths(&[(0xf, vec![I, O])]));
    let control = ExecutionControl::default();
    let mut evaluator = evaluator(&source, &index, 0, None, false);
    evaluator.pre_reveal_enabled = true;
    evaluator.prepare_trie(&control).unwrap();
    let state = initial_probe_state(&evaluator);
    let first = evaluator.solve(&probe, state, &control).unwrap();
    assert_eq!(first.pattern_count, 2);
    assert_eq!(first.score, 1.0);
    let normalized_key = evaluator
        .memo_state(
            &probe,
            PolicyState {
                language_node: probe.offset(),
                ..state
            },
        )
        .unwrap();
    same_policy_value(evaluator.memo[&normalized_key], first);
    let normalizations = probe.normalizations.get();
    let supplied = probe.supplied.borrow().len();
    let canonical_values = evaluator.memo.len();
    let second = evaluator.solve(&probe, state, &control).unwrap();
    same_policy_value(first, second);
    assert_eq!(probe.normalizations.get(), normalizations);
    assert_eq!(probe.supplied.borrow().len(), supplied);
    assert_eq!(evaluator.memo.len(), canonical_values);
    assert_eq!(evaluator.pre_reveal_memo.metrics, [2, 1, 1, 0]);

    evaluator.pre_reveal_memo.clear_values();
    let third = evaluator.solve(&probe, state, &control).unwrap();
    same_policy_value(first, third);
    assert_eq!(probe.normalizations.get(), normalizations + 1);
    assert_eq!(evaluator.memo.len(), canonical_values);
    assert_eq!(evaluator.pre_reveal_memo.metrics, [3, 1, 2, 0]);
}

#[test]
fn pre_reveal_memo_preserves_weighted_hidden_ids_and_partial_policy_replay() {
    use PieceKind::{I, J, L, O, S, T, Z};
    let first = vec![I, O, T, S, Z, J, L, I];
    let second = vec![I, O, T, S, Z, J, L, O];
    let (source, index) = universe(
        vec![first.clone(), first.clone(), second.clone(), second.clone()],
        vec![0.0, 0.2, 0.3, 0.5],
    );
    // Different first physical actions must still be selected before the
    // eighth draw is observed. A known-source normalization cannot move back
    // across this hidden choice merely because the new value cache is on.
    let language = Language::paths(&[(0xf, first), (0xf0, second)]);
    for collect_full in [false, true] {
        let results = [false, true].map(|enabled| {
            let probe = PreRevealProbe::new(language.clone());
            let mut evaluator = evaluator(&source, &index, 0, None, false);
            evaluator.pre_reveal_enabled = enabled;
            evaluator.collect_full = collect_full;
            let result = evaluator
                .evaluate(&probe, &ExecutionControl::default())
                .unwrap();
            if enabled {
                assert!(result.metrics.pre_reveal[0] > 0);
                assert!(result.metrics.pre_reveal_retained_bytes > 0);
                assert!(
                    result.metrics.pre_reveal_peak_bytes
                        >= result.metrics.pre_reveal_retained_bytes
                );
            } else {
                assert_eq!(result.metrics.pre_reveal, [0; 4]);
                assert_eq!(result.metrics.pre_reveal_retained_bytes, 0);
            }
            result
        });
        same_collection_coverage(&results[0], &results[1]);
        assert_eq!(covered_ids(&results[1]), vec![2, 3]);
        assert_eq!(results[1].covered_weight, 0.8);
        assert!(results[1].metrics.collection[0] > 1);
    }
}

#[test]
fn pre_reveal_memo_keeps_hold_eof_projected_draws_and_terminal_targets() {
    use PieceKind::{I, J, L, O, S, T, Z};
    let control = ExecutionControl::default();
    let (source, index) = universe(vec![vec![I]], vec![1.0]);
    for held in [None, Some(O)] {
        let results = [false, true].map(|enabled| {
            let probe = PreRevealProbe::new(Language::paths(&[(0xf, vec![I, O])]));
            let mut evaluator = evaluator(&source, &index, 0, held, true);
            evaluator.pre_reveal_enabled = enabled;
            evaluator.evaluate(&probe, &control).unwrap()
        });
        same_collection_coverage(&results[0], &results[1]);
        assert_eq!(
            results[1].covered_pattern_count,
            usize::from(held.is_some())
        );
    }

    let (source, _) = universe(vec![vec![I, O, T, S, Z, J]], vec![1.0]);
    let results = [false, true].map(|enabled| {
        let probe = PreRevealProbe::new(Language::paths(&[(0xf, vec![I, O, T, S, Z, J, L])]));
        let mut evaluator = QueueObservationPolicyEvaluator::for_original_universe(
            &source,
            QueueObservationPolicy::VisibleSeven,
            0,
            None,
            false,
            false,
            true,
            None,
        )
        .unwrap();
        evaluator.pre_reveal_enabled = enabled;
        let result = evaluator.evaluate(&probe, &control).unwrap();
        assert_eq!(probe.supplied.borrow()[0], [1; 7]);
        result
    });
    same_collection_coverage(&results[0], &results[1]);
    assert_eq!(results[1].covered_pattern_count, 1);

    use clearra_problem::{compile_setup_search_conditions, SetupSearchQuery};
    let target = compile_setup_search_conditions(
        &SetupSearchQuery::default()
            .with_remaining_pieces(vec![I, T, S, O])
            .with_next_cycle_remaining_pieces(vec![O]),
    )
    .unwrap()
    .remove(0)
    .terminal_supply_target()
    .unwrap();
    let (source, index) = universe(vec![vec![I, T, S, O]; 2], vec![0.25, 0.75]);
    let results = [false, true].map(|enabled| {
        let probe = PreRevealProbe::new(Language::paths(&[(0xf, vec![I, T, S])]));
        let mut evaluator = evaluator(&source, &index, 0, None, false);
        evaluator.pre_reveal_enabled = enabled;
        evaluator.set_terminal_supply_target(Some(target));
        let result = evaluator.evaluate(&probe, &control).unwrap();
        assert!(probe.supplied.borrow().is_empty());
        result
    });
    same_collection_coverage(&results[0], &results[1]);
    assert_eq!(results[1].covered_pattern_count, 2);

    let (source, index) = universe(vec![vec![I, O], vec![O, I]], vec![0.4, 0.6]);
    let mut language = Language::paths(&[(0xf, vec![I]), (0xf0, vec![O, I])]);
    language.horizon = None;
    let results = [false, true].map(|enabled| {
        let probe = PreRevealProbe::new(language.clone());
        let mut evaluator = evaluator(&source, &index, 0, None, false);
        evaluator.pre_reveal_enabled = enabled;
        let result = evaluator.evaluate(&probe, &control).unwrap();
        assert!(probe.supplied.borrow().is_empty());
        result
    });
    same_collection_coverage(&results[0], &results[1]);
    assert_eq!(results[1].min_accepted_depth, Some(1));
    assert_eq!(results[1].max_accepted_depth, Some(2));
}

#[test]
fn pre_reveal_memo_is_bounded_by_initial_observation_and_evaluation_scope() {
    use PieceKind::{I, O, T};
    let control = ExecutionControl::default();
    let (source, index) = universe(vec![vec![I, O], vec![T, O]], vec![0.5; 2]);
    let probe = PreRevealProbe::new(Language::paths(&[(0xf, vec![O])]));
    let mut evaluator = evaluator(&source, &index, 1, None, false);
    evaluator.pre_reveal_enabled = true;
    evaluator.prepare_trie(&control).unwrap();
    assert_eq!(evaluator.trie.initial_observations.len(), 2);
    let keys: Vec<_> = evaluator
        .trie
        .initial_observations
        .iter()
        .map(|&observation_node| {
            evaluator
                .memo_state(
                    &probe,
                    PolicyState {
                        observation_node,
                        ..initial_probe_state(&evaluator)
                    },
                )
                .unwrap()
        })
        .collect();
    assert_eq!(
        keys[0], keys[1],
        "the existing future quotient remains exact"
    );
    let result = evaluator.evaluate(&probe, &control).unwrap();
    assert_eq!(covered_ids(&result), vec![0, 1]);
    assert_eq!(result.metrics.pre_reveal, [2, 0, 2, 0]);
    assert!(evaluator.pre_reveal_memo.values.is_empty());

    // The same numeric node in another evaluation has no raw-cache authority.
    let rejecting = PreRevealProbe::new(Language::paths(&[(0xf, vec![I])]));
    let rejected = evaluator.evaluate(&rejecting, &control).unwrap();
    assert_eq!(rejected.covered_pattern_count, 0);
    assert_eq!(rejected.metrics.pre_reveal, [2, 0, 2, 0]);
    assert_eq!(
        rejected.metrics.pre_reveal_retained_bytes,
        result.metrics.pre_reveal_retained_bytes
    );

    #[cfg(not(target_family = "wasm"))]
    {
        let fork = evaluator.fork_empty().unwrap();
        assert!(fork.pre_reveal_enabled);
        assert!(fork.pre_reveal_memo.values.is_empty());
        assert_eq!(fork.pre_reveal_memo.metrics, [0; 4]);
        assert_eq!(fork.pre_reveal_memo.retained_bytes(), 0);
    }
}

#[test]
fn pre_reveal_memo_skips_uninterested_reusable_and_normalized_nodes() {
    use PieceKind::I;
    let (source, index) = universe(vec![vec![I]], vec![1.0]);
    let control = ExecutionControl::default();
    for (interested, reusable) in [(false, false), (true, true)] {
        let mut probe = PreRevealProbe::new(Language::paths(&[(0xf, vec![I])]));
        probe.interested = interested;
        probe.reusable = reusable;
        let mut evaluator = evaluator(&source, &index, 0, None, false);
        evaluator.pre_reveal_enabled = true;
        let result = evaluator.evaluate(&probe, &control).unwrap();
        assert_eq!(result.covered_pattern_count, 1);
        assert_eq!(result.metrics.pre_reveal, [0; 4]);
        assert_eq!(result.metrics.pre_reveal_retained_bytes, 0);
    }
    let probe = PreRevealProbe::new(Language::paths(&[(0xf, vec![I])]));
    let mut evaluator = evaluator(&source, &index, 0, None, false);
    evaluator.pre_reveal_enabled = true;
    evaluator.prepare_trie(&control).unwrap();
    let state = PolicyState {
        language_node: probe.offset(),
        ..initial_probe_state(&evaluator)
    };
    assert_eq!(
        evaluator
            .solve(&probe, state, &control)
            .unwrap()
            .pattern_count,
        1
    );
    assert_eq!(evaluator.pre_reveal_memo.metrics, [0; 4]);
}

#[test]
fn pre_reveal_memo_failure_and_cancel_never_become_cached_rejection() {
    use PieceKind::I;
    let (source, index) = universe(vec![vec![I]], vec![1.0]);
    let control = ExecutionControl::default();
    let probe = PreRevealProbe::new(Language::paths(&[(0xf, vec![I])]));
    probe.fail_reveal.set(true);
    let mut evaluator = evaluator(&source, &index, 0, None, false);
    evaluator.pre_reveal_enabled = true;
    evaluator.prepare_trie(&control).unwrap();
    let state = initial_probe_state(&evaluator);
    assert!(matches!(
        evaluator.solve(&probe, state, &control),
        Err(WasmExactSearchError::InvalidProblem(
            "pre_reveal_probe_failure"
        ))
    ));
    assert!(evaluator.pre_reveal_memo.values.is_empty());
    assert_eq!(evaluator.pre_reveal_memo.metrics, [1, 0, 0, 0]);
    probe.fail_reveal.set(false);
    assert_eq!(
        evaluator
            .solve(&probe, state, &control)
            .unwrap()
            .pattern_count,
        1
    );
    let cancelled = ExecutionControl::default();
    cancelled.cancellation.handle().cancel();
    assert!(matches!(
        evaluator.solve(&probe, state, &cancelled),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert_eq!(evaluator.pre_reveal_memo.values.len(), 1);
    assert_eq!(evaluator.pre_reveal_memo.metrics[2], 1);
}

#[test]
fn pre_reveal_memo_full_admission_preserves_values_and_growth_peak() {
    let mut cache = PreRevealMemo::default();
    for index in 0..MAX_PRE_REVEAL_VALUES {
        cache
            .insert(
                PolicyMemoState(index as u128),
                PolicyValue {
                    score: 0.0,
                    // Zero weight with a positive accepted count must survive.
                    pattern_count: 1,
                },
            )
            .unwrap();
    }
    assert_eq!(cache.values.len(), MAX_PRE_REVEAL_VALUES);
    let retained = cache.retained_bytes();
    let peak = cache.peak_bytes;
    assert!(retained > MAX_PRE_REVEAL_VALUES * core::mem::size_of::<PolicyValue>());
    assert!(peak >= retained);
    cache
        .insert(PolicyMemoState(u128::MAX), PolicyValue::REJECT)
        .unwrap();
    assert_eq!(cache.values.len(), MAX_PRE_REVEAL_VALUES);
    assert_eq!(cache.retained_bytes(), retained);
    assert_eq!(cache.peak_bytes, peak);
    assert!(cache.get(&PolicyMemoState(u128::MAX)).is_none());
    assert_eq!(cache.get(&PolicyMemoState(0)).unwrap().pattern_count, 1);
    assert_eq!(cache.metrics, [2, 1, MAX_PRE_REVEAL_VALUES, 1]);
    cache.clear_values();
    assert_eq!(cache.retained_bytes(), retained);
    assert_eq!(cache.peak_bytes, peak);
    cache
        .insert(PolicyMemoState(0), PolicyValue::REJECT)
        .unwrap();
    assert_eq!(cache.get(&PolicyMemoState(0)).unwrap().pattern_count, 0);
    cache.begin_evaluation();
    assert!(cache.values.is_empty());
    assert_eq!(cache.metrics, [0; 4]);
    assert_eq!(cache.peak_bytes, retained);
}

#[cfg(feature = "placement-dependency-pc")]
mod revealed_word_membership_tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Clone, Copy)]
    enum Reply {
        Exact,
        Fallback,
        Failure,
        Cancel,
    }

    /// The reference is the existing policy evaluator on a single, fully
    /// observed queue. This tests the new consumer boundary independently of
    /// bitmap ranks, union construction, or a second handwritten hold kernel.
    struct Probe {
        language: Language,
        cut_depth: u8,
        interested: bool,
        reply: Reply,
        queries: RefCell<Vec<(u32, KnownSupplyQuery)>>,
        normalizations: Cell<usize>,
    }

    impl Probe {
        fn new(language: Language, cut_depth: u8) -> Self {
            Self {
                language,
                cut_depth,
                interested: true,
                reply: Reply::Exact,
                queries: RefCell::new(Vec::new()),
                normalizations: Cell::new(0),
            }
        }
    }

    impl ObservationPieceLanguage for Probe {
        fn root(&self) -> u32 {
            self.language.root()
        }
        fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
            self.language.node(node)
        }
        fn edge_count(&self, node: u32) -> Option<usize> {
            self.language.edge_count(node)
        }
        fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
            self.language.edge(node, index)
        }
        fn has_physical_actions(&self, node: u32) -> bool {
            self.language.has_physical_actions(node)
        }
        fn physical_action_cells(&self, node: u32, index: usize) -> Option<u64> {
            self.language.physical_action_cells(node, index)
        }
        fn fixed_horizon(&self) -> Option<u8> {
            self.language.fixed_horizon()
        }
        fn fully_revealed_node(
            &self,
            node: u32,
            _: &ExecutionControl,
        ) -> Result<u32, WasmExactSearchError> {
            self.normalizations.set(self.normalizations.get() + 1);
            Ok(node)
        }
        fn supports_revealed_word_membership(&self, node: u32) -> bool {
            self.interested
                && self
                    .node(node)
                    .is_some_and(|node| !node.accepting && node.depth == self.cut_depth)
        }
        fn revealed_word_membership(
            &self,
            node: u32,
            query: &KnownSupplyQuery,
            control: &ExecutionControl,
        ) -> Result<Option<bool>, WasmExactSearchError> {
            self.queries.borrow_mut().push((node, *query));
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            match self.reply {
                Reply::Fallback => return Ok(None),
                Reply::Failure => {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "word_membership_probe_failure",
                    ))
                }
                Reply::Exact | Reply::Cancel => {}
            }
            let accepts = if query.len == 0 {
                // This provider only advertises nonterminal nodes, and both
                // projected-lookahead/held-only release contracts are absent.
                false
            } else {
                let sequence = query.queue[..usize::from(query.len)]
                    .iter()
                    .copied()
                    .map(piece_from_code)
                    .collect();
                let (known, _) = universe(vec![sequence], vec![1.0]);
                let mut suffix = self.language.clone();
                suffix.root = node;
                let mut reference = strict(
                    &known,
                    0,
                    (query.hold_code != 0).then(|| piece_from_code(query.hold_code)),
                    query.hold_enabled,
                );
                reference.evaluate(&suffix, control)?.covered_pattern_count == 1
            };
            if matches!(self.reply, Reply::Cancel) {
                control.cancellation.handle().cancel();
            }
            Ok(Some(accepts))
        }
    }

    fn strict(
        source: &MaterializedPatternUniverse,
        cursor: u16,
        held: Option<PieceKind>,
        hold_enabled: bool,
    ) -> QueueObservationPolicyEvaluator {
        QueueObservationPolicyEvaluator::for_original_universe(
            source,
            QueueObservationPolicy::VisibleSeven,
            cursor,
            held,
            hold_enabled,
            false,
            false,
            None,
        )
        .unwrap()
    }

    fn state(
        evaluator: &QueueObservationPolicyEvaluator,
        language: &impl ObservationPieceLanguage,
    ) -> PolicyState {
        PolicyState {
            language_node: language.root(),
            ..initial_probe_state(evaluator)
        }
    }

    #[test]
    fn revealed_word_membership_reads_only_actual_relevant_draws() {
        use PieceKind::{I, J, L, O, S, T, Z};
        let control = ExecutionControl::default();
        let first = vec![I, O, T, S, Z, J, L, I];
        for source in [
            universe(vec![first.clone()], vec![1.0]).0,
            universe(
                vec![first.clone(), vec![I, O, T, S, Z, J, L, O]],
                vec![0.5; 2],
            )
            .0,
        ] {
            let mut evaluator = strict(&source, 0, None, true);
            evaluator.prepare_trie(&control).unwrap();
            let probe = Probe::new(Language::paths(&[(0xf, first[..7].to_vec())]), 0);
            let state = state(&evaluator, &probe);
            assert_eq!(
                evaluator.trie.nodes[state.observation_node as usize].depth,
                7
            );
            let determined = evaluator
                .trie
                .future_is_determined(state.observation_node)
                .unwrap();
            assert!(evaluator
                .revealed_word_value(&probe, state, &control)
                .unwrap()
                .is_none());
            assert!(
                probe.queries.borrow().is_empty(),
                "an optional empty-hold draw is still hidden"
            );
            evaluator
                .observation_language_state(&probe, state, &control)
                .unwrap();
            assert_eq!(probe.normalizations.get(), usize::from(determined));

            let probe = Probe::new(Language::paths(&[(0xf, first[..6].to_vec())]), 0);
            assert!(evaluator
                .revealed_word_value(&probe, state, &control)
                .unwrap()
                .is_some());
            let query = probe.queries.borrow()[0].1;
            assert_eq!(query.len, 7);
            assert_eq!(query.queue, [1, 2, 3, 4, 5, 6, 7]);
            assert_eq!(query.hold_code, 0);
            assert!(query.hold_enabled);
        }
        let (source, _) = universe(vec![first], vec![1.0]);
        let mut evaluator = strict(&source, 0, None, false);
        evaluator.prepare_trie(&control).unwrap();
        let mut uninterested = Probe::new(Language::paths(&[(0xf, vec![I])]), 0);
        uninterested.interested = false;
        assert!(evaluator
            .revealed_word_value(&uninterested, state(&evaluator, &uninterested), &control)
            .unwrap()
            .is_none());
        assert!(uninterested.queries.borrow().is_empty());
    }

    #[test]
    fn revealed_word_membership_preserves_hidden_physical_ties_and_duplicate_ids() {
        use PieceKind::{I, J, L, O, S, T, Z};
        let control = ExecutionControl::default();
        let prefix = vec![I, O, T, S, Z, J, L];
        let mut low = prefix.clone();
        low.push(I);
        let mut middle = prefix.clone();
        middle.push(O);
        let mut high = prefix;
        high.push(T);
        let (source, _) = universe(
            vec![low.clone(), low.clone(), middle, high.clone(), high.clone()],
            vec![0.2; 5],
        );
        for reverse in [false, true] {
            let mut language = Language::paths(&[(0xf, low.clone()), (0xf0, high.clone())]);
            if reverse {
                language.reverse_storage();
            }
            let results = [false, true].map(|enabled| {
                let mut probe = Probe::new(language.clone(), 2);
                probe.interested = enabled;
                let mut evaluator = strict(&source, 0, None, false);
                evaluator.collect_full = false;
                evaluator.set_branch_bound_enabled(false);
                let coverage = evaluator.evaluate(&probe, &control).unwrap();
                let initial = state(&evaluator, &probe);
                let (_, transition) = evaluator
                    .best_transition(&probe, initial, &control, false)
                    .unwrap();
                assert_eq!(transition.unwrap().physical_action.unwrap().cells, 0xf);
                assert_eq!(covered_ids(&coverage), vec![0, 1]);
                assert_eq!(coverage.min_accepted_depth, Some(8));
                assert_eq!(coverage.max_accepted_depth, Some(8));
                assert_eq!(!probe.queries.borrow().is_empty(), enabled);
                for &(node, query) in probe.queries.borrow().iter() {
                    assert_eq!(probe.node(node).unwrap().depth, 2);
                    assert_eq!(query.len, 6);
                }
                coverage
            });
            same_collection_coverage(&results[0], &results[1]);
            assert!(results[1].metrics.collection[0] < results[0].metrics.collection[0]);
        }
    }

    #[test]
    fn revealed_word_membership_keeps_weight_target_projection_and_variable_fallbacks() {
        use clearra_problem::{compile_setup_search_conditions, SetupSearchQuery};
        use PieceKind::{I, J, L, O, S, T, Z};
        let control = ExecutionControl::default();
        for weights in [vec![0.25, 0.75], vec![0.0; 2]] {
            let (source, _) = universe(vec![vec![I, O]; 2], weights);
            let probe = Probe::new(Language::paths(&[(0xf, vec![I, O])]), 0);
            let mut evaluator = strict(&source, 0, None, false);
            assert!(evaluator.trie.uniform_weight.is_none());
            let optimized = evaluator.evaluate(&probe, &control).unwrap();
            let baseline = strict(&source, 0, None, false)
                .evaluate(&probe.language, &control)
                .unwrap();
            same_collection_coverage(&baseline, &optimized);
            assert!(probe.queries.borrow().is_empty());
        }
        let (source, _) = universe(vec![vec![I, O]; 2], vec![0.5; 2]);
        let mut language = Language::paths(&[(0xf, vec![I]), (0xf0, vec![I, O])]);
        language.horizon = None;
        let probe = Probe::new(language, 0);
        let optimized = strict(&source, 0, None, false)
            .evaluate(&probe, &control)
            .unwrap();
        let baseline = strict(&source, 0, None, false)
            .evaluate(&probe.language, &control)
            .unwrap();
        same_collection_coverage(&baseline, &optimized);
        assert!(probe.queries.borrow().is_empty());

        let target = compile_setup_search_conditions(
            &SetupSearchQuery::default()
                .with_remaining_pieces(vec![I, T, S, O])
                .with_next_cycle_remaining_pieces(vec![O]),
        )
        .unwrap()
        .remove(0)
        .terminal_supply_target()
        .unwrap();
        let (source, _) = universe(vec![vec![I, T, S, O]; 2], vec![0.5; 2]);
        let probe = Probe::new(Language::paths(&[(0xf, vec![I, T, S])]), 0);
        let mut evaluator = strict(&source, 0, None, false);
        evaluator.set_terminal_supply_target(Some(target));
        assert_eq!(
            evaluator
                .evaluate(&probe, &control)
                .unwrap()
                .covered_pattern_count,
            2
        );
        assert!(probe.queries.borrow().is_empty());

        let (source, _) = universe(vec![vec![I]], vec![1.0]);
        let probe = Probe::new(Language::paths(&[(0xf, vec![I, O])]), 0);
        let mut evaluator = QueueObservationPolicyEvaluator::for_original_universe(
            &source,
            QueueObservationPolicy::VisibleSeven,
            0,
            Some(O),
            true,
            true,
            false,
            None,
        )
        .unwrap();
        assert_eq!(
            evaluator
                .evaluate(&probe, &control)
                .unwrap()
                .covered_pattern_count,
            1
        );
        assert!(probe.queries.borrow().is_empty());

        let (source, _) = universe(vec![vec![I, O, T, S, Z, J]], vec![1.0]);
        let probe = Probe::new(Language::paths(&[(0xf, vec![I, O, T, S, Z, J, L])]), 1);
        let mut evaluator = QueueObservationPolicyEvaluator::for_original_universe(
            &source,
            QueueObservationPolicy::VisibleSeven,
            0,
            None,
            false,
            false,
            true,
            None,
        )
        .unwrap();
        assert_eq!(
            evaluator
                .evaluate(&probe, &control)
                .unwrap()
                .covered_pattern_count,
            1
        );
        assert!(probe.queries.borrow().is_empty());
    }

    #[test]
    fn revealed_word_membership_preserves_hold_draw_bounds_and_real_eof() {
        use PieceKind::{I, O};
        let control = ExecutionControl::default();
        let (source, _) = universe(vec![vec![I]], vec![1.0]);
        for (cursor, held, desired, expected) in
            [(0, None, O, 0), (0, Some(O), O, 1), (1, Some(I), I, 0)]
        {
            let probe = Probe::new(Language::paths(&[(0xf, vec![desired])]), 0);
            let mut evaluator = strict(&source, cursor, held, true);
            let optimized = evaluator.evaluate(&probe, &control).unwrap();
            let baseline = strict(&source, cursor, held, true)
                .evaluate(&probe.language, &control)
                .unwrap();
            same_collection_coverage(&baseline, &optimized);
            assert_eq!(optimized.covered_pattern_count, expected);
            let query = probe.queries.borrow()[0].1;
            assert_eq!(query.hold_code, held.map_or(0, piece_code));
            assert_eq!(query.len, 1 - cursor as u8);
            assert!(query.queue[usize::from(query.len)..]
                .iter()
                .all(|&piece| piece == 0));
        }
    }

    #[test]
    fn revealed_word_membership_caches_only_exact_values_and_resets_evaluation_and_forks() {
        use PieceKind::{I, O};
        let control = ExecutionControl::default();
        let (source, _) = universe(vec![vec![I, O]; 2], vec![0.5; 2]);
        let probe = Probe::new(Language::paths(&[(0xf, vec![I, O])]), 0);
        let mut evaluator = strict(&source, 0, None, false);
        evaluator.collect_full = false;
        let first = evaluator.evaluate(&probe, &control).unwrap();
        assert_eq!(
            probe.queries.borrow().len(),
            1,
            "collect reuses the exact raw-state value"
        );
        assert_eq!(first.metrics.action_checks, 0);
        assert_eq!(covered_ids(&first), vec![0, 1]);
        let second = evaluator.evaluate(&probe, &control).unwrap();
        assert_eq!(probe.queries.borrow().len(), 2);
        same_collection_coverage(&first, &second);
        #[cfg(not(target_family = "wasm"))]
        {
            let mut fork = evaluator.fork_empty().unwrap();
            assert!(fork.memo.is_empty() && fork.zero_scores.is_empty());
            assert!(!fork.projects_unplaced_lookahead && !fork.projects_standard_bag_lookahead);
            let forked = fork.evaluate(&probe, &control).unwrap();
            assert_eq!(probe.queries.borrow().len(), 3);
            same_collection_coverage(&first, &forked);
        }
        let rejecting = Probe::new(Language::paths(&[(0xf, vec![O, I])]), 0);
        let rejected = evaluator.evaluate(&rejecting, &control).unwrap();
        assert_eq!(rejected.covered_pattern_count, 0);
        assert!(covered_ids(&rejected).is_empty());
        assert_eq!(rejecting.queries.borrow().len(), 1);
        assert!(!evaluator.zero_scores.is_empty());

        let mut fallback = Probe::new(Language::paths(&[(0xf, vec![I, O])]), 0);
        fallback.reply = Reply::Fallback;
        let mut evaluator = strict(&source, 0, None, false);
        evaluator.prepare_trie(&control).unwrap();
        let initial = state(&evaluator, &fallback);
        assert!(evaluator
            .revealed_word_value(&fallback, initial, &control)
            .unwrap()
            .is_none());
        assert!(evaluator.memo.is_empty() && evaluator.zero_scores.is_empty());
        let result = evaluator.evaluate(&fallback, &control).unwrap();
        same_collection_coverage(&first, &result);
        assert!(result.metrics.action_checks > 0);
    }

    #[test]
    fn revealed_word_membership_errors_and_cancel_never_become_cached_rejection() {
        use PieceKind::I;
        let (source, _) = universe(vec![vec![I]], vec![1.0]);
        for reply in [Reply::Failure, Reply::Cancel] {
            let control = ExecutionControl::default();
            let mut probe = Probe::new(Language::paths(&[(0xf, vec![I])]), 0);
            probe.reply = reply;
            let mut evaluator = strict(&source, 0, None, false);
            evaluator.prepare_trie(&control).unwrap();
            let initial = state(&evaluator, &probe);
            let result = evaluator.solve(&probe, initial, &control);
            match reply {
                Reply::Failure => assert!(matches!(
                    result,
                    Err(WasmExactSearchError::InvalidProblem(
                        "word_membership_probe_failure"
                    ))
                )),
                Reply::Cancel => assert!(matches!(result, Err(WasmExactSearchError::Cancelled))),
                _ => unreachable!(),
            }
            assert!(evaluator.memo.is_empty() && evaluator.zero_scores.is_empty());
            assert!(evaluator.selected_patterns.is_empty());
        }
        let control = ExecutionControl::default();
        let probe = Probe::new(Language::paths(&[(0xf, vec![I])]), 0);
        let mut evaluator = strict(&source, 0, None, false);
        evaluator.prepare_trie(&control).unwrap();
        let initial = state(&evaluator, &probe);
        assert_eq!(
            evaluator
                .solve(&probe, initial, &control)
                .unwrap()
                .pattern_count,
            1
        );
        assert_eq!(probe.queries.borrow().len(), 1);
        evaluator.peer_abort = Some(Arc::new(AtomicBool::new(true)));
        assert!(matches!(
            evaluator.solve(&probe, initial, &control),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert_eq!(
            probe.queries.borrow().len(),
            1,
            "a cached hit still checks peer cancellation"
        );
    }
}
