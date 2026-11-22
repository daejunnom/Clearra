use clearra_supply::{
    execution_automaton::{SupplyObservationIdentity, SupplyProvenanceId},
    pattern_universe::PatternUniverseMaterializer,
    piece_source::PieceSourceId,
    queue::queue_pattern_expression::QueuePatternExpression,
};

use super::*;

fn universe(expression: &str) -> MaterializedPatternUniverse {
    PatternUniverseMaterializer::queue_pattern_expression(
        &QueuePatternExpression::parse(expression, 0).unwrap(),
        83,
    )
    .unwrap()
}

fn initial(held: Option<PieceKind>) -> HoldAutomatonState {
    HoldAutomatonState::with_contract(
        PieceSourceId::new(83),
        PieceSourceKind::MaterializedPatternUniverse,
        0,
        held,
        HoldPolicy::Allowed,
        0,
        0,
        SupplyObservationIdentity::full_queue_oracle(),
        SupplyProvenanceId(83),
    )
}

fn owner(source: &MaterializedPatternUniverse) -> StandardBagCoverage {
    StandardBagCoverage::for_universe(source, initial(None), true, true)
        .unwrap()
        .unwrap()
}

// Exact original queue sets, not physical-positive fixtures: first independent
// draw I, followed by one of the selected first-bag draws, then every suffix.
fn first_i_with_bag_head(coverage: &mut StandardBagCoverage, heads: u8) -> u32 {
    let mut suffixes = [REJECT; 7];
    for (piece, child) in suffixes.iter_mut().enumerate() {
        if heads & (1 << piece) != 0 {
            *child = ACCEPT;
        }
    }
    let tail = coverage
        .intern_node(1, FULL_STANDARD_BAG, suffixes)
        .unwrap();
    let mut first = [REJECT; 7];
    first[0] = tail;
    coverage.intern_node(0, FULL_STANDARD_BAG, first).unwrap()
}

#[test]
fn first_draw_blocks_count_exact_new_bits_across_overlapping_epochs() {
    let source = universe("P1P7P7");
    let mut coverage = owner(&source);
    let variants = 3_628_800_u32; // 6! * 7! after fixing two original draws.
    let root = first_i_with_bag_head(&mut coverage, 0b001_1111);
    coverage.merge_global(root).unwrap();
    let epoch = coverage.first_draw_block_snapshot().unwrap().unwrap();
    assert_eq!(epoch.epoch_counts, [5 * variants, 0, 0, 0, 0, 0, 0]);
    assert_eq!(epoch.flushed_counts, [0; 7]);
    assert_eq!(epoch.known_complete_mask, 0);
    coverage.flush_and_recycle_local_cache().unwrap();
    let flushed = coverage.first_draw_block_snapshot().unwrap().unwrap();
    assert_eq!(flushed.flushed_counts, epoch.epoch_counts);
    assert_eq!(flushed.epoch_counts, [0; 7]);

    // The counts' arithmetic sum exceeds a block, but their UNION still lacks
    // bag-head L. A sum/saturation-based certificate would be false here.
    let overlapping = first_i_with_bag_head(&mut coverage, 0b011_0110);
    coverage.merge_global(overlapping).unwrap();
    let overlap = coverage.first_draw_block_snapshot().unwrap().unwrap();
    assert!(overlap.flushed_counts[0] + overlap.epoch_counts[0] > overlap.block_pattern_count);
    assert_eq!(overlap.known_complete_mask, 0);
    coverage.flush_and_recycle_local_cache().unwrap();
    let overlap = coverage.first_draw_block_snapshot().unwrap().unwrap();
    assert_eq!(overlap.flushed_counts[0], 6 * variants);
    assert_eq!(overlap.known_complete_mask, 0);

    // Neither half is complete before flushing, but their exact union is.
    let last = first_i_with_bag_head(&mut coverage, 0b100_0001);
    coverage.merge_global(last).unwrap();
    assert_eq!(
        coverage
            .first_draw_block_snapshot()
            .unwrap()
            .unwrap()
            .known_complete_mask,
        0
    );
    coverage.flush_and_recycle_local_cache().unwrap();
    let complete = coverage.first_draw_block_snapshot().unwrap().unwrap();
    assert_eq!(complete.flushed_counts, [25_401_600, 0, 0, 0, 0, 0, 0]);
    assert_eq!(complete.flushed_complete_mask, 1);
    assert_eq!(complete.epoch_complete_mask, 0);
    assert_eq!(complete.known_complete_mask, 1);
    assert!(!coverage.global_is_complete());
    assert_eq!(coverage.global_covered_pattern_count, 25_401_600);
    assert!(coverage.global_words[..396_900]
        .iter()
        .all(|word| *word == u64::MAX));
    assert!(coverage.global_words[396_900..]
        .iter()
        .all(|word| *word == 0));

    // A complete epoch child is a constant-time independent certificate. The
    // already flushed bits must not be counted again after arena recycling.
    let duplicate = first_i_with_bag_head(&mut coverage, FULL_STANDARD_BAG);
    coverage.merge_global(duplicate).unwrap();
    let duplicate = coverage.first_draw_block_snapshot().unwrap().unwrap();
    assert_eq!(duplicate.epoch_complete_mask, 1);
    assert_eq!(duplicate.known_complete_mask, 1);
    coverage.flush_and_recycle_local_cache().unwrap();
    assert_eq!(coverage.global_covered_pattern_count, 25_401_600);

    coverage.merge_global(ACCEPT).unwrap();
    assert!(coverage.global_is_complete());
    assert_eq!(
        coverage
            .first_draw_block_snapshot()
            .unwrap()
            .unwrap()
            .epoch_complete_mask,
        0x7f
    );
    coverage.flush_and_recycle_local_cache().unwrap();
    let all = coverage.first_draw_block_snapshot().unwrap().unwrap();
    assert_eq!(all.flushed_counts, [25_401_600; 7]);
    assert_eq!(all.flushed_complete_mask, 0x7f);
    assert_eq!(all.epoch_counts, [0; 7]);
    assert_eq!(all.known_complete_mask, 0x7f);
    assert_eq!(coverage.global_covered_pattern_count, 177_811_200);
    assert!(coverage.global_is_complete());
    assert!(coverage.global_words.iter().all(|word| *word == u64::MAX));
}

#[test]
fn first_draw_blocks_bind_actual_order_and_reset_with_the_coverage_owner() {
    for expression in ["P1P7P7", "[LOITSZJ]1P7P7"] {
        let source = universe(expression);
        let coverage = owner(&source);
        let snapshot = coverage.first_draw_block_snapshot().unwrap().unwrap();
        let expected: Vec<_> = source
            .complete_factorized_layout()
            .unwrap()
            .segments()
            .next()
            .unwrap()
            .choices()
            .iter()
            .map(|piece| piece_code(*piece) - 1)
            .collect();
        assert_eq!(snapshot.first_draw_order.as_slice(), expected);
        assert_eq!(snapshot.block_pattern_count, 25_401_600);
        assert_eq!(snapshot.flushed_counts, [0; 7]);
        assert_eq!(snapshot.epoch_counts, [0; 7]);
        for (rank, piece_index) in snapshot.first_draw_order.into_iter().enumerate() {
            let queue = source.sequence_at(rank * 25_401_600);
            assert_eq!(piece_code(queue[0]) - 1, piece_index);
        }
        assert!(
            coverage.retained_bytes()
                >= coverage.shared_request_retained_bytes()
                    + StandardBagCoverage::inline_source_state_bytes()
        );
        assert!(
            StandardBagCoverage::checked_creation_future_bytes(&source).unwrap()
                >= (coverage.shared_request_retained_bytes()
                    + StandardBagCoverage::inline_source_state_bytes()) as u128
        );
        let changed_hold =
            StandardBagCoverage::for_universe(&source, initial(Some(PieceKind::T)), true, true)
                .unwrap()
                .unwrap();
        let changed = changed_hold.first_draw_block_snapshot().unwrap().unwrap();
        assert_eq!(changed.flushed_counts, [0; 7]);
        assert_eq!(changed.epoch_counts, [0; 7]);
        assert_eq!(changed.known_complete_mask, 0);
    }
    for expression in ["P7P1P7", "P1P7P6", "P7P7"] {
        let coverage = owner(&universe(expression));
        assert!(coverage.first_draw_block_snapshot().unwrap().is_none());
    }
}
