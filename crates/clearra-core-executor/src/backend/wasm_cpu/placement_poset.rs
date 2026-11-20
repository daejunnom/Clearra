//! Sufficient certificate for an exact placement-order language.
//!
//! First derive precedence constraints necessary for every grounded physical
//! execution. Then prove that every ideal transition of that poset has an ILC
//! realization with guaranteed grounding and either an empty vertical entry
//! corridor or an existing positive guard covering the whole board interval.
//! If both directions hold, the valid operation orders are exactly the linear
//! extensions of the poset. No placed-operation subsets or reachability states
//! are enumerated. Failure to obtain a certificate is always `None` (fallback),
//! never an infeasibility claim.
//!
//! The owning module gates this prototype to tests / `local-search-ab`. Callers
//! must additionally keep CountAll, finesse and non-clear-to-empty completion
//! on their existing paths. A certificate belongs to the supplied candidate;
//! its operation indices are concrete row IDs, not interchangeable kind IDs.

use clearra_core_domain::piece::piece_kind::PieceKind;

use super::{
    buildup::{compact_target_board, placement_is_grounded},
    catalog::{GeometryCatalog, InstantiatedRealization},
    geometry::GeometryCandidate,
    MAX_BOARD64_PIECES,
};

const MAX_HEIGHT: usize = 6;
const MAX_CLEAR_FRAMES: usize = 1 << MAX_HEIGHT;

#[derive(Clone, Debug)]
pub(super) struct CertifiedPlacementPoset {
    operation_count: usize,
    pieces: [PieceKind; MAX_BOARD64_PIECES],
    predecessors: [u16; MAX_BOARD64_PIECES],
    observed_guard_count: usize,
}

impl CertifiedPlacementPoset {
    pub(super) fn pieces(&self) -> &[PieceKind] {
        &self.pieces[..self.operation_count]
    }

    /// Transitive, acyclic necessary predecessor masks, strengthened by the
    /// separate proof that every available transition is physically valid.
    pub(super) fn predecessors(&self) -> &[u16] {
        &self.predecessors[..self.operation_count]
    }

    /// Number of operation / deleted-row frames certified by an observed
    /// positive guard instead of the direct hard-drop corridor proof.
    pub(super) fn observed_guard_count(&self) -> usize {
        self.observed_guard_count
    }
}

struct CandidateGeometry {
    width: u8,
    height: u8,
    initial_board: u64,
    operation_count: usize,
    all_operations: u16,
    pieces: [PieceKind; MAX_BOARD64_PIECES],
    cells: [u64; MAX_BOARD64_PIECES],
    row_contributors: [u16; MAX_HEIGHT],
    completed_rows: u16,
}

/// Try to certify an entire clear-to-empty candidate using only bounded
/// catalog / row-frame work. Scratch is fixed-size stack storage; there is no
/// heap allocation, CandidateProjection, BuildOrderGraph or primitive search.
#[cfg(test)]
pub(super) fn compile(
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
) -> Option<CertifiedPlacementPoset> {
    compile_with_guard(catalog, candidate, |_, _, _, _, _| false)
}

/// Extend the hard-drop certificate with already observed positive guards.
///
/// The callback receives kind, the exact ILC target pose, original deleted-row
/// mask, minimum physical board and maximum physical board. It may return true
/// only if one existing positive certificate covers the entire occupancy
/// interval under this catalog owner, kick profile and row frame. Independent
/// successful point queries at the two bounds are insufficient. The callback
/// must not start a primitive traversal; a miss remains Unknown / fallback.
pub(super) fn compile_with_guard(
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    mut observed_guard: impl FnMut(PieceKind, InstantiatedRealization, u16, u64, u64) -> bool,
) -> Option<CertifiedPlacementPoset> {
    let geometry = CandidateGeometry::compile(catalog, candidate)?;
    if geometry.has_unprovable_initial_support_disjunction(catalog, candidate) {
        return None;
    }

    let mut predecessors = [0_u16; MAX_BOARD64_PIECES];
    predecessors[..geometry.operation_count].fill(geometry.all_operations);
    let mut saw_grounding_witness = 0_u16;
    let mut instantiation_domains = [0_u64; MAX_BOARD64_PIECES];
    let mut required_by_frame = [0_u16; MAX_CLEAR_FRAMES];
    let mut possible_frames = 0_u64;

    for deleted in 0..1_u16 << geometry.height {
        let Some(required) = geometry.required_for_frame(deleted) else {
            continue;
        };
        required_by_frame[usize::from(deleted)] = required;
        possible_frames |= 1_u64 << deleted;
        let (initial, physical_cells) = geometry.project_frame(deleted);
        for operation in 0..geometry.operation_count {
            let operation_bit = 1_u16 << operation;
            if required & operation_bit != 0 {
                // An already completed original row contains this operation;
                // it cannot still be pending at this frame.
                continue;
            }
            let lock_mask = physical_cells[operation];
            let mut has_instantiation = false;
            for realization in catalog.instantiations(candidate.row_ids()[operation], deleted) {
                // ILC projects the physical lock back to precisely these
                // original cells. Keep this as an explicit certificate gate
                // if the catalog's representation changes in the future.
                if realization.lock_mask != lock_mask || lock_mask.count_ones() != 4 {
                    return None;
                }
                has_instantiation = true;
            }
            if !has_instantiation {
                continue;
            }
            instantiation_domains[operation] |= 1_u64 << deleted;

            if placement_is_grounded(geometry.width, initial, lock_mask) {
                predecessors[operation] &= required;
                saw_grounding_witness |= operation_bit;
                continue;
            }
            let support_cells = lock_mask >> geometry.width;
            for supporter in 0..geometry.operation_count {
                if supporter == operation || physical_cells[supporter] & support_cells == 0 {
                    continue;
                }
                let branch_required = required | (1_u16 << supporter);
                if geometry.deleted_rows(branch_required) != deleted {
                    continue;
                }
                // Every actual lock has some ground witness. Intersecting
                // requirements over all possible witnesses keeps only facts
                // necessary for every physical order; unexecutable witnesses
                // can weaken this intersection but cannot make it unsound.
                predecessors[operation] &= branch_required;
                saw_grounding_witness |= operation_bit;
            }
        }
    }
    if saw_grounding_witness != geometry.all_operations
        || !close_predecessors(&mut predecessors, geometry.operation_count)
    {
        return None;
    }

    let mut maximum_without = [0_u16; MAX_BOARD64_PIECES];
    for operation in 0..geometry.operation_count {
        let operation_bit = 1_u16 << operation;
        let mut excluded = operation_bit;
        for (other, &requirements) in predecessors[..geometry.operation_count].iter().enumerate() {
            if requirements & operation_bit != 0 {
                excluded |= 1_u16 << other;
            }
        }
        maximum_without[operation] = geometry.all_operations & !excluded;
    }

    let mut certified_operations = 0_u16;
    let mut observed_guard_count = 0;
    let mut pending_frames = possible_frames;
    while pending_frames != 0 {
        let deleted = pending_frames.trailing_zeros() as u16;
        pending_frames &= pending_frames - 1;
        let required = required_by_frame[usize::from(deleted)];
        let (initial, physical_cells) = geometry.project_frame(deleted);
        for operation in 0..geometry.operation_count {
            let minimum = predecessor_closure(predecessors[operation] | required, &predecessors);
            let mut maximum = maximum_without[operation];
            if minimum & !maximum != 0 || geometry.deleted_rows(minimum) != deleted {
                // No ideal with this D can make the operation available.
                continue;
            }
            for original_row in 0..usize::from(geometry.height) {
                let row_bit = 1_u16 << original_row;
                let contributors = geometry.row_contributors[original_row];
                if geometry.completed_rows & row_bit == 0
                    || deleted & row_bit != 0
                    || contributors & !maximum != 0
                {
                    continue;
                }
                let missing = contributors & !minimum;
                if missing.count_ones() == 1 {
                    // Negative row-clear guard: this row is not deleted and
                    // every contributor except q is forced present by min.
                    // Hence q and all of its successors must remain absent.
                    // Only the fixed min derives absences; shrinking max does
                    // not turn other unresolved contributors into absences.
                    maximum &= maximum_without[missing.trailing_zeros() as usize];
                }
            }
            if minimum & !maximum != 0 {
                continue;
            }
            if instantiation_domains[operation] & (1_u64 << deleted) == 0 {
                return None;
            }
            let lock_mask = physical_cells[operation];
            let minimum_board = board_for_operations(initial, &physical_cells, minimum);
            let maximum_board = board_for_operations(initial, &physical_cells, maximum);
            if !placement_is_grounded(geometry.width, minimum_board, lock_mask) {
                return None;
            }
            if maximum_board & vertical_entry_mask(lock_mask, geometry.width, geometry.height) != 0
            {
                let guarded = catalog
                    .instantiations(candidate.row_ids()[operation], deleted)
                    .any(|realization| {
                        observed_guard(
                            geometry.pieces[operation],
                            realization,
                            deleted,
                            minimum_board,
                            maximum_board,
                        )
                    });
                if !guarded {
                    return None;
                }
                observed_guard_count += 1;
            }
            // For every actual ideal E in this frame, min ⊆ E ⊆ max.
            // Compaction at fixed D preserves occupancy inclusion. Grounding
            // at min and an empty corridor at max imply a hard-drop edge at E.
            // Otherwise the callback supplies one existing positive proof for
            // the entire interval, including first-success kick conditions.
            // The candidate's disjoint original cells and exact ILC lock mask
            // also make place/clear commute with target compaction.
            certified_operations |= 1_u16 << operation;
        }
    }
    if certified_operations != geometry.all_operations {
        return None;
    }

    Some(CertifiedPlacementPoset {
        operation_count: geometry.operation_count,
        pieces: geometry.pieces,
        predecessors,
        observed_guard_count,
    })
}

impl CandidateGeometry {
    fn compile(catalog: &GeometryCatalog, candidate: &GeometryCandidate) -> Option<Self> {
        let width = catalog.width();
        let height = catalog.height();
        let operation_count = candidate.row_ids().len();
        if !(1..=10).contains(&width)
            || !(1..=MAX_HEIGHT as u8).contains(&height)
            || operation_count == 0
            || operation_count > MAX_BOARD64_PIECES
        {
            return None;
        }
        let board_mask = (1_u64 << (u32::from(width) * u32::from(height))) - 1;
        let initial_board = catalog.initial_board();
        let required_cells = catalog.required_cells();
        if (initial_board | required_cells) & !board_mask != 0
            || initial_board & required_cells != 0
        {
            return None;
        }
        let all_operations = (1_u16 << operation_count) - 1;
        let mut pieces = [PieceKind::I; MAX_BOARD64_PIECES];
        let mut cells = [0_u64; MAX_BOARD64_PIECES];
        let mut row_contributors = [0_u16; MAX_HEIGHT];
        let mut selected_cells = 0_u64;
        let row_mask = (1_u64 << width) - 1;
        for (operation, &row_id) in candidate.row_ids().iter().enumerate() {
            let row = catalog.try_skeleton(row_id)?;
            if row.cells.count_ones() != 4
                || row.cells & !required_cells != 0
                || row.cells & selected_cells != 0
                || !PieceKind::STANDARD_TETROMINOES.contains(&row.piece)
            {
                return None;
            }
            selected_cells |= row.cells;
            pieces[operation] = row.piece;
            cells[operation] = row.cells;
            for original_row in 0..usize::from(height) {
                if (row.cells >> (original_row * usize::from(width))) & row_mask != 0 {
                    row_contributors[original_row] |= 1_u16 << operation;
                }
            }
        }
        if selected_cells != required_cells {
            return None;
        }
        let final_board = initial_board | selected_cells;
        let mut completed_rows = 0_u16;
        for original_row in 0..usize::from(height) {
            let shift = original_row * usize::from(width);
            if (initial_board >> shift) & row_mask == row_mask {
                // Preserve CandidateProjection's preclear-normalization gate.
                return None;
            }
            if (final_board >> shift) & row_mask == row_mask {
                completed_rows |= 1_u16 << original_row;
            }
        }
        if compact_target_board(width, height, final_board, completed_rows) != 0 {
            return None;
        }
        Some(Self {
            width,
            height,
            initial_board,
            operation_count,
            all_operations,
            pieces,
            cells,
            row_contributors,
            completed_rows,
        })
    }

    fn has_unprovable_initial_support_disjunction(
        &self,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
    ) -> bool {
        for operation in 0..self.operation_count {
            let lock_mask = self.cells[operation];
            if placement_is_grounded(self.width, self.initial_board, lock_mask)
                || catalog
                    .instantiations(candidate.row_ids()[operation], 0)
                    .next()
                    .is_none()
            {
                continue;
            }
            let support_cells = lock_mask >> self.width;
            let mut supporters = 0;
            for supporter in 0..self.operation_count {
                if supporter != operation
                    && self.cells[supporter] & support_cells != 0
                    && self.deleted_rows(1_u16 << supporter) == 0
                {
                    supporters += 1;
                    if supporters == 2 {
                        // At D=0 the two singleton witness requirements have
                        // empty intersection. Further frames cannot strengthen
                        // that result, so pred[p]=0 and the later min=empty
                        // grounding proof must fail. Reject this certificate
                        // before doing the full D-indexed work.
                        return true;
                    }
                }
            }
        }
        false
    }

    fn deleted_rows(&self, operations: u16) -> u16 {
        let mut deleted = 0_u16;
        for original_row in 0..usize::from(self.height) {
            let row_bit = 1_u16 << original_row;
            if self.completed_rows & row_bit != 0
                && self.row_contributors[original_row] & !operations == 0
            {
                deleted |= row_bit;
            }
        }
        deleted
    }

    fn required_for_frame(&self, deleted: u16) -> Option<u16> {
        if deleted & !self.completed_rows != 0 {
            return None;
        }
        let mut required = 0_u16;
        let mut rows = deleted;
        while rows != 0 {
            let original_row = rows.trailing_zeros() as usize;
            rows &= rows - 1;
            required |= self.row_contributors[original_row];
        }
        (self.deleted_rows(required) == deleted).then_some(required)
    }

    fn project_frame(&self, deleted: u16) -> (u64, [u64; MAX_BOARD64_PIECES]) {
        let initial = compact_target_board(self.width, self.height, self.initial_board, deleted);
        let mut physical_cells = [0_u64; MAX_BOARD64_PIECES];
        for (operation, physical) in physical_cells[..self.operation_count]
            .iter_mut()
            .enumerate()
        {
            *physical =
                compact_target_board(self.width, self.height, self.cells[operation], deleted);
        }
        (initial, physical_cells)
    }
}

fn close_predecessors(predecessors: &mut [u16; MAX_BOARD64_PIECES], count: usize) -> bool {
    for intermediate in 0..count {
        let intermediate_bit = 1_u16 << intermediate;
        let inherited = predecessors[intermediate];
        for requirements in &mut predecessors[..count] {
            if *requirements & intermediate_bit != 0 {
                *requirements |= inherited;
            }
        }
    }
    predecessors[..count]
        .iter()
        .enumerate()
        .all(|(operation, &requirements)| requirements & (1_u16 << operation) == 0)
}

fn predecessor_closure(mut operations: u16, predecessors: &[u16; MAX_BOARD64_PIECES]) -> u16 {
    // Each predecessor mask already contains its transitive ancestors.
    let mut pending = operations;
    while pending != 0 {
        let operation = pending.trailing_zeros() as usize;
        pending &= pending - 1;
        operations |= predecessors[operation];
    }
    operations
}

fn board_for_operations(
    mut board: u64,
    physical_cells: &[u64; MAX_BOARD64_PIECES],
    mut operations: u16,
) -> u64 {
    while operations != 0 {
        let operation = operations.trailing_zeros() as usize;
        operations &= operations - 1;
        board |= physical_cells[operation];
    }
    board
}

fn vertical_entry_mask(lock_mask: u64, width: u8, height: u8) -> u64 {
    // Same collision corridor as the existing hard-drop fast path, bounded
    // to all target rows. Above-target bits cannot intersect a valid board.
    let mut path = lock_mask;
    for _ in 1..height {
        path |= path << width;
    }
    path
}
