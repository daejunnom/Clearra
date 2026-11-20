// SRP rationale: compile target-frame placement and row-clear conditions once,
// before tiling, and specialize those conditions without a placement-subset graph.
use super::catalog::GeometryCatalog;
use crate::row_frame::CompactedRowFrame;

pub(super) const MAX_CONDITION_HEIGHT: u8 = 6;
// This is a bounded prototype. The board64, six-row tetromino catalogs are much
// smaller, but a future catalog extension must explicitly revisit this bound.
const MAX_CONDITION_ROWS: usize = 128 * 1024;
const MAX_SURVIVING_RUNS: usize = (MAX_CONDITION_HEIGHT as usize).div_ceil(2);
const MAX_ORDER_GUARD_RECORDS: usize = 64 * 1024;
// Order guards are restricted to at most ten columns and six rows. Neither
// original footprint can own this bit; use it for the known initial term.
const INITIAL_TERM: u64 = 1 << 63;

#[cfg(test)]
thread_local! {
    static PRECOMPUTED_ORDER_GUARDS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(super) fn with_precomputed_order_guards<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            PRECOMPUTED_ORDER_GUARDS.set(self.0);
        }
    }
    let _restore = Restore(PRECOMPUTED_ORDER_GUARDS.replace(enabled));
    run()
}

pub(super) fn precomputed_order_guards_enabled() -> bool {
    #[cfg(test)]
    {
        PRECOMPUTED_ORDER_GUARDS.get()
    }
    #[cfg(not(test))]
    {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| {
            std::env::var("CLEARRA_STRUCTURAL_PRECOMPUTED_GUARDS").is_ok_and(|value| value == "1")
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct SurvivingRun {
    original_cells: u64,
    shift: u8,
}

/// Position map of the surviving row runs after a set of atomic clears.
///
/// Each run maps a contiguous original interval by one downward shift.
/// Compile it once for the frame and reuse it to project initial, executed
/// and pending operation cells. Projection only moves surviving cells; it
/// does not establish whether the deletion set, an ILC lock, or a physical
/// move is reachable.
#[derive(Clone, Copy, Debug)]
pub(super) struct AtomicRowProjection {
    runs: [SurvivingRun; MAX_SURVIVING_RUNS],
    run_count: usize,
}

impl AtomicRowProjection {
    pub(super) fn new(width: u8, height: u8, deleted_original_rows: u16) -> Option<Self> {
        let frame = CompactedRowFrame::new(height, deleted_original_rows)?;
        if width == 0 || u32::from(width) * u32::from(height) > u64::BITS {
            return None;
        }
        let mut result = Self {
            runs: [SurvivingRun {
                original_cells: 0,
                shift: 0,
            }; MAX_SURVIVING_RUNS],
            run_count: 0,
        };
        let deleted = frame.deleted_original_rows();
        let mut original = 0;
        let mut deleted_below = 0;
        while original < height {
            if deleted & (1 << original) != 0 {
                deleted_below += 1;
                original += 1;
                continue;
            }
            let start = original;
            while original < height && deleted & (1 << original) == 0 {
                original += 1;
            }
            // start and deleted_below are strictly below height for
            // each nonempty run, so neither shift can reach 64.
            let cells = low_bits(u32::from(original - start) * u32::from(width));
            result.runs[result.run_count] = SurvivingRun {
                original_cells: cells << (u32::from(start) * u32::from(width)),
                shift: deleted_below * width,
            };
            result.run_count += 1;
        }
        Some(result)
    }

    pub(super) fn physical_board(&self, original_cells: u64) -> u64 {
        self.runs[..self.run_count].iter().fold(0, |board, run| {
            board | ((original_cells & run.original_cells) >> run.shift)
        })
    }

    /// Restore original indices for the surviving footprint. Bits in the
    /// physical headroom above the last surviving row have no contributor.
    fn surviving_original_cells(&self, physical_cells: u64) -> u64 {
        self.runs[..self.run_count].iter().fold(0, |board, run| {
            board | ((physical_cells << run.shift) & run.original_cells)
        })
    }
}

/// Grounding of one pending lock footprint in a fixed, compacted frame.
/// A satisfied premise does not prove ILC instantiation or move reachability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GroundingSources {
    /// The lock touches the floor or a surviving initial-board cell.
    AlreadyGrounded,
    /// A nonempty disjunction of concrete operations in selected-row order.
    AnyOf(u16),
    /// Even all eligible support cells cannot ground this lock in this frame.
    Ungrounded,
}

/// The supplied cells have already been projected to the same row frame.
/// Cleared cells are absent, but the surviving cells of an executed operation
/// still count, independently of whether that operation could instantiate
/// again. Eligibility restricts the possible supporters, not actual execution.
/// A malformed query is unknown (`None`), not an ungrounded lock.
pub(super) fn grounding_sources(
    width: u8,
    initial: u64,
    lock_mask: u64,
    projected_cells: &[u64],
    eligible_mask: u16,
) -> Option<GroundingSources> {
    if !(1..=10).contains(&width)
        || lock_mask.count_ones() != 4
        || initial & lock_mask != 0
        || projected_cells.len() > u16::BITS as usize
        || u64::from(eligible_mask) & !low_bits(projected_cells.len() as u32) != 0
    {
        return None;
    }

    let grounded_at_initial = super::buildup::placement_is_grounded(width, initial, lock_mask);
    let below = lock_mask >> width;
    let mut pending = eligible_mask;
    let mut supporters = 0;
    while pending != 0 {
        let index = pending.trailing_zeros() as usize;
        let bit = 1_u16 << index;
        pending ^= bit;
        let cells = projected_cells[index];
        if cells & lock_mask != 0 {
            return None;
        }
        if below & cells != 0 {
            supporters |= bit;
        }
    }
    Some(if grounded_at_initial {
        GroundingSources::AlreadyGrounded
    } else if supporters != 0 {
        GroundingSources::AnyOf(supporters)
    } else {
        GroundingSources::Ungrounded
    })
}

#[derive(Clone, Copy, Debug)]
struct CanonicalPlacement {
    cells: u64,
    occupied_rows: u16,
    domain_id: u32,
}

/// Static Boolean terms for a concrete, valid (row, deleted-frame) lock.
/// The masks are lifted into the original frame so current executed
/// operations need not be projected individually just to test intersection.
/// Neither term certifies a kick, translation, or move on its own.
#[derive(Clone, Copy, Debug)]
pub(super) struct PrecomputedOrderGuard {
    support: u64,
    corridor: u64,
}

impl PrecomputedOrderGuard {
    pub(super) const fn ground_at_initial(self) -> bool {
        self.support & INITIAL_TERM != 0
    }

    pub(super) const fn support_cells(self) -> u64 {
        self.support & !INITIAL_TERM
    }

    pub(super) const fn corridor_hits_initial(self) -> bool {
        self.corridor & INITIAL_TERM != 0
    }

    pub(super) const fn corridor_cells(self) -> u64 {
        self.corridor & !INITIAL_TERM
    }
}

#[derive(Debug)]
struct PrecomputedOrderGuards {
    offsets: Vec<u32>,
    guards: Vec<PrecomputedOrderGuard>,
}

impl PrecomputedOrderGuards {
    fn compile(
        catalog: &GeometryCatalog,
        placements: &[CanonicalPlacement],
        clear_domains: &[u64],
    ) -> Option<Self> {
        let width = catalog.width();
        let height = catalog.height();
        if !(1..=10).contains(&width) || !(1..=MAX_CONDITION_HEIGHT).contains(&height) {
            return None;
        }
        let record_count = placements.iter().try_fold(0_usize, |count, row| {
            count.checked_add(clear_domains[row.domain_id as usize].count_ones() as usize)
        })?;
        if record_count > MAX_ORDER_GUARD_RECORDS {
            return None;
        }
        let mut offsets = Vec::new();
        let mut guards = Vec::new();
        offsets.try_reserve_exact(placements.len()).ok()?;
        guards.try_reserve_exact(record_count).ok()?;
        let mut projections = [None; 1_usize << MAX_CONDITION_HEIGHT];
        for (row_id, row) in placements.iter().enumerate() {
            offsets.push(u32::try_from(guards.len()).ok()?);
            let mut frames = clear_domains[row.domain_id as usize];
            while frames != 0 {
                let deleted = frames.trailing_zeros() as u16;
                frames &= frames - 1;
                let projection = &mut projections[usize::from(deleted)];
                if projection.is_none() {
                    let frame = AtomicRowProjection::new(width, height, deleted)?;
                    *projection = Some((frame, frame.physical_board(catalog.initial_board())));
                }
                let (frame, initial) = (*projection)?;
                let lock = frame.physical_board(row.cells);
                // Preserve the old certificate's check of every distinct
                // realization, not just the fact that the domain is nonempty.
                // A disagreement discards the *optional* table.
                if lock.count_ones() != 4
                    || !catalog.all_instantiations_match_lock(row_id as u32, deleted, lock)
                {
                    return None;
                }
                let physical_corridor = vertical_entry_mask(lock, width, height);
                guards.push(PrecomputedOrderGuard {
                    support: frame.surviving_original_cells(lock >> width)
                        | (u64::from(super::buildup::placement_is_grounded(width, initial, lock))
                            * INITIAL_TERM),
                    corridor: frame.surviving_original_cells(physical_corridor)
                        | (u64::from(initial & physical_corridor != 0) * INITIAL_TERM),
                });
            }
        }
        Some(Self { offsets, guards })
    }

    fn retained_bytes(&self) -> usize {
        self.offsets.capacity() * core::mem::size_of::<u32>()
            + self.guards.capacity() * core::mem::size_of::<PrecomputedOrderGuard>()
    }
}

pub(super) fn vertical_entry_mask(lock: u64, width: u8, height: u8) -> u64 {
    // Same collision corridor as the existing hard-drop fast path.
    // Above-target bits cannot intersect a valid board.
    let mut corridor = lock;
    for _ in 1..height {
        corridor |= corridor << width;
    }
    corridor
}

/// Immutable conditions for the owning geometry catalog. A row ID always means
/// one canonical `(piece, cells)` placement, never just a piece kind.
///
/// Each interned domain is an exact disjunction over deleted-row masks. Bit `D`
/// is set iff at least one ILC realization instantiates at `D`. This preserves
/// alternatives and the prohibition on deleting a pending placement's cells;
/// it is stronger than a union of minimum required-clear masks.
///
/// The owner must scope use to this particular catalog. No digest is used as an
/// equality proof, and these conditions do not assert movement reachability.
#[derive(Debug)]
pub(super) struct PlacementConditionCatalog {
    width: u8,
    height: u8,
    initial_board: u64,
    placements: Vec<CanonicalPlacement>,
    clear_domains: Vec<u64>,
    order_guards: Option<PrecomputedOrderGuards>,
}

impl PlacementConditionCatalog {
    /// Conservative additional compile peak for a row-count upper bound,
    /// including temporary exact-domain sorting.
    /// A coarse caller bound can exceed the admission cap while the actual
    /// catalog fits. Clamp to that cap rather than treating such a bound as an
    /// unsupported, zero-cost compile. `compile` still rejects actual excess.
    /// Dimension support is checked separately.
    pub(super) fn checked_compile_peak_upper_bound(row_count: usize) -> Option<u128> {
        let row_count = row_count.min(MAX_CONDITION_ROWS);
        let per_row = core::mem::size_of::<CanonicalPlacement>()
            + core::mem::size_of::<(u64, u32)>()
            + core::mem::size_of::<u64>();
        (row_count as u128)
            .checked_mul(per_row as u128)?
            .checked_add((core::mem::size_of::<Self>() + 2 * core::mem::size_of::<usize>()) as u128)
    }

    pub(super) fn order_guard_compile_heap_bound(row_count: usize, width: u8, height: u8) -> u128 {
        if !(1..=10).contains(&width) || !(1..=MAX_CONDITION_HEIGHT).contains(&height) {
            return 0;
        }
        let rows = row_count.min(MAX_CONDITION_ROWS);
        let frames = rows
            .saturating_mul(1_usize << height)
            .min(MAX_ORDER_GUARD_RECORDS);
        (rows * core::mem::size_of::<u32>()
            + frames * core::mem::size_of::<PrecomputedOrderGuard>()) as u128
    }

    /// Unsupported dimensions or an unavailable bounded allocation leave the
    /// caller on its existing exact path.
    pub(super) fn compile(catalog: &GeometryCatalog) -> Option<Self> {
        let width = catalog.width();
        let height = catalog.height();
        let cell_count = usize::from(width).checked_mul(usize::from(height))?;
        let row_count = catalog.skeleton_count();
        if width == 0
            || height == 0
            || height > MAX_CONDITION_HEIGHT
            || cell_count > u64::BITS as usize
            || row_count > MAX_CONDITION_ROWS
        {
            return None;
        }
        let all_cells = low_bits(cell_count as u32);
        if (catalog.initial_board() | catalog.required_cells()) & !all_cells != 0
            || catalog.initial_board() & catalog.required_cells() != 0
        {
            return None;
        }

        let mut placements = Vec::new();
        let mut domains_by_row = Vec::new();
        placements.try_reserve_exact(row_count).ok()?;
        domains_by_row.try_reserve_exact(row_count).ok()?;
        for index in 0..row_count {
            let row_id = u32::try_from(index).ok()?;
            let row = catalog.try_skeleton(row_id)?;
            if row.cells.count_ones() != 4 || row.cells & !catalog.required_cells() != 0 {
                return None;
            }
            let domain = catalog.exact_clear_domain_mask(row_id)?;
            domains_by_row.push((domain, row_id));
            placements.push(CanonicalPlacement {
                cells: row.cells,
                occupied_rows: catalog.skeleton_occupied_rows(row_id),
                domain_id: 0,
            });
        }

        // Sort exact values rather than admitting a hash collision as equality.
        // The auxiliary vector is bounded by the catalog, not by 2^placements.
        domains_by_row.sort_unstable();
        let distinct_domains = usize::from(!domains_by_row.is_empty())
            + domains_by_row
                .windows(2)
                .filter(|pair| pair[0].0 != pair[1].0)
                .count();
        let mut clear_domains = Vec::new();
        clear_domains.try_reserve_exact(distinct_domains).ok()?;
        for (domain, row_id) in domains_by_row {
            if clear_domains.last().copied() != Some(domain) {
                clear_domains.push(domain);
            }
            placements[row_id as usize].domain_id =
                u32::try_from(clear_domains.len().checked_sub(1)?).ok()?;
        }

        let order_guards = if precomputed_order_guards_enabled() {
            PrecomputedOrderGuards::compile(catalog, &placements, &clear_domains)
        } else {
            None
        };
        Some(Self {
            width,
            height,
            initial_board: catalog.initial_board(),
            placements,
            clear_domains,
            order_guards,
        })
    }

    #[cfg(test)]
    pub(super) const fn width(&self) -> u8 {
        self.width
    }

    #[cfg(test)]
    pub(super) const fn height(&self) -> u8 {
        self.height
    }

    #[cfg(test)]
    pub(super) const fn initial_board(&self) -> u64 {
        self.initial_board
    }

    pub(super) fn row_count(&self) -> usize {
        self.placements.len()
    }

    pub(super) fn domain_count(&self) -> usize {
        self.clear_domains.len()
    }

    pub(super) fn exact_clear_domain(&self, row_id: u32) -> Option<u64> {
        let row = self.placements.get(row_id as usize)?;
        self.clear_domains.get(row.domain_id as usize).copied()
    }

    pub(super) fn supports_deleted_rows(&self, row_id: u32, deleted_rows: u16) -> Option<bool> {
        if deleted_rows >= 1_u16 << self.height {
            return None;
        }
        Some(self.exact_clear_domain(row_id)? & (1_u64 << deleted_rows) != 0)
    }

    pub(super) const fn has_precomputed_order_guards(&self) -> bool {
        self.order_guards.is_some()
    }

    /// `Some(None)` is exact domain absence. An unavailable table, invalid
    /// row/frame, or failed lookup is `None` and retains the old calculation.
    pub(super) fn precomputed_order_guard(
        &self,
        row_id: u32,
        deleted: u16,
    ) -> Option<Option<PrecomputedOrderGuard>> {
        let owner = self.order_guards.as_ref()?;
        if deleted >= 1_u16 << self.height {
            return None;
        }
        let domain = self.exact_clear_domain(row_id)?;
        let bit = 1_u64 << deleted;
        if domain & bit == 0 {
            return Some(None);
        }
        let offset = *owner.offsets.get(row_id as usize)? as usize;
        let rank = (domain & (bit - 1)).count_ones() as usize;
        owner.guards.get(offset + rank).copied().map(Some)
    }

    /// Return all canonical placements whose *already executed*, surviving
    /// cells can occupy this physical footprint at the supplied row frame.
    ///
    /// A placement is intentionally not tested for a pending ILC instantiation:
    /// its deleted cells have already disappeared, while its surviving cells
    /// can still affect a collision or an ordered kick decision. Conversely,
    /// cells above the surviving target frame have no catalog contributor.
    /// This is a structural influence set, not a claim that every returned
    /// placement was selected or executed in a particular candidate.
    pub(super) fn influencing_placements(
        &self,
        catalog: &GeometryCatalog,
        physical_mask: u64,
        deleted_rows: u16,
    ) -> Option<Vec<u32>> {
        // Only the caller's pinned catalog can resolve the canonical IDs.
        // Matching dimensions, counts or digests do not establish ownership.
        if !catalog
            .placement_conditions()
            .is_some_and(|owner| core::ptr::eq(owner.as_ref(), self))
        {
            return None;
        }
        let all_cells = low_bits(u32::from(self.width) * u32::from(self.height));
        if physical_mask & !all_cells != 0 {
            return None;
        }
        let mut source_mask = if deleted_rows == 0 {
            physical_mask
        } else {
            AtomicRowProjection::new(self.width, self.height, deleted_rows)?
                .surviving_original_cells(physical_mask)
        };

        let mut support_count = 0_usize;
        let mut cells = source_mask;
        while cells != 0 {
            let cell = cells.trailing_zeros() as u8;
            cells &= cells - 1;
            support_count = support_count.checked_add(catalog.support(cell).len())?;
        }
        let mut rows = Vec::new();
        rows.try_reserve_exact(support_count).ok()?;
        while source_mask != 0 {
            let cell = source_mask.trailing_zeros() as u8;
            source_mask &= source_mask - 1;
            rows.extend_from_slice(catalog.support(cell));
        }
        rows.sort_unstable();
        rows.dedup();
        Some(rows)
    }

    /// Bind catalog variables to distinct, nonoverlapping selected operations.
    /// The returned formula uses execution bits in the input's order. Selection
    /// alone never marks a contributor as executed.
    #[cfg(test)]
    pub(super) fn specialize(&self, row_ids: &[u32]) -> Option<RowConditions> {
        if row_ids.is_empty()
            || row_ids.len() > super::MAX_BOARD64_PIECES
            || row_ids.len() > u16::BITS as usize
        {
            return None;
        }
        let mut row_contributors = [0_u16; MAX_CONDITION_HEIGHT as usize];
        let mut final_board = self.initial_board;
        for (operation_index, &row_id) in row_ids.iter().enumerate() {
            let placement = self.placements.get(row_id as usize)?;
            // This also rejects duplicate canonical IDs. Two different IDs of
            // the same kind remain separate variables when their cells differ.
            if final_board & placement.cells != 0 {
                return None;
            }
            final_board |= placement.cells;
            let mut occupied = placement.occupied_rows;
            while occupied != 0 {
                let row = occupied.trailing_zeros() as usize;
                occupied &= occupied - 1;
                row_contributors[row] |= 1_u16 << operation_index;
            }
        }

        let mut selected_rows = Vec::new();
        selected_rows.try_reserve_exact(row_ids.len()).ok()?;
        selected_rows.extend_from_slice(row_ids);
        let mut completed_rows = 0;
        let row_bits = low_bits(u32::from(self.width));
        let height = usize::from(self.height);
        for row in 0..height {
            let shift = row * usize::from(self.width);
            if (final_board >> shift) & row_bits != row_bits {
                // Even executing all selected operations cannot clear this row.
                continue;
            }
            completed_rows |= 1_u16 << row;
        }
        Some(RowConditions {
            selected_rows,
            grouped: GroupedClearConditions::from_completed_rows(
                &row_contributors[..height],
                completed_rows,
            )?,
        })
    }

    /// Additional owned heap payload; the containing object's size is counted
    /// by its owner, as for GeometryCatalog::retained_bytes.
    pub(super) fn retained_bytes(&self) -> usize {
        self.placements.capacity() * core::mem::size_of::<CanonicalPlacement>()
            + self.clear_domains.capacity() * core::mem::size_of::<u64>()
            + self
                .order_guards
                .as_ref()
                .map_or(0, PrecomputedOrderGuards::retained_bytes)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RowConditionGroup {
    contributors: u16,
    rows: u16,
}

/// Geometry-only row events in original coordinates. A group is a
/// conjunction over concrete execution bits, never piece-kind membership.
/// Rows with identical contributors complete in the same event; no
/// per-frame or per-subset table is allocated.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct GroupedClearConditions {
    groups: [RowConditionGroup; MAX_CONDITION_HEIGHT as usize],
    group_count: u8,
    completed_rows: u16,
}

impl GroupedClearConditions {
    /// `completed_rows` are exactly those full in the initial board plus all
    /// selected placements. Other rows cannot clear and need no condition.
    /// A row full without any operation must be precleared by the caller.
    pub(super) fn from_completed_rows(
        row_contributors: &[u16],
        completed_rows: u16,
    ) -> Option<Self> {
        if row_contributors.is_empty()
            || row_contributors.len() > MAX_CONDITION_HEIGHT as usize
            || completed_rows >> row_contributors.len() != 0
        {
            return None;
        }
        let mut result = Self {
            groups: [RowConditionGroup::default(); MAX_CONDITION_HEIGHT as usize],
            group_count: 0,
            completed_rows,
        };
        let mut pending = completed_rows;
        while pending != 0 {
            let row = pending.trailing_zeros() as usize;
            let bit = 1_u16 << row;
            pending ^= bit;
            let contributors = row_contributors[row];
            if contributors == 0 {
                return None;
            }
            let count = result.group_count as usize;
            if let Some(group) = result.groups[..count]
                .iter_mut()
                .find(|group| group.contributors == contributors)
            {
                group.rows |= bit;
            } else {
                result.groups[count] = RowConditionGroup {
                    contributors,
                    rows: bit,
                };
                result.group_count += 1;
            }
        }
        Some(result)
    }

    fn active_groups(&self) -> &[RowConditionGroup] {
        &self.groups[..self.group_count as usize]
    }

    pub(super) fn group_count(&self) -> usize {
        self.active_groups().len()
    }

    pub(super) fn atomic_group_count(&self) -> usize {
        self.active_groups()
            .iter()
            .filter(|group| group.rows.count_ones() > 1)
            .count()
    }

    /// Inline groups have no additional retained heap payload.
    pub(super) const fn retained_bytes(&self) -> usize {
        0
    }

    /// Yields `(concrete contributor mask, atomic original-row mask)`.
    pub(super) fn groups(&self) -> impl Iterator<Item = (u16, u16)> + '_ {
        self.active_groups()
            .iter()
            .map(|group| (group.contributors, group.rows))
    }

    #[cfg(test)]
    pub(super) fn contributors_for_row(&self, original_row: usize) -> Option<u16> {
        let bit = 1_u16.checked_shl(u32::try_from(original_row).ok()?)?;
        self.active_groups()
            .iter()
            .find(|group| group.rows & bit != 0)
            .map(|group| group.contributors)
    }

    pub(super) const fn completed_rows(&self) -> u16 {
        self.completed_rows
    }

    /// Which original rows complete for these execution bits. This says
    /// nothing about whether a legal order can reach the execution set.
    pub(super) fn deleted_rows(&self, executed: u16) -> u16 {
        self.active_groups().iter().fold(0, |deleted, group| {
            if group.contributors & !executed == 0 {
                deleted | group.rows
            } else {
                deleted
            }
        })
    }

    /// Current original-row frame and its exact single-operation changes.
    /// Each slot describes only that concrete operation, not a kind or a
    /// reachable child. Indices outside the caller's bounded slots are not
    /// represented; the ordinary subset predicate remains available.
    pub(super) fn deleted_rows_with_child_deltas<const N: usize>(
        &self,
        executed: u16,
    ) -> (u16, [u16; N]) {
        let mut deleted = 0;
        let mut deltas = [0; N];
        for group in self.active_groups() {
            let missing = group.contributors & !executed;
            if missing == 0 {
                deleted |= group.rows;
            } else if missing.is_power_of_two() {
                if let Some(slot) = deltas.get_mut(missing.trailing_zeros() as usize) {
                    *slot |= group.rows;
                }
            }
        }
        (deleted, deltas)
    }

    /// Minimal executed set in this geometric frame, or no consistent set.
    /// The minimum may include contributors to several atomic row groups.
    pub(super) fn required_for_frame(&self, deleted: u16) -> Option<u16> {
        if deleted & !self.completed_rows() != 0 {
            return None;
        }
        let required = self
            .active_groups()
            .iter()
            .filter(|group| group.rows & deleted != 0)
            .fold(0, |mask, group| mask | group.contributors);
        (self.deleted_rows(required) == deleted).then_some(required)
    }

    /// Operations individually possible in this geometric frame. `required`
    /// must be the minimum returned by `required_for_frame(deleted)` for this
    /// same object. The union is not itself a possible executed set: two
    /// optional operations might jointly clear an undeleted row.
    pub(super) fn possible_operations_in_frame(
        &self,
        deleted: u16,
        required: u16,
        all_operations: u16,
    ) -> u16 {
        debug_assert_eq!(self.required_for_frame(deleted), Some(required));
        debug_assert_eq!(required & !all_operations, 0);
        let mut impossible = 0;
        for group in self.active_groups() {
            if group.rows & deleted == 0 {
                let missing = group.contributors & !required;
                if missing.is_power_of_two() {
                    // Adding this one remaining contributor would clear a
                    // different row even on the minimum execution set.
                    impossible |= missing;
                }
            }
        }
        all_operations & !impossible
    }
}

/// A checked frame is still geometry-only: none of these results asserts
/// reachability, ordered-kick legality, or a valid supply order.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FramePrefixCompileError {
    OperationCountOutOfRange,
    NonCompletableRows,
    ContributorOutsideOperationDomain,
    NotClosed { closure: u16 },
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PrefixOperationOutsideDomain;

/// Exact extrema for a nonempty prefix slice. The upper mask is the union
/// of possible prefixes, not necessarily a possible prefix itself.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FramePrefixBounds {
    pub(super) minimum: u16,
    pub(super) individually_possible: u16,
}

/// The frame's prefixes are exactly the supersets of `minimum` that do not
/// contain any whole forbidden conjunction. Retaining only minimal forbidden
/// masks is exact: if A is a subset of B, forbidding A also forbids B.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FramePrefixDomain {
    deleted: u16,
    operations: u16,
    minimum: u16,
    forbidden: [u16; MAX_CONDITION_HEIGHT as usize],
    forbidden_count: u8,
}

#[cfg(test)]
const _: () = assert!(core::mem::size_of::<FramePrefixDomain>() <= 20);

#[cfg(test)]
impl GroupedClearConditions {
    pub(super) fn compile_frame_prefix(
        &self,
        deleted: u16,
        operation_count: u8,
    ) -> Result<FramePrefixDomain, FramePrefixCompileError> {
        if u32::from(operation_count) > u16::BITS {
            return Err(FramePrefixCompileError::OperationCountOutOfRange);
        }
        if deleted & !self.completed_rows() != 0 {
            return Err(FramePrefixCompileError::NonCompletableRows);
        }
        let operations = ((1_u32 << operation_count) - 1) as u16;
        let mut minimum = 0;
        for group in self.active_groups() {
            if group.contributors & !operations != 0 {
                return Err(FramePrefixCompileError::ContributorOutsideOperationDomain);
            }
            if group.rows & deleted != 0 {
                minimum |= group.contributors;
            }
        }
        // Forcing the contributors of every requested deleted row may also
        // delete other rows. The requested frame is possible exactly when
        // it is a fixed point of this closure.
        let closure = self.deleted_rows(minimum);
        if closure != deleted {
            return Err(FramePrefixCompileError::NotClosed { closure });
        }
        let mut domain = FramePrefixDomain {
            deleted,
            operations,
            minimum,
            forbidden: [0; MAX_CONDITION_HEIGHT as usize],
            forbidden_count: 0,
        };
        for group in self.active_groups() {
            if group.rows & deleted == 0 {
                let bad = group.contributors & !minimum;
                debug_assert_ne!(bad, 0, "the frame closure already ruled this out");
                domain.insert_minimal_forbidden(bad);
            }
        }
        Ok(domain)
    }
}

#[cfg(test)]
impl FramePrefixDomain {
    fn insert_minimal_forbidden(&mut self, bad: u16) {
        if self.forbidden_masks().iter().any(|&old| old & !bad == 0) {
            return;
        }
        let mut kept = 0;
        for index in 0..usize::from(self.forbidden_count) {
            let old = self.forbidden[index];
            if bad & !old != 0 {
                self.forbidden[kept] = old;
                kept += 1;
            }
        }
        // One insertion per original group, at most MAX_CONDITION_HEIGHT.
        self.forbidden[kept] = bad;
        self.forbidden[kept + 1..].fill(0);
        self.forbidden_count = (kept + 1) as u8;
    }

    pub(super) fn forbidden_masks(&self) -> &[u16] {
        &self.forbidden[..usize::from(self.forbidden_count)]
    }

    pub(super) fn deleted_rows(&self) -> u16 {
        self.deleted
    }

    pub(super) fn admits(&self, executed: u16) -> Result<bool, PrefixOperationOutsideDomain> {
        if executed & !self.operations != 0 {
            return Err(PrefixOperationOutsideDomain);
        }
        Ok(self.minimum & !executed == 0
            && self
                .forbidden_masks()
                .iter()
                .all(|bad| bad & !executed != 0))
    }

    /// Restrict the same frame by explicit executed or absent operations.
    /// Only the minimum and one missing operation per clause can affect the
    /// extrema. Work is linear in the bounded number of row groups. This
    /// does not close requirements under a separate precedence relation.
    pub(super) fn constrain(
        &self,
        must_execute: u16,
        must_be_absent: u16,
    ) -> Result<Option<FramePrefixBounds>, PrefixOperationOutsideDomain> {
        if (must_execute | must_be_absent) & !self.operations != 0 {
            return Err(PrefixOperationOutsideDomain);
        }
        let minimum = self.minimum | must_execute;
        if minimum & must_be_absent != 0 {
            return Ok(None);
        }
        let mut individually_possible = self.operations & !must_be_absent;
        for bad in self.forbidden_masks() {
            let missing = bad & !minimum;
            if missing == 0 {
                return Ok(None);
            }
            if missing.is_power_of_two() {
                individually_possible &= !missing;
            }
        }
        Ok(Some(FramePrefixBounds {
            minimum,
            individually_possible,
        }))
    }
}

/// Binds the grouped conjunctions to the selected canonical row identities.
#[cfg(test)]
#[derive(Debug, Eq, PartialEq)]
pub(super) struct RowConditions {
    selected_rows: Vec<u32>,
    grouped: GroupedClearConditions,
}

#[cfg(test)]
impl RowConditions {
    pub(super) fn operation_count(&self) -> usize {
        self.selected_rows.len()
    }

    pub(super) fn selected_rows(&self) -> &[u32] {
        &self.selected_rows
    }

    pub(super) fn group_count(&self) -> usize {
        self.grouped.group_count()
    }

    pub(super) fn atomic_group_count(&self) -> usize {
        self.grouped.atomic_group_count()
    }

    pub(super) fn completed_row_count(&self) -> u32 {
        self.grouped.completed_rows().count_ones()
    }

    /// Evaluate row-completion predicates on executed selected operations.
    /// Bits outside the selected-operation domain are irrelevant to the formula.
    /// This says which target rows are full, not whether a legal execution order
    /// exists for that set of operations.
    pub(super) fn deleted_rows(&self, placed_mask: u16) -> u16 {
        self.grouped.deleted_rows(placed_mask)
    }

    /// Rows newly completed by an execution-set change. Equal contributor sets
    /// appear atomically, including when one placement completes several rows.
    #[cfg(test)]
    pub(super) fn clear_delta(&self, before: u16, after: u16) -> u16 {
        self.deleted_rows(after) & !self.deleted_rows(before)
    }

    fn retained_bytes(&self) -> usize {
        self.selected_rows.capacity() * core::mem::size_of::<u32>()
    }
}

fn low_bits(count: u32) -> u64 {
    if count == u64::BITS {
        u64::MAX
    } else {
        (1_u64 << count) - 1
    }
}

#[cfg(test)]
#[path = "placement_conditions_algebra_tests.rs"]
mod algebra_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::piece::piece_kind::PieceKind;

    fn catalog(width: u8, height: u8) -> GeometryCatalog {
        let required = low_bits(u32::from(width) * u32::from(height));
        GeometryCatalog::compile_for_required_cells_on_dimensions(width, height, 0, required)
            .unwrap()
    }

    fn attached_catalog(width: u8, height: u8) -> GeometryCatalog {
        super::super::placement_dependency::with_mode(true, || catalog(width, height)).0
    }

    fn o_id(catalog: &GeometryCatalog, lower_row: u8, upper_row: u8, x: u8) -> u32 {
        let lower_shift = u32::from(lower_row) * u32::from(catalog.width()) + u32::from(x);
        let upper_shift = u32::from(upper_row) * u32::from(catalog.width()) + u32::from(x);
        catalog
            .skeleton_id(
                PieceKind::O,
                (3_u64 << lower_shift) | (3_u64 << upper_shift),
            )
            .unwrap()
    }

    #[test]
    fn exact_ilc_disjunction_interns_equal_domains_without_merging_placements() {
        let catalog = catalog(4, 4);
        let conditions = PlacementConditionCatalog::compile(&catalog).unwrap();
        let left = o_id(&catalog, 0, 2, 0);
        let right = o_id(&catalog, 0, 2, 2);
        let lower = o_id(&catalog, 0, 1, 0);
        assert_ne!(left, right);
        assert_ne!(catalog.skeleton(left).cells, catalog.skeleton(right).cells);
        assert_eq!(
            conditions.exact_clear_domain(left),
            Some((1_u64 << 0b0010) | (1_u64 << 0b1010))
        );
        assert_eq!(
            conditions.placements[left as usize].domain_id,
            conditions.placements[right as usize].domain_id
        );
        assert_ne!(
            conditions.exact_clear_domain(left),
            conditions.exact_clear_domain(lower)
        );
        assert_eq!(conditions.supports_deleted_rows(left, 0b0010), Some(true));
        assert_eq!(conditions.supports_deleted_rows(left, 0b1010), Some(true));
        assert_eq!(conditions.supports_deleted_rows(left, 0b0000), Some(false));
        // The minimum ILC gap is present, but an occupied target row is gone.
        assert!(catalog.realization_requirement_is_satisfied(left, 0b0110));
        assert_eq!(conditions.supports_deleted_rows(left, 0b0110), Some(false));
        assert!(conditions.domain_count() < conditions.row_count());
    }

    #[test]
    fn all_compiled_domains_match_exact_catalog_instantiation() {
        let catalog = catalog(4, 4);
        let conditions = PlacementConditionCatalog::compile(&catalog).unwrap();
        for row_id in 0..catalog.skeleton_count() as u32 {
            let row = catalog.skeleton(row_id);
            for deleted in 0..16 {
                // Bypass both the compact table and the optional domain cache
                // so a missing table interval cannot validate itself.
                let directly_projected = catalog.realizations(row_id).iter().any(|&realization| {
                    catalog
                        .instantiate_realization(row.cells, realization, deleted)
                        .is_some()
                });
                assert_eq!(
                    catalog.instantiations(row_id, deleted).next().is_some(),
                    directly_projected
                );
                assert_eq!(
                    conditions.supports_deleted_rows(row_id, deleted),
                    Some(directly_projected),
                    "row={row_id} deleted={deleted}"
                );
            }
        }
    }

    #[test]
    fn order_guards_match_original_to_physical_intersections_for_every_lifted_cell() {
        for (width, height, initial) in [(4, 4, 0), (4, 4, 0x4310), (10, 4, 0xf0)] {
            let size = width * height;
            let full = low_bits(u32::from(size));
            let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
                width,
                height,
                initial,
                full & !initial,
            )
            .unwrap();
            let conditions = with_precomputed_order_guards(true, || {
                PlacementConditionCatalog::compile(&catalog)
            })
            .unwrap();
            let guards = conditions.order_guards.as_ref().expect("small guard table");
            assert!(
                guards.retained_bytes() as u128
                    <= PlacementConditionCatalog::order_guard_compile_heap_bound(
                        conditions.row_count(),
                        width,
                        height
                    )
            );
            assert!(conditions.precomputed_order_guard(0, 1 << height).is_none());
            for row_id in 0..catalog.skeleton_count() as u32 {
                let original_lock = catalog.skeleton(row_id).cells;
                for deleted in 0..1_u16 << height {
                    let guard = conditions
                        .precomputed_order_guard(row_id, deleted)
                        .expect("lookup known within compiled domain");
                    let instantiation = catalog.instantiations(row_id, deleted).next();
                    assert_eq!(guard.is_some(), instantiation.is_some());
                    let (Some(guard), Some(instantiation)) = (guard, instantiation) else {
                        continue;
                    };
                    let lock = instantiation.lock_mask;
                    // Intersection with a union equals the disjunction of the
                    // intersections. Check each basis cell (and the empty
                    // contribution) to cover arbitrary original-board subsets.
                    for extra in std::iter::once(0).chain((0..size).map(|i| 1_u64 << i)) {
                        if extra & original_lock != 0 {
                            continue;
                        }
                        let physical = super::super::buildup::compact_target_board(
                            width,
                            height,
                            initial | extra,
                            deleted,
                        );
                        let expected_grounded =
                            super::super::buildup::placement_is_grounded(width, physical, lock);
                        let expected_blocked =
                            (0..height).any(|dy| physical & (lock << (width * dy)) != 0);
                        assert_eq!(
                            guard.ground_at_initial() || guard.support_cells() & extra != 0,
                            expected_grounded,
                            "ground row={row_id} D={deleted} extra={extra:#x}",
                        );
                        assert_eq!(
                            guard.corridor_hits_initial() || guard.corridor_cells() & extra != 0,
                            expected_blocked,
                            "corridor row={row_id} D={deleted} extra={extra:#x}",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn executed_survivors_influence_even_when_pending_instantiation_is_impossible() {
        let catalog = attached_catalog(4, 4);
        let conditions = catalog.placement_conditions().unwrap();
        let split = o_id(&catalog, 0, 2, 0);
        assert_eq!(conditions.supports_deleted_rows(split, 0b0001), Some(false));
        // Deleting original row 0 leaves the piece's original row 2 at physical 1.
        let influences = conditions
            .influencing_placements(&catalog, 3 << 4, 0b0001)
            .unwrap();
        assert!(influences.contains(&split));
        assert!(!conditions
            .influencing_placements(&catalog, 3, 0b0001)
            .unwrap()
            .contains(&split));
        assert!(!conditions
            .influencing_placements(&catalog, 0xffff, 0b0101)
            .unwrap()
            .contains(&split));
        assert!(conditions
            .influencing_placements(&catalog, 0xf000, 0b0001)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn influence_incidence_matches_surviving_cell_projection_for_every_row_frame() {
        let catalog = attached_catalog(4, 4);
        let conditions = catalog.placement_conditions().unwrap();
        // Each physical cell independently witnesses the exact incidence set.
        for deleted in 0..16 {
            for cell in 0..16 {
                let mask = 1_u64 << cell;
                let actual = conditions
                    .influencing_placements(&catalog, mask, deleted)
                    .unwrap();
                let expected: Vec<u32> = (0..catalog.skeleton_count() as u32)
                    .filter(|&row_id| {
                        super::super::buildup::compact_target_board(
                            4,
                            4,
                            catalog.skeleton(row_id).cells,
                            deleted,
                        ) & mask
                            != 0
                    })
                    .collect();
                assert_eq!(actual, expected, "cell={cell} deleted={deleted}");
            }
        }
    }

    #[test]
    fn influence_incidence_rejects_a_separately_compiled_owner_even_with_identical_rows() {
        let catalog = attached_catalog(4, 4);
        let same_rows = attached_catalog(4, 4);
        let other_frame = attached_catalog(4, 5);
        let conditions = catalog.placement_conditions().unwrap();
        assert_eq!(catalog.identity_digest(), same_rows.identity_digest());
        assert!(conditions.influencing_placements(&catalog, 1, 0).is_some());
        assert!(conditions
            .influencing_placements(&same_rows, 1, 0)
            .is_none());
        assert!(conditions
            .influencing_placements(&other_frame, 1, 0)
            .is_none());
    }

    #[test]
    fn same_kind_canonical_contributors_stay_distinct_and_clear_rows_atomically() {
        let catalog = catalog(4, 4);
        let conditions = PlacementConditionCatalog::compile(&catalog).unwrap();
        let selected = [
            o_id(&catalog, 0, 1, 0),
            o_id(&catalog, 0, 1, 2),
            o_id(&catalog, 2, 3, 0),
            o_id(&catalog, 2, 3, 2),
        ];
        let rows = conditions.specialize(&selected).unwrap();
        assert_eq!(rows.selected_rows(), selected);
        assert_eq!(rows.operation_count(), 4);
        assert_eq!(rows.group_count(), 2);
        assert_eq!(rows.atomic_group_count(), 2);
        assert_eq!(rows.completed_row_count(), 4);
        assert_eq!(rows.deleted_rows(0b0001), 0);
        assert_eq!(rows.deleted_rows(0b0101), 0);
        assert_eq!(rows.deleted_rows(0b0011), 0b0011);
        assert_eq!(rows.deleted_rows(0b1100), 0b1100);
        assert_eq!(rows.clear_delta(0b0011, 0b0111), 0);
        assert_eq!(rows.clear_delta(0b0111, 0b1111), 0b1100);
        assert_eq!(rows.clear_delta(0b0001, 0b0011), 0b0011);

        // Changing selection order changes bit positions, not canonical identity.
        let reordered = conditions
            .specialize(&[selected[2], selected[0], selected[3], selected[1]])
            .unwrap();
        assert_eq!(reordered.deleted_rows(0b1010), 0b0011);
        assert_eq!(reordered.deleted_rows(0b0101), 0b1100);
    }

    #[test]
    fn an_incomplete_target_row_never_becomes_a_clear_condition() {
        let catalog = catalog(4, 4);
        let conditions = PlacementConditionCatalog::compile(&catalog).unwrap();
        let rows = conditions.specialize(&[o_id(&catalog, 0, 1, 0)]).unwrap();
        assert_eq!(rows.group_count(), 0);
        assert_eq!(rows.deleted_rows(1), 0);
        assert_eq!(rows.clear_delta(0, 1), 0);
    }

    #[test]
    fn initial_cells_are_constants_in_the_contributor_formula() {
        let initial = 0x0033;
        let required = 0x00cc;
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, initial, required)
                .unwrap();
        let conditions = PlacementConditionCatalog::compile(&catalog).unwrap();
        let rows = conditions.specialize(&[o_id(&catalog, 0, 1, 2)]).unwrap();
        assert_eq!(conditions.initial_board(), initial);
        assert_eq!(catalog.required_cells(), required);
        assert_eq!(rows.group_count(), 1);
        assert_eq!(rows.atomic_group_count(), 1);
        assert_eq!(rows.deleted_rows(0), 0);
        assert_eq!(rows.clear_delta(0, 1), 0b0011);
    }

    fn scalar_row_mask(width: usize, height: usize, board: u64) -> u16 {
        let mut cleared = 0;
        for y in 0..height {
            if (0..width).all(|x| board & (1_u64 << (y * width + x)) != 0) {
                cleared |= 1_u16 << y;
            }
        }
        cleared
    }

    fn scalar_project(width: usize, height: usize, deleted: u16, cells: u64) -> u64 {
        let mut next = 0;
        let mut physical_row = 0;
        for original_row in 0..height {
            if deleted & (1_u16 << original_row) != 0 {
                continue;
            }
            for x in 0..width {
                if cells & (1_u64 << (original_row * width + x)) != 0 {
                    next |= 1_u64 << (physical_row * width + x);
                }
            }
            physical_row += 1;
        }
        next
    }

    fn scalar_grounding(width: usize, height: usize, board: u64, placement: u64) -> bool {
        (0..height).any(|y| {
            (0..width).any(|x| {
                placement & (1_u64 << (y * width + x)) != 0
                    && (y == 0 || board & (1_u64 << ((y - 1) * width + x)) != 0)
            })
        })
    }

    #[test]
    fn atomic_projection_preserves_surviving_cells_for_each_compact_frame() {
        for (width, height) in [(4, 4), (10, 6), (64, 1), (32, 2), (16, 4)] {
            let area = u32::from(width) * u32::from(height);
            for deleted in 0..(1_u16 << height) {
                let map = AtomicRowProjection::new(width, height, deleted).unwrap();
                let surviving = (0..height).fold(0, |mask, original| {
                    if deleted & (1_u16 << original) == 0 {
                        mask | (low_bits(u32::from(width))
                            << (u32::from(original) * u32::from(width)))
                    } else {
                        mask
                    }
                });
                for cell in (0..area).map(|bit| 1_u64 << bit).chain([u64::MAX]) {
                    let physical =
                        scalar_project(usize::from(width), usize::from(height), deleted, cell);
                    assert_eq!(map.physical_board(cell), physical);
                    assert_eq!(map.surviving_original_cells(physical), cell & surviving);
                }
                // Influence in the above-board headroom has no contributor.
                assert_eq!(map.surviving_original_cells(u64::MAX), surviving);
            }
        }
        assert!(AtomicRowProjection::new(0, 4, 0).is_none());
        assert!(AtomicRowProjection::new(16, 5, 0).is_none());
        assert!(AtomicRowProjection::new(4, 7, 0).is_none());
        assert!(AtomicRowProjection::new(4, 4, 1 << 4).is_none());
    }

    #[test]
    fn concrete_support_or_and_grouped_clears_match_the_scalar_grid() {
        let (width, height) = (10, 3);
        let initial = 0x3f0f_83f4;
        let placements = [0x00f0_0000, 0x0c03, 0x7008];
        let required = placements.iter().fold(0, |mask, &cells| mask | cells);
        let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
            width as u8,
            height as u8,
            initial,
            required,
        )
        .unwrap();
        let selected = [PieceKind::I, PieceKind::O, PieceKind::T]
            .into_iter()
            .zip(placements)
            .map(|(piece, cells)| catalog.skeleton_id(piece, cells).unwrap())
            .collect::<Vec<_>>();
        let owner = PlacementConditionCatalog::compile(&catalog).unwrap();
        let candidate = owner.specialize(&selected).unwrap();
        let groups = &candidate.grouped;
        assert_eq!(candidate.selected_rows(), selected.as_slice());
        assert_eq!(groups.completed_rows(), 0b111);
        assert_eq!(candidate.atomic_group_count(), 1);
        assert_eq!(groups.contributors_for_row(0), Some(0b110));
        assert_eq!(groups.contributors_for_row(1), Some(0b110));
        assert_eq!(groups.contributors_for_row(2), Some(0b001));
        assert_eq!(groups.contributors_for_row(3), None);
        assert_eq!(candidate.clear_delta(0b010, 0b110), 0b011);
        assert_eq!(candidate.clear_delta(0b110, 0b111), 0b100);

        for deleted in 0..8_u16 {
            let mut required_in_frame = 0b111;
            let mut possible_in_frame = 0;
            let mut observed_frame = false;
            let projection = AtomicRowProjection::new(width as u8, height as u8, deleted).unwrap();
            let projected_initial = projection.physical_board(initial);
            let projected = placements.map(|cells| projection.physical_board(cells));
            for executed in 0..8_u16 {
                let logical = placements
                    .iter()
                    .enumerate()
                    .fold(initial, |board, (op, &cells)| {
                        if executed & (1_u16 << op) != 0 {
                            board | cells
                        } else {
                            board
                        }
                    });
                let observed_deleted = scalar_row_mask(width, height, logical);
                assert_eq!(candidate.deleted_rows(executed), observed_deleted);
                if observed_deleted != deleted {
                    continue;
                }
                observed_frame = true;
                required_in_frame &= executed;
                possible_in_frame |= executed;
                let physical = scalar_project(width, height, deleted, logical);
                let surviving = projection.surviving_original_cells(physical);
                let combined = (0..projected.len())
                    .filter(|op| executed & (1_u16 << op) != 0)
                    .fold(projected_initial, |board, op| board | projected[op]);
                assert_eq!(physical, combined);
                for (op, &cells) in placements.iter().enumerate() {
                    let bit = 1_u16 << op;
                    if executed & bit != 0 {
                        continue;
                    }
                    let next_deleted = scalar_row_mask(width, height, logical | cells);
                    assert_eq!(candidate.deleted_rows(executed | bit), next_deleted);
                    let next_map =
                        AtomicRowProjection::new(width as u8, height as u8, next_deleted).unwrap();
                    // The former frame has already dropped its cleared cells.
                    // They cannot reappear across the next (possibly atomic)
                    // clear. Surviving executed cells still participate.
                    assert_eq!(
                        next_map.physical_board(surviving | cells),
                        scalar_project(width, height, next_deleted, logical | cells)
                    );
                }
                for (op, &lock_mask) in projected.iter().enumerate() {
                    let pending = 1_u16 << op;
                    if executed & pending != 0 {
                        continue;
                    }
                    let source = grounding_sources(
                        width as u8,
                        projected_initial,
                        lock_mask,
                        &projected,
                        0b111 ^ pending,
                    );
                    if lock_mask.count_ones() != 4 {
                        assert_eq!(source, None);
                        continue;
                    }
                    let grounded = match source.unwrap() {
                        GroundingSources::AlreadyGrounded => true,
                        GroundingSources::AnyOf(choices) => {
                            assert_ne!(choices, 0);
                            (choices & executed) != 0
                        }
                        GroundingSources::Ungrounded => false,
                    };
                    assert_eq!(
                        grounded,
                        scalar_grounding(width, height, physical, lock_mask)
                    );
                }
            }
            assert_eq!(
                groups.required_for_frame(deleted),
                observed_frame.then_some(required_in_frame)
            );
            if observed_frame {
                assert_eq!(
                    groups.possible_operations_in_frame(deleted, required_in_frame, 0b111),
                    possible_in_frame
                );
            }
        }
        assert_eq!(
            grounding_sources(10, initial, placements[0], &placements, 0b110),
            Some(GroundingSources::AnyOf(0b110))
        );
    }

    #[test]
    fn same_kind_support_bits_remain_distinct_and_deleted_cells_are_not_supporters() {
        let placements = [0x0033, 0x00cc, 0x0f00, 0xf000];
        let width = 4;
        assert_eq!(
            grounding_sources(width, 0, placements[2], &placements, 0b1011),
            Some(GroundingSources::AnyOf(0b0011))
        );
        assert_eq!(
            grounding_sources(width, 0, placements[2], &placements, 0b1001),
            Some(GroundingSources::AnyOf(0b0001))
        );
        assert_eq!(
            grounding_sources(width, 0, placements[2], &placements, 0b1000),
            Some(GroundingSources::Ungrounded)
        );
        assert_eq!(
            grounding_sources(width, 0x0010, placements[2], &placements, 0b1000),
            Some(GroundingSources::AlreadyGrounded)
        );
        let after_two_clears = placements.map(|cells| scalar_project(4, 4, 0b0011, cells));
        assert_eq!(after_two_clears, [0, 0, 0x000f, 0x00f0]);
        assert_eq!(
            grounding_sources(width, 0, after_two_clears[2], &after_two_clears, 0b1011),
            Some(GroundingSources::AlreadyGrounded)
        );
        assert_eq!(
            grounding_sources(width, 0, placements[2], &placements, 0b0100),
            None
        );
        assert_eq!(
            grounding_sources(width, 0, placements[2], &placements, 0b1_0000),
            None
        );
        assert_eq!(
            grounding_sources(0, 0, placements[2], &placements, 0b1011),
            None
        );
    }

    #[test]
    fn unsupported_and_mismatched_inputs_return_fallback_without_aliasing() {
        let tall = catalog(4, 7);
        let catalog = attached_catalog(4, 4);
        let conditions = catalog.placement_conditions().unwrap();
        let left = o_id(&catalog, 0, 1, 0);
        let overlapping = o_id(&catalog, 0, 1, 1);
        let missing = catalog.skeleton_count() as u32;
        assert!(conditions.specialize(&[]).is_none());
        assert!(conditions.specialize(&[left, left]).is_none());
        assert!(conditions.specialize(&[left, overlapping]).is_none());
        assert!(conditions.specialize(&[missing]).is_none());
        assert!(conditions
            .specialize(&[left; super::super::MAX_BOARD64_PIECES + 1])
            .is_none());
        assert_eq!(conditions.exact_clear_domain(missing), None);
        assert_eq!(conditions.supports_deleted_rows(left, 0b1_0000), None);
        assert_eq!(conditions.supports_deleted_rows(missing, 0), None);
        assert!(conditions
            .influencing_placements(&catalog, 1 << 16, 0)
            .is_none());
        assert!(conditions
            .influencing_placements(&catalog, 1, 0b1_0000)
            .is_none());
        assert!(PlacementConditionCatalog::compile(&tall).is_none());
    }

    #[test]
    fn initial_full_rows_require_preclear_normalization() {
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0x000f, 0x0330)
                .unwrap();
        let conditions = PlacementConditionCatalog::compile(&catalog).unwrap();
        assert!(conditions.specialize(&[o_id(&catalog, 1, 2, 0)]).is_none());
    }

    #[test]
    fn board64_masks_and_six_row_domains_are_bounded() {
        assert_eq!(low_bits(64), u64::MAX);
        // The owning geometry projection currently supports width at most 10.
        // Exercise its actual largest six-row frame as well as bit 63 in D.
        let six = attached_catalog(10, 6);
        let six_conditions = six.placement_conditions().unwrap();
        assert_eq!(six_conditions.width(), 10);
        assert_eq!(six_conditions.height(), 6);
        assert!(six_conditions
            .influencing_placements(&six, low_bits(60), 0)
            .is_some());
        assert!(six_conditions
            .influencing_placements(&six, low_bits(60), 63)
            .unwrap()
            .is_empty());
        assert!(six_conditions
            .influencing_placements(&six, 1 << 60, 0)
            .is_none());
        for row in 0..six.skeleton_count() as u32 {
            assert_eq!(six_conditions.supports_deleted_rows(row, 63), Some(false));
        }
        assert!(six_conditions.retained_bytes() > 0);
        assert!(
            six_conditions
                .specialize(&[o_id(&six, 0, 1, 0)])
                .unwrap()
                .retained_bytes()
                > 0
        );
        let admitted =
            PlacementConditionCatalog::checked_compile_peak_upper_bound(six_conditions.row_count())
                .unwrap();
        assert!(admitted >= six_conditions.retained_bytes() as u128);
        assert_eq!(
            PlacementConditionCatalog::checked_compile_peak_upper_bound(usize::MAX),
            PlacementConditionCatalog::checked_compile_peak_upper_bound(MAX_CONDITION_ROWS)
        );
    }
}
