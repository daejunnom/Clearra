use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_rules::kicks::KickTableProfileId;
use sha2::{Digest, Sha256};

use super::{
    built_in_local_relation_binding, encode_local_relation_candidate_pack, generation_identity,
    load_local_relation_candidate_pack, LocalRelationBinding, LocalRelationPackError, HEADER_BYTES,
};
use crate::conditioned_local_index::LocalRelationCandidateLookup;
use crate::conditioned_local_relation::{
    derive_exact_conditioned_local_relation, derive_exact_conditioned_local_relation_with_frame,
    ConditionedPoseWindow, ExactConditionedLocalRelation, LocalRelationRowFrame,
};
use crate::conditioned_reachability::ConditionedReachabilityEntryPose;

fn record(
    profile: KickTableProfileId,
) -> (
    ExactConditionedLocalRelation,
    ConditionedPoseWindow,
    ConditionedReachabilityEntryPose,
) {
    let window = ConditionedPoseWindow {
        min_x: 4,
        max_x: 4,
        min_y: 4,
        max_y: 4,
    };
    let entry = ConditionedReachabilityEntryPose {
        rotation: RotationState::Zero,
        x: 4,
        y: 4,
    };
    let record =
        derive_exact_conditioned_local_relation(10, 4, 0, PieceKind::T, profile, window, &[entry])
            .expect("valid complete-board local relation");
    (record, window, entry)
}

#[test]
fn each_profile_round_trips_in_an_independent_candidate_bundle() {
    for profile in [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ] {
        let binding = built_in_local_relation_binding(profile).unwrap();
        let (record, window, entry) = record(profile);
        let encoded = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
        let generation: [u8; 32] = encoded[80..112].try_into().unwrap();
        let loaded = load_local_relation_candidate_pack(&encoded, binding, Some(generation))
            .expect("same profile and immutable generation");
        assert_eq!(loaded.binding(), binding);
        assert_eq!(loaded.generation_identity(), generation);
        assert_eq!(loaded.encoded_bytes(), encoded.len());
        assert_eq!(loaded.record_count(), 1);
        assert!(matches!(
            loaded.lookup(10, 4, 0, PieceKind::T, profile, window, &[entry]),
            LocalRelationCandidateLookup::Hit(_)
        ));
        let wrong_profile = if profile == KickTableProfileId::NoKick {
            KickTableProfileId::Srs90
        } else {
            KickTableProfileId::NoKick
        };
        assert_eq!(
            loaded.lookup(10, 4, 0, PieceKind::T, wrong_profile, window, &[entry],),
            LocalRelationCandidateLookup::OutOfScope
        );
    }
}

#[test]
fn original_row_correspondence_is_a_distinct_lookup_and_binary_identity() {
    let profile = KickTableProfileId::SrsPlus;
    let (plain, window, entry) = record(profile);
    let frame = LocalRelationRowFrame::new(4, 1 << 2).unwrap();
    let framed = derive_exact_conditioned_local_relation_with_frame(
        10,
        4,
        0,
        frame,
        PieceKind::T,
        profile,
        window,
        &[entry],
    )
    .expect("same physical board with a different original-row frame");
    assert_eq!(
        plain.grounded_lock_anchors(),
        framed.grounded_lock_anchors()
    );
    let binding = built_in_local_relation_binding(profile).unwrap();
    let bytes = encode_local_relation_candidate_pack(binding, &[framed]).unwrap();
    let loaded = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
    assert!(matches!(
        loaded.lookup(10, 4, 0, PieceKind::T, profile, window, &[entry]),
        LocalRelationCandidateLookup::Miss
    ));
    assert!(matches!(
        loaded.lookup_with_frame(10, 4, 0, frame, PieceKind::T, profile, window, &[entry]),
        LocalRelationCandidateLookup::Hit(found) if found.row_frame() == frame
    ));
}

#[test]
fn reused_local_relation_continues_on_the_query_board_not_the_source_board() {
    let profile = KickTableProfileId::SrsPlus;
    let (record, window, entry) = record(profile);
    let reference_entry = crate::reachability_reference::ReferenceReachabilityEntryPose {
        rotation: entry.rotation,
        x: entry.x,
        y: entry.y,
    };
    let reference_source =
        crate::backend::exact_entry_lock_anchors(10, 4, 0, PieceKind::T, profile, &[entry])
            .unwrap();
    assert_eq!(
        record.compose_exact_global_lock_anchors_for_board(0),
        Some(reference_source)
    );
    let binding = built_in_local_relation_binding(profile).unwrap();
    let bytes = encode_local_relation_candidate_pack(binding, &[record.clone()]).unwrap();
    let loaded = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
    let (query_board, expected) = (0..40_u32)
        .filter(|bit| record.dependency_mask() & (1_u64 << bit) == 0)
        .find_map(|bit| {
            let board = 1_u64 << bit;
            let locks = crate::backend::exact_entry_lock_anchors(
                10,
                4,
                board,
                PieceKind::T,
                profile,
                &[entry],
            )?;
            (locks != reference_source).then_some((board, locks))
        })
        .expect("an outside-dependency obstacle changes a global continuation");
    let LocalRelationCandidateLookup::Hit(found) =
        loaded.lookup(10, 4, query_board, PieceKind::T, profile, window, &[entry])
    else {
        panic!("unchanged local dependency cells must reuse the relation");
    };
    assert_ne!(expected, reference_source);
    assert_eq!(
        crate::reachability_reference::reference_entry_lock_anchors(
            10,
            4,
            query_board,
            PieceKind::T,
            profile,
            &[reference_entry],
        ),
        Some(expected),
        "the independent primitive must agree on the changed global path"
    );
    assert_eq!(
        found.compose_exact_global_lock_anchors_for_board(query_board),
        Some(expected)
    );
    assert_eq!(
        found.compose_exact_global_lock_anchors_for_board(1_u64 << 60),
        None,
        "an out-of-domain board must never be composed"
    );
}

#[test]
fn truncation_tampering_snapshot_and_duplicate_signature_are_rejected() {
    let profile = KickTableProfileId::SrsPlus;
    let binding = built_in_local_relation_binding(profile).unwrap();
    let (record, _, _) = record(profile);
    assert!(matches!(
        encode_local_relation_candidate_pack(binding, &[record.clone(), record.clone()]),
        Err(LocalRelationPackError::NonCanonicalOrder)
    ));
    let encoded = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
    let mut prior_schema = encoded.clone();
    prior_schema[..8].copy_from_slice(b"CLLR0001");
    assert!(matches!(
        load_local_relation_candidate_pack(&prior_schema, binding, None),
        Err(LocalRelationPackError::Header)
    ));
    assert!(matches!(
        load_local_relation_candidate_pack(&encoded[..encoded.len() - 1], binding, None),
        Err(LocalRelationPackError::Header)
    ));
    let mut tampered = encoded.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(matches!(
        load_local_relation_candidate_pack(&tampered, binding, None),
        Err(LocalRelationPackError::PayloadDigest)
    ));
    assert!(matches!(
        load_local_relation_candidate_pack(&encoded, binding, Some([0; 32])),
        Err(LocalRelationPackError::SnapshotMismatch)
    ));
    let other_binding = built_in_local_relation_binding(KickTableProfileId::SrsX).unwrap();
    assert!(matches!(
        load_local_relation_candidate_pack(&encoded, other_binding, None),
        Err(LocalRelationPackError::BindingMismatch)
    ));

    // Recompute both digests to prove this is a structural parser failure,
    // not merely a checksum rejection of the altered record.
    let mut malformed = encoded;
    malformed[HEADER_BYTES + 7] = 0x80;
    reseal_candidate(&mut malformed, binding);
    assert!(matches!(
        load_local_relation_candidate_pack(&malformed, binding, None),
        Err(LocalRelationPackError::Record)
    ));
}

fn reseal_candidate(bytes: &mut [u8], binding: LocalRelationBinding) {
    let payload_identity: [u8; 32] = Sha256::digest(&bytes[HEADER_BYTES..]).into();
    bytes[48..80].copy_from_slice(&payload_identity);
    let generation = generation_identity(binding, payload_identity, 1);
    bytes[80..112].copy_from_slice(&generation);
}
