//! Exact lock predicates over row-selection or original-cell variables.
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

#[derive(Clone, Copy, Eq, PartialEq)]
enum OccupancyVariableSpace {
    CatalogRows,
    OriginalCells,
}

/// Request-lifetime expressions in one explicitly chosen variable space.
/// Row mode maps variable r to catalog row r; cell mode maps c to original
/// board cell c. Source variables are never part of this physical relation.
pub(in super::super) struct SymbolicOccupancy<'catalog> {
    catalog: &'catalog GeometryCatalog,
    arena_owner: u64,
    variable_space: OccupancyVariableSpace,
    valid_domain: BooleanId,
    original: [BooleanId; 64],
}

/// Rank-lifetime expressions, all interpreted in this exact domain and arena.
/// Drop this value and its relations before rewinding the rank's arena nodes.
pub(in super::super) struct SymbolicBoard<'catalog> {
    catalog: &'catalog GeometryCatalog,
    arena_owner: u64,
    variable_space: OccupancyVariableSpace,
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
    /// Downward certificate intersections, each on an immutable topology edge.
    pub vertical_seed_edges: u64,
    pub vertical_seed_updates: u64,
    /// Transfers rejected before evaluating collision or ordered-kick guards.
    pub early_reached_rejections: u64,
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
    /// The supplied piece domain intersected with the board's base domain.
    domain: BooleanId,
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
        Self::compile_variables(catalog, arena, control, OccupancyVariableSpace::CatalogRows)
    }

    /// Compile primitive original-cell variables, without expanding a row
    /// family or building a row-to-cell relation. The caller supplies its exact
    /// cell image (or a proved superset) to for_domain and pulls predicates back
    /// to the original row/source relation before accepting an extension.
    pub(in super::super) fn compile_cells(
        catalog: &'catalog GeometryCatalog,
        arena: &mut BooleanArena,
        control: &ExecutionControl,
    ) -> Result<Self, BooleanError> {
        Self::compile_variables(
            catalog,
            arena,
            control,
            OccupancyVariableSpace::OriginalCells,
        )
    }

    fn compile_variables(
        catalog: &'catalog GeometryCatalog,
        arena: &mut BooleanArena,
        control: &ExecutionControl,
        variable_space: OccupancyVariableSpace,
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
        if variable_space == OccupancyVariableSpace::CatalogRows {
            for (cell, root) in original[..area].iter_mut().enumerate() {
                if initial & (1_u64 << cell) != 0 {
                    *root = BooleanId::TRUE;
                }
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
            if variable_space == OccupancyVariableSpace::CatalogRows {
                let selected = arena.variable(row_id)?;
                let mut cells = row.cells;
                while cells != 0 {
                    let cell = cells.trailing_zeros() as usize;
                    cells &= cells - 1;
                    original[cell] = arena.or(selected, original[cell])?;
                }
            }
        }
        let mut valid_domain = BooleanId::TRUE;
        if variable_space == OccupancyVariableSpace::OriginalCells {
            // Descending construction keeps this fixed-input constraint
            // linear in the board area. It does not enumerate occupied masks.
            for cell in (0..area).rev() {
                check_cancelled(control)?;
                let variable = cell as u32;
                original[cell] = arena.variable(variable)?;
                let bit = 1_u64 << cell;
                if initial & bit != 0 {
                    valid_domain = arena.mk(variable, BooleanId::FALSE, valid_domain)?;
                } else if catalog.required_cells() & bit == 0 {
                    valid_domain = arena.mk(variable, valid_domain, BooleanId::FALSE)?;
                }
            }
        }
        Ok(Self {
            catalog,
            arena_owner: arena.owner_id(),
            variable_space,
            valid_domain,
            original,
        })
    }

    /// Interpret occupancy in the chosen variable space and fixed-input scope.
    /// Row compatibility or an original-cell image remains the caller's
    /// authority. This function does not grant geometric membership.
    pub(in super::super) fn for_domain(
        &self,
        arena: &mut BooleanArena,
        domain: BooleanId,
        control: &ExecutionControl,
    ) -> Result<SymbolicBoard<'catalog>, BooleanError> {
        self.check_owner(arena)?;
        check_cancelled(control)?;
        // This also validates a nonterminal domain before deriving any roots.
        let domain = arena.and(domain, self.valid_domain)?;
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
            variable_space: self.variable_space,
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
    pub(in super::super) fn uses_cell_variables(&self) -> bool {
        self.variable_space == OccupancyVariableSpace::OriginalCells
    }

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
        self.compile_piece_in_domain(arena, templates, piece, self.domain, control)
    }

    /// Restrict movement to a source-admitted piece domain while sharing the
    /// base board's exact occupancy, deleted-row and frame expressions.
    pub(in super::super) fn compile_piece_in_domain<'board>(
        &'board self,
        arena: &mut BooleanArena,
        templates: &SharedReachabilityTemplates,
        piece: PieceKind,
        domain: BooleanId,
        control: &ExecutionControl,
    ) -> Result<SymbolicPieceRelation<'board, 'catalog>, BooleanError> {
        self.compile_piece_with_vertical_seed::<true>(arena, templates, piece, domain, control)
    }

    // The unseeded specialization exists only in finite correctness fixtures.
    // Both paths retain the same complete physical movement fixed point.
    fn compile_piece_with_vertical_seed<'board, const SEED_VERTICAL: bool>(
        &'board self,
        arena: &mut BooleanArena,
        templates: &SharedReachabilityTemplates,
        piece: PieceKind,
        domain: BooleanId,
        control: &ExecutionControl,
    ) -> Result<SymbolicPieceRelation<'board, 'catalog>, BooleanError> {
        self.check_owner(arena)?;
        // Only a board owned by the pinned prefix may outlive a moving fork
        // suffix. Preexisting private roots may belong to another caller, so
        // only a fresh suffix is owned completely by this piece operation.
        // Fork-local boards and reused scratch keep the non-collecting path.
        let collect_private =
            self.arena_owner != arena.owner_id() && arena.private_node_count() == 0;
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
        let mut domain = arena.and(self.domain, domain)?;
        // This owner binds the complete imported ordered table, not merely a
        // nominal profile ID, and supplies the exact existing sky authority.
        let template = templates.template(piece);
        let count = template.state_masks.len();
        let mut work = SymbolicReachabilityWork {
            poses: count,
            ..SymbolicReachabilityWork::default()
        };
        symbolic_movement_progress::report_phase(control, piece, MovementPhase::Free, arena);
        let mut outside_domain = arena.not(domain)?;
        let mut free = filled_vec(count, BooleanId::FALSE)?;
        for source in 0..count {
            check_cancelled(control)?;
            let mut cells = template.state_masks[source];
            if cells == INVALID_STATE_MASK {
                continue;
            }
            // Occupancy remains in the shared base domain. Build only the
            // collision predicate here; seeds supply this piece's domain.
            // Repeating domain can duplicate domain-conditioned intermediates.
            {
                let mut allowed = BooleanId::TRUE;
                while cells != 0 && allowed != BooleanId::FALSE {
                    let cell = cells.trailing_zeros() as usize;
                    cells &= cells - 1;
                    let empty = arena.not(self.occupied[cell])?;
                    allowed = arena.and(allowed, empty)?;
                }
                // A collision predicate equal to !domain admits no in-domain
                // assignment. Its values outside domain are unobservable
                // because every reached/pending family stays inside domain.
                free[source] = if allowed == outside_domain {
                    BooleanId::FALSE
                } else {
                    allowed
                };
            }
            if collect_movement_garbage(
                arena,
                collect_private,
                &mut domain,
                &mut free,
                &mut [],
                &mut [],
            )? {
                // This complement shares the domain's node. Recreate its ID
                // after relocation; no transfer-local root survives the call.
                outside_domain = arena.not(domain)?;
            }
        }

        collect_movement_garbage(
            arena,
            collect_private,
            &mut domain,
            &mut free,
            &mut [],
            &mut [],
        )?;
        symbolic_movement_progress::report_phase(control, piece, MovementPhase::Edges, arena);
        let (offsets, edges) = compile_transfers(&template, &free, control, &mut work)?;
        symbolic_movement_progress::report_phase(control, piece, MovementPhase::Propagation, arena);
        let reachable = propagate(
            arena,
            &mut domain,
            &offsets,
            &edges,
            &template.sky_seeds,
            &mut free,
            &self.occupied,
            &template.rotation_targets,
            SEED_VERTICAL.then_some(template.translation_targets.as_slice()),
            collect_private,
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
            domain,
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
    /// Return domain ∩ pending(row) ∩ exact ILC/grounded/reachable lock.
    /// Cell mode requires every original target cell to be empty: a surviving
    /// occupied cell collides, while a deleted target row has no realization.
    /// This also excludes an already-selected row after the exact pullback.
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
        let available = match self.board.variable_space {
            OccupancyVariableSpace::CatalogRows => {
                let selected = arena.variable(row_id)?;
                let pending = arena.not(selected)?;
                arena.and(self.domain, pending)?
            }
            OccupancyVariableSpace::OriginalCells => {
                let mut available = self.domain;
                let mut cells = row.cells;
                while cells != 0 && available != BooleanId::FALSE {
                    check_cancelled(control)?;
                    let cell = cells.trailing_zeros();
                    cells &= cells - 1;
                    let occupied = arena.variable(cell)?;
                    let empty = arena.not(occupied)?;
                    available = arena.and(available, empty)?;
                }
                available
            }
        };
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

/// Lift the existing positive sky/vertical-entry certificate to exact board
/// families. A descending pass closes only downward translations, so every
/// admitted assignment has an actual path from this template's sky seeds.
/// A failed certificate is unknown: all other translations and ordered kicks
/// remain in the subsequent fixed point. Row-selection correlations stay in
/// the Boolean roots; this does not assume independent occupied cells.
fn seed_vertical_closure(
    arena: &mut BooleanArena,
    domain: &mut BooleanId,
    free: &mut [BooleanId],
    translation_targets: &[[u16; 3]],
    reached: &mut [BooleanId],
    collect_private: bool,
    control: &ExecutionControl,
    work: &mut SymbolicReachabilityWork,
) -> Result<(), BooleanError> {
    if translation_targets.len() != reached.len() {
        return Err(BooleanError::InvalidInput("symbolic_vertical_topology"));
    }
    for source in (0..reached.len()).rev() {
        check_cancelled(control)?;
        let down = translation_targets[source][0];
        if down == INVALID_STATE_INDEX {
            continue;
        }
        let target = usize::from(down);
        // Existing state IDs increase with y within each rotation. Refuse a
        // changed topology instead of silently using an invalid DAG order.
        if target >= source {
            return Err(BooleanError::InvalidInput("symbolic_vertical_order"));
        }
        if reached[source] == BooleanId::FALSE || reached[source] == reached[target] {
            continue;
        }
        {
            increment(&mut work.vertical_seed_edges)?;
            let admitted = arena.and(reached[source], free[target])?;
            let combined = arena.or(reached[target], admitted)?;
            if combined != reached[target] {
                reached[target] = combined;
                increment(&mut work.vertical_seed_updates)?;
            }
        }
        collect_movement_garbage(arena, collect_private, domain, free, reached, &mut [])?;
    }
    Ok(())
}

/// Call only after all transfer-local roots are dead. Domains and free roots
/// survive propagation and must be remapped along with reached/pending roots.
/// The immutable board expressions are pinned by the checked prefix owner.
fn collect_movement_garbage(
    arena: &mut BooleanArena,
    eligible: bool,
    domain: &mut BooleanId,
    free: &mut [BooleanId],
    reached: &mut [BooleanId],
    pending: &mut [BooleanId],
) -> Result<bool, BooleanError> {
    if eligible && arena.private_compaction_due() {
        let mut roots = [std::slice::from_mut(domain), free, reached, pending];
        return Ok(arena.compact_private(&mut roots)?.compacted);
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn propagate(
    arena: &mut BooleanArena,
    domain: &mut BooleanId,
    offsets: &[usize],
    edges: &[Edge],
    seeds: &[u16],
    free: &mut [BooleanId],
    occupied: &[BooleanId; 64],
    prior_targets: &[u16],
    vertical_targets: Option<&[[u16; 3]]>,
    collect_private: bool,
    control: &ExecutionControl,
    work: &mut SymbolicReachabilityWork,
) -> Result<Vec<BooleanId>, BooleanError> {
    let count = free.len();
    let mut reached = filled_vec(count, BooleanId::FALSE)?;
    for &seed in seeds {
        check_cancelled(control)?;
        let seed = usize::from(seed);
        let admitted = arena.and(*domain, free[seed])?;
        reached[seed] = arena.or(reached[seed], admitted)?;
    }
    if let Some(translation_targets) = vertical_targets {
        seed_vertical_closure(
            arena,
            domain,
            free,
            translation_targets,
            &mut reached,
            collect_private,
            control,
            work,
        )?;
    }
    let mut pending = filled_vec(count, BooleanId::FALSE)?;
    pending.copy_from_slice(&reached);
    let mut queue = VecDeque::new();
    queue
        .try_reserve(count)
        .map_err(|_| BooleanError::Allocation)?;
    for (source, &root) in pending.iter().enumerate() {
        if root != BooleanId::FALSE {
            queue.push_back(source);
        }
    }
    while let Some(source) = queue.pop_front() {
        check_cancelled(control)?;
        {
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
                // Every delta is inside this piece's domain. A target that
                // already covers it cannot gain a world, even when delta is
                // only a proper subset. Reuse the existing root before any
                // Boolean work. Later ordered edges still consume every
                // original prior through their target-independent cursor.
                if reached[edge.target] == *domain {
                    increment(&mut work.early_reached_rejections)?;
                    continue;
                }
                // Reuse the already-proved target family before evaluating its
                // cell/kick dependencies. The target remains unchanged until this
                // transfer is admitted, so its contribution is already novel.
                let unseen = arena.not(reached[edge.target])?;
                let unexplored = arena.and(delta, unseen)?;
                if unexplored == BooleanId::FALSE {
                    increment(&mut work.early_reached_rejections)?;
                    continue;
                }
                let candidate = arena.and(unexplored, free[edge.target])?;
                if candidate == BooleanId::FALSE {
                    continue;
                }
                let novel = match edge.guard {
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
        // delta/remaining/candidate/novel are all out of scope. The complete
        // surviving private root set is exactly these four mutable slices.
        collect_movement_garbage(
            arena,
            collect_private,
            domain,
            free,
            &mut reached,
            &mut pending,
        )?;
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
