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

fn cell_value(arena: &BooleanArena, root: BooleanId, occupied: u64) -> bool {
    arena.evaluate(root, |cell| occupied & (1_u64 << cell) != 0)
}

/// Explicit worlds belong only to the bounded, independent fixture oracle.
fn finite_cell_domain(arena: &mut BooleanArena, area: u32, worlds: &[u64]) -> BooleanId {
    let mut domain = BooleanId::FALSE;
    for &world in worlds {
        let mut singleton = BooleanId::TRUE;
        for cell in (0..area).rev() {
            singleton = if world & (1_u64 << cell) != 0 {
                arena.mk(cell, BooleanId::FALSE, singleton).unwrap()
            } else {
                arena.mk(cell, singleton, BooleanId::FALSE).unwrap()
            };
        }
        domain = arena.or(domain, singleton).unwrap();
    }
    domain
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
    let cell_worlds: Vec<_> = (0..1 << chosen.len())
        .map(|bits| {
            chosen.iter().enumerate().fold(0, |cells, (index, &row)| {
                cells
                    | if bits & (1 << index) == 0 {
                        0
                    } else {
                        catalog.skeleton(row).cells
                    }
            })
        })
        .collect();
    for profile in PROFILES {
        let templates = SharedReachabilityTemplates::new(4, 4, profile);
        let mut arena = arena(&catalog);
        let occupancy = SymbolicOccupancy::compile(&catalog, &mut arena, &control).unwrap();
        let domain = free_selection_domain(&mut arena, catalog.skeleton_count(), &chosen);
        let board = occupancy.for_domain(&mut arena, domain, &control).unwrap();
        let outside = arena.not(domain).unwrap();
        let mut cells = BooleanArena::new(16, BooleanLimits::default()).unwrap();
        let cell_occupancy =
            SymbolicOccupancy::compile_cells(&catalog, &mut cells, &control).unwrap();
        let cell_domain = finite_cell_domain(&mut cells, 16, &cell_worlds);
        let cell_board = cell_occupancy
            .for_domain(&mut cells, cell_domain, &control)
            .unwrap();
        assert!(!board.uses_cell_variables());
        assert!(cell_board.uses_cell_variables());
        for piece in PieceKind::STANDARD_TETROMINOES {
            let relation = board
                .compile_piece(&mut arena, &templates, piece, &control)
                .unwrap();
            let unseeded = board
                .compile_piece_with_vertical_seed::<false>(
                    &mut arena, &templates, piece, domain, &control,
                )
                .unwrap();
            assert_eq!(relation.locks, unseeded.locks, "{profile:?} {piece:?}");
            let cell_relation = cell_board
                .compile_piece(&mut cells, &templates, piece, &control)
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
                .map(|row| {
                    (
                        row,
                        relation.legal_row(&mut arena, row, &control).unwrap(),
                        cell_relation.legal_row(&mut cells, row, &control).unwrap(),
                    )
                })
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
                for &(row, root, cell_root) in &predicates {
                    let expected = raw_row_is_legal(&catalog, &selected, row, anchors);
                    assert_eq!(
                        value(&arena, root, &selected),
                        expected,
                        "profile={profile:?}, piece={piece:?}, row={row}, selected={selected:?}"
                    );
                    assert_eq!(cell_value(&cells, cell_root, cell_worlds[bits]), expected,
                        "cell profile={profile:?}, piece={piece:?}, row={row}, selected={selected:?}");
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
fn symbolic_piece_subdomain_reuses_base_occupancy_without_admitting_other_worlds() {
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
    let chosen: Vec<_> = [0x0033, 0x00cc, 0x3300, 0xcc00]
        .into_iter()
        .map(|cells| catalog.skeleton_id(PieceKind::O, cells).unwrap())
        .collect();
    let control = ExecutionControl::default();
    let templates = SharedReachabilityTemplates::new(4, 4, KickTableProfileId::Jstris180);
    let mut arena = arena(&catalog);
    let occupancy = SymbolicOccupancy::compile(&catalog, &mut arena, &control).unwrap();
    let domain = free_selection_domain(&mut arena, catalog.skeleton_count(), &chosen);
    let board = occupancy.for_domain(&mut arena, domain, &control).unwrap();
    let requested = arena.variable(chosen[0]).unwrap();
    let effective = arena.and(domain, requested).unwrap();
    let outside = arena.not(effective).unwrap();
    // Shared physical roots need not vanish outside the narrower piece domain.
    assert!(board
        .occupied
        .iter()
        .any(|&root| arena.and(root, outside).unwrap() != BooleanId::FALSE));
    for piece in PieceKind::STANDARD_TETROMINOES {
        let broad = board
            .compile_piece(&mut arena, &templates, piece, &control)
            .unwrap();
        let restricted = board
            .compile_piece_in_domain(&mut arena, &templates, piece, requested, &control)
            .unwrap();
        assert_eq!(restricted.domain, effective);
        for (&full, &part) in broad.locks.iter().zip(&restricted.locks) {
            assert_eq!(part, arena.and(effective, full).unwrap());
            assert_eq!(arena.and(part, outside).unwrap(), BooleanId::FALSE);
        }
        let rows: Vec<_> = board.row_ids(piece).collect();
        for &row in [
            rows.first().unwrap(),
            &rows[rows.len() / 2],
            rows.last().unwrap(),
        ] {
            let full = broad.legal_row(&mut arena, row, &control).unwrap();
            let part = restricted.legal_row(&mut arena, row, &control).unwrap();
            assert_eq!(part, arena.and(effective, full).unwrap());
        }
    }
}

#[test]
fn symbolic_original_cells_preserve_distinct_deleted_frames_on_the_same_physical_board() {
    const INITIAL: u64 = 0x113;
    const REQUIRED: u64 = 0xeec;
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(4, 3, INITIAL, REQUIRED).unwrap();
    let row_for_cells = |mask| {
        (0..catalog.skeleton_count() as u32)
            .find(|&row| catalog.skeleton(row).cells == mask)
            .unwrap()
    };
    let first = [row_for_cells(0x06c), row_for_cells(0x0e4)];
    let last = [row_for_cells(0xe80), row_for_cells(0xe08)];
    let piece = catalog.skeleton(last[0]).piece;
    assert_eq!(catalog.skeleton(last[1]).piece, piece);
    let control = ExecutionControl::default();
    let templates = SharedReachabilityTemplates::new(4, 3, KickTableProfileId::Jstris180);
    let mut rows = arena(&catalog);
    let row_occupancy = SymbolicOccupancy::compile(&catalog, &mut rows, &control).unwrap();
    let empty = exact_selection_domain(&mut rows, catalog.skeleton_count(), &[]);
    let initial_board = row_occupancy
        .for_domain(&mut rows, empty, &control)
        .unwrap();
    for &row in &first {
        let relation = initial_board
            .compile_piece(&mut rows, &templates, catalog.skeleton(row).piece, &control)
            .unwrap();
        let legal = relation.legal_row(&mut rows, row, &control).unwrap();
        assert!(value(&rows, legal, &[]));
    }
    let left = exact_selection_domain(&mut rows, catalog.skeleton_count(), &[first[0]]);
    let right = exact_selection_domain(&mut rows, catalog.skeleton_count(), &[first[1]]);
    let row_domain = rows.or(left, right).unwrap();
    let row_board = row_occupancy
        .for_domain(&mut rows, row_domain, &control)
        .unwrap();
    let mut cells = BooleanArena::new(12, BooleanLimits::default()).unwrap();
    let cell_occupancy = SymbolicOccupancy::compile_cells(&catalog, &mut cells, &control).unwrap();
    let original = [0x17f, 0x1f7];
    let cell_domain = finite_cell_domain(&mut cells, 12, &original);
    let cell_board = cell_occupancy
        .for_domain(&mut cells, cell_domain, &control)
        .unwrap();
    let row_relation = row_board
        .compile_piece(&mut rows, &templates, piece, &control)
        .unwrap();
    let cell_relation = cell_board
        .compile_piece(&mut cells, &templates, piece, &control)
        .unwrap();
    for branch in 0..2 {
        let (physical, deleted, _) = concrete_state(&catalog, &[first[branch]]);
        assert_eq!(physical, 0x017);
        assert_eq!(deleted, 1 << branch);
        let observed = cell_board
            .occupied
            .iter()
            .enumerate()
            .fold(0, |mask, (cell, &root)| {
                mask | (u64::from(cell_value(&cells, root, original[branch])) << cell)
            });
        assert_eq!(observed, physical);
        for (index, &root) in cell_board.deleted_rows().iter().enumerate() {
            assert_eq!(cell_value(&cells, root, original[branch]), index == branch);
        }
        for (target, &row) in last.iter().enumerate() {
            let expected = target == branch;
            let row_root = row_relation.legal_row(&mut rows, row, &control).unwrap();
            let cell_root = cell_relation.legal_row(&mut cells, row, &control).unwrap();
            assert_eq!(value(&rows, row_root, &[first[branch]]), expected);
            assert_eq!(cell_value(&cells, cell_root, original[branch]), expected);
        }
        assert_eq!(
            catalog.skeleton(first[branch]).cells | catalog.skeleton(last[branch]).cells,
            REQUIRED
        );
    }
}

#[test]
fn symbolic_cell_scope_preserves_fixed_inputs_and_rejects_foreign_owners() {
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(4, 2, 1, 0xf0).unwrap();
    let control = ExecutionControl::default();
    let mut arena = BooleanArena::new(8, BooleanLimits::default()).unwrap();
    let occupancy = SymbolicOccupancy::compile_cells(&catalog, &mut arena, &control).unwrap();
    let board = occupancy
        .for_domain(&mut arena, BooleanId::TRUE, &control)
        .unwrap();
    assert!(board.uses_cell_variables());
    assert!(cell_value(&arena, board.domain(), 1));
    assert!(cell_value(&arena, board.domain(), 0xf1));
    assert!(!cell_value(&arena, board.domain(), 0));
    assert!(!cell_value(&arena, board.domain(), 3));
    let mut foreign = BooleanArena::new(8, BooleanLimits::default()).unwrap();
    assert!(matches!(
        occupancy.for_domain(&mut foreign, BooleanId::TRUE, &control),
        Err(BooleanError::InvalidInput("symbolic_arena_owner"))
    ));
    let templates = SharedReachabilityTemplates::new(4, 2, KickTableProfileId::Jstris180);
    assert!(matches!(
        board.compile_piece(&mut foreign, &templates, PieceKind::I, &control),
        Err(BooleanError::InvalidInput("symbolic_arena_owner"))
    ));
}

#[test]
fn symbolic_vertical_certificate_is_positive_and_retains_lateral_detours() {
    let control = ExecutionControl::default();
    let mut arena = BooleanArena::new(2, BooleanLimits::default()).unwrap();
    let blocker = arena.variable(0).unwrap();
    let mut domain = arena.variable(1).unwrap();
    let occupied = arena.and(domain, blocker).unwrap();
    let free_middle = arena.not(occupied).unwrap();
    let mut free = [
        BooleanId::TRUE,
        BooleanId::TRUE,
        free_middle,
        BooleanId::TRUE,
        BooleanId::TRUE,
        BooleanId::TRUE,
    ];
    let invalid = INVALID_STATE_INDEX;
    let translations = [
        [invalid, invalid, 1],
        [invalid, 0, invalid],
        [0, invalid, 3],
        [1, 2, invalid],
        [2, invalid, 5],
        [3, 4, invalid],
    ];
    let targets = [1, 0, 0, 3, 1, 2, 2, 5, 3, 4];
    let edges: Vec<_> = targets
        .into_iter()
        .map(|target| Edge {
            target,
            guard: TransferGuard::TargetFree,
        })
        .collect();
    let mut certificate = vec![BooleanId::FALSE; free.len()];
    certificate[4] = domain;
    certificate[5] = domain;
    seed_vertical_closure(
        &mut arena,
        &mut domain,
        &mut free,
        &translations,
        &mut certificate,
        false,
        &control,
        &mut SymbolicReachabilityWork::default(),
    )
    .unwrap();
    let directly_reachable = arena.and(domain, free_middle).unwrap();
    assert_eq!(certificate[0], directly_reachable);
    let mut seeded_work = SymbolicReachabilityWork::default();
    let mut unseeded_work = SymbolicReachabilityWork::default();
    let mut run = |vertical, work: &mut SymbolicReachabilityWork| {
        propagate(
            &mut arena,
            &mut domain,
            &[0, 1, 2, 4, 6, 8, 10],
            &edges,
            &[4, 5],
            &mut free,
            &[BooleanId::FALSE; 64],
            &[],
            vertical,
            false,
            &control,
            work,
        )
        .unwrap()
    };
    let seeded = run(Some(translations.as_slice()), &mut seeded_work);
    let unseeded = run(None, &mut unseeded_work);
    assert_eq!(seeded, unseeded);
    assert_eq!(seeded[0], domain);
    assert_ne!(seeded[0], certificate[0]);
    assert_eq!(seeded[2], directly_reachable);
    assert_eq!(seeded_work.vertical_seed_updates, 4);
    assert!(seeded_work.early_reached_rejections > 0);
    assert_eq!(unseeded_work.vertical_seed_edges, 0);
}

#[test]
fn symbolic_collision_only_transfers_keep_reached_inside_the_seed_domain() {
    let control = ExecutionControl::default();
    let mut arena = BooleanArena::new(3, BooleanLimits::default()).unwrap();
    let selected_domain = arena.variable(0).unwrap();
    let blocker = arena.variable(1).unwrap();
    let excluded = arena.variable(2).unwrap();
    let included = arena.not(excluded).unwrap();
    let mut domain = arena.and(selected_domain, included).unwrap();
    let occupied = arena.and(domain, blocker).unwrap();
    let free_target = arena.not(occupied).unwrap();
    // Outside domain all collision predicates are true. Seeding, rather than
    // a later lock or row gate, must reject those otherwise admitted worlds.
    let mut free = [BooleanId::TRUE, free_target, BooleanId::TRUE];
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
        &mut domain,
        &[0, 1, 2, 3],
        &edges,
        &[0],
        &mut free,
        &[BooleanId::FALSE; 64],
        &[],
        None,
        false,
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
    let mut empty_domain = BooleanId::FALSE;
    let none = propagate(
        &mut arena,
        &mut empty_domain,
        &[0, 1, 2, 3],
        &edges,
        &[0],
        &mut free,
        &[BooleanId::FALSE; 64],
        &[],
        None,
        false,
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
    let mut domain = arena.variable(3).unwrap();
    let mut occupied = [BooleanId::FALSE; 64];
    for (cell, root) in occupied[..3].iter_mut().enumerate() {
        let variable = arena.variable(cell as u32).unwrap();
        *root = arena.and(domain, variable).unwrap();
    }
    let free_a = arena.not(occupied[0]).unwrap();
    let free_b = arena.not(occupied[1]).unwrap();
    let free_c = arena.not(occupied[2]).unwrap();
    let mut free = [
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
        &mut domain,
        &[0, 5, 5, 5, 5, 5, 5, 5, 5, 6, 7, 8],
        &edges,
        &[8, 9],
        &mut free,
        &occupied,
        &prior_targets,
        None,
        false,
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

#[test]
fn symbolic_source_pop_compaction_remaps_live_families_before_following_moves() {
    let control = ExecutionControl::default();
    let mut parent = BooleanArena::new(12, BooleanLimits::default()).unwrap();
    let variables: Vec<_> = (0..12)
        .map(|variable| parent.variable(variable).unwrap())
        .collect();
    let mut occupied = [BooleanId::FALSE; 64];
    occupied[0] = variables[0];
    occupied[1] = variables[1];
    let prefix = parent.freeze_prefix().unwrap();
    let edges = [
        Edge {
            target: 1,
            guard: TransferGuard::TargetFree,
        },
        Edge {
            target: 2,
            guard: TransferGuard::SingleCell { cell: 0 },
        },
        Edge {
            target: 3,
            guard: TransferGuard::TargetFree,
        },
        Edge {
            target: 3,
            guard: TransferGuard::TargetFree,
        },
        Edge {
            target: 0,
            guard: TransferGuard::TargetFree,
        },
    ];
    let mut answers = Vec::new();
    for collect in [false, true] {
        let mut arena = BooleanArena::fork_prefix(Arc::clone(&prefix)).unwrap();
        // These bounded, unreachable cubes precede every live private root.
        // Reclaiming them therefore relocates domain/free/reached/pending IDs.
        for assignment in 0..64 {
            let mut garbage = BooleanId::TRUE;
            for variable in (4..12).rev() {
                garbage = if assignment & (1 << (variable - 4)) == 0 {
                    arena.mk(variable, garbage, BooleanId::FALSE).unwrap()
                } else {
                    arena.mk(variable, BooleanId::FALSE, garbage).unwrap()
                };
            }
            assert_ne!(garbage, BooleanId::FALSE);
        }
        let included = arena.not(variables[3]).unwrap();
        let mut domain = arena.and(variables[2], included).unwrap();
        let either_blocked = arena.or(variables[0], variables[1]).unwrap();
        let both_free = arena.not(either_blocked).unwrap();
        let b_free = arena.not(variables[1]).unwrap();
        let mut free = [BooleanId::TRUE, both_free, b_free, BooleanId::TRUE];
        arena.set_private_compaction_threshold_for_test(1);
        let original_owner = arena.owner_id();
        let mut work = SymbolicReachabilityWork::default();
        let reached = propagate(
            &mut arena,
            &mut domain,
            &[0, 2, 3, 4, 5],
            &edges,
            &[0],
            &mut free,
            &occupied,
            &[],
            None,
            collect,
            &control,
            &mut work,
        )
        .unwrap();
        // Pose 0 is the only initial pending pose. Its boundary collects;
        // subsequent moves 1→3 and 2→3 must consume the relocated pending IDs.
        if collect {
            assert!(arena.compaction_count() > 0);
            assert!(arena.compaction_reclaimed_nodes() >= 64);
            assert_ne!(arena.owner_id(), original_owner);
        } else {
            assert_eq!(arena.compaction_count(), 0);
            assert_eq!(arena.owner_id(), original_owner);
        }
        let mut observed = Vec::new();
        let next_move = arena.and(reached[3], free[1]).unwrap();
        for assignment in 0..16 {
            let a = assignment & 1 != 0;
            let b = assignment & 2 != 0;
            let in_domain = assignment & 4 != 0 && assignment & 8 == 0;
            let expected = [
                in_domain,
                in_domain && !a && !b,
                in_domain && a && !b,
                in_domain && !b,
            ];
            let value = |root| {
                arena.evaluate(root, |variable| {
                    variable < 4 && assignment & (1 << variable) != 0
                })
            };
            assert_eq!(value(domain), in_domain);
            assert_eq!(value(free[1]), !a && !b);
            assert_eq!(value(next_move), in_domain && !a && !b);
            for (pose, &root) in reached.iter().enumerate() {
                let actual = value(root);
                assert_eq!(actual, expected[pose]);
                observed.push(actual);
            }
        }
        answers.push(observed);
    }
    assert_eq!(answers[0], answers[1]);
}

#[test]
fn symbolic_fork_local_board_keeps_its_immutable_private_roots() {
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(4, 2, 0, 0xff).unwrap();
    let control = ExecutionControl::default();
    let mut parent =
        BooleanArena::new(catalog.skeleton_count() as u32, BooleanLimits::default()).unwrap();
    let prefix = parent.freeze_prefix().unwrap();
    let mut arena = BooleanArena::fork_prefix(prefix).unwrap();
    let occupancy = SymbolicOccupancy::compile(&catalog, &mut arena, &control).unwrap();
    let domain = exact_selection_domain(&mut arena, catalog.skeleton_count(), &[]);
    let board = occupancy.for_domain(&mut arena, domain, &control).unwrap();
    let original_owner = arena.owner_id();
    arena.set_private_compaction_threshold_for_test(1);
    assert!(arena.private_compaction_due());
    let templates = SharedReachabilityTemplates::new(4, 2, KickTableProfileId::Jstris180);
    let relation = board
        .compile_piece(&mut arena, &templates, PieceKind::I, &control)
        .unwrap();
    let row = catalog.skeleton_id(PieceKind::I, 0xf).unwrap();
    let legal = relation.legal_row(&mut arena, row, &control).unwrap();
    assert!(value(&arena, legal, &[]));
    assert_eq!(arena.owner_id(), original_owner);
    assert_eq!(arena.compaction_count(), 0);
}

#[test]
fn symbolic_prefix_board_preserves_preexisting_fork_private_roots() {
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(4, 2, 0, 0xff).unwrap();
    let control = ExecutionControl::default();
    let mut parent =
        BooleanArena::new(catalog.skeleton_count() as u32, BooleanLimits::default()).unwrap();
    let occupancy = SymbolicOccupancy::compile(&catalog, &mut parent, &control).unwrap();
    let domain = exact_selection_domain(&mut parent, catalog.skeleton_count(), &[]);
    let board = occupancy.for_domain(&mut parent, domain, &control).unwrap();
    let prefix = parent.freeze_prefix().unwrap();
    let mut arena = BooleanArena::fork_prefix(prefix).unwrap();
    let first = arena.variable(0).unwrap();
    let second = arena.variable(1).unwrap();
    let existing = arena.and(first, second).unwrap();
    assert!(arena.private_node_count() > 0);
    arena.set_private_compaction_threshold_for_test(1);
    let original_owner = arena.owner_id();
    let templates = SharedReachabilityTemplates::new(4, 2, KickTableProfileId::Jstris180);
    let relation = board
        .compile_piece(&mut arena, &templates, PieceKind::I, &control)
        .unwrap();
    let row = catalog.skeleton_id(PieceKind::I, 0xf).unwrap();
    let legal = relation.legal_row(&mut arena, row, &control).unwrap();
    assert!(value(&arena, legal, &[]));
    assert!(arena.evaluate(existing, |variable| variable <= 1));
    assert!(!arena.evaluate(existing, |variable| variable == 0));
    assert_eq!(arena.owner_id(), original_owner);
    assert_eq!(arena.compaction_count(), 0);
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
    let mut cells = BooleanArena::new(20, BooleanLimits::default()).unwrap();
    let cell_occupancy = SymbolicOccupancy::compile_cells(&catalog, &mut cells, &control).unwrap();
    let cell_domain = finite_cell_domain(&mut cells, 20, &[INITIAL]);
    let cell_board = cell_occupancy
        .for_domain(&mut cells, cell_domain, &control)
        .unwrap();
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
            let unseeded = board
                .compile_piece_with_vertical_seed::<false>(
                    &mut arena,
                    &templates,
                    PieceKind::T,
                    domain,
                    &control,
                )
                .unwrap();
            assert_eq!(relation.locks, unseeded.locks);
            let outside = arena.not(domain).unwrap();
            for &lock in &relation.locks {
                assert_eq!(arena.and(lock, outside).unwrap(), BooleanId::FALSE);
            }
            let root = relation.legal_row(&mut arena, target, &control).unwrap();
            let actual = value(&arena, root, &[]);
            let cell_relation = cell_board
                .compile_piece(&mut cells, &templates, PieceKind::T, &control)
                .unwrap();
            let cell_root = cell_relation
                .legal_row(&mut cells, target, &control)
                .unwrap();
            assert_eq!(cell_value(&cells, cell_root, INITIAL), actual);
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
