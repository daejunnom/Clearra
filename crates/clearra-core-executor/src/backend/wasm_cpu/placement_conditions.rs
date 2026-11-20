// SRP rationale: compile target-frame placement and row-clear conditions once,
// before tiling, and specialize those conditions without a placement-subset graph.
use super::catalog::GeometryCatalog;
use crate::row_frame::CompactedRowFrame;

pub(super) const MAX_CONDITION_HEIGHT: u8 = 6;
// This is a bounded prototype. The board64, six-row tetromino catalogs are much
// smaller, but a future catalog extension must explicitly revisit this bound.
const MAX_CONDITION_ROWS: usize = 128 * 1024;

#[derive(Clone, Copy, Debug)]
struct CanonicalPlacement {
    cells: u64,
    occupied_rows: u16,
    domain_id: u32,
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

        Some(Self {
            width,
            height,
            initial_board: catalog.initial_board(),
            placements,
            clear_domains,
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
        let frame = CompactedRowFrame::new(self.height, deleted_rows)?;
        let mut source_mask = physical_mask;
        if deleted_rows != 0 {
            source_mask = 0;
            let width = u32::from(self.width);
            let row_bits = low_bits(width);
            for original_y in 0..self.height {
                if let Some(physical_y) = frame.physical_row_for_original(original_y) {
                    let cells = (physical_mask >> (u32::from(physical_y) * width)) & row_bits;
                    source_mask |= cells << (u32::from(original_y) * width);
                }
            }
        }

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
        let mut groups: Vec<RowConditionGroup> = Vec::new();
        groups.try_reserve_exact(usize::from(self.height)).ok()?;
        let row_bits = low_bits(u32::from(self.width));
        for (row, &contributors) in row_contributors
            .iter()
            .enumerate()
            .take(usize::from(self.height))
        {
            let shift = row * usize::from(self.width);
            if (final_board >> shift) & row_bits != row_bits {
                // Even executing all selected operations cannot clear this row.
                continue;
            }
            if contributors == 0 {
                // Match CandidateProjection's preclear-normalization contract.
                return None;
            }
            if let Some(group) = groups
                .iter_mut()
                .find(|group| group.contributors == contributors)
            {
                // With unique selected row IDs, equal execution masks mean
                // equal canonical-ID sets, not merely equal piece-kind sets.
                group.rows |= 1_u16 << row;
            } else {
                groups.push(RowConditionGroup {
                    contributors,
                    rows: 1_u16 << row,
                });
            }
        }
        Some(RowConditions {
            selected_rows,
            groups,
        })
    }

    /// Additional owned heap payload; the containing object's size is counted
    /// by its owner, as for GeometryCatalog::retained_bytes.
    pub(super) fn retained_bytes(&self) -> usize {
        self.placements.capacity() * core::mem::size_of::<CanonicalPlacement>()
            + self.clear_domains.capacity() * core::mem::size_of::<u64>()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RowConditionGroup {
    contributors: u16,
    rows: u16,
}

/// A small conjunction per completable target row, shared only where the
/// canonical contributor sets are identical. No subset table is allocated.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct RowConditions {
    selected_rows: Vec<u32>,
    groups: Vec<RowConditionGroup>,
}

impl RowConditions {
    pub(super) fn operation_count(&self) -> usize {
        self.selected_rows.len()
    }

    pub(super) fn selected_rows(&self) -> &[u32] {
        &self.selected_rows
    }

    pub(super) fn group_count(&self) -> usize {
        self.groups.len()
    }

    pub(super) fn atomic_group_count(&self) -> usize {
        self.groups
            .iter()
            .filter(|group| group.rows.count_ones() > 1)
            .count()
    }

    pub(super) fn completed_row_count(&self) -> u32 {
        self.groups
            .iter()
            .map(|group| group.rows.count_ones())
            .sum()
    }

    /// Evaluate row-completion predicates on executed selected operations.
    /// Bits outside the selected-operation domain are irrelevant to the formula.
    /// This says which target rows are full, not whether a legal execution order
    /// exists for that set of operations.
    pub(super) fn deleted_rows(&self, placed_mask: u16) -> u16 {
        self.groups.iter().fold(0, |deleted, group| {
            if group.contributors & !placed_mask == 0 {
                deleted | group.rows
            } else {
                deleted
            }
        })
    }

    /// Rows newly completed by an execution-set change. Equal contributor sets
    /// appear atomically, including when one placement completes several rows.
    #[cfg(test)]
    pub(super) fn clear_delta(&self, before: u16, after: u16) -> u16 {
        self.deleted_rows(after) & !self.deleted_rows(before)
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.selected_rows.capacity() * core::mem::size_of::<u32>()
            + self.groups.capacity() * core::mem::size_of::<RowConditionGroup>()
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
