use super::{
    catalog::GeometryCatalog, geometry::TargetGroup, geometry_apdp::partial_shape_kind, piece_index,
};

const BUMPER_DOMAIN_MAX_RESIDUAL_CELLS: u32 = 24;

const ALL_STANDARD_PIECES: u8 = 0x7f;

#[cfg(any(
    test,
    feature = "wasm-stage-profiling",
    feature = "minimum-physical-ab"
))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryApdpScanPolicy {
    Legacy,
    Off,
    Fused,
}

#[cfg(any(
    test,
    feature = "wasm-stage-profiling",
    feature = "minimum-physical-ab"
))]
std::thread_local! {
    static APDP_SCAN_POLICY: std::cell::Cell<GeometryApdpScanPolicy> =
        const { std::cell::Cell::new(GeometryApdpScanPolicy::Legacy) };
}

#[cfg(any(
    test,
    feature = "wasm-stage-profiling",
    feature = "minimum-physical-ab"
))]
pub fn set_geometry_apdp_scan_policy(policy: GeometryApdpScanPolicy) -> GeometryApdpScanPolicy {
    APDP_SCAN_POLICY.with(|current| current.replace(policy))
}

#[cfg(any(
    test,
    feature = "wasm-stage-profiling",
    feature = "minimum-physical-ab"
))]
pub fn geometry_apdp_scan_policy() -> GeometryApdpScanPolicy {
    APDP_SCAN_POLICY.with(std::cell::Cell::get)
}

#[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GeometryApdpScanCounters {
    pub advanced_domain_calls: u64,
    pub minimum_domain_calls: u64,
    pub eligible_pivot_calls: u64,
    pub legacy_queries: u64,
    pub off_queries: u64,
    pub fused_queries: u64,
    pub completeness_rows: u64,
    pub recount_rows: u64,
    pub incomplete_domains: u64,
}

#[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
std::thread_local! {
    static APDP_SCAN_DIAGNOSTICS_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static APDP_SCAN_COUNTERS: std::cell::Cell<GeometryApdpScanCounters> =
        std::cell::Cell::new(GeometryApdpScanCounters::default());
}

#[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
pub fn set_geometry_apdp_scan_diagnostics(enabled: bool) {
    APDP_SCAN_COUNTERS.with(|slot| slot.set(GeometryApdpScanCounters::default()));
    APDP_SCAN_DIAGNOSTICS_ENABLED.with(|slot| slot.set(enabled));
}

#[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
pub fn geometry_apdp_scan_counters() -> GeometryApdpScanCounters {
    APDP_SCAN_COUNTERS.with(std::cell::Cell::get)
}

#[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
fn record_apdp_scan(update: impl FnOnce(&mut GeometryApdpScanCounters)) {
    if !APDP_SCAN_DIAGNOSTICS_ENABLED.with(std::cell::Cell::get) {
        return;
    }
    APDP_SCAN_COUNTERS.with(|slot| {
        let mut counters = slot.get();
        update(&mut counters);
        slot.set(counters);
    });
}

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
        #[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
        record_apdp_scan(|value| {
            value.advanced_domain_calls = value.advanced_domain_calls.saturating_add(1);
        });
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
        {
            if let Some((count, piece_mask)) = selected_apdp_parent_domain(
                catalog,
                remaining,
                feasible_piece_mask,
                result.pivot_required_cells,
            ) {
                if count < result.pivot_support_count {
                    result.pivot_support_count = count;
                    result.pivot_piece_mask = piece_mask;
                    result.apdp_required_cells = result.pivot_required_cells;
                }
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
        #[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
        record_apdp_scan(|value| {
            value.minimum_domain_calls = value.minimum_domain_calls.saturating_add(1);
        });
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

fn selected_apdp_parent_domain(
    catalog: &GeometryCatalog,
    remaining: u64,
    feasible_piece_mask: u8,
    required: u64,
) -> Option<(usize, u8)> {
    #[cfg(any(
        test,
        feature = "wasm-stage-profiling",
        feature = "minimum-physical-ab"
    ))]
    {
        let policy = geometry_apdp_scan_policy();
        record_apdp_scan(|value| {
            value.eligible_pivot_calls = value.eligible_pivot_calls.saturating_add(1);
            match policy {
                GeometryApdpScanPolicy::Legacy => value.legacy_queries = value.legacy_queries.saturating_add(1),
                GeometryApdpScanPolicy::Off => value.off_queries = value.off_queries.saturating_add(1),
                GeometryApdpScanPolicy::Fused => value.fused_queries = value.fused_queries.saturating_add(1),
            }
        });
        match policy {
            GeometryApdpScanPolicy::Off => return None,
            GeometryApdpScanPolicy::Fused => {
                return complete_apdp_parent_domain(catalog, remaining, feasible_piece_mask, required);
            }
            GeometryApdpScanPolicy::Legacy => {}
        }
    }
    if !apdp_domain_is_complete(catalog, remaining, feasible_piece_mask, required) {
        return None;
    }
    #[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
    record_apdp_scan(|value| {
        value.recount_rows = value.recount_rows.saturating_add(
            catalog.support(required.trailing_zeros() as u8).len() as u64,
        );
    });
    Some(exact_parent_rows(
        catalog,
        remaining,
        feasible_piece_mask,
        required,
        true,
    ))
}

// Retain the existing two-pass path as the product default and the A/B control.
fn apdp_domain_is_complete(
    catalog: &GeometryCatalog,
    remaining: u64,
    feasible_piece_mask: u8,
    required: u64,
) -> bool {
    let first = required.trailing_zeros() as u8;
    let mut saw_parent = false;
    for (_scan_index, row_id) in catalog.support(first).iter().copied().enumerate() {
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
            #[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
            record_apdp_scan(|value| {
                value.completeness_rows = value.completeness_rows.saturating_add(_scan_index as u64 + 1);
                value.incomplete_domains = value.incomplete_domains.saturating_add(1);
            });
            return false;
        }
    }
    #[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
    record_apdp_scan(|value| {
        value.completeness_rows = value.completeness_rows.saturating_add(catalog.support(first).len() as u64);
        value.incomplete_domains = value.incomplete_domains.saturating_add(u64::from(!saw_parent));
    });
    saw_parent
}

#[cfg(any(
    test,
    feature = "wasm-stage-profiling",
    feature = "minimum-physical-ab"
))]
fn complete_apdp_parent_domain(
    catalog: &GeometryCatalog,
    remaining: u64,
    feasible_piece_mask: u8,
    required: u64,
) -> Option<(usize, u8)> {
    let first = required.trailing_zeros() as u8;
    let mut count = 0;
    let mut piece_mask = 0;
    for (scan_index, row_id) in catalog.support(first).iter().copied().enumerate() {
        let row = catalog.skeleton(row_id);
        if row.cells & required != required
            || !row_feasible(catalog, row_id, remaining, feasible_piece_mask)
        {
            continue;
        }
        if !catalog.apdp_row_is_static_exact(row_id)
            || !catalog.apdp_index().row_supports(row_id, required)
        {
            // Incomplete static support cannot remove a temporal parent or
            // authorize the partial result accumulated before this row.
            record_apdp_scan(|value| {
                value.completeness_rows = value.completeness_rows.saturating_add(scan_index as u64 + 1);
                value.incomplete_domains = value.incomplete_domains.saturating_add(1);
            });
            return None;
        }
        count += 1;
        piece_mask |= 1_u8 << piece_index(row.piece);
    }
    // The completeness check and parent aggregation share this one traversal.
    // Absence of any full parent is handled by the full-placement domain.
    record_apdp_scan(|value| {
        value.completeness_rows = value.completeness_rows.saturating_add(catalog.support(first).len() as u64);
        value.incomplete_domains = value.incomplete_domains.saturating_add(u64::from(count == 0));
    });
    (count != 0).then_some((count, piece_mask))
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
    use std::collections::BTreeSet;

    use clearra_core_domain::pc::pc_target::PcTarget;
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::OpeningPcSearchQuery;
    use clearra_problem::ProblemCompiler;

    use super::{
        ALL_STANDARD_PIECES, DomainPropagation, DomainStatus, GeometryApdpScanPolicy,
        GeometryCatalog, apdp_domain_is_complete, complete_apdp_parent_domain, exact_parent_rows,
        geometry_apdp_scan_policy, partial_shape_kind, row_feasible, selected_apdp_parent_domain,
        set_geometry_apdp_scan_policy,
    };

    struct RestorePolicy(GeometryApdpScanPolicy);

    impl Drop for RestorePolicy {
        fn drop(&mut self) {
            set_geometry_apdp_scan_policy(self.0);
        }
    }

    #[test]
    fn apdp_scan_policies_preserve_static_and_temporal_full_parent_domains() {
        let _restore = RestorePolicy(geometry_apdp_scan_policy());
        let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
            .with_objective(ObjectivePolicy::unique());
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("four-line problem");
        let catalog = GeometryCatalog::compile(&problem).expect("inverse lock-clear catalog");
        let remaining = catalog.required_cells();
        let mut partials = BTreeSet::new();
        for row_id in 0..catalog.skeleton_count() as u32 {
            let row = catalog.skeleton(row_id);
            let mut cells = row.cells;
            while cells != 0 {
                let bit = cells & cells.wrapping_neg();
                cells &= cells - 1;
                let partial = row.cells & !bit;
                if partial_shape_kind(catalog.width(), partial) != 0 {
                    partials.insert(partial);
                }
            }
        }
        let mut complete_domains = 0;
        let mut temporal_domains = 0;
        for required in partials {
            let baseline_complete =
                apdp_domain_is_complete(&catalog, remaining, ALL_STANDARD_PIECES, required);
            let baseline = baseline_complete.then(|| {
                exact_parent_rows(&catalog, remaining, ALL_STANDARD_PIECES, required, true)
            });
            assert_eq!(
                complete_apdp_parent_domain(&catalog, remaining, ALL_STANDARD_PIECES, required),
                baseline,
                "partial {required:#x}"
            );
            if let Some(parent_domain) = baseline {
                complete_domains += 1;
                assert_eq!(
                    parent_domain,
                    exact_parent_rows(&catalog, remaining, ALL_STANDARD_PIECES, required, false)
                );
            } else {
                let has_temporal_parent = catalog
                    .support(required.trailing_zeros() as u8)
                    .iter()
                    .copied()
                    .any(|row_id| {
                        catalog.skeleton(row_id).cells & required == required
                            && row_feasible(&catalog, row_id, remaining, ALL_STANDARD_PIECES)
                            && !catalog.apdp_row_is_static_exact(row_id)
                    });
                temporal_domains += usize::from(has_temporal_parent);
            }
            for (policy, expected) in [
                (GeometryApdpScanPolicy::Legacy, baseline),
                (GeometryApdpScanPolicy::Fused, baseline),
                (GeometryApdpScanPolicy::Off, None),
            ] {
                set_geometry_apdp_scan_policy(policy);
                assert_eq!(
                    selected_apdp_parent_domain(&catalog, remaining, ALL_STANDARD_PIECES, required),
                    expected
                );
            }
        }
        assert!(
            complete_domains > 0,
            "fixture must exercise complete static domains"
        );
        assert!(temporal_domains > 0, "fixture must retain temporal parents");

        // Compare the rows actually admitted by the full domain, including
        // bounded residual fields with forced three-cell owner groups.
        for residual in [
            remaining,
            0x407,
            0x100007,
            0x300c03,
            0xf,
            0x407 | 0x380e0000,
        ] {
            let mut baseline = None;
            for policy in [
                GeometryApdpScanPolicy::Legacy,
                GeometryApdpScanPolicy::Off,
                GeometryApdpScanPolicy::Fused,
            ] {
                set_geometry_apdp_scan_policy(policy);
                let compiled = DomainPropagation::compile(&catalog, residual, ALL_STANDARD_PIECES);
                let rows = if compiled.status == DomainStatus::Empty {
                    Vec::new()
                } else {
                    (0..catalog.skeleton_count() as u32)
                        .filter(|row_id| {
                            compiled.propagation.row_allowed(
                                &catalog,
                                *row_id,
                                residual,
                                ALL_STANDARD_PIECES,
                            )
                        })
                        .collect::<Vec<_>>()
                };
                let observed = (
                    compiled.status,
                    compiled.propagation.pivot_required_cells,
                    rows,
                );
                if let Some(expected) = &baseline {
                    assert_eq!(
                        &observed, expected,
                        "policy {policy:?}, residual {residual:#x}"
                    );
                } else {
                    baseline = Some(observed);
                }
            }
        }
    }
}
