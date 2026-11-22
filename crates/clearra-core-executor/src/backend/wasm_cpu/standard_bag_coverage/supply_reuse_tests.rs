//! Reusing a certified draw changes work, never source membership or storage.

use std::cell::Cell;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_supply::bag::BagState;
use clearra_supply::hold_automaton::{HoldAutomatonState, SupplyProvenanceId};
use clearra_supply::pattern_universe::{MaterializedPatternUniverse, PatternUniverseMaterializer};
use clearra_supply::piece_source::PieceSourceId;
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

use super::super::compact_exact_u64_map::ExactU64MemoStorage;
use super::super::piece_order_language::PieceOrderLanguageCache;
use super::super::standard_bag_product_memo::{
    StandardBagProductMemo, StandardBagProductMemoLayout,
};
use super::{StandardBagCoverage, StandardBagCoverageResult};

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

fn initial(source: &MaterializedPatternUniverse, held: Option<PieceKind>) -> HoldAutomatonState {
    HoldAutomatonState::new(
        PieceSourceId::new(97),
        0,
        held,
        0,
        if source.complete_factorized_layout().is_some() {
            0
        } else {
            BagState::fresh_standard_7_bag().packed_remainder_key()
        },
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
    for layout in [
        StandardBagProductMemoLayout::Flat,
        StandardBagProductMemoLayout::StateMajor,
    ] {
        let make = || {
            let mut coverage = StandardBagCoverage::for_universe(
                source,
                initial(source, held),
                hold_enabled,
                projected,
            )
            .unwrap()
            .unwrap();
            coverage.product_memo = StandardBagProductMemo::new(
                layout,
                ExactU64MemoStorage::Reference,
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
            work.0 += old_draws;
            work.1 += new_draws;

            // A warm lookup and recycling preserve the same root/union owner.
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
fn supply_draw_reuse_preserves_standard_prefixes_hold_eof_and_storage() {
    use PieceKind::{I, J, O, S, T, Z};

    let mut work = (0, 0);
    for (draws, limit, held, enabled, projected, pieces) in [
        (4, 0, None, true, false, vec![I, O, T]),
        (4, 19, Some(I), true, false, vec![I, I, O, T]),
        (4, 0, None, false, false, vec![I, O, T]),
        (6, 0, None, true, true, vec![I, O, T, S, Z, J]),
        (6, 0, Some(I), true, true, vec![I, I, O, T, S, Z, J]),
    ] {
        let source = PatternUniverseMaterializer::standard_7_bag(draws, limit, 97).unwrap();
        let predecessors = vec![0; pieces.len()];
        let measured = compare(&source, held, enabled, projected, &pieces, &predecessors);
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
fn supply_draw_reuse_preserves_factorized_partial_atoms_and_initial_hold() {
    use PieceKind::{I, L, O, T};

    let mut work = (0, 0);
    for expression in ["P1P4", "P2P3", "[LOI]2P5"] {
        let source = PatternUniverseMaterializer::queue_pattern_expression(
            &QueuePatternExpression::parse(expression, 0).unwrap(),
            97,
        )
        .unwrap();
        assert!(source.complete_factorized_layout().is_some());
        for held in [None, Some(T)] {
            // Two distinct I branches enable different later pieces; their
            // supply draw is shared while their language successors stay apart.
            let pieces = [I, I, O, T, L];
            let measured = compare(&source, held, true, false, &pieces, &[0, 0, 1, 2, 0]);
            work.0 += measured.0;
            work.1 += measured.1;
        }
        let pieces = source.sequence_at(0).to_vec();
        let predecessors = (0..pieces.len())
            .map(|depth| if depth == 0 { 0 } else { 1 << (depth - 1) })
            .collect::<Vec<_>>();
        let measured = compare(&source, None, true, true, &pieces, &predecessors);
        work.0 += measured.0;
        work.1 += measured.1;
    }
    assert!(work.1 < work.0);
    eprintln!(
        "supply_draw_reuse factorized cursor advances {} -> {}",
        work.0, work.1
    );
}
