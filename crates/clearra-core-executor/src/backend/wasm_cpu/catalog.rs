use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_problem::SearchProblem;

use super::{
    geometry_apdp::ExactArmPairIndex, geometry_projection::ProjectionCatalog,
    geometry_separator::SeparatorCatalog, mix_digest, piece_index, WasmExactSearchError,
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
    identity_digest: u64,
}

pub(super) struct InstantiatedRealizationIter<'a> {
    catalog: &'a GeometryCatalog,
    target_cells: u64,
    deleted_rows: u16,
    precomputed: Option<core::slice::Iter<'a, InstantiatedRealization>>,
    raw: Option<core::slice::Iter<'a, Realization>>,
}

impl Iterator for InstantiatedRealizationIter<'_> {
    type Item = InstantiatedRealization;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(precomputed) = self.precomputed.as_mut() {
            return precomputed.next().copied();
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

impl GeometryCatalog {
    pub fn compile(problem: &SearchProblem) -> Result<Self, WasmExactSearchError> {
        let width = u8::try_from(problem.initial_board().width())
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_board_width_overflow"))?;
        // Geometry is compiled in the target frame. The larger spawn/search
        // height belongs to reachability and must not expand the exact-cover
        // universe beyond the active PC rows.
        let height = u8::try_from(problem.visible_height()).map_err(|_| {
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
        if required_cells.count_ones() % 4 != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_required_fill_area_not_tetromino_aligned",
            ));
        }

        Self::compile_for_required_cells(problem, required_cells)
    }

    /// Compile the inverse lock-clear catalog for a fixed set of cells that
    /// must be added to the initial board. PC search passes every unoccupied
    /// target-frame cell; build-probability search passes only its white setup
    /// layer.
    pub fn compile_for_required_cells(
        problem: &SearchProblem,
        required_cells: u64,
    ) -> Result<Self, WasmExactSearchError> {
        Self::compile_for_required_cells_on_board(
            problem,
            problem.initial_board().occupied_mask(),
            required_cells,
        )
    }

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
        if required_cells.count_ones() % 4 != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_required_fill_area_not_tetromino_aligned",
            ));
        }

        let registry = standard_tetromino_registry();
        let mut realizations = Vec::new();
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
                let mut local_rows = shape
                    .cells()
                    .into_iter()
                    .map(|cell| cell.y() as u8)
                    .collect::<Vec<_>>();
                local_rows.sort_unstable();
                local_rows.dedup();
                for x in 0..=max_x {
                    let mut target_rows = [0_u8; 4];
                    enumerate_row_projections(
                        width,
                        height,
                        initial_board,
                        required_cells,
                        piece,
                        rotation,
                        shape.cells(),
                        &local_rows,
                        &mut target_rows,
                        0,
                        x as i8,
                        &mut realizations,
                    );
                }
            }
        }
        realizations.sort_unstable();
        realizations.dedup();

        let mut skeletons = Vec::new();
        let mut ordered_realizations = Vec::with_capacity(realizations.len());
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

        let mut support_offsets = Vec::with_capacity(cell_count + 1);
        let mut support_rows = Vec::new();
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
        let separator_catalog = SeparatorCatalog::compile(width, height, &skeletons);
        let (skeleton_occupied_rows, skeleton_requirement_domains) =
            compile_skeleton_temporal_metadata(width, height, &skeletons, &ordered_realizations);

        let (clear_state_count, instantiation_offsets, instantiated_realizations) =
            compile_instantiation_table(width, height, &skeletons, &ordered_realizations);

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

        Ok(Self {
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
            identity_digest,
        })
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

    pub fn instantiations(
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
            }
        } else {
            InstantiatedRealizationIter {
                catalog: self,
                target_cells: self.skeleton(row_id).cells,
                deleted_rows,
                precomputed: None,
                raw: Some(self.realizations(row_id).iter()),
            }
        }
    }

    pub fn support(&self, cell: u8) -> &[u32] {
        let start = self.support_offsets[cell as usize] as usize;
        let end = self.support_offsets[cell as usize + 1] as usize;
        &self.support_rows[start..end]
    }

    #[cfg(feature = "webgpu-search")]
    pub fn support_offsets(&self) -> &[u32] {
        &self.support_offsets
    }

    #[cfg(feature = "webgpu-search")]
    pub fn support_rows(&self) -> &[u32] {
        &self.support_rows
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
        self.skeletons.len() * core::mem::size_of::<SkeletonRow>()
            + self.realizations.len() * core::mem::size_of::<Realization>()
            + self.skeleton_occupied_rows.capacity() * core::mem::size_of::<u16>()
            + self
                .skeleton_requirement_domains
                .as_ref()
                .map_or(0, |domains| {
                    domains.capacity() * core::mem::size_of::<u64>()
                })
            + self.support_offsets.len() * core::mem::size_of::<u32>()
            + self.support_rows.len() * core::mem::size_of::<u32>()
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

const MAX_INSTANTIATION_TABLE_VALUES: usize = 4 * 1024 * 1024;

fn compile_skeleton_temporal_metadata(
    width: u8,
    height: u8,
    skeletons: &[SkeletonRow],
    realizations: &[Realization],
) -> (Vec<u16>, Option<Vec<u64>>) {
    let mut occupied_rows = Vec::with_capacity(skeletons.len());
    let mut requirement_domains = (height <= 6).then(|| Vec::with_capacity(skeletons.len()));
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
    (occupied_rows, requirement_domains)
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
            values[value_start..].sort_unstable();
            let mut write = value_start;
            for read in value_start..values.len() {
                if write == value_start || values[read] != values[write - 1] {
                    values[write] = values[read];
                    write += 1;
                }
            }
            values.truncate(write);
            offsets.push(values.len() as u32);
        }
    }
    (clear_state_count, Some(offsets), values)
}

fn instantiate_realization_raw(
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
    let maximum = height - 1 - remaining_span;
    for target_row in minimum..=maximum {
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
            output,
        );
    }
}
