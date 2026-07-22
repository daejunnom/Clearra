use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::{
    registry::piece_registry::ShapeCell, standard::tetromino_registry::standard_tetromino_registry,
};
use clearra_problem::BuildProbabilityField;

use super::{
    extended_board::{logical_row_for_physical, lower_row_mask, ExtendedBoard},
    extended_geometry_domain::ExtendedArmPairIndex,
    geometry_projection::ProjectionCatalog,
    mix_digest, piece_index, WasmExactSearchError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ExtendedSkeletonRow {
    pub piece: PieceKind,
    pub cells: ExtendedBoard,
    realization_start: u32,
    realization_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ExtendedRealization {
    piece: PieceKind,
    cells: ExtendedBoard,
    required_deleted_rows: u32,
    rotation: RotationState,
    x: i8,
    target_anchor_y: i8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct ExtendedInstantiation {
    pub lock_mask: ExtendedBoard,
    pub rotation: RotationState,
    pub x: i8,
    pub lock_y: i8,
}

pub(super) struct ExtendedInverseCatalog {
    width: u8,
    height: u8,
    initial_board: ExtendedBoard,
    required_cells: ExtendedBoard,
    skeletons: Vec<ExtendedSkeletonRow>,
    realizations: Vec<ExtendedRealization>,
    support_offsets: Vec<u32>,
    support_rows: Vec<u32>,
    apdp_index: ExtendedArmPairIndex,
    projection_catalog: ProjectionCatalog,
    dense_geometry: Option<DenseExtendedGeometryCatalog>,
    identity_digest: u64,
}

pub(super) struct DenseExtendedGeometryCatalog {
    dense_to_world: Vec<u16>,
    world_to_dense: [u8; 256],
    skeleton_cells: Vec<u64>,
}

impl DenseExtendedGeometryCatalog {
    fn compile(required_cells: ExtendedBoard, skeletons: &[ExtendedSkeletonRow]) -> Option<Self> {
        let cell_count = required_cells.count_ones() as usize;
        if cell_count == 0 || cell_count > 64 {
            return None;
        }

        let mut dense_to_world = Vec::new();
        dense_to_world.try_reserve_exact(cell_count).ok()?;
        let mut world_to_dense = [u8::MAX; 256];
        for (dense, world) in required_cells.cells().enumerate() {
            let dense = u8::try_from(dense).ok()?;
            world_to_dense[usize::from(world)] = dense;
            dense_to_world.push(world);
        }

        let mut skeleton_cells = Vec::new();
        skeleton_cells.try_reserve_exact(skeletons.len()).ok()?;
        for row in skeletons {
            let mut mask = 0_u64;
            for world in row.cells.cells() {
                let dense = *world_to_dense.get(usize::from(world))?;
                if dense == u8::MAX {
                    return None;
                }
                mask |= 1_u64 << dense;
            }
            if mask.count_ones() != 4 {
                return None;
            }
            skeleton_cells.push(mask);
        }

        Some(Self {
            dense_to_world,
            world_to_dense,
            skeleton_cells,
        })
    }

    pub fn encode(&self, cells: ExtendedBoard) -> Option<u64> {
        let mut mask = 0_u64;
        for world in cells.cells() {
            let dense = *self.world_to_dense.get(usize::from(world))?;
            if dense == u8::MAX {
                return None;
            }
            mask |= 1_u64 << dense;
        }
        Some(mask)
    }

    pub fn decode(&self, mut mask: u64) -> Option<ExtendedBoard> {
        let mut cells = ExtendedBoard::EMPTY;
        while mask != 0 {
            let dense = mask.trailing_zeros() as u8;
            cells.insert(self.world_cell(dense)?);
            mask &= mask - 1;
        }
        Some(cells)
    }

    pub fn cell_count(&self) -> usize {
        self.dense_to_world.len()
    }

    pub fn world_cell(&self, dense: u8) -> Option<u16> {
        self.dense_to_world.get(usize::from(dense)).copied()
    }

    pub fn skeleton_cells(&self, row_id: u32) -> Option<u64> {
        self.skeleton_cells.get(row_id as usize).copied()
    }

    fn retained_bytes(&self) -> usize {
        self.dense_to_world.capacity() * core::mem::size_of::<u16>()
            + self.skeleton_cells.capacity() * core::mem::size_of::<u64>()
    }
}

impl ExtendedInverseCatalog {
    pub fn compile(field: BuildProbabilityField) -> Result<Self, WasmExactSearchError> {
        let width = field.width();
        let height = field.height();
        if !(7..=24).contains(&height) || width != 10 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_catalog_requires_7_to_24_rows",
            ));
        }
        let initial_board = ExtendedBoard::from_mask(field.base());
        let required_cells = ExtendedBoard::from_mask(field.target());
        if initial_board.intersects(required_cells) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_target_overlaps_initial_board",
            ));
        }
        if required_cells.count_ones() % 4 != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_required_area_not_tetromino_aligned",
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
        let mut cursor = 0usize;
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
            skeletons.push(ExtendedSkeletonRow {
                piece,
                cells,
                realization_start: start as u32,
                realization_count: (ordered_realizations.len() - start) as u32,
            });
        }

        let cell_count = usize::from(width) * usize::from(height);
        let mut by_cell = (0..cell_count).map(|_| Vec::new()).collect::<Vec<_>>();
        for (row_id, row) in skeletons.iter().enumerate() {
            for cell in row.cells.cells() {
                by_cell[cell as usize].push(row_id as u32);
            }
        }
        let mut support_offsets = Vec::with_capacity(cell_count + 1);
        let mut support_rows = Vec::new();
        support_offsets.push(0);
        for mut rows in by_cell {
            rows.sort_unstable();
            rows.dedup();
            support_rows.extend(rows);
            support_offsets.push(support_rows.len() as u32);
        }
        let apdp_index = ExtendedArmPairIndex::compile(width, &skeletons).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_extended_apdp_catalog_storage_unavailable"),
        )?;
        let projection_catalog = ProjectionCatalog::compile_extended(width, height, &skeletons)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_projection_catalog_storage_unavailable",
            ))?;
        let dense_geometry = DenseExtendedGeometryCatalog::compile(required_cells, &skeletons);

        let mut identity_digest = mix_digest(0, u64::from(width));
        identity_digest = mix_digest(identity_digest, u64::from(height));
        for word in initial_board
            .words()
            .into_iter()
            .chain(required_cells.words())
        {
            identity_digest = mix_digest(identity_digest, word);
        }
        for realization in &ordered_realizations {
            identity_digest = mix_digest(identity_digest, piece_index(realization.piece) as u64);
            for word in realization.cells.words() {
                identity_digest = mix_digest(identity_digest, word);
            }
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

        Ok(Self {
            width,
            height,
            initial_board,
            required_cells,
            skeletons,
            realizations: ordered_realizations,
            support_offsets,
            support_rows,
            apdp_index,
            projection_catalog,
            dense_geometry,
            identity_digest,
        })
    }

    pub const fn width(&self) -> u8 {
        self.width
    }

    pub const fn height(&self) -> u8 {
        self.height
    }

    pub const fn initial_board(&self) -> ExtendedBoard {
        self.initial_board
    }

    pub const fn required_cells(&self) -> ExtendedBoard {
        self.required_cells
    }

    pub fn skeleton(&self, row_id: u32) -> ExtendedSkeletonRow {
        self.skeletons[row_id as usize]
    }

    pub fn skeletons(&self) -> &[ExtendedSkeletonRow] {
        &self.skeletons
    }

    pub fn support(&self, cell: u16) -> &[u32] {
        let start = self.support_offsets[cell as usize] as usize;
        let end = self.support_offsets[cell as usize + 1] as usize;
        &self.support_rows[start..end]
    }

    pub fn instantiations(
        &self,
        row_id: u32,
        deleted_rows: u32,
    ) -> impl Iterator<Item = ExtendedInstantiation> + '_ {
        let row = self.skeleton(row_id);
        let start = row.realization_start as usize;
        let end = start + row.realization_count as usize;
        self.realizations[start..end]
            .iter()
            .copied()
            .filter_map(move |realization| self.instantiate(realization, deleted_rows))
    }

    pub const fn identity_digest(&self) -> u64 {
        self.identity_digest
    }

    pub const fn projection_catalog(&self) -> &ProjectionCatalog {
        &self.projection_catalog
    }

    pub const fn apdp_index(&self) -> &ExtendedArmPairIndex {
        &self.apdp_index
    }

    pub const fn dense_geometry(&self) -> Option<&DenseExtendedGeometryCatalog> {
        self.dense_geometry.as_ref()
    }

    pub fn apdp_row_is_static_exact(&self, row_id: u32) -> bool {
        let row = self.skeleton(row_id);
        let start = row.realization_start as usize;
        let end = start + row.realization_count as usize;
        self.apdp_index.row_support_flags(row_id) != 0
            && self.realizations[start..end]
                .iter()
                .all(|realization| realization.required_deleted_rows == 0)
    }

    pub fn retained_bytes(&self) -> usize {
        self.skeletons.capacity() * core::mem::size_of::<ExtendedSkeletonRow>()
            + self.realizations.capacity() * core::mem::size_of::<ExtendedRealization>()
            + self.support_offsets.capacity() * core::mem::size_of::<u32>()
            + self.support_rows.capacity() * core::mem::size_of::<u32>()
            + self.apdp_index.retained_bytes()
            + self.projection_catalog.retained_bytes()
            + self
                .dense_geometry
                .as_ref()
                .map_or(0, DenseExtendedGeometryCatalog::retained_bytes)
    }

    fn instantiate(
        &self,
        realization: ExtendedRealization,
        deleted_rows: u32,
    ) -> Option<ExtendedInstantiation> {
        if realization.required_deleted_rows & !deleted_rows != 0 {
            return None;
        }
        for row in occupied_rows(self.width, realization.cells) {
            if deleted_rows & (1_u32 << row) != 0 {
                return None;
            }
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
        let mut physical = ExtendedBoard::EMPTY;
        let mut projected = ExtendedBoard::EMPTY;
        for cell in shape.cells() {
            let x = realization.x + cell.x();
            let y = lock_y + cell.y();
            if x < 0 || x >= self.width as i8 || y < 0 || y >= self.height as i8 {
                return None;
            }
            physical.insert(y as u16 * u16::from(self.width) + x as u16);
            let logical_y = logical_row_for_physical(self.height, deleted_rows, y as u8)?;
            projected.insert(u16::from(logical_y) * u16::from(self.width) + x as u16);
        }
        (projected == realization.cells).then_some(ExtendedInstantiation {
            lock_mask: physical,
            rotation: realization.rotation,
            x: realization.x,
            lock_y,
        })
    }
}

fn occupied_rows(width: u8, cells: ExtendedBoard) -> impl Iterator<Item = u8> {
    let mut rows = 0_u32;
    for cell in cells.cells() {
        rows |= 1_u32 << (cell / u16::from(width));
    }
    (0..32).filter(move |row| rows & (1_u32 << row) != 0)
}

#[allow(clippy::too_many_arguments)]
fn enumerate_row_projections(
    width: u8,
    height: u8,
    initial_board: ExtendedBoard,
    required_cells: ExtendedBoard,
    piece: PieceKind,
    rotation: RotationState,
    cells: [ShapeCell; 4],
    local_rows: &[u8],
    target_rows: &mut [u8; 4],
    row_index: usize,
    x: i8,
    output: &mut Vec<ExtendedRealization>,
) {
    if row_index == local_rows.len() {
        let mut mask = ExtendedBoard::EMPTY;
        for cell in cells {
            let local_row_index = local_rows
                .binary_search(&(cell.y() as u8))
                .expect("shape row belongs to projection domain");
            let target_y = target_rows[local_row_index];
            let target_x = x + cell.x();
            if target_x < 0 || target_x >= width as i8 {
                return;
            }
            mask.insert(u16::from(target_y) * u16::from(width) + target_x as u16);
        }
        if mask.intersects(initial_board) || !mask.is_subset_of(required_cells) {
            return;
        }
        let mut required_deleted_rows = 0_u32;
        for index in 1..local_rows.len() {
            let local_gap = local_rows[index] - local_rows[index - 1];
            let first_deleted = target_rows[index - 1] + local_gap;
            for row in first_deleted..target_rows[index] {
                required_deleted_rows |= 1_u32 << row;
            }
        }
        output.push(ExtendedRealization {
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
