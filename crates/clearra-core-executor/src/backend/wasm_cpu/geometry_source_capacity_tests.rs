use std::sync::Arc;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::pattern_universe::PieceMultisetKey;
use clearra_supply::queue::fixed_sequence::FixedSequence;
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;
use clearra_supply::QueueObservationPolicy;

use super::{
    selected_capacity_prefix_relaxation, selected_capacity_source_prefix_decision,
    FullDrawPrefixRequest, GeometryCatalog, TargetGroup,
};

fn problem(queue: PcQueueInput, hold: bool) -> SearchProblem {
    ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(6, 0),
            queue,
            PieceWindow::new(15),
        )
        .with_rule(srs_plus())
        .with_exact_pieces(Some(15))
        .with_supply_window_size(SupplyWindowSize::new(15))
        .with_min_remaining_queue(0)
        .with_hold_piece(None)
        .with_allow_hold(hold)
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
        .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
        .with_retained_trace_limit(1)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_exact_legal_board_enabled(false)
                .with_conditioned_reachability_enabled(false)
                .with_tablebase_requested(false)
                .with_max_patterns(0),
        ),
    )
    .unwrap()
}

fn expression(source: &str) -> PcQueueInput {
    PcQueueInput::pattern_expression(QueuePatternExpression::parse(source, 0).unwrap())
}

fn target(counts: [u8; 7]) -> TargetGroup {
    TargetGroup {
        key: PieceMultisetKey::from_counts(counts),
        pattern_index_id: 0,
        possible_patterns: Arc::new(PatternBitSet::new(1)),
        pattern_index: None,
    }
}

#[cfg(all(not(target_family = "wasm"), target_pointer_width = "64"))]
#[test]
fn full_draw_capacity_compiles_actual_atoms_fixed_prefixes_and_hold_policy() {
    let p1 = FullDrawPrefixRequest::compile(&problem(expression("P1P7P7"), true)).unwrap();
    assert_eq!(p1.prefix_max, [2; 7]);
    assert_eq!(p1.suffix_min, [1; 7]);
    let targets: Vec<_> = (0..7)
        .map(|extra| {
            let mut counts = [2; 7];
            counts[extra] += 1;
            target(counts)
        })
        .collect();
    let p1 = p1.for_targets(&targets).unwrap();
    assert!(p1.two_bag_plus_one);
    for (extra, target) in targets.iter().enumerate() {
        let mut caps = [1; 7];
        caps[extra] += 1;
        assert_eq!(p1.target_caps(target.key.counts()), Some(caps));
    }
    assert!(!p1.for_targets(&targets[..1]).unwrap().two_bag_plus_one);
    let mut wide_targets = targets.clone();
    wide_targets.extend([
        target([15, 0, 0, 0, 0, 0, 0]),
        target([0, 15, 0, 0, 0, 0, 0]),
    ]);
    assert!(p1.for_targets(&wide_targets).is_none());

    let op = FullDrawPrefixRequest::compile(&problem(expression("OP7P7"), true)).unwrap();
    assert_eq!(op.prefix_max, [1, 2, 1, 1, 1, 1, 1]);
    assert_eq!(op.suffix_min, [1; 7]);
    let op = op.for_targets(&[target([2, 3, 2, 2, 2, 2, 2])]).unwrap();
    assert!(!op.two_bag_plus_one);
    assert_eq!(op.target_caps([2, 3, 2, 2, 2, 2, 2]), Some(op.prefix_max));

    // The cut lies inside the second atom: its six remaining draws do not
    // force any individual kind. The fixed final I does force one I.
    let tail = FullDrawPrefixRequest::compile(&problem(expression("P7P7I"), true)).unwrap();
    assert_eq!(tail.prefix_max, [2; 7]);
    assert_eq!(tail.suffix_min, [1, 0, 0, 0, 0, 0, 0]);

    use PieceKind::{I, J, L, O, S, T, Z};
    let queue = [I, I, I, O, O, T, T, S, S, Z, Z, J, J, L, L];
    for (hold, expected_prefix, expected_suffix) in [
        (true, [3, 2, 2, 1, 0, 0, 0], [0, 0, 0, 1, 2, 2, 2]),
        (false, [3, 2, 2, 0, 0, 0, 0], [0, 0, 0, 2, 2, 2, 2]),
    ] {
        let fixed = FullDrawPrefixRequest::compile(&problem(
            PcQueueInput::fixed_sequence(FixedSequence::new(queue.to_vec())),
            hold,
        ))
        .unwrap();
        assert_eq!(fixed.prefix_max, expected_prefix);
        assert_eq!(fixed.suffix_min, expected_suffix);
        assert_eq!(
            fixed.target_caps([3, 2, 2, 2, 2, 2, 2]),
            Some(expected_prefix)
        );
    }
    let explicit =
        FullDrawPrefixRequest::compile(&problem(expression("IIIOOTTSSZZJJLL"), true)).unwrap();
    assert_eq!(explicit.prefix_max, [3, 2, 2, 1, 0, 0, 0]);
    assert_eq!(explicit.suffix_min, [0, 0, 0, 1, 2, 2, 2]);

    // Correlated explicit alternatives remain with the exact queue owner.
    // This optional request never enumerates them or assumes independence.
    assert!(FullDrawPrefixRequest::compile(&problem(
        expression("IIIOOTTSSZZJJLL;OIIIOOTTSZZJJLL"),
        true,
    ))
    .is_none());
}

/// Reference: enumerate relaxed lock subsets, adding free future operations
/// and clearing all completed rows automatically. This does not use the
/// product's row-state traversal or capped-availability sum.
fn oracle_has_prefix(
    prefix_locks: usize,
    target: [u8; 7],
    caps: [u8; 7],
    selected_kinds: &[usize],
    rows: &[u16],
    requirements: &[Vec<u16>],
) -> bool {
    let mut pieces = selected_kinds.to_vec();
    for kind in 0..7 {
        let used = selected_kinds
            .iter()
            .filter(|&&piece| piece == kind)
            .count();
        if used > usize::from(target[kind]) {
            return false;
        }
        pieces.extend(std::iter::repeat_n(kind, usize::from(target[kind]) - used));
    }
    let mut reachable = vec![false; 1 << pieces.len()];
    reachable[0] = true;
    for placed in 0_usize..reachable.len() {
        if !reachable[placed] {
            continue;
        }
        if placed.count_ones() as usize == prefix_locks {
            return true;
        }
        let mut counts = [0_u8; 7];
        for (operation, &kind) in pieces.iter().enumerate() {
            counts[kind] += u8::from(placed & (1 << operation) != 0);
        }
        let deleted = rows.iter().enumerate().fold(0_u16, |mask, (row, owners)| {
            mask | if owners & !(placed as u16) == 0 {
                1 << row
            } else {
                0
            }
        });
        for (operation, &kind) in pieces.iter().enumerate() {
            if placed & (1 << operation) != 0 || counts[kind] >= caps[kind] {
                continue;
            }
            if operation >= selected_kinds.len()
                || requirements[operation]
                    .iter()
                    .any(|required| required & !deleted == 0)
            {
                reachable[placed | (1 << operation)] = true;
            }
        }
    }
    false
}

#[test]
fn general_capacity_relaxation_matches_independent_lock_subset_oracle() {
    let mut positives = 0;
    let mut negatives = 0;
    for counts in [[3, 2, 0, 0, 0, 0, 0], [2, 2, 2, 0, 0, 0, 0]] {
        let pieces: Vec<_> = counts
            .iter()
            .enumerate()
            .flat_map(|(kind, &count)| std::iter::repeat_n(kind, usize::from(count)))
            .collect();
        for cap_pattern in [[1, 1, 1, 0, 0, 0, 0], [2, 1, 1, 0, 0, 0, 0], counts] {
            let caps = std::array::from_fn(|kind| cap_pattern[kind].min(counts[kind]));
            for variant in 0..8 {
                for selected in 0..1_usize << pieces.len() {
                    let selected_kinds: Vec<_> = pieces
                        .iter()
                        .enumerate()
                        .filter_map(|(operation, &kind)| {
                            (selected & (1 << operation) != 0).then_some(kind)
                        })
                        .collect();
                    let mut masks = [0_u16; 7];
                    let mut rows = [0_u16; 3];
                    let mut requirements = Vec::new();
                    for (operation, &kind) in selected_kinds.iter().enumerate() {
                        masks[kind] |= 1 << operation;
                        let occupied = ((operation + 1) * (variant + 3)) & 7;
                        for (row, owners) in rows.iter_mut().enumerate() {
                            if occupied & (1 << row) != 0 {
                                *owners |= 1 << operation;
                            }
                        }
                        requirements.push(match (variant + operation) % 4 {
                            0 => vec![0],
                            1 => vec![1 << (operation % 3)],
                            2 => vec![1, 6],
                            _ => vec![7],
                        });
                    }
                    let expected =
                        oracle_has_prefix(3, counts, caps, &selected_kinds, &rows, &requirements);
                    let actual = selected_capacity_prefix_relaxation(
                        3,
                        counts,
                        caps,
                        masks,
                        &rows,
                        |operation, deleted| {
                            requirements[operation]
                                .iter()
                                .any(|required| required & !deleted == 0)
                        },
                    );
                    assert_eq!(
                        !actual.proved_infeasible, expected,
                        "counts={counts:?} caps={caps:?} variant={variant} selected={selected:#x}"
                    );
                    positives += usize::from(expected);
                    negatives += usize::from(!expected);
                }
            }
        }
    }
    assert!(positives > 0 && negatives > 0);
}

#[cfg(all(not(target_family = "wasm"), target_pointer_width = "64"))]
#[test]
fn general_capacity_keeps_any_compatible_target_and_rejects_only_all_targets() {
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(10, 6, 0, (1_u64 << 60) - 1)
            .unwrap();
    let request = FullDrawPrefixRequest::compile(&problem(
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 15])),
        true,
    ))
    .unwrap();
    let impossible = target([0, 15, 0, 0, 0, 0, 0]);
    let possible = target([15, 0, 0, 0, 0, 0, 0]);
    assert!(
        selected_capacity_source_prefix_decision(
            request,
            std::slice::from_ref(&impossible),
            &catalog,
            &[],
        )
        .proved_infeasible
    );
    for targets in [
        [impossible.clone(), possible.clone()],
        [possible, impossible],
    ] {
        let request = request.for_targets(&targets).unwrap();
        assert!(!request.two_bag_plus_one);
        assert!(
            !selected_capacity_source_prefix_decision(request, &targets, &catalog, &[])
                .proved_infeasible
        );
    }
}
