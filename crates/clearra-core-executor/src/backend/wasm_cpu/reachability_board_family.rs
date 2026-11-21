//! Executable combinatorial proof of board-family reachability.
//!
//! Each pose retains the Boolean family of boards that can reach it. The
//! compiler never expands the assignments of that family. Collision, ordered
//! kick, seed and grounding semantics come from the existing movement owner.
//! Only the differential tests enumerate assignments, against the independent
//! cell-by-cell interpreter. This module is test-only; it changes no product
//! admission, fallback, or probability result.

use std::collections::VecDeque;

use clearra_core_domain::execution_cancellation::ExecutionControl;

use super::{
    boolean_family::{BooleanFamily, FamilyId, Refused},
    independence::edge_possible_in_cube,
    state_from_index, ReachabilityTemplate, INVALID_STATE_INDEX, INVALID_STATE_MASK,
};

const MAX_POSES: usize = 4096;
const MAX_EDGES: usize = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FamilyFailure {
    Unsupported,
    Cancelled,
    WorkLimit,
    Allocation,
    Boolean(Refused),
}

impl From<Refused> for FamilyFailure {
    fn from(value: Refused) -> Self {
        Self::Boolean(value)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FamilyWork {
    potential_edges: usize,
    cube_rejected_edges: usize,
    symbolic_edges: usize,
    propagation_edges: usize,
    family_updates: usize,
}

#[derive(Clone, Copy)]
struct Edge {
    target: usize,
    guard: FamilyId,
}

struct BoardFamilyRelation {
    lock_families: Vec<FamilyId>,
    work: FamilyWork,
}

/// Board bits may themselves be arbitrary Boolean functions, for example the
/// original-row compaction of an operation subset. The constant cube below is
/// only an overapproximation used to discard identically impossible edges.
fn compile(
    template: &ReachabilityTemplate,
    family: &mut BooleanFamily,
    occupied: &[FamilyId; 64],
    control: &ExecutionControl,
    propagation_limit: usize,
) -> Result<BoardFamilyRelation, FamilyFailure> {
    if control.is_cancelled() {
        return Err(FamilyFailure::Cancelled);
    }
    let area = usize::from(template.width) * usize::from(template.height);
    let count = template.state_masks.len();
    if !(1..=10).contains(&template.width)
        || !(1..=6).contains(&template.height)
        || area > 64
        || !(1..=MAX_POSES).contains(&count)
        || occupied[area..].iter().any(|&root| root != FamilyId::FALSE)
    {
        return Err(FamilyFailure::Unsupported);
    }
    let mut fixed = 0_u64;
    let mut variable = 0_u64;
    for (cell, &root) in occupied[..area].iter().enumerate() {
        if root == FamilyId::TRUE {
            fixed |= 1_u64 << cell;
        } else if root != FamilyId::FALSE {
            variable |= 1_u64 << cell;
        }
    }

    let mut free = Vec::new();
    free.try_reserve_exact(count)
        .map_err(|_| FamilyFailure::Allocation)?;
    for &mask in &template.state_masks {
        let mut root = FamilyId::TRUE;
        if mask == INVALID_STATE_MASK {
            root = FamilyId::FALSE;
        } else {
            let mut remaining = mask;
            while remaining != 0 {
                let cell = remaining.trailing_zeros() as usize;
                let empty = family.not(occupied[cell])?;
                root = family.and(root, empty)?;
                remaining &= remaining - 1;
            }
        }
        free.push(root);
    }

    let mut outgoing: Vec<Vec<Edge>> = Vec::new();
    outgoing
        .try_reserve_exact(count)
        .map_err(|_| FamilyFailure::Allocation)?;
    outgoing.resize_with(count, Vec::new);
    let mut work = FamilyWork::default();
    for source in 0..count {
        if control.is_cancelled() {
            return Err(FamilyFailure::Cancelled);
        }
        let source_mask = template.state_masks[source];
        if source_mask == INVALID_STATE_MASK {
            continue;
        }
        for &target in &template.translation_targets[source] {
            if target == INVALID_STATE_INDEX {
                continue;
            }
            let target = usize::from(target);
            work.potential_edges += 1;
            if !edge_possible_in_cube(
                source_mask,
                template.state_masks[target],
                std::iter::empty(),
                fixed,
                variable,
            ) {
                work.cube_rejected_edges += 1;
                continue;
            }
            let guard = family.and(free[source], free[target])?;
            append_edge(&mut outgoing[source], target, guard, &mut work)?;
        }
        for slot in 0..if template.allow_180 { 3 } else { 2 } {
            let transition = source * 3 + slot;
            let start = template.rotation_target_offsets[transition] as usize;
            let end = template.rotation_target_offsets[transition + 1] as usize;
            let mut prior_blocked = FamilyId::TRUE;
            for offset in start..end {
                let target = usize::from(template.rotation_targets[offset]);
                let prior_masks = template.rotation_targets[start..offset]
                    .iter()
                    .map(|&earlier| template.state_masks[usize::from(earlier)]);
                work.potential_edges += 1;
                if edge_possible_in_cube(
                    source_mask,
                    template.state_masks[target],
                    prior_masks,
                    fixed,
                    variable,
                ) {
                    let pair_free = family.and(free[source], free[target])?;
                    let guard = family.and(pair_free, prior_blocked)?;
                    append_edge(&mut outgoing[source], target, guard, &mut work)?;
                } else {
                    work.cube_rejected_edges += 1;
                }
                // An impossible edge can still have a free destination that
                // suppresses a later kick. Never delete this earlier read.
                let blocked = family.not(free[target])?;
                prior_blocked = family.and(prior_blocked, blocked)?;
            }
        }
    }

    let seeds = template.sky_seeds.iter().map(|&seed| {
        let seed = usize::from(seed);
        (seed, free[seed])
    });
    let reachable = propagate(
        family,
        &outgoing,
        seeds,
        control,
        propagation_limit,
        &mut work,
    )?;
    let mut locks = vec![FamilyId::FALSE; count];
    for target in 0..count {
        let pose = state_from_index(template.width, template.ceiling, target);
        // Clipped sky masks are collision tests, not complete lock identities.
        if pose.y >= template.height as i8
            || template.state_masks[target] == INVALID_STATE_MASK
            || template.state_masks[target].count_ones() != 4
        {
            continue;
        }
        let down = template.translation_targets[target][0];
        let grounded = if pose.y == 0 || down == INVALID_STATE_INDEX {
            FamilyId::TRUE
        } else {
            family.not(free[usize::from(down)])?
        };
        locks[target] = family.and(reachable[target], grounded)?;
    }
    Ok(BoardFamilyRelation {
        lock_families: locks,
        work,
    })
}

fn append_edge(
    outgoing: &mut Vec<Edge>,
    target: usize,
    guard: FamilyId,
    work: &mut FamilyWork,
) -> Result<(), FamilyFailure> {
    if guard == FamilyId::FALSE {
        return Ok(());
    }
    if work.symbolic_edges >= MAX_EDGES {
        return Err(FamilyFailure::WorkLimit);
    }
    outgoing
        .try_reserve(1)
        .map_err(|_| FamilyFailure::Allocation)?;
    outgoing.push(Edge { target, guard });
    work.symbolic_edges += 1;
    Ok(())
}

fn propagate(
    family: &mut BooleanFamily,
    outgoing: &[Vec<Edge>],
    seeds: impl IntoIterator<Item = (usize, FamilyId)>,
    control: &ExecutionControl,
    limit: usize,
    work: &mut FamilyWork,
) -> Result<Vec<FamilyId>, FamilyFailure> {
    let mut reachable = vec![FamilyId::FALSE; outgoing.len()];
    let mut pending = reachable.clone();
    let mut queue = VecDeque::new();
    queue
        .try_reserve(outgoing.len())
        .map_err(|_| FamilyFailure::Allocation)?;
    for (seed, root) in seeds {
        let old_pending = pending[seed];
        reachable[seed] = family.or(reachable[seed], root)?;
        pending[seed] = family.or(pending[seed], root)?;
        if old_pending == FamilyId::FALSE && pending[seed] != FamilyId::FALSE {
            queue.push_back(seed);
        }
    }
    while let Some(source) = queue.pop_front() {
        if control.is_cancelled() {
            return Err(FamilyFailure::Cancelled);
        }
        let delta = std::mem::replace(&mut pending[source], FamilyId::FALSE);
        for edge in &outgoing[source] {
            if work.propagation_edges >= limit {
                return Err(FamilyFailure::WorkLimit);
            }
            work.propagation_edges += 1;
            let contribution = family.and(delta, edge.guard)?;
            let unseen = family.not(reachable[edge.target])?;
            let novel = family.and(contribution, unseen)?;
            if novel == FamilyId::FALSE {
                continue;
            }
            reachable[edge.target] = family.or(reachable[edge.target], novel)?;
            let old_pending = pending[edge.target];
            pending[edge.target] = family.or(old_pending, novel)?;
            if old_pending == FamilyId::FALSE {
                queue.push_back(edge.target);
            }
            work.family_updates += 1;
        }
    }
    Ok(reachable)
}

/// Symbolic original-row compaction. select[r][y] means original row r is
/// alive and has physical rank y. Recurrence is over rows, not 2^n subsets.
struct RowPullback {
    occupied: [FamilyId; 64],
    select: Vec<Vec<FamilyId>>,
    deleted: Vec<FamilyId>,
}

fn compile_row_pullback(
    family: &mut BooleanFamily,
    width: u8,
    height: u8,
    base: u64,
    footprints: &[u64],
) -> Result<RowPullback, FamilyFailure> {
    let area = usize::from(width) * usize::from(height);
    if !(1..=10).contains(&width)
        || !(1..=6).contains(&height)
        || area > 64
        || footprints.len() > 64
    {
        return Err(FamilyFailure::Unsupported);
    }
    let all = u64::MAX >> (64 - area);
    if base & !all != 0 {
        return Err(FamilyFailure::Unsupported);
    }
    let mut used = base;
    for &footprint in footprints {
        if footprint.count_ones() != 4 || footprint & used != 0 || footprint & !all != 0 {
            return Err(FamilyFailure::Unsupported);
        }
        used |= footprint;
    }
    let mut operations = Vec::new();
    for operation in 0..footprints.len() {
        operations.push(family.variable(operation as u8)?);
    }
    let mut original = [FamilyId::FALSE; 64];
    for (cell, root) in original[..area].iter_mut().enumerate() {
        if base & (1_u64 << cell) != 0 {
            *root = FamilyId::TRUE;
        } else if let Some(owner) = footprints
            .iter()
            .position(|mask| mask & (1_u64 << cell) != 0)
        {
            *root = operations[owner];
        }
    }
    let row_mask = (1_u64 << width) - 1;
    let mut deleted = vec![FamilyId::FALSE; usize::from(height)];
    for row in 0..usize::from(height) {
        let mask = row_mask << (row * usize::from(width));
        if base & mask == mask {
            return Err(FamilyFailure::Unsupported);
        }
        if used & mask != mask {
            continue;
        }
        let mut root = FamilyId::TRUE;
        for (owner, &footprint) in footprints.iter().enumerate() {
            if footprint & mask != 0 {
                root = family.and(root, operations[owner])?;
            }
        }
        deleted[row] = root;
    }

    let rows = usize::from(height);
    let mut rank = vec![FamilyId::FALSE; rows + 1];
    rank[0] = FamilyId::TRUE;
    let mut select = vec![vec![FamilyId::FALSE; rows]; rows];
    let mut occupied = [FamilyId::FALSE; 64];
    for original_row in 0..rows {
        let alive = family.not(deleted[original_row])?;
        let mut next = vec![FamilyId::FALSE; rows + 1];
        for physical_row in 0..=original_row {
            let removed = family.and(rank[physical_row], deleted[original_row])?;
            next[physical_row] = family.or(next[physical_row], removed)?;
            let kept = family.and(rank[physical_row], alive)?;
            next[physical_row + 1] = family.or(next[physical_row + 1], kept)?;
            select[original_row][physical_row] = kept;
            for x in 0..usize::from(width) {
                let cell = original_row * usize::from(width) + x;
                let projected = physical_row * usize::from(width) + x;
                let present = family.and(kept, original[cell])?;
                occupied[projected] = family.or(occupied[projected], present)?;
            }
        }
        rank = next;
    }
    Ok(RowPullback {
        occupied,
        select,
        deleted,
    })
}

/// The caller binds `operation` to this footprint in `row` and supplies a
/// template of the same piece kind. Row selection, relation, and template
/// must share the physical frame; these are internal proof-checker inputs.
fn operation_family(
    family: &mut BooleanFamily,
    template: &ReachabilityTemplate,
    relation: &BoardFamilyRelation,
    row: &RowPullback,
    footprint: u64,
    operation: u8,
) -> Result<FamilyId, FamilyFailure> {
    let width = usize::from(template.width);
    let height = usize::from(template.height);
    let area = width * height;
    if width == 0
        || area == 0
        || area > 64
        || row.select.len() != height
        || row.select.iter().any(|selected| selected.len() != height)
        || relation.lock_families.len() != template.state_masks.len()
        || (area < 64 && footprint >> area != 0)
    {
        return Err(FamilyFailure::Unsupported);
    }
    let original_cells: Vec<usize> = (0..64)
        .filter(|cell| footprint & (1_u64 << cell) != 0)
        .collect();
    if original_cells.len() != 4 {
        return Err(FamilyFailure::Unsupported);
    }
    let variable = family.variable(operation)?;
    let pending = family.not(variable)?;
    let mut result = FamilyId::FALSE;
    for (target, &lock_root) in relation.lock_families.iter().enumerate() {
        if lock_root == FamilyId::FALSE {
            continue;
        }
        let target_mask = template.state_masks[target];
        let mut matches = pending;
        for &cell in &original_cells {
            let x = cell % width;
            let original_y = cell / width;
            let mut mapped = FamilyId::FALSE;
            for physical_y in 0..usize::from(template.height) {
                if target_mask & (1_u64 << (physical_y * width + x)) != 0 {
                    mapped = family.or(mapped, row.select[original_y][physical_y])?;
                }
            }
            matches = family.and(matches, mapped)?;
        }
        // Four distinct surviving cells and an injective rank map make this
        // full-footprint inclusion equality, with the template's fixed kind.
        let accepted = family.and(matches, lock_root)?;
        result = family.or(result, accepted)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        thread,
    };

    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_rules::kicks::KickTableProfileId;

    use super::*;
    use crate::reachability_reference::reference_spawn_lock_anchors;

    const PROFILES: [KickTableProfileId; 5] = [
        KickTableProfileId::NoKick,
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
    ];
    const PIECES: [PieceKind; 7] = [
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::L,
    ];

    fn cube_planes(family: &mut BooleanFamily, fixed: u64, cells: &[usize]) -> [FamilyId; 64] {
        let mut planes = [FamilyId::FALSE; 64];
        for (cell, plane) in planes.iter_mut().enumerate() {
            if fixed & (1_u64 << cell) != 0 {
                *plane = FamilyId::TRUE;
            }
        }
        for (variable, &cell) in cells.iter().enumerate() {
            assert_eq!(planes[cell], FamilyId::FALSE);
            planes[cell] = family.variable(variable as u8).unwrap();
        }
        planes
    }

    fn concrete_board(family: &BooleanFamily, planes: &[FamilyId; 64], assignment: u64) -> u64 {
        planes.iter().enumerate().fold(0, |board, (cell, &root)| {
            board | (u64::from(family.evaluate(root, assignment)) << cell)
        })
    }

    #[test]
    fn board_family_builtin_ceiling_profiles_satisfy_height_independent_conditions() {
        use super::super::{
            builtin_kick_profile, normalized_kick_delta, standard_tetromino_registry,
            KickTransition,
        };

        let mut checked_profiles = 0;
        let mut guarded_crossings = 0;
        for profile_id in PROFILES {
            let profile = builtin_kick_profile(profile_id).expect("builtin profile");
            for piece in PIECES {
                checked_profiles += 1;
                // The sky proof needs an anchor at or above H to have no occupied
                // cell below H. It also needs below-floor anchor rejection to
                // coincide with geometric invalidity.
                let definition = standard_tetromino_registry()
                    .get(piece)
                    .expect("standard tetromino");
                for rotation in RotationState::ALL {
                    let shape = definition.shape(rotation);
                    assert_eq!(shape.cells().iter().map(|cell| cell.y()).min(), Some(0));
                    assert_eq!(shape.cells().iter().map(|cell| cell.x()).min(), Some(0));
                }

                let mut transitions = Vec::new();
                for from in RotationState::ALL {
                    for to in [
                        from.clockwise(),
                        from.counter_clockwise(),
                        from.rotated_180(),
                    ] {
                        let transition = KickTransition::new(piece, from, to);
                        if transition.is_180() && !profile.supports_180() {
                            continue;
                        }
                        let Some(sequence) = profile.sequence_for(transition) else {
                            continue;
                        };
                        let deltas = sequence
                            .offsets()
                            .iter()
                            .map(|offset| {
                                let (dx, dy) = normalized_kick_delta(
                                    piece,
                                    from,
                                    to,
                                    offset.dx(),
                                    offset.dy(),
                                );
                                (i16::from(dx), i16::from(dy))
                            })
                            .collect::<Vec<_>>();
                        transitions.push((transition, deltas));
                    }
                }
                let down = transitions
                    .iter()
                    .flat_map(|(_, deltas)| deltas.iter())
                    .map(|&(_, dy)| (-dy).max(0))
                    .max()
                    .unwrap_or(0);
                let up = transitions
                    .iter()
                    .flat_map(|(_, deltas)| deltas.iter())
                    .map(|&(_, dy)| dy.max(0))
                    .max()
                    .unwrap_or(0);
                assert!(
                    up <= down,
                    "{profile_id:?}/{piece:?}: low-source candidates may exceed H+D: U={up}, D={down}"
                );

                // If a sky source H+t has a candidate below H, then t < D.
                // The inclusive endpoint also covers D=0 without special cases.
                for (transition, deltas) in &transitions {
                    for t in 0..=down {
                        for (j, &(target_dx, target_dy)) in deltas.iter().enumerate() {
                            if t + target_dy >= 0 {
                                continue;
                            }
                            let earlier = &deltas[..j];
                            let loses_earlier_sky_candidate =
                                earlier.iter().any(|&(_, dy)| t + dy > down);
                            if !loses_earlier_sky_candidate {
                                continue;
                            }
                            guarded_crossings += 1;
                            // The witness has the same target orientation and x
                            // as j, so j's horizontal validity implies its own.
                            // It is wholly sky and retained under ceiling H+D.
                            let retained_sky_witness = earlier
                                .iter()
                                .any(|&(dx, dy)| dx == target_dx && 0 <= t + dy && t + dy <= down);
                            assert!(
                                retained_sky_witness,
                                "{profile_id:?}/{transition:?}: truncated earlier kick could enable a false sky-to-low edge; D={down}, t={t}, j={j}, deltas={deltas:?}"
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(checked_profiles, 35);
        assert!(guarded_crossings > 0);
        eprintln!("height-independent ceiling qualification: profiles_and_pieces={checked_profiles}, guarded_crossings={guarded_crossings}");
    }

    #[test]
    fn board_family_custom_upward_kick_exposes_an_unqualified_ceiling() {
        // This is a counterexample to extending the builtin ceiling theorem
        // to arbitrary profiles without its finite-table premises.
        // The first O candidate is always free in unbounded sky. Truncating
        // it allows a horizontal jump into a sealed, grounded cavity.
        let kicks: &[(i8, i8)] = &[(0, 7), (4, 0)];
        let wall = (0..5).fold(0_u64, |board, y| board | (0b001100 << (6 * y)));
        let board = wall | (0b110000 << 12);
        let mut answers = Vec::new();
        for ceiling in [5, 12] {
            let template = ReachabilityTemplate::compile_normalized(
                6,
                5,
                PieceKind::O,
                true,
                ceiling,
                [kicks; 12],
            );
            let mut family = BooleanFamily::new(0, 64).unwrap();
            let planes = cube_planes(&mut family, board, &[]);
            let relation = compile(
                &template,
                &mut family,
                &planes,
                &ExecutionControl::default(),
                100_000,
            )
            .unwrap();
            let target = super::super::state_index(
                6,
                ceiling,
                super::super::State {
                    rotation: RotationState::Zero,
                    x: 4,
                    y: 0,
                },
            )
            .unwrap();
            answers.push(family.evaluate(relation.lock_families[target], 0));
        }
        // H+7 preserves all low-source candidates. Sky-to-low movement here
        // is down-only, so its last-sky suffix agrees with unbounded sky.
        assert_eq!(answers, [true, false]);
    }

    #[test]
    fn board_family_keeps_correlations_that_an_unlabelled_edge_union_loses() {
        let mut family = BooleanFamily::new(1, 100).unwrap();
        let x = family.variable(0).unwrap();
        let not_x = family.not(x).unwrap();
        let outgoing = vec![
            vec![Edge {
                target: 1,
                guard: x,
            }],
            vec![Edge {
                target: 2,
                guard: not_x,
            }],
            vec![],
        ];
        let roots = propagate(
            &mut family,
            &outgoing,
            [(0, FamilyId::TRUE)],
            &ExecutionControl::default(),
            100,
            &mut FamilyWork::default(),
        )
        .unwrap();
        assert_ne!(x, FamilyId::FALSE);
        assert_ne!(not_x, FamilyId::FALSE);
        assert_eq!(roots[2], FamilyId::FALSE);
    }

    #[test]
    fn board_family_height_one_to_six_all_pieces_profiles_match_primitive_oracle() {
        let mut cases = Vec::new();
        for height in 1..=6 {
            for profile in PROFILES {
                for piece in PIECES {
                    cases.push((height, profile, piece));
                }
            }
        }
        let next = AtomicUsize::new(0);
        let workers = thread::available_parallelism().unwrap().get();
        let totals = thread::scope(|scope| {
            let handles: Vec<_> = (0..workers).map(|_| scope.spawn(|| {
                let mut boards = 0_usize;
                let mut lock_checks = 0_usize;
                let mut skipped_edges = 0_usize;
                let mut peak_nodes = 0_usize;
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(height, profile, piece)) = cases.get(index) else { break; };
                    let template = ReachabilityTemplate::compile(4, height, piece, profile);
                    let area = 4 * usize::from(height);
                    let mut cells: Vec<_> = (0..4).chain(area.saturating_sub(4)..area).collect();
                    cells.sort_unstable();
                    cells.dedup();
                    let mut family = BooleanFamily::new(cells.len() as u8, 100_000).unwrap();
                    family.set_work_limit(20_000_000);
                    let planes = cube_planes(&mut family, 0, &cells);
                    let relation = compile(&template, &mut family, &planes, &ExecutionControl::default(), 1_000_000)
                        .unwrap_or_else(|error| panic!("height={height} piece={piece:?} profile={profile:?}: {error:?}"));
                    skipped_edges += relation.work.cube_rejected_edges;
                    peak_nodes = peak_nodes.max(family.node_count());
                    for assignment in 0..(1_u64 << cells.len()) {
                        let board = concrete_board(&family, &planes, assignment);
                        let reference = reference_spawn_lock_anchors(4, height, board, piece, profile).unwrap();
                        boards += 1;
                        for (target, &mask) in template.state_masks.iter().enumerate() {
                            let pose = state_from_index(4, template.ceiling, target);
                            if mask == INVALID_STATE_MASK || mask.count_ones() != 4 || pose.y >= height as i8 {
                                continue;
                            }
                            let anchor = pose.y as usize * 4 + pose.x as usize;
                            let expected = reference[pose.rotation.quarter_turns() as usize] & (1_u64 << anchor) != 0;
                            let actual = family.evaluate(relation.lock_families[target], assignment);
                            assert_eq!(actual, expected, "height={height} piece={piece:?} profile={profile:?} board={board:#x} pose={pose:?}");
                            lock_checks += 1;
                        }
                    }
                }
                (boards, lock_checks, skipped_edges, peak_nodes)
            })).collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        let boards: usize = totals.iter().map(|t| t.0).sum();
        let checks: usize = totals.iter().map(|t| t.1).sum();
        let skipped: usize = totals.iter().map(|t| t.2).sum();
        let nodes = totals.iter().map(|t| t.3).max().unwrap();
        assert_eq!(boards, 45_360);
        assert!(checks > 100_000);
        assert!(skipped > 0);
        eprintln!("board-family differential: cases={}, workers={workers}, boards={boards}, full_lock_checks={checks}, cube_rejected_edges={skipped}, peak_bdd_nodes_per_case={nodes}", cases.len());
    }

    #[test]
    fn board_family_keeps_the_ordered_kick_occupancy_nonmonotonicity() {
        let template =
            ReachabilityTemplate::compile(10, 4, PieceKind::T, KickTableProfileId::Srs90);
        let mut family = BooleanFamily::new(1, 20_000).unwrap();
        let planes = cube_planes(&mut family, 0x4002, &[11]);
        let relation = compile(
            &template,
            &mut family,
            &planes,
            &ExecutionControl::default(),
            100_000,
        )
        .unwrap();
        let target = super::super::state_index(
            10,
            template.ceiling,
            super::super::State {
                rotation: RotationState::Zero,
                x: 2,
                y: 0,
            },
        )
        .unwrap();
        assert_eq!(template.state_masks[target], 0x201c);
        let root = relation.lock_families[target];
        assert!(!family.evaluate(root, 0));
        assert!(family.evaluate(root, 1));
        assert_eq!(family.support(root), 1);
    }

    #[test]
    fn board_family_budget_exhaustion_is_refusal_not_an_empty_family() {
        let template =
            ReachabilityTemplate::compile(4, 4, PieceKind::T, KickTableProfileId::SrsPlus);
        let mut family = BooleanFamily::new(1, 20_000).unwrap();
        let planes = cube_planes(&mut family, 0, &[0]);
        assert!(matches!(
            compile(
                &template,
                &mut family,
                &planes,
                &ExecutionControl::default(),
                0
            ),
            Err(FamilyFailure::WorkLimit)
        ));
        let mut family = BooleanFamily::new(1, 2).unwrap();
        assert_eq!(family.variable(0), Err(Refused::NodeLimit));
    }

    fn concrete_projection(
        width: usize,
        height: usize,
        board: u64,
    ) -> (u64, u64, Vec<Option<usize>>) {
        let row_mask = (1_u64 << width) - 1;
        let mut projected = 0_u64;
        let mut deleted = 0_u64;
        let mut destinations = vec![None; height];
        let mut destination = 0;
        for (row, mapped) in destinations.iter_mut().enumerate() {
            let value = (board >> (row * width)) & row_mask;
            if value == row_mask {
                deleted |= 1 << row;
            } else {
                *mapped = Some(destination);
                projected |= value << (destination * width);
                destination += 1;
            }
        }
        (projected, deleted, destinations)
    }

    fn check_row_pullback(width: u8, height: u8, base: u64, footprints: &[u64]) {
        let mut family = BooleanFamily::new(footprints.len() as u8, 100_000).unwrap();
        let row = compile_row_pullback(&mut family, width, height, base, footprints).unwrap();
        for subset in 0..(1_u64 << footprints.len()) {
            let original = footprints
                .iter()
                .enumerate()
                .fold(base, |board, (i, &mask)| {
                    board | if subset & (1 << i) != 0 { mask } else { 0 }
                });
            let (board, deleted, selected) =
                concrete_projection(usize::from(width), usize::from(height), original);
            assert_eq!(concrete_board(&family, &row.occupied, subset), board);
            for (r, &root) in row.deleted.iter().enumerate() {
                assert_eq!(family.evaluate(root, subset), deleted & (1 << r) != 0);
                for y in 0..usize::from(height) {
                    assert_eq!(
                        family.evaluate(row.select[r][y], subset),
                        selected[r] == Some(y)
                    );
                }
            }
        }
    }

    #[test]
    fn board_family_symbolic_row_induction_matches_independent_place_and_clear() {
        // Six ordinary O operations, including two rows above the 4L boundary.
        let mut footprints = Vec::new();
        for y in [0, 2, 4] {
            for x in [0, 2] {
                footprints.push((3_u64 << x | 3_u64 << (x + 4)) << (4 * y));
            }
        }
        check_row_pullback(4, 6, 0, &footprints);
        // One-row I base and non-full Build rows exercise the F restriction.
        check_row_pullback(4, 1, 0, &[0xf]);
        check_row_pullback(4, 3, 1 << 8, &[0x33, 0xcc]);
        // Inverse T/L support deadlock: original row1 is the T gap.
        let t = 0x207;
        let l = 0x1130;
        check_row_pullback(4, 4, 0xffff & !(t | l), &[t, l]);
    }

    #[test]
    fn board_family_pretiling_relation_pulls_back_without_prefix_reachability() {
        let template =
            ReachabilityTemplate::compile(2, 6, PieceKind::O, KickTableProfileId::Jstris180);
        let mut family = BooleanFamily::new(12, 100_000).unwrap();
        let raw_planes = cube_planes(&mut family, 0, &(0..12).collect::<Vec<_>>());
        let pretiling = compile(
            &template,
            &mut family,
            &raw_planes,
            &ExecutionControl::default(),
            200_000,
        )
        .unwrap();
        let footprints = [0xf, 0xf0, 0xf00];
        let row = compile_row_pullback(&mut family, 2, 6, 0, &footprints).unwrap();
        let replacements: Vec<_> = row.occupied[..12].to_vec();
        let pulled_locks = pretiling
            .lock_families
            .iter()
            .map(|&root| family.compose(root, &replacements).unwrap())
            .collect();
        let pulled = BoardFamilyRelation {
            lock_families: pulled_locks,
            work: FamilyWork::default(),
        };
        let operations: Vec<_> = footprints
            .iter()
            .enumerate()
            .map(|(i, &mask)| {
                operation_family(&mut family, &template, &pulled, &row, mask, i as u8).unwrap()
            })
            .collect();
        for subset in 0..8_u64 {
            // Every completed O immediately clears its two full rows. Thus
            // all physical boards coincide although target frames do not.
            assert_eq!(concrete_board(&family, &row.occupied, subset), 0);
            for (i, &root) in operations.iter().enumerate() {
                let expected = subset & (1 << i) == 0 && subset & ((1 << i) - 1) == (1 << i) - 1;
                assert_eq!(
                    family.evaluate(root, subset),
                    expected,
                    "subset={subset} operation={i}"
                );
                assert_eq!(family.support(root), (1_u64 << (i + 1)) - 1);
            }
        }
        eprintln!("pretiling pullback: height=6, raw_board_variables=12, operation_variables=3, prefix_assignments_checked=8, prefix_reachability_calls=0, bdd_nodes={}", family.node_count());
    }

    #[test]
    fn board_family_six_line_partial_clear_operation_guards_match_physics() {
        let template =
            ReachabilityTemplate::compile(4, 6, PieceKind::O, KickTableProfileId::SrsPlus);
        let mut footprints = Vec::new();
        for y in [0, 2, 4] {
            for x in [0, 2] {
                footprints.push((3_u64 << x | 3_u64 << (x + 4)) << (4 * y));
            }
        }
        let mut family = BooleanFamily::new(6, 100_000).unwrap();
        let row = compile_row_pullback(&mut family, 4, 6, 0, &footprints).unwrap();
        let relation = compile(
            &template,
            &mut family,
            &row.occupied,
            &ExecutionControl::default(),
            300_000,
        )
        .unwrap();
        let operations: Vec<_> = footprints
            .iter()
            .enumerate()
            .map(|(i, &mask)| {
                operation_family(&mut family, &template, &relation, &row, mask, i as u8).unwrap()
            })
            .collect();
        let mut checks = 0;
        for subset in 0..64_u64 {
            let board = concrete_board(&family, &row.occupied, subset);
            let reference = reference_spawn_lock_anchors(
                4,
                6,
                board,
                PieceKind::O,
                KickTableProfileId::SrsPlus,
            )
            .unwrap();
            let original = footprints.iter().enumerate().fold(0, |b, (i, &m)| {
                b | if subset & (1 << i) != 0 { m } else { 0 }
            });
            let (_, _, mapping) = concrete_projection(4, 6, original);
            for (i, &mask) in footprints.iter().enumerate() {
                let expected = if subset & (1 << i) != 0 {
                    false
                } else {
                    let anchor = mask.trailing_zeros() as usize;
                    let x = anchor % 4;
                    let y = mapping[anchor / 4].unwrap();
                    reference[0] & (1 << (y * 4 + x)) != 0
                };
                assert_eq!(
                    family.evaluate(operations[i], subset),
                    expected,
                    "subset={subset:#x}, operation={i}"
                );
                checks += 1;
            }
        }
        eprintln!("symbolic ILC pullback: height=6, operations=6, subsets=64, exact_operation_checks={checks}, symbolic_relation_compiles=1, per_prefix_relation_compiles=0, bdd_nodes={}", family.node_count());
    }
}
