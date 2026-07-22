use std::collections::BTreeMap;

use super::{
    extended_board::ExtendedBoard,
    extended_inverse_catalog::{ExtendedInverseCatalog, ExtendedSkeletonRow},
    geometry_apdp::{APDP_ARM, APDP_ELBOW},
    geometry_projection::ProjectionReachabilityCache,
    mix_digest, piece_index,
};

const NO_CELL: u16 = u16::MAX;
const BUMPER_MAX_RESIDUAL_CELLS: u32 = 24;

#[derive(Clone, Copy)]
struct ExtendedParentRange {
    partial_cells: ExtendedBoard,
    parent_start: u32,
    parent_count: u16,
}

#[derive(Clone)]
pub(super) struct ExtendedArmPairIndex {
    ranges: Vec<ExtendedParentRange>,
    parent_rows: Vec<u32>,
    row_support_flags: Vec<u8>,
    identity_digest: u64,
}

impl ExtendedArmPairIndex {
    pub fn compile(width: u8, rows: &[ExtendedSkeletonRow]) -> Option<Self> {
        let mut parents_by_partial = BTreeMap::<ExtendedBoard, Vec<u32>>::new();
        let mut row_support_flags = Vec::new();
        row_support_flags.try_reserve_exact(rows.len()).ok()?;
        for (row_id, row) in rows.iter().enumerate() {
            let mut partials = [ExtendedBoard::EMPTY; 4];
            let mut kinds = [0_u8; 4];
            let mut partial_count = 0usize;
            for cell in row.cells.cells() {
                let partial = row.cells.without(single_cell(cell));
                let kind = partial_shape_kind(width, partial);
                if kind != 0 {
                    partials[partial_count] = partial;
                    kinds[partial_count] = kind;
                    partial_count += 1;
                }
            }
            let mut flags = 0_u8;
            for left in 0..partial_count {
                for right in left + 1..partial_count {
                    if partials[left].union(partials[right]) != row.cells {
                        continue;
                    }
                    flags |= match (kinds[left], kinds[right]) {
                        (APDP_ARM, APDP_ARM) => 1,
                        (APDP_ELBOW, APDP_ELBOW) => 4,
                        _ => 2,
                    };
                    let row_id = u32::try_from(row_id).ok()?;
                    parents_by_partial
                        .entry(partials[left])
                        .or_default()
                        .push(row_id);
                    parents_by_partial
                        .entry(partials[right])
                        .or_default()
                        .push(row_id);
                }
            }
            row_support_flags.push(flags);
        }
        let mut ranges = Vec::new();
        let mut parent_rows = Vec::new();
        ranges.try_reserve_exact(parents_by_partial.len()).ok()?;
        for (partial_cells, mut parents) in parents_by_partial {
            parents.sort_unstable();
            parents.dedup();
            let parent_start = u32::try_from(parent_rows.len()).ok()?;
            let parent_count = u16::try_from(parents.len()).ok()?;
            parent_rows.try_reserve(parents.len()).ok()?;
            parent_rows.extend_from_slice(&parents);
            ranges.push(ExtendedParentRange {
                partial_cells,
                parent_start,
                parent_count,
            });
        }
        let mut identity_digest = mix_digest(0, u64::from(width));
        for range in &ranges {
            for word in range.partial_cells.words() {
                identity_digest = mix_digest(identity_digest, word);
            }
            identity_digest = mix_digest(identity_digest, u64::from(range.parent_start));
            identity_digest = mix_digest(identity_digest, u64::from(range.parent_count));
        }
        for row in &parent_rows {
            identity_digest = mix_digest(identity_digest, u64::from(*row));
        }
        Some(Self {
            ranges,
            parent_rows,
            row_support_flags,
            identity_digest,
        })
    }

    pub fn row_supports(&self, row_id: u32, partial_cells: ExtendedBoard) -> bool {
        let Ok(index) = self
            .ranges
            .binary_search_by_key(&partial_cells, |range| range.partial_cells)
        else {
            return false;
        };
        let range = self.ranges[index];
        let start = range.parent_start as usize;
        let end = start + usize::from(range.parent_count);
        self.parent_rows[start..end].binary_search(&row_id).is_ok()
    }

    pub fn row_support_flags(&self, row_id: u32) -> u8 {
        self.row_support_flags[row_id as usize]
    }

    pub const fn identity_digest(&self) -> u64 {
        self.identity_digest
    }

    pub fn retained_bytes(&self) -> usize {
        self.ranges.capacity() * core::mem::size_of::<ExtendedParentRange>()
            + self.parent_rows.capacity() * core::mem::size_of::<u32>()
            + self.row_support_flags.capacity() * core::mem::size_of::<u8>()
    }
}

#[derive(Clone, Copy)]
pub(super) struct ExtendedDomainPropagation {
    pub pivot_required_cells: ExtendedBoard,
    pub pivot_piece_mask: u8,
    pub pivot_support_count: usize,
    pub pivot_cell: u16,
    bumper_cell: u16,
    apdp_required_cells: ExtendedBoard,
}

pub(super) enum ExtendedDomainResult {
    Supported(ExtendedDomainPropagation),
    Empty,
    HallImpossible,
    ProjectionImpossible,
}

pub(super) struct ExtendedDomainWorkspace {
    projection_cache: ProjectionReachabilityCache,
    parents: [u16; 256],
    group_sizes: [u16; 256],
}

impl ExtendedDomainWorkspace {
    pub fn new() -> Self {
        Self {
            projection_cache: ProjectionReachabilityCache::default(),
            parents: core::array::from_fn(|index| index as u16),
            group_sizes: [0; 256],
        }
    }

    pub fn compile(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        remaining: ExtendedBoard,
        used_counts: [u8; 7],
        targets: &[[u8; 7]],
        depth: usize,
    ) -> ExtendedDomainResult {
        let feasible_piece_mask = feasible_piece_mask(targets, used_counts);
        if feasible_piece_mask == 0 {
            return ExtendedDomainResult::Empty;
        }

        self.reset_union_find();
        let mut propagation = ExtendedDomainPropagation {
            pivot_required_cells: ExtendedBoard::EMPTY,
            pivot_piece_mask: 0,
            pivot_support_count: usize::MAX,
            pivot_cell: NO_CELL,
            bumper_cell: NO_CELL,
            apdp_required_cells: ExtendedBoard::EMPTY,
        };
        let mut cell_piece_masks = [0_u8; 256];

        for cell in remaining.cells() {
            let mut support_count = 0usize;
            let mut piece_mask = 0_u8;
            let mut common_owner = remaining;
            for &row_id in catalog.support(cell) {
                let row = catalog.skeleton(row_id);
                let piece = piece_index(row.piece);
                if feasible_piece_mask & (1_u8 << piece) == 0 || !row.cells.is_subset_of(remaining)
                {
                    continue;
                }
                support_count += 1;
                piece_mask |= 1_u8 << piece;
                common_owner = common_owner.intersection(row.cells);
            }
            if support_count == 0 {
                return ExtendedDomainResult::Empty;
            }
            cell_piece_masks[usize::from(cell)] = piece_mask;
            for owner in common_owner.cells() {
                self.union(cell, owner);
            }
            if support_count < propagation.pivot_support_count {
                propagation.pivot_required_cells = single_cell(cell);
                propagation.pivot_piece_mask = piece_mask;
                propagation.pivot_support_count = support_count;
                propagation.pivot_cell = cell;
            }
        }

        self.group_sizes.fill(0);
        for cell in remaining.cells() {
            let root = self.find(cell);
            self.group_sizes[usize::from(root)] += 1;
        }
        for root in 0..self.group_sizes.len() {
            let size = self.group_sizes[root];
            if size <= 1 {
                continue;
            }
            if size > 4 {
                return ExtendedDomainResult::Empty;
            }
            let mut required = ExtendedBoard::EMPTY;
            for cell in remaining.cells() {
                if usize::from(self.find(cell)) == root {
                    required.insert(cell);
                }
            }
            let (support_count, piece_mask) =
                exact_parent_rows(catalog, remaining, feasible_piece_mask, required);
            if support_count == 0 {
                return ExtendedDomainResult::Empty;
            }
            if support_count < propagation.pivot_support_count {
                propagation.pivot_required_cells = required;
                propagation.pivot_piece_mask = piece_mask;
                propagation.pivot_support_count = support_count;
                propagation.pivot_cell = required.cells().next().unwrap_or(NO_CELL);
            }
        }

        if let Some((cell, support_count, piece_mask)) = bumper_domain(
            catalog,
            remaining,
            feasible_piece_mask,
            propagation.pivot_support_count,
        ) {
            if support_count == 0 {
                return ExtendedDomainResult::Empty;
            }
            propagation.pivot_required_cells = single_cell(cell);
            propagation.pivot_piece_mask = piece_mask;
            propagation.pivot_support_count = support_count;
            propagation.pivot_cell = cell;
            propagation.bumper_cell = cell;
        }

        if propagation.pivot_required_cells.count_ones() == 3
            && partial_shape_kind(catalog.width(), propagation.pivot_required_cells) != 0
            && apdp_domain_is_complete(
                catalog,
                remaining,
                feasible_piece_mask,
                propagation.pivot_required_cells,
            )
        {
            propagation.apdp_required_cells = propagation.pivot_required_cells;
        }

        let target_depth = catalog.required_cells().count_ones() as usize / 4;
        let advanced = target_depth >= 7 && !catalog.initial_board().is_empty();
        let adaptive = depth <= 2 || propagation.pivot_support_count >= 5;
        if advanced
            && adaptive
            && hall_impossible(remaining, used_counts, targets, &cell_piece_masks)
        {
            return ExtendedDomainResult::HallImpossible;
        }
        if advanced
            && remaining.count_ones() >= 24
            && adaptive
            && self.projection_cache.extended_residual_impossible(
                catalog.projection_catalog(),
                targets,
                used_counts,
                remaining,
                true,
            )
        {
            return ExtendedDomainResult::ProjectionImpossible;
        }
        ExtendedDomainResult::Supported(propagation)
    }

    pub fn retained_bytes(&self) -> usize {
        self.projection_cache.retained_bytes()
    }

    fn reset_union_find(&mut self) {
        for (index, parent) in self.parents.iter_mut().enumerate() {
            *parent = index as u16;
        }
    }

    fn find(&mut self, cell: u16) -> u16 {
        find_root(&mut self.parents, cell)
    }

    fn union(&mut self, left: u16, right: u16) {
        union_cells(&mut self.parents, left, right);
    }
}

impl ExtendedDomainPropagation {
    pub const fn empty() -> Self {
        Self {
            pivot_required_cells: ExtendedBoard::EMPTY,
            pivot_piece_mask: 0,
            pivot_support_count: 0,
            pivot_cell: NO_CELL,
            bumper_cell: NO_CELL,
            apdp_required_cells: ExtendedBoard::EMPTY,
        }
    }

    pub fn row_allowed(
        self,
        catalog: &ExtendedInverseCatalog,
        row_id: u32,
        remaining: ExtendedBoard,
        feasible_piece_mask: u8,
    ) -> bool {
        let row = catalog.skeleton(row_id);
        if feasible_piece_mask & (1_u8 << piece_index(row.piece)) == 0
            || !row.cells.is_subset_of(remaining)
            || !self.pivot_required_cells.is_subset_of(row.cells)
        {
            return false;
        }
        (self.bumper_cell == NO_CELL
            || bumper_row_compatible(catalog, remaining, self.bumper_cell, row.cells))
            && (self.apdp_required_cells.is_empty()
                || catalog
                    .apdp_index()
                    .row_supports(row_id, self.apdp_required_cells))
    }
}

pub(super) fn feasible_piece_mask(targets: &[[u8; 7]], used_counts: [u8; 7]) -> u8 {
    let mut mask = 0_u8;
    for piece in 0..7 {
        let mut next = used_counts;
        next[piece] = next[piece].saturating_add(1);
        if targets.iter().any(|target| counts_dominate(*target, next)) {
            mask |= 1_u8 << piece;
        }
    }
    mask
}

fn exact_parent_rows(
    catalog: &ExtendedInverseCatalog,
    remaining: ExtendedBoard,
    feasible_piece_mask: u8,
    required: ExtendedBoard,
) -> (usize, u8) {
    let Some(first) = required.cells().next() else {
        return (0, 0);
    };
    let mut count = 0usize;
    let mut piece_mask = 0_u8;
    for &row_id in catalog.support(first) {
        let row = catalog.skeleton(row_id);
        let piece = piece_index(row.piece);
        if feasible_piece_mask & (1_u8 << piece) == 0
            || !row.cells.is_subset_of(remaining)
            || !required.is_subset_of(row.cells)
        {
            continue;
        }
        count += 1;
        piece_mask |= 1_u8 << piece;
    }
    (count, piece_mask)
}

fn apdp_domain_is_complete(
    catalog: &ExtendedInverseCatalog,
    remaining: ExtendedBoard,
    feasible_piece_mask: u8,
    required: ExtendedBoard,
) -> bool {
    let Some(first) = required.cells().next() else {
        return false;
    };
    let mut saw_parent = false;
    for &row_id in catalog.support(first) {
        let row = catalog.skeleton(row_id);
        if feasible_piece_mask & (1_u8 << piece_index(row.piece)) == 0
            || !row.cells.is_subset_of(remaining)
            || !required.is_subset_of(row.cells)
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

fn hall_impossible(
    remaining: ExtendedBoard,
    used_counts: [u8; 7],
    targets: &[[u8; 7]],
    cell_piece_masks: &[u8; 256],
) -> bool {
    let mut maximum_by_subset = [0_u8; 128];
    let mut active_targets = 0usize;
    for target in targets.iter().copied() {
        if !counts_dominate(target, used_counts) {
            continue;
        }
        active_targets += 1;
        let residual: [u8; 7] = core::array::from_fn(|piece| target[piece] - used_counts[piece]);
        let mut sums = [0_u8; 128];
        for subset in 1_u8..128 {
            let lowest = subset & subset.wrapping_neg();
            let piece = lowest.trailing_zeros() as usize;
            sums[subset as usize] = sums[(subset ^ lowest) as usize] + residual[piece];
            maximum_by_subset[subset as usize] =
                maximum_by_subset[subset as usize].max(sums[subset as usize]);
        }
    }
    if active_targets == 0 {
        return true;
    }
    for subset in 1_u8..128 {
        let constrained = remaining
            .cells()
            .filter(|cell| {
                let allowed = cell_piece_masks[usize::from(*cell)];
                allowed != 0 && allowed & !subset == 0
            })
            .count();
        if constrained > usize::from(maximum_by_subset[subset as usize]) * 4 {
            return true;
        }
    }
    false
}

fn bumper_domain(
    catalog: &ExtendedInverseCatalog,
    remaining: ExtendedBoard,
    feasible_piece_mask: u8,
    current_support_count: usize,
) -> Option<(u16, usize, u8)> {
    if remaining.count_ones() > BUMPER_MAX_RESIDUAL_CELLS {
        return None;
    }
    let occupied = catalog
        .initial_board()
        .union(catalog.required_cells().without(remaining));
    let mut best = None;
    for column in 0..catalog.width() {
        let top = u16::from(catalog.height() - 1) * u16::from(catalog.width()) + u16::from(column);
        if !remaining.contains(top) || occupied.contains(top) {
            continue;
        }
        if (0..catalog.height() - 1).any(|row| {
            !occupied.contains(u16::from(row) * u16::from(catalog.width()) + u16::from(column))
        }) {
            continue;
        }
        let mut support_count = 0usize;
        let mut filtered = 0usize;
        let mut piece_mask = 0_u8;
        for &row_id in catalog.support(top) {
            let row = catalog.skeleton(row_id);
            let piece = piece_index(row.piece);
            if feasible_piece_mask & (1_u8 << piece) == 0 || !row.cells.is_subset_of(remaining) {
                continue;
            }
            if !bumper_row_compatible(catalog, remaining, top, row.cells) {
                filtered += 1;
                continue;
            }
            support_count += 1;
            piece_mask |= 1_u8 << piece;
        }
        if filtered == 0 || (support_count >= current_support_count && support_count > 5) {
            continue;
        }
        if best.is_none_or(|(best_cell, best_count, _)| {
            support_count < best_count || (support_count == best_count && top < best_cell)
        }) {
            best = Some((top, support_count, piece_mask));
        }
    }
    best
}

fn bumper_row_compatible(
    catalog: &ExtendedInverseCatalog,
    remaining: ExtendedBoard,
    bumper_cell: u16,
    row: ExtendedBoard,
) -> bool {
    if !row.contains(bumper_cell) {
        return false;
    }
    let column = (bumper_cell % u16::from(catalog.width())) as u8;
    let mut left_demand = 0_u32;
    let mut right_demand = 0_u32;
    let mut left_supply = 0_u32;
    let mut right_supply = 0_u32;
    for cell in remaining.cells() {
        let x = (cell % u16::from(catalog.width())) as u8;
        if x < column {
            left_demand += 1;
        } else if x > column {
            right_demand += 1;
        }
    }
    for cell in row.cells() {
        let x = (cell % u16::from(catalog.width())) as u8;
        if x < column {
            left_supply += 1;
        } else if x > column {
            right_supply += 1;
        } else if cell != bumper_cell {
            return false;
        }
    }
    left_supply <= left_demand
        && right_supply <= right_demand
        && (left_demand - left_supply).is_multiple_of(4)
        && (right_demand - right_supply).is_multiple_of(4)
}

fn single_cell(cell: u16) -> ExtendedBoard {
    let mut board = ExtendedBoard::EMPTY;
    board.insert(cell);
    board
}

fn partial_shape_kind(width: u8, cells: ExtendedBoard) -> u8 {
    if width == 0 || cells.count_ones() != 3 {
        return 0;
    }
    let mut min_x = u8::MAX;
    let mut max_x = 0_u8;
    let mut min_y = u8::MAX;
    let mut max_y = 0_u8;
    for cell in cells.cells() {
        let x = (cell % u16::from(width)) as u8;
        let y = (cell / u16::from(width)) as u8;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    if (min_y == max_y && max_x - min_x == 2) || (min_x == max_x && max_y - min_y == 2) {
        APDP_ARM
    } else if max_x - min_x == 1 && max_y - min_y == 1 {
        APDP_ELBOW
    } else {
        0
    }
}

fn counts_dominate(counts: [u8; 7], used: [u8; 7]) -> bool {
    (0..7).all(|piece| counts[piece] >= used[piece])
}

fn find_root(parents: &mut [u16; 256], cell: u16) -> u16 {
    let mut root = cell;
    while parents[usize::from(root)] != root {
        root = parents[usize::from(root)];
    }
    let mut cursor = cell;
    while parents[usize::from(cursor)] != cursor {
        let next = parents[usize::from(cursor)];
        parents[usize::from(cursor)] = root;
        cursor = next;
    }
    root
}

fn union_cells(parents: &mut [u16; 256], left: u16, right: u16) {
    let mut left_root = find_root(parents, left);
    let mut right_root = find_root(parents, right);
    if left_root == right_root {
        return;
    }
    if right_root < left_root {
        core::mem::swap(&mut left_root, &mut right_root);
    }
    parents[usize::from(right_root)] = left_root;
}
