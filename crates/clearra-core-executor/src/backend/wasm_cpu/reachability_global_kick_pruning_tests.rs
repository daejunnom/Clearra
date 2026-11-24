//! The reverse index can omit a rotation only when the ordered transition is
//! impossible on every board. Forward kick order and scoring witnesses retain
//! their original authority. The scalar board enumeration below is independent
//! of the symbolic predicate and the integration oracle reads raw rule data.

use super::*;
use crate::reachability_reference::reference_spawn_lock_anchors;

const PROFILES: [KickTableProfileId; 5] = [
    KickTableProfileId::Srs90,
    KickTableProfileId::SrsPlus,
    KickTableProfileId::SrsX,
    KickTableProfileId::Jstris180,
    KickTableProfileId::NoKick,
];

#[test]
fn ordered_edge_cube_matches_exhaustive_occupancies_including_sky_and_invalid_poses() {
    const MASKS: [u64; 9] = [0, 1, 2, 3, 4, 5, 6, 7, INVALID_STATE_MASK];
    for source in MASKS {
        for target in MASKS {
            for earlier in MASKS {
                for first in MASKS {
                    for fixed in 0..8 {
                        for variable in 0..8 {
                            let expected = (0..8).any(|subset| {
                                if subset & !variable != 0 {
                                    return false;
                                }
                                let board = fixed | subset;
                                let free = |mask| mask != INVALID_STATE_MASK && mask & board == 0;
                                free(source) && free(target) && !free(first) && !free(earlier)
                            });
                            assert_eq!(
                                edge_possible_in_cube(source, target, [first, earlier], fixed, variable),
                                expected,
                                "source={source:x} target={target:x} prior={first:x},{earlier:x} fixed={fixed:x} variable={variable:x}"
                            );
                        }
                    }
                }
            }
        }
    }
}

fn pair(
    piece: PieceKind,
    profile: KickTableProfileId,
) -> (ReachabilityTemplate, ReachabilityTemplate) {
    let make = || ReachabilityTemplate::compile(6, 4, piece, profile);
    (
        with_global_kick_pruning(false, make),
        with_global_kick_pruning(true, make),
    )
}

#[test]
fn global_kick_pruning_preserves_forward_authority_and_boolean_first_discovery() {
    let mut omitted = 0;
    let mut reached = 0;
    let mut unreachable = 0;
    for profile in PROFILES {
        for piece in PieceKind::STANDARD_TETROMINOES {
            let (before, after) = pair(piece, profile);
            assert_eq!(before.state_masks, after.state_masks);
            assert_eq!(
                before.rotation_target_offsets,
                after.rotation_target_offsets
            );
            assert_eq!(before.rotation_targets, after.rotation_targets);
            assert_eq!(before.rotation_kick_indices, after.rotation_kick_indices);
            assert_eq!(before.translation_targets, after.translation_targets);
            assert_eq!(before.sky_seeds, after.sky_seeds);
            omitted += before.reverse_rotation_sources.len() - after.reverse_rotation_sources.len();
            let mut legacy = ReachabilityScratch::default();
            let mut pruned = ReachabilityScratch::default();
            for board in [0, 0x20, 0x1f020, 0x0f020, 0x2156a, 0x0f24f] {
                let reference = reference_spawn_lock_anchors(6, 4, board, piece, profile).unwrap();
                for rotation in RotationState::ALL {
                    for y in 0..4 {
                        for x in 0..6 {
                            let old =
                                reverse_lock_reachable(&before, board, rotation, x, y, &mut legacy);
                            let new =
                                reverse_lock_reachable(&after, board, rotation, x, y, &mut pruned);
                            assert_eq!(
                                (old.reachable, old.visited_state_count),
                                (new.reachable, new.visited_state_count)
                            );
                            assert_eq!(legacy.queue, pruned.queue);
                            assert_eq!(legacy.visited_generations, pruned.visited_generations);
                            assert_eq!(
                                new.reachable,
                                reference[rotation.quarter_turns() as usize] & (1 << (y * 6 + x))
                                    != 0
                            );
                            reached += usize::from(new.reachable);
                            unreachable += usize::from(!new.reachable);
                        }
                    }
                }
            }
        }
    }
    assert!(omitted > 0);
    assert!(reached > 0 && unreachable > 0);
}

fn scoring_pair<const LEGACY: bool>(
    before: &ReachabilityTemplate,
    after: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
) {
    let mut first = ReachabilityScratch::default();
    let mut second = ReachabilityScratch::default();
    assert_eq!(
        preferred_scoring_lock_evidence::<LEGACY>(before, board, rotation, x, y, &mut first),
        preferred_scoring_lock_evidence::<LEGACY>(after, board, rotation, x, y, &mut second),
        "board={board:x}, rotation={rotation:?}, ({x},{y}), legacy={LEGACY}"
    );
}

#[test]
fn global_kick_pruning_preserves_exact_final_rotation_evidence_for_both_rankings() {
    for profile in PROFILES {
        for piece in PieceKind::STANDARD_TETROMINOES {
            let (before, after) = pair(piece, profile);
            for board in [0, 0x1f020, 0x0f24f] {
                for rotation in RotationState::ALL {
                    for y in 0..4 {
                        for x in 0..6 {
                            scoring_pair::<false>(&before, &after, board, rotation, x, y);
                            scoring_pair::<true>(&before, &after, board, rotation, x, y);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn imported_shared_templates_pin_exact_order_and_count_authority_once() {
    use super::super::compact_kick_authority::CompactKickAuthority;
    use clearra_rules::kicks::{
        KickOffsetSequence, KickTableEntry, KickTableProfile, SrsKicks, VerifiedKickTableProfile,
    };

    let base = SrsKicks::profile();
    let canonical = VerifiedKickTableProfile::try_new(base.clone()).unwrap();
    let mut entries = base.entries().to_vec();
    let selected = entries
        .iter_mut()
        .find(|entry| {
            entry.transition()
                == KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero)
        })
        .unwrap();
    let mut offsets = selected.sequence().offsets().to_vec();
    offsets[..=2].rotate_right(1);
    *selected = KickTableEntry::new(selected.transition(), KickOffsetSequence::new(offsets));
    let changed = VerifiedKickTableProfile::try_new(KickTableProfile::new(
        base.id(),
        base.source_rule(),
        entries,
    ))
    .unwrap();
    let admit = |profile| CompactKickAuthority::admit_local_order_variant(10, 2, profile).unwrap();
    let canonical = admit(&canonical);
    let changed = admit(&changed);
    assert_ne!(canonical, changed);
    let owner = Arc::new(SharedReachabilityTemplates::with_authority(changed.clone()));
    assert!(owner.matches(10, 2, base.id(), Some(&changed)));
    assert!(!owner.matches(10, 2, base.id(), Some(&canonical)));
    assert!(!owner.matches(10, 2, base.id(), None));
    assert!(!owner.matches(10, 3, base.id(), Some(&changed)));
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(10, 2, 0, 0).unwrap();
    let mut first = ReachabilityWorkspace::default();
    let mut second = ReachabilityWorkspace::default();
    first.set_shared_templates(Arc::clone(&owner)).unwrap();
    second.set_shared_templates(Arc::clone(&owner)).unwrap();
    assert_eq!(first.private_retained_bytes(), 0);
    assert_eq!(first.retained_bytes(), changed.retained_bytes());
    first.configure_kick_profile(KickTableProfileId::SrsPlus, true);
    assert_eq!(first.kick_profile_id, base.id());
    first.prepare_template(&catalog, PieceKind::T);
    second.prepare_template(&catalog, PieceKind::T);
    let index = piece_index(PieceKind::T);
    assert!(Arc::ptr_eq(
        first.templates[index].as_ref().unwrap(),
        second.templates[index].as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        first.templates[index].as_ref().unwrap(),
        &owner.template(PieceKind::T)
    ));
    assert_eq!(first.private_retained_bytes(), 0);
    assert_eq!(second.private_retained_bytes(), 0);
    assert_eq!(
        first.retained_bytes(),
        changed.retained_bytes() + owner.template(PieceKind::T).retained_bytes()
    );
    assert!(first.bind_local_import(canonical, &catalog).is_err());
    assert!(first.set_shared_templates(Arc::clone(&owner)).is_err());
    // A pinned built-in relation snapshot is already a different authority,
    // even before a template or query cache has been populated.
    let mut configured = ReachabilityWorkspace::default();
    configured.configure_kick_profile(base.id(), true);
    assert!(configured.set_shared_templates(Arc::clone(&owner)).is_err());
    assert!(!configured.has_local_import());
}
