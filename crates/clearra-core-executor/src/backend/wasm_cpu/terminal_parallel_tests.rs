//! Canonical assembly only: synthetic identities are codec-valid disjoint
//! four-cell placements, not claims about a physical Full6 search or its queues.

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::solution::normalized_tiling_solution::{
    normalized_tiling_solution_set_hash_from_sorted_canonical_strings,
    normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
    NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity,
};

use super::{canonical_keys, sort_identities, useful_workers};
use super::{MIN_KEY_ITEMS_PER_WORKER, MIN_SORT_ITEMS_PER_WORKER};

fn identity(ordinal: usize, placement_count: usize) -> StandardBoard64TilingIdentity {
    assert!(placement_count <= 16);
    let occupied = if placement_count == 16 {
        u64::MAX
    } else {
        (1_u64 << (4 * placement_count)) - 1
    };
    let mut digits = ordinal;
    let placements = (0..placement_count).map(|slot| {
        let piece = PieceKind::STANDARD_TETROMINOES[digits % 7];
        digits /= 7;
        PiecePlacementMask::new(piece, 0xf_u64 << (4 * slot))
    });
    StandardBoard64TilingIdentity::from_placements(ordinal as u64 & !occupied, placements)
        .expect("valid exact identity with disjoint four-cell masks")
}

fn identities(count: usize, placement_count: usize) -> Vec<StandardBoard64TilingIdentity> {
    let mut identities = Vec::with_capacity(count + 17);
    // Reversal plus base-seven piece labels supplies deterministic, unsorted
    // identities without a random source or an external geometry producer.
    identities.extend(
        (0..count)
            .rev()
            .map(|ordinal| identity(ordinal, placement_count)),
    );
    identities
}

fn serial_keys(identities: &[StandardBoard64TilingIdentity]) -> Vec<String> {
    let mut keys = Vec::new();
    keys.try_reserve_exact(identities.len()).unwrap();
    keys.extend(identities.iter().copied().map(|identity| {
        NormalizedTilingSolutionKey::from_standard_board64_identity(identity).into_string()
    }));
    keys
}

fn key_payload_capacity(keys: &[String]) -> usize {
    keys.iter().map(String::capacity).sum()
}

#[test]
fn terminal_parallel_worker_budget_keeps_small_and_non_native_work_serial() {
    let native_parallel = cfg!(all(feature = "parallel", not(target_family = "wasm")));
    for minimum in [MIN_SORT_ITEMS_PER_WORKER, MIN_KEY_ITEMS_PER_WORKER] {
        for requested in [0, 1, 2, 8, usize::MAX] {
            for items in [0, 1, minimum - 1, minimum, minimum * 2 - 1] {
                assert_eq!(useful_workers(items, requested, minimum), 1);
            }
            for items in [minimum * 2, minimum * 3 + 1, minimum * 8, minimum * 9 + 1] {
                let actual = useful_workers(items, requested, minimum);
                assert!(actual >= 1);
                assert!(actual <= requested.max(1));
                assert!(actual <= (items / minimum).max(1));
                assert_eq!(
                    actual,
                    if native_parallel {
                        requested.min(items / minimum).max(1)
                    } else {
                        1
                    }
                );
            }
        }
    }

    // Each split assigns disjoint budgets whose sum is the parent's budget.
    // The caller participates in that count; it is not an additional worker.
    fn maximum_live_workers(items: usize, requested: usize, minimum: usize, pivot: bool) -> usize {
        let admitted = useful_workers(items, requested, minimum);
        if admitted <= 1 {
            return 1;
        }
        let left_workers = admitted / 2;
        let middle = items / admitted * left_workers;
        maximum_live_workers(middle, left_workers, minimum, pivot)
            + maximum_live_workers(
                items - middle - usize::from(pivot),
                admitted - left_workers,
                minimum,
                pivot,
            )
    }
    for requested in 1..=9 {
        for (minimum, pivot) in [
            (MIN_SORT_ITEMS_PER_WORKER, true),
            (MIN_KEY_ITEMS_PER_WORKER, false),
        ] {
            for items in [minimum * 2 + 1, minimum * 8 + 3, minimum * 17 + 5] {
                assert!(maximum_live_workers(items, requested, minimum, pivot) <= requested);
            }
        }
    }
}

#[test]
fn terminal_parallel_canonical_sort_and_dedup_retain_the_input_allocation() {
    let mut empty = Vec::<StandardBoard64TilingIdentity>::with_capacity(17);
    let empty_pointer = empty.as_ptr();
    let empty_capacity = empty.capacity();
    sort_identities(&mut empty, 8);
    assert_eq!(empty.as_ptr(), empty_pointer);
    assert_eq!(empty.capacity(), empty_capacity);
    assert!(canonical_keys(&empty, 8).unwrap().is_empty());

    let mut actual = Vec::with_capacity(256);
    for count in (0..=16).rev() {
        for ordinal in [0, 1, 13, 107, 1] {
            actual.push(identity(ordinal, count));
        }
    }
    let original_len = actual.len();
    let pointer = actual.as_ptr();
    let capacity = actual.capacity();
    let mut expected = actual.clone();
    expected.sort_unstable();
    sort_identities(&mut actual, 8);
    assert_eq!(actual, expected);
    assert_eq!(
        actual.len(),
        original_len,
        "sorting does not erase duplicates"
    );
    assert_eq!(actual.as_ptr(), pointer);
    assert_eq!(actual.capacity(), capacity);
    actual.dedup();
    expected.dedup();
    assert_eq!(actual, expected);
    assert!(actual.len() < original_len);
    assert_eq!(actual.as_ptr(), pointer);
    assert_eq!(actual.capacity(), capacity);
}

#[test]
fn terminal_parallel_keys_and_cts1_match_every_supported_placement_count() {
    let mut identities = (0..=16)
        .flat_map(|count| [0, 1, 13, 107].map(move |ordinal| identity(ordinal, count)))
        .collect::<Vec<_>>();
    identities.sort_unstable();
    identities.dedup();
    for count in 0..=16 {
        assert!(identities
            .iter()
            .any(|identity| identity.placement_count() == count));
    }
    let expected = serial_keys(&identities);
    let mut independently_ordered = expected.clone();
    independently_ordered.sort_unstable();
    assert_eq!(expected, independently_ordered);
    let old_hash =
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(&identities);
    assert!(old_hash.starts_with("cts1:"));
    for workers in [0, 1, 2, 8] {
        let actual = canonical_keys(&identities, workers).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual.capacity(), expected.capacity());
        assert_eq!(
            key_payload_capacity(&actual),
            key_payload_capacity(&expected)
        );
        assert_eq!(
            normalized_tiling_solution_set_hash_from_sorted_canonical_strings(&actual),
            old_hash
        );
    }
    assert_eq!(
        normalized_tiling_solution_set_hash_from_sorted_canonical_strings(&[]),
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(&[])
    );
}

#[test]
#[cfg(not(target_family = "wasm"))]
fn terminal_parallel_large_sort_and_keys_match_one_two_and_eight_worker_budgets() {
    let input = identities(MIN_SORT_ITEMS_PER_WORKER * 8 + 257, 15);
    assert!(input.windows(2).any(|pair| pair[0] < pair[1]));
    assert!(input.windows(2).any(|pair| pair[0] > pair[1]));
    if cfg!(feature = "parallel") {
        assert_eq!(useful_workers(input.len(), 8, MIN_SORT_ITEMS_PER_WORKER), 8);
    }
    let mut expected = input.clone();
    expected.sort_unstable();
    assert!(expected.windows(2).all(|pair| pair[0] < pair[1]));
    // Eight real sort partitions need 524,545 identities. Eight key workers
    // need only this prefix; keep the larger string payload out of a unit test.
    let key_count = MIN_KEY_ITEMS_PER_WORKER * 8 + 257;
    let expected_keys = serial_keys(&expected[..key_count]);
    let expected_hash = normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
        &expected[..key_count],
    );
    for workers in [1, 2, 8] {
        let mut actual = Vec::with_capacity(input.capacity());
        actual.extend_from_slice(&input);
        let pointer = actual.as_ptr();
        let capacity = actual.capacity();
        sort_identities(&mut actual, workers);
        assert_eq!(actual, expected);
        assert_eq!(actual.as_ptr(), pointer);
        assert_eq!(actual.capacity(), capacity);
        let keys = canonical_keys(&actual[..key_count], workers).unwrap();
        assert_eq!(keys, expected_keys);
        assert_eq!(keys.capacity(), expected_keys.capacity());
        assert_eq!(
            key_payload_capacity(&keys),
            key_payload_capacity(&expected_keys)
        );
        assert_eq!(
            normalized_tiling_solution_set_hash_from_sorted_canonical_strings(&keys),
            expected_hash
        );
    }
    for reverse in [false, true] {
        let mut ordered = expected.clone();
        if reverse {
            ordered.reverse();
        }
        let pointer = ordered.as_ptr();
        let capacity = ordered.capacity();
        sort_identities(&mut ordered, 8);
        assert_eq!(ordered, expected);
        assert_eq!(ordered.as_ptr(), pointer);
        assert_eq!(ordered.capacity(), capacity);
    }
}

#[test]
#[ignore = "supervised canonical-assembly diagnostic; synthetic identities only, no search"]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn terminal_parallel_synthetic_456923_sort_keys_hash_abba() {
    use std::time::Duration;
    use std::time::Instant;

    #[derive(Clone, Copy, Debug)]
    enum Assembly {
        SerialReference,
        Parallel,
    }

    struct ResultAndTiming {
        identities: Vec<StandardBoard64TilingIdentity>,
        keys: Vec<String>,
        hash: String,
        sort: Duration,
        keys_time: Duration,
        hash_time: Duration,
        total: Duration,
    }

    fn execute(
        input: &[StandardBoard64TilingIdentity],
        capacity: usize,
        strategy: Assembly,
    ) -> ResultAndTiming {
        // Fixture allocation/copy is deliberately outside the measured assembly.
        let mut identities = Vec::with_capacity(capacity);
        identities.extend_from_slice(input);
        let pointer = identities.as_ptr();
        let capacity = identities.capacity();
        let started = Instant::now();
        let phase = Instant::now();
        match strategy {
            Assembly::SerialReference => identities.sort_unstable(),
            Assembly::Parallel => sort_identities(&mut identities, 8),
        }
        identities.dedup();
        let sort = phase.elapsed();
        let (keys, hash, keys_time, hash_time) = match strategy {
            Assembly::SerialReference => {
                // The original order hashes identities before serializing keys.
                let phase = Instant::now();
                let hash =
                    normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                        &identities,
                    );
                let hash_time = phase.elapsed();
                let phase = Instant::now();
                let keys = serial_keys(&identities);
                (keys, hash, phase.elapsed(), hash_time)
            }
            Assembly::Parallel => {
                let phase = Instant::now();
                let keys = canonical_keys(&identities, 8).unwrap();
                let keys_time = phase.elapsed();
                let phase = Instant::now();
                let hash = normalized_tiling_solution_set_hash_from_sorted_canonical_strings(&keys);
                (keys, hash, keys_time, phase.elapsed())
            }
        };
        let total = started.elapsed();
        assert_eq!(identities.as_ptr(), pointer);
        assert_eq!(identities.capacity(), capacity);
        ResultAndTiming {
            identities,
            keys,
            hash,
            sort,
            keys_time,
            hash_time,
            total,
        }
    }

    let input = identities(456_923, 15);
    assert!(input
        .iter()
        .all(|identity| identity.placement_count() == 15));
    let mut reference: Option<ResultAndTiming> = None;
    for (arm, strategy) in [
        ("A1", Assembly::SerialReference),
        ("B1", Assembly::Parallel),
        ("B2", Assembly::Parallel),
        ("A2", Assembly::SerialReference),
    ] {
        let result = execute(&input, input.capacity(), strategy);
        assert_eq!(result.identities.len(), 456_923);
        assert!(result.identities.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(result.keys.len(), result.identities.len());
        if let Some(reference) = &reference {
            assert_eq!(result.identities, reference.identities);
            assert_eq!(result.keys, reference.keys);
            assert_eq!(result.hash, reference.hash);
            assert_eq!(
                result.identities.capacity(),
                reference.identities.capacity()
            );
            assert_eq!(result.keys.capacity(), reference.keys.capacity());
            assert_eq!(
                key_payload_capacity(&result.keys),
                key_payload_capacity(&reference.keys)
            );
        }
        eprintln!(
            "terminal_parallel_abba arm={arm} strategy={strategy:?} identities={} requested_workers={} sort_workers={} key_workers={} sort_us={} keys_us={} hash_us={} total_us={} identity_capacity={} key_capacity={} string_capacity={} hash={}",
            result.identities.len(),
            if matches!(strategy, Assembly::Parallel) { 8 } else { 1 },
            useful_workers(result.identities.len(), if matches!(strategy, Assembly::Parallel) { 8 } else { 1 }, MIN_SORT_ITEMS_PER_WORKER),
            useful_workers(result.identities.len(), if matches!(strategy, Assembly::Parallel) { 8 } else { 1 }, MIN_KEY_ITEMS_PER_WORKER),
            result.sort.as_micros(),
            result.keys_time.as_micros(),
            result.hash_time.as_micros(),
            result.total.as_micros(),
            result.identities.capacity(),
            result.keys.capacity(),
            key_payload_capacity(&result.keys),
            result.hash,
        );
        if reference.is_none() {
            reference = Some(result);
        }
    }
}
