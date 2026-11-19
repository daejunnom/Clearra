//! Differential qualification of conditional whole-lock certificates.
//! Single-cell perturbations exercise the Boolean reuse condition directly;
//! the catalog itself always starts with a tetromino-aligned fill domain.

use super::*;
use crate::reachability_reference::reference_spawn_lock_anchors;

const PROFILES: [KickTableProfileId; 5] = [
    KickTableProfileId::Srs90,
    KickTableProfileId::SrsPlus,
    KickTableProfileId::SrsX,
    KickTableProfileId::Jstris180,
    KickTableProfileId::NoKick,
];

fn full_catalog(height: u8, initial: u64) -> GeometryCatalog {
    let full = (1_u64 << (10 * height)) - 1;
    assert_eq!((full & !initial).count_ones() % 4, 0);
    GeometryCatalog::compile_for_required_cells_on_dimensions(10, height, initial, full & !initial)
        .expect("aligned finite geometry catalog")
}

#[test]
fn every_profile_and_piece_preserves_whole_locks_under_each_outside_bit_change() {
    const HEIGHT: u8 = 6;
    let source_board = (0x1ff_u64 << 30) | (7_u64 << 7);
    let full = (1_u64 << (10 * HEIGHT)) - 1;
    let (summary, metrics) = placement_dependency::with_mode(true, || {
        let catalog = full_catalog(HEIGHT, source_board);
        let frame = LocalRelationRowFrame::new(HEIGHT, 0).unwrap();
        let mut changed_boards = 0_u64;
        let mut cases_with_outside_cells = 0_usize;
        let mut full_mask_cases = Vec::new();
        for profile in PROFILES {
            for piece in PieceKind::STANDARD_TETROMINOES {
                let template = ReachabilityTemplate::compile(10, HEIGHT, piece, profile);
                let mut cache = PlacementDependencyCache::default();
                let mut scratch = ReachabilityScratch::default();
                let (_, generated_states) = cache
                    .query(
                        &catalog,
                        &template,
                        source_board,
                        piece,
                        profile,
                        RotationState::Zero,
                        2,
                        4,
                        Some(frame),
                        &mut scratch,
                    )
                    .expect("complete source certificate");
                assert!(generated_states > 0);
                assert_eq!(cache.relations.len(), 1);
                let source_locks = cache.relations[0].locks.anchors;
                assert_eq!(
                    source_locks,
                    reference_spawn_lock_anchors(10, HEIGHT, source_board, piece, profile)
                        .expect("independent source-board oracle"),
                    "source {profile:?} {piece:?}"
                );
                let mask = cache.relations[0].mask;
                assert_eq!(mask & !full, 0);
                let mut outside = full & !mask;
                if outside == 0 {
                    // A conservative mask may legitimately cover everything.
                    // Record this case; do not count it as a reuse experiment.
                    full_mask_cases.push((profile, piece));
                    continue;
                }
                cases_with_outside_cells += 1;
                while outside != 0 {
                    let bit = 1_u64 << outside.trailing_zeros();
                    outside &= outside - 1;
                    let changed = source_board ^ bit;
                    let independent =
                        reference_spawn_lock_anchors(10, HEIGHT, changed, piece, profile)
                            .expect("independent changed-board oracle");
                    assert_eq!(
                        source_locks, independent,
                        "whole family {profile:?} {piece:?} bit={bit:#x} mask={mask:#x}"
                    );

                    // A different target from the source query prevents its
                    // target-specific static condition from answering this.
                    // This must be a real cross-board whole-family reuse.
                    let (observed, visited) = cache
                        .query(
                            &catalog,
                            &template,
                            changed,
                            piece,
                            profile,
                            RotationState::Right,
                            0,
                            0,
                            Some(frame),
                            &mut scratch,
                        )
                        .unwrap();
                    assert_eq!(
                        observed,
                        independent[RotationState::Right.quarter_turns() as usize] & 1 != 0,
                        "cache query {profile:?} {piece:?} bit={bit:#x}"
                    );
                    assert_eq!(visited, 0, "outside-mask reuse must bypass the BFS");
                    assert_eq!(cache.relations.len(), 1);
                    changed_boards += 1;
                }
            }
        }
        assert_eq!(cases_with_outside_cells + full_mask_cases.len(), 5 * 7);
        assert!(cases_with_outside_cells > 0);
        assert!(changed_boards > 0);
        (changed_boards, cases_with_outside_cells, full_mask_cases)
    });
    assert_eq!(metrics.generated_relations, 5 * 7);
    assert_eq!(metrics.reused_relations, summary.0);
    assert_eq!(metrics.cross_board_reuses, summary.0);
    assert_eq!(metrics.static_reuses, 0);
    eprintln!(
        "whole-family differential: {} outside-bit boards, {} nonvacuous profile/piece cases, full-mask cases={:?}",
        summary.0, summary.1, summary.2
    );
}

#[test]
fn a_full_relation_cache_computes_exact_positive_and_negative_misses() {
    let (_, metrics) = placement_dependency::with_mode(true, || {
        // A fixed four-cell target leaves one canonical O placement. Repeating
        // its exact record saturates the relation-count boundary without also
        // exhausting the independent influencing-placement allocation budget.
        let required = 3_u64 | (3_u64 << 10);
        let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, required)
            .expect("one-placement aligned catalog");
        let profile = KickTableProfileId::SrsPlus;
        let piece = PieceKind::O;
        let template = ReachabilityTemplate::compile(10, 4, piece, profile);
        let frame = LocalRelationRowFrame::new(4, 0).unwrap();
        let mut cache = PlacementDependencyCache::default();
        let mut scratch = ReachabilityScratch::default();
        assert!(
            cache
                .query(
                    &catalog,
                    &template,
                    0,
                    piece,
                    profile,
                    RotationState::Zero,
                    0,
                    0,
                    Some(frame),
                    &mut scratch,
                )
                .unwrap()
                .0
        );
        assert_eq!(cache.relations.len(), 1);
        while cache.relations.len() < MAX_RELATIONS {
            let source = &cache.relations[0];
            let copy = Relation {
                piece: source.piece,
                frame: source.frame,
                scope: source.scope,
                source_board: source.source_board,
                mask: source.mask,
                occupancy: source.occupancy,
                locks: source.locks,
                target_condition: source.target_condition,
                influencing_placements: source.influencing_placements.clone(),
                next_in_bucket: source.next_in_bucket,
            };
            cache.influence_ids += copy.influencing_placements.capacity();
            cache.relations.push(copy);
        }
        assert_eq!(cache.relations.len(), MAX_RELATIONS);
        assert!(cache.influence_ids < MAX_INFLUENCE_IDS);

        // Both changes touch the existing whole-family condition. Querying a
        // different rotation also rules out the seed's target-specific proof.
        for (board, expected) in [(1_u64 << 2, true), (1, false)] {
            assert_ne!(board & cache.relations[0].mask, 0);
            let oracle = reference_spawn_lock_anchors(10, 4, board, piece, profile).unwrap();
            assert_eq!(
                oracle[RotationState::Right.quarter_turns() as usize] & 1 != 0,
                expected
            );
            let (actual, visited) = cache
                .query(
                    &catalog,
                    &template,
                    board,
                    piece,
                    profile,
                    RotationState::Right,
                    0,
                    0,
                    Some(frame),
                    &mut scratch,
                )
                .unwrap();
            assert_eq!(actual, expected);
            assert!(
                visited > 0,
                "capacity saturation must use the exact primitive"
            );
            assert_eq!(cache.relations.len(), MAX_RELATIONS);
        }
    });
    assert_eq!(
        metrics.generated_relations, 1,
        "a saturated cache must not compile unused proofs"
    );
    assert_eq!(metrics.reused_relations, 0);
}

#[test]
fn a_partial_influence_budget_stops_observations_but_preserves_exact_queries() {
    placement_dependency::with_mode(true, || {
        // Use actual four-line catalog incidence allocations, not a synthetic
        // near-limit counter. Each complete empty-board O proof touches every
        // target cell, so its vector retains four entries per canonical row
        // before deduplication. Repeating valid observations fills the budget
        // well before the relation-count limit, with a nonzero remainder.
        let catalog = full_catalog(4, 0);
        let profile = KickTableProfileId::SrsPlus;
        let piece = PieceKind::O;
        let template = ReachabilityTemplate::compile(10, 4, piece, profile);
        let frame = LocalRelationRowFrame::new(4, 0).unwrap();
        let mut cache = PlacementDependencyCache::default();
        let mut scratch = ReachabilityScratch::default();
        let result = search_reachable_locks(&template, 0, &mut scratch, None);
        assert!(result.exhaustive);
        for _ in 0..=MAX_RELATIONS {
            cache.observe_full(
                &catalog,
                &template,
                0,
                piece,
                profile,
                RotationState::Zero,
                0,
                0,
                Some(frame),
                &scratch,
                &result,
            );
            if cache.observation_admission_closed {
                break;
            }
        }
        assert!(cache.observation_admission_closed);
        assert!(cache.relations.len() < MAX_RELATIONS);
        let row_count = catalog.placement_conditions().unwrap().row_count();
        let proof_capacity = cache.relations[0].influencing_placements.capacity();
        assert_eq!(cache.relations[0].mask, (1_u64 << 40) - 1);
        assert_eq!(proof_capacity, 4 * row_count);
        assert_eq!(cache.influence_ids, cache.relations.len() * proof_capacity);
        let remainder = MAX_INFLUENCE_IDS - cache.influence_ids;
        assert!(remainder > 0 && remainder < proof_capacity);
        assert!(cache.retained_bytes() <= PlacementDependencyCache::retained_upper_bound());
        eprintln!(
            "empty 4L partial influence budget: rows={row_count}, proof_capacity={proof_capacity}, retained_relations={}, retained_ids={}, remainder={remainder}",
            cache.relations.len(),
            cache.influence_ids
        );
        let retained_relations = cache.relations.len();
        let retained_static_masks = cache.static_masks.len();
        let (_, closed_metrics) = placement_dependency::with_mode(true, || {
            assert_eq!(
                cache
                    .query(
                        &catalog,
                        &template,
                        0,
                        piece,
                        profile,
                        RotationState::Zero,
                        0,
                        0,
                        Some(frame),
                        &mut scratch,
                    )
                    .unwrap(),
                (true, 0),
                "existing certificates remain available after admission closes"
            );
            for (board, expected) in [(1_u64 << 2, true), (1, false)] {
                let oracle = reference_spawn_lock_anchors(10, 4, board, piece, profile).unwrap();
                assert_eq!(oracle[1] & 1 != 0, expected);
                let (actual, visited) = cache
                    .query(
                        &catalog,
                        &template,
                        board,
                        piece,
                        profile,
                        RotationState::Right,
                        0,
                        0,
                        Some(frame),
                        &mut scratch,
                    )
                    .unwrap();
                assert_eq!(actual, expected);
                assert!(visited > 0, "a proof miss still uses the exact primitive");
                let reverse = reverse_lock_reachable(
                    &template,
                    board,
                    RotationState::Right,
                    0,
                    0,
                    &mut scratch,
                );
                assert_eq!(reverse.reachable, expected);
                cache.observe_reverse(
                    &catalog,
                    &template,
                    board,
                    piece,
                    profile,
                    RotationState::Right,
                    0,
                    0,
                    Some(frame),
                    &scratch,
                    &reverse,
                );
            }
        });
        assert_eq!(closed_metrics.generated_relations, 0);
        assert_eq!(closed_metrics.full_relations, 0);
        assert_eq!(closed_metrics.reverse_relations, 0);
        assert_eq!(closed_metrics.static_masks_compiled, 0);
        assert_eq!(closed_metrics.reused_relations, 1);
        assert_eq!(cache.relations.len(), retained_relations);
        assert_eq!(cache.static_masks.len(), retained_static_masks);

        // An equal-shaped new catalog has a distinct owner. Admission must
        // reset before the first new observation can consult the old latch.
        let replacement = full_catalog(4, 0);
        let (_, visited) = cache
            .query(
                &replacement,
                &template,
                0,
                piece,
                profile,
                RotationState::Zero,
                0,
                0,
                Some(frame),
                &mut scratch,
            )
            .unwrap();
        assert!(visited > 0);
        assert!(!cache.observation_admission_closed);
        assert_eq!(cache.relations.len(), 1);
        assert_eq!(cache.influence_ids, proof_capacity);

        let result = search_reachable_locks(&template, 0, &mut scratch, None);
        for _ in 0..=MAX_RELATIONS {
            cache.observe_full(
                &replacement,
                &template,
                0,
                piece,
                profile,
                RotationState::Zero,
                0,
                0,
                Some(frame),
                &scratch,
                &result,
            );
            if cache.observation_admission_closed {
                break;
            }
        }
        assert!(cache.observation_admission_closed);
        let next_profile = KickTableProfileId::NoKick;
        let next_template = ReachabilityTemplate::compile(10, 4, piece, next_profile);
        let (actual, visited) = cache
            .query(
                &replacement,
                &next_template,
                0,
                piece,
                next_profile,
                RotationState::Zero,
                0,
                0,
                Some(frame),
                &mut scratch,
            )
            .unwrap();
        assert!(actual && visited > 0);
        assert!(!cache.observation_admission_closed);
        assert_eq!(cache.relations.len(), 1);
        assert_eq!(cache.profile, Some(next_profile));
    });
}

#[test]
fn switching_profiles_invalidates_certificates_before_the_next_query() {
    let (_, metrics) = placement_dependency::with_mode(true, || {
        let catalog = full_catalog(4, 0);
        let piece = PieceKind::T;
        let frame = LocalRelationRowFrame::new(4, 0).unwrap();
        let mut cache = PlacementDependencyCache::default();
        let mut scratch = ReachabilityScratch::default();
        for profile in PROFILES {
            let template = ReachabilityTemplate::compile(10, 4, piece, profile);
            let oracle = reference_spawn_lock_anchors(10, 4, 0, piece, profile).unwrap();
            let (first, visited) = cache
                .query(
                    &catalog,
                    &template,
                    0,
                    piece,
                    profile,
                    RotationState::Zero,
                    0,
                    0,
                    Some(frame),
                    &mut scratch,
                )
                .unwrap();
            assert!(
                visited > 0,
                "old-profile proof must be discarded: {profile:?}"
            );
            assert_eq!(cache.profile, Some(profile));
            assert_eq!(cache.relations.len(), 1);
            assert_eq!(cache.relations[0].locks.anchors, oracle);
            assert_eq!(first, oracle[0] & 1 != 0);

            let (repeated, visited) = cache
                .query(
                    &catalog,
                    &template,
                    0,
                    piece,
                    profile,
                    RotationState::Zero,
                    0,
                    0,
                    Some(frame),
                    &mut scratch,
                )
                .unwrap();
            assert_eq!(repeated, first);
            assert_eq!(visited, 0, "same-profile proof remains reusable");
        }
    });
    assert_eq!(metrics.generated_relations, 5);
    assert_eq!(metrics.reused_relations, 5);
    assert_eq!(metrics.cross_board_reuses, 0);
}
