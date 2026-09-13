use super::*;
use clearra_core_domain::{
    piece::piece_kind::PieceKind, solution::normalized_tiling_solution::PiecePlacementMask,
};
use core::cell::Cell;

fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

// Canonical identity carriers only: arbitrary four-cell masks are sufficient
// for ordering tests and never claimed to be legal placements or PC evidence.
fn candidates(count: usize) -> Vec<StandardBoard64TilingIdentity> {
    let mut result = Vec::new();
    'outer: for a in 0..20 {
        for b in a + 1..21 {
            for c in b + 1..22 {
                for d in c + 1..23 {
                    if result.len() == count {
                        break 'outer;
                    }
                    let mask = (1u64 << a) | (1u64 << b) | (1u64 << c) | (1u64 << d);
                    let piece = PieceKind::STANDARD_TETROMINOES[result.len() % 7];
                    result.push(
                        StandardBoard64TilingIdentity::from_placements(
                            0,
                            [PiecePlacementMask::new(piece, mask)],
                        )
                        .unwrap(),
                    );
                }
            }
        }
    }
    assert_eq!(result.len(), count);
    result
}

fn begin(values: &[StandardBoard64TilingIdentity]) -> CooperativeCandidateCanonicalizer {
    CooperativeCandidateCanonicalizer::begin(values.iter().copied().collect(), nz(8 * 1024 * 1024))
        .unwrap()
}

fn finish(owner: &mut CooperativeCandidateCanonicalizer, quantum: usize) {
    for _ in 0..1_000_000 {
        let before = owner.work_done();
        let done = owner.advance(nz(quantum), &|| false).unwrap();
        assert!(owner.work_done() - before <= quantum);
        if done {
            return;
        }
    }
    panic!("canonicalization must finish within the fixture budget");
}

#[test]
fn pc4_compact_graph_union_canonicalization_matches_bulk_order_and_digest_at_every_quantum() {
    for count in [
        0, 1, 2, 3, 4, 5, 7, 8, 9, 16, 31, 32, 33, 127, 255, 256, 257, 1023, 1024, 1025,
    ] {
        let mut expected = candidates(count);
        expected.sort_unstable();
        let expected_digest = PcCandidateSetDigest::calculate(&expected).unwrap();
        for quantum in [1, 2, 7, 64, 1024] {
            let mut owner = begin(&expected);
            finish(&mut owner, quantum);
            let (actual, digest) = owner.into_parts().unwrap();
            assert_eq!(actual, expected, "count={count} quantum={quantum}");
            assert_eq!(digest, expected_digest);
        }
    }
}

#[test]
fn pc4_compact_graph_union_canonicalization_limits_both_live_candidate_buffers() {
    let values = candidates(257);
    let input: HashSet<_> = values.iter().copied().collect();
    let required = (input.capacity() + input.len()) * size_of::<StandardBoard64TilingIdentity>();
    match CooperativeCandidateCanonicalizer::begin(input, nz(required - 1)) {
        Err(CandidateCanonicalizationError::BufferLimit {
            limit,
            required: actual,
        }) => {
            assert_eq!(limit, required - 1);
            assert_eq!(actual, required);
        }
        _ => panic!("both simultaneously live buffers must be admitted before allocation"),
    }
    let mut discovery = begin(&values);
    let peak = discovery.peak_buffer_bytes();
    finish(&mut discovery, 64);
    assert_eq!(discovery.peak_buffer_bytes(), peak);
    assert!(peak >= values.len() * 2 * size_of::<StandardBoard64TilingIdentity>());
}

#[test]
fn pc4_compact_graph_union_canonicalization_cancels_collection_sort_hash_and_late_completion() {
    let values = candidates(257);
    for phase in 0..5 {
        let mut owner = begin(&values);
        while match (phase, owner.phase) {
            (0, Phase::Collect)
            | (1, Phase::BuildHeap { .. })
            | (2, Phase::Sort { .. })
            | (3, Phase::Hash { .. })
            | (4, Phase::Complete) => false,
            _ => true,
        } {
            owner.advance(nz(1), &|| false).unwrap();
        }
        assert!(matches!(
            owner.advance(nz(1), &|| true),
            Err(CandidateCanonicalizationError::Cancelled)
        ));
        assert!(matches!(
            owner.advance(nz(1), &|| false),
            Err(CandidateCanonicalizationError::Terminated)
        ));
        assert!(owner.into_parts().is_err());
    }
    let mut owner = begin(&values);
    let checks = Cell::new(0);
    assert!(matches!(
        owner.advance(nz(64), &|| {
            checks.set(checks.get() + 1);
            checks.get() == 4
        }),
        Err(CandidateCanonicalizationError::Cancelled)
    ));
    assert_eq!(owner.work_done(), 3);
    assert!(begin(&values).into_parts().is_err());
}

#[test]
fn pc4_compact_graph_union_canonicalization_preserves_version_one_digest_bytes() {
    let single = StandardBoard64TilingIdentity::from_placements(
        0,
        [PiecePlacementMask::new(PieceKind::I, 15)],
    )
    .unwrap();
    // Independent SHA-256 of v1 domain + BE count + BE candidate fields,
    // calculated with Node crypto, not this Rust hashing implementation.
    for (values, hex) in [
        (
            vec![],
            "77efdc03929ed7a3ddf804ac50eb827b096cd8082db478f793f1da0a519c1c68",
        ),
        (
            vec![single],
            "eec28c3245367fedd824a740fc94fd99b90c910201017eb36c6fb134462ccfab",
        ),
    ] {
        let mut owner = begin(&values);
        finish(&mut owner, 1);
        let (_, digest) = owner.into_parts().unwrap();
        let actual = digest
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(actual, hex);
    }
}
