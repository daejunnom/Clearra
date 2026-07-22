use std::{collections::HashMap, sync::Arc};

use super::{
    extended_board::ExtendedBoard,
    extended_inverse_catalog::{DenseExtendedGeometryCatalog, ExtendedInverseCatalog},
    geometry_family::{GeometrySolutionFamily, FAMILY_EMPTY, FAMILY_INVALID},
    piece_index,
};

// This is an optimization budget, not a search limit. Exceeding it returns to
// the exact monolithic compiler without dropping a candidate.
const DENSE_COMPONENT_STATE_BUDGET: usize = 1_000_000;

#[derive(Clone, Copy)]
struct ExtendedComponentEntry {
    counts: [u8; 7],
    family: u32,
}

pub(super) enum ExtendedComponentPlanResult {
    NotApplicable,
    Impossible,
    Complete { family: u32, expanded_nodes: usize },
    StorageUnavailable,
}

pub(super) fn compile_component_plan(
    catalog: &ExtendedInverseCatalog,
    remaining: ExtendedBoard,
    _depth: u8,
    used_counts: [u8; 7],
    targets: &[[u8; 7]],
    feasible_mask: u8,
    family: &mut GeometrySolutionFamily,
) -> ExtendedComponentPlanResult {
    if remaining.count_ones() < 8 || remaining.count_ones() > 64 {
        return ExtendedComponentPlanResult::NotApplicable;
    }
    let Some(dense_catalog) = catalog.dense_geometry() else {
        return ExtendedComponentPlanResult::NotApplicable;
    };
    let Some(dense_remaining) = dense_catalog.encode(remaining) else {
        return ExtendedComponentPlanResult::NotApplicable;
    };

    compile_dense_component_plan(
        catalog,
        dense_catalog,
        dense_remaining,
        used_counts,
        targets,
        feasible_mask,
        family,
    )
}

pub(super) fn compile_dense_component_plan(
    catalog: &ExtendedInverseCatalog,
    dense_catalog: &DenseExtendedGeometryCatalog,
    dense_remaining: u64,
    used_counts: [u8; 7],
    targets: &[[u8; 7]],
    feasible_mask: u8,
    family: &mut GeometrySolutionFamily,
) -> ExtendedComponentPlanResult {
    let components =
        match decompose_hypergraph(catalog, dense_catalog, dense_remaining, feasible_mask) {
            HypergraphDecomposition::Connected => {
                return ExtendedComponentPlanResult::NotApplicable
            }
            HypergraphDecomposition::Impossible => return ExtendedComponentPlanResult::Impossible,
            HypergraphDecomposition::Components(components) => components,
        };
    if components
        .iter()
        .any(|component| !component.count_ones().is_multiple_of(4))
    {
        return ExtendedComponentPlanResult::Impossible;
    }

    let checkpoint = family.checkpoint();
    let mut compiler = DenseComponentCompiler::new(
        catalog,
        dense_catalog,
        feasible_mask,
        DENSE_COMPONENT_STATE_BUDGET,
    );
    let mut aggregate = vec![ExtendedComponentEntry {
        counts: [0; 7],
        family: FAMILY_EMPTY,
    }];

    for component in components {
        let table = match compiler.compile(component, family) {
            Ok(table) => table,
            Err(DenseComponentCompileError::BudgetExceeded) => {
                family.rewind(checkpoint);
                return ExtendedComponentPlanResult::NotApplicable;
            }
            Err(DenseComponentCompileError::StorageUnavailable) => {
                family.rewind(checkpoint);
                return ExtendedComponentPlanResult::StorageUnavailable;
            }
        };
        if table.is_empty() {
            family.rewind(checkpoint);
            return ExtendedComponentPlanResult::Impossible;
        }
        aggregate = match product_signature_tables(
            &aggregate,
            table.as_ref(),
            used_counts,
            targets,
            family,
        ) {
            Ok(entries) => entries,
            Err(()) => {
                family.rewind(checkpoint);
                return ExtendedComponentPlanResult::StorageUnavailable;
            }
        };
        if aggregate.is_empty() {
            family.rewind(checkpoint);
            return ExtendedComponentPlanResult::Impossible;
        }
    }

    let mut root = FAMILY_INVALID;
    for entry in aggregate
        .into_iter()
        .filter(|entry| counts_complete(used_counts, entry.counts, targets))
    {
        let Some(union) = family.union(root, entry.family) else {
            family.rewind(checkpoint);
            return ExtendedComponentPlanResult::StorageUnavailable;
        };
        root = union;
    }
    if root == FAMILY_INVALID {
        family.rewind(checkpoint);
        return ExtendedComponentPlanResult::Impossible;
    }
    ExtendedComponentPlanResult::Complete {
        family: root,
        expanded_nodes: compiler.expanded_nodes,
    }
}

enum HypergraphDecomposition {
    Connected,
    Impossible,
    Components(Vec<u64>),
}

fn decompose_hypergraph(
    catalog: &ExtendedInverseCatalog,
    dense_catalog: &DenseExtendedGeometryCatalog,
    remaining: u64,
    feasible_mask: u8,
) -> HypergraphDecomposition {
    let mut parents = core::array::from_fn(|index| index as u8);
    let mut supported = 0_u64;
    for (row_id, row) in catalog.skeletons().iter().enumerate() {
        if feasible_mask & (1_u8 << piece_index(row.piece)) == 0 {
            continue;
        }
        let Some(cells) = dense_catalog.skeleton_cells(row_id as u32) else {
            return HypergraphDecomposition::Impossible;
        };
        if cells & remaining != cells {
            continue;
        }
        let first = cells.trailing_zeros() as u8;
        let mut rest = cells & (cells - 1);
        while rest != 0 {
            let cell = rest.trailing_zeros() as u8;
            union_cells(&mut parents, first, cell);
            rest &= rest - 1;
        }
        supported |= cells;
    }
    if supported != remaining {
        return HypergraphDecomposition::Impossible;
    }

    let mut groups = [0_u64; 64];
    let mut cells = remaining;
    while cells != 0 {
        let cell = cells.trailing_zeros() as u8;
        let root = find_root(&mut parents, cell);
        groups[usize::from(root)] |= 1_u64 << cell;
        cells &= cells - 1;
    }
    let mut components = groups
        .into_iter()
        .filter(|component| *component != 0)
        .collect::<Vec<_>>();
    if components.len() <= 1 {
        return HypergraphDecomposition::Connected;
    }
    components
        .sort_unstable_by_key(|component| (component.count_ones(), component.trailing_zeros()));
    HypergraphDecomposition::Components(components)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DenseComponentCompileError {
    BudgetExceeded,
    StorageUnavailable,
}

struct DenseComponentCompiler<'a> {
    catalog: &'a ExtendedInverseCatalog,
    dense_catalog: &'a DenseExtendedGeometryCatalog,
    feasible_mask: u8,
    state_budget: usize,
    memo: HashMap<u64, Arc<[ExtendedComponentEntry]>>,
    expanded_nodes: usize,
}

impl<'a> DenseComponentCompiler<'a> {
    fn new(
        catalog: &'a ExtendedInverseCatalog,
        dense_catalog: &'a DenseExtendedGeometryCatalog,
        feasible_mask: u8,
        state_budget: usize,
    ) -> Self {
        Self {
            catalog,
            dense_catalog,
            feasible_mask,
            state_budget,
            memo: HashMap::new(),
            expanded_nodes: 0,
        }
    }

    fn compile(
        &mut self,
        remaining: u64,
        family: &mut GeometrySolutionFamily,
    ) -> Result<Arc<[ExtendedComponentEntry]>, DenseComponentCompileError> {
        if let Some(entries) = self.memo.get(&remaining) {
            return Ok(Arc::clone(entries));
        }
        if self.expanded_nodes >= self.state_budget {
            return Err(DenseComponentCompileError::BudgetExceeded);
        }
        self.expanded_nodes += 1;

        let entries = if remaining == 0 {
            vec![ExtendedComponentEntry {
                counts: [0; 7],
                family: FAMILY_EMPTY,
            }]
        } else if !remaining.count_ones().is_multiple_of(4) {
            Vec::new()
        } else {
            let Some(pivot) = self.minimum_domain_cell(remaining) else {
                return self.memoize(remaining, Vec::new());
            };
            let world = self
                .dense_catalog
                .world_cell(pivot)
                .ok_or(DenseComponentCompileError::StorageUnavailable)?;
            let support_len = self.catalog.support(world).len();
            let mut combined = Vec::new();
            for support_index in 0..support_len {
                let row_id = self.catalog.support(world)[support_index];
                let row = self.catalog.skeleton(row_id);
                if self.feasible_mask & (1_u8 << piece_index(row.piece)) == 0 {
                    continue;
                }
                let cells = self
                    .dense_catalog
                    .skeleton_cells(row_id)
                    .ok_or(DenseComponentCompileError::StorageUnavailable)?;
                if cells & remaining != cells {
                    continue;
                }
                let suffixes = self.compile(remaining ^ cells, family)?;
                for suffix in suffixes.iter().copied() {
                    let mut counts = suffix.counts;
                    counts[piece_index(row.piece)] = counts[piece_index(row.piece)]
                        .checked_add(1)
                        .ok_or(DenseComponentCompileError::StorageUnavailable)?;
                    let branch = family
                        .append(row_id, suffix.family)
                        .ok_or(DenseComponentCompileError::StorageUnavailable)?;
                    merge_signature_entry(&mut combined, counts, branch, family)?;
                }
            }
            combined
        };
        self.memoize(remaining, entries)
    }

    fn minimum_domain_cell(&self, remaining: u64) -> Option<u8> {
        let mut best = None;
        let mut cells = remaining;
        while cells != 0 {
            let dense = cells.trailing_zeros() as u8;
            let world = self.dense_catalog.world_cell(dense)?;
            let count = self
                .catalog
                .support(world)
                .iter()
                .copied()
                .filter(|row_id| {
                    let row = self.catalog.skeleton(*row_id);
                    if self.feasible_mask & (1_u8 << piece_index(row.piece)) == 0 {
                        return false;
                    }
                    self.dense_catalog
                        .skeleton_cells(*row_id)
                        .is_some_and(|row_cells| row_cells & remaining == row_cells)
                })
                .count();
            if count == 0 {
                return None;
            }
            if best.is_none_or(|(_, best_count)| count < best_count) {
                best = Some((dense, count));
            }
            cells &= cells - 1;
        }
        best.map(|(cell, _)| cell)
    }

    fn memoize(
        &mut self,
        remaining: u64,
        entries: Vec<ExtendedComponentEntry>,
    ) -> Result<Arc<[ExtendedComponentEntry]>, DenseComponentCompileError> {
        self.memo
            .try_reserve(1)
            .map_err(|_| DenseComponentCompileError::StorageUnavailable)?;
        let entries: Arc<[ExtendedComponentEntry]> = entries.into();
        self.memo.insert(remaining, Arc::clone(&entries));
        Ok(entries)
    }
}

fn product_signature_tables(
    left: &[ExtendedComponentEntry],
    right: &[ExtendedComponentEntry],
    used_counts: [u8; 7],
    targets: &[[u8; 7]],
    family: &mut GeometrySolutionFamily,
) -> Result<Vec<ExtendedComponentEntry>, ()> {
    let mut product = Vec::new();
    for left_entry in left {
        for right_entry in right {
            let Some(counts) = add_counts(left_entry.counts, right_entry.counts) else {
                continue;
            };
            if !counts_admissible(used_counts, counts, targets) {
                continue;
            }
            let branch = family
                .product(left_entry.family, right_entry.family)
                .ok_or(())?;
            merge_signature_entry(&mut product, counts, branch, family).map_err(|_| ())?;
        }
    }
    Ok(product)
}

fn merge_signature_entry(
    entries: &mut Vec<ExtendedComponentEntry>,
    counts: [u8; 7],
    family_ref: u32,
    family: &mut GeometrySolutionFamily,
) -> Result<(), DenseComponentCompileError> {
    match entries.binary_search_by_key(&counts, |entry| entry.counts) {
        Ok(index) => {
            entries[index].family = family
                .union(entries[index].family, family_ref)
                .ok_or(DenseComponentCompileError::StorageUnavailable)?;
        }
        Err(index) => {
            entries
                .try_reserve(1)
                .map_err(|_| DenseComponentCompileError::StorageUnavailable)?;
            entries.insert(
                index,
                ExtendedComponentEntry {
                    counts,
                    family: family_ref,
                },
            );
        }
    }
    Ok(())
}

fn add_counts(left: [u8; 7], right: [u8; 7]) -> Option<[u8; 7]> {
    let mut result = [0_u8; 7];
    for piece in 0..7 {
        result[piece] = left[piece].checked_add(right[piece])?;
    }
    Some(result)
}

fn counts_admissible(used: [u8; 7], local: [u8; 7], targets: &[[u8; 7]]) -> bool {
    targets.iter().any(|target| {
        (0..7).all(|piece| {
            used[piece]
                .checked_add(local[piece])
                .is_some_and(|count| count <= target[piece])
        })
    })
}

fn counts_complete(used: [u8; 7], local: [u8; 7], targets: &[[u8; 7]]) -> bool {
    targets.iter().any(|target| {
        (0..7).all(|piece| used[piece].checked_add(local[piece]) == Some(target[piece]))
    })
}

fn find_root(parents: &mut [u8; 64], cell: u8) -> u8 {
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

fn union_cells(parents: &mut [u8; 64], left: u8, right: u8) {
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
