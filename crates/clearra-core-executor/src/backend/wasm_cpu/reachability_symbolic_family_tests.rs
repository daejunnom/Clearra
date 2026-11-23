//! Finite correctness oracles only. The compiler never enumerates these worlds.

use std::collections::BTreeSet;

use clearra_core_domain::piece::rotation::RotationState;
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::{
    kicks::{
        KickOffsetSequence, KickTableEntry, KickTableProfile, KickTableProfileId, KickTransition,
        SrsKicks, VerifiedKickTableProfile,
    },
    profile::rule_profile::RuleProfileId,
};

use super::super::super::{
    compact_kick_authority::CompactKickAuthority, symbolic_boolean::BooleanLimits,
};
use super::*;
use crate::reachability_reference::{
    reference_spawn_lock_anchors, reference_spawn_locks_for_ordered_profile,
};

const PROFILES: [KickTableProfileId; 5] = [
    KickTableProfileId::NoKick,
    KickTableProfileId::Srs90,
    KickTableProfileId::SrsPlus,
    KickTableProfileId::SrsX,
    KickTableProfileId::Jstris180,
];

fn arena(catalog: &GeometryCatalog) -> BooleanArena {
    BooleanArena::new(
        catalog.skeleton_count() as u32,
        BooleanLimits {
            max_nodes: 1_000_000,
            max_work: 100_000_000,
            max_memo_entries: 1_000_000,
        },
    )
    .unwrap()
}

/// All non-free row variables are false. No loop enumerates free assignments.
fn free_selection_domain(arena: &mut BooleanArena, row_count: usize, free: &[u32]) -> BooleanId {
    let mut domain = BooleanId::TRUE;
    for row in (0..row_count as u32).rev() {
        if !free.contains(&row) {
            let variable = arena.variable(row).unwrap();
            domain = arena.ite(variable, BooleanId::FALSE, domain).unwrap();
        }
    }
    domain
}

fn exact_selection_domain(
    arena: &mut BooleanArena,
    row_count: usize,
    selected: &[u32],
) -> BooleanId {
    let mut domain = BooleanId::TRUE;
    for row in (0..row_count as u32).rev() {
        let variable = arena.variable(row).unwrap();
        domain = if selected.contains(&row) {
            arena.ite(variable, domain, BooleanId::FALSE).unwrap()
        } else {
            arena.ite(variable, BooleanId::FALSE, domain).unwrap()
        };
    }
    domain
}

fn value(arena: &BooleanArena, root: BooleanId, selected: &[u32]) -> bool {
    arena.evaluate(root, |row| selected.contains(&row))
}

fn mask(arena: &BooleanArena, cells: &[BooleanId; 64], selected: &[u32]) -> u64 {
    cells.iter().enumerate().fold(0, |bits, (cell, &root)| {
        bits | (u64::from(value(arena, root, selected)) << cell)
    })
}

/// Independent cell-level deletion, without CandidateProjection or row-frame helpers.
fn concrete_state(catalog: &GeometryCatalog, selected: &[u32]) -> (u64, u16, Vec<Option<usize>>) {
    let width = usize::from(catalog.width());
    let height = usize::from(catalog.height());
    let original = selected
        .iter()
        .fold(catalog.initial_board(), |board, &row| {
            board | catalog.skeleton(row).cells
        });
    let mut board = 0;
    let mut deleted = 0;
    let mut destinations = vec![None; height];
    let mut next_y = 0;
    for (y, destination) in destinations.iter_mut().enumerate() {
        if (0..width).all(|x| original & (1 << (y * width + x)) != 0) {
            deleted |= 1 << y;
            continue;
        }
        *destination = Some(next_y);
        for x in 0..width {
            if original & (1 << (y * width + x)) != 0 {
                board |= 1 << (next_y * width + x);
            }
        }
        next_y += 1;
    }
    (board, deleted, destinations)
}

/// Project four original cells, then compare whole standard shapes. This does
/// not use catalog realizations or the symbolic compiler's target mapping.
fn raw_row_is_legal(
    catalog: &GeometryCatalog,
    selected: &[u32],
    row_id: u32,
    anchors: [u64; 4],
) -> bool {
    if selected.contains(&row_id) {
        return false;
    }
    let width = usize::from(catalog.width());
    let row = catalog.skeleton(row_id);
    let (_, _, destinations) = concrete_state(catalog, selected);
    let mut target = 0_u64;
    let mut min_x = width;
    let mut min_y = usize::from(catalog.height());
    let mut cells = row.cells;
    while cells != 0 {
        let cell = cells.trailing_zeros() as usize;
        cells &= cells - 1;
        let x = cell % width;
        let Some(y) = destinations[cell / width] else {
            return false;
        };
        target |= 1 << (y * width + x);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
    }
    let shape = standard_tetromino_registry().get(row.piece).unwrap();
    RotationState::ALL.into_iter().any(|rotation| {
        let pose = shape.shape(rotation);
        let mut placed = 0_u64;
        for cell in pose.cells() {
            let x = min_x + cell.x() as usize;
            let y = min_y + cell.y() as usize;
            if x >= width || y >= usize::from(catalog.height()) {
                return false;
            }
            placed |= 1 << (y * width + x);
        }
        placed == target
            && anchors[rotation.quarter_turns() as usize] & (1 << (min_y * width + min_x)) != 0
    })
}

#[test]
fn symbolic_global_row_occupancy_preserves_overlapping_alternatives_and_wide_ids() {
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
    assert!(catalog.skeleton_count() > 64);
    let first = 0;
    let last = catalog.skeleton_count() as u32 - 1;
    let overlap = (1..catalog.skeleton_count() as u32)
        .find(|&row| catalog.skeleton(row).cells & catalog.skeleton(first).cells != 0)
        .unwrap();
    let mut arena = arena(&catalog);
    let control = ExecutionControl::default();
    let occupancy = SymbolicOccupancy::compile(&catalog, &mut arena, &control).unwrap();
    let domain = free_selection_domain(
        &mut arena,
        catalog.skeleton_count(),
        &[first, overlap, last],
    );
    let board = occupancy.for_domain(&mut arena, domain, &control).unwrap();
    for selected in [
        vec![],
        vec![first],
        vec![overlap],
        vec![last],
        vec![first, last],
    ] {
        let (expected, deleted, _) = concrete_state(&catalog, &selected);
        assert_eq!(mask(&arena, board.occupied_cells(), &selected), expected);
        for (y, &root) in board.deleted_rows().iter().enumerate() {
            assert_eq!(value(&arena, root, &selected), deleted & (1 << y) != 0);
        }
        assert_eq!(
            board
                .frames
                .iter()
                .filter(|&&root| value(&arena, root, &selected))
                .count(),
            1
        );
        assert!(value(&arena, board.frames[usize::from(deleted)], &selected));
    }
    // Catalog alternatives may overlap. They are OR owners of a cell, never
    // silently replaced by the first owner as in a fixed-cover-only pullback.
    let both = [first, overlap];
    let (expected, _, _) = concrete_state(&catalog, &both);
    assert_eq!(mask(&arena, board.occupied_cells(), &both), expected);
}

#[test]
fn symbolic_six_row_compaction_keeps_initial_cells_and_domain_correlation() {
    const INITIAL: u64 = 0x11_1133;
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
        4,
        6,
        INITIAL,
        0xff_ffff & !INITIAL,
    )
    .unwrap();
    let first = 0;
    let last = catalog.skeleton_count() as u32 - 1;
    let mut arena = arena(&catalog);
    let control = ExecutionControl::default();
    let occupancy = SymbolicOccupancy::compile(&catalog, &mut arena, &control).unwrap();
    let a = exact_selection_domain(&mut arena, catalog.skeleton_count(), &[first]);
    let b = exact_selection_domain(&mut arena, catalog.skeleton_count(), &[last]);
    let domain = arena.or(a, b).unwrap();
    let board = occupancy.for_domain(&mut arena, domain, &control).unwrap();
    assert_eq!(board.frames.len(), 64);
    for selected in [[first], [last]] {
        assert_eq!(
            mask(&arena, board.occupied_cells(), &selected),
            concrete_state(&catalog, &selected).0
        );
    }
    assert!(!value(&arena, board.domain(), &[]));
    assert_eq!(mask(&arena, board.occupied_cells(), &[]), 0);
    assert!(!value(&arena, board.domain(), &[first, last]));
}

#[test]
fn symbolic_row_legality_matches_independent_physics_for_all_builtin_kicks() {
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
    let chosen: Vec<_> = [0x0033, 0x00cc, 0x3300, 0xcc00]
        .into_iter()
        .map(|cells| catalog.skeleton_id(PieceKind::O, cells).unwrap())
        .collect();
    let control = ExecutionControl::default();
    let mut checked = 0;
    let mut gap_targets = 0;
    for profile in PROFILES {
        let templates = SharedReachabilityTemplates::new(4, 4, profile);
        let mut arena = arena(&catalog);
        let occupancy = SymbolicOccupancy::compile(&catalog, &mut arena, &control).unwrap();
        let domain = free_selection_domain(&mut arena, catalog.skeleton_count(), &chosen);
        let board = occupancy.for_domain(&mut arena, domain, &control).unwrap();
        let outside = arena.not(domain).unwrap();
        for piece in PieceKind::STANDARD_TETROMINOES {
            let relation = board
                .compile_piece(&mut arena, &templates, piece, &control)
                .unwrap();
            // Check the movement result before legal_row's availability gate:
            // collision-only guards must never publish an outside-domain lock.
            for &lock in &relation.locks {
                assert_eq!(arena.and(lock, outside).unwrap(), BooleanId::FALSE);
            }
            let rows: Vec<_> = (0..catalog.skeleton_count() as u32)
                .filter(|&row| catalog.skeleton(row).piece == piece)
                .collect();
            let mut targets = BTreeSet::new();
            targets.insert(rows[0]);
            targets.insert(*rows.last().unwrap());
            if let Some(&row) = rows
                .iter()
                .find(|&&row| !catalog.has_frame_zero_realization(row))
            {
                targets.insert(row);
                gap_targets += 1;
            }
            targets.extend(
                chosen
                    .iter()
                    .copied()
                    .filter(|&row| catalog.skeleton(row).piece == piece),
            );
            let predicates: Vec<_> = targets
                .into_iter()
                .map(|row| (row, relation.legal_row(&mut arena, row, &control).unwrap()))
                .collect();
            // Sixteen finite oracle worlds; no such loop exists in production.
            for bits in 0..1 << chosen.len() {
                let selected: Vec<_> = chosen
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &row)| (bits & (1 << i) != 0).then_some(row))
                    .collect();
                let physical = concrete_state(&catalog, &selected).0;
                let anchors = reference_spawn_lock_anchors(4, 4, physical, piece, profile).unwrap();
                for &(row, root) in &predicates {
                    assert_eq!(
                        value(&arena, root, &selected),
                        raw_row_is_legal(&catalog, &selected, row, anchors),
                        "profile={profile:?}, piece={piece:?}, row={row}, selected={selected:?}"
                    );
                    checked += 1;
                }
            }
            assert!(relation.work().poses > 0);
        }
    }
    assert!(checked > 1_000);
    assert!(gap_targets > 0);
}

#[test]
fn symbolic_collision_only_transfers_keep_reached_inside_the_seed_domain() {
    let control = ExecutionControl::default();
    let mut arena = BooleanArena::new(3, BooleanLimits::default()).unwrap();
    let selected_domain = arena.variable(0).unwrap();
    let blocker = arena.variable(1).unwrap();
    let excluded = arena.variable(2).unwrap();
    let included = arena.not(excluded).unwrap();
    let domain = arena.and(selected_domain, included).unwrap();
    let occupied = arena.and(domain, blocker).unwrap();
    let free_target = arena.not(occupied).unwrap();
    // Outside domain all collision predicates are true. Seeding, rather than
    // a later lock or row gate, must reject those otherwise admitted worlds.
    let free = [BooleanId::TRUE, free_target, BooleanId::TRUE];
    let edges = [
        Edge {
            target: 1,
            guard: TransferGuard::TargetFree,
        },
        Edge {
            target: 2,
            guard: TransferGuard::TargetFree,
        },
        Edge {
            target: 0,
            guard: TransferGuard::TargetFree,
        },
    ];
    let mut work = SymbolicReachabilityWork::default();
    let reached = propagate(
        &mut arena,
        domain,
        &[0, 1, 2, 3],
        &edges,
        &[0],
        &free,
        &[BooleanId::FALSE; 64],
        &[],
        &control,
        &mut work,
    )
    .unwrap();
    for assignment in 0..8 {
        let in_domain = assignment & 1 != 0 && assignment & 4 == 0;
        for (pose, &root) in reached.iter().enumerate() {
            let expected = in_domain && (pose == 0 || assignment & 2 == 0);
            assert_eq!(
                arena.evaluate(root, |variable| assignment & (1 << variable) != 0),
                expected
            );
        }
    }
    let outside = arena.not(domain).unwrap();
    for &root in &reached {
        assert_eq!(arena.and(root, outside).unwrap(), BooleanId::FALSE);
    }
    let none = propagate(
        &mut arena,
        BooleanId::FALSE,
        &[0, 1, 2, 3],
        &edges,
        &[0],
        &free,
        &[BooleanId::FALSE; 64],
        &[],
        &control,
        &mut work,
    )
    .unwrap();
    assert_eq!(none, [BooleanId::FALSE; 3]);
}

#[test]
fn symbolic_transfer_descriptors_keep_priority_slots_and_requeued_deltas_exact() {
    let control = ExecutionControl::default();
    let mut arena = BooleanArena::new(4, BooleanLimits::default()).unwrap();
    let domain = arena.variable(3).unwrap();
    let mut occupied = [BooleanId::FALSE; 64];
    for (cell, root) in occupied[..3].iter_mut().enumerate() {
        let variable = arena.variable(cell as u32).unwrap();
        *root = arena.and(domain, variable).unwrap();
    }
    let free_a = arena.not(occupied[0]).unwrap();
    let free_b = arena.not(occupied[1]).unwrap();
    let free_c = arena.not(occupied[2]).unwrap();
    let free = [
        BooleanId::TRUE,
        free_c,
        free_c,
        free_c,
        free_a,
        free_b,
        BooleanId::TRUE,
        BooleanId::TRUE,
        BooleanId::TRUE,
        BooleanId::TRUE,
        free_a,
    ];
    // Slot 0's original sequence is [4, 3, 5, 6]; slot 1's is [5, 7].
    // Poses 4 and 5 have no transfer descriptor but remain first-success
    // competitors. The earlier target 3 becomes a prior for target 6.
    let prior_targets = [4, 3, 5, 6, 5, 7];
    let edges = [
        Edge {
            target: 1,
            guard: TransferGuard::SingleCell { cell: 0 },
        },
        Edge {
            target: 2,
            guard: TransferGuard::BlockedPriorPose { pose: 4 },
        },
        Edge {
            target: 3,
            guard: TransferGuard::Ordered {
                slot: 0,
                prior_offset: 0,
                prior_count: 1,
            },
        },
        Edge {
            target: 6,
            guard: TransferGuard::Ordered {
                slot: 0,
                prior_offset: 0,
                prior_count: 3,
            },
        },
        Edge {
            target: 7,
            guard: TransferGuard::Ordered {
                slot: 1,
                prior_offset: 4,
                prior_count: 1,
            },
        },
        // Seed 8 sends a first delta with a=true directly to pose 0. Seed 9
        // sends a=false through pose 10, so pose 0 is processed a second time.
        Edge {
            target: 0,
            guard: TransferGuard::SingleCell { cell: 0 },
        },
        Edge {
            target: 10,
            guard: TransferGuard::TargetFree,
        },
        Edge {
            target: 0,
            guard: TransferGuard::TargetFree,
        },
    ];
    let mut work = SymbolicReachabilityWork::default();
    let reached = propagate(
        &mut arena,
        domain,
        &[0, 5, 5, 5, 5, 5, 5, 5, 5, 6, 7, 8],
        &edges,
        &[8, 9],
        &free,
        &occupied,
        &prior_targets,
        &control,
        &mut work,
    )
    .unwrap();
    for assignment in 0..16 {
        let in_domain = assignment & 8 != 0;
        let a = assignment & 1 != 0;
        let b = assignment & 2 != 0;
        let c = assignment & 4 != 0;
        let expected = [
            in_domain,
            in_domain && a && !c,
            in_domain && a && !c,
            in_domain && a && !c,
            false,
            false,
            in_domain && a && b && c,
            in_domain && b,
            in_domain,
            in_domain,
            in_domain && !a,
        ];
        for (pose, &root) in reached.iter().enumerate() {
            assert_eq!(
                arena.evaluate(root, |variable| assignment & (1 << variable) != 0),
                expected[pose],
                "assignment={assignment} pose={pose}",
            );
        }
    }
    // The first pop consumes three original slot-0 terms and one slot-1
    // term. On requeue, slot 0 stops after one false term; slot 1 still runs.
    // A target-conditioned prefix would incorrectly reject target 6, and a
    // prefix reused across pops would lose a=false assignments at target 7.
    assert_eq!(work.rotation_ordered_prior_terms, 6);
    eprintln!(
        "symbolic_transfer_descriptor_bytes={}",
        std::mem::size_of::<Edge>()
    );
}

fn ordered_profile(reorder: bool, first_repetitions: usize) -> VerifiedKickTableProfile {
    let selected = KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
    let mut entries = SrsKicks::profile().entries().to_vec();
    let entry = entries
        .iter_mut()
        .find(|entry| entry.transition() == selected)
        .unwrap();
    let mut offsets = entry.sequence().offsets().to_vec();
    if reorder {
        offsets[..=2].rotate_right(1);
    }
    let mut repeated = vec![offsets[0]; first_repetitions];
    repeated.extend_from_slice(&offsets[1..]);
    *entry = KickTableEntry::new(selected, KickOffsetSequence::new(repeated));
    VerifiedKickTableProfile::try_new(KickTableProfile::new(
        KickTableProfileId::Srs90,
        RuleProfileId::Srs,
        entries,
    ))
    .unwrap()
}

#[test]
fn symbolic_imported_ordered_tables_keep_distinct_same_tag_answers() {
    const INITIAL: u64 = 0x000f_83c7;
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
        10,
        2,
        INITIAL,
        ((1 << 20) - 1) & !INITIAL,
    )
    .unwrap();
    let target = catalog.skeleton_id(PieceKind::T, 0x4038).unwrap();
    let control = ExecutionControl::default();
    let mut arena = arena(&catalog);
    let occupancy = SymbolicOccupancy::compile(&catalog, &mut arena, &control).unwrap();
    let domain = exact_selection_domain(&mut arena, catalog.skeleton_count(), &[]);
    let board = occupancy.for_domain(&mut arena, domain, &control).unwrap();
    for repetitions in [1, 300] {
        let mut answers = Vec::new();
        for reordered in [false, true] {
            let profile = ordered_profile(reordered, repetitions);
            let authority =
                CompactKickAuthority::admit_profile(10, 2, RuleProfileId::Srs, &profile).unwrap();
            let templates = SharedReachabilityTemplates::with_authority(authority);
            let relation = board
                .compile_piece(&mut arena, &templates, PieceKind::T, &control)
                .unwrap();
            let outside = arena.not(domain).unwrap();
            for &lock in &relation.locks {
                assert_eq!(arena.and(lock, outside).unwrap(), BooleanId::FALSE);
            }
            let root = relation.legal_row(&mut arena, target, &control).unwrap();
            let actual = value(&arena, root, &[]);
            let anchors = reference_spawn_locks_for_ordered_profile(
                10,
                2,
                INITIAL,
                PieceKind::T,
                profile.profile(),
            )
            .unwrap();
            assert_eq!(actual, raw_row_is_legal(&catalog, &[], target, anchors));
            if repetitions > 1 {
                assert!(relation.work().rotation_candidates_pruned > 0);
                assert!(relation
                    .template
                    .reverse_rotation_sources
                    .iter()
                    .any(|candidate| candidate.prior_target_count >= 300));
            }
            answers.push(actual);
        }
        assert_eq!(answers, [false, true]);
    }
}

#[test]
fn symbolic_rotation_lookup_reuses_exact_template_edges_and_original_ordinals() {
    let mut checked = 0;
    let mut pruned = 0;
    super::super::with_global_kick_pruning(true, || {
        for profile in PROFILES {
            let templates = SharedReachabilityTemplates::new(4, 4, profile);
            for piece in PieceKind::STANDARD_TETROMINOES {
                let template = templates.template(piece);
                for source in 0..template.state_masks.len() {
                    for slot in 0..if template.allow_180 { 3 } else { 2 } {
                        let transition = source * 3 + slot;
                        let start = template.rotation_target_offsets[transition] as usize;
                        let end = template.rotation_target_offsets[transition + 1] as usize;
                        for candidate in start..end {
                            let target = usize::from(template.rotation_targets[candidate]);
                            let retained = retained_rotation_candidate(
                                &template,
                                source,
                                slot,
                                candidate - start,
                                target,
                            )
                            .unwrap();
                            let possible = super::super::edge_possible_in_cube(
                                template.state_masks[source],
                                template.state_masks[target],
                                template.rotation_targets[start..candidate]
                                    .iter()
                                    .map(|&prior| template.state_masks[usize::from(prior)]),
                                0,
                                u64::MAX,
                            );
                            assert_eq!(retained.is_some(), possible);
                            if let Some(retained) = retained {
                                assert_eq!(usize::from(retained.source), source);
                                assert_eq!(usize::from(retained.slot), slot);
                                assert_eq!(retained.prior_target_count as usize, candidate - start);
                                assert_eq!(
                                    reverse_rotation_prior_targets(&template, retained),
                                    &template.rotation_targets[start..candidate]
                                );
                            } else {
                                pruned += 1;
                            }
                            checked += 1;
                        }
                    }
                }
            }
        }
    });
    assert!(checked > 0);
    assert!(pruned > 0);
}

#[test]
fn symbolic_refusals_never_become_negative_movement_answers() {
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
    let control = ExecutionControl::default();
    let mut arena = arena(&catalog);
    let occupancy = SymbolicOccupancy::compile(&catalog, &mut arena, &control).unwrap();
    let mut foreign =
        BooleanArena::new(catalog.skeleton_count() as u32, BooleanLimits::default()).unwrap();
    assert!(matches!(
        occupancy.for_domain(&mut foreign, BooleanId::TRUE, &control),
        Err(BooleanError::InvalidInput("symbolic_arena_owner"))
    ));
    let domain = exact_selection_domain(&mut arena, catalog.skeleton_count(), &[]);
    let board = occupancy.for_domain(&mut arena, domain, &control).unwrap();
    let mismatched = SharedReachabilityTemplates::new(10, 4, KickTableProfileId::SrsPlus);
    assert!(matches!(
        board.compile_piece(&mut arena, &mismatched, PieceKind::T, &control),
        Err(BooleanError::InvalidInput("symbolic_movement_owner"))
    ));
    let cancelled = ExecutionControl::default();
    cancelled.cancellation.handle().cancel();
    assert!(matches!(
        occupancy.for_domain(&mut arena, domain, &cancelled),
        Err(BooleanError::Cancelled)
    ));
    let mut limited = BooleanArena::new(
        catalog.skeleton_count() as u32,
        BooleanLimits {
            max_nodes: 2,
            ..BooleanLimits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        SymbolicOccupancy::compile(&catalog, &mut limited, &control),
        Err(BooleanError::NodeLimit)
    ));
    let empty = occupancy
        .for_domain(&mut arena, BooleanId::FALSE, &control)
        .unwrap();
    let templates = SharedReachabilityTemplates::new(4, 4, KickTableProfileId::SrsPlus);
    let relation = empty
        .compile_piece(&mut arena, &templates, PieceKind::T, &control)
        .unwrap();
    let target = (0..catalog.skeleton_count() as u32)
        .find(|&row| catalog.skeleton(row).piece == PieceKind::T)
        .unwrap();
    assert_eq!(
        relation.legal_row(&mut arena, target, &control).unwrap(),
        BooleanId::FALSE
    );
}
