use super::{
    catalog::GeometryCatalog,
    geometry::{pack_piece_counts, TargetGroup},
    geometry_domain::row_feasible,
    geometry_family::{GeometrySolutionFamily, FAMILY_EMPTY, FAMILY_INVALID},
    piece_index, MAX_BOARD64_PIECES,
};

const COMPONENT_ENUMERATION_NODE_LIMIT: usize = 8_192;
const COMPONENT_MAX_CELLS: u32 = 16;
const SEPARATOR_JOIN_MAX_CELLS: u32 = 24;
const SEPARATOR_JOIN_MAX_RESIDUAL_CELLS: u32 = COMPONENT_MAX_CELLS;
const SEPARATOR_JOIN_MAX_PATHS: u128 = 1_000_000;

#[derive(Clone, Copy, Debug)]
pub(super) struct ComponentFamilyEntry {
    pub piece_signature: u32,
    pub family: u32,
    path_count: u128,
}

#[derive(Debug)]
pub(super) struct ComponentPlan {
    pub owner_cells: u64,
    pub remainder_cells: u64,
    pub entries: Vec<ComponentFamilyEntry>,
    pub expanded_nodes: usize,
}

pub(super) enum ComponentPlanResult {
    NotApplicable,
    Impossible,
    Complete { family: u32, expanded_nodes: usize },
    Ready(ComponentPlan),
    StorageUnavailable,
}

// Component compilation keeps each bounded scratch surface explicit.
#[allow(clippy::too_many_arguments)]
pub(super) fn compile_component_plan(
    catalog: &GeometryCatalog,
    remaining: u64,
    depth: u8,
    used_counts: [u8; 7],
    targets: &[TargetGroup],
    admissible_prefixes: &[u32],
    feasible_piece_mask: u8,
    family: &mut GeometrySolutionFamily,
) -> ComponentPlanResult {
    if remaining.count_ones() < 8 || !component_analysis_should_run(catalog, remaining, depth) {
        return ComponentPlanResult::NotApplicable;
    }
    if catalog.initial_board() != 0 && remaining.count_ones() <= SEPARATOR_JOIN_MAX_RESIDUAL_CELLS {
        if let Some(split) = catalog.separator_catalog().certified_split(remaining) {
            if let Some(result) = compile_separator_join(
                catalog,
                split.owner_cells,
                split.remainder_cells,
                used_counts,
                targets,
                admissible_prefixes,
                family,
            ) {
                return result;
            }
        }
    }
    let Some(components) = decompose(catalog, remaining, feasible_piece_mask) else {
        return ComponentPlanResult::Impossible;
    };
    if components.len() <= 1 {
        return ComponentPlanResult::NotApplicable;
    }
    if components
        .iter()
        .any(|component| !component.count_ones().is_multiple_of(4))
    {
        return ComponentPlanResult::Impossible;
    }
    let Some((component, remainder)) = canonical_composition_owner(remaining, &components) else {
        return ComponentPlanResult::NotApplicable;
    };
    if component.count_ones() > COMPONENT_MAX_CELLS {
        return ComponentPlanResult::NotApplicable;
    }

    let mut compiler = ComponentCompiler {
        catalog,
        admissible_prefixes,
        base_used_counts: used_counts,
        local_counts: [0; 7],
        rows: [0; MAX_BOARD64_PIECES],
        entries: Vec::new(),
        expanded_nodes: 0,
        aborted: false,
        storage_unavailable: false,
    };
    compiler.enumerate(component, 0, family);
    if compiler.storage_unavailable {
        return ComponentPlanResult::StorageUnavailable;
    }
    if compiler.aborted {
        return ComponentPlanResult::NotApplicable;
    }
    if compiler.entries.is_empty() {
        return ComponentPlanResult::Impossible;
    }
    compiler
        .entries
        .sort_unstable_by_key(|entry| entry.piece_signature);
    ComponentPlanResult::Ready(ComponentPlan {
        owner_cells: component,
        remainder_cells: remainder,
        entries: compiler.entries,
        expanded_nodes: compiler.expanded_nodes,
    })
}

fn canonical_composition_owner(remaining: u64, components: &[u64]) -> Option<(u64, u64)> {
    if remaining == 0 || components.len() <= 1 {
        return None;
    }
    let mut partition = 0_u64;
    let mut previous_key = None;
    for component in components.iter().copied() {
        let key = (component.count_ones(), component.trailing_zeros());
        if component == 0
            || component & !remaining != 0
            || partition & component != 0
            || previous_key.is_some_and(|previous| previous >= key)
        {
            return None;
        }
        partition |= component;
        previous_key = Some(key);
    }
    if partition != remaining {
        return None;
    }
    let owner = components[0];
    let remainder = remaining & !owner;
    (owner != 0 && remainder != 0 && owner & remainder == 0).then_some((owner, remainder))
}

struct ComponentCompiler<'a> {
    catalog: &'a GeometryCatalog,
    admissible_prefixes: &'a [u32],
    base_used_counts: [u8; 7],
    local_counts: [u8; 7],
    rows: [u32; MAX_BOARD64_PIECES],
    entries: Vec<ComponentFamilyEntry>,
    expanded_nodes: usize,
    aborted: bool,
    storage_unavailable: bool,
}

impl ComponentCompiler<'_> {
    fn enumerate(&mut self, remaining: u64, depth: usize, family: &mut GeometrySolutionFamily) {
        if self.aborted || self.storage_unavailable {
            return;
        }
        self.expanded_nodes = self.expanded_nodes.saturating_add(1);
        if self.expanded_nodes > COMPONENT_ENUMERATION_NODE_LIMIT {
            self.aborted = true;
            return;
        }
        if remaining == 0 {
            self.record_solution(depth, family);
            return;
        }
        if depth >= MAX_BOARD64_PIECES || !remaining.count_ones().is_multiple_of(4) {
            return;
        }
        let feasible_piece_mask = self.feasible_piece_mask();
        let Some(pivot) = minimum_domain_cell(self.catalog, remaining, feasible_piece_mask) else {
            return;
        };
        for row_id in self.catalog.support(pivot).iter().copied() {
            if !row_feasible(self.catalog, row_id, remaining, feasible_piece_mask) {
                continue;
            }
            let piece = piece_index(self.catalog.skeleton(row_id).piece);
            self.rows[depth] = row_id;
            self.local_counts[piece] += 1;
            self.enumerate(
                remaining ^ self.catalog.skeleton(row_id).cells,
                depth + 1,
                family,
            );
            self.local_counts[piece] -= 1;
            if self.aborted || self.storage_unavailable {
                return;
            }
        }
    }

    fn feasible_piece_mask(&self) -> u8 {
        let mut mask = 0_u8;
        for piece in 0..7 {
            let mut counts = self.base_used_counts;
            for (count, local_count) in counts.iter_mut().zip(self.local_counts) {
                *count = count.saturating_add(local_count);
            }
            counts[piece] = counts[piece].saturating_add(1);
            if self
                .admissible_prefixes
                .binary_search(&pack_piece_counts(counts))
                .is_ok()
            {
                mask |= 1_u8 << piece;
            }
        }
        mask
    }

    fn record_solution(&mut self, depth: usize, family: &mut GeometrySolutionFamily) {
        let mut path = FAMILY_EMPTY;
        for row_id in self.rows[..depth].iter().rev().copied() {
            let Some(next) = family.append(row_id, path) else {
                self.storage_unavailable = true;
                return;
            };
            path = next;
        }
        let signature = pack_piece_counts(self.local_counts);
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| entry.piece_signature == signature)
        {
            let Some(union) = family.union(existing.family, path) else {
                self.storage_unavailable = true;
                return;
            };
            existing.family = union;
            existing.path_count = existing.path_count.saturating_add(1);
            return;
        }
        if self.entries.try_reserve(1).is_err() {
            self.storage_unavailable = true;
            return;
        }
        self.entries.push(ComponentFamilyEntry {
            piece_signature: signature,
            family: path,
            path_count: 1,
        });
    }
}

fn minimum_domain_cell(
    catalog: &GeometryCatalog,
    remaining: u64,
    feasible_piece_mask: u8,
) -> Option<u8> {
    let mut best = None;
    let mut cells = remaining;
    while cells != 0 {
        let cell = cells.trailing_zeros() as u8;
        cells &= cells - 1;
        let count = catalog
            .support(cell)
            .iter()
            .copied()
            .filter(|row_id| row_feasible(catalog, *row_id, remaining, feasible_piece_mask))
            .count();
        if count == 0 {
            return None;
        }
        if best.is_none_or(|(_, best_count)| count < best_count) {
            best = Some((cell, count));
        }
    }
    best.map(|(cell, _)| cell)
}

fn decompose(
    catalog: &GeometryCatalog,
    remaining: u64,
    feasible_piece_mask: u8,
) -> Option<Vec<u64>> {
    let mut parents = core::array::from_fn(|index| index as u8);
    let mut supported = 0_u64;
    for row_id in 0..catalog.skeleton_count() as u32 {
        if !row_feasible(catalog, row_id, remaining, feasible_piece_mask) {
            continue;
        }
        let row = catalog.skeleton(row_id).cells;
        let first = row.trailing_zeros() as u8;
        let mut rest = row & !(1_u64 << first);
        while rest != 0 {
            let cell = rest.trailing_zeros() as u8;
            rest &= rest - 1;
            union_cells(&mut parents, first, cell);
        }
        supported |= row;
    }
    if supported & remaining != remaining {
        return None;
    }
    let mut groups = [0_u64; 64];
    let mut cells = remaining;
    while cells != 0 {
        let cell = cells.trailing_zeros() as u8;
        cells &= cells - 1;
        let root = find_root(&mut parents, cell);
        groups[root as usize] |= 1_u64 << cell;
    }
    let mut components = groups
        .into_iter()
        .filter(|component| *component != 0)
        .collect::<Vec<_>>();
    components
        .sort_unstable_by_key(|component| (component.count_ones(), component.trailing_zeros()));
    Some(components)
}

fn component_analysis_should_run(catalog: &GeometryCatalog, remaining: u64, depth: u8) -> bool {
    (depth == 0 && catalog.initial_board() != 0)
        || (remaining.count_ones() <= COMPONENT_MAX_CELLS
            && spatial_component_count(catalog, remaining) > 1)
}

fn compile_separator_join(
    catalog: &GeometryCatalog,
    left_cells: u64,
    right_cells: u64,
    used_counts: [u8; 7],
    targets: &[TargetGroup],
    admissible_prefixes: &[u32],
    family: &mut GeometrySolutionFamily,
) -> Option<ComponentPlanResult> {
    if left_cells.count_ones() > SEPARATOR_JOIN_MAX_CELLS
        || right_cells.count_ones() > SEPARATOR_JOIN_MAX_CELLS
    {
        return None;
    }
    let checkpoint = family.checkpoint();
    let mut left = component_compiler(catalog, admissible_prefixes, used_counts);
    left.enumerate(left_cells, 0, family);
    if left.storage_unavailable {
        family.rewind(checkpoint);
        return Some(ComponentPlanResult::StorageUnavailable);
    }
    if left.aborted {
        family.rewind(checkpoint);
        return None;
    }
    if left.entries.is_empty() {
        family.rewind(checkpoint);
        return Some(ComponentPlanResult::Impossible);
    }

    let mut right = component_compiler(catalog, admissible_prefixes, used_counts);
    right.enumerate(right_cells, 0, family);
    if right.storage_unavailable {
        family.rewind(checkpoint);
        return Some(ComponentPlanResult::StorageUnavailable);
    }
    if right.aborted {
        family.rewind(checkpoint);
        return None;
    }
    if right.entries.is_empty() {
        family.rewind(checkpoint);
        return Some(ComponentPlanResult::Impossible);
    }

    let mut joined_path_count = 0_u128;
    for left_entry in &left.entries {
        for right_entry in &right.entries {
            if !combined_signature_is_target(
                used_counts,
                left_entry.piece_signature,
                right_entry.piece_signature,
                targets,
            ) {
                continue;
            }
            joined_path_count = joined_path_count
                .saturating_add(left_entry.path_count.saturating_mul(right_entry.path_count));
            if joined_path_count > SEPARATOR_JOIN_MAX_PATHS {
                family.rewind(checkpoint);
                return None;
            }
        }
    }

    let mut root = FAMILY_INVALID;
    for left_entry in &left.entries {
        for right_entry in &right.entries {
            if !combined_signature_is_target(
                used_counts,
                left_entry.piece_signature,
                right_entry.piece_signature,
                targets,
            ) {
                continue;
            }
            let Some(product) = family.product(left_entry.family, right_entry.family) else {
                family.rewind(checkpoint);
                return Some(ComponentPlanResult::StorageUnavailable);
            };
            let Some(union) = family.union(root, product) else {
                family.rewind(checkpoint);
                return Some(ComponentPlanResult::StorageUnavailable);
            };
            root = union;
        }
    }
    if root == FAMILY_INVALID {
        family.rewind(checkpoint);
        return Some(ComponentPlanResult::Impossible);
    }
    Some(ComponentPlanResult::Complete {
        family: root,
        expanded_nodes: left.expanded_nodes.saturating_add(right.expanded_nodes),
    })
}

fn component_compiler<'a>(
    catalog: &'a GeometryCatalog,
    admissible_prefixes: &'a [u32],
    used_counts: [u8; 7],
) -> ComponentCompiler<'a> {
    ComponentCompiler {
        catalog,
        admissible_prefixes,
        base_used_counts: used_counts,
        local_counts: [0; 7],
        rows: [0; MAX_BOARD64_PIECES],
        entries: Vec::new(),
        expanded_nodes: 0,
        aborted: false,
        storage_unavailable: false,
    }
}

fn combined_signature_is_target(
    mut counts: [u8; 7],
    left: u32,
    right: u32,
    targets: &[TargetGroup],
) -> bool {
    for (piece, count) in counts.iter_mut().enumerate() {
        let left_count = ((left >> (piece * 4)) & 0x0f) as u8;
        let right_count = ((right >> (piece * 4)) & 0x0f) as u8;
        let Some(combined_count) = count
            .checked_add(left_count)
            .and_then(|count| count.checked_add(right_count))
        else {
            return false;
        };
        *count = combined_count;
    }
    targets.iter().any(|target| target.key.counts() == counts)
}

fn spatial_component_count(catalog: &GeometryCatalog, remaining: u64) -> usize {
    let mut unseen = remaining;
    let mut count = 0;
    while unseen != 0 {
        count += 1;
        let start = unseen & unseen.wrapping_neg();
        let mut frontier = start;
        unseen &= !start;
        while frontier != 0 {
            let cell = frontier.trailing_zeros() as u8;
            frontier &= frontier - 1;
            let x = cell % catalog.width();
            let y = cell / catalog.width();
            for neighbor in [
                (x > 0).then(|| cell - 1),
                (x + 1 < catalog.width()).then(|| cell + 1),
                (y > 0).then(|| cell - catalog.width()),
                (y + 1 < catalog.height()).then(|| cell + catalog.width()),
            ]
            .into_iter()
            .flatten()
            {
                let bit = 1_u64 << neighbor;
                if unseen & bit != 0 {
                    unseen &= !bit;
                    frontier |= bit;
                }
            }
        }
    }
    count
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
