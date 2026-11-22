//! Existential supply admission for a completed physical prefix relation.
//!
//! The caller owns the request certificate: a complete fresh Standard7Bag
//! universe, 7..=11 locks, exactly locks + 1 materialized draws, cursor zero,
//! initially empty allowed hold, no terminal supply target, and neither
//! unplaced nor synthetic standard-bag lookahead projection. This predicate
//! supplies no coverage, probability, representative queue, or path count.
//!
//! Under that certificate a fixed kind word is executable for some original
//! queue iff its first six kinds are distinct and every total kind count is
//! at most two. An active six-operation prefix lies on a complete physical
//! path, so its kind set and the candidate's fixed totals decide existence
//! without constructing a kind DAG or running a supply product.

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};

use crate::backend::wasm_cpu::{
    piece_index, symbolic_prefix_orders::ExactPrefixRelation, WasmExactSearchError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FreshBagWitness {
    pub(super) feasible: bool,
    /// Active subset bits actually inspected, including their depth check.
    /// The separate bounded piece-count check does not enter this counter.
    pub(super) checked_prefixes: usize,
}

pub(super) fn counts_fit(pieces: impl IntoIterator<Item = PieceKind>) -> bool {
    let mut counts = [0_u8; 7];
    for piece in pieces {
        let count = &mut counts[piece_index(piece)];
        *count += 1;
        if *count > 2 {
            return false;
        }
    }
    true
}

pub(super) fn has_fresh_bag_witness(
    relation: &ExactPrefixRelation,
    control: &ExecutionControl,
) -> Result<FreshBagWitness, WasmExactSearchError> {
    check_active_prefixes(relation.pieces(), relation.active_words(), control)
}

fn check_active_prefixes(
    pieces: &[PieceKind],
    active_words: &[u64],
    control: &ExecutionControl,
) -> Result<FreshBagWitness, WasmExactSearchError> {
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    if !(7..=11).contains(&pieces.len())
        || active_words.len() != (1_usize << pieces.len()).div_ceil(64)
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_observation_fresh_bag_witness_scope_invalid",
        ));
    }
    let mut result = FreshBagWitness {
        feasible: false,
        checked_prefixes: 0,
    };
    if !counts_fit(pieces.iter().copied()) {
        return Ok(result);
    }
    for (word_index, &active) in active_words.iter().enumerate() {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let mut pending = active;
        while pending != 0 {
            let lane = pending.trailing_zeros() as usize;
            pending &= pending - 1;
            let mut prefix = word_index * 64 + lane;
            result.checked_prefixes += 1;
            if prefix.count_ones() != 6 {
                continue;
            }
            let mut kinds = 0_u8;
            while prefix != 0 {
                let operation = prefix.trailing_zeros() as usize;
                prefix &= prefix - 1;
                let kind = 1_u8 << piece_index(pieces[operation]);
                if kinds & kind != 0 {
                    break;
                }
                kinds |= kind;
            }
            if kinds.count_ones() == 6 {
                result.feasible = true;
                return Ok(result);
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use clearra_rules::kicks::KickTableProfileId;
    use clearra_supply::{
        execution_automaton::{
            SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState, SupplyProvenanceId,
        },
        piece_source::PieceSourceId,
    };

    use super::*;
    use crate::backend::wasm_cpu::{
        buildup::{accepts_pattern_path, CoverageState},
        catalog::GeometryCatalog,
        geometry::GeometryCandidate,
        reachability::ReachabilityWorkspace,
        symbolic_prefix_orders::{SymbolicPrefixOrders, SymbolicPrefixWork},
    };

    fn chain_active(operation_order: &[usize]) -> Vec<u64> {
        let mut active = vec![0_u64; (1_usize << operation_order.len()).div_ceil(64)];
        let mut subset = 0;
        active[0] = 1;
        for &operation in operation_order {
            subset |= 1 << operation;
            active[subset / 64] |= 1 << (subset % 64);
        }
        active
    }

    fn word_result(word: &[PieceKind]) -> FreshBagWitness {
        let order: Vec<_> = (0..word.len()).collect();
        check_active_prefixes(word, &chain_active(&order), &ExecutionControl::default()).unwrap()
    }

    fn initial_supply() -> SupplyExecutionState {
        SupplyExecutionState::new(
            PieceSourceId::new(0x14),
            0,
            None,
            0,
            0,
            SupplyProvenanceId(0x14),
        )
    }

    /// Construct the theorem's queue; execution below uses the independently
    /// maintained supply automaton, not the production admission predicate.
    fn construct_supply(word: &[PieceKind]) -> (Vec<PieceKind>, Vec<SupplyBranchKind>) {
        let missing = PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .find(|piece| !word[..6].contains(piece))
            .unwrap();
        let mut queue = vec![missing];
        queue.extend_from_slice(&word[..6]);
        let mut branches = vec![SupplyBranchKind::StoreCurrent];
        branches.extend([SupplyBranchKind::Current; 5]);
        let tail = &word[6..];
        if let Some(first_missing) = tail.iter().position(|&piece| piece == missing) {
            queue.extend(
                tail.iter()
                    .enumerate()
                    .filter_map(|(index, &piece)| (index != first_missing).then_some(piece)),
            );
            let filler = PieceKind::STANDARD_TETROMINOES
                .into_iter()
                .find(|&piece| piece != missing && !queue[7..].contains(&piece))
                .unwrap();
            queue.push(filler);
            branches.extend((0..tail.len()).map(|index| {
                if index < first_missing {
                    SupplyBranchKind::Current
                } else {
                    SupplyBranchKind::SwapHeld
                }
            }));
        } else {
            queue.extend_from_slice(tail);
            branches.extend((0..tail.len()).map(|_| SupplyBranchKind::Current));
        }
        (queue, branches)
    }

    fn execute_constructed_supply(word: &[PieceKind]) -> CoverageState {
        let (queue, branches) = construct_supply(word);
        assert_eq!(queue.len(), word.len() + 1);
        assert_eq!(branches.len(), word.len());
        for bag in queue.chunks(7) {
            let kinds: HashSet<_> = bag.iter().copied().collect();
            assert_eq!(kinds.len(), bag.len(), "fresh-bag witness {queue:?}");
        }
        let mut state = initial_supply();
        for (&desired, branch) in word.iter().zip(branches) {
            let cursor = usize::from(state.cursor);
            let current = *queue.get(cursor).expect("every lock has an actual current");
            let step = SupplyExecutionAutomaton::sequence()
                .transition(state, branch, current, queue.get(cursor + 1).copied())
                .unwrap();
            assert_eq!(step.used_piece, desired, "word={word:?}, queue={queue:?}");
            state = step.next_state;
            assert!(usize::from(state.cursor) <= queue.len());
        }
        assert_eq!(usize::from(state.cursor), queue.len());
        assert!(state.hold_piece.is_some());
        let terminal = CoverageState {
            node: 0,
            cursor: state.cursor,
            hold: state.hold_piece,
        };
        assert!(accepts_pattern_path(queue.len(), false, terminal));
        terminal
    }

    /// Independent existential bag product for a few adversarial words. It
    /// explores real bag/hold transitions and never reads a prefix-kind mask.
    fn bag_product_accepts(word: &[PieceKind]) -> bool {
        let automaton =
            SupplyExecutionAutomaton::for_bag(&PieceKind::STANDARD_TETROMINOES).unwrap();
        let initial = automaton
            .project_bag_initial_state(initial_supply(), true, word.len())
            .unwrap();
        let mut states = HashSet::from([initial]);
        for &desired in word {
            let mut next = HashSet::new();
            for state in states {
                automaton
                    .for_each_matching_bag_step(state, desired, |step| {
                        if usize::from(step.next_state.cursor) <= word.len() + 1 {
                            next.insert(step.next_state);
                        }
                    })
                    .unwrap();
            }
            states = next;
        }
        !states.is_empty()
    }

    #[test]
    fn fresh_bag_witness_requires_global_cap_and_first_six_distinct() {
        use PieceKind::{I, J, L, O, S, T, Z};

        assert!(counts_fit([I, I, O, O, T, S, Z, J, L]));
        assert!(!counts_fit([I, O, T, S, Z, J, I, I]));
        let repeat = word_result(&[I, I, O, T, S, Z, J]);
        assert!(!repeat.feasible);
        assert_eq!(repeat.checked_prefixes, 8);
        let third = word_result(&[I, O, T, S, Z, J, I, I]);
        assert!(!third.feasible);
        assert_eq!(third.checked_prefixes, 0);
        let distinct = word_result(&[I, O, T, S, Z, J, I]);
        assert!(distinct.feasible);
        assert_eq!(distinct.checked_prefixes, 7);
    }

    #[test]
    fn fresh_bag_witness_reads_only_active_operation_subsets() {
        use PieceKind::{I, J, O, S, T, Z};

        let pieces = [I, I, O, T, S, Z, J];
        let control = ExecutionControl::default();
        let bad = chain_active(&[0, 1, 2, 3, 4, 5, 6]);
        let good = chain_active(&[1, 2, 3, 4, 5, 6, 0]);
        assert!(
            !check_active_prefixes(&pieces, &bad, &control)
                .unwrap()
                .feasible
        );
        assert!(
            check_active_prefixes(&pieces, &good, &control)
                .unwrap()
                .feasible
        );
        assert_eq!(
            check_active_prefixes(&pieces, &[0, 0], &control).unwrap(),
            FreshBagWitness {
                feasible: false,
                checked_prefixes: 0,
            }
        );
    }

    #[test]
    fn fresh_bag_witness_constructs_every_valid_short_tail_through_real_hold() {
        fn tails(
            word: &mut Vec<PieceKind>,
            counts: &mut [u8; 7],
            kinds: &[PieceKind; 7],
            remaining: usize,
            checked: &mut usize,
        ) {
            if remaining == 0 {
                assert!(word_result(word).feasible);
                execute_constructed_supply(word);
                *checked += 1;
                return;
            }
            for (index, &piece) in kinds.iter().enumerate() {
                if counts[index] == 2 {
                    continue;
                }
                counts[index] += 1;
                word.push(piece);
                tails(word, counts, kinds, remaining - 1, checked);
                word.pop();
                counts[index] -= 1;
            }
        }

        // Fixing six distinct positions loses no kind-word pattern: renaming
        // kinds is a symmetry. Rotate that naming as well to cover every enum
        // slot, and enumerate all valid tails of lengths 1..=5 (n=7..=11).
        let mut kinds = PieceKind::STANDARD_TETROMINOES;
        for _ in 0..7 {
            let mut checked = 0;
            for tail_len in 1..=5 {
                let mut word = kinds[..6].to_vec();
                let mut counts = [1, 1, 1, 1, 1, 1, 0];
                tails(&mut word, &mut counts, &kinds, tail_len, &mut checked);
            }
            assert_eq!(checked, 5_018);
            kinds.rotate_left(1);
        }
    }

    #[test]
    fn fresh_bag_witness_matches_independent_bag_product_on_boundary_words() {
        use PieceKind::{I, J, L, O, S, T, Z};

        let words: &[&[PieceKind]] = &[
            &[I, O, T, S, Z, J, L],
            &[I, O, T, S, Z, J, I],
            &[I, O, T, S, Z, J, L, L],
            &[I, O, T, S, Z, J, O, L],
            &[I, O, T, S, Z, J, L, O, L],
            &[I, O, T, S, Z, J, O, T, S, L],
            &[I, O, T, S, Z, J, L, L, I, O, T],
            &[I, I, O, T, S, Z, J],
            &[I, O, T, S, I, Z, J, L],
            &[I, O, T, S, Z, J, I, I],
            &[I, O, T, S, Z, J, L, L, L],
            &[I, O, T, S, Z, J, I, O, T, S, I],
        ];
        for word in words {
            assert_eq!(
                word_result(word).feasible,
                bag_product_accepts(word),
                "{word:?}"
            );
        }
    }

    #[test]
    fn fresh_bag_witness_projection_requires_a_separate_request_gate() {
        use PieceKind::{I, J, L, O, S, T, Z};

        for word in [
            vec![I, O, T, S, Z, J, L],
            vec![I, O, T, S, Z, J, L, L],
            vec![I, O, T, S, Z, J, O, T, S, L],
        ] {
            let terminal = execute_constructed_supply(&word);
            // Exactly N=n+1 actual draws and one unplaced held piece do not
            // meet the distinct projected-terminal contract. Never authorize
            // that mode merely because the ordinary hold word is feasible.
            assert!(!accepts_pattern_path(word.len() + 1, true, terminal));
        }
    }

    #[test]
    fn fresh_bag_witness_accepts_a_real_productive_ten_lock_geometry() {
        use PieceKind::{I, J, L, O, S, T, Z};

        // Existing empty-origin Jstris PC identity; IOTZSLJZTI is a real
        // no-hold order. This test consumes the compiler's certified active
        // relation rather than constructing an artificial authority token.
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
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, (1_u64 << 40) - 1)
                .unwrap();
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
        .unwrap();
        let result =
            has_fresh_bag_witness(orders.observation_relation().unwrap(), &control).unwrap();
        assert!(result.feasible);
        assert!(result.checked_prefixes > 0);
    }

    #[test]
    fn fresh_bag_witness_rejects_uncertified_lengths_and_propagates_cancellation() {
        let control = ExecutionControl::default();
        for length in [6, 12] {
            let pieces: Vec<_> = PieceKind::STANDARD_TETROMINOES
                .into_iter()
                .cycle()
                .take(length)
                .collect();
            assert!(matches!(
                check_active_prefixes(&pieces, &[], &control),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_fresh_bag_witness_scope_invalid"
                ))
            ));
        }
        let pieces = PieceKind::STANDARD_TETROMINOES;
        let active = chain_active(&[0, 1, 2, 3, 4, 5, 6]);
        control.cancellation.handle().cancel();
        assert!(matches!(
            check_active_prefixes(&pieces, &active, &control),
            Err(WasmExactSearchError::Cancelled)
        ));
    }
}
