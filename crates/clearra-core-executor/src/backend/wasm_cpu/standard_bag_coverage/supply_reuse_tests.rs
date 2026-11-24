//! A certified Current draw is reusable by Hold branches. Compare against
//! the existing matching-step path, including its same-kind edge loop hoist.

use std::cell::Cell;

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_supply::{
    bag::BagState,
    hold_automaton::{HoldAutomatonState, SupplyProvenanceId},
    pattern_universe::{MaterializedPatternUniverse, PatternUniverseMaterializer},
    piece_source::PieceSourceId,
};

use super::super::{
    compact_exact_u64_map::ExactU64MemoStorage,
    piece_order_language::PieceOrderLanguageCache,
    standard_bag_product_memo::{StandardBagProductMemo, StandardBagProductMemoLayout},
};
use super::{StandardBagCoverage, StandardBagCoverageResult, ACCEPT};

#[derive(Clone, Copy)]
enum DrawReuse {
    Enabled,
    Reference,
}

thread_local! {
    static DRAW_REUSE: Cell<DrawReuse> = const { Cell::new(DrawReuse::Enabled) };
    static CURSOR_ADVANCES: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn reuse_supply_draws() -> bool {
    DRAW_REUSE.with(|mode| matches!(mode.get(), DrawReuse::Enabled))
}

pub(super) fn record_cursor_advance() {
    CURSOR_ADVANCES.with(|count| count.set(count.get().saturating_add(1)));
}

fn measure<T>(mode: DrawReuse, operation: impl FnOnce() -> T) -> (T, usize) {
    struct Restore(DrawReuse);
    impl Drop for Restore {
        fn drop(&mut self) {
            DRAW_REUSE.with(|mode| mode.set(self.0));
        }
    }
    let previous = DRAW_REUSE.with(|cell| cell.replace(mode));
    let _restore = Restore(previous);
    CURSOR_ADVANCES.with(|count| count.set(0));
    let value = operation();
    (value, CURSOR_ADVANCES.with(Cell::get))
}

fn initial(held: Option<PieceKind>) -> HoldAutomatonState {
    HoldAutomatonState::new(
        PieceSourceId::new(97),
        0,
        held,
        0,
        BagState::fresh_standard_7_bag().packed_remainder_key(),
        SupplyProvenanceId(97),
    )
}

fn assert_same_result(reference: &StandardBagCoverageResult, reused: &StandardBagCoverageResult) {
    assert_eq!(reused.root, reference.root);
    assert_eq!(reused.covers_any_pattern, reference.covers_any_pattern);
    assert_eq!(
        reused.covered_pattern_count,
        reference.covered_pattern_count
    );
    assert_eq!(reused.witness_pattern_id, reference.witness_pattern_id);
    assert_eq!(reused.product_states, reference.product_states);
    assert_eq!(reused.edge_checks, reference.edge_checks);
}

fn compare(
    source: &MaterializedPatternUniverse,
    held: Option<PieceKind>,
    hold_enabled: bool,
    projected: bool,
    pieces: &[PieceKind],
    predecessors: &[u16],
) -> (usize, usize) {
    let mut language = PieceOrderLanguageCache::default();
    let compiled = language
        .compile_poset_language(pieces, predecessors, 1 << pieces.len())
        .unwrap()
        .unwrap();
    let control = ExecutionControl::default();
    let mut work = (0, 0);
    for (layout, storage) in [
        (
            StandardBagProductMemoLayout::Flat,
            ExactU64MemoStorage::Reference,
        ),
        (
            StandardBagProductMemoLayout::Flat,
            ExactU64MemoStorage::Compact,
        ),
        (
            StandardBagProductMemoLayout::StateMajor,
            ExactU64MemoStorage::Reference,
        ),
    ] {
        let make = || {
            let mut coverage =
                StandardBagCoverage::for_universe(source, initial(held), hold_enabled, projected)
                    .unwrap()
                    .unwrap();
            coverage.product_memo = StandardBagProductMemo::new(
                layout,
                storage,
                coverage.tables.cursor_transitions.max_source_depth,
            );
            coverage
        };
        let mut reference = make();
        let mut reused = make();
        for epoch in 0..2 {
            let (a, old_draws) = measure(DrawReuse::Reference, || {
                reference
                    .cover_language(&language, compiled.root, &control)
                    .unwrap()
            });
            let (b, new_draws) = measure(DrawReuse::Enabled, || {
                reused
                    .cover_language(&language, compiled.root, &control)
                    .unwrap()
            });
            assert_same_result(&a, &b);
            assert_eq!(
                reused.materialize_root(b.root).unwrap(),
                reference.materialize_root(a.root).unwrap()
            );
            assert_eq!(reused.retained_bytes(), reference.retained_bytes());
            assert_eq!(reused.local_live_bytes(), reference.local_live_bytes());
            let old_memo = reference.memo_accounting();
            let new_memo = reused.memo_accounting();
            assert_eq!(
                (
                    new_memo.product_entries,
                    new_memo.union_entries,
                    new_memo.product_capacity,
                    new_memo.union_capacity,
                    new_memo.product_directory_bytes,
                    new_memo.product_payload_bytes,
                    new_memo.union_payload_bytes,
                ),
                (
                    old_memo.product_entries,
                    old_memo.union_entries,
                    old_memo.product_capacity,
                    old_memo.union_capacity,
                    old_memo.product_directory_bytes,
                    old_memo.product_payload_bytes,
                    old_memo.union_payload_bytes,
                )
            );
            assert!(new_draws <= old_draws, "epoch={epoch}");
            if !hold_enabled {
                assert_eq!(new_draws, old_draws);
            }
            work.0 += old_draws;
            work.1 += new_draws;

            let warm_a = reference
                .cover_language(&language, compiled.root, &control)
                .unwrap();
            let warm_b = reused
                .cover_language(&language, compiled.root, &control)
                .unwrap();
            assert_same_result(&warm_a, &warm_b);
            assert_eq!(warm_a.product_states, 0);
            reference.merge_global(a.root).unwrap();
            reused.merge_global(b.root).unwrap();
            reference.flush_and_recycle_local_cache().unwrap();
            reused.flush_and_recycle_local_cache().unwrap();
            assert_eq!(reused.global_words, reference.global_words);
            assert_eq!(reused.retained_bytes(), reference.retained_bytes());
        }
    }
    work
}

#[test]
fn supply_draw_reuse_preserves_standard_prefixes_hold_and_storage() {
    use PieceKind::{I, J, L, O, S, T, Z};

    let mut work = (0, 0);
    for (draws, limit, held, enabled, pieces, predecessors) in [
        (4, 0, None, true, vec![I, O, T], vec![0, 0, 0]),
        (4, 19, Some(I), true, vec![I, I, O, T], vec![0, 0, 1, 2]),
        (4, 0, None, false, vec![I, O, T], vec![0, 0, 0]),
        (8, 19, None, true, vec![I, I, O, T, S, Z, J, L], vec![0; 8]),
    ] {
        let source = PatternUniverseMaterializer::standard_7_bag(draws, limit, 97).unwrap();
        let measured = compare(&source, held, enabled, false, &pieces, &predecessors);
        work.0 += measured.0;
        work.1 += measured.1;
    }
    assert!(work.1 < work.0);
    eprintln!(
        "supply_draw_reuse standard cursor advances {} -> {}",
        work.0, work.1
    );
}

#[test]
fn supply_draw_reuse_preserves_projected_complement_and_terminal_hold() {
    use PieceKind::{I, J, O, S, T, Z};

    let mut work = (0, 0);
    for (held, pieces) in [
        (None, vec![I, O, T, S, Z, J]),
        (Some(I), vec![I, I, O, T, S, Z, J]),
    ] {
        let source = PatternUniverseMaterializer::standard_7_bag(6, 0, 97).unwrap();
        let predecessors = vec![0; pieces.len()];
        let measured = compare(&source, held, true, true, &pieces, &predecessors);
        work.0 += measured.0;
        work.1 += measured.1;
    }
    assert!(work.1 < work.0);
    eprintln!(
        "supply_draw_reuse projected cursor advances {} -> {}",
        work.0, work.1
    );
}

#[test]
fn supply_draw_reuse_preserves_scalar_minimum_ordinals_and_work() {
    use PieceKind::{I, J, L, O, S, T, Z};

    let control = ExecutionControl::default();
    let mut work = (0, 0);
    let mut saw_witness = false;
    let mut saw_empty = false;
    for (draws, limit, held, enabled, projected, pieces) in [
        (1, 0, None, false, false, vec![O]),
        (
            1,
            1,
            None,
            false,
            false,
            vec![PieceKind::STANDARD_TETROMINOES[6]],
        ),
        (4, 0, None, true, false, vec![O, I, T]),
        (4, 19, Some(I), true, false, vec![O, I, I, T]),
        (6, 0, None, true, true, vec![O, I, T, S, Z, J]),
        (6, 0, Some(I), true, true, vec![O, I, I, T, S, Z, J]),
        (7, 0, Some(I), true, true, vec![O, T, S, Z, J, L, I, I]),
    ] {
        let source = PatternUniverseMaterializer::standard_7_bag(draws, limit, 97).unwrap();
        let fixed_expected = (!enabled && !projected).then(|| {
            (0..source.pattern_count())
                .find(|&ordinal| source.sequence_at(ordinal).starts_with(&pieces))
                .map(|ordinal| ordinal as u32)
        });
        let predecessors = (0..pieces.len())
            .map(|depth| if depth == 0 { 0 } else { 1 << (depth - 1) })
            .collect::<Vec<_>>();
        let mut language = PieceOrderLanguageCache::default();
        let root = language
            .compile_poset_language(&pieces, &predecessors, 1 << pieces.len())
            .unwrap()
            .unwrap()
            .root;
        for layout in [
            StandardBagProductMemoLayout::Flat,
            StandardBagProductMemoLayout::StateMajor,
        ] {
            let make = || {
                let mut coverage =
                    StandardBagCoverage::for_universe(&source, initial(held), enabled, projected)
                        .unwrap()
                        .unwrap();
                coverage.product_memo = StandardBagProductMemo::new(
                    layout,
                    ExactU64MemoStorage::Compact,
                    coverage.tables.cursor_transitions.max_source_depth,
                );
                // ACCEPT denotes every original queue of this owner; it is a
                // complete coverage fixture, not a geometry-search shortcut.
                coverage.merge_global(ACCEPT).unwrap();
                assert!(coverage.can_use_first_pattern_witness());
                coverage
            };
            let mut reference = make();
            let mut reused = make();
            let original_words = reference.materialize_global().unwrap().to_owned_words();
            assert_eq!(
                reused.materialize_global().unwrap().to_owned_words(),
                original_words
            );
            let query = |coverage: &mut StandardBagCoverage| {
                let before = coverage.product_work_counts();
                let ordinal = coverage
                    .first_pattern_witness(&language, root, &control)
                    .unwrap();
                let after = coverage.product_work_counts();
                (ordinal, (after.0 - before.0, after.1 - before.1))
            };
            for epoch in 0..2 {
                let (a, old_draws) = measure(DrawReuse::Reference, || query(&mut reference));
                let (b, new_draws) = measure(DrawReuse::Enabled, || query(&mut reused));
                assert_eq!(
                    a, b,
                    "minimum original ordinal and logical work, epoch={epoch}"
                );
                if let Some(expected) = fixed_expected {
                    assert_eq!(a.0, expected);
                }
                saw_witness |= a.0.is_some();
                saw_empty |= a.0.is_none();
                assert!(new_draws <= old_draws, "epoch={epoch}");
                if !enabled {
                    assert_eq!(new_draws, old_draws);
                }
                work.0 += old_draws;
                work.1 += new_draws;
                assert_eq!(reused.retained_bytes(), reference.retained_bytes());
                assert_eq!(reused.local_live_bytes(), reference.local_live_bytes());

                let (warm_a, old_warm_draws) =
                    measure(DrawReuse::Reference, || query(&mut reference));
                let (warm_b, new_warm_draws) = measure(DrawReuse::Enabled, || query(&mut reused));
                assert_eq!(warm_a, (a.0, (0, 0)));
                assert_eq!(warm_b, warm_a);
                assert_eq!((old_warm_draws, new_warm_draws), (0, 0));
                reference.flush_and_recycle_local_cache().unwrap();
                reused.flush_and_recycle_local_cache().unwrap();
                assert_eq!(reused.retained_bytes(), reference.retained_bytes());
                assert_eq!(
                    reference.materialize_global().unwrap().to_owned_words(),
                    original_words
                );
                assert_eq!(
                    reused.materialize_global().unwrap().to_owned_words(),
                    original_words
                );
            }
            assert_eq!(
                reference.memo_accounting().first_witness_mode_transitions,
                1
            );
            assert_eq!(reused.memo_accounting().first_witness_mode_transitions, 1);
        }
    }
    assert!(saw_witness && saw_empty);
    assert!(work.1 < work.0);
    eprintln!(
        "supply_draw_reuse scalar cursor advances {} -> {}",
        work.0, work.1
    );
}
