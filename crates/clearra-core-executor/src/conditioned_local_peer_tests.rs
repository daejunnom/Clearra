use super::*;
use crate::conditioned_local_pack::{
    built_in_local_relation_binding, encode_local_relation_candidate_pack,
    load_local_relation_candidate_pack,
};
use crate::conditioned_local_product::{
    tests::signed_authority, QualifiedLocalRelationPack, LOCAL_RELATION_COMPLETENESS_SCOPE,
};
use crate::conditioned_local_relation::{
    derive_exact_conditioned_local_relation, solver_local_relation_spawn_entries,
};

fn fixture(
    height: u8,
) -> (
    QualifiedLocalRelationPack,
    VerifiedAcceleratorAuthority,
    ConditionedPoseWindow,
    Vec<ConditionedReachabilityEntryPose>,
) {
    let profile = KickTableProfileId::SrsPlus;
    let (ceiling, entries) =
        solver_local_relation_spawn_entries(height, PieceKind::T, profile).unwrap();
    let window = ConditionedPoseWindow {
        min_x: 0,
        max_x: 9,
        min_y: 0,
        max_y: ceiling,
    };
    let records = (0..32)
        .map(|board| {
            derive_exact_conditioned_local_relation(
                10,
                height,
                board,
                PieceKind::T,
                profile,
                window,
                &entries,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let binding = built_in_local_relation_binding(profile).unwrap();
    let bytes = encode_local_relation_candidate_pack(binding, &records).unwrap();
    let candidate = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
    let authority = signed_authority(
        &candidate,
        LOCAL_RELATION_COMPLETENESS_SCOPE,
        candidate.encoded_identity(),
    );
    (
        QualifiedLocalRelationPack::qualify(candidate, &authority).unwrap(),
        authority,
        window,
        entries,
    )
}

fn context(
    peer: &QualifiedRelationPeer,
    height: u8,
    window: ConditionedPoseWindow,
    entries: &[ConditionedReachabilityEntryPose],
) -> PreparedPeerContext {
    peer.prepare_context(
        10,
        height,
        LocalRelationRowFrame::new(height, 0).unwrap(),
        PieceKind::T,
        KickTableProfileId::SrsPlus,
        window,
        entries,
    )
    .unwrap()
}

fn answer(owner: &QualifiedLocalRelationPack, peer: &QualifiedRelationPeer) -> bool {
    let Some(query) = peer.drain().unwrap() else {
        return false;
    };
    let queries = wire::decode_queries(&query, PeerIdentity::owner(owner)).unwrap();
    peer.import(&wire::encode_reply(owner, &queries).unwrap())
        .unwrap();
    true
}

#[test]
fn conditioned_peer_batched_records_equal_full_owner_on_one_to_six_lines() {
    for height in 1..=6 {
        let (owner, authority, window, entries) = fixture(height);
        let seed = wire::encode_seed(&owner).unwrap();
        let peer = QualifiedRelationPeer::from_trusted_seed(
            &seed,
            &authority,
            MIN_RELATION_PEER_RESERVED_BYTES,
        )
        .unwrap();
        let prepared = context(&peer, height, window, &entries);
        // Query misses are immediate: no query drain or reply is needed for
        // the baseline exact path to continue. Then populate a bounded batch.
        let mut queued = 0;
        for board in 0..32 {
            if matches!(peer.lookup(board, prepared), Err(ProviderStatus::Miss)) {
                queued += 1;
            }
        }
        assert!(queued > 0);
        assert!(answer(&owner, &peer));
        assert!(peer.drain().unwrap().is_none());
        let full = owner
            .prepare_context(
                10,
                height,
                LocalRelationRowFrame::new(height, 0).unwrap(),
                PieceKind::T,
                KickTableProfileId::SrsPlus,
                window,
                &entries,
            )
            .unwrap();
        for board in 0..32 {
            let baseline = owner
                .lookup_prepared_record_for_proven_entries(board, full)
                .unwrap();
            let cached = peer
                .lookup(board, prepared)
                .unwrap_or_else(|error| panic!("height={height} board={board}: {error:?}"));
            assert_eq!(
                cached.grounded_lock_anchors(),
                baseline.grounded_lock_anchors()
            );
            assert_eq!(cached.exits(), baseline.exits());
            // This full-height relation is closed, so no exit-composition
            // difference can hide a missing global lock in the peer cache.
            assert!(cached.exits().is_empty());
            assert_eq!(
                cached.grounded_lock_anchors(),
                crate::backend::exact_entry_lock_anchors(
                    10,
                    height,
                    board,
                    PieceKind::T,
                    KickTableProfileId::SrsPlus,
                    &entries
                )
                .unwrap()
            );
        }
        let state = peer.state.lock().unwrap();
        assert!(state.bytes <= peer.cache_budget);
        assert!(state.records.len() <= MAX_RECORDS);
    }
}

#[test]
fn conditioned_peer_miss_remains_miss_and_queues_are_bounded_without_a_reply() {
    let (owner, authority, window, entries) = fixture(2);
    let peer = QualifiedRelationPeer::from_trusted_seed(
        &wire::encode_seed(&owner).unwrap(),
        &authority,
        MIN_RELATION_PEER_RESERVED_BYTES,
    )
    .unwrap();
    let prepared = context(&peer, 2, window, &entries);
    for board in 32..2048 {
        let _ = peer.lookup(board, prepared);
    }
    let query = peer.drain().unwrap().unwrap();
    assert!(peer.drain().unwrap().is_none()); // no duplicate in-flight batch
    for board in 2048..4096 {
        let _ = peer.lookup(board, prepared);
    }
    {
        let state = peer.state.lock().unwrap();
        assert!(state.pending.len() <= wire::MAX_RELATION_PEER_BATCH);
        assert!(state.sent.len() <= wire::MAX_RELATION_PEER_BATCH);
        assert_eq!(state.records.len(), 0);
    }
    let queries = wire::decode_queries(&query, PeerIdentity::owner(&owner)).unwrap();
    peer.import(&wire::encode_reply(&owner, &queries).unwrap())
        .unwrap();
    let memoized_miss = peer.state.lock().unwrap().misses.first().copied().unwrap();
    assert!(matches!(
        peer.lookup(memoized_miss.board, prepared),
        Err(ProviderStatus::Miss)
    ));
    assert!(!peer.state.lock().unwrap().pending.contains(&memoized_miss));
    assert!(matches!(
        peer.lookup(1 << 20, prepared),
        Err(ProviderStatus::OutOfScope)
    ));
}

#[test]
fn conditioned_peer_seed_requires_matching_signature_identity_and_budget() {
    let (owner, authority, window, entries) = fixture(2);
    let seed = wire::encode_seed(&owner).unwrap();
    assert!(matches!(
        QualifiedRelationPeer::from_trusted_seed(&seed, &authority, 1024),
        Err(LocalRelationPeerError::Budget)
    ));
    for offset in [
        0,
        8,
        12,
        16,
        20,
        24,
        28,
        60,
        92,
        124,
        156,
        188,
        200,
        seed.len() - 1,
    ] {
        let mut bad = seed.clone();
        bad[offset] ^= 1;
        assert!(
            QualifiedRelationPeer::from_trusted_seed(
                &bad,
                &authority,
                MIN_RELATION_PEER_RESERVED_BYTES
            )
            .is_err(),
            "offset={offset}"
        );
    }
    let peer = QualifiedRelationPeer::from_trusted_seed(
        &seed,
        &authority,
        MIN_RELATION_PEER_RESERVED_BYTES,
    )
    .unwrap();
    assert!(matches!(
        peer.prepare_context(
            10,
            2,
            LocalRelationRowFrame::new(2, 1).unwrap(),
            PieceKind::T,
            KickTableProfileId::SrsPlus,
            window,
            &entries
        ),
        Err(ProviderStatus::Miss)
    ));
    assert!(matches!(
        peer.prepare_context(
            10,
            2,
            LocalRelationRowFrame::new(2, 0).unwrap(),
            PieceKind::T,
            KickTableProfileId::SrsX,
            window,
            &entries
        ),
        Err(ProviderStatus::OutOfScope)
    ));
}

#[test]
fn conditioned_peer_corrupt_late_reply_poisons_previously_used_cache() {
    let (owner, authority, window, entries) = fixture(2);
    let peer = QualifiedRelationPeer::from_trusted_seed(
        &wire::encode_seed(&owner).unwrap(),
        &authority,
        MIN_RELATION_PEER_RESERVED_BYTES,
    )
    .unwrap();
    let prepared = context(&peer, 2, window, &entries);
    for board in 0..32 {
        let _ = peer.lookup(board, prepared);
    }
    assert!(answer(&owner, &peer));
    assert!(peer.lookup(7, prepared).is_ok());
    for board in 32..512 {
        let _ = peer.lookup(board, prepared);
    }
    let query = peer.drain().unwrap().unwrap();
    let queries = wire::decode_queries(&query, PeerIdentity::owner(&owner)).unwrap();
    let mut reply = wire::encode_reply(&owner, &queries).unwrap();
    let last = reply.len() - 1;
    reply[last] ^= 1;
    assert_eq!(
        peer.import(&reply),
        Err(LocalRelationPeerError::InvalidWire)
    );
    assert!(matches!(
        peer.lookup(7, prepared),
        Err(ProviderStatus::InvalidAsset)
    ));
    assert!(peer.drain().is_err());
}

#[test]
fn conditioned_peer_unknown_context_and_stale_reply_are_not_accepted() {
    let (owner, authority, window, entries) = fixture(2);
    let peer = QualifiedRelationPeer::from_trusted_seed(
        &wire::encode_seed(&owner).unwrap(),
        &authority,
        MIN_RELATION_PEER_RESERVED_BYTES,
    )
    .unwrap();
    assert!(wire::encode_reply(
        &owner,
        &[PeerQuery {
            context: u32::MAX,
            board: 0
        }]
    )
    .is_err());
    let prepared = context(&peer, 2, window, &entries);
    for board in 0..32 {
        let _ = peer.lookup(board, prepared);
    }
    let query = peer.drain().unwrap().unwrap();
    let queries = wire::decode_queries(&query, PeerIdentity::owner(&owner)).unwrap();
    let reply = wire::encode_reply(&owner, &queries).unwrap();
    peer.import(&reply).unwrap();
    assert_eq!(peer.import(&reply), Err(LocalRelationPeerError::InFlight));
    assert!(matches!(
        peer.lookup(7, prepared),
        Err(ProviderStatus::InvalidAsset)
    ));
}

#[test]
fn conditioned_peer_structural_validation_is_not_only_a_checksum_check() {
    use sha2::{Digest, Sha256};
    let (owner, authority, window, entries) = fixture(2);
    let repair_checksum = |wire: &mut Vec<u8>| {
        let end = wire.len() - 32;
        let digest = Sha256::digest(&wire[..end]);
        wire[end..].copy_from_slice(&digest);
    };
    let mut seed = wire::encode_seed(&owner).unwrap();
    seed[192..200].copy_from_slice(&u64::MAX.to_le_bytes());
    repair_checksum(&mut seed);
    assert!(matches!(
        QualifiedRelationPeer::from_trusted_seed(
            &seed,
            &authority,
            MIN_RELATION_PEER_RESERVED_BYTES
        ),
        Err(LocalRelationPeerError::InvalidWire)
    ));
    let identity = PeerIdentity::owner(&owner);
    let query = PeerQuery {
        context: 0,
        board: 1,
    };
    let repeated = wire::encode_queries(identity, &[query, query]).unwrap();
    assert!(matches!(
        wire::decode_queries(&repeated, identity),
        Err(LocalRelationPeerError::InvalidWire)
    ));
    let peer = QualifiedRelationPeer::from_trusted_seed(
        &wire::encode_seed(&owner).unwrap(),
        &authority,
        MIN_RELATION_PEER_RESERVED_BYTES,
    )
    .unwrap();
    let prepared = context(&peer, 2, window, &entries);
    for board in 0..32 {
        let _ = peer.lookup(board, prepared);
    }
    let queries = wire::decode_queries(&peer.drain().unwrap().unwrap(), identity).unwrap();
    let mut reply = wire::encode_reply(&owner, &queries).unwrap();
    reply[192..196].copy_from_slice(&99_u32.to_le_bytes());
    repair_checksum(&mut reply);
    assert_eq!(peer.import(&reply), Err(LocalRelationPeerError::InFlight));
    assert!(matches!(
        peer.lookup(7, prepared),
        Err(ProviderStatus::InvalidAsset)
    ));
}
