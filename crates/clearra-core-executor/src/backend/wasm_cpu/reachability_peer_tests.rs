//! SRP rationale: peer records must compose in the REAL BuildUp workspace,
//! not just equal an owner's isolated lookup. No global registry, timing,
//! benchmark policy, implicit network I/O or qualification promotion occurs.

use super::*;
use crate::conditioned_local_pack::{
    built_in_local_relation_binding, coalesce_identical_local_relation_records,
    encode_local_relation_candidate_pack, load_local_relation_candidate_pack,
};
use crate::conditioned_local_peer::{solver_test_support as transport, QualifiedRelationPeer};
use crate::conditioned_local_product::{
    tests::signed_authority, QualifiedLocalRelationPack, LOCAL_RELATION_COMPLETENESS_SCOPE,
};
use crate::conditioned_local_relation::{
    derive_exact_conditioned_local_relation, ConditionedPoseWindow,
};
use crate::reachability_reference::reference_spawn_lock_anchors;

fn workspace(profile: KickTableProfileId, source: RelationSource) -> ReachabilityWorkspace {
    let mut workspace = ReachabilityWorkspace::default();
    workspace.configure_kick_profile(profile, false);
    // Own and pin the explicit test source, avoiding contention with unrelated
    // registry/policy tests. Admission/signature checks still happen upstream.
    workspace.conditioned = Some(source);
    workspace.conditioned_enabled = Some(true);
    workspace.conditioned_policy_enabled = Some(true);
    workspace.metrics.conditioned_requested = true;
    workspace.metrics.conditioned_policy_enabled = true;
    workspace.metrics.conditioned_snapshot_active = true;
    workspace
}

#[allow(clippy::too_many_arguments)]
fn query(
    workspace: &mut ReachabilityWorkspace,
    height: u8,
    board: u64,
    frame: LocalRelationRowFrame,
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
) -> bool {
    // This is immutable dimension-only scaffolding, not an asset registry.
    // Build it once rather than regenerate Geometry for every cache assertion.
    static CATALOGS: [OnceLock<GeometryCatalog>; 6] = [const { OnceLock::new() }; 6];
    let catalog = CATALOGS[usize::from(height - 1)].get_or_init(|| {
        GeometryCatalog::compile_for_required_cells_on_dimensions(10, height, 0, 0).unwrap()
    });
    workspace.prepare_template(catalog, piece);
    workspace.lock_reachable_after_harddrop_miss_in_frame(
        catalog,
        board,
        piece,
        rotation,
        x,
        y,
        Some(frame),
    )
}

fn assert_full_family(
    profile: KickTableProfileId,
    height: u8,
    board: u64,
    frame: LocalRelationRowFrame,
    piece: PieceKind,
    source: RelationSource,
    boolean_shortcuts: bool,
) -> ReachabilityMetrics {
    let reference = reference_spawn_lock_anchors(10, height, board, piece, profile).unwrap();
    let mut workspace = workspace(profile, source);
    workspace.configure_conditioned_boolean_shortcuts(boolean_shortcuts);
    // First-time dimension setup resets the workspace cache. Complete that
    // real initialization BEFORE requesting an exhaustive observation.
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(10, height, 0, 0).unwrap();
    workspace.prepare_template(&catalog, piece);
    // Explicitly request an exhaustive cache fill through the existing
    // observation policy; no private substitute traversal is used.
    workspace.cache.insert(
        board,
        piece,
        ReachableLocks::default(),
        ReachableLocks::default(),
        false,
        true,
    );
    let slot = workspace
        .cache
        .keys
        .iter()
        .position(|key| key.valid && key.board == board)
        .unwrap();
    workspace.cache.keys[slot].observation = u8::MAX;
    let got = query(
        &mut workspace,
        height,
        board,
        frame,
        piece,
        RotationState::Zero,
        0,
        0,
    );
    assert_eq!(got, reference[0] & 1 != 0);
    assert!(workspace.cache.keys[slot].exhaustive);
    assert_eq!(
        workspace.cache.locks[slot].anchors,
        reference,
        "{profile:?} {height}L {piece:?} {board:#x} deleted={}",
        frame.deleted_original_rows()
    );
    // Both positive and negative lookups now come from the SAME exact cache.
    for rotation in RotationState::ALL {
        for y in 0..height as i8 {
            for x in 0..10_i8 {
                assert_eq!(
                    query(&mut workspace, height, board, frame, piece, rotation, x, y),
                    reference[rotation.quarter_turns() as usize]
                        & (1_u64 << (y as usize * 10 + x as usize))
                        != 0,
                );
            }
        }
    }
    workspace.metrics()
}

fn fixture(
    height: u8,
    sky: bool,
    piece: PieceKind,
) -> (
    Arc<QualifiedLocalRelationPack>,
    clearra_accelerator_activation::VerifiedAcceleratorAuthority,
) {
    let profile = KickTableProfileId::SrsPlus;
    let template = ReachabilityTemplate::compile(10, height, piece, profile);
    let entries = canonical_sky_entry_poses(10, template.ceiling, &template.sky_seeds);
    let window = ConditionedPoseWindow {
        min_x: 0,
        max_x: 9,
        min_y: if sky { height as i8 } else { 0 },
        max_y: template.ceiling,
    };
    // The open sky fixture needs one canonical occupancy condition. A list
    // of independently derived overlapping conditions is NOT a pack cover.
    let boards = if sky { 0..1 } else { 0..32 };
    let mut records = boards
        .map(|board| {
            derive_exact_conditioned_local_relation(
                10, height, board, piece, profile, window, &entries,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    coalesce_identical_local_relation_records(&mut records).unwrap();
    let binding = built_in_local_relation_binding(profile).unwrap();
    let bytes = encode_local_relation_candidate_pack(binding, &records).unwrap();
    let candidate = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
    let authority = signed_authority(
        &candidate,
        LOCAL_RELATION_COMPLETENESS_SCOPE,
        candidate.encoded_identity(),
    );
    (
        Arc::new(QualifiedLocalRelationPack::qualify(candidate, &authority).unwrap()),
        authority,
    )
}

#[test]
fn v081_conditioned_peer_cold_miss_then_warm_closed_family_matches_primitive() {
    let profile = KickTableProfileId::SrsPlus;
    for height in 1..=6 {
        let (owner, authority) = fixture(height, false, PieceKind::T);
        let peer = transport::peer(&owner, &authority);
        let frame = LocalRelationRowFrame::new(height, 0).unwrap();
        let mut misses = 0;
        for board in 0..32 {
            let metrics = assert_full_family(
                profile,
                height,
                board,
                frame,
                PieceKind::T,
                RelationSource::Peer(Arc::clone(&peer)),
                true,
            );
            misses += metrics.conditioned_misses;
        }
        assert!(
            misses > 0,
            "cold misses must use exact fallback without waiting"
        );
        assert!(transport::exchange(&owner, &peer, false).unwrap());
        for board in [0, 1, 17, 31] {
            for shortcuts in [false, true] {
                let metrics = assert_full_family(
                    profile,
                    height,
                    board,
                    frame,
                    PieceKind::T,
                    RelationSource::Peer(Arc::clone(&peer)),
                    shortcuts,
                );
                assert!(metrics.conditioned_complete_hits > 0);
                assert_eq!(metrics.conditioned_misses, 0);
                assert_full_family(
                    profile,
                    height,
                    board,
                    frame,
                    PieceKind::T,
                    RelationSource::Full(Arc::clone(&owner)),
                    shortcuts,
                );
            }
        }
    }
}

#[test]
fn v081_conditioned_peer_boolean_positive_does_not_authorize_missing_locks() {
    let profile = KickTableProfileId::SrsPlus;
    let (owner, authority) = fixture(4, true, PieceKind::O);
    let peer = transport::peer(&owner, &authority);
    let frame = LocalRelationRowFrame::new(4, 0).unwrap();
    let mut workspace = workspace(profile, RelationSource::Peer(Arc::clone(&peer)));
    assert!(query(
        &mut workspace,
        4,
        0,
        frame,
        PieceKind::O,
        RotationState::Zero,
        4,
        0
    ));
    assert!(workspace.metrics().conditioned_complete_hits > 0);
    let slot = workspace
        .cache
        .keys
        .iter()
        .position(|key| key.valid)
        .unwrap();
    assert!(!workspace.cache.keys[slot].exhaustive);
    let reference = reference_spawn_lock_anchors(10, 4, 0, PieceKind::O, profile).unwrap();
    let (rotation, anchor) = reference
        .iter()
        .zip(workspace.cache.locks[slot].anchors)
        .enumerate()
        .find_map(|(rotation, (expected, returned))| {
            let missing = expected & !returned;
            (missing != 0).then_some((RotationState::ALL[rotation], missing.trailing_zeros()))
        })
        .expect("one Boolean proof must leave other targets unknown");
    assert!(query(
        &mut workspace,
        4,
        0,
        frame,
        PieceKind::O,
        rotation,
        (anchor % 10) as i8,
        (anchor / 10) as i8
    ));
    for shortcuts in [false, true] {
        assert_full_family(
            profile,
            4,
            0,
            frame,
            PieceKind::O,
            RelationSource::Peer(Arc::clone(&peer)),
            shortcuts,
        );
    }
}

#[test]
fn v081_conditioned_peer_missing_frame_and_late_corruption_fall_back_exactly() {
    let profile = KickTableProfileId::SrsPlus;
    let (owner, authority) = fixture(2, false, PieceKind::T);
    let peer = transport::peer(&owner, &authority);
    // The pack has no deleted-row context. This must NOT prove impossibility.
    let metrics = assert_full_family(
        profile,
        2,
        1,
        LocalRelationRowFrame::new(2, 1).unwrap(),
        PieceKind::T,
        RelationSource::Peer(Arc::clone(&peer)),
        true,
    );
    assert!(metrics.conditioned_misses > 0);
    assert_eq!(metrics.conditioned_complete_hits, 0);
    let frame = LocalRelationRowFrame::new(2, 0).unwrap();
    for board in 0..32 {
        let mut cold = workspace(profile, RelationSource::Peer(Arc::clone(&peer)));
        query(
            &mut cold,
            2,
            board,
            frame,
            PieceKind::T,
            RotationState::Zero,
            0,
            0,
        );
    }
    assert!(transport::exchange(&owner, &peer, false).unwrap());
    let warmed = assert_full_family(
        profile,
        2,
        1,
        frame,
        PieceKind::T,
        RelationSource::Peer(Arc::clone(&peer)),
        true,
    );
    assert!(warmed.conditioned_complete_hits > 0);
    // A previously consumed good reply does not excuse a later corrupt one.
    let mut pending = workspace(profile, RelationSource::Peer(Arc::clone(&peer)));
    query(
        &mut pending,
        2,
        255,
        frame,
        PieceKind::T,
        RotationState::Zero,
        0,
        0,
    );
    assert!(transport::exchange(&owner, &peer, true).is_err());
    let invalid = assert_full_family(
        profile,
        2,
        1,
        frame,
        PieceKind::T,
        RelationSource::Peer(Arc::clone(&peer)),
        true,
    );
    assert!(invalid.conditioned_invalid_asset > 0);
    assert_eq!(invalid.conditioned_complete_hits, 0);
    // Whole-result invalidation after this error belongs to the worker host;
    // cached answers in an already-discarded session are not release evidence.
}

#[test]
#[ignore = "requires explicit download of the five catalog-bound immutable packs"]
fn v081_signed_conditioned_pack_owner_peer_solver_smoke() {
    use clearra_accelerator_product_host::{
        embedded_catalog, CatalogProfileStatus, ProductCatalogKind,
    };
    let directory = std::env::var_os("CLEARRA_SIGNED_CONDITIONED_SMOKE_DIR")
        .map(std::path::PathBuf::from)
        .expect("set the explicit downloaded-pack directory");
    let catalog = embedded_catalog(ProductCatalogKind::BoardConditionedReachability).unwrap();
    for (profile, name) in [
        (KickTableProfileId::Srs90, "srs"),
        (KickTableProfileId::SrsPlus, "srs-plus"),
        (KickTableProfileId::SrsX, "srs-x"),
        (KickTableProfileId::Jstris180, "jstris-180"),
        (KickTableProfileId::NoKick, "no-kick"),
    ] {
        let CatalogProfileStatus::Qualified(asset) = catalog.profile(name).unwrap() else {
            panic!("{name}: current catalog must qualify the smoke asset");
        };
        let bytes = std::fs::read(directory.join(format!("conditioned-{name}.cllr"))).unwrap();
        assert_eq!(bytes.len() as u64, asset.authority().payload_bytes());
        let candidate = load_local_relation_candidate_pack(
            &bytes,
            built_in_local_relation_binding(profile).unwrap(),
            None,
        )
        .unwrap();
        let owner =
            Arc::new(QualifiedLocalRelationPack::qualify(candidate, asset.authority()).unwrap());
        assert!(owner.accounted_bytes() as u64 <= asset.metadata().active_session_shared_bytes());
        let peer: Arc<QualifiedRelationPeer> = transport::peer(&owner, asset.authority());
        for height in 1..=6 {
            let frames: &[u16] = if height == 2 { &[0, 1, 2] } else { &[0] };
            for piece in [
                PieceKind::I,
                PieceKind::O,
                PieceKind::T,
                PieceKind::S,
                PieceKind::Z,
                PieceKind::J,
                PieceKind::L,
            ] {
                for &deleted in frames {
                    let frame = LocalRelationRowFrame::new(height, deleted).unwrap();
                    for board in [0, 17] {
                        assert_full_family(
                            profile,
                            height,
                            board,
                            frame,
                            piece,
                            RelationSource::Full(Arc::clone(&owner)),
                            true,
                        );
                        // A cold miss must still complete before any reply.
                        assert_full_family(
                            profile,
                            height,
                            board,
                            frame,
                            piece,
                            RelationSource::Peer(Arc::clone(&peer)),
                            true,
                        );
                        while transport::exchange(&owner, &peer, false).unwrap() {}
                        for shortcuts in [false, true] {
                            let metrics = assert_full_family(
                                profile,
                                height,
                                board,
                                frame,
                                piece,
                                RelationSource::Peer(Arc::clone(&peer)),
                                shortcuts,
                            );
                            assert!(
                                metrics.conditioned_complete_hits > 0,
                                "{name} {height}L {piece:?} board={board} deleted={deleted}"
                            );
                            assert_eq!(metrics.conditioned_misses, 0);
                        }
                    }
                }
            }
        }
    }
}
