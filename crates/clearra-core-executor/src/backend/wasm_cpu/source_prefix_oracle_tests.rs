//! Independent finite-source oracles for the negative-only graph prefix filter.
//!
//! The source union theorem concerns ordered kind words and existence of one
//! original queue. It grants neither physical reachability nor pointwise
//! original-queue coverage; the production graph/product retains both checks.

use std::collections::{BTreeSet, HashSet};

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::{
    execution_automaton::{
        SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState,
        SupplyObservationIdentity, SupplyProvenanceId,
    },
    hold::hold_policy::HoldPolicy,
    pattern_universe::PatternUniverseMaterializer,
    piece_source::{PieceSourceId, PieceSourceKind},
    queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
    QueueObservationPolicy,
};

use super::super::{catalog::GeometryCatalog, geometry::GeometryCandidate};
use super::FullDrawSourcePrefix;

fn index(piece: PieceKind) -> usize {
    PieceKind::STANDARD_TETROMINOES
        .iter()
        .position(|&kind| kind == piece)
        .unwrap()
}

fn counts(word: &[PieceKind]) -> [u8; 7] {
    let mut result = [0; 7];
    for &piece in word {
        result[index(piece)] += 1;
    }
    result
}

fn initial() -> SupplyExecutionState {
    SupplyExecutionState::with_contract(
        PieceSourceId::new(91),
        PieceSourceKind::MaterializedPatternUniverse,
        0,
        None,
        HoldPolicy::Allowed,
        0,
        0,
        SupplyObservationIdentity::full_queue_oracle(),
        SupplyProvenanceId(91),
    )
}

fn kind_operations(bag_size: usize, extra: usize) -> [u16; 7] {
    let mut result = [0; 7];
    let mut operation = 0;
    for (kind, mask) in result.iter_mut().enumerate().take(bag_size) {
        for _ in 0..2 + usize::from(kind == extra) {
            *mask |= 1 << operation;
            operation += 1;
        }
    }
    result
}

fn encode(word: &[PieceKind]) -> u64 {
    word.iter()
        .fold(0, |code, &piece| (code << 3) | index(piece) as u64)
}

/// All real draws go through the canonical automaton, not a second Hold
/// transition implementation. The sole projected step is a retained piece's
/// final release at genuine finite EOF; it draws no replacement piece.
fn canonical_steps(
    queue: &[PieceKind],
    state: SupplyExecutionState,
    depth: usize,
) -> [Option<(PieceKind, SupplyExecutionState)>; 3] {
    let cursor = usize::from(state.cursor());
    assert!(cursor <= queue.len());
    if cursor == queue.len() {
        let release = state
            .hold_piece()
            .filter(|_| depth + 1 == queue.len())
            .map(|piece| {
                let mut released = state;
                released.hold_piece = None;
                released.hold_empty = true;
                (piece, released)
            });
        return [release, None, None];
    }
    let automaton = SupplyExecutionAutomaton::sequence();
    [
        SupplyBranchKind::Current,
        SupplyBranchKind::SwapHeld,
        SupplyBranchKind::StoreCurrent,
    ]
    .map(|branch| {
        automaton
            .transition(state, branch, queue[cursor], queue.get(cursor + 1).copied())
            .ok()
            .map(|step| (step.used_piece, step.next_state))
    })
}

fn finite_accepts(queue: &[PieceKind], word: &[PieceKind]) -> bool {
    let mut states = vec![initial()];
    for (depth, &expected) in word.iter().enumerate() {
        let mut next = Vec::new();
        for state in states {
            for (piece, reached) in canonical_steps(queue, state, depth).into_iter().flatten() {
                if piece == expected && !next.contains(&reached) {
                    next.push(reached);
                }
            }
        }
        states = next;
    }
    states
        .iter()
        .any(|state| usize::from(state.cursor()) == queue.len() && state.hold_piece().is_none())
}

fn prefix_condition(word: &[PieceKind], bag_size: usize, extra: usize) -> bool {
    let used = counts(&word[..bag_size]);
    used.iter()
        .enumerate()
        .all(|(kind, &count)| count <= u8::from(kind < bag_size) + u8::from(kind == extra))
}

fn guard_accepts(filter: &FullDrawSourcePrefix, operations: &[u16; 7], word: &[PieceKind]) -> bool {
    let mut subset = 0;
    for &piece in word {
        let remaining = operations[index(piece)] & !subset;
        if remaining == 0 {
            return false;
        }
        let bit = 1_u16 << remaining.trailing_zeros();
        if filter.permitted_operations(subset) & bit == 0 {
            return false;
        }
        subset |= bit;
    }
    true
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Draw(SupplyBranchKind),
    ReleaseHeld,
}

struct Witness {
    queue: Vec<PieceKind>,
    actions: Vec<Action>,
    held_at_boundary: PieceKind,
    first_case: usize,
}

/// Construct p + permutation(A) + permutation(A) for a fixed ordered word.
/// This is deliberately separate from both the filter and canonical oracle.
fn construct_word(word: &[PieceKind], bag_size: usize, extra: usize) -> Option<Witness> {
    if word.len() != 2 * bag_size + 1 || !prefix_condition(word, bag_size, extra) {
        return None;
    }
    let inventory = counts(word);
    if inventory
        .iter()
        .enumerate()
        .any(|(kind, &count)| count != 2 * u8::from(kind < bag_size) + u8::from(kind == extra))
    {
        return None;
    }
    let prefix = &word[..bag_size];
    let prefix_counts = counts(prefix);
    let held_index =
        (0..bag_size).find(|&kind| prefix_counts[kind] < 1 + u8::from(kind == extra))?;
    let held = PieceKind::STANDARD_TETROMINOES[held_index];
    let extra_piece = PieceKind::STANDARD_TETROMINOES[extra];
    let mut queue = vec![extra_piece];
    let mut actions = Vec::new();
    let first_case;
    if held == extra_piece {
        first_case = 0;
        queue.extend_from_slice(prefix);
        actions.push(Action::Draw(SupplyBranchKind::StoreCurrent));
        actions.extend((1..bag_size).map(|_| Action::Draw(SupplyBranchKind::Current)));
    } else if prefix[0] == extra_piece {
        first_case = 1;
        queue.push(held);
        queue.extend_from_slice(&prefix[1..]);
        actions.push(Action::Draw(SupplyBranchKind::Current));
        actions.push(Action::Draw(SupplyBranchKind::StoreCurrent));
        actions.extend((2..bag_size).map(|_| Action::Draw(SupplyBranchKind::Current)));
    } else {
        first_case = 2;
        let first_extra = prefix.iter().position(|&piece| piece == extra_piece)?;
        for (depth, &piece) in prefix.iter().enumerate() {
            queue.push(if depth == first_extra { held } else { piece });
            actions.push(Action::Draw(if depth == 0 {
                SupplyBranchKind::StoreCurrent
            } else if depth == first_extra {
                SupplyBranchKind::SwapHeld
            } else {
                SupplyBranchKind::Current
            }));
        }
    }
    let suffix = &word[bag_size..];
    let last = suffix[bag_size];
    let swap_at = if last == held {
        None
    } else {
        Some(suffix[..bag_size].iter().position(|&piece| piece == held)?)
    };
    for (depth, &piece) in suffix[..bag_size].iter().enumerate() {
        queue.push(if swap_at == Some(depth) { last } else { piece });
        actions.push(Action::Draw(if swap_at == Some(depth) {
            SupplyBranchKind::SwapHeld
        } else {
            SupplyBranchKind::Current
        }));
    }
    actions.push(Action::ReleaseHeld);
    Some(Witness {
        queue,
        actions,
        held_at_boundary: held,
        first_case,
    })
}

fn check_constructed_word(witness: &Witness, word: &[PieceKind], bag_size: usize, extra: usize) {
    assert_eq!(witness.queue.len(), 2 * bag_size + 1);
    assert_eq!(witness.actions.len(), word.len());
    assert_eq!(index(witness.queue[0]), extra);
    let expected_bag: [u8; 7] = std::array::from_fn(|kind| u8::from(kind < bag_size));
    assert_eq!(counts(&witness.queue[1..bag_size + 1]), expected_bag);
    assert_eq!(counts(&witness.queue[bag_size + 1..]), expected_bag);
    let automaton = SupplyExecutionAutomaton::sequence();
    let mut state = initial();
    for (depth, (&expected, &action)) in word.iter().zip(&witness.actions).enumerate() {
        match action {
            Action::Draw(branch) => {
                let cursor = usize::from(state.cursor());
                assert!(cursor < witness.queue.len());
                let step = automaton
                    .transition(
                        state,
                        branch,
                        witness.queue[cursor],
                        witness.queue.get(cursor + 1).copied(),
                    )
                    .unwrap();
                assert_eq!(step.used_piece, expected, "depth={depth}, word={word:?}");
                state = step.next_state;
            }
            Action::ReleaseHeld => {
                assert_eq!(depth + 1, word.len());
                assert_eq!(usize::from(state.cursor()), witness.queue.len());
                assert_eq!(state.hold_piece(), Some(expected));
                state.hold_piece = None;
                state.hold_empty = true;
            }
        }
        assert_eq!(
            usize::from(state.cursor()),
            depth + 1 + usize::from(state.hold_piece().is_some())
        );
        if depth + 1 == bag_size {
            assert_eq!(usize::from(state.cursor()), bag_size + 1);
            assert_eq!(state.hold_piece(), Some(witness.held_at_boundary));
        }
    }
    assert_eq!(usize::from(state.cursor()), witness.queue.len());
    assert!(state.hold_piece().is_none());
}

fn visit_words(
    remaining: &mut [u8; 7],
    word: &mut Vec<PieceKind>,
    visit: &mut impl FnMut(&[PieceKind]),
) {
    if remaining.iter().all(|&count| count == 0) {
        visit(word);
        return;
    }
    for kind in 0..7 {
        if remaining[kind] != 0 {
            remaining[kind] -= 1;
            word.push(PieceKind::STANDARD_TETROMINOES[kind]);
            visit_words(remaining, word, visit);
            word.pop();
            remaining[kind] += 1;
        }
    }
}

#[test]
fn source_prefix_small_alphabets_match_every_canonical_finite_queue_and_word() {
    let mut construction_cases = [0_usize; 3];
    let mut final_hold_cases = [0_usize; 2];
    for bag_size in 2..=4 {
        let alphabet = PieceKind::STANDARD_TETROMINOES[..bag_size]
            .iter()
            .map(|piece| piece.as_ascii())
            .collect::<String>();
        let expression = format!("[{alphabet}]1[{alphabet}]{bag_size}[{alphabet}]{bag_size}");
        let source = PatternUniverseMaterializer::queue_pattern_expression(
            &QueuePatternExpression::parse(&expression, 0).unwrap(),
            91,
        )
        .unwrap();
        let factorial = (1..=bag_size).product::<usize>();
        assert_eq!(source.pattern_count(), bag_size * factorial * factorial);
        let operations = (0..bag_size)
            .map(|extra| kind_operations(bag_size, extra))
            .collect::<Vec<_>>();
        let filters = operations
            .iter()
            .map(|&masks| {
                FullDrawSourcePrefix::from_kind_operations(masks, bag_size as u8).unwrap()
            })
            .collect::<Vec<_>>();
        let mut accepted = vec![BTreeSet::new(); bag_size];
        let mut prefix_visits = 0;
        for ordinal in 0..source.pattern_count() {
            let queue = source.sequence_at(ordinal);
            assert_eq!(queue.len(), 2 * bag_size + 1);
            let extra = index(queue[0]);
            let mut states = vec![(initial(), 0_u64, 0_u16)];
            for depth in 0..queue.len() {
                prefix_visits += states.len();
                let mut next = Vec::new();
                let mut seen = HashSet::new();
                for (state, word, subset) in states {
                    assert_eq!(
                        usize::from(state.cursor()),
                        depth + usize::from(state.hold_piece().is_some())
                    );
                    let distinct = operations[extra]
                        .iter()
                        .filter(|&&mask| mask & subset != 0)
                        .count();
                    assert!(distinct >= depth.saturating_sub(1).min(bag_size - 1));
                    for (piece, reached) in
                        canonical_steps(&queue, state, depth).into_iter().flatten()
                    {
                        let remaining = operations[extra][index(piece)] & !subset;
                        assert_ne!(remaining, 0);
                        let bit = 1_u16 << remaining.trailing_zeros();
                        assert_ne!(
                            filters[extra].permitted_operations(subset) & bit,
                            0,
                            "removed a real source prefix: {expression} ordinal={ordinal} depth={depth}"
                        );
                        let next_word = (word << 3) | index(piece) as u64;
                        if seen.insert((reached, next_word)) {
                            next.push((reached, next_word, subset | bit));
                        }
                    }
                }
                states = next;
            }
            for (state, word, subset) in states {
                assert_eq!(usize::from(state.cursor()), queue.len());
                assert!(state.hold_piece().is_none());
                assert_eq!(subset.count_ones() as usize, queue.len());
                accepted[extra].insert(word);
            }
        }

        let mut total_words = 0;
        let mut admitted_words = 0;
        for extra in 0..bag_size {
            let mut inventory =
                std::array::from_fn(|kind| 2 * u8::from(kind < bag_size) + u8::from(kind == extra));
            let mut accepted_count = 0;
            visit_words(&mut inventory, &mut Vec::new(), &mut |word| {
                total_words += 1;
                let expected = accepted[extra].contains(&encode(word));
                assert_eq!(
                    prefix_condition(word, bag_size, extra),
                    expected,
                    "word={word:?}"
                );
                assert_eq!(
                    guard_accepts(&filters[extra], &operations[extra], word),
                    expected,
                    "word={word:?}"
                );
                let witness = construct_word(word, bag_size, extra);
                assert_eq!(witness.is_some(), expected);
                if let Some(witness) = witness {
                    check_constructed_word(&witness, word, bag_size, extra);
                    construction_cases[witness.first_case] += 1;
                    final_hold_cases
                        [usize::from(word.last() == Some(&witness.held_at_boundary))] += 1;
                    accepted_count += 1;
                }
            });
            assert_eq!(accepted_count, accepted[extra].len());
            admitted_words += accepted_count;
        }
        eprintln!(
            "source_prefix_oracle alphabet={bag_size} originals={} prefix_states={prefix_visits} all_words={total_words} accepted_words={admitted_words}",
            source.pattern_count()
        );
    }
    assert!(construction_cases.iter().all(|&count| count > 0));
    assert!(final_hold_cases.iter().all(|&count| count > 0));
}

#[test]
fn source_prefix_full7_constructions_keep_eof_and_pointwise_coverage_separate() {
    use PieceKind::{I, J, L, O, S, T, Z};

    let words = [
        [I, O, T, S, Z, J, L, O, T, S, Z, J, L, I, I],
        [O, I, T, S, Z, J, L, I, T, S, Z, J, L, I, O],
        [I, T, S, Z, J, L, I, I, O, T, S, Z, J, L, O],
        [T, I, S, Z, J, L, I, O, I, T, S, Z, J, O, L],
    ];
    // Relabel the same three constructive cases for every possible extra
    // kind, including kinds outside the smaller oracle alphabets.
    for extra in 0..7 {
        let masks = kind_operations(7, extra);
        let filter = FullDrawSourcePrefix::from_kind_operations(masks, 7).unwrap();
        for word in words {
            let renamed = word.map(|piece| {
                if piece == I {
                    PieceKind::STANDARD_TETROMINOES[extra]
                } else if index(piece) == extra {
                    I
                } else {
                    piece
                }
            });
            assert!(guard_accepts(&filter, &masks, &renamed));
            let witness = construct_word(&renamed, 7, extra).unwrap();
            check_constructed_word(&witness, &renamed, 7, extra);
            assert!(finite_accepts(&witness.queue, &renamed));
        }
    }
    // Existence of one source cannot supply another original ordinal's bit.
    // This word starts O, while the alternative original starts I,I and
    // therefore cannot emit O on its first lock under any Hold branch.
    let other_original = [I, I, O, T, S, Z, J, L, I, O, T, S, Z, J, L];
    assert!(prefix_condition(&words[1], 7, index(I)));
    assert!(!finite_accepts(&other_original, &words[1]));

    // This exact harddrop order has a physical witness in the separate
    // mixed15 regression. Its inventory agrees, but T and S repeat before
    // the first bag boundary: that particular word has no source witness.
    // This says nothing about another physical order for the same tiling.
    let physical_only = [I, T, T, Z, S, S, J, Z, L, I, I, O, O, L, J];
    let masks = kind_operations(7, index(I));
    let filter = FullDrawSourcePrefix::from_kind_operations(masks, 7).unwrap();
    assert_eq!(counts(&physical_only), counts(&words[0]));
    assert!(!guard_accepts(&filter, &masks, &physical_only));
    assert!(construct_word(&physical_only, 7, index(I)).is_none());
}

#[test]
fn source_prefix_kind_masks_preserve_operation_reindexing_and_reject_bad_inventory() {
    let masks = kind_operations(7, 0);
    let reverse = |mask: u16| -> u16 {
        (0..15).fold(0, |mapped, bit| mapped | ((mask >> bit & 1) << (14 - bit)))
    };
    let mapped = masks.map(reverse);
    let original = FullDrawSourcePrefix::from_kind_operations(masks, 7).unwrap();
    let reordered = FullDrawSourcePrefix::from_kind_operations(mapped, 7).unwrap();
    for subset in 0..1_u16 << 15 {
        let expected = reverse(original.permitted_operations(subset) & !subset & 0x7fff);
        let actual = reordered.permitted_operations(reverse(subset)) & !reverse(subset) & 0x7fff;
        assert_eq!(actual, expected, "subset={subset:#x}");
        if subset.count_ones() >= 7 {
            assert_eq!(original.permitted_operations(subset), u16::MAX);
        }
    }
    let small = kind_operations(3, 0);
    let mut overlap = small;
    overlap[1] |= 1;
    let mut missing = small;
    missing[2] &= missing[2] - 1;
    let mut outside_alphabet = small;
    outside_alphabet[6] = 1 << 7;
    for invalid in [
        overlap,
        missing,
        outside_alphabet,
        small.map(|mask| mask << 1),
    ] {
        assert!(FullDrawSourcePrefix::from_kind_operations(invalid, 3).is_none());
    }
    assert!(FullDrawSourcePrefix::from_kind_operations(small, 0).is_none());
    assert!(FullDrawSourcePrefix::from_kind_operations(small, 8).is_none());
}

fn full_query(queue: PcQueueInput) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(6, 0),
        queue,
        PieceWindow::new(15),
    )
    .with_rule(srs_plus())
    .with_allow_hold(true)
    .with_hold_piece(None)
    .with_exact_pieces(Some(15))
    .with_supply_window_size(SupplyWindowSize::new(15))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_retained_trace_limit(1)
    .with_execution_policy(PcExecutionPolicy::mvp_default().with_max_patterns(0))
}

#[cfg(all(not(target_family = "wasm"), target_pointer_width = "64"))]
#[test]
fn source_prefix_compile_requires_actual_full15_owner_empty_hold_and_inventory() {
    use PieceKind::{I, J, L, O, S, T, Z};

    let pattern =
        || PcQueueInput::pattern_expression(QueuePatternExpression::parse("P1P7P7", 0).unwrap());
    let query = full_query(pattern());
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let catalog = GeometryCatalog::compile(&problem).unwrap();
    let placements = [
        (I, 0x4010_0401_u64),
        (T, 0x100e),
        (T, 0x8070),
        (Z, 0x0003_0180),
        (S, 0x0300_6000),
        (S, 0x0006_00c0_0000),
        (J, 0x0001_8020_0800),
        (Z, 0x0018_0c00_0000),
        (L, 0x0060_1004_0000),
        (I, 0x0080_2008_0200),
        (I, 0x001e << 40),
        (O, 0x0003_00c0 << 40),
        (O, 0x000c_0300 << 40),
        (L, 0x1c01 << 40),
        (J, 0xe020 << 40),
    ];
    let rows = placements
        .iter()
        .map(|&(kind, cells)| catalog.skeleton_id(kind, cells).unwrap())
        .collect::<Vec<_>>();
    let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
    let source = problem.piece_source().materialized_universe().unwrap();
    assert_eq!(source.pattern_count(), 177_811_200);
    assert!(source.complete_factorized_layout().is_some());
    assert!(FullDrawSourcePrefix::compile(&problem, &catalog, &candidate).is_some());
    for evidence_problem in [
        problem.clone().with_pc_chance_probability_v2_evidence(),
        problem.clone().with_build_replay_evidence(),
    ] {
        assert!(FullDrawSourcePrefix::compile(&evidence_problem, &catalog, &candidate).is_none());
    }

    for alternative in [
        query.clone().with_hold_piece(Some(I)),
        query.clone().with_allow_hold(false),
        query.clone().with_count_policy(PcCountPolicy::CountAll),
        query
            .clone()
            .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven),
        query.clone().with_retained_trace_limit(2),
        full_query(PcQueueInput::pattern_expression(
            QueuePatternExpression::parse("P7P1P7", 0).unwrap(),
        )),
        full_query(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            I, I, O, T, S, Z, J, L, I, O, T, S, Z, J, L,
        ]))),
    ] {
        let other = ProblemCompiler::compile_scenario_pc(&alternative).unwrap();
        assert!(FullDrawSourcePrefix::compile(&other, &catalog, &candidate).is_none());
    }
    let shortened = GeometryCandidate::from_rows(&catalog, 0, &rows[..14]).unwrap();
    assert!(FullDrawSourcePrefix::compile(&problem, &catalog, &shortened).is_none());
    let square_rows = (0..3)
        .flat_map(|pair| (0..5).map(move |column| (pair, column)))
        .map(|(pair, column)| {
            let bottom = 3_u64 << (20 * pair + 2 * column);
            catalog.skeleton_id(O, bottom | (bottom << 10)).unwrap()
        })
        .collect::<Vec<_>>();
    let wrong_inventory = GeometryCandidate::from_rows(&catalog, 0, &square_rows).unwrap();
    assert!(FullDrawSourcePrefix::compile(&problem, &catalog, &wrong_inventory).is_none());
}
