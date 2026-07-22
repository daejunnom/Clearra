use super::{
    catalog::GeometryCatalog, geometry::TargetGroup, geometry_apdp::partial_shape_kind, piece_index,
};

const BUMPER_DOMAIN_MAX_RESIDUAL_CELLS: u32 = 24;

const ALL_STANDARD_PIECES: u8 = 0x7f;

#[derive(Clone, Copy, Debug)]
pub(super) struct DomainPropagation {
    pub pivot_required_cells: u64,
    pub pivot_piece_mask: u8,
    pub pivot_support_count: usize,
    pub pivot_cell: u8,
    bumper_cell: u8,
    apdp_required_cells: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DomainCompilation {
    pub status: DomainStatus,
    pub propagation: DomainPropagation,
    pub cell_piece_masks: [u8; 64],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DomainStatus {
    Supported,
    Empty,
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

        let mut cells = remaining;
        while cells != 0 {
            let cell = cells.trailing_zeros() as u8;
            cells &= cells - 1;
            let mut support_count = 0;
            let mut piece_mask = 0_u8;
            let mut common = remaining;
            for row_id in catalog.support(cell).iter().copied() {
                if !row_feasible(catalog, row_id, remaining, feasible_piece_mask) {
                    continue;
                }
                let row = catalog.skeleton(row_id);
                support_count += 1;
                piece_mask |= 1_u8 << piece_index(row.piece);
                common &= row.cells;
            }
            cell_piece_masks[cell as usize] = piece_mask;
            common_owner_cells[cell as usize] = common;
            if support_count == 0 {
                result.pivot_cell = cell;
                result.pivot_required_cells = 1_u64 << cell;
                result.pivot_support_count = 0;
                return DomainCompilation {
                    status: DomainStatus::Empty,
                    propagation: result,
                    cell_piece_masks,
                };
            }
            if support_count < result.pivot_support_count {
                result.pivot_cell = cell;
                result.pivot_required_cells = 1_u64 << cell;
                result.pivot_support_count = support_count;
                result.pivot_piece_mask = piece_mask;
            }
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
                };
            }
            let (count, piece_mask) =
                exact_parent_rows(catalog, remaining, feasible_piece_mask, required, false);
            if count == 0 {
                result.pivot_required_cells = required;
                result.pivot_support_count = 0;
                result.pivot_cell = required.trailing_zeros() as u8;
                return DomainCompilation {
                    status: DomainStatus::Empty,
                    propagation: result,
                    cell_piece_masks,
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
                };
            }
            result.pivot_cell = cell;
            result.pivot_required_cells = 1_u64 << cell;
            result.pivot_support_count = count;
            result.pivot_piece_mask = piece_mask;
            result.bumper_cell = cell;
        }

        if result.pivot_required_cells.count_ones() == 3
            && partial_shape_kind(catalog.width(), result.pivot_required_cells) != 0
            && apdp_domain_is_complete(
                catalog,
                remaining,
                feasible_piece_mask,
                result.pivot_required_cells,
            )
        {
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
                };
            }
            if count < result.pivot_support_count {
                result.pivot_support_count = count;
                result.pivot_piece_mask = piece_mask;
                result.apdp_required_cells = result.pivot_required_cells;
            }
        }

        DomainCompilation {
            status: DomainStatus::Supported,
            propagation: result,
            cell_piece_masks,
        }
    }

    pub fn compile_minimum(
        catalog: &GeometryCatalog,
        remaining: u64,
        feasible_piece_mask: u8,
    ) -> (DomainStatus, Self) {
        let mut result = Self::empty();
        result.pivot_support_count = usize::MAX;
        let mut cells = remaining;
        while cells != 0 {
            let cell = cells.trailing_zeros() as u8;
            cells &= cells - 1;
            let mut support_count = 0;
            let mut piece_mask = 0;
            for row_id in catalog.support(cell).iter().copied() {
                if !row_feasible(catalog, row_id, remaining, feasible_piece_mask) {
                    continue;
                }
                support_count += 1;
                piece_mask |= 1_u8 << piece_index(catalog.skeleton(row_id).piece);
            }
            if support_count == 0 {
                result.pivot_cell = cell;
                result.pivot_required_cells = 1_u64 << cell;
                result.pivot_support_count = 0;
                return (DomainStatus::Empty, result);
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
                return (DomainStatus::Empty, result);
            }
            result.pivot_cell = cell;
            result.pivot_required_cells = 1_u64 << cell;
            result.pivot_support_count = count;
            result.pivot_piece_mask = piece_mask;
            result.bumper_cell = cell;
        }
        (DomainStatus::Supported, result)
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
            let lowest = subset & subset.wrapping_neg();
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
