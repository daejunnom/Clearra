use super::{
    catalog::GeometryCatalog, geometry::TargetGroup, geometry_apdp::partial_shape_kind, piece_index,
};

const BUMPER_DOMAIN_MAX_RESIDUAL_CELLS: u32 = 24;

const ALL_STANDARD_PIECES: u8 = 0x7f;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DomainPropagation {
    pub pivot_required_cells: u64,
    pub pivot_piece_mask: u8,
    pub pivot_support_count: usize,
    pub pivot_cell: u8,
    bumper_cell: u8,
    apdp_required_cells: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DomainCompilation {
    pub status: DomainStatus,
    pub propagation: DomainPropagation,
    pub cell_piece_masks: [u8; 64],
    #[cfg(any(feature = "local-search-ab", test))]
    pub observation: DomainCompileObservation,
}

#[cfg(any(feature = "local-search-ab", test))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct DomainCompileObservation {
    pub support_cells_examined: u64,
    pub same_tile_parent_scans: u8,
    pub same_tile_parent_reuses: u8,
    pub apdp_static_check: bool,
    pub apdp_static_complete: bool,
    pub apdp_pivot_tightened: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DomainStatus {
    Supported,
    Empty,
}

/// Work on direct support incidences only; bumper-domain scans are unchanged
/// and excluded. A cell is counted as cut off only when an incidence was skipped.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct MinimumDomainSupportWork {
    pub incidence_rows_scanned: u64,
    pub incidence_rows_skipped: u64,
    pub cells_cut_off: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SameTileParentMode {
    Scan,
    #[cfg(any(feature = "local-search-ab", test))]
    Reuse,
}

impl SameTileParentMode {
    pub fn from_experiment() -> Self {
        #[cfg(any(feature = "local-search-ab", test))]
        {
            static MODE: std::sync::OnceLock<SameTileParentMode> = std::sync::OnceLock::new();
            *MODE.get_or_init(|| {
                if std::env::var("CLEARRA_SAME_TILE_PARENT_REUSE").as_deref() == Ok("1") {
                    Self::Reuse
                } else {
                    Self::Scan
                }
            })
        }
        #[cfg(not(any(feature = "local-search-ab", test)))]
        Self::Scan
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CellSupport {
    // The catalog indexes every full parent with a u32 row ID.
    count: u32,
    pieces: u8,
    common_owner_cells: u64,
}

impl CellSupport {
    fn compile(catalog: &GeometryCatalog, cell: u8, remaining: u64, piece_mask: u8) -> Self {
        let mut support = Self {
            common_owner_cells: remaining,
            ..Self::default()
        };
        for &row_id in catalog.support(cell) {
            let row = catalog.skeleton(row_id);
            let piece_bit = 1_u8 << piece_index(row.piece);
            if row.cells & remaining == row.cells && piece_mask & piece_bit != 0 {
                support.count += 1;
                support.pieces |= piece_bit;
                support.common_owner_cells &= row.cells;
            }
        }
        support
    }
}

#[derive(Debug)]
struct DomainSupportSnapshot {
    depth: u8,
    remaining: u64,
    piece_mask: u8,
    cells: [CellSupport; 64],
}

impl DomainSupportSnapshot {
    fn dirty_cells(&self, catalog: &GeometryCatalog, remaining: u64, piece_mask: u8) -> u64 {
        // Restored/expanded row eligibility is never evaluated using a
        // descendant's intersections.
        if remaining & !self.remaining != 0 || piece_mask & !self.piece_mask != 0 {
            return remaining;
        }

        let removed_pieces = self.piece_mask & !piece_mask;
        let mut dirty = 0;
        if removed_pieces != 0 {
            let mut active = remaining;
            while active != 0 {
                let cell = active.trailing_zeros() as usize;
                let bit = active.isolate_lowest_one();
                active &= active - 1;
                if self.cells[cell].pieces & removed_pieces != 0 {
                    dirty |= bit;
                }
            }
        }

        let removed = self.remaining & !remaining;
        let mut deleted = removed;
        while deleted != 0 && dirty != remaining {
            let cell = deleted.trailing_zeros() as u8;
            let bit = deleted.isolate_lowest_one();
            deleted &= deleted - 1;
            for &row_id in catalog.support(cell) {
                let row = catalog.skeleton(row_id);
                if row.cells & self.remaining == row.cells
                    && (row.cells & removed).isolate_lowest_one() == bit
                    && piece_mask & (1_u8 << piece_index(row.piece)) != 0
                {
                    dirty |= row.cells & remaining;
                }
            }
        }
        dirty
    }
}

/// Exact full-parent cell summaries along one geometry compiler's DFS, over
/// the same immutable catalog as its residual memo. These snapshots are
/// optional scratch: allocation failure just leaves us using fresh sums.
#[derive(Debug, Default)]
pub(super) struct DomainSupportCache {
    ancestors: Vec<DomainSupportSnapshot>,
    #[cfg(any(feature = "local-search-ab", test))]
    reused_support_cells: u32,
}

impl DomainSupportCache {
    pub fn compile(
        &mut self,
        catalog: &GeometryCatalog,
        depth: u8,
        remaining: u64,
        piece_mask: u8,
        same_tile_parents: SameTileParentMode,
    ) -> DomainCompilation {
        while self
            .ancestors
            .last()
            .is_some_and(|parent| parent.depth >= depth)
        {
            self.ancestors.pop();
        }
        let parent = self.ancestors.last();
        let dirty = parent.map_or(remaining, |parent| {
            parent.dirty_cells(catalog, remaining, piece_mask)
        });
        let mut cells = [CellSupport::default(); 64];
        let result = DomainPropagation::compile_with_support(
            catalog,
            remaining,
            piece_mask,
            same_tile_parents,
            |cell| {
                let support = match parent {
                    Some(parent) if dirty & (1_u64 << cell) == 0 => parent.cells[cell as usize],
                    _ => CellSupport::compile(catalog, cell, remaining, piece_mask),
                };
                cells[cell as usize] = support;
                support
            },
        );
        #[cfg(any(feature = "local-search-ab", test))]
        {
            self.reused_support_cells =
                (result.observation.support_cells_examined & !dirty).count_ones();
        }
        if result.status == DomainStatus::Supported && self.ancestors.try_reserve(1).is_ok() {
            self.ancestors.push(DomainSupportSnapshot {
                depth,
                remaining,
                piece_mask,
                cells,
            });
        }
        result
    }

    pub fn retained_bytes(&self) -> usize {
        self.ancestors.capacity() * core::mem::size_of::<DomainSupportSnapshot>()
    }

    #[cfg(any(feature = "local-search-ab", test))]
    pub fn reused_support_cells(&self) -> u32 {
        self.reused_support_cells
    }
}

impl DomainPropagation {
    pub const fn empty() -> Self {
        Self {
            pivot_required_cells: 0,
            pivot_piece_mask: 0,
            pivot_support_count: 0,
            pivot_cell: u8::MAX,
            bumper_cell: u8::MAX,
            apdp_required_cells: 0,
        }
    }

    pub fn compile(
        catalog: &GeometryCatalog,
        remaining: u64,
        feasible_piece_mask: u8,
    ) -> DomainCompilation {
        Self::compile_with_mode(
            catalog,
            remaining,
            feasible_piece_mask,
            SameTileParentMode::Scan,
        )
    }

    pub fn compile_with_mode(
        catalog: &GeometryCatalog,
        remaining: u64,
        feasible_piece_mask: u8,
        same_tile_parents: SameTileParentMode,
    ) -> DomainCompilation {
        Self::compile_with_support(
            catalog,
            remaining,
            feasible_piece_mask,
            same_tile_parents,
            |cell| CellSupport::compile(catalog, cell, remaining, feasible_piece_mask),
        )
    }

    fn compile_with_support(
        catalog: &GeometryCatalog,
        remaining: u64,
        feasible_piece_mask: u8,
        same_tile_parents: SameTileParentMode,
        mut support_for: impl FnMut(u8) -> CellSupport,
    ) -> DomainCompilation {
        let mut result = Self {
            pivot_required_cells: 0,
            pivot_piece_mask: 0,
            pivot_support_count: usize::MAX,
            pivot_cell: u8::MAX,
            bumper_cell: u8::MAX,
            apdp_required_cells: 0,
        };
        let mut cell_piece_masks = [0_u8; 64];
        let mut common_owner_cells = [0_u64; 64];
        let mut parents = core::array::from_fn(|index| index as u8);
        #[cfg(any(feature = "local-search-ab", test))]
        let mut observation = DomainCompileObservation::default();
        #[cfg(any(feature = "local-search-ab", test))]
        let mut support_counts = match same_tile_parents {
            SameTileParentMode::Scan => None,
            SameTileParentMode::Reuse => Some([0_u32; 64]),
        };
        #[cfg(not(any(feature = "local-search-ab", test)))]
        let _ = same_tile_parents;

        let mut cells = remaining;
        while cells != 0 {
            let cell = cells.trailing_zeros() as u8;
            cells &= cells - 1;
            let support = support_for(cell);
            let support_count = support.count as usize;
            cell_piece_masks[cell as usize] = support.pieces;
            common_owner_cells[cell as usize] = support.common_owner_cells;
            #[cfg(any(feature = "local-search-ab", test))]
            if let Some(counts) = support_counts.as_mut() {
                counts[cell as usize] = support.count;
            }
            if support_count == 0 {
                #[cfg(any(feature = "local-search-ab", test))]
                {
                    observation.support_cells_examined = remaining ^ cells;
                }
                result.pivot_cell = cell;
                result.pivot_required_cells = 1_u64 << cell;
                result.pivot_support_count = 0;
                return DomainCompilation {
                    status: DomainStatus::Empty,
                    propagation: result,
                    cell_piece_masks,
                    #[cfg(any(feature = "local-search-ab", test))]
                    observation,
                };
            }
            if support_count < result.pivot_support_count {
                result.pivot_cell = cell;
                result.pivot_required_cells = 1_u64 << cell;
                result.pivot_support_count = support_count;
                result.pivot_piece_mask = support.pieces;
            }
        }
        #[cfg(any(feature = "local-search-ab", test))]
        {
            observation.support_cells_examined = remaining;
        }

        // Every feasible full placement covering a cell also covers the
        // intersection recorded here. Transitive SameTile groups therefore
        // remain exact certificates, not visual-shape guesses.
        cells = remaining;
        while cells != 0 {
            let cell = cells.trailing_zeros() as u8;
            cells &= cells - 1;
            let mut common = common_owner_cells[cell as usize];
            while common != 0 {
                let other = common.trailing_zeros() as u8;
                common &= common - 1;
                union_cells(&mut parents, cell, other);
            }
        }

        let mut groups = [0_u64; 64];
        cells = remaining;
        while cells != 0 {
            let cell = cells.trailing_zeros() as u8;
            cells &= cells - 1;
            let root = find_root(&mut parents, cell);
            groups[root as usize] |= 1_u64 << cell;
        }
        for required in groups.into_iter().filter(|group| group.count_ones() > 1) {
            if required.count_ones() > 4 {
                result.pivot_required_cells = required;
                result.pivot_support_count = 0;
                result.pivot_cell = required.trailing_zeros() as u8;
                return DomainCompilation {
                    status: DomainStatus::Empty,
                    propagation: result,
                    cell_piece_masks,
                    #[cfg(any(feature = "local-search-ab", test))]
                    observation,
                };
            }
            #[cfg(any(feature = "local-search-ab", test))]
            let from_summary = support_counts.as_ref().and_then(|counts| {
                same_tile_parent_summary(required, &common_owner_cells, counts, &cell_piece_masks)
            });
            #[cfg(not(any(feature = "local-search-ab", test)))]
            let from_summary = None;
            let (count, piece_mask) = if let Some(summary) = from_summary {
                #[cfg(any(feature = "local-search-ab", test))]
                {
                    observation.same_tile_parent_reuses += 1;
                }
                summary
            } else {
                #[cfg(any(feature = "local-search-ab", test))]
                {
                    observation.same_tile_parent_scans += 1;
                }
                exact_parent_rows(catalog, remaining, feasible_piece_mask, required, false)
            };
            if count == 0 {
                result.pivot_required_cells = required;
                result.pivot_support_count = 0;
                result.pivot_cell = required.trailing_zeros() as u8;
                return DomainCompilation {
                    status: DomainStatus::Empty,
                    propagation: result,
                    cell_piece_masks,
                    #[cfg(any(feature = "local-search-ab", test))]
                    observation,
                };
            }
            if count < result.pivot_support_count {
                result.pivot_required_cells = required;
                result.pivot_support_count = count;
                result.pivot_piece_mask = piece_mask;
                result.pivot_cell = required.trailing_zeros() as u8;
            }
        }

        if let Some((cell, count, piece_mask)) = bumper_domain(
            catalog,
            remaining,
            feasible_piece_mask,
            result.pivot_support_count,
        ) {
            if count == 0 {
                result.pivot_cell = cell;
                result.pivot_required_cells = 1_u64 << cell;
                result.pivot_support_count = 0;
                return DomainCompilation {
                    status: DomainStatus::Empty,
                    propagation: result,
                    cell_piece_masks,
                    #[cfg(any(feature = "local-search-ab", test))]
                    observation,
                };
            }
            result.pivot_cell = cell;
            result.pivot_required_cells = 1_u64 << cell;
            result.pivot_support_count = count;
            result.pivot_piece_mask = piece_mask;
            result.bumper_cell = cell;
        }

        let apdp_candidate = crate::search_prune_policy::apdp_enabled()
            && result.pivot_required_cells.count_ones() == 3
            && partial_shape_kind(catalog.width(), result.pivot_required_cells) != 0;
        #[cfg(any(feature = "local-search-ab", test))]
        {
            observation.apdp_static_check = apdp_candidate;
        }
        if apdp_candidate
            && apdp_domain_is_complete(
                catalog,
                remaining,
                feasible_piece_mask,
                result.pivot_required_cells,
            )
        {
            #[cfg(any(feature = "local-search-ab", test))]
            {
                observation.apdp_static_complete = true;
            }
            let (count, piece_mask) = exact_parent_rows(
                catalog,
                remaining,
                feasible_piece_mask,
                result.pivot_required_cells,
                true,
            );
            if count == 0 {
                result.pivot_support_count = 0;
                return DomainCompilation {
                    status: DomainStatus::Empty,
                    propagation: result,
                    cell_piece_masks,
                    #[cfg(any(feature = "local-search-ab", test))]
                    observation,
                };
            }
            if count < result.pivot_support_count {
                result.pivot_support_count = count;
                result.pivot_piece_mask = piece_mask;
                result.apdp_required_cells = result.pivot_required_cells;
                #[cfg(any(feature = "local-search-ab", test))]
                {
                    observation.apdp_pivot_tightened = true;
                }
            }
        }

        DomainCompilation {
            status: DomainStatus::Supported,
            propagation: result,
            cell_piece_masks,
            #[cfg(any(feature = "local-search-ab", test))]
            observation,
        }
    }

    pub fn compile_minimum(
        catalog: &GeometryCatalog,
        remaining: u64,
        feasible_piece_mask: u8,
    ) -> (DomainStatus, Self) {
        let (status, propagation, _) =
            Self::compile_minimum_support::<false, false>(catalog, remaining, feasible_piece_mask);
        (status, propagation)
    }

    pub fn compile_minimum_cutoff(
        catalog: &GeometryCatalog,
        remaining: u64,
        feasible_piece_mask: u8,
    ) -> (DomainStatus, Self) {
        let (status, propagation, _) =
            Self::compile_minimum_support::<true, false>(catalog, remaining, feasible_piece_mask);
        (status, propagation)
    }

    #[cfg(any(feature = "local-search-ab", test))]
    pub fn compile_minimum_profiled(
        catalog: &GeometryCatalog,
        remaining: u64,
        feasible_piece_mask: u8,
        cutoff: bool,
    ) -> (DomainStatus, Self, MinimumDomainSupportWork) {
        if cutoff {
            Self::compile_minimum_support::<true, true>(catalog, remaining, feasible_piece_mask)
        } else {
            Self::compile_minimum_support::<false, true>(catalog, remaining, feasible_piece_mask)
        }
    }

    fn compile_minimum_support<const CUTOFF: bool, const RECORD_WORK: bool>(
        catalog: &GeometryCatalog,
        remaining: u64,
        feasible_piece_mask: u8,
    ) -> (DomainStatus, Self, MinimumDomainSupportWork) {
        let mut result = Self::empty();
        result.pivot_support_count = usize::MAX;
        let mut work = MinimumDomainSupportWork::default();
        let mut cells = remaining;
        while cells != 0 {
            let cell = cells.trailing_zeros() as u8;
            cells &= cells - 1;
            let mut support_count = 0;
            let mut piece_mask = 0;
            let incidences = catalog.support(cell);
            let mut scanned = incidences.len();
            for (index, &row_id) in incidences.iter().enumerate() {
                if !row_feasible(catalog, row_id, remaining, feasible_piece_mask) {
                    continue;
                }
                support_count += 1;
                piece_mask |= 1_u8 << piece_index(catalog.skeleton(row_id).piece);
                if CUTOFF && support_count == result.pivot_support_count {
                    // Only a strict improvement can replace the earlier
                    // pivot. This cell has at least as many feasible parents
                    // and is not empty, so its tail cannot affect either
                    // decision. A contender below the bound is scanned to
                    // exhaustion and retains its exact kind mask.
                    if RECORD_WORK {
                        scanned = index + 1;
                    }
                    break;
                }
            }
            if RECORD_WORK {
                let skipped = incidences.len() - scanned;
                work.incidence_rows_scanned += scanned as u64;
                work.incidence_rows_skipped += skipped as u64;
                work.cells_cut_off += u64::from(skipped != 0);
            }
            if support_count == 0 {
                result.pivot_cell = cell;
                result.pivot_required_cells = 1_u64 << cell;
                result.pivot_support_count = 0;
                return (DomainStatus::Empty, result, work);
            }
            if support_count < result.pivot_support_count {
                result.pivot_cell = cell;
                result.pivot_required_cells = 1_u64 << cell;
                result.pivot_support_count = support_count;
                result.pivot_piece_mask = piece_mask;
            }
        }
        if let Some((cell, count, piece_mask)) = bumper_domain(
            catalog,
            remaining,
            feasible_piece_mask,
            result.pivot_support_count,
        ) {
            if count == 0 {
                result.pivot_cell = cell;
                result.pivot_required_cells = 1_u64 << cell;
                result.pivot_support_count = 0;
                return (DomainStatus::Empty, result, work);
            }
            result.pivot_cell = cell;
            result.pivot_required_cells = 1_u64 << cell;
            result.pivot_support_count = count;
            result.pivot_piece_mask = piece_mask;
            result.bumper_cell = cell;
        }
        (DomainStatus::Supported, result, work)
    }

    pub fn row_allowed(
        &self,
        catalog: &GeometryCatalog,
        row_id: u32,
        remaining: u64,
        feasible_piece_mask: u8,
    ) -> bool {
        if !row_feasible(catalog, row_id, remaining, feasible_piece_mask) {
            return false;
        }
        let row = catalog.skeleton(row_id);
        if row.cells & self.pivot_required_cells != self.pivot_required_cells {
            return false;
        }
        if self.bumper_cell != u8::MAX
            && !catalog.separator_catalog().bumper_row_compatible(
                remaining,
                self.bumper_cell,
                row.cells,
            )
        {
            return false;
        }
        self.apdp_required_cells == 0
            || apdp_row_supports(catalog, row_id, self.apdp_required_cells)
    }
}

pub(super) fn hall_impossible(
    targets: &[TargetGroup],
    used_counts: [u8; 7],
    remaining: u64,
    cell_piece_masks: &[u8; 64],
) -> bool {
    let mut restricted = false;
    let mut cells = remaining;
    while cells != 0 {
        let cell = cells.trailing_zeros() as usize;
        cells &= cells - 1;
        let allowed = cell_piece_masks[cell];
        if allowed == 0 {
            return true;
        }
        restricted |= allowed != ALL_STANDARD_PIECES;
    }
    if !restricted {
        return false;
    }

    let mut maximum_by_subset = [0_u8; 128];
    let mut active_target_count = 0;
    for target in targets {
        let counts = target.key.counts();
        if !counts_dominate(counts, used_counts) {
            continue;
        }
        active_target_count += 1;
        let remaining_counts: [u8; 7] =
            core::array::from_fn(|index| counts[index] - used_counts[index]);
        let mut sums = [0_u8; 128];
        for subset in 1_u8..128 {
            let lowest = subset.isolate_lowest_one();
            let piece = lowest.trailing_zeros() as usize;
            sums[subset as usize] = sums[(subset ^ lowest) as usize] + remaining_counts[piece];
            maximum_by_subset[subset as usize] =
                maximum_by_subset[subset as usize].max(sums[subset as usize]);
        }
    }
    if active_target_count == 0 {
        return true;
    }

    for subset in 1_u8..128 {
        let mut constrained_cells = 0_u8;
        let mut cells = remaining;
        while cells != 0 {
            let cell = cells.trailing_zeros() as usize;
            cells &= cells - 1;
            let allowed = cell_piece_masks[cell];
            if allowed & !subset == 0 {
                constrained_cells += 1;
            }
        }
        if constrained_cells > maximum_by_subset[subset as usize].saturating_mul(4) {
            return true;
        }
    }
    false
}

pub(super) fn row_feasible(
    catalog: &GeometryCatalog,
    row_id: u32,
    remaining: u64,
    feasible_piece_mask: u8,
) -> bool {
    let row = catalog.skeleton(row_id);
    row.cells & remaining == row.cells
        && feasible_piece_mask & (1_u8 << piece_index(row.piece)) != 0
}

#[cfg(any(feature = "local-search-ab", test))]
fn same_tile_parent_summary(
    required: u64,
    common_owner_cells: &[u64; 64],
    counts: &[u32; 64],
    cell_piece_masks: &[u8; 64],
) -> Option<(usize, u8)> {
    let mut members = required;
    while members != 0 {
        let cell = members.trailing_zeros() as usize;
        members &= members - 1;
        // Every group parent covers this member. Conversely, the member's
        // exact intersection proves every one of its parents covers the
        // whole group. A transitive DSU connection alone is not that proof.
        if counts[cell] != 0 && common_owner_cells[cell] & required == required {
            return Some((counts[cell] as usize, cell_piece_masks[cell]));
        }
    }
    None
}

fn exact_parent_rows(
    catalog: &GeometryCatalog,
    remaining: u64,
    feasible_piece_mask: u8,
    required: u64,
    require_apdp_pair: bool,
) -> (usize, u8) {
    let first = required.trailing_zeros() as u8;
    let mut count = 0;
    let mut piece_mask = 0;
    for row_id in catalog.support(first).iter().copied() {
        let row = catalog.skeleton(row_id);
        if row.cells & required != required
            || !row_feasible(catalog, row_id, remaining, feasible_piece_mask)
            || (require_apdp_pair && !apdp_row_supports(catalog, row_id, required))
        {
            continue;
        }
        count += 1;
        piece_mask |= 1_u8 << piece_index(row.piece);
    }
    (count, piece_mask)
}

fn apdp_row_supports(catalog: &GeometryCatalog, row_id: u32, required: u64) -> bool {
    catalog.apdp_index().row_supports(row_id, required)
}

fn apdp_domain_is_complete(
    catalog: &GeometryCatalog,
    remaining: u64,
    feasible_piece_mask: u8,
    required: u64,
) -> bool {
    let first = required.trailing_zeros() as u8;
    let mut saw_parent = false;
    for row_id in catalog.support(first).iter().copied() {
        let row = catalog.skeleton(row_id);
        if row.cells & required != required
            || !row_feasible(catalog, row_id, remaining, feasible_piece_mask)
        {
            continue;
        }
        saw_parent = true;
        if !catalog.apdp_row_is_static_exact(row_id)
            || !catalog.apdp_index().row_supports(row_id, required)
        {
            return false;
        }
    }
    saw_parent
}

fn bumper_domain(
    catalog: &GeometryCatalog,
    remaining: u64,
    feasible_piece_mask: u8,
    current_support_count: usize,
) -> Option<(u8, usize, u8)> {
    if catalog.initial_board() == 0 || remaining.count_ones() > BUMPER_DOMAIN_MAX_RESIDUAL_CELLS {
        return None;
    }
    let mut best = None;
    for top_cell in catalog.separator_catalog().dynamic_bumper_cells(remaining) {
        let mut count = 0;
        let mut filtered = 0;
        let mut piece_mask = 0;
        for row_id in catalog.support(top_cell).iter().copied() {
            if !row_feasible(catalog, row_id, remaining, feasible_piece_mask) {
                continue;
            }
            let row = catalog.skeleton(row_id);
            if !catalog
                .separator_catalog()
                .bumper_row_compatible(remaining, top_cell, row.cells)
            {
                filtered += 1;
                continue;
            }
            count += 1;
            piece_mask |= 1_u8 << piece_index(row.piece);
        }
        if filtered == 0 || (count >= current_support_count && count > 5) {
            continue;
        }
        if best.is_none_or(|(best_cell, best_count, _)| {
            count < best_count || (count == best_count && top_cell < best_cell)
        }) {
            best = Some((top_cell, count, piece_mask));
        }
    }
    best
}

fn counts_dominate(counts: [u8; 7], used_counts: [u8; 7]) -> bool {
    (0..7).all(|piece| counts[piece] >= used_counts[piece])
}

fn find_root(parents: &mut [u8; 64], cell: u8) -> u8 {
    let mut root = cell;
    while parents[root as usize] != root {
        root = parents[root as usize];
    }
    let mut cursor = cell;
    while parents[cursor as usize] != cursor {
        let next = parents[cursor as usize];
        parents[cursor as usize] = root;
        cursor = next;
    }
    root
}

fn union_cells(parents: &mut [u8; 64], left: u8, right: u8) {
    let mut left_root = find_root(parents, left);
    let mut right_root = find_root(parents, right);
    if left_root == right_root {
        return;
    }
    if right_root < left_root {
        core::mem::swap(&mut left_root, &mut right_root);
    }
    parents[right_root as usize] = left_root;
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{
        PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PieceWindow, SupplyWindowSize,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_rules::profile::builtin_rules::srs_plus;
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;
    use clearra_supply::QueueObservationPolicy;

    // Separated 4x4 chambers have both local parent changes and cells
    // guaranteed unaffected by a choice in the other chamber. The initial
    // board is nonempty, and the 32-cell root is an eight-piece residual.
    const LEFT: u64 = 0x0f | (0x0f << 10) | (0x0f << 20) | (0x0f << 30);
    const RIGHT: u64 = LEFT << 6;
    const REMAINING: u64 = LEFT | RIGHT;
    const LEFT_O: u64 = 0x03 | (0x03 << 10);
    const RIGHT_O: u64 = LEFT_O << 6;

    #[test]
    fn same_tile_parent_reuse_requires_a_single_member_to_prove_the_group() {
        let mut common_owners = [0_u64; 64];
        let mut counts = [0_u32; 64];
        let mut piece_masks = [0_u8; 64];
        let required = 0b111;
        // The two outside cells each require the middle one, but neither
        // requires the opposite outside cell. DSU connects all three.
        common_owners[..3].copy_from_slice(&[0b011, 0b010, 0b110]);
        counts[..3].copy_from_slice(&[4, 6, 2]);
        piece_masks[..3].copy_from_slice(&[0b1110, 0b1111, 0b0100]);
        assert_eq!(
            same_tile_parent_summary(required, &common_owners, &counts, &piece_masks),
            None
        );
        common_owners[2] = required;
        assert_eq!(
            same_tile_parent_summary(required, &common_owners, &counts, &piece_masks),
            Some((2, 0b0100))
        );
    }

    fn catalog() -> GeometryCatalog {
        GeometryCatalog::compile_build_stage(4, 1 << 4, REMAINING).unwrap()
    }

    // Scan the whole catalog, rather than relying on reverse cell incidence
    // or the delta dependency calculation used by the implementation.
    fn exhaustive_cell_support(
        catalog: &GeometryCatalog,
        cell: u8,
        remaining: u64,
        piece_mask: u8,
    ) -> CellSupport {
        let mut support = CellSupport {
            common_owner_cells: remaining,
            ..CellSupport::default()
        };
        for raw_id in 0..catalog.skeleton_count() {
            let row = catalog.skeleton(raw_id as u32);
            let piece_bit = 1_u8 << piece_index(row.piece);
            if row.cells & (1_u64 << cell) != 0
                && row.cells & remaining == row.cells
                && piece_mask & piece_bit != 0
            {
                support.count += 1;
                support.pieces |= piece_bit;
                support.common_owner_cells &= row.cells;
            }
        }
        support
    }

    fn exhaustive_minimum_on_empty_board(
        catalog: &GeometryCatalog,
        remaining: u64,
        piece_mask: u8,
    ) -> (DomainStatus, DomainPropagation) {
        // With an empty origin, no dynamic bumper can replace the pivot.
        assert_eq!(catalog.initial_board(), 0);
        let mut best = DomainPropagation::empty();
        best.pivot_support_count = usize::MAX;
        for cell in 0..64 {
            let bit = 1_u64 << cell;
            if remaining & bit == 0 {
                continue;
            }
            let support = exhaustive_cell_support(catalog, cell, remaining, piece_mask);
            if support.count == 0 {
                best.pivot_cell = cell;
                best.pivot_required_cells = bit;
                best.pivot_support_count = 0;
                return (DomainStatus::Empty, best);
            }
            if (support.count as usize, cell) < (best.pivot_support_count, best.pivot_cell) {
                best.pivot_cell = cell;
                best.pivot_required_cells = bit;
                best.pivot_support_count = support.count as usize;
                best.pivot_piece_mask = support.pieces;
            }
        }
        (DomainStatus::Supported, best)
    }

    #[test]
    fn minimum_domain_cutoff_preserves_winner_and_skips_real_incidence_rows() {
        let full = 0xfffff;
        let catalog = GeometryCatalog::compile_build_stage(2, 0, full).unwrap();
        let o_only = 1 << piece_index(PieceKind::O);
        let (full_status, full_pivot, reference) =
            DomainPropagation::compile_minimum_profiled(&catalog, full, o_only, false);
        let (fast_status, fast_pivot, cutoff) =
            DomainPropagation::compile_minimum_profiled(&catalog, full, o_only, true);

        assert_eq!(
            (fast_status, fast_pivot),
            exhaustive_minimum_on_empty_board(&catalog, full, o_only),
        );
        assert_eq!((fast_status, fast_pivot), (full_status, full_pivot));
        assert_eq!(fast_status, DomainStatus::Supported);
        assert_eq!(fast_pivot.pivot_cell, 0);
        assert_eq!(fast_pivot.pivot_support_count, 1);
        assert_eq!(fast_pivot.pivot_piece_mask, o_only);
        assert_eq!(reference.incidence_rows_skipped, 0);
        assert_eq!(reference.cells_cut_off, 0);
        assert!(cutoff.incidence_rows_skipped > 0);
        assert!(cutoff.cells_cut_off > 0);
        assert_eq!(
            cutoff.incidence_rows_scanned + cutoff.incidence_rows_skipped,
            reference.incidence_rows_scanned
        );
    }

    #[test]
    fn minimum_domain_cutoff_still_detects_late_zero_after_positive_pivots() {
        let catalog = GeometryCatalog::compile_build_stage(2, 0, 0xfffff).unwrap();
        let o_only = 1 << piece_index(PieceKind::O);
        // Cells 0 and 1 have a feasible O. Cell 4 (the next cell)
        // cannot participate in any feasible O, despite four extra cells.
        let remaining = LEFT_O | 1 << 4 | 1 << 6 | 1 << 8 | 1 << 9;
        let (status, pivot, _) =
            DomainPropagation::compile_minimum_profiled(&catalog, remaining, o_only, true);
        assert_eq!(
            (status, pivot),
            exhaustive_minimum_on_empty_board(&catalog, remaining, o_only)
        );
        assert_eq!(status, DomainStatus::Empty);
        assert_eq!(pivot.pivot_cell, 4);
        assert_eq!(pivot.pivot_support_count, 0);
    }

    #[test]
    fn minimum_domain_cutoff_matches_full_selection_for_all_piece_masks() {
        let full = 0xfffff;
        let catalog = GeometryCatalog::compile_build_stage(2, 0, full).unwrap();
        for remaining in [full, full & !LEFT_O, full & !(LEFT_O | RIGHT_O), LEFT_O, 0] {
            for piece_mask in 0..=ALL_STANDARD_PIECES {
                let reference = exhaustive_minimum_on_empty_board(&catalog, remaining, piece_mask);
                let (status, pivot, _) = DomainPropagation::compile_minimum_profiled(
                    &catalog, remaining, piece_mask, true,
                );
                assert_eq!(
                    (status, pivot),
                    reference,
                    "remaining {remaining:#x}, pieces {piece_mask:#x}"
                );
                assert_eq!(
                    DomainPropagation::compile_minimum_cutoff(&catalog, remaining, piece_mask),
                    reference
                );
                assert_eq!(
                    DomainPropagation::compile_minimum(&catalog, remaining, piece_mask),
                    reference
                );
            }
        }
    }

    #[test]
    fn minimum_domain_cutoff_matches_full6l_factorized_target_prefixes() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(6, 0),
            PcQueueInput::pattern_expression(QueuePatternExpression::parse("P1P7P7", 0).unwrap()),
            PieceWindow::new(15),
        )
        .with_rule(srs_plus())
        .with_allow_hold(true)
        .with_exact_pieces(Some(15))
        .with_supply_window_size(SupplyWindowSize::new(15))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_max_patterns(0));
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let catalog = GeometryCatalog::compile(&problem).unwrap();
        assert_eq!(catalog.height(), 6);
        assert_eq!(catalog.initial_board(), 0);
        assert_eq!(catalog.required_cells(), (1_u64 << 60) - 1);
        assert!(
            (0..catalog.skeleton_count() as u32).any(|row| {
                catalog
                    .realizations(row)
                    .iter()
                    .any(|realization| realization.required_deleted_rows != 0)
            }),
            "use the actual PC inverse-line-clear catalog, not a build-only catalog"
        );

        // Read the seven target inventories from the finite source's actual
        // factorization without enumerating its 177,811,200 queue ordinals.
        let layout = problem
            .piece_source()
            .materialized_universe()
            .unwrap()
            .complete_factorized_layout()
            .unwrap();
        assert_eq!(layout.pattern_count(), 177_811_200);
        assert_eq!(layout.full_sequence_len(), 15);
        assert_eq!(layout.visible_sequence_len(), 15);
        let segments: Vec<_> = layout.segments().collect();
        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.draw_count())
                .collect::<Vec<_>>(),
            [1, 7, 7]
        );
        let mut base = [0_u8; 7];
        for segment in &segments[1..] {
            assert_eq!(segment.draw_count(), segment.choices().len());
            for &piece in segment.choices() {
                base[piece_index(piece)] += 1;
            }
        }
        assert_eq!(base, [2; 7]);
        let targets: Vec<_> = segments[0]
            .choices()
            .iter()
            .map(|&piece| {
                let mut target = base;
                target[piece_index(piece)] += 1;
                target
            })
            .collect();
        assert_eq!(targets.len(), 7);
        assert!(targets
            .iter()
            .all(|counts| counts.iter().copied().sum::<u8>() == 15));

        let feasible_mask = |targets: &[[u8; 7]], used: [u8; 7]| {
            let mut mask = 0;
            for &target in targets {
                if counts_dominate(target, used) {
                    for piece in 0..7 {
                        if used[piece] < target[piece] {
                            mask |= 1 << piece;
                        }
                    }
                }
            }
            mask
        };
        let mut states = 0;
        let mut residual_states = 0;
        let mut total_work = MinimumDomainSupportWork::default();
        let mut check = |remaining: u64, pieces: u8| {
            let (full_status, full_pivot, reference) =
                DomainPropagation::compile_minimum_profiled(&catalog, remaining, pieces, false);
            let (fast_status, fast_pivot, cutoff) =
                DomainPropagation::compile_minimum_profiled(&catalog, remaining, pieces, true);
            assert_eq!(
                (fast_status, fast_pivot),
                (full_status, full_pivot),
                "remaining {remaining:#x}, pieces {pieces:#x}"
            );
            assert_eq!(
                DomainPropagation::compile_minimum_cutoff(&catalog, remaining, pieces),
                (full_status, full_pivot)
            );
            assert_eq!(reference.incidence_rows_skipped, 0);
            assert_eq!(reference.cells_cut_off, 0);
            assert_eq!(
                cutoff.incidence_rows_scanned + cutoff.incidence_rows_skipped,
                reference.incidence_rows_scanned
            );
            states += 1;
            residual_states += usize::from(remaining != catalog.required_cells());
            total_work.incidence_rows_scanned += cutoff.incidence_rows_scanned;
            total_work.incidence_rows_skipped += cutoff.incidence_rows_skipped;
            total_work.cells_cut_off += cutoff.cells_cut_off;
            (full_status, full_pivot)
        };

        // Check both the shared-target search and every single-target shard.
        // Three deterministic row choices produce actual disjoint prefixes;
        // a dead prefix is also checked before the walk stops.
        for scope in 0..=targets.len() {
            let active_targets = if scope == targets.len() {
                targets.as_slice()
            } else {
                &targets[scope..scope + 1]
            };
            for choice in 0..3 {
                let mut remaining = catalog.required_cells();
                let mut used = [0_u8; 7];
                for _ in 0..=15 {
                    let pieces = feasible_mask(active_targets, used);
                    let (status, domain) = check(remaining, pieces);
                    if remaining == 0 || status == DomainStatus::Empty {
                        break;
                    }
                    let rows: Vec<_> = catalog
                        .support(domain.pivot_cell)
                        .iter()
                        .copied()
                        .filter(|&row| domain.row_allowed(&catalog, row, remaining, pieces))
                        .collect();
                    assert_eq!(rows.len(), domain.pivot_support_count);
                    let row = rows[(rows.len() - 1) * choice / 2];
                    let row = catalog.skeleton(row);
                    remaining &= !row.cells;
                    used[piece_index(row.piece)] += 1;
                }
            }
        }
        // Near EOF, each exact target also admits the one-kind residual
        // obtained by spending all but one copy of a piece. These inputs
        // exercise exhausted inventories even if a greedy prefix dies early.
        for target in &targets {
            for piece in 0..7 {
                let mut used = *target;
                used[piece] -= 1;
                let pieces = feasible_mask(core::slice::from_ref(target), used);
                assert_eq!(pieces, 1 << piece);
                let row = (0..catalog.skeleton_count() as u32)
                    .find(|&row| piece_index(catalog.skeleton(row).piece) == piece)
                    .unwrap();
                check(catalog.skeleton(row).cells, pieces);
            }
        }
        check(0, 0);
        assert!(states > 7 * 3);
        assert!(residual_states > 7 * 3);
        assert!(total_work.incidence_rows_scanned > 0);
        assert!(total_work.incidence_rows_skipped > 0);
        assert!(total_work.cells_cut_off > 0);
        eprintln!(
            "Full6L minimum-domain parity: {states} states, {residual_states} residual states, \
             full {} incidences, cutoff {} scanned, {} skipped, {} cells cut off",
            total_work.incidence_rows_scanned + total_work.incidence_rows_skipped,
            total_work.incidence_rows_scanned,
            total_work.incidence_rows_skipped,
            total_work.cells_cut_off,
        );
    }

    fn assert_matches_full(
        cache: &mut DomainSupportCache,
        catalog: &GeometryCatalog,
        depth: u8,
        remaining: u64,
        piece_mask: u8,
    ) -> DomainCompilation {
        let actual = cache.compile(
            catalog,
            depth,
            remaining,
            piece_mask,
            SameTileParentMode::Scan,
        );
        let expected = DomainPropagation::compile_with_support(
            catalog,
            remaining,
            piece_mask,
            SameTileParentMode::Scan,
            |cell| exhaustive_cell_support(catalog, cell, remaining, piece_mask),
        );
        assert_eq!(
            actual, expected,
            "remaining {remaining:#x}, pieces {piece_mask:#x}"
        );
        if actual.status == DomainStatus::Supported {
            let mut active = remaining;
            let snapshot = cache.ancestors.last().unwrap();
            while active != 0 {
                let cell = active.trailing_zeros() as u8;
                active &= active - 1;
                assert_eq!(
                    snapshot.cells[cell as usize],
                    exhaustive_cell_support(catalog, cell, remaining, piece_mask)
                );
            }
            let legal_parent_ids = |domain: DomainPropagation| -> Vec<_> {
                (0..catalog.skeleton_count())
                    .map(|raw_id| raw_id as u32)
                    .filter(|&row_id| domain.row_allowed(catalog, row_id, remaining, piece_mask))
                    .collect()
            };
            assert_eq!(
                legal_parent_ids(actual.propagation),
                legal_parent_ids(expected.propagation)
            );
        }
        actual
    }

    #[test]
    fn incremental_domain_updates_only_impacted_chamber_and_undoes_siblings() {
        let catalog = catalog();
        let mut cache = DomainSupportCache::default();
        assert_eq!(cache.retained_bytes(), 0);
        let root = assert_matches_full(&mut cache, &catalog, 0, REMAINING, ALL_STANDARD_PIECES);
        assert_eq!(root.status, DomainStatus::Supported);
        let dirty =
            cache.ancestors[0].dirty_cells(&catalog, REMAINING & !LEFT_O, ALL_STANDARD_PIECES);
        assert_ne!(dirty, 0);
        assert_eq!(dirty & RIGHT, 0, "right chamber support must be reused");
        let left = assert_matches_full(
            &mut cache,
            &catalog,
            1,
            REMAINING & !LEFT_O,
            ALL_STANDARD_PIECES,
        );
        assert_eq!(left.status, DomainStatus::Supported);
        assert_matches_full(
            &mut cache,
            &catalog,
            2,
            REMAINING & !(LEFT_O | RIGHT_O),
            ALL_STANDARD_PIECES,
        );
        let right = assert_matches_full(
            &mut cache,
            &catalog,
            1,
            REMAINING & !RIGHT_O,
            ALL_STANDARD_PIECES,
        );
        assert_eq!(right.status, DomainStatus::Supported);
        assert_eq!(cache.ancestors.len(), 2);
        assert_eq!(
            assert_matches_full(&mut cache, &catalog, 0, REMAINING, ALL_STANDARD_PIECES),
            root
        );
    }

    #[test]
    fn incremental_domain_invalidates_removed_and_restored_piece_kinds() {
        let catalog = catalog();
        let mut cache = DomainSupportCache::default();
        let o_kind = 1 << piece_index(PieceKind::O);
        let full = assert_matches_full(&mut cache, &catalog, 0, REMAINING, ALL_STANDARD_PIECES);
        assert_eq!(full.status, DomainStatus::Supported);
        let reduced = assert_matches_full(
            &mut cache,
            &catalog,
            1,
            REMAINING & !LEFT_O,
            ALL_STANDARD_PIECES & !o_kind,
        );
        assert_eq!(reduced.status, DomainStatus::Supported);
        assert_eq!(full.cell_piece_masks[9] & o_kind, o_kind);
        assert_eq!(reduced.cell_piece_masks[9] & o_kind, 0);
        let restored = assert_matches_full(
            &mut cache,
            &catalog,
            2,
            REMAINING & !LEFT_O,
            ALL_STANDARD_PIECES,
        );
        assert_eq!(restored.cell_piece_masks[9] & o_kind, o_kind);
        let unsupported = assert_matches_full(&mut cache, &catalog, 2, REMAINING & !LEFT_O, 0);
        assert_eq!(unsupported.status, DomainStatus::Empty);
        assert_eq!(cache.ancestors.len(), 2);
        assert_matches_full(
            &mut cache,
            &catalog,
            1,
            REMAINING & !RIGHT_O,
            ALL_STANDARD_PIECES,
        );
    }

    #[test]
    fn incremental_domain_matches_full_over_pivot_generated_branches() {
        fn visit(
            cache: &mut DomainSupportCache,
            catalog: &GeometryCatalog,
            depth: u8,
            remaining: u64,
            piece_mask: u8,
        ) -> usize {
            let compilation = assert_matches_full(cache, catalog, depth, remaining, piece_mask);
            if depth == 3 || compilation.status != DomainStatus::Supported {
                return 1;
            }
            let mut visited = 1;
            for &row_id in catalog
                .support(compilation.propagation.pivot_cell)
                .iter()
                .filter(|&&row_id| {
                    compilation
                        .propagation
                        .row_allowed(catalog, row_id, remaining, piece_mask)
                })
                .take(4)
            {
                let row = catalog.skeleton(row_id);
                // Alternate unchanged supply with a last-instance removal;
                // a later sibling must see the original supply restored.
                let child_piece_mask = if depth.is_multiple_of(2) {
                    piece_mask
                } else {
                    piece_mask & !(1 << piece_index(row.piece))
                };
                visited += visit(
                    cache,
                    catalog,
                    depth + 1,
                    remaining ^ row.cells,
                    child_piece_mask,
                );
            }
            visited
        }

        let catalog = catalog();
        let visited = visit(
            &mut DomainSupportCache::default(),
            &catalog,
            0,
            REMAINING,
            ALL_STANDARD_PIECES,
        );
        assert!(
            visited > 4,
            "the differential must visit descendants and siblings"
        );
    }
}
