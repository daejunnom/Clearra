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
    placement_conditions::{
        grounding_sources, AtomicRowProjection, GroundingSources, GroupedClearConditions,
    },
    MAX_BOARD64_PIECES,
};

const MAX_HEIGHT: usize = 6;
const MAX_CLEAR_FRAMES: usize = 1 << MAX_HEIGHT;

#[cfg(test)]
thread_local! {
    static JOINT_CLEAR_ADMISSION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PROJECTION_COMPARISON: std::cell::Cell<Option<(bool, ConditionalProjectionWork)>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ConditionalProjectionWork {
    pub lock_projections: usize,
    pub reused_same_frame: usize,
    pub clear_reprojections: usize,
}

#[cfg(test)]
fn record_projection_work(update: impl FnOnce(&mut ConditionalProjectionWork)) {
    if let Some((reuse, mut counts)) = PROJECTION_COMPARISON.get() {
        update(&mut counts);
        PROJECTION_COMPARISON.set(Some((reuse, counts)));
    }
}

#[cfg(test)]
pub(super) fn with_conditional_projection_reuse<T>(
    reuse: bool,
    action: impl FnOnce() -> T,
) -> (T, ConditionalProjectionWork) {
    struct Restore(Option<(bool, ConditionalProjectionWork)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            PROJECTION_COMPARISON.set(self.0);
        }
    }
    let _restore = Restore(PROJECTION_COMPARISON.replace(Some((reuse, Default::default()))));
    let output = action();
    (output, PROJECTION_COMPARISON.get().unwrap().1)
}

#[cfg(test)]
pub(super) fn with_joint_clear_admission<T>(enabled: bool, action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            JOINT_CLEAR_ADMISSION.set(self.0);
        }
    }
    let _restore = Restore(JOINT_CLEAR_ADMISSION.replace(enabled));
    action()
}

#[derive(Clone, Debug)]
pub(super) struct CertifiedPlacementPoset {
    operation_count: usize,
    pieces: [PieceKind; MAX_BOARD64_PIECES],
    predecessors: [u16; MAX_BOARD64_PIECES],
    interval_guard_count: usize,
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

    /// Number of operation / deleted-row frames certified by an exact analytic
    /// or observed interval guard instead of the direct hard-drop corridor.
    pub(super) fn interval_guard_count(&self) -> usize {
        self.interval_guard_count
    }
}

pub(super) struct CandidateGeometry {
    width: u8,
    height: u8,
    initial_board: u64,
    operation_count: usize,
    all_operations: u16,
    pieces: [PieceKind; MAX_BOARD64_PIECES],
    cells: [u64; MAX_BOARD64_PIECES],
    clear_conditions: GroupedClearConditions,
}

/// One exact executed-board snapshot and its immutable original-row frame.
/// Kept only while the conditional proof walks this state's outgoing edges.
pub(super) struct ConditionalState {
    pub(super) board: u64,
    pub(super) deleted: u16,
    projection: AtomicRowProjection,
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

#[cfg(test)]
pub(super) fn compile_with_guard(
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    mut interval_guard: impl FnMut(PieceKind, InstantiatedRealization, u16, u64, u64) -> bool,
) -> Option<CertifiedPlacementPoset> {
    compile_with_movement_guard(catalog, candidate, false, |query| {
        let MovementProof::Reachable { maximum } = query.proof else {
            return false;
        };
        interval_guard(
            query.piece,
            query.realization,
            query.deleted,
            query.minimum,
            maximum,
        )
    })
}

#[derive(Clone, Copy)]
pub(super) enum MovementProof {
    /// Existence of an exact edge on every board in [minimum, maximum].
    Reachable { maximum: u64 },
    /// Nonexistence of this exact pose on all supersets of minimum in
    /// the same physical frame. This does not assert reachability is monotone.
    Closed,
}

#[derive(Clone, Copy)]
pub(super) struct MovementProofQuery {
    pub(super) piece: PieceKind,
    pub(super) realization: InstantiatedRealization,
    pub(super) deleted: u16,
    pub(super) minimum: u64,
    pub(super) proof: MovementProof,
}

/// Extend the hard-drop certificate with scoped movement evidence.
/// A guard miss is Unknown. Only closed proofs for *all* realizations remove a
/// frame from the necessary-order calculation. Positive endpoint samples alone
/// cannot certify an interval under ordered kicks.
pub(super) fn compile_with_movement_guard(
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    infer_closed_frames: bool,
    mut movement_guard: impl FnMut(MovementProofQuery) -> bool,
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
    let mut possibly_before = [0_u16; MAX_BOARD64_PIECES];

    for deleted in 0..1_u16 << geometry.height {
        let Some(required) = geometry.clear_conditions.required_for_frame(deleted) else {
            continue;
        };
        required_by_frame[usize::from(deleted)] = required;
        possible_frames |= 1_u64 << deleted;
        let (initial, physical_cells) = geometry.project_frame(deleted);
        let minimum_board = if infer_closed_frames && deleted != 0 {
            board_for_operations(initial, &physical_cells, required)
        } else {
            initial
        };
        // A completed row's contributors are required. For an undeleted
        // row with one missing contributor, adding that contributor would
        // change the frame. Compute candidate eligibility once per frame.
        let mut eligible = None;
        let mut operations_in_frame = || {
            *eligible.get_or_insert_with(|| {
                geometry.clear_conditions.possible_operations_in_frame(
                    deleted,
                    required,
                    geometry.all_operations,
                )
            })
        };
        let may_be_placed = if infer_closed_frames {
            operations_in_frame()
        } else {
            0
        };
        for operation in 0..geometry.operation_count {
            let operation_bit = 1_u16 << operation;
            if required & operation_bit != 0 {
                // An already completed original row contains this operation;
                // it cannot still be pending at this frame.
                continue;
            }
            let lock_mask = physical_cells[operation];
            let mut has_instantiation = false;
            let mut all_closed = infer_closed_frames && deleted != 0;
            for realization in catalog.instantiations(candidate.row_ids()[operation], deleted) {
                // ILC projects the physical lock back to precisely these
                // original cells. Keep this as an explicit certificate gate
                // if the catalog's representation changes in the future.
                if realization.lock_mask != lock_mask || lock_mask.count_ones() != 4 {
                    return None;
                }
                has_instantiation = true;
                if all_closed {
                    all_closed = movement_guard(MovementProofQuery {
                        piece: geometry.pieces[operation],
                        realization,
                        deleted,
                        minimum: minimum_board,
                        proof: MovementProof::Closed,
                    });
                }
            }
            if !has_instantiation || all_closed {
                continue;
            }
            instantiation_domains[operation] |= 1_u64 << deleted;

            if placement_is_grounded(geometry.width, initial, lock_mask) {
                predecessors[operation] &= required;
                saw_grounding_witness |= operation_bit;
                possibly_before[operation] |= may_be_placed;
                continue;
            }
            if let GroundingSources::AnyOf(mut supporters) = grounding_sources(
                geometry.width,
                initial,
                lock_mask,
                &physical_cells[..geometry.operation_count],
                operations_in_frame() & !operation_bit,
            )? {
                while supporters != 0 {
                    let supporter_bit = 1_u16 << supporters.trailing_zeros();
                    supporters &= supporters - 1;
                    // Every actual lock has some ground witness. Intersecting
                    // requirements over all possible witnesses keeps only facts
                    // necessary for every physical order; unexecutable witnesses
                    // can weaken this intersection but cannot make it unsound.
                    predecessors[operation] &= required | supporter_bit;
                    saw_grounding_witness |= operation_bit;
                    possibly_before[operation] |= may_be_placed;
                }
            }
        }
    }
    if infer_closed_frames {
        // Every physically executable p has one of the frames and grounding
        // witnesses considered above. If q is absent even from the union of
        // all those overapproximated prefixes, p must precede q in every
        // complete execution. The order is not inferred from one failed D.
        for (operation, allowed_before) in possibly_before[..geometry.operation_count]
            .iter()
            .copied()
            .enumerate()
        {
            if saw_grounding_witness & (1_u16 << operation) == 0 {
                continue;
            }
            let mut must_follow = geometry.all_operations & !(allowed_before | (1 << operation));
            while must_follow != 0 {
                let after = must_follow.trailing_zeros() as usize;
                must_follow &= must_follow - 1;
                predecessors[after] |= 1 << operation;
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
    let mut interval_guard_count = 0;
    let mut pending_frames = possible_frames;
    while pending_frames != 0 {
        let deleted = pending_frames.trailing_zeros() as u16;
        pending_frames &= pending_frames - 1;
        let required = required_by_frame[usize::from(deleted)];
        let (initial, physical_cells) = geometry.project_frame(deleted);
        for operation in 0..geometry.operation_count {
            let minimum = predecessor_closure(predecessors[operation] | required, &predecessors);
            let mut maximum = maximum_without[operation];
            if minimum & !maximum != 0 || geometry.clear_conditions.deleted_rows(minimum) != deleted
            {
                // No ideal with this D can make the operation available.
                continue;
            }
            for (contributors, atomic_rows) in geometry.clear_conditions.groups() {
                if deleted & atomic_rows != 0 || contributors & !maximum != 0 {
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
                        movement_guard(MovementProofQuery {
                            piece: geometry.pieces[operation],
                            realization,
                            deleted,
                            minimum: minimum_board,
                            proof: MovementProof::Reachable {
                                maximum: maximum_board,
                            },
                        })
                    });
                if !guarded {
                    return None;
                }
                interval_guard_count += 1;
            }
            // For every actual ideal E in this frame, min ⊆ E ⊆ max.
            // Compaction at fixed D preserves occupancy inclusion. Grounding
            // at min and an empty corridor at max imply a hard-drop edge at E.
            // Otherwise the callback supplies an exact analytic or observed
            // proof for the interval, including first-success kick conditions.
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
        interval_guard_count,
    })
}

impl CandidateGeometry {
    /// Start only with small Boolean candidates with a geometric support
    /// disjunction, either initially or after a clear induced by just one
    /// concrete operation. This admission does not prove a physical move.
    pub(super) fn for_conditional_order(
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
    ) -> Option<Self> {
        if !(3..=8).contains(&candidate.row_ids().len()) {
            return None;
        }
        let geometry = Self::compile(catalog, candidate)?;
        if geometry.has_unprovable_initial_support_disjunction(catalog, candidate)
            || geometry.has_support_disjunction_after_group_clear::<false>(catalog, candidate)
        {
            return Some(geometry);
        }
        #[cfg(test)]
        if JOINT_CLEAR_ADMISSION.get()
            && geometry.has_support_disjunction_after_group_clear::<true>(catalog, candidate)
        {
            return Some(geometry);
        }
        None
    }

    pub(super) fn pieces(&self) -> &[PieceKind] {
        &self.pieces[..self.operation_count]
    }

    /// The state is only the executed set: completed rows are derived from
    /// their concrete contributor conjunctions, and all such rows are
    /// removed together in the original-row frame.
    pub(super) fn conditional_state(&self, executed: u16) -> ConditionalState {
        debug_assert_eq!(executed & !self.all_operations, 0);
        let deleted = self.clear_conditions.deleted_rows(executed);
        let projection = self.row_projection(deleted);
        let board = board_for_operations(self.initial_board, &self.cells, executed);
        ConditionalState {
            board: projection.physical_board(board),
            deleted,
            projection,
        }
    }

    pub(super) fn conditional_lock_mask(&self, operation: usize, state: &ConditionalState) -> u64 {
        #[cfg(test)]
        record_projection_work(|work| work.lock_projections += 1);
        state.projection.physical_board(self.cells[operation])
    }

    /// Return this concrete operation's projected footprint and a deferred
    /// child check bound to the same candidate, executed set and row frame.
    /// Rejecting an ungrounded or uninstantiable operation need not build its child.
    pub(super) fn conditional_operation<'a>(
        &'a self,
        state: &'a ConditionalState,
        executed: u16,
        operation: usize,
    ) -> (u64, impl FnOnce() -> (u64, u16) + 'a) {
        let mask = self.conditional_lock_mask(operation, state);
        (mask, move || {
            self.conditional_after_operation(state, executed, operation, mask)
        })
    }

    fn conditional_after_operation(
        &self,
        state: &ConditionalState,
        executed: u16,
        operation: usize,
        mask: u64,
    ) -> (u64, u16) {
        let bit = 1_u16 << operation;
        debug_assert_eq!(executed & bit, 0);
        debug_assert_eq!((executed | bit) & !self.all_operations, 0);
        let next = executed | bit;
        let deleted = self.clear_conditions.deleted_rows(next);
        if deleted == state.deleted {
            // Projection is a fixed permutation/truncation of disjoint cells.
            // The unchanged frame preserves the exact footprint already used
            // by the ILC/grounding checks of this same operation.
            #[cfg(test)]
            if PROJECTION_COMPARISON.get().is_some_and(|(reuse, _)| !reuse) {
                return (
                    state.board | self.conditional_lock_mask(operation, state),
                    deleted,
                );
            }
            #[cfg(test)]
            record_projection_work(|work| work.reused_same_frame += 1);
            return (state.board | mask, deleted);
        }
        // When rows clear, reproject the exact original executed union;
        // neither the stale compacted board nor individually possible pieces
        // identify the survivors in the new frame.
        let projection = self.row_projection(deleted);
        let original = board_for_operations(self.initial_board, &self.cells, next);
        #[cfg(test)]
        record_projection_work(|work| work.clear_reprojections += 1);
        (projection.physical_board(original), deleted)
    }

    fn row_projection(&self, deleted: u16) -> AtomicRowProjection {
        AtomicRowProjection::new(self.width, self.height, deleted)
            .expect("candidate geometry owns a validated compact row frame")
    }

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
            clear_conditions: GroupedClearConditions::from_completed_rows(
                &row_contributors[..usize::from(height)],
                completed_rows,
            )?,
        })
    }

    fn has_unprovable_initial_support_disjunction(
        &self,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
    ) -> bool {
        let eligible =
            self.clear_conditions
                .possible_operations_in_frame(0, 0, self.all_operations);
        for operation in 0..self.operation_count {
            let lock_mask = self.cells[operation];
            if let Some(GroundingSources::AnyOf(supporters)) = grounding_sources(
                self.width,
                self.initial_board,
                lock_mask,
                &self.cells[..self.operation_count],
                eligible & !(1_u16 << operation),
            ) {
                if supporters.count_ones() >= 2
                    && catalog
                        .instantiations(candidate.row_ids()[operation], 0)
                        .next()
                        .is_some()
                {
                    // At D=0 singleton support alternatives have no common
                    // prerequisite, so an AND certificate cannot prove all
                    // ideal transitions. The conditional compiler can.
                    return true;
                }
            }
        }
        false
    }

    fn has_support_disjunction_after_group_clear<const JOINT: bool>(
        &self,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
    ) -> bool {
        if self.operation_count < if JOINT { 5 } else { 4 } {
            return false;
        }
        // The test-only joint lane inspects at most two actual contributor
        // groups *after* the ordinary admission missed. Never enumerate pairs
        // or subsets to find a frame. Missing a later group is only fallback.
        let groups = self.clear_conditions.groups().filter(|(required, _)| {
            if JOINT {
                required.count_ones() == 2
            } else {
                required.is_power_of_two()
            }
        });
        for (required, group_rows) in groups.take(if JOINT { 2 } else { MAX_HEIGHT }) {
            // A pair can also finish a distinct singleton group; the union of
            // all rows actually cleared by these concrete operations is atomic.
            let deleted = if JOINT {
                self.clear_conditions.deleted_rows(required)
            } else {
                group_rows
            };
            let optional = self.clear_conditions.possible_operations_in_frame(
                deleted,
                required,
                self.all_operations,
            ) & !required;
            if optional.count_ones() < 2 {
                continue;
            }
            let (initial, physical) = self.project_frame(deleted);
            let first = initial | physical[required.trailing_zeros() as usize];
            let minimum = if JOINT {
                first | physical[(required & (required - 1)).trailing_zeros() as usize]
            } else {
                first
            };
            for (operation, &lock_mask) in physical[..self.operation_count].iter().enumerate() {
                let operation_bit = 1_u16 << operation;
                if required & operation_bit != 0
                    || placement_is_grounded(self.width, minimum, lock_mask)
                {
                    continue;
                }
                let Some(GroundingSources::AnyOf(mut supporters)) = grounding_sources(
                    self.width,
                    minimum,
                    lock_mask,
                    &physical[..self.operation_count],
                    optional & !operation_bit,
                ) else {
                    continue;
                };
                if supporters.count_ones() < 2
                    || catalog
                        .instantiations(candidate.row_ids()[operation], deleted)
                        .next()
                        .is_none()
                {
                    continue;
                }
                let mut available = 0;
                while supporters != 0 {
                    let supporter = supporters.trailing_zeros() as usize;
                    supporters &= supporters - 1;
                    if catalog
                        .instantiations(candidate.row_ids()[supporter], deleted)
                        .next()
                        .is_some()
                    {
                        available += 1;
                        if available == 2 {
                            // Still only admission. The ordinary bounded
                            // proof checks support, movement and the clear
                            // transition at actual reachable executed sets.
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn project_frame(&self, deleted: u16) -> (u64, [u64; MAX_BOARD64_PIECES]) {
        let projection = self.row_projection(deleted);
        let initial = projection.physical_board(self.initial_board);
        let mut physical_cells = [0_u64; MAX_BOARD64_PIECES];
        for (operation, physical) in physical_cells[..self.operation_count]
            .iter_mut()
            .enumerate()
        {
            *physical = projection.physical_board(self.cells[operation]);
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
