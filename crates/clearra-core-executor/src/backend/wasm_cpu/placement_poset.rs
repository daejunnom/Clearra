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
//! The owning module gates this prototype to tests / `local-search-ab`. The
//! optional catalog guard table replaces repeated same-frame geometric tests,
//! not movement proofs or the conditions on which they depend. Callers
//! must additionally keep CountAll, finesse and non-clear-to-empty completion
//! on their existing paths. A certificate belongs to the supplied candidate;
//! its operation indices are concrete row IDs, not interchangeable kind IDs.

use clearra_core_domain::piece::piece_kind::PieceKind;
use std::cell::OnceCell;

use super::{
    buildup::{compact_target_board, placement_is_grounded},
    catalog::{GeometryCatalog, InstantiatedRealization},
    geometry::GeometryCandidate,
    placement_conditions::{
        grounding_sources, vertical_entry_mask, AtomicRowProjection, GroundingSources,
        GroupedClearConditions, PlacementConditionCatalog, PrecomputedOrderGuard,
    },
    MAX_BOARD64_PIECES,
};

const MAX_HEIGHT: usize = 6;
const MAX_CLEAR_FRAMES: usize = 1 << MAX_HEIGHT;
const MAX_CONDITIONAL_OPERATIONS: usize = 8;

#[cfg(test)]
thread_local! {
    static JOINT_CLEAR_ADMISSION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PROJECTION_COMPARISON: std::cell::Cell<Option<(bool, bool, ConditionalProjectionWork)>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ConditionalProjectionWork {
    pub lock_projections: usize,
    pub reused_same_frame: usize,
    pub clear_reprojections: usize,
    pub parent_group_visits: usize,
    pub child_group_visits: usize,
    pub direct_child_scans: usize,
    pub prepared_parents: usize,
    pub delta_reads: usize,
    pub nonempty_delta_reads: usize,
}

#[cfg(test)]
fn record_projection_work(update: impl FnOnce(&mut ConditionalProjectionWork)) {
    if let Some((reuse_lock, reuse_deltas, mut counts)) = PROJECTION_COMPARISON.get() {
        update(&mut counts);
        PROJECTION_COMPARISON.set(Some((reuse_lock, reuse_deltas, counts)));
    }
}

#[cfg(test)]
pub(super) fn with_conditional_work_comparison<T>(
    reuse_lock: bool,
    reuse_deltas: bool,
    action: impl FnOnce() -> T,
) -> (T, ConditionalProjectionWork) {
    struct Restore(Option<(bool, bool, ConditionalProjectionWork)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            PROJECTION_COMPARISON.set(self.0);
        }
    }
    let _restore = Restore(PROJECTION_COMPARISON.replace(Some((
        reuse_lock,
        reuse_deltas,
        Default::default(),
    ))));
    let output = action();
    (output, PROJECTION_COMPARISON.get().unwrap().2)
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

#[derive(Clone, Debug, Eq, PartialEq)]
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

struct CandidateOrderGuards<'a> {
    conditions: &'a PlacementConditionCatalog,
    // The candidate has disjoint validated original cells and at most 15
    // operations, so each cell identifies one concrete execution bit.
    owner_bits: [u16; u64::BITS as usize],
}

impl<'a> CandidateOrderGuards<'a> {
    fn new(catalog: &'a GeometryCatalog, geometry: &CandidateGeometry) -> Option<Self> {
        let conditions = catalog.placement_conditions()?.as_ref();
        if !conditions.has_precomputed_order_guards() {
            return None;
        }
        let mut owner_bits = [0; u64::BITS as usize];
        for (operation, &cells) in geometry.cells[..geometry.operation_count]
            .iter()
            .enumerate()
        {
            let mut cells = cells;
            while cells != 0 {
                owner_bits[cells.trailing_zeros() as usize] = 1 << operation;
                cells &= cells - 1;
            }
        }
        Some(Self {
            conditions,
            owner_bits,
        })
    }

    fn get(&self, row_id: u32, deleted: u16) -> Option<Option<PrecomputedOrderGuard>> {
        self.conditions.precomputed_order_guard(row_id, deleted)
    }

    fn touched_operations(&self, mut footprint: u64) -> u16 {
        let mut operations = 0;
        while footprint != 0 {
            operations |= self.owner_bits[footprint.trailing_zeros() as usize];
            footprint &= footprint - 1;
        }
        operations
    }

    fn touches_executed(&self, mut footprint: u64, executed: u16) -> bool {
        while footprint != 0 {
            if self.owner_bits[footprint.trailing_zeros() as usize] & executed != 0 {
                return true;
            }
            footprint &= footprint - 1;
        }
        false
    }
}

/// One exact executed-board snapshot and its immutable original-row frame.
/// Kept only while the conditional proof walks this state's outgoing edges.
pub(super) struct ConditionalState {
    pub(super) board: u64,
    pub(super) deleted: u16,
    projection: AtomicRowProjection,
    executed: u16,
    child_deltas: Option<[u16; MAX_CONDITIONAL_OPERATIONS]>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MovementProof {
    /// Existence of an exact edge on every board in [minimum, maximum].
    Reachable { maximum: u64 },
    /// Nonexistence of this exact pose on all supersets of minimum in
    /// the same physical frame. This does not assert reachability is monotone.
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    let precomputed = CandidateOrderGuards::new(catalog, &geometry);
    if geometry.has_unprovable_initial_support_disjunction(catalog, candidate, precomputed.as_ref())
    {
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
        let frame = OnceCell::new();
        let project = || frame.get_or_init(|| geometry.project_frame(deleted));
        let closed_enabled = infer_closed_frames && deleted != 0;
        let closed_board = OnceCell::new();
        let minimum_closed_board = || {
            *closed_board.get_or_init(|| {
                let (initial, physical_cells) = project();
                board_for_operations(*initial, physical_cells, required)
            })
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
            let row_id = candidate.row_ids()[operation];
            let guard = precomputed
                .as_ref()
                .and_then(|data| data.get(row_id, deleted));
            let mut is_closed = |realization| {
                movement_guard(MovementProofQuery {
                    piece: geometry.pieces[operation],
                    realization,
                    deleted,
                    minimum: minimum_closed_board(),
                    proof: MovementProof::Closed,
                })
            };
            match guard {
                Some(None) => continue, // Complete, empty instantiation domain.
                Some(Some(_)) => {
                    if closed_enabled && catalog.instantiations(row_id, deleted).all(&mut is_closed)
                    {
                        continue;
                    }
                }
                None => {
                    let lock_mask = project().1[operation];
                    let mut has_instantiation = false;
                    let mut all_closed = closed_enabled;
                    for realization in catalog.instantiations(row_id, deleted) {
                        // The optional catalog checks this identity once at
                        // compilation; ordinary fallback checks it here.
                        if realization.lock_mask != lock_mask || lock_mask.count_ones() != 4 {
                            return None;
                        }
                        has_instantiation = true;
                        if all_closed {
                            all_closed = is_closed(realization);
                        }
                    }
                    if !has_instantiation || all_closed {
                        continue;
                    }
                }
            }
            instantiation_domains[operation] |= 1_u64 << deleted;

            let grounded_at_initial = guard.flatten().map_or_else(
                || {
                    let (initial, physical_cells) = project();
                    placement_is_grounded(geometry.width, *initial, physical_cells[operation])
                },
                PrecomputedOrderGuard::ground_at_initial,
            );
            if grounded_at_initial {
                predecessors[operation] &= required;
                saw_grounding_witness |= operation_bit;
                possibly_before[operation] |= may_be_placed;
                continue;
            }
            let eligible = operations_in_frame() & !operation_bit;
            let mut supporters = if let (Some(data), Some(guard)) = (&precomputed, guard.flatten())
            {
                // Survivor contributions must not consult a supporter's
                // pending ILC domain: it has *already* been executed.
                data.touched_operations(guard.support_cells()) & eligible
            } else {
                let (initial, physical_cells) = project();
                match grounding_sources(
                    geometry.width,
                    *initial,
                    physical_cells[operation],
                    &physical_cells[..geometry.operation_count],
                    eligible,
                )? {
                    GroundingSources::AnyOf(supporters) => supporters,
                    _ => 0,
                }
            };
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
        // Closure distributes over a union; each predecessor mask is
        // already transitive. Reuse this invariant for all operations at D.
        let required_closed = predecessor_closure(required, &predecessors);
        let frame = OnceCell::new();
        let project = || frame.get_or_init(|| geometry.project_frame(deleted));
        for operation in 0..geometry.operation_count {
            let minimum = predecessors[operation] | required_closed;
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
            let row_id = candidate.row_ids()[operation];
            let guard = precomputed
                .as_ref()
                .and_then(|data| data.get(row_id, deleted))
                .flatten();
            let physical_boards = OnceCell::new();
            let project_boards = || {
                *physical_boards.get_or_init(|| {
                    let (initial, physical_cells) = project();
                    (
                        board_for_operations(*initial, physical_cells, minimum),
                        board_for_operations(*initial, physical_cells, maximum),
                    )
                })
            };
            let (grounded, corridor_blocked) = if let (Some(data), Some(guard)) =
                (&precomputed, guard)
            {
                (
                    guard.ground_at_initial()
                        || data.touches_executed(guard.support_cells(), minimum),
                    guard.corridor_hits_initial()
                        || data.touches_executed(guard.corridor_cells(), maximum),
                )
            } else {
                let lock_mask = project().1[operation];
                let (minimum_board, maximum_board) = project_boards();
                (
                    placement_is_grounded(geometry.width, minimum_board, lock_mask),
                    maximum_board & vertical_entry_mask(lock_mask, geometry.width, geometry.height)
                        != 0,
                )
            };
            if !grounded {
                return None;
            }
            if corridor_blocked {
                let (minimum_board, maximum_board) = project_boards();
                let guarded = catalog.instantiations(row_id, deleted).any(|realization| {
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
    #[cfg(any(
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    pub(super) fn for_symbolic_prefix(
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
    ) -> Option<Self> {
        Self::compile(catalog, candidate)
    }

    #[cfg(any(
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    pub(super) fn symbolic_clear_groups(&self) -> impl Iterator<Item = (u16, u16)> + '_ {
        self.clear_conditions.groups()
    }

    #[cfg(any(
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    pub(super) fn symbolic_frame(&self, deleted: u16) -> (u64, [u64; MAX_BOARD64_PIECES]) {
        self.project_frame(deleted)
    }

    /// Start only with small Boolean candidates with a geometric support
    /// disjunction, either initially or after a clear induced by just one
    /// concrete operation. This admission does not prove a physical move.
    pub(super) fn for_conditional_order(
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
    ) -> Option<Self> {
        if !(3..=MAX_CONDITIONAL_OPERATIONS).contains(&candidate.row_ids().len()) {
            return None;
        }
        let geometry = Self::compile(catalog, candidate)?;
        if geometry.has_unprovable_initial_support_disjunction(catalog, candidate, None)
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
        let pending = self.all_operations & !executed;
        // A lone remaining operation or row group rarely amortizes setup.
        // Otherwise prepare siblings while doing the parent's existing scan.
        let prepare = self.operation_count <= MAX_CONDITIONAL_OPERATIONS
            && pending.count_ones() >= 2
            && self.clear_conditions.group_count() >= 2;
        #[cfg(test)]
        let prepare = prepare
            && !PROJECTION_COMPARISON
                .get()
                .is_some_and(|(_, deltas, _)| !deltas);
        let (deleted, child_deltas) = if prepare {
            let (deleted, deltas) = self
                .clear_conditions
                .deleted_rows_with_child_deltas(executed);
            (deleted, Some(deltas))
        } else {
            (self.clear_conditions.deleted_rows(executed), None)
        };
        #[cfg(test)]
        record_projection_work(|work| {
            work.parent_group_visits += self.clear_conditions.group_count();
            work.prepared_parents += usize::from(prepare);
        });
        let projection = self.row_projection(deleted);
        let board = board_for_operations(self.initial_board, &self.cells, executed);
        ConditionalState {
            board: projection.physical_board(board),
            deleted,
            projection,
            executed,
            child_deltas,
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
        operation: usize,
    ) -> (u64, impl FnOnce() -> (u64, u16) + 'a) {
        let mask = self.conditional_lock_mask(operation, state);
        (mask, move || {
            self.conditional_after_operation(state, operation, mask)
        })
    }

    fn conditional_after_operation(
        &self,
        state: &ConditionalState,
        operation: usize,
        mask: u64,
    ) -> (u64, u16) {
        let bit = 1_u16 << operation;
        debug_assert_eq!(state.executed & bit, 0);
        let next = state.executed | bit;
        debug_assert_eq!(next & !self.all_operations, 0);
        let deleted = if let Some(deltas) = &state.child_deltas {
            // This state belongs to this executed set. Other groups either
            // already contributed to D, or still lack some operation after q.
            let delta = deltas[operation];
            #[cfg(test)]
            record_projection_work(|work| {
                work.delta_reads += 1;
                work.nonempty_delta_reads += usize::from(delta != 0);
            });
            state.deleted | delta
        } else {
            #[cfg(test)]
            record_projection_work(|work| {
                work.direct_child_scans += 1;
                work.child_group_visits += self.clear_conditions.group_count();
            });
            self.clear_conditions.deleted_rows(next)
        };
        if deleted == state.deleted {
            // Projection is a fixed permutation/truncation of disjoint cells.
            // The unchanged frame preserves the exact footprint already used
            // by the ILC/grounding checks of this same operation.
            #[cfg(test)]
            if PROJECTION_COMPARISON
                .get()
                .is_some_and(|(reuse, _, _)| !reuse)
            {
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
        precomputed: Option<&CandidateOrderGuards<'_>>,
    ) -> bool {
        let eligible =
            self.clear_conditions
                .possible_operations_in_frame(0, 0, self.all_operations);
        for operation in 0..self.operation_count {
            let row_id = candidate.row_ids()[operation];
            if let Some(data) = precomputed {
                match data.get(row_id, 0) {
                    Some(Some(guard)) => {
                        let supporters = data.touched_operations(guard.support_cells())
                            & eligible
                            & !(1_u16 << operation);
                        if !guard.ground_at_initial() && supporters.count_ones() >= 2 {
                            return true;
                        }
                        continue;
                    }
                    Some(None) => continue,
                    None => (),
                }
            }
            let lock_mask = self.cells[operation];
            if let Some(GroundingSources::AnyOf(supporters)) = grounding_sources(
                self.width,
                self.initial_board,
                lock_mask,
                &self.cells[..self.operation_count],
                eligible & !(1_u16 << operation),
            ) {
                if supporters.count_ones() >= 2
                    && catalog.instantiations(row_id, 0).next().is_some()
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

#[cfg(test)]
mod precomputed_order_guard_tests {
    use super::*;
    use crate::backend::wasm_cpu::{
        placement_conditions::with_precomputed_order_guards, placement_dependency,
    };

    fn catalog_with_guards(width: u8, height: u8, required: u64, guarded: bool) -> GeometryCatalog {
        let full = (1_u64 << (width * height)) - 1;
        placement_dependency::with_mode(true, || {
            with_precomputed_order_guards(guarded, || {
                GeometryCatalog::compile_for_required_cells_on_dimensions(
                    width,
                    height,
                    full & !required,
                    required,
                )
                .unwrap()
            })
        })
        .0
    }

    fn visit_tilings(
        catalog: &GeometryCatalog,
        remaining: u64,
        rows: &mut Vec<u32>,
        visit: &mut impl FnMut(&[u32]),
    ) {
        if remaining == 0 {
            visit(rows);
            return;
        }
        for &row_id in catalog.support(remaining.trailing_zeros() as u8) {
            let cells = catalog.skeleton(row_id).cells;
            if cells & !remaining == 0 {
                rows.push(row_id);
                visit_tilings(catalog, remaining & !cells, rows, visit);
                rows.pop();
            }
        }
    }

    #[test]
    fn precomputed_guards_preserve_certificates_and_ordered_proof_queries() {
        let mut candidates = 0;
        let mut certified = 0;
        let mut fallback = 0;
        let mut closed_queries = 0;
        let mut reachability_queries = 0;
        let o = 3 | (3 << 10);
        // Exhaust the complete small canonical tilings, then cover a late
        // atomic clear with initial blockers and nontrivial roof queries.
        for (width, height, required) in [
            (4, 3, 0xfff),
            (10, 2, o | 0x3c),
            (10, 4, o | (o << 2) | (o << 20) | (o << 22)),
        ] {
            let plain = catalog_with_guards(width, height, required, false);
            let fast = catalog_with_guards(width, height, required, true);
            assert_eq!(plain.identity_digest(), fast.identity_digest());
            assert!(fast
                .placement_conditions()
                .unwrap()
                .has_precomputed_order_guards());
            visit_tilings(&plain, required, &mut Vec::new(), &mut |rows| {
                candidates += 1;
                for infer_closed in [false, true] {
                    for response in [false, true] {
                        let run = |catalog| {
                            let candidate = GeometryCandidate::from_rows(catalog, 0, rows).unwrap();
                            let mut questions = Vec::new();
                            let poset = compile_with_movement_guard(
                                catalog,
                                &candidate,
                                infer_closed,
                                |query| {
                                    questions.push(query);
                                    response
                                },
                            );
                            (poset, questions)
                        };
                        let expected = run(&plain);
                        let actual = run(&fast);
                        assert_eq!(actual, expected, "rows={rows:?}");
                        // Returning both possible answers is only a
                        // differential test of callback scope/order; it is
                        // not a physical oracle or a real movement proof.
                        if !response {
                            certified += usize::from(actual.0.is_some());
                            fallback += usize::from(actual.0.is_none());
                        }
                        for query in &actual.1 {
                            match query.proof {
                                MovementProof::Closed => closed_queries += 1,
                                MovementProof::Reachable { .. } => reachability_queries += 1,
                            }
                        }
                    }
                }
            });
        }
        assert!(candidates > 10 && certified > 0 && fallback > 0);
        assert!(closed_queries > 0 && reachability_queries > 0);
    }
}
