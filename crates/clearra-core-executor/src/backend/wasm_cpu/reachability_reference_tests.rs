//! Product traversal compared with a separate primitive local qualifier.

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_rules::kicks::KickTableProfileId;

use super::super::catalog::{GeometryCatalog, InstantiatedRealization};
use super::{
    no_entry_components::with_mode as with_no_entry_components_mode, search_reachable_locks,
    search_reachable_locks_from_entries, ReachabilityScratch, ReachabilityTemplate,
    ReachabilityWorkspace,
};
use crate::conditioned_local_relation::{
    derive_exact_conditioned_local_relation, ConditionedPoseWindow, LocalRelationRowFrame,
};
use crate::conditioned_reachability::{
    derive_exact_conditioned_entry_lock_anchors, ConditionedReachabilityEntryPose,
};
use crate::reachability_reference::{
    reference_entry_lock_anchors, reference_local_relation, reference_spawn_lock_anchors,
    ReferenceReachabilityEntryPose,
};

// Intentionally specify the physical lock independently of the catalog's
// output: horizontal T on (2,0), (3,0), (4,0), and (3,1).
const ADVERSARIAL_T_LOCK: InstantiatedRealization = InstantiatedRealization {
    lock_mask: 0x201c,
    rotation: RotationState::Zero,
    x: 2,
    lock_y: 0,
};

fn adversarial_t_catalog() -> GeometryCatalog {
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
        10,
        3,
        0,
        ADVERSARIAL_T_LOCK.lock_mask,
    )
    .expect("the physical T lock forms an aligned catalog target");
    let row = catalog
        .skeleton_id(PieceKind::T, ADVERSARIAL_T_LOCK.lock_mask)
        .expect("T skeleton");
    assert!(catalog
        .instantiations(row, 0)
        .any(|actual| actual == ADVERSARIAL_T_LOCK));
    catalog
}

fn reference_has_adversarial_t_lock(anchors: [u64; 4]) -> bool {
    anchors[0] & (1 << 2) != 0
}

#[test]
fn adding_a_blocker_can_enable_a_later_ordered_kick() {
    let profile = KickTableProfileId::Srs90;
    let base = 0x4002;
    let added = base | (1 << 11);
    let reference_entry = ReferenceReachabilityEntryPose {
        rotation: RotationState::Right,
        x: 2,
        y: 0,
    };
    let entry = ConditionedReachabilityEntryPose {
        rotation: reference_entry.rotation,
        x: reference_entry.x,
        y: reference_entry.y,
    };
    let catalog = adversarial_t_catalog();
    let template = ReachabilityTemplate::compile(10, 3, PieceKind::T, profile);
    let mut scratch = ReachabilityScratch::default();
    let mut workspace = ReachabilityWorkspace::default();
    workspace.configure_kick_profile(profile, false);
    for (board, expected) in [(base, false), (added, true)] {
        assert_eq!(board & ADVERSARIAL_T_LOCK.lock_mask, 0);
        // Right-to-Zero first tries anchor (1,1), then (2,1), then (2,0).
        // (4,1) blocks the second; adding (1,1) blocks the first without
        // touching either the source or the target of the third kick.
        let reference =
            reference_entry_lock_anchors(10, 3, board, PieceKind::T, profile, &[reference_entry])
                .expect("entry is legal with and without the added blocker");
        let exact = search_reachable_locks_from_entries(&template, board, &mut scratch, &[entry])
            .expect("entry is legal in the production solver");
        assert!(exact.exhaustive);
        assert_eq!(exact.locks.anchors, reference);
        assert_eq!(reference_has_adversarial_t_lock(reference), expected);
        assert_eq!(
            reference_has_adversarial_t_lock(
                reference_spawn_lock_anchors(10, 3, board, PieceKind::T, profile).unwrap()
            ),
            expected
        );
        for components in [false, true] {
            assert!(
                !with_no_entry_components_mode(components, || {
                    workspace.try_prove_lock_pose_unreachable_at_minimum(
                        &catalog,
                        board,
                        PieceKind::T,
                        profile,
                        ADVERSARIAL_T_LOCK,
                        LocalRelationRowFrame::new(3, 0),
                    )
                }),
                "absence at minimum is not proof on every superset; components={components}"
            );
        }
    }
}

#[test]
fn horizontal_no_entry_components_cover_a_wide_cavity_but_not_its_opening() {
    const FULL_WIDTH: u8 = 10;
    const HEIGHT: u8 = 6;
    let sealed = 0x1ff_u64 << 40;
    // One hole in row four cannot admit an O. Opening column eight
    // makes a two-cell channel connected to the real sky entries.
    let open = sealed & !(1 << 48);
    let profile = KickTableProfileId::NoKick;
    let pose = InstantiatedRealization {
        lock_mask: 0xc03,
        rotation: RotationState::Zero,
        x: 0,
        lock_y: 0,
    };
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
        FULL_WIDTH,
        HEIGHT,
        sealed,
        pose.lock_mask,
    )
    .expect("aligned physical O target");
    let frame = LocalRelationRowFrame::new(HEIGHT, 0);
    let mut workspace = ReachabilityWorkspace::default();
    workspace.configure_kick_profile(profile, false);
    for (board, reachable) in [(sealed, false), (open, true)] {
        let exact =
            reference_spawn_lock_anchors(FULL_WIDTH, HEIGHT, board, PieceKind::O, profile).unwrap();
        assert_eq!(exact[0] & 1 != 0, reachable);
        let mut prove = |components| {
            with_no_entry_components_mode(components, || {
                workspace.try_prove_lock_pose_unreachable_at_minimum(
                    &catalog,
                    board,
                    PieceKind::O,
                    profile,
                    pose,
                    frame,
                )
            })
        };
        assert!(
            !prove(false),
            "the 16-pose limit does not cover this cavity"
        );
        assert_eq!(prove(true), !reachable);
    }
    assert_eq!(workspace.generated_state_count(), 0);
}

#[test]
fn two_blockers_close_the_inlet_only_when_combined() {
    let profile = KickTableProfileId::Srs90;
    let base = 0x10_0800;
    let left = 1 << 12;
    let right = 1 << 14;
    let sealed = base | left | right;
    let catalog = adversarial_t_catalog();
    let mut workspace = ReachabilityWorkspace::default();
    workspace.configure_kick_profile(profile, false);
    for (board, reachable) in [
        (base, true),
        (base | left, true),
        (base | right, true),
        (sealed, false),
    ] {
        assert_eq!(board & ADVERSARIAL_T_LOCK.lock_mask, 0);
        let reference = reference_spawn_lock_anchors(10, 3, board, PieceKind::T, profile).unwrap();
        assert_eq!(reference_has_adversarial_t_lock(reference), reachable);
        assert_eq!(
            workspace.try_prove_lock_pose_unreachable_at_minimum(
                &catalog,
                board,
                PieceKind::T,
                profile,
                ADVERSARIAL_T_LOCK,
                LocalRelationRowFrame::new(3, 0),
            ),
            !reachable,
            "certificate vs scalar ordered-kick oracle on board {board:#x}"
        );
    }

    // The sealed case has a real negative certificate while the target
    // remains collision-free. Check its minimum-board scope independently:
    // each additional legal blocker must preserve that proven absence.
    for cell in 0..30 {
        let added = 1 << cell;
        if (sealed | ADVERSARIAL_T_LOCK.lock_mask) & added == 0 {
            let reference =
                reference_spawn_lock_anchors(10, 3, sealed | added, PieceKind::T, profile).unwrap();
            assert!(!reference_has_adversarial_t_lock(reference), "cell {cell}");
        }
    }
}

#[test]
fn no_entry_certificate_cannot_cross_an_enabled_half_turn_profile() {
    let board = 0x03c4_0840;
    let frame = LocalRelationRowFrame::new(3, 0);
    let catalog = adversarial_t_catalog();
    let mut workspace = ReachabilityWorkspace::default();
    let certifies_no_entry = |workspace: &mut ReachabilityWorkspace, profile| {
        workspace.try_prove_lock_pose_unreachable_at_minimum(
            &catalog,
            board,
            PieceKind::T,
            profile,
            ADVERSARIAL_T_LOCK,
            frame,
        )
    };
    let no_half_turn = KickTableProfileId::Srs90;
    let with_half_turn = KickTableProfileId::SrsX;
    for (profile, expected) in [(no_half_turn, false), (with_half_turn, true)] {
        let reference = reference_spawn_lock_anchors(10, 3, board, PieceKind::T, profile).unwrap();
        assert_eq!(reference_has_adversarial_t_lock(reference), expected);
        let template = ReachabilityTemplate::compile(10, 3, PieceKind::T, profile);
        let result =
            search_reachable_locks(&template, board, &mut ReachabilityScratch::default(), None);
        assert!(result.exhaustive);
        assert_eq!(result.locks.anchors, reference);
    }

    workspace.configure_kick_profile(no_half_turn, false);
    assert!(certifies_no_entry(&mut workspace, no_half_turn));
    assert!(!certifies_no_entry(&mut workspace, with_half_turn));
    workspace.configure_kick_profile(with_half_turn, false);
    assert!(!certifies_no_entry(&mut workspace, with_half_turn));
    workspace.configure_kick_profile(no_half_turn, false);
    assert!(certifies_no_entry(&mut workspace, no_half_turn));
}

fn assert_board_suite_matches_reference(width: u8, height: u8, boards: &[u64]) {
    const PROFILES: [KickTableProfileId; 5] = [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ];
    for profile_id in PROFILES {
        for piece in PieceKind::STANDARD_TETROMINOES {
            let template = ReachabilityTemplate::compile(width, height, piece, profile_id);
            let mut scratch = ReachabilityScratch::default();
            for &board in boards {
                let optimized = search_reachable_locks(&template, board, &mut scratch, None);
                assert!(optimized.exhaustive);
                assert_eq!(
                    optimized.locks.anchors,
                    reference_spawn_lock_anchors(width, height, board, piece, profile_id)
                        .expect("bounded reference domain"),
                    "{profile_id:?} {piece:?} {width}x{height} board={board:#x}"
                );
            }
        }
    }
}

#[test]
fn small_boards_with_zero_one_or_two_obstacles_match_independent_primitive_bfs() {
    const WIDTH: u8 = 4;
    const HEIGHT: u8 = 4;
    let cell_count = usize::from(WIDTH) * usize::from(HEIGHT);
    let mut boards = vec![0];
    for first in 0..cell_count {
        boards.push(1_u64 << first);
        for second in first + 1..cell_count {
            boards.push((1_u64 << first) | (1_u64 << second));
        }
    }
    assert_board_suite_matches_reference(WIDTH, HEIGHT, &boards);
}

#[test]
fn product_width_and_all_supported_heights_match_independent_primitive_bfs() {
    const WIDTH: u8 = 10;
    for height in 1..=6_u8 {
        let top = usize::from(height - 1) * usize::from(WIDTH);
        let boards = [
            0,
            1,
            1_u64 << top,
            (1_u64 << (top + 4)) | (1_u64 << (top + 5)),
            (1_u64 << 1) | (1_u64 << (top + 8)),
        ];
        assert_board_suite_matches_reference(WIDTH, height, &boards);
    }
}

#[test]
fn actual_entry_pose_sets_match_independent_full_board_bfs() {
    const PROFILES: [KickTableProfileId; 5] = [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ];
    for profile in PROFILES {
        for piece in PieceKind::STANDARD_TETROMINOES {
            let template = ReachabilityTemplate::compile(10, 4, piece, profile);
            let mut scratch = ReachabilityScratch::default();
            for board in [0, 1, (1_u64 << 7) | (1_u64 << 18)] {
                let entry = ConditionedReachabilityEntryPose {
                    rotation: RotationState::Zero,
                    x: 4,
                    y: 4,
                };
                let optimized =
                    search_reachable_locks_from_entries(&template, board, &mut scratch, &[entry])
                        .expect("sky entry is valid");
                let independent = reference_entry_lock_anchors(
                    10,
                    4,
                    board,
                    piece,
                    profile,
                    &[ReferenceReachabilityEntryPose {
                        rotation: entry.rotation,
                        x: entry.x,
                        y: entry.y,
                    }],
                )
                .expect("bounded reference domain");
                assert!(optimized.exhaustive);
                assert_eq!(
                    optimized.locks.anchors, independent,
                    "{profile:?} {piece:?} {board:#x}"
                );
                assert_eq!(
                    derive_exact_conditioned_entry_lock_anchors(
                        10,
                        4,
                        board,
                        piece,
                        profile,
                        &[entry]
                    ),
                    Some(independent),
                    "public entry relation: {profile:?} {piece:?} {board:#x}"
                );
            }
        }
    }
}

#[test]
fn invalid_entry_cannot_become_a_complete_negative() {
    let template = ReachabilityTemplate::compile(10, 4, PieceKind::T, KickTableProfileId::SrsPlus);
    let mut scratch = ReachabilityScratch::default();
    assert!(search_reachable_locks_from_entries(
        &template,
        0,
        &mut scratch,
        &[ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: -1,
            y: 4,
        }],
    )
    .is_none());
    assert_eq!(
        derive_exact_conditioned_entry_lock_anchors(
            10,
            4,
            0,
            PieceKind::T,
            KickTableProfileId::SrsPlus,
            &[]
        ),
        None
    );
    assert_eq!(
        derive_exact_conditioned_entry_lock_anchors(
            10,
            4,
            0,
            PieceKind::T,
            KickTableProfileId::SrsPlus,
            &[ConditionedReachabilityEntryPose {
                rotation: RotationState::Zero,
                x: -1,
                y: 4,
            }]
        ),
        None
    );
}

#[test]
fn multiple_actual_entries_match_the_independent_relation() {
    let entries = [
        ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 3,
            y: 4,
        },
        ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 5,
            y: 4,
        },
    ];
    let reference_entries = entries.map(|entry| ReferenceReachabilityEntryPose {
        rotation: entry.rotation,
        x: entry.x,
        y: entry.y,
    });
    for profile in [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ] {
        let board = (1_u64 << 2) | (1_u64 << 17);
        let expected =
            reference_entry_lock_anchors(10, 4, board, PieceKind::T, profile, &reference_entries)
                .expect("both entries are valid on this complete board");
        assert_eq!(
            derive_exact_conditioned_entry_lock_anchors(
                10,
                4,
                board,
                PieceKind::T,
                profile,
                &entries,
            ),
            Some(expected),
            "{profile:?}"
        );
    }
}

#[test]
fn local_relation_exits_preserve_paths_outside_the_window() {
    let entry = ConditionedReachabilityEntryPose {
        rotation: RotationState::Zero,
        x: 4,
        y: 4,
    };
    let reference_entry = ReferenceReachabilityEntryPose {
        rotation: entry.rotation,
        x: entry.x,
        y: entry.y,
    };
    let window = ConditionedPoseWindow {
        min_x: 4,
        max_x: 4,
        min_y: 4,
        max_y: 4,
    };
    for profile in [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ] {
        for board in [0, 1_u64 << 1, (1_u64 << 1) | (1_u64 << 27)] {
            let local = derive_exact_conditioned_local_relation(
                10,
                4,
                board,
                PieceKind::T,
                profile,
                window,
                &[entry],
            )
            .expect("bounded full-board local relation");
            let independent_local = reference_local_relation(
                10,
                4,
                board,
                PieceKind::T,
                profile,
                window,
                &[reference_entry],
            )
            .expect("independent local relation");
            assert_eq!(
                local.grounded_lock_anchors(),
                independent_local.grounded_lock_anchors,
                "local locks: {profile:?} board={board:#x}"
            );
            let independent_exits: Vec<_> = independent_local
                .exits
                .iter()
                .map(|exit| ConditionedReachabilityEntryPose {
                    rotation: exit.rotation,
                    x: exit.x,
                    y: exit.y,
                })
                .collect();
            assert_eq!(
                local.exits(),
                independent_exits.as_slice(),
                "first exits: {profile:?} board={board:#x}"
            );
            assert!(!local.exits.is_empty(), "{profile:?} board={board:#x}");
            let exit_entries: Vec<_> = local
                .exits
                .iter()
                .map(|exit| ReferenceReachabilityEntryPose {
                    rotation: exit.rotation,
                    x: exit.x,
                    y: exit.y,
                })
                .collect();
            let from_exits =
                reference_entry_lock_anchors(10, 4, board, PieceKind::T, profile, &exit_entries)
                    .expect("every exit is placeable on the full board");
            let full = reference_entry_lock_anchors(
                10,
                4,
                board,
                PieceKind::T,
                profile,
                &[reference_entry],
            )
            .expect("entry is placeable on the full board");
            for rotation in 0..4 {
                assert_eq!(
                    local.grounded_lock_anchors[rotation] | from_exits[rotation],
                    full[rotation],
                    "{profile:?} board={board:#x} rotation={rotation}"
                );
            }
        }
    }
}

#[test]
fn whole_pose_domain_local_relation_has_no_unchecked_exit() {
    let entry = ConditionedReachabilityEntryPose {
        rotation: RotationState::Zero,
        x: 4,
        y: 4,
    };
    for profile in [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ] {
        let template = ReachabilityTemplate::compile(10, 4, PieceKind::T, profile);
        let window = ConditionedPoseWindow {
            min_x: 0,
            max_x: 9,
            min_y: 0,
            max_y: i8::try_from(template.ceiling).unwrap(),
        };
        let local = derive_exact_conditioned_local_relation(
            10,
            4,
            1_u64 << 1,
            PieceKind::T,
            profile,
            window,
            &[entry],
        )
        .expect("complete pose domain and actual entry");
        assert!(local.exits.is_empty(), "{profile:?}");
        assert_eq!(
            local.grounded_lock_anchors,
            reference_entry_lock_anchors(
                10,
                4,
                1_u64 << 1,
                PieceKind::T,
                profile,
                &[ReferenceReachabilityEntryPose {
                    rotation: entry.rotation,
                    x: entry.x,
                    y: entry.y,
                }],
            )
            .expect("independent full-board reference"),
            "{profile:?}"
        );
    }
}

#[test]
fn dependency_signature_reuses_only_equal_collision_conditions() {
    let entry = ConditionedReachabilityEntryPose {
        rotation: RotationState::Zero,
        x: 4,
        y: 4,
    };
    let window = ConditionedPoseWindow {
        min_x: 4,
        max_x: 4,
        min_y: 4,
        max_y: 4,
    };
    for profile in [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ] {
        let original = derive_exact_conditioned_local_relation(
            10,
            4,
            0,
            PieceKind::T,
            profile,
            window,
            &[entry],
        )
        .expect("valid local query");
        let mask = original.dependency_mask();
        assert_ne!(mask, 0, "{profile:?}");
        for cell in 0..40 {
            let outside = 1_u64 << cell;
            if mask & outside != 0 {
                continue;
            }
            assert!(original.matches_query(
                10,
                4,
                outside,
                PieceKind::T,
                profile,
                window,
                &[entry],
            ));
            let changed = derive_exact_conditioned_local_relation(
                10,
                4,
                outside,
                PieceKind::T,
                profile,
                window,
                &[entry],
            )
            .expect("outside condition does not invalidate the entry");
            assert_eq!(
                original.grounded_lock_anchors(),
                changed.grounded_lock_anchors(),
                "{profile:?} outside cell={cell}"
            );
            assert_eq!(
                original.exits(),
                changed.exits(),
                "{profile:?} outside cell={cell}"
            );
        }
        let inside = 1_u64 << mask.trailing_zeros();
        assert!(!original.matches_query(10, 4, inside, PieceKind::T, profile, window, &[entry],));
        assert!(!original.matches_query(
            10,
            4,
            0,
            PieceKind::T,
            profile,
            window,
            &[ConditionedReachabilityEntryPose { x: 5, ..entry }],
        ));
    }
}

#[test]
fn local_relation_rejects_invalid_boundaries_instead_of_proving_absence() {
    let entry = ConditionedReachabilityEntryPose {
        rotation: RotationState::Zero,
        x: 4,
        y: 4,
    };
    let outside = ConditionedPoseWindow {
        min_x: 0,
        max_x: 3,
        min_y: 0,
        max_y: 4,
    };
    assert!(derive_exact_conditioned_local_relation(
        10,
        4,
        0,
        PieceKind::T,
        KickTableProfileId::SrsPlus,
        outside,
        &[entry],
    )
    .is_none());
    let valid = ConditionedPoseWindow {
        min_x: 0,
        max_x: 9,
        min_y: 0,
        max_y: 4,
    };
    assert!(derive_exact_conditioned_local_relation(
        10,
        4,
        1_u64 << 41,
        PieceKind::T,
        KickTableProfileId::SrsPlus,
        valid,
        &[entry],
    )
    .is_none());
    assert!(derive_exact_conditioned_local_relation(
        10,
        4,
        0,
        PieceKind::T,
        KickTableProfileId::SrsPlus,
        valid,
        &[],
    )
    .is_none());
}

#[test]
fn local_first_exits_match_primitive_reference_across_top_row_obstacles() {
    let entry = ConditionedReachabilityEntryPose {
        rotation: RotationState::Zero,
        x: 4,
        y: 4,
    };
    let reference_entry = ReferenceReachabilityEntryPose {
        rotation: entry.rotation,
        x: entry.x,
        y: entry.y,
    };
    let window = ConditionedPoseWindow {
        min_x: 2,
        max_x: 6,
        min_y: 2,
        max_y: 4,
    };
    let mut boards = vec![0_u64];
    for x in 0..10 {
        boards.push(1_u64 << (30 + x));
        if x < 9 {
            boards.push((1_u64 << (30 + x)) | (1_u64 << (31 + x)));
        }
    }
    for profile in [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ] {
        for piece in PieceKind::STANDARD_TETROMINOES {
            for &board in &boards {
                let optimized = derive_exact_conditioned_local_relation(
                    10,
                    4,
                    board,
                    piece,
                    profile,
                    window,
                    &[entry],
                );
                let independent = reference_local_relation(
                    10,
                    4,
                    board,
                    piece,
                    profile,
                    window,
                    &[reference_entry],
                );
                let (optimized, independent) = match (optimized, independent) {
                    (Some(optimized), Some(independent)) => (optimized, independent),
                    (None, None) => continue,
                    _ => panic!("entry validation differs: {profile:?} {piece:?} board={board:#x}"),
                };
                let independent_exits: Vec<_> = independent
                    .exits
                    .iter()
                    .map(|exit| ConditionedReachabilityEntryPose {
                        rotation: exit.rotation,
                        x: exit.x,
                        y: exit.y,
                    })
                    .collect();
                assert_eq!(
                    optimized.grounded_lock_anchors(),
                    independent.grounded_lock_anchors,
                    "locks: {profile:?} {piece:?} board={board:#x}"
                );
                assert_eq!(
                    optimized.exits(),
                    independent_exits.as_slice(),
                    "first exits: {profile:?} {piece:?} board={board:#x}"
                );
            }
        }
    }
}
