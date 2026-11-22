// SRP rationale: this module's single change reason is compiling one immutable WASM Geometry catalog from a normalized search problem.
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_problem::SearchProblem;
#[cfg(any(test, feature = "local-search-ab"))]
use std::sync::{Arc, OnceLock};

#[cfg(any(test, feature = "local-search-ab"))]
use super::placement_conditions::{PlacementConditionCatalog, MAX_CONDITION_HEIGHT};
use super::{
    build_clear_rows::ClearRowDomain,
    geometry_apdp::ExactArmPairIndex,
    geometry_projection::ProjectionCatalog,
    geometry_separator::SeparatorCatalog,
    inverse_projection::{inverse_projection_policy, ProjectionAvailability, ProjectionRowFilter},
    mix_digest, piece_index, WasmExactSearchError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SkeletonRow {
    pub piece: PieceKind,
    pub cells: u64,
    pub realization_start: u32,
    pub realization_count: u16,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Realization {
    pub piece: PieceKind,
    pub cells: u64,
    pub required_deleted_rows: u16,
    pub rotation: RotationState,
    pub x: i8,
    pub target_anchor_y: i8,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct InstantiatedRealization {
    pub lock_mask: u64,
    pub rotation: RotationState,
    pub x: i8,
    pub lock_y: i8,
}

#[derive(Clone, Debug)]
pub(super) struct GeometryCatalog {
    #[cfg(any(test, feature = "local-search-ab"))]
    placement_conditions: Option<std::sync::Arc<PlacementConditionCatalog>>,
    width: u8,
    height: u8,
    initial_board: u64,
    required_cells: u64,
    skeletons: Vec<SkeletonRow>,
    realizations: Vec<Realization>,
    skeleton_occupied_rows: Vec<u16>,
    skeleton_requirement_domains: Option<Vec<u64>>,
    support_offsets: Vec<u32>,
    support_rows: Vec<u32>,
    apdp_index: ExactArmPairIndex,
    projection_catalog: ProjectionCatalog,
    separator_catalog: SeparatorCatalog,
    clear_state_count: usize,
    instantiation_offsets: Option<Vec<u32>>,
    instantiated_realizations: Vec<InstantiatedRealization>,
    #[cfg(any(test, feature = "local-search-ab"))]
    lazy_instantiations: Option<Arc<LazyInstantiationCache>>,
    identity_digest: u64,
}

pub(super) struct InstantiatedRealizationIter<'a> {
    catalog: &'a GeometryCatalog,
    target_cells: u64,
    deleted_rows: u16,
    precomputed: Option<core::slice::Iter<'a, InstantiatedRealization>>,
    raw: Option<core::slice::Iter<'a, Realization>>,
    #[cfg(any(test, feature = "local-search-ab"))]
    last_sorted: Option<Option<InstantiatedRealization>>,
}

impl Iterator for InstantiatedRealizationIter<'_> {
    type Item = InstantiatedRealization;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(precomputed) = self.precomputed.as_mut() {
            return precomputed.next().copied();
        }
        #[cfg(any(test, feature = "local-search-ab"))]
        if let Some(last) = self.last_sorted.as_mut() {
            // Fallback for a busy/occupied bounded memo slot. Raw domains
            // have at most LAZY_VALUES_PER_SLOT alternatives in this mode.
            // Repeatedly select the next distinct minimum without allocating.
            let next = self
                .raw
                .as_ref()?
                .as_slice()
                .iter()
                .filter_map(|&realization| {
                    self.catalog.instantiate_realization(
                        self.target_cells,
                        realization,
                        self.deleted_rows,
                    )
                })
                .filter(|current| last.is_none_or(|previous| previous < *current))
                .min()?;
            *last = Some(next);
            return Some(next);
        }
        let raw = self.raw.as_mut()?;
        for realization in raw {
            if let Some(instantiated) = self.catalog.instantiate_realization(
                self.target_cells,
                *realization,
                self.deleted_rows,
            ) {
                return Some(instantiated);
            }
        }
        None
    }
}

#[cfg(any(test, feature = "local-search-ab"))]
const LAZY_FRAME_SLOTS: usize = 512;
#[cfg(any(test, feature = "local-search-ab"))]
const LAZY_VALUES_PER_SLOT: usize = 8;

/// A request-owned, fixed-capacity exact memo. A published slot is immutable,
/// so callers can borrow its values without extending the catalog's lifetime
/// or copying a cache entry into every BuildUp worker.
#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Debug)]
struct LazyInstantiationCache {
    slots: Vec<OnceLock<LazyInstantiationSlot>>,
    dense_order: bool,
}

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Debug)]
struct LazyInstantiationSlot {
    row_id: u32,
    deleted_rows: u16,
    len: usize,
    values: [InstantiatedRealization; LAZY_VALUES_PER_SLOT],
}

#[cfg(any(test, feature = "local-search-ab"))]
impl LazyInstantiationCache {
    fn try_new(dense_order: bool, capacity: usize) -> Option<Self> {
        if capacity == 0 {
            return None;
        }
        let mut slots = Vec::new();
        slots.try_reserve_exact(capacity).ok()?;
        slots.resize_with(capacity, OnceLock::new);
        Some(Self { slots, dense_order })
    }

    fn supports_rows(skeletons: &[SkeletonRow]) -> bool {
        skeletons
            .iter()
            .all(|row| usize::from(row.realization_count) <= LAZY_VALUES_PER_SLOT)
    }

    fn get_or_compile(
        &self,
        row_id: u32,
        deleted_rows: u16,
        project: impl FnOnce() -> LazyInstantiationSlot,
    ) -> Option<&[InstantiatedRealization]> {
        // Small keyed set, not an equality proof. The complete row/frame key
        // must match after lookup; geometry authority is the cache owner.
        let mixed =
            row_id.wrapping_mul(0x9e37_79b1) ^ u32::from(deleted_rows).wrapping_mul(0x85eb_ca6b);
        let slot = self.slots.get((mixed as usize) % self.slots.len())?;
        let slot = slot.get_or_init(project);
        if slot.row_id != row_id || slot.deleted_rows != deleted_rows {
            return None;
        }
        Some(&slot.values[..slot.len])
    }

    fn compile_slot(
        &self,
        catalog: &GeometryCatalog,
        row_id: u32,
        deleted_rows: u16,
    ) -> LazyInstantiationSlot {
        const EMPTY: InstantiatedRealization = InstantiatedRealization {
            lock_mask: 0,
            rotation: RotationState::ALL[0],
            x: 0,
            lock_y: 0,
        };
        let mut values = [EMPTY; LAZY_VALUES_PER_SLOT];
        let mut len = 0;
        for value in catalog.unfiltered_instantiations(row_id, deleted_rows) {
            // Admission checked the raw realization count, an upper bound on
            // the number of results that projection can yield.
            values[len] = value;
            len += 1;
        }
        if self.dense_order {
            len = sort_unique_realizations(&mut values, 0, len);
        }
        LazyInstantiationSlot {
            row_id,
            deleted_rows,
            len,
            values,
        }
    }

    fn compile_payload_bound() -> u128 {
        (core::mem::size_of::<Self>()
            + 2 * core::mem::size_of::<usize>()
            + LAZY_FRAME_SLOTS * core::mem::size_of::<OnceLock<LazyInstantiationSlot>>())
            as u128
    }

    fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + 2 * core::mem::size_of::<usize>()
            + self.slots.capacity() * core::mem::size_of::<OnceLock<LazyInstantiationSlot>>()
    }
}

#[cfg(any(test, feature = "local-search-ab"))]
fn lazy_frame_enabled() -> bool {
    #[cfg(all(not(test), feature = "local-search-ab"))]
    {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED
            .get_or_init(|| std::env::var("CLEARRA_ILC_LAZY_FRAME").is_ok_and(|value| value == "1"))
    }
    #[cfg(any(test, not(feature = "local-search-ab")))]
    {
        false
    }
}

impl GeometryCatalog {
    /// Allocation-free conservative peak for compiling the complete Board64
    /// geometry catalog. The bound keeps raw and ordered realization storage
    /// live together and uses pre-dedup row counts for every downstream index;
    /// it is deliberately an upper bound rather than an exact allocation
    /// claim.
    pub fn checked_compile_peak_upper_bound(problem: &SearchProblem) -> Option<u128> {
        let width = u8::try_from(problem.initial_board().width()).ok()?;
        let height = u8::try_from(problem.compact_pc_geometry_height()).ok()?;
        let cell_count = usize::from(width).checked_mul(usize::from(height))?;
        if width == 0 || height == 0 || cell_count > u64::BITS as usize || height > 16 {
            return None;
        }
        let realization_count = checked_realization_count_upper_bound(width, height)?;
        let row_count = realization_count;
        let row_count_usize = usize::try_from(row_count).ok()?;
        #[cfg(any(test, feature = "local-search-ab"))]
        let prototype_peak =
            if height <= MAX_CONDITION_HEIGHT && super::placement_dependency::enabled() {
                let conditions =
                    PlacementConditionCatalog::checked_compile_peak_upper_bound(row_count_usize)?;
                let guards = if super::placement_conditions::precomputed_order_guards_enabled() {
                    PlacementConditionCatalog::order_guard_compile_heap_bound(
                        row_count_usize,
                        width,
                        height,
                    )
                } else {
                    0
                };
                conditions.checked_add(guards)?
            } else {
                0
            };
        #[cfg(not(any(test, feature = "local-search-ab")))]
        let prototype_peak = 0;
        #[cfg(any(test, feature = "local-search-ab"))]
        let lazy_peak = if height <= MAX_CONDITION_HEIGHT && lazy_frame_enabled() {
            LazyInstantiationCache::compile_payload_bound()
        } else {
            0
        };
        #[cfg(not(any(test, feature = "local-search-ab")))]
        let lazy_peak = 0;
        let clear_state_count = 1_u128.checked_shl(u32::from(height))?;

        let raw_and_ordered_realizations = realization_count
            .checked_mul(2)?
            .checked_mul(core::mem::size_of::<Realization>() as u128)?;
        let skeleton_bytes = row_count.checked_mul(core::mem::size_of::<SkeletonRow>() as u128)?;
        let temporal_bytes = row_count
            .checked_mul(core::mem::size_of::<u16>() as u128)?
            .checked_add(if height <= 6 {
                row_count.checked_mul(core::mem::size_of::<u64>() as u128)?
            } else {
                0
            })?;
        let support_bytes = ((cell_count as u128).checked_add(1)?)
            .checked_mul(core::mem::size_of::<u32>() as u128)?
            .checked_add(
                row_count
                    .checked_mul(4)?
                    .checked_mul(core::mem::size_of::<u32>() as u128)?,
            )?;
        let apdp_bytes = ExactArmPairIndex::checked_compile_peak_upper_bound(row_count_usize)?;
        let projection_bytes =
            ProjectionCatalog::checked_compile_peak_upper_bound(row_count_usize, width)?;
        let separator_bytes = u128::from(width)
            .checked_mul(3)?
            .checked_mul(core::mem::size_of::<u64>() as u128)?;

        let instantiation_value_count = realization_count.checked_mul(clear_state_count)?;
        let (instantiation_offset_bytes, instantiation_value_bytes) =
            if instantiation_value_count <= MAX_INSTANTIATION_TABLE_VALUES as u128 {
                (
                    row_count
                        .checked_mul(clear_state_count)?
                        .checked_add(1)?
                        .checked_mul(core::mem::size_of::<u32>() as u128)?,
                    instantiation_value_count
                        .checked_mul(core::mem::size_of::<InstantiatedRealization>() as u128)?,
                )
            } else {
                (0, 0)
            };

        raw_and_ordered_realizations
            .checked_add(prototype_peak)?
            .checked_add(lazy_peak)?
            .checked_add(skeleton_bytes)?
            .checked_add(temporal_bytes)?
            .checked_add(support_bytes)?
            .checked_add(apdp_bytes)?
            .checked_add(projection_bytes)?
            .checked_add(separator_bytes)?
            .checked_add(instantiation_offset_bytes)?
            .checked_add(instantiation_value_bytes)
    }

    pub fn compile(problem: &SearchProblem) -> Result<Self, WasmExactSearchError> {
        let width = u8::try_from(problem.initial_board().width())
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_board_width_overflow"))?;
        // Geometry is compiled in the target frame. The larger spawn/search
        // height belongs to reachability and must not expand the exact-cover
        // universe beyond the active PC rows.
        let height = u8::try_from(problem.compact_pc_geometry_height()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_target_frame_height_overflow")
        })?;
        let cell_count = usize::from(width) * usize::from(height);
        if width == 0 || height == 0 || cell_count > u64::BITS as usize || height > 16 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_geometry_requires_board64_target_height_at_most_16",
            ));
        }
        let all_cells = if cell_count == u64::BITS as usize {
            u64::MAX
        } else {
            (1_u64 << cell_count) - 1
        };
        let initial_board = problem.initial_board().occupied_mask();
        if initial_board & !all_cells != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_initial_board_outside_search_region",
            ));
        }
        let required_cells = all_cells & !initial_board;
        let proven_empty = problem.compact_pc_proven_empty_family_depth().is_some();
        if !required_cells.count_ones().is_multiple_of(4) && !proven_empty {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_required_fill_area_not_tetromino_aligned",
            ));
        }

        Self::compile_with_clear_row_domain(
            width,
            height,
            initial_board,
            required_cells,
            false,
            proven_empty,
        )
    }

    #[cfg(test)]
    pub fn compile_for_required_cells_on_board(
        problem: &SearchProblem,
        initial_board: u64,
        required_cells: u64,
    ) -> Result<Self, WasmExactSearchError> {
        let width = u8::try_from(problem.initial_board().width())
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_board_width_overflow"))?;
        let height = u8::try_from(problem.visible_height()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_target_frame_height_overflow")
        })?;
        Self::compile_for_required_cells_on_dimensions(width, height, initial_board, required_cells)
    }

    pub fn compile_for_build_probability_on_board(
        problem: &SearchProblem,
        initial_board: u64,
        required_cells: u64,
    ) -> Result<Self, WasmExactSearchError> {
        let width = u8::try_from(problem.initial_board().width())
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_board_width_overflow"))?;
        let height = u8::try_from(problem.visible_height()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_target_frame_height_overflow")
        })?;
        Self::compile_with_clear_row_domain(
            width,
            height,
            initial_board,
            required_cells,
            true,
            false,
        )
    }

    /// Compile the same inverse lock-clear catalog for an already qualified
    /// compact target frame. This keeps PC4 graph-edge materialization on the
    /// exact ILC implementation without manufacturing a product search
    /// request or giving a graph record any search authority beyond its
    /// source/target cells.
    pub(super) fn compile_for_required_cells_on_dimensions(
        width: u8,
        height: u8,
        initial_board: u64,
        required_cells: u64,
    ) -> Result<Self, WasmExactSearchError> {
        Self::compile_with_clear_row_domain(
            width,
            height,
            initial_board,
            required_cells,
            false,
            false,
        )
    }

    pub(super) fn compile_build_stage(
        height: u8,
        base: u64,
        target: u64,
    ) -> Result<Self, WasmExactSearchError> {
        Self::compile_with_clear_row_domain(10, height, base, target, true, false)
    }

    fn compile_with_clear_row_domain(
        width: u8,
        height: u8,
        initial_board: u64,
        required_cells: u64,
        build_target: bool,
        legacy_proven_empty: bool,
    ) -> Result<Self, WasmExactSearchError> {
        let cell_count = usize::from(width) * usize::from(height);
        if width == 0 || height == 0 || cell_count > u64::BITS as usize || height > 16 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_geometry_requires_board64_target_height_at_most_16",
            ));
        }
        let all_cells = if cell_count == u64::BITS as usize {
            u64::MAX
        } else {
            (1_u64 << cell_count) - 1
        };
        if initial_board & !all_cells != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_initial_board_outside_search_region",
            ));
        }
        if required_cells & !all_cells != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_target_outside_search_region",
            ));
        }
        if required_cells & initial_board != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_target_overlaps_initial_board",
            ));
        }
        if !required_cells.count_ones().is_multiple_of(4) && !legacy_proven_empty {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_required_fill_area_not_tetromino_aligned",
            ));
        }

        let clear_rows = if build_target {
            let full_row = if width == 64 {
                u64::MAX
            } else {
                (1_u64 << width) - 1
            };
            ClearRowDomain::for_completed_target(width, height, |row| {
                ((initial_board | required_cells) >> (u32::from(row) * u32::from(width))) & full_row
            })
        } else {
            ClearRowDomain::unrestricted()
        };
        let realization_capacity = checked_realization_capacity(width, height).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_geometry_catalog_projection_overflow"),
        )?;
        let realizations = compile_inverse_clear_realizations(
            width,
            height,
            initial_board,
            required_cells,
            clear_rows,
            realization_capacity,
        )?;

        let mut skeletons = Vec::new();
        skeletons
            .try_reserve_exact(realizations.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_geometry_catalog_skeleton_storage_unavailable",
                )
            })?;
        let mut ordered_realizations = Vec::new();
        ordered_realizations
            .try_reserve_exact(realizations.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_geometry_catalog_realization_storage_unavailable",
                )
            })?;
        let mut cursor = 0;
        while cursor < realizations.len() {
            let piece = realizations[cursor].piece;
            let cells = realizations[cursor].cells;
            let start = ordered_realizations.len();
            while cursor < realizations.len()
                && realizations[cursor].piece == piece
                && realizations[cursor].cells == cells
            {
                ordered_realizations.push(realizations[cursor]);
                cursor += 1;
            }
            let count = ordered_realizations.len() - start;
            skeletons.push(SkeletonRow {
                piece,
                cells,
                realization_start: start as u32,
                realization_count: count as u16,
            });
        }
        drop(realizations);

        let mut support_offsets = Vec::new();
        support_offsets
            .try_reserve_exact(cell_count.checked_add(1).ok_or(
                WasmExactSearchError::InvalidProblem(
                    "wasm_geometry_catalog_support_projection_overflow",
                ),
            )?)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_geometry_catalog_support_storage_unavailable",
                )
            })?;
        let mut support_rows = Vec::new();
        support_rows
            .try_reserve_exact(skeletons.len().checked_mul(4).ok_or(
                WasmExactSearchError::InvalidProblem(
                    "wasm_geometry_catalog_support_projection_overflow",
                ),
            )?)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_geometry_catalog_support_storage_unavailable",
                )
            })?;
        support_offsets.push(0);
        for cell in 0..cell_count {
            let bit = 1_u64 << cell;
            for (row_id, row) in skeletons.iter().enumerate() {
                if row.cells & bit != 0 {
                    support_rows.push(row_id as u32);
                }
            }
            support_offsets.push(support_rows.len() as u32);
        }

        let apdp_index = ExactArmPairIndex::compile(width, &skeletons).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_apdp_catalog_storage_unavailable"),
        )?;
        let projection_catalog = ProjectionCatalog::compile(width, height, &skeletons).ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_projection_catalog_storage_unavailable_or_unsupported",
            ),
        )?;
        let separator_catalog = SeparatorCatalog::compile(width, height, &skeletons).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_separator_catalog_storage_unavailable"),
        )?;
        let (skeleton_occupied_rows, skeleton_requirement_domains) =
            compile_skeleton_temporal_metadata(width, height, &skeletons, &ordered_realizations)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_geometry_temporal_metadata_storage_unavailable",
                ))?;

        #[cfg(any(test, feature = "local-search-ab"))]
        let lazy_instantiations = if height <= MAX_CONDITION_HEIGHT
            && lazy_frame_enabled()
            && LazyInstantiationCache::supports_rows(&skeletons)
        {
            let dense_order = ordered_realizations
                .len()
                .checked_mul(1_usize << height)
                .is_some_and(|values| values <= MAX_INSTANTIATION_TABLE_VALUES);
            LazyInstantiationCache::try_new(dense_order, LAZY_FRAME_SLOTS).map(Arc::new)
        } else {
            None
        };
        #[cfg(any(test, feature = "local-search-ab"))]
        let eager_instantiation = lazy_instantiations.is_none();
        #[cfg(not(any(test, feature = "local-search-ab")))]
        let eager_instantiation = true;
        let (clear_state_count, instantiation_offsets, instantiated_realizations) =
            if eager_instantiation {
                compile_instantiation_table(width, height, &skeletons, &ordered_realizations)
            } else {
                (1_usize << height, None, Vec::new())
            };

        let mut identity_digest = mix_digest(0, u64::from(width));
        identity_digest = mix_digest(identity_digest, u64::from(height));
        identity_digest = mix_digest(identity_digest, initial_board);
        for realization in &ordered_realizations {
            identity_digest = mix_digest(identity_digest, piece_index(realization.piece) as u64);
            identity_digest = mix_digest(identity_digest, realization.cells);
            identity_digest = mix_digest(
                identity_digest,
                u64::from(realization.required_deleted_rows),
            );
            identity_digest = mix_digest(
                identity_digest,
                u64::from(realization.rotation.quarter_turns()),
            );
            identity_digest = mix_digest(identity_digest, realization.x as u8 as u64);
            identity_digest = mix_digest(identity_digest, realization.target_anchor_y as u8 as u64);
        }
        identity_digest = mix_digest(identity_digest, apdp_index.identity_digest());
        identity_digest = mix_digest(identity_digest, projection_catalog.identity_digest());
        identity_digest = mix_digest(identity_digest, separator_catalog.identity_digest());

        let catalog = Self {
            #[cfg(any(test, feature = "local-search-ab"))]
            placement_conditions: None,
            width,
            height,
            initial_board,
            required_cells,
            skeletons,
            realizations: ordered_realizations,
            skeleton_occupied_rows,
            skeleton_requirement_domains,
            support_offsets,
            support_rows,
            apdp_index,
            projection_catalog,
            separator_catalog,
            clear_state_count,
            instantiation_offsets,
            instantiated_realizations,
            #[cfg(any(test, feature = "local-search-ab"))]
            lazy_instantiations,
            identity_digest,
        };
        #[cfg(any(test, feature = "local-search-ab"))]
        let catalog = {
            let mut catalog = catalog;
            if super::placement_dependency::enabled() {
                catalog.placement_conditions =
                    PlacementConditionCatalog::compile(&catalog).map(|conditions| {
                        super::placement_dependency::record(|m| {
                            m.catalogs_compiled += 1;
                            m.catalog_placements += conditions.row_count() as u64;
                            m.unique_clear_domains += conditions.domain_count() as u64;
                            m.shared_clear_domains += conditions
                                .row_count()
                                .saturating_sub(conditions.domain_count())
                                as u64;
                        });
                        std::sync::Arc::new(conditions)
                    });
            }
            catalog
        };
        Ok(catalog)
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    pub fn placement_conditions(&self) -> Option<&std::sync::Arc<PlacementConditionCatalog>> {
        self.placement_conditions.as_ref()
    }

    pub const fn width(&self) -> u8 {
        self.width
    }

    pub const fn height(&self) -> u8 {
        self.height
    }

    pub const fn initial_board(&self) -> u64 {
        self.initial_board
    }

    pub const fn required_cells(&self) -> u64 {
        self.required_cells
    }

    pub fn skeleton(&self, row_id: u32) -> SkeletonRow {
        self.skeletons[row_id as usize]
    }

    pub fn try_skeleton(&self, row_id: u32) -> Option<SkeletonRow> {
        self.skeletons.get(row_id as usize).copied()
    }

    pub fn skeleton_id(&self, piece: PieceKind, cells: u64) -> Option<u32> {
        self.skeletons
            .binary_search_by_key(&(piece, cells), |row| (row.piece, row.cells))
            .ok()
            .and_then(|index| u32::try_from(index).ok())
    }

    pub fn realizations(&self, row_id: u32) -> &[Realization] {
        let row = self.skeleton(row_id);
        let start = row.realization_start as usize;
        &self.realizations[start..start + row.realization_count as usize]
    }

    pub fn skeleton_occupied_rows(&self, row_id: u32) -> u16 {
        self.skeleton_occupied_rows[row_id as usize]
    }

    pub fn realization_requirement_is_satisfied(&self, row_id: u32, deleted_rows: u16) -> bool {
        if let Some(domains) = &self.skeleton_requirement_domains {
            return domains[row_id as usize] & (1_u64 << deleted_rows) != 0;
        }
        self.realizations(row_id)
            .iter()
            .any(|realization| realization.required_deleted_rows & !deleted_rows == 0)
    }

    pub fn instantiated_realizations(
        &self,
        row_id: u32,
        deleted_rows: u16,
    ) -> Option<&[InstantiatedRealization]> {
        let offsets = self.instantiation_offsets.as_ref()?;
        let deleted_index = usize::from(deleted_rows);
        if deleted_index >= self.clear_state_count {
            return Some(&[]);
        }
        let index = row_id as usize * self.clear_state_count + deleted_index;
        let start = offsets[index] as usize;
        let end = offsets[index + 1] as usize;
        Some(&self.instantiated_realizations[start..end])
    }

    pub fn instantiate_realization(
        &self,
        target_cells: u64,
        realization: Realization,
        deleted_rows: u16,
    ) -> Option<InstantiatedRealization> {
        instantiate_realization_raw(
            self.width,
            self.height,
            target_cells,
            realization,
            deleted_rows,
        )
    }

    /// Exact existential domain over the deleted-original-row masks supported
    /// by a Board64 condition catalog. Table offsets already encode the
    /// existential result, including exclusions that the requirement domain
    /// cannot prove. If table allocation was declined, use the same raw
    /// projection as enumeration, without consulting the condition being built.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub(super) fn exact_clear_domain_mask(&self, row_id: u32) -> Option<u64> {
        self.try_skeleton(row_id)?;
        let state_count = self.clear_state_count;
        if state_count > u64::BITS as usize {
            return None;
        }
        if let Some(offsets) = &self.instantiation_offsets {
            let start = row_id as usize * state_count;
            return Some(
                offsets[start..=start + state_count]
                    .windows(2)
                    .enumerate()
                    .fold(0_u64, |mask, (deleted, pair)| {
                        mask | (u64::from(pair[0] != pair[1]) << deleted)
                    }),
            );
        }

        let occupied = self.skeleton_occupied_rows(row_id);
        let mut possible = self.skeleton_requirement_domains.as_ref()?[row_id as usize];
        let mut exact = 0_u64;
        while possible != 0 {
            let deleted = possible.trailing_zeros() as u16;
            let deleted_bit = 1_u64 << deleted;
            possible &= possible - 1;
            if deleted & occupied == 0
                && self
                    .unfiltered_instantiations(row_id, deleted)
                    .next()
                    .is_some()
            {
                exact |= deleted_bit;
            }
        }
        Some(exact)
    }

    /// Check the concrete lock identity without warming a lazy runtime frame
    /// cache or using the optional dependency table under construction.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub(super) fn all_instantiations_match_lock(
        &self,
        row_id: u32,
        deleted_rows: u16,
        lock_mask: u64,
    ) -> bool {
        let mut instantiations = self.unfiltered_instantiations(row_id, deleted_rows);
        instantiations
            .next()
            .is_some_and(|first| first.lock_mask == lock_mask)
            && instantiations.all(|other| other.lock_mask == lock_mask)
    }

    pub fn instantiations(
        &self,
        row_id: u32,
        deleted_rows: u16,
    ) -> InstantiatedRealizationIter<'_> {
        #[cfg(any(test, feature = "local-search-ab"))]
        // A precomputed slice already answers this query exactly. The
        // condition avoids raw projections only when that slice is absent.
        if self.instantiation_offsets.is_none()
            && super::placement_dependency::enabled()
            && self
                .placement_conditions
                .as_ref()
                .is_some_and(|conditions| {
                    conditions.supports_deleted_rows(row_id, deleted_rows) == Some(false)
                })
        {
            return InstantiatedRealizationIter {
                catalog: self,
                target_cells: self.skeleton(row_id).cells,
                deleted_rows,
                precomputed: None,
                raw: None,
                last_sorted: None,
            };
        }
        #[cfg(any(test, feature = "local-search-ab"))]
        if let Some(cache) = &self.lazy_instantiations {
            if usize::from(deleted_rows) < self.clear_state_count {
                let mut iter = self.unfiltered_instantiations(row_id, deleted_rows);
                if let Some(values) = cache.get_or_compile(row_id, deleted_rows, || {
                    cache.compile_slot(self, row_id, deleted_rows)
                }) {
                    iter.precomputed = Some(values.iter());
                    iter.raw = None;
                } else if cache.dense_order {
                    iter.last_sorted = Some(None);
                }
                return iter;
            }
        }
        self.unfiltered_instantiations(row_id, deleted_rows)
    }

    fn unfiltered_instantiations(
        &self,
        row_id: u32,
        deleted_rows: u16,
    ) -> InstantiatedRealizationIter<'_> {
        if let Some(precomputed) = self.instantiated_realizations(row_id, deleted_rows) {
            InstantiatedRealizationIter {
                catalog: self,
                target_cells: self.skeleton(row_id).cells,
                deleted_rows,
                precomputed: Some(precomputed.iter()),
                raw: None,
                #[cfg(any(test, feature = "local-search-ab"))]
                last_sorted: None,
            }
        } else {
            InstantiatedRealizationIter {
                catalog: self,
                target_cells: self.skeleton(row_id).cells,
                deleted_rows,
                precomputed: None,
                raw: Some(self.realizations(row_id).iter()),
                #[cfg(any(test, feature = "local-search-ab"))]
                last_sorted: None,
            }
        }
    }

    pub fn support(&self, cell: u8) -> &[u32] {
        let start = self.support_offsets[cell as usize] as usize;
        let end = self.support_offsets[cell as usize + 1] as usize;
        &self.support_rows[start..end]
    }

    pub fn apdp_index(&self) -> &ExactArmPairIndex {
        &self.apdp_index
    }

    pub fn apdp_row_is_static_exact(&self, row_id: u32) -> bool {
        self.apdp_index.row_support_flags(row_id) != 0
            && self
                .realizations(row_id)
                .iter()
                .all(|realization| realization.required_deleted_rows == 0)
    }

    pub fn projection_catalog(&self) -> &ProjectionCatalog {
        &self.projection_catalog
    }

    pub fn separator_catalog(&self) -> &SeparatorCatalog {
        &self.separator_catalog
    }

    pub const fn identity_digest(&self) -> u64 {
        self.identity_digest
    }

    pub fn skeleton_count(&self) -> usize {
        self.skeletons.len()
    }

    pub fn realization_count(&self) -> usize {
        self.realizations.len()
    }

    pub fn instantiated_realization_count(&self) -> usize {
        self.instantiated_realizations.len()
    }

    pub fn has_instantiation_table(&self) -> bool {
        self.instantiation_offsets.is_some()
    }

    pub fn retained_bytes(&self) -> usize {
        #[cfg(any(test, feature = "local-search-ab"))]
        let prototype_bytes = self.placement_conditions.as_ref().map_or(0, |conditions| {
            conditions.retained_bytes()
                + core::mem::size_of::<super::placement_conditions::PlacementConditionCatalog>()
                + 2 * core::mem::size_of::<usize>()
        });
        #[cfg(not(any(test, feature = "local-search-ab")))]
        let prototype_bytes = 0;
        #[cfg(any(test, feature = "local-search-ab"))]
        let lazy_bytes = self
            .lazy_instantiations
            .as_ref()
            .map_or(0, |cache| cache.retained_bytes());
        #[cfg(not(any(test, feature = "local-search-ab")))]
        let lazy_bytes = 0;
        self.skeletons.capacity() * core::mem::size_of::<SkeletonRow>()
            + prototype_bytes
            + lazy_bytes
            + self.realizations.capacity() * core::mem::size_of::<Realization>()
            + self.skeleton_occupied_rows.capacity() * core::mem::size_of::<u16>()
            + self
                .skeleton_requirement_domains
                .as_ref()
                .map_or(0, |domains| {
                    domains.capacity() * core::mem::size_of::<u64>()
                })
            + self.support_offsets.capacity() * core::mem::size_of::<u32>()
            + self.support_rows.capacity() * core::mem::size_of::<u32>()
            + self.apdp_index.retained_bytes()
            + self.projection_catalog.retained_bytes()
            + self.separator_catalog.retained_bytes()
            + self.instantiation_offsets.as_ref().map_or(0, |offsets| {
                offsets.capacity() * core::mem::size_of::<u32>()
            })
            + self.instantiated_realizations.capacity()
                * core::mem::size_of::<InstantiatedRealization>()
    }
}

/// Callers validate Board64 dimensions and pass the complete disjoint set of
/// cells that the projected pieces may own. Standard PC and generic packing
/// share these exact realizations, not PC-specific projection/pruning indexes.
pub(super) fn compile_inverse_clear_realizations(
    width: u8,
    height: u8,
    initial_board: u64,
    required_cells: u64,
    clear_rows: ClearRowDomain,
    realization_capacity: usize,
) -> Result<Vec<Realization>, WasmExactSearchError> {
    let registry = standard_tetromino_registry();
    let inverse_policy = inverse_projection_policy();
    let row_mask = u64::MAX >> (u64::BITS - u32::from(width));
    let availability = ProjectionAvailability::compile(inverse_policy, width, height, |row| {
        (required_cells >> (u32::from(row) * u32::from(width))) & row_mask
    });
    let mut realizations = Vec::new();
    realizations
        .try_reserve_exact(realization_capacity)
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_geometry_catalog_realization_storage_unavailable",
            )
        })?;
    for piece in PieceKind::STANDARD_TETROMINOES {
        let definition = registry
            .get(piece)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "standard_piece_missing",
            ))?;
        for rotation in RotationState::ALL {
            let shape = definition.shape(rotation);
            let max_x = i16::from(width) - i16::from(shape.width());
            if max_x < 0 {
                continue;
            }
            let (local_rows, row_count) = occupied_shape_rows(shape.cells());
            let local_rows = &local_rows[..row_count];
            for x in 0..=max_x {
                let row_filter = availability.as_ref().and_then(|available| {
                    ProjectionRowFilter::compile_from_availability(
                        available,
                        shape.cells(),
                        local_rows,
                        x as i8,
                    )
                });
                if row_filter
                    .as_ref()
                    .is_some_and(|filter| !filter.columns_possible())
                {
                    continue;
                }
                enumerate_row_projections(
                    width,
                    height,
                    initial_board,
                    required_cells,
                    piece,
                    rotation,
                    shape.cells(),
                    local_rows,
                    &mut [0; 4],
                    0,
                    x as i8,
                    row_filter.as_ref(),
                    clear_rows,
                    &mut realizations,
                );
            }
        }
    }
    realizations.sort_unstable();
    realizations.dedup();
    Ok(realizations)
}

fn occupied_shape_rows(
    cells: [clearra_piece_registry::registry::piece_registry::ShapeCell; 4],
) -> ([u8; 4], usize) {
    let mut rows = cells.map(|cell| cell.y() as u8);
    rows.sort_unstable();
    let mut count = 0;
    for row in rows {
        if count == 0 || rows[count - 1] != row {
            rows[count] = row;
            count += 1;
        }
    }
    (rows, count)
}

pub(super) fn checked_realization_capacity(width: u8, height: u8) -> Option<usize> {
    usize::try_from(checked_realization_count_upper_bound(width, height)?).ok()
}

fn checked_realization_count_upper_bound(width: u8, height: u8) -> Option<u128> {
    let registry = standard_tetromino_registry();
    let mut total = 0_u128;
    for piece in PieceKind::STANDARD_TETROMINOES {
        let definition = registry.get(piece)?;
        for rotation in RotationState::ALL {
            let shape = definition.shape(rotation);
            let max_x = i16::from(width).checked_sub(i16::from(shape.width()))?;
            if max_x < 0 {
                continue;
            }
            let (local_rows, local_row_count) = occupied_shape_rows(shape.cells());
            let row_projection_count = checked_row_projection_count(
                &local_rows[..local_row_count],
                height,
                &mut [0_u8; 4],
                0,
            )?;
            total = total.checked_add(
                row_projection_count.checked_mul(u128::try_from(max_x.checked_add(1)?).ok()?)?,
            )?;
        }
    }
    Some(total)
}

fn checked_row_projection_count(
    local_rows: &[u8],
    height: u8,
    target_rows: &mut [u8; 4],
    row_index: usize,
) -> Option<u128> {
    if row_index == local_rows.len() {
        return Some(1);
    }
    let local_row = *local_rows.get(row_index)?;
    let local_last = *local_rows.last()?;
    let minimum = if row_index == 0 {
        local_row
    } else {
        target_rows[row_index - 1].checked_add(local_row.checked_sub(local_rows[row_index - 1])?)?
    };
    let remaining_span = local_last.checked_sub(local_row)?;
    if remaining_span >= height {
        return Some(0);
    }
    let maximum = height.checked_sub(1)?.checked_sub(remaining_span)?;
    if minimum > maximum {
        return Some(0);
    }
    let mut count = 0_u128;
    for target_row in minimum..=maximum {
        target_rows[row_index] = target_row;
        count = count.checked_add(checked_row_projection_count(
            local_rows,
            height,
            target_rows,
            row_index + 1,
        )?)?;
    }
    Some(count)
}

const MAX_INSTANTIATION_TABLE_VALUES: usize = 4 * 1024 * 1024;

fn compile_skeleton_temporal_metadata(
    width: u8,
    height: u8,
    skeletons: &[SkeletonRow],
    realizations: &[Realization],
) -> Option<(Vec<u16>, Option<Vec<u64>>)> {
    let mut occupied_rows = Vec::new();
    occupied_rows.try_reserve_exact(skeletons.len()).ok()?;
    let mut requirement_domains = if height <= 6 {
        let mut domains = Vec::new();
        domains.try_reserve_exact(skeletons.len()).ok()?;
        Some(domains)
    } else {
        None
    };
    for skeleton in skeletons {
        let mut row_mask = 0_u16;
        let mut cells = skeleton.cells;
        while cells != 0 {
            let cell = cells.trailing_zeros() as usize;
            cells &= cells - 1;
            row_mask |= 1_u16 << (cell / usize::from(width));
        }
        occupied_rows.push(row_mask);

        if let Some(domains) = requirement_domains.as_mut() {
            let start = skeleton.realization_start as usize;
            let end = start + skeleton.realization_count as usize;
            let mut domain = 0_u64;
            for deleted_rows in 0..(1_u16 << height) {
                if realizations[start..end]
                    .iter()
                    .any(|realization| realization.required_deleted_rows & !deleted_rows == 0)
                {
                    domain |= 1_u64 << deleted_rows;
                }
            }
            domains.push(domain);
        }
    }
    Some((occupied_rows, requirement_domains))
}

fn compile_instantiation_table(
    width: u8,
    height: u8,
    skeletons: &[SkeletonRow],
    realizations: &[Realization],
) -> (usize, Option<Vec<u32>>, Vec<InstantiatedRealization>) {
    let clear_state_count = 1_usize << height;
    let Some(offset_count) = skeletons
        .len()
        .checked_mul(clear_state_count)
        .and_then(|count| count.checked_add(1))
    else {
        return (clear_state_count, None, Vec::new());
    };
    let Some(value_upper_bound) = realizations.len().checked_mul(clear_state_count) else {
        return (clear_state_count, None, Vec::new());
    };
    if value_upper_bound > MAX_INSTANTIATION_TABLE_VALUES {
        return (clear_state_count, None, Vec::new());
    }
    let mut offsets = Vec::new();
    let mut values = Vec::new();
    if offsets.try_reserve_exact(offset_count).is_err()
        || values.try_reserve_exact(value_upper_bound).is_err()
    {
        return (clear_state_count, None, Vec::new());
    }
    offsets.push(0);
    for skeleton in skeletons {
        let start = skeleton.realization_start as usize;
        let end = start + skeleton.realization_count as usize;
        for deleted_rows in 0..clear_state_count {
            let value_start = values.len();
            for realization in &realizations[start..end] {
                if let Some(instantiated) = instantiate_realization_raw(
                    width,
                    height,
                    skeleton.cells,
                    *realization,
                    deleted_rows as u16,
                ) {
                    values.push(instantiated);
                }
            }
            let end = values.len();
            let write = sort_unique_realizations(&mut values, value_start, end);
            values.truncate(write);
            offsets.push(values.len() as u32);
        }
    }
    (clear_state_count, Some(offsets), values)
}

fn sort_unique_realizations(
    values: &mut [InstantiatedRealization],
    start: usize,
    end: usize,
) -> usize {
    values[start..end].sort_unstable();
    let mut write = start;
    for read in start..end {
        if write == start || values[read] != values[write - 1] {
            values[write] = values[read];
            write += 1;
        }
    }
    write
}

pub(super) fn instantiate_realization_raw(
    width: u8,
    height: u8,
    target_cells: u64,
    realization: Realization,
    deleted_rows: u16,
) -> Option<InstantiatedRealization> {
    if realization.required_deleted_rows & !deleted_rows != 0 {
        return None;
    }
    let occupied_target_rows = occupied_rows(width, target_cells);
    if occupied_target_rows & deleted_rows != 0 {
        return None;
    }
    let anchor = realization.target_anchor_y as u8;
    let deleted_below = (deleted_rows & lower_row_mask(anchor)).count_ones() as i8;
    let lock_y = realization.target_anchor_y - deleted_below;
    if lock_y < 0 {
        return None;
    }
    let shape = standard_tetromino_registry()
        .get(realization.piece)
        .expect("standard piece exists")
        .shape(realization.rotation);
    let mut physical = 0_u64;
    let mut projected = 0_u64;
    for cell in shape.cells() {
        let x = realization.x + cell.x();
        let y = lock_y + cell.y();
        if x < 0 || x >= width as i8 || y < 0 || y >= height as i8 {
            return None;
        }
        physical |= 1_u64 << (y as usize * width as usize + x as usize);
        let target_y = target_row_for_current_row(height, deleted_rows, y as u8)?;
        projected |= 1_u64 << (target_y as usize * width as usize + x as usize);
    }
    (projected == target_cells).then_some(InstantiatedRealization {
        lock_mask: physical,
        rotation: realization.rotation,
        x: realization.x,
        lock_y,
    })
}

fn target_row_for_current_row(height: u8, deleted_rows: u16, current_row: u8) -> Option<u8> {
    let mut visible_row = 0_u8;
    for target_row in 0..height {
        if deleted_rows & (1_u16 << target_row) != 0 {
            continue;
        }
        if visible_row == current_row {
            return Some(target_row);
        }
        visible_row += 1;
    }
    None
}

fn occupied_rows(width: u8, mut cells: u64) -> u16 {
    let mut rows = 0_u16;
    while cells != 0 {
        let cell = cells.trailing_zeros() as usize;
        cells &= cells - 1;
        rows |= 1_u16 << (cell / width as usize);
    }
    rows
}

const fn lower_row_mask(row: u8) -> u16 {
    if row == 0 {
        0
    } else {
        (1_u16 << row) - 1
    }
}

#[allow(clippy::too_many_arguments)]
fn enumerate_row_projections(
    width: u8,
    height: u8,
    initial_board: u64,
    required_cells: u64,
    piece: PieceKind,
    rotation: RotationState,
    cells: [clearra_piece_registry::registry::piece_registry::ShapeCell; 4],
    local_rows: &[u8],
    target_rows: &mut [u8; 4],
    row_index: usize,
    x: i8,
    row_filter: Option<&ProjectionRowFilter>,
    clear_rows: ClearRowDomain,
    output: &mut Vec<Realization>,
) {
    if row_index == local_rows.len() {
        let mut mask = 0_u64;
        for cell in cells {
            let local_row_index = local_rows
                .binary_search(&(cell.y() as u8))
                .expect("shape row belongs to projection domain");
            let target_y = target_rows[local_row_index];
            let target_x = x + cell.x();
            if target_x < 0 || target_x >= width as i8 {
                return;
            }
            mask |= 1_u64 << (usize::from(target_y) * usize::from(width) + target_x as usize);
        }
        if mask & initial_board != 0 || mask & !required_cells != 0 {
            return;
        }
        let mut required_deleted_rows = 0_u16;
        for index in 1..local_rows.len() {
            let local_gap = local_rows[index] - local_rows[index - 1];
            let first_deleted = target_rows[index - 1] + local_gap;
            for row in first_deleted..target_rows[index] {
                required_deleted_rows |= 1_u16 << row;
            }
        }
        if !clear_rows.allows_required_rows(u32::from(required_deleted_rows)) {
            return;
        }
        output.push(Realization {
            piece,
            cells: mask,
            required_deleted_rows,
            rotation,
            x,
            target_anchor_y: target_rows[0] as i8,
        });
        return;
    }

    let local_row = local_rows[row_index];
    let local_last = local_rows[local_rows.len() - 1];
    let minimum = if row_index == 0 {
        local_row
    } else {
        target_rows[row_index - 1] + local_row - local_rows[row_index - 1]
    };
    let remaining_span = local_last - local_row;
    if remaining_span >= height {
        return;
    }
    let maximum = if row_index != 0 && clear_rows.is_empty() {
        minimum.min(height - 1 - remaining_span)
    } else {
        height - 1 - remaining_span
    };
    for target_row in minimum..=maximum {
        if row_index != 0 && !clear_rows.allows_gap(minimum, target_row) {
            continue;
        }
        if row_filter.is_some_and(|filter| !filter.row_allowed(row_index, target_row)) {
            continue;
        }
        target_rows[row_index] = target_row;
        enumerate_row_projections(
            width,
            height,
            initial_board,
            required_cells,
            piece,
            rotation,
            cells,
            local_rows,
            target_rows,
            row_index + 1,
            x,
            row_filter,
            clear_rows,
            output,
        );
    }
}

#[cfg(test)]
mod exact_clear_domain_tests {
    use super::*;

    #[test]
    fn exact_clear_domains_match_raw_projection_when_table_storage_is_unavailable() {
        let table =
            GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
        assert!(table.has_instantiation_table());
        let mut raw = table.clone();
        // Model the bounded table allocation declining without changing the
        // admitted dimensions, skeletons, projections, or weak domains.
        raw.instantiation_offsets = None;
        raw.instantiated_realizations = Vec::new();

        for row_id in 0..table.skeleton_count() as u32 {
            assert_eq!(
                table.exact_clear_domain_mask(row_id),
                raw.exact_clear_domain_mask(row_id),
                "row={row_id}"
            );
            for deleted in 0..16 {
                let mut table_values: Vec<_> = table.instantiations(row_id, deleted).collect();
                let mut raw_values: Vec<_> = raw.instantiations(row_id, deleted).collect();
                table_values.sort_unstable();
                table_values.dedup();
                raw_values.sort_unstable();
                raw_values.dedup();
                assert_eq!(table_values, raw_values, "row={row_id} deleted={deleted}");
            }
        }
        assert_eq!(
            table.exact_clear_domain_mask(table.skeleton_count() as u32),
            None
        );
    }

    #[test]
    fn lazy_frame_queries_match_exact_dense_order_after_slot_collision() {
        let eager =
            GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
        assert!(eager.has_instantiation_table());
        assert!(LazyInstantiationCache::supports_rows(&eager.skeletons));

        let mut lazy = eager.clone();
        lazy.instantiation_offsets = None;
        lazy.instantiated_realizations = Vec::new();
        lazy.lazy_instantiations = LazyInstantiationCache::try_new(true, 1).map(Arc::new);
        let cache = lazy.lazy_instantiations.as_ref().unwrap();
        assert!(cache.slots[0].get().is_none());
        let observed: Vec<_> = lazy.instantiations(0, 0).collect();
        let observed_again: Vec<_> = lazy.instantiations(0, 0).collect();
        assert_eq!(observed, observed_again);
        let resident = cache.slots[0].get().unwrap();
        assert_eq!((resident.row_id, resident.deleted_rows), (0, 0));
        assert_eq!(&resident.values[..resident.len], observed.as_slice());

        for row_id in 0..eager.skeleton_count() as u32 {
            for deleted in 0..16 {
                assert_eq!(
                    eager.instantiations(row_id, deleted).collect::<Vec<_>>(),
                    lazy.instantiations(row_id, deleted).collect::<Vec<_>>(),
                    "row={row_id} deleted={deleted}"
                );
            }
        }
        assert!(core::ptr::eq(resident, cache.slots[0].get().unwrap()));
        assert!(cache.retained_bytes() as u128 <= LazyInstantiationCache::compile_payload_bound());
    }

    #[test]
    fn lazy_frame_raw_order_and_unsupported_queries_are_not_changed() {
        let mut fallback =
            GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
        fallback.instantiation_offsets = None;
        fallback.instantiated_realizations = Vec::new();
        let mut lazy = fallback.clone();
        lazy.lazy_instantiations = LazyInstantiationCache::try_new(false, 1).map(Arc::new);
        // The out-of-frame query is passed through, not interpreted as a
        // certificate of absence or admitted to a lazy slot.
        assert_eq!(
            fallback.instantiations(0, 1 << 4).collect::<Vec<_>>(),
            lazy.instantiations(0, 1 << 4).collect::<Vec<_>>()
        );
        assert!(lazy.lazy_instantiations.as_ref().unwrap().slots[0]
            .get()
            .is_none());
        for row_id in 0..fallback.skeleton_count() as u32 {
            for deleted in [0, 1, 2, 4, 15] {
                assert_eq!(
                    fallback.instantiations(row_id, deleted).collect::<Vec<_>>(),
                    lazy.instantiations(row_id, deleted).collect::<Vec<_>>(),
                    "row={row_id} deleted={deleted}"
                );
            }
        }
    }
}
