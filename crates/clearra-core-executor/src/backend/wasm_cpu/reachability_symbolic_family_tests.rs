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
        for piece in PieceKind::STANDARD_TETROMINOES {
            let relation = board
                .compile_piece(&mut arena, &templates, piece, &control)
                .unwrap();
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

fn ordered_profile(reorder: bool) -> VerifiedKickTableProfile {
    let selected = KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
    let mut entries = SrsKicks::profile().entries().to_vec();
    if reorder {
        let entry = entries
            .iter_mut()
            .find(|entry| entry.transition() == selected)
            .unwrap();
        let mut offsets = entry.sequence().offsets().to_vec();
        offsets[..=2].rotate_right(1);
        *entry = KickTableEntry::new(selected, KickOffsetSequence::new(offsets));
    }
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
    let mut answers = Vec::new();
    for reordered in [false, true] {
        let profile = ordered_profile(reordered);
        let authority =
            CompactKickAuthority::admit_profile(10, 2, RuleProfileId::Srs, &profile).unwrap();
        let templates = SharedReachabilityTemplates::with_authority(authority);
        let relation = board
            .compile_piece(&mut arena, &templates, PieceKind::T, &control)
            .unwrap();
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
        answers.push(actual);
    }
    assert_eq!(answers, [false, true]);
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
