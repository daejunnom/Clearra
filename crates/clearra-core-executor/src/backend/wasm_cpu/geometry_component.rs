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

/// Comparison policies do not change the exact monolithic fallback or the
/// component ownership rule. Legacy remains the production default until A/B.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum ComponentJoinPolicy {
    Legacy = 0,
    Off = 1,
    Complement = 2,
}

#[cfg(any(
    test,
    feature = "wasm-stage-profiling",
    feature = "minimum-physical-ab"
))]
std::thread_local! {
    static COMPONENT_JOIN_POLICY: std::cell::Cell<ComponentJoinPolicy> =
        const { std::cell::Cell::new(ComponentJoinPolicy::Legacy) };
}

pub(super) fn component_join_policy() -> ComponentJoinPolicy {
    #[cfg(any(
        test,
        feature = "wasm-stage-profiling",
        feature = "minimum-physical-ab"
    ))]
    {
        COMPONENT_JOIN_POLICY.with(std::cell::Cell::get)
    }
    #[cfg(not(any(
        test,
        feature = "wasm-stage-profiling",
        feature = "minimum-physical-ab"
    )))]
    {
        ComponentJoinPolicy::Legacy
    }
}

/// Configure an isolated worker before admitting work. The host is responsible
/// for rejecting policy changes while that worker still owns an active job.
#[cfg(any(
    test,
    feature = "wasm-stage-profiling",
    feature = "minimum-physical-ab"
))]
pub(crate) fn set_component_join_policy(policy: ComponentJoinPolicy) -> ComponentJoinPolicy {
    COMPONENT_JOIN_POLICY.with(|current| current.replace(policy))
}

/// The host must scope this separately in each worker; a worker never inherits
/// a policy from another thread. The previous policy is restored on unwind.
#[cfg(any(
    test,
    feature = "wasm-stage-profiling",
    feature = "minimum-physical-ab"
))]
pub(crate) fn with_component_join_policy<T>(
    policy: ComponentJoinPolicy,
    run: impl FnOnce() -> T,
) -> T {
    struct Restore(ComponentJoinPolicy);
    impl Drop for Restore {
        fn drop(&mut self) {
            set_component_join_policy(self.0);
        }
    }
    let _restore = Restore(set_component_join_policy(policy));
    run()
}

/// A bounded index of count signatures, not geometry or reachability evidence.
/// Retain original right-table indices so products are emitted in legacy order.
pub(super) struct ComponentSignatureJoinIndex {
    right: Vec<([u8; 7], usize)>,
    targets: Vec<[u8; 7]>,
    matching_indices: Vec<usize>,
}

impl ComponentSignatureJoinIndex {
    pub(super) fn new(
        right_counts: impl ExactSizeIterator<Item = [u8; 7]>,
        targets: impl IntoIterator<Item = [u8; 7]>,
    ) -> Result<Self, ()> {
        let mut right = Vec::new();
        right
            .try_reserve_exact(right_counts.len())
            .map_err(|_| ())?;
        for (index, counts) in right_counts.enumerate() {
            right.push((counts, index));
        }
        right.sort_unstable();
        let mut target_counts = Vec::new();
        for counts in targets {
            target_counts.try_reserve(1).map_err(|_| ())?;
            target_counts.push(counts);
        }
        target_counts.sort_unstable();
        target_counts.dedup();
        // A right entry can match only one deduplicated target for a fixed left
        // entry. Reserve before creating products so optional scratch failure
        // can fall back to the legacy join without retaining partial products.
        let mut matching_indices = Vec::new();
        matching_indices
            .try_reserve_exact(right.len())
            .map_err(|_| ())?;
        Ok(Self {
            right,
            targets: target_counts,
            matching_indices,
        })
    }

    pub(super) fn matching_right_indices(&mut self, used: [u8; 7], left: [u8; 7]) -> &[usize] {
        self.matching_indices.clear();
        for target in &self.targets {
            let mut complement = [0_u8; 7];
            let mut admissible = true;
            for piece in 0..7 {
                let Some(remaining) = target[piece]
                    .checked_sub(used[piece])
                    .and_then(|count| count.checked_sub(left[piece]))
                else {
                    admissible = false;
                    break;
                };
                complement[piece] = remaining;
            }
            if !admissible {
                continue;
            }
            let begin = self
                .right
                .partition_point(|(counts, _)| *counts < complement);
            let end = self
                .right
                .partition_point(|(counts, _)| *counts <= complement);
            for (_, index) in &self.right[begin..end] {
                self.matching_indices.push(*index);
            }
        }
        self.matching_indices.sort_unstable();
        self.matching_indices.dedup();
        &self.matching_indices
    }
}

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
    if component_join_policy() == ComponentJoinPolicy::Off {
        return ComponentPlanResult::NotApplicable;
    }
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

    let joined = join_separator_tables(&left.entries, &right.entries, used_counts, targets, family);
    let root = match joined {
        SeparatorJoinResult::Ready(root) => root,
        SeparatorJoinResult::PathLimit => {
            family.rewind(checkpoint);
            return None;
        }
        SeparatorJoinResult::StorageUnavailable => {
            family.rewind(checkpoint);
            return Some(ComponentPlanResult::StorageUnavailable);
        }
    };
    if root == FAMILY_INVALID {
        family.rewind(checkpoint);
        return Some(ComponentPlanResult::Impossible);
    }
    Some(ComponentPlanResult::Complete {
        family: root,
        expanded_nodes: left.expanded_nodes.saturating_add(right.expanded_nodes),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SeparatorJoinResult {
    Ready(u32),
    PathLimit,
    StorageUnavailable,
}

fn join_separator_tables(
    left: &[ComponentFamilyEntry],
    right: &[ComponentFamilyEntry],
    used_counts: [u8; 7],
    targets: &[TargetGroup],
    family: &mut GeometrySolutionFamily,
) -> SeparatorJoinResult {
    match component_join_policy() {
        ComponentJoinPolicy::Complement => {
            join_separator_tables_complement(left, right, used_counts, targets, family)
        }
        ComponentJoinPolicy::Legacy | ComponentJoinPolicy::Off => {
            join_separator_tables_legacy(left, right, used_counts, targets, family)
        }
    }
}

fn join_separator_tables_legacy(
    left: &[ComponentFamilyEntry],
    right: &[ComponentFamilyEntry],
    used_counts: [u8; 7],
    targets: &[TargetGroup],
    family: &mut GeometrySolutionFamily,
) -> SeparatorJoinResult {
    let mut joined_path_count = 0_u128;
    for left_entry in left {
        for right_entry in right {
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
                return SeparatorJoinResult::PathLimit;
            }
        }
    }

    let mut root = FAMILY_INVALID;
    for left_entry in left {
        for right_entry in right {
            if !combined_signature_is_target(
                used_counts,
                left_entry.piece_signature,
                right_entry.piece_signature,
                targets,
            ) {
                continue;
            }
            let Some(product) = family.product(left_entry.family, right_entry.family) else {
                return SeparatorJoinResult::StorageUnavailable;
            };
            let Some(union) = family.union(root, product) else {
                return SeparatorJoinResult::StorageUnavailable;
            };
            root = union;
        }
    }
    SeparatorJoinResult::Ready(root)
}

fn signature_counts(signature: u32) -> [u8; 7] {
    core::array::from_fn(|piece| ((signature >> (piece * 4)) & 0x0f) as u8)
}

fn join_separator_tables_complement(
    left: &[ComponentFamilyEntry],
    right: &[ComponentFamilyEntry],
    used_counts: [u8; 7],
    targets: &[TargetGroup],
    family: &mut GeometrySolutionFamily,
) -> SeparatorJoinResult {
    let Ok(mut index) = ComponentSignatureJoinIndex::new(
        right
            .iter()
            .map(|entry| signature_counts(entry.piece_signature)),
        targets.iter().map(|target| target.key.counts()),
    ) else {
        return join_separator_tables_legacy(left, right, used_counts, targets, family);
    };
    let mut matches = Vec::new();
    let mut joined_path_count = 0_u128;
    for (left_index, left_entry) in left.iter().enumerate() {
        let right_indices =
            index.matching_right_indices(used_counts, signature_counts(left_entry.piece_signature));
        if matches.try_reserve(right_indices.len()).is_err() {
            drop(index);
            drop(matches);
            return join_separator_tables_legacy(left, right, used_counts, targets, family);
        }
        for right_index in right_indices.iter().copied() {
            joined_path_count = joined_path_count.saturating_add(
                left_entry
                    .path_count
                    .saturating_mul(right[right_index].path_count),
            );
            if joined_path_count > SEPARATOR_JOIN_MAX_PATHS {
                return SeparatorJoinResult::PathLimit;
            }
            matches.push((left_index, right_index));
        }
    }
    drop(index);
    let mut root = FAMILY_INVALID;
    for (left_index, right_index) in matches {
        let Some(product) = family.product(left[left_index].family, right[right_index].family)
        else {
            return SeparatorJoinResult::StorageUnavailable;
        };
        let Some(union) = family.union(root, product) else {
            return SeparatorJoinResult::StorageUnavailable;
        };
        root = union;
    }
    SeparatorJoinResult::Ready(root)
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
    use clearra_supply::pattern_universe::PieceMultisetKey;

    use super::*;

    fn counts(first: u8, second: u8) -> [u8; 7] {
        [first, second, 0, 0, 0, 0, 0]
    }

    fn target(counts: [u8; 7]) -> TargetGroup {
        TargetGroup {
            key: PieceMultisetKey::from_counts(counts),
            pattern_index_id: 0,
            possible_patterns: Arc::new(PatternBitSet::all(1)),
            pattern_index: None,
        }
    }

    fn entry(
        family: &mut GeometrySolutionFamily,
        row: u32,
        counts: [u8; 7],
    ) -> ComponentFamilyEntry {
        ComponentFamilyEntry {
            piece_signature: pack_piece_counts(counts),
            family: family.append(row, FAMILY_EMPTY).expect("fixture family"),
            path_count: 1,
        }
    }

    #[test]
    fn complement_index_matches_exact_sums_in_original_order() {
        let right = [
            counts(0, 1),
            counts(16, 0),
            counts(1, 0),
            counts(0, 1),
            counts(0, 0),
        ];
        let targets = [counts(2, 1), counts(16, 1), counts(2, 1), counts(0, 0)];
        let mut index = ComponentSignatureJoinIndex::new(right.into_iter(), targets)
            .expect("fixture signature index");
        for used in [counts(0, 0), counts(1, 0), counts(0, 1)] {
            for left in [counts(1, 0), counts(0, 1), counts(17, 0), counts(0, 0)] {
                let expected = right
                    .iter()
                    .enumerate()
                    .filter_map(|(index, right)| {
                        targets
                            .iter()
                            .any(|target| {
                                (0..7).all(|piece| {
                                    u16::from(used[piece])
                                        + u16::from(left[piece])
                                        + u16::from(right[piece])
                                        == u16::from(target[piece])
                                })
                            })
                            .then_some(index)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(index.matching_right_indices(used, left), expected);
            }
        }
    }

    #[test]
    fn component_policy_setter_and_nested_scope_restore_previous_policy() {
        let original = component_join_policy();
        with_component_join_policy(ComponentJoinPolicy::Legacy, || {
            assert_eq!(
                set_component_join_policy(ComponentJoinPolicy::Off),
                ComponentJoinPolicy::Legacy,
            );
            with_component_join_policy(ComponentJoinPolicy::Complement, || {
                assert_eq!(component_join_policy(), ComponentJoinPolicy::Complement);
            });
            assert_eq!(component_join_policy(), ComponentJoinPolicy::Off);
        });
        assert_eq!(component_join_policy(), original);
    }

    #[test]
    fn separator_complement_preserves_legacy_family_and_target_deduplication() {
        let mut family = GeometrySolutionFamily::new();
        let left = [
            entry(&mut family, 0, [1, 0, 0, 0, 0, 0, 0]),
            entry(&mut family, 1, [0, 1, 0, 0, 0, 0, 0]),
            entry(&mut family, 2, [0, 0, 1, 0, 0, 0, 0]),
        ];
        let right = [
            entry(&mut family, 3, [0, 1, 0, 0, 0, 0, 0]),
            entry(&mut family, 4, [1, 0, 0, 0, 0, 0, 0]),
            entry(&mut family, 5, [0, 0, 1, 0, 0, 0, 0]),
        ];
        let used = [0, 0, 0, 1, 0, 0, 0];
        let targets = [
            target([1, 0, 1, 1, 0, 0, 0]),
            target([1, 1, 0, 1, 0, 0, 0]),
            target([0, 1, 1, 1, 0, 0, 0]),
            target([1, 1, 0, 1, 0, 0, 0]),
        ];
        let legacy = with_component_join_policy(ComponentJoinPolicy::Legacy, || {
            join_separator_tables(&left, &right, used, &targets, &mut family)
        });
        let legacy_nodes = family.node_count();
        let complement = with_component_join_policy(ComponentJoinPolicy::Complement, || {
            join_separator_tables(&left, &right, used, &targets, &mut family)
        });
        assert_eq!(complement, legacy);
        assert_eq!(family.node_count(), legacy_nodes);
        let SeparatorJoinResult::Ready(root) = complement else {
            panic!("fixture has six valid products");
        };
        assert_eq!(family.path_count(root), Some(6));
    }

    #[test]
    fn separator_join_preserves_path_and_family_storage_limits() {
        let mut family = GeometrySolutionFamily::new();
        let mut left = [entry(&mut family, 0, counts(1, 0))];
        let right = [entry(&mut family, 1, counts(0, 1))];
        let targets = [target(counts(1, 1))];
        left[0].path_count = SEPARATOR_JOIN_MAX_PATHS + 1;
        let original_nodes = family.node_count();
        for policy in [ComponentJoinPolicy::Legacy, ComponentJoinPolicy::Complement] {
            let result = with_component_join_policy(policy, || {
                join_separator_tables(&left, &right, [0; 7], &targets, &mut family)
            });
            assert_eq!(result, SeparatorJoinResult::PathLimit);
            assert_eq!(family.node_count(), original_nodes);
        }

        left[0].path_count = 1;
        // Fill the current family chunk so a new product requires storage.
        while family.node_count() < 4096 {
            family
                .append(100 + family.node_count(), FAMILY_EMPTY)
                .expect("fixture family");
        }
        family.set_retained_limit_bytes(Some(family.retained_bytes() as u128));
        let original_nodes = family.node_count();
        for policy in [ComponentJoinPolicy::Legacy, ComponentJoinPolicy::Complement] {
            let result = with_component_join_policy(policy, || {
                join_separator_tables(&left, &right, [0; 7], &targets, &mut family)
            });
            assert_eq!(result, SeparatorJoinResult::StorageUnavailable);
            assert_eq!(family.node_count(), original_nodes);
        }
    }
}
