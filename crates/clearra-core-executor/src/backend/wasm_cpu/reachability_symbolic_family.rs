//! Exact lock predicates over a request's catalog row-selection variables.
//!
//! Original occupancy is compiled once. Row deletion, compaction and movement
//! are restricted to the caller's current partial-selection domain, rather
//! than compiling an unrestricted global legality relation. That domain may
//! project source variables away only for this physical calculation; the
//! caller must apply the returned predicate to its original joint relation.
//!
//! No complete candidate or operation-subset table is constructed here. The
//! only explicit Boolean-assignment table is the shared deleted-row frame,
//! with at most 64 entries for the existing six-row movement authority.

use std::collections::VecDeque;
use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_domain::piece::piece_kind::PieceKind;

use super::super::catalog::GeometryCatalog;
use super::super::symbolic_boolean::{BooleanArena, BooleanError, BooleanId};
use super::super::symbolic_movement_progress::{self, MovementPhase};
use super::{
    reverse_rotation_prior_targets, state_from_index, state_index, ReachabilityTemplate,
    ReverseRotationSource, SharedReachabilityTemplates, State, INVALID_STATE_INDEX,
    INVALID_STATE_MASK, NO_PRIOR_GUARD, ORDERED_GUARD, SINGLE_BIT_GUARD_BASE,
    SINGLE_MASK_GUARD_BASE,
};

const MAX_ROWS: usize = 6;

/// Request-lifetime expressions. Variable `r` selects catalog row ID `r`.
/// The caller reserves these variables before allocating source variables.
pub(in super::super) struct SymbolicOccupancy<'catalog> {
    catalog: &'catalog GeometryCatalog,
    arena_owner: u64,
    original: [BooleanId; 64],
}

/// Rank-lifetime expressions, all interpreted in this exact domain and arena.
/// Drop this value and its relations before rewinding the rank's arena nodes.
pub(in super::super) struct SymbolicBoard<'catalog> {
    catalog: &'catalog GeometryCatalog,
    arena_owner: u64,
    domain: BooleanId,
    occupied: [BooleanId; 64],
    deleted: [BooleanId; MAX_ROWS],
    frames: Vec<BooleanId>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in super::super) struct SymbolicReachabilityWork {
    pub poses: usize,
    /// Retained transfer descriptors, excluding already-impossible poses.
    /// These are static conditions, not materialized Boolean edge families.
    pub guarded_edges: usize,
    pub propagation_edges: u64,
    pub family_updates: u64,
    pub rotation_candidate_checks: u64,
    pub rotation_candidates_pruned: u64,
    pub rotation_guard_no_prior: u64,
    pub rotation_guard_single_bit: u64,
    pub rotation_guard_single_mask: u64,
    pub rotation_guard_ordered: u64,
    /// Original prior terms applied to source-pop deltas during propagation.
    pub rotation_ordered_prior_terms: u64,
}

/// One piece and one current board domain, shared by every matching catalog row.
pub(in super::super) struct SymbolicPieceRelation<'board, 'catalog> {
    board: &'board SymbolicBoard<'catalog>,
    arena_owner: u64,
    piece: PieceKind,
    template: Arc<ReachabilityTemplate>,
    locks: Vec<BooleanId>,
    work: SymbolicReachabilityWork,
}

impl<'catalog> SymbolicOccupancy<'catalog> {
    pub(in super::super) fn compile(
        catalog: &'catalog GeometryCatalog,
        arena: &mut BooleanArena,
        control: &ExecutionControl,
    ) -> Result<Self, BooleanError> {
        check_cancelled(control)?;
        let (width, height) = dimensions(catalog)?;
        let area = width * height;
        let board_mask = (1_u64 << area) - 1;
        let initial = catalog.initial_board();
        if initial & !board_mask != 0 || initial & catalog.required_cells() != 0 {
            return Err(BooleanError::InvalidInput("symbolic_initial_board"));
        }
        let row_mask = (1_u64 << width) - 1;
        if (0..height).any(|y| (initial >> (y * width)) & row_mask == row_mask) {
            return Err(BooleanError::Unsupported("symbolic_initial_full_row"));
        }
        let row_count = u32::try_from(catalog.skeleton_count())
            .map_err(|_| BooleanError::Unsupported("symbolic_catalog_row_count"))?;
        let mut original = [BooleanId::FALSE; 64];
        for (cell, root) in original[..area].iter_mut().enumerate() {
            if initial & (1_u64 << cell) != 0 {
                *root = BooleanId::TRUE;
            }
        }
        // Descending IDs prepend an earlier BDD variable to each cell's OR.
        // Building the same OR in ascending order needlessly rebuilds prefixes.
        for row_id in (0..row_count).rev() {
            check_cancelled(control)?;
            let row = catalog.skeleton(row_id);
            if row.cells.count_ones() != 4
                || row.cells & !board_mask != 0
                || row.cells & initial != 0
                || row.cells & !catalog.required_cells() != 0
                || !PieceKind::STANDARD_TETROMINOES.contains(&row.piece)
            {
                return Err(BooleanError::InvalidInput("symbolic_catalog_row"));
            }
            let selected = arena.variable(row_id)?;
            let mut cells = row.cells;
            while cells != 0 {
                let cell = cells.trailing_zeros() as usize;
                cells &= cells - 1;
                original[cell] = arena.or(selected, original[cell])?;
            }
        }
        Ok(Self {
            catalog,
            arena_owner: arena.owner_id(),
            original,
        })
    }

    /// Pull occupancy back to one exact live row-set domain. Overlapping
    /// catalog alternatives are allowed; the caller's Down(F) domain supplies
    /// their compatibility. This function does not grant geometric membership.
    pub(in super::super) fn for_domain(
        &self,
        arena: &mut BooleanArena,
        domain: BooleanId,
        control: &ExecutionControl,
    ) -> Result<SymbolicBoard<'catalog>, BooleanError> {
        self.check_owner(arena)?;
        check_cancelled(control)?;
        // This also validates a nonterminal domain before deriving any roots.
        let domain = arena.and(domain, BooleanId::TRUE)?;
        let (width, height) = dimensions(self.catalog)?;
        let mut deleted = [BooleanId::FALSE; MAX_ROWS];
        for (y, root) in deleted[..height].iter_mut().enumerate() {
            check_cancelled(control)?;
            let mut full = domain;
            for x in 0..width {
                full = arena.and(full, self.original[y * width + x])?;
                if full == BooleanId::FALSE {
                    break;
                }
            }
            *root = full;
        }

        // rank[y] describes assignments with y surviving original rows below
        // the current row. Every intermediate is already restricted to domain.
        let mut rank = [BooleanId::FALSE; MAX_ROWS + 1];
        rank[0] = domain;
        let mut occupied = [BooleanId::FALSE; 64];
        for original_y in 0..height {
            check_cancelled(control)?;
            let alive = arena.not(deleted[original_y])?;
            let mut next = [BooleanId::FALSE; MAX_ROWS + 1];
            for physical_y in 0..=original_y {
                let removed = arena.and(rank[physical_y], deleted[original_y])?;
                next[physical_y] = arena.or(next[physical_y], removed)?;
                let kept = arena.and(rank[physical_y], alive)?;
                next[physical_y + 1] = arena.or(next[physical_y + 1], kept)?;
                if kept == BooleanId::FALSE {
                    continue;
                }
                for x in 0..width {
                    let present = arena.and(kept, self.original[original_y * width + x])?;
                    let cell = physical_y * width + x;
                    occupied[cell] = arena.or(occupied[cell], present)?;
                }
            }
            rank = next;
        }

        // This small, request-domain frame table is shared across kinds and
        // rows. It enumerates H row-clear bits, never catalog selections.
        let mut frames = filled_vec(1 << height, BooleanId::FALSE)?;
        for (mask, root) in frames.iter_mut().enumerate() {
            let mut matches = domain;
            for (y, &deleted_row) in deleted[..height].iter().enumerate() {
                let condition = if mask & (1 << y) != 0 {
                    deleted_row
                } else {
                    arena.not(deleted_row)?
                };
                matches = arena.and(matches, condition)?;
                if matches == BooleanId::FALSE {
                    break;
                }
            }
            *root = matches;
        }
        Ok(SymbolicBoard {
            catalog: self.catalog,
            arena_owner: self.arena_owner,
            domain,
            occupied,
            deleted,
            frames,
        })
    }

    fn check_owner(&self, arena: &BooleanArena) -> Result<(), BooleanError> {
        if self.arena_owner != arena.owner_id() {
            return Err(BooleanError::InvalidInput("symbolic_arena_owner"));
        }
        Ok(())
    }
}

impl<'catalog> SymbolicBoard<'catalog> {
    pub(in super::super) fn domain(&self) -> BooleanId {
        self.domain
    }

    pub(in super::super) fn occupied_cells(&self) -> &[BooleanId; 64] {
        &self.occupied
    }

    pub(in super::super) fn deleted_rows(&self) -> &[BooleanId] {
        &self.deleted[..usize::from(self.catalog.height())]
    }

    pub(in super::super) fn row_ids(&self, piece: PieceKind) -> impl Iterator<Item = u32> + '_ {
        (0..self.catalog.skeleton_count()).filter_map(move |row_id| {
            let row_id = row_id as u32;
            (self.catalog.skeleton(row_id).piece == piece).then_some(row_id)
        })
    }

    pub(in super::super) fn compile_piece<'board>(
        &'board self,
        arena: &mut BooleanArena,
        templates: &SharedReachabilityTemplates,
        piece: PieceKind,
        control: &ExecutionControl,
    ) -> Result<SymbolicPieceRelation<'board, 'catalog>, BooleanError> {
        self.check_owner(arena)?;
        check_cancelled(control)?;
        if !PieceKind::STANDARD_TETROMINOES.contains(&piece)
            || templates.width != self.catalog.width()
            || templates.height != self.catalog.height()
        {
            return Err(BooleanError::InvalidInput("symbolic_movement_owner"));
        }
        if templates.authority.is_none() && super::builtin_kick_profile(templates.profile).is_none()
        {
            return Err(BooleanError::Unsupported(
                "symbolic_unconnected_kick_profile",
            ));
        }
        // This owner binds the complete imported ordered table, not merely a
        // nominal profile ID, and supplies the exact existing sky authority.
        let template = templates.template(piece);
        let count = template.state_masks.len();
        let mut work = SymbolicReachabilityWork {
            poses: count,
            ..SymbolicReachabilityWork::default()
        };
        symbolic_movement_progress::report_phase(control, piece, MovementPhase::Free, arena);
        let outside_domain = arena.not(self.domain)?;
        let mut free = filled_vec(count, BooleanId::FALSE)?;
        for (source, root) in free.iter_mut().enumerate() {
            check_cancelled(control)?;
            let mut cells = template.state_masks[source];
            if cells == INVALID_STATE_MASK {
                continue;
            }
            // Occupancy already vanishes outside domain. Build only the
            // collision predicate here; seed admission supplies domain once.
            // Repeating domain can duplicate domain-conditioned intermediates.
            let mut allowed = BooleanId::TRUE;
            while cells != 0 && allowed != BooleanId::FALSE {
                let cell = cells.trailing_zeros() as usize;
                cells &= cells - 1;
                let empty = arena.not(self.occupied[cell])?;
                allowed = arena.and(allowed, empty)?;
            }
            // A collision predicate equal to !domain admits no in-domain
            // assignment. Its values outside domain are unobservable because
            // every reached/pending family remains inside domain.
            *root = if allowed == outside_domain {
                BooleanId::FALSE
            } else {
                allowed
            };
        }

        symbolic_movement_progress::report_phase(control, piece, MovementPhase::Edges, arena);
        let (offsets, edges) = compile_transfers(&template, &free, control, &mut work)?;
        symbolic_movement_progress::report_phase(control, piece, MovementPhase::Propagation, arena);
        let reachable = propagate(
            arena,
            self.domain,
            &offsets,
            &edges,
            &template.sky_seeds,
            &free,
            &self.occupied,
            &template.rotation_targets,
            control,
            &mut work,
        )?;
        symbolic_movement_progress::report_phase(control, piece, MovementPhase::Locks, arena);
        let mut locks = filled_vec(count, BooleanId::FALSE)?;
        for target in 0..count {
            check_cancelled(control)?;
            if reachable[target] == BooleanId::FALSE
                || template.state_masks[target] == INVALID_STATE_MASK
                || template.state_masks[target].count_ones() != 4
            {
                continue;
            }
            let pose = state_from_index(template.width, template.ceiling, target);
            if pose.y >= i16::from(template.height) {
                continue;
            }
            let down = template.translation_targets[target][0];
            locks[target] = if pose.y == 0 || down == INVALID_STATE_INDEX {
                reachable[target]
            } else {
                let blocked_below = arena.not(free[usize::from(down)])?;
                arena.and(reachable[target], blocked_below)?
            };
        }
        Ok(SymbolicPieceRelation {
            board: self,
            arena_owner: arena.owner_id(),
            piece,
            template,
            locks,
            work,
        })
    }

    fn check_owner(&self, arena: &BooleanArena) -> Result<(), BooleanError> {
        if !arena.can_read_owner(self.arena_owner) {
            return Err(BooleanError::InvalidInput("symbolic_arena_owner"));
        }
        Ok(())
    }
}

impl SymbolicPieceRelation<'_, '_> {
    /// Return domain ∩ !selected(row) ∩ exact ILC/grounded/reachable lock.
    /// Down(F) after adding the row remains the caller's geometric constraint.
    pub(in super::super) fn legal_row(
        &self,
        arena: &mut BooleanArena,
        row_id: u32,
        control: &ExecutionControl,
    ) -> Result<BooleanId, BooleanError> {
        if self.arena_owner != arena.owner_id() {
            return Err(BooleanError::InvalidInput("symbolic_relation_arena_owner"));
        }
        self.board.check_owner(arena)?;
        check_cancelled(control)?;
        let catalog = self.board.catalog;
        let row = catalog
            .try_skeleton(row_id)
            .ok_or(BooleanError::InvalidInput("symbolic_row_id"))?;
        if row.piece != self.piece {
            return Err(BooleanError::InvalidInput("symbolic_row_piece"));
        }
        let selected = arena.variable(row_id)?;
        let pending = arena.not(selected)?;
        let available = arena.and(self.board.domain, pending)?;
        if available == BooleanId::FALSE {
            return Ok(BooleanId::FALSE);
        }
        let mut result = BooleanId::FALSE;
        for (deleted, &frame) in self.board.frames.iter().enumerate() {
            if frame == BooleanId::FALSE {
                continue;
            }
            check_cancelled(control)?;
            let mut frame_locks = BooleanId::FALSE;
            for realization in catalog.instantiations(row_id, deleted as u16) {
                check_cancelled(control)?;
                let target = state_index(
                    self.template.width,
                    self.template.ceiling,
                    State {
                        rotation: realization.rotation,
                        x: realization.x,
                        y: i16::from(realization.lock_y),
                    },
                )
                .ok_or(BooleanError::InvalidInput("symbolic_ilc_pose"))?;
                if self.template.state_masks[target] != realization.lock_mask {
                    return Err(BooleanError::InvalidInput("symbolic_ilc_lock_mask"));
                }
                frame_locks = arena.or(frame_locks, self.locks[target])?;
            }
            let accepted = arena.and(frame, frame_locks)?;
            result = arena.or(result, accepted)?;
        }
        // Availability is common to every frame. Factoring it out avoids
        // rebuilding the row-specific gate for each physical realization.
        arena.and(available, result)
    }

    pub(in super::super) fn work(&self) -> SymbolicReachabilityWork {
        self.work
    }
}

#[derive(Clone, Copy)]
struct Edge {
    target: usize,
    guard: TransferGuard,
}

/// Conditions over the immutable topology, evaluated only on a live delta.
/// No variant owns a Boolean edge family or a copy of an ordered kick list.
#[derive(Clone, Copy)]
enum TransferGuard {
    TargetFree,
    SingleCell {
        cell: u8,
    },
    BlockedPriorPose {
        pose: u16,
    },
    Ordered {
        slot: u8,
        prior_offset: u32,
        prior_count: u32,
    },
}

/// There is deliberately no arena parameter: this stage retains topology
/// descriptors without constructing any Boolean conjunction or negation.
fn compile_transfers(
    template: &ReachabilityTemplate,
    free: &[BooleanId],
    control: &ExecutionControl,
    work: &mut SymbolicReachabilityWork,
) -> Result<(Vec<usize>, Vec<Edge>), BooleanError> {
    let count = free.len();
    let mut edges = Vec::new();
    let mut offsets = Vec::new();
    offsets
        .try_reserve_exact(count.checked_add(1).ok_or(BooleanError::CountOverflow)?)
        .map_err(|_| BooleanError::Allocation)?;
    for source in 0..count {
        check_cancelled(control)?;
        offsets.push(edges.len());
        if free[source] == BooleanId::FALSE {
            continue;
        }
        for &target in &template.translation_targets[source] {
            if target != INVALID_STATE_INDEX && free[usize::from(target)] != BooleanId::FALSE {
                append_edge(&mut edges, usize::from(target), TransferGuard::TargetFree)?;
            }
        }
        for slot in 0..if template.allow_180 { 3 } else { 2 } {
            let transition = source * 3 + slot;
            let prior_offset = template.rotation_target_offsets[transition];
            let start = prior_offset as usize;
            let end = template.rotation_target_offsets[transition + 1] as usize;
            for candidate in start..end {
                check_cancelled(control)?;
                increment(&mut work.rotation_candidate_checks)?;
                let target = usize::from(template.rotation_targets[candidate]);
                let Some(rotation) =
                    retained_rotation_candidate(template, source, slot, candidate - start, target)?
                else {
                    increment(&mut work.rotation_candidates_pruned)?;
                    continue;
                };
                if free[target] == BooleanId::FALSE {
                    continue;
                }
                let guard = match rotation.free_endpoint_guard {
                    ORDERED_GUARD => {
                        increment(&mut work.rotation_guard_ordered)?;
                        TransferGuard::Ordered {
                            slot: rotation.slot,
                            prior_offset,
                            prior_count: rotation.prior_target_count,
                        }
                    }
                    NO_PRIOR_GUARD => {
                        increment(&mut work.rotation_guard_no_prior)?;
                        TransferGuard::TargetFree
                    }
                    bit if bit < SINGLE_MASK_GUARD_BASE => {
                        increment(&mut work.rotation_guard_single_bit)?;
                        TransferGuard::SingleCell {
                            cell: bit - SINGLE_BIT_GUARD_BASE,
                        }
                    }
                    mask => {
                        increment(&mut work.rotation_guard_single_mask)?;
                        let prior = reverse_rotation_prior_targets(template, rotation);
                        let &pose = prior
                            .get(usize::from(mask - SINGLE_MASK_GUARD_BASE))
                            .ok_or(BooleanError::InvalidInput("symbolic_rotation_guard"))?;
                        TransferGuard::BlockedPriorPose { pose }
                    }
                };
                append_edge(&mut edges, target, guard)?;
            }
        }
    }
    offsets.push(edges.len());
    work.guarded_edges = edges.len();
    Ok((offsets, edges))
}

/// Reuse the template's global edge proof and endpoint-conditioned guard.
/// CSR fill visits source, slot and original candidate ordinal in increasing
/// order, so each target slice is sorted by this complete edge identity.
/// A missing descriptor is a globally pruned edge, never a missing earlier
/// first-success competitor: ORDERED guards still read the forward prefix.
fn retained_rotation_candidate(
    template: &ReachabilityTemplate,
    source: usize,
    slot: usize,
    prior_count: usize,
    target: usize,
) -> Result<Option<ReverseRotationSource>, BooleanError> {
    let source = u16::try_from(source).map_err(|_| BooleanError::CountOverflow)?;
    let slot = u8::try_from(slot).map_err(|_| BooleanError::CountOverflow)?;
    let prior_count = u32::try_from(prior_count).map_err(|_| BooleanError::CountOverflow)?;
    let start = template.reverse_rotation_offsets[target] as usize;
    let end = template.reverse_rotation_offsets[target + 1] as usize;
    let candidates = &template.reverse_rotation_sources[start..end];
    let index = candidates.binary_search_by_key(&(source, slot, prior_count), |candidate| {
        (
            candidate.source,
            candidate.slot,
            candidate.prior_target_count,
        )
    });
    Ok(index.ok().map(|index| candidates[index]))
}

fn increment(value: &mut u64) -> Result<(), BooleanError> {
    *value = value.checked_add(1).ok_or(BooleanError::CountOverflow)?;
    Ok(())
}

fn append_edge(
    edges: &mut Vec<Edge>,
    target: usize,
    guard: TransferGuard,
) -> Result<(), BooleanError> {
    edges.try_reserve(1).map_err(|_| BooleanError::Allocation)?;
    edges.push(Edge { target, guard });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn propagate(
    arena: &mut BooleanArena,
    domain: BooleanId,
    offsets: &[usize],
    edges: &[Edge],
    seeds: &[u16],
    free: &[BooleanId],
    occupied: &[BooleanId; 64],
    prior_targets: &[u16],
    control: &ExecutionControl,
    work: &mut SymbolicReachabilityWork,
) -> Result<Vec<BooleanId>, BooleanError> {
    let count = free.len();
    let mut reached = filled_vec(count, BooleanId::FALSE)?;
    let mut pending = filled_vec(count, BooleanId::FALSE)?;
    let mut queue = VecDeque::new();
    queue
        .try_reserve(count)
        .map_err(|_| BooleanError::Allocation)?;
    for &seed in seeds {
        check_cancelled(control)?;
        let seed = usize::from(seed);
        let admitted = arena.and(domain, free[seed])?;
        let was_pending = pending[seed] != BooleanId::FALSE;
        reached[seed] = arena.or(reached[seed], admitted)?;
        pending[seed] = arena.or(pending[seed], admitted)?;
        if !was_pending && pending[seed] != BooleanId::FALSE {
            queue.push_back(seed);
        }
    }
    while let Some(source) = queue.pop_front() {
        check_cancelled(control)?;
        let delta = std::mem::replace(&mut pending[source], BooleanId::FALSE);
        let mut prior_cursors = [None; 3];
        let mut remaining = [delta; 3];
        // Inductively delta is inside domain and the source's free family.
        // Every edge intersects it with target-free conditions, so no edge
        // needs another domain/source gate and no outside assignment enters.
        for edge in &edges[offsets[source]..offsets[source + 1]] {
            check_cancelled(control)?;
            work.propagation_edges = work
                .propagation_edges
                .checked_add(1)
                .ok_or(BooleanError::CountOverflow)?;
            let candidate = arena.and(delta, free[edge.target])?;
            if candidate == BooleanId::FALSE {
                continue;
            }
            let contribution = match edge.guard {
                TransferGuard::TargetFree => candidate,
                TransferGuard::SingleCell { cell } => {
                    arena.and(candidate, occupied[usize::from(cell)])?
                }
                TransferGuard::BlockedPriorPose { pose } => {
                    let blocked = arena.not(free[usize::from(pose)])?;
                    arena.and(candidate, blocked)?
                }
                TransferGuard::Ordered {
                    slot,
                    prior_offset,
                    prior_count,
                } => {
                    let slot = usize::from(slot);
                    let start = prior_offset as usize;
                    let end = start
                        .checked_add(prior_count as usize)
                        .ok_or(BooleanError::CountOverflow)?;
                    let cursor = prior_cursors[slot].get_or_insert(start);
                    // This prefix is target-independent and resets for each
                    // source-pop/slot. Never store candidate here: admitting
                    // this target must not erase a later kick's assignments.
                    // Read every original prior, including skipped descriptors,
                    // and advance each term at most once for this delta/slot.
                    while *cursor < end && remaining[slot] != BooleanId::FALSE {
                        check_cancelled(control)?;
                        let prior = usize::from(prior_targets[*cursor]);
                        let blocked = arena.not(free[prior])?;
                        remaining[slot] = arena.and(remaining[slot], blocked)?;
                        *cursor += 1;
                        increment(&mut work.rotation_ordered_prior_terms)?;
                    }
                    arena.and(candidate, remaining[slot])?
                }
            };
            if contribution == BooleanId::FALSE {
                continue;
            }
            let unseen = arena.not(reached[edge.target])?;
            let novel = arena.and(contribution, unseen)?;
            if novel == BooleanId::FALSE {
                continue;
            }
            reached[edge.target] = arena.or(reached[edge.target], novel)?;
            let was_pending = pending[edge.target] != BooleanId::FALSE;
            pending[edge.target] = arena.or(pending[edge.target], novel)?;
            if !was_pending {
                queue.push_back(edge.target);
            }
            work.family_updates = work
                .family_updates
                .checked_add(1)
                .ok_or(BooleanError::CountOverflow)?;
        }
    }
    Ok(reached)
}

fn dimensions(catalog: &GeometryCatalog) -> Result<(usize, usize), BooleanError> {
    let width = usize::from(catalog.width());
    let height = usize::from(catalog.height());
    if !(1..=10).contains(&width) || !(1..=MAX_ROWS).contains(&height) || width * height > 64 {
        return Err(BooleanError::Unsupported("symbolic_board_dimensions"));
    }
    Ok((width, height))
}

fn check_cancelled(control: &ExecutionControl) -> Result<(), BooleanError> {
    if control.is_cancelled() {
        return Err(BooleanError::Cancelled);
    }
    Ok(())
}

fn filled_vec<T: Clone>(length: usize, value: T) -> Result<Vec<T>, BooleanError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(length)
        .map_err(|_| BooleanError::Allocation)?;
    result.resize(length, value);
    Ok(result)
}

#[cfg(test)]
#[path = "reachability_symbolic_family_tests.rs"]
mod tests;
