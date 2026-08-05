use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::{
    registry::piece_registry::ShapeCell, standard::tetromino_registry::standard_tetromino_registry,
};

use crate::{
    board::StructureBoard,
    model::{canonical_geometry_rotation, piece_index, PieceInventory, StructureOperation},
};

const PIECE_KIND_COUNT: usize = 7;

/// A stable index into [`LogicalOperationCatalog::operations`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CatalogOperationId(u32);

impl CatalogOperationId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The row-independent portion of a projected operation.
///
/// Operations with one key differ only in their monotone projection onto the
/// bounded logical rows. Geometry-equivalent rotations have already been
/// canonicalized.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct OperationGeometryKey {
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
}

impl OperationGeometryKey {
    pub(crate) const fn new(piece: PieceKind, rotation: RotationState, x: i8) -> Self {
        Self {
            piece,
            rotation: canonical_geometry_rotation(piece, rotation),
            x,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationCatalogError {
    InvalidHeight(u8),
    InitialBoardOutsideHeight,
    OperationCountOverflow,
}

/// A compact immutable adjacency index. The domain owns only offsets; every
/// edge is a stable operation id into the catalog's single operation array.
#[derive(Clone, Debug, Eq, PartialEq)]
struct OperationCsr {
    offsets: Box<[u32]>,
    operation_ids: Box<[CatalogOperationId]>,
}

impl OperationCsr {
    fn get(&self, key: usize) -> &[CatalogOperationId] {
        let Some(window) = self.offsets.get(key..=key + 1) else {
            return &[];
        };
        &self.operation_ids[window[0] as usize..window[1] as usize]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GeometryBucket {
    key: OperationGeometryKey,
    start: u32,
    end: u32,
}

/// All static logical placements for a fixed height, initial field and piece
/// inventory.
///
/// The catalog contains geometry only. Reachability and temporal validation
/// remain exact runtime responsibilities. A projected gap records precisely
/// the rows that must already have been deleted before that geometry can be
/// instantiated on the compacted physical field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogicalOperationCatalog {
    height: u8,
    operations: Box<[StructureOperation]>,
    piece_offsets: [u32; PIECE_KIND_COUNT + 1],
    row_index: OperationCsr,
    cell_index: OperationCsr,
    geometry_buckets: Box<[GeometryBucket]>,
}

impl LogicalOperationCatalog {
    pub(crate) fn compile(
        height: u8,
        initial_board: StructureBoard,
        inventory: PieceInventory,
    ) -> Result<Self, OperationCatalogError> {
        if height == 0 || height > StructureBoard::MAX_HEIGHT {
            return Err(OperationCatalogError::InvalidHeight(height));
        }
        if initial_board.has_cells_at_or_above(height) {
            return Err(OperationCatalogError::InitialBoardOutsideHeight);
        }

        let registry = standard_tetromino_registry();
        let mut operations = Vec::new();
        for piece in PieceKind::STANDARD_TETROMINOES {
            if inventory.count(piece) == 0 {
                continue;
            }
            let definition = registry.get(piece).expect("standard tetromino exists");
            let mut emitted_rotations = [None; 4];
            let mut emitted_rotation_count = 0_usize;
            for rotation in RotationState::ALL {
                let rotation = canonical_geometry_rotation(piece, rotation);
                if emitted_rotations[..emitted_rotation_count].contains(&Some(rotation)) {
                    continue;
                }
                emitted_rotations[emitted_rotation_count] = Some(rotation);
                emitted_rotation_count += 1;

                let shape = definition.shape(rotation);
                let cells = shape.cells();
                let minimum_x = cells
                    .iter()
                    .map(|cell| cell.x())
                    .min()
                    .expect("tetromino has cells");
                let maximum_x = cells
                    .iter()
                    .map(|cell| cell.x())
                    .max()
                    .expect("tetromino has cells");
                let first_x = -minimum_x;
                let last_x = i16::from(StructureBoard::WIDTH - 1) - i16::from(maximum_x);
                if i16::from(first_x) > last_x {
                    continue;
                }

                let mut local_rows = cells.map(|cell| cell.y());
                local_rows.sort_unstable();
                let local_row_count = deduplicate_sorted_rows(&mut local_rows);
                let local_rows = &local_rows[..local_row_count];
                let mut target_rows = [0_u8; 4];
                for x in i16::from(first_x)..=last_x {
                    enumerate_row_projections(
                        height,
                        initial_board,
                        piece,
                        rotation,
                        cells,
                        local_rows,
                        &mut target_rows,
                        0,
                        x as i8,
                        &mut operations,
                    );
                }
            }
        }

        operations.sort_unstable();
        operations.dedup();
        if operations.len() > u32::MAX as usize {
            return Err(OperationCatalogError::OperationCountOverflow);
        }

        let piece_offsets = compile_piece_offsets(&operations);
        let geometry_buckets = compile_geometry_buckets(&operations)?;
        let row_index = compile_row_index(height, &operations)?;
        let cell_index = compile_cell_index(height, &operations)?;

        Ok(Self {
            height,
            operations: operations.into_boxed_slice(),
            piece_offsets,
            row_index,
            cell_index,
            geometry_buckets,
        })
    }

    pub(crate) const fn height(&self) -> u8 {
        self.height
    }

    pub(crate) fn operations(&self) -> &[StructureOperation] {
        &self.operations
    }

    pub(crate) fn operation(&self, id: CatalogOperationId) -> Option<StructureOperation> {
        self.operations.get(id.index()).copied()
    }

    pub(crate) fn operations_for_piece(&self, piece: PieceKind) -> &[StructureOperation] {
        let index = piece_index(piece);
        let range = self.piece_offsets[index] as usize..self.piece_offsets[index + 1] as usize;
        &self.operations[range]
    }

    #[cfg(test)]
    pub(crate) fn operation_ids_for_row(&self, row: u8) -> &[CatalogOperationId] {
        if row >= self.height {
            return &[];
        }
        self.row_index.get(usize::from(row))
    }

    pub(crate) fn operation_ids_for_cell(&self, x: u8, y: u8) -> &[CatalogOperationId] {
        if x >= StructureBoard::WIDTH || y >= self.height {
            return &[];
        }
        let cell = usize::from(y) * usize::from(StructureBoard::WIDTH) + usize::from(x);
        self.cell_index.get(cell)
    }

    #[cfg(test)]
    pub(crate) fn operations_for_geometry(
        &self,
        key: OperationGeometryKey,
    ) -> &[StructureOperation] {
        let Ok(index) = self
            .geometry_buckets
            .binary_search_by_key(&key, |bucket| bucket.key)
        else {
            return &[];
        };
        let bucket = self.geometry_buckets[index];
        &self.operations[bucket.start as usize..bucket.end as usize]
    }
}

fn deduplicate_sorted_rows(rows: &mut [i8; 4]) -> usize {
    let mut output = 1_usize;
    for input in 1..rows.len() {
        if rows[input] != rows[output - 1] {
            rows[output] = rows[input];
            output += 1;
        }
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn enumerate_row_projections(
    height: u8,
    initial_board: StructureBoard,
    piece: PieceKind,
    rotation: RotationState,
    cells: [ShapeCell; 4],
    local_rows: &[i8],
    target_rows: &mut [u8; 4],
    row_index: usize,
    x: i8,
    output: &mut Vec<StructureOperation>,
) {
    if row_index == local_rows.len() {
        emit_projection(
            initial_board,
            piece,
            rotation,
            cells,
            local_rows,
            target_rows,
            x,
            output,
        );
        return;
    }

    let minimum = if row_index == 0 {
        0
    } else {
        let local_gap = local_rows[row_index] - local_rows[row_index - 1];
        target_rows[row_index - 1] + local_gap as u8
    };
    let remaining_span = local_rows[local_rows.len() - 1] - local_rows[row_index];
    let Some(maximum) = (height - 1).checked_sub(remaining_span as u8) else {
        return;
    };
    for target_row in minimum..=maximum {
        target_rows[row_index] = target_row;
        enumerate_row_projections(
            height,
            initial_board,
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

#[allow(clippy::too_many_arguments)]
fn emit_projection(
    initial_board: StructureBoard,
    piece: PieceKind,
    rotation: RotationState,
    cells: [ShapeCell; 4],
    local_rows: &[i8],
    target_rows: &[u8; 4],
    x: i8,
    output: &mut Vec<StructureOperation>,
) {
    let mut mask = StructureBoard::EMPTY;
    for cell in cells {
        let local_row_index = local_rows
            .binary_search(&cell.y())
            .expect("shape row belongs to projection domain");
        let target_y = target_rows[local_row_index];
        let target_x = x + cell.x();
        debug_assert!((0..StructureBoard::WIDTH as i8).contains(&target_x));
        mask.insert_index(u16::from(target_y) * u16::from(StructureBoard::WIDTH) + target_x as u16);
    }
    if mask.intersects(initial_board) {
        return;
    }

    let mut need_deleted_rows = 0_u32;
    for index in 1..local_rows.len() {
        let local_gap = (local_rows[index] - local_rows[index - 1]) as u8;
        let first_deleted = target_rows[index - 1] + local_gap;
        for row in first_deleted..target_rows[index] {
            need_deleted_rows |= 1_u32 << row;
        }
    }
    let y = i16::from(target_rows[0]) - i16::from(local_rows[0]);
    let y = i8::try_from(y).expect("bounded logical height fits i8");
    output.push(StructureOperation::new(
        piece,
        rotation,
        x,
        y,
        mask,
        need_deleted_rows,
    ));
}

fn compile_piece_offsets(operations: &[StructureOperation]) -> [u32; PIECE_KIND_COUNT + 1] {
    let mut offsets = [0_u32; PIECE_KIND_COUNT + 1];
    let mut cursor = 0_usize;
    for piece in PieceKind::STANDARD_TETROMINOES {
        let index = piece_index(piece);
        offsets[index] = cursor as u32;
        while cursor < operations.len() && operations[cursor].piece() == piece {
            cursor += 1;
        }
    }
    offsets[PIECE_KIND_COUNT] = cursor as u32;
    offsets
}

fn compile_geometry_buckets(
    operations: &[StructureOperation],
) -> Result<Box<[GeometryBucket]>, OperationCatalogError> {
    let mut buckets = Vec::new();
    let mut start = 0_usize;
    while start < operations.len() {
        let operation = operations[start];
        let key = OperationGeometryKey::new(operation.piece(), operation.rotation(), operation.x());
        let mut end = start + 1;
        while end < operations.len() {
            let next = operations[end];
            if OperationGeometryKey::new(next.piece(), next.rotation(), next.x()) != key {
                break;
            }
            end += 1;
        }
        buckets.push(GeometryBucket {
            key,
            start: u32::try_from(start)
                .map_err(|_| OperationCatalogError::OperationCountOverflow)?,
            end: u32::try_from(end).map_err(|_| OperationCatalogError::OperationCountOverflow)?,
        });
        start = end;
    }
    Ok(buckets.into_boxed_slice())
}

fn compile_row_index(
    height: u8,
    operations: &[StructureOperation],
) -> Result<OperationCsr, OperationCatalogError> {
    compile_csr(usize::from(height), operations, |operation, emit| {
        for row in 0..height {
            if operation.mask().row_bits(row) != 0 {
                emit(usize::from(row));
            }
        }
    })
}

fn compile_cell_index(
    height: u8,
    operations: &[StructureOperation],
) -> Result<OperationCsr, OperationCatalogError> {
    let width = usize::from(StructureBoard::WIDTH);
    compile_csr(
        width * usize::from(height),
        operations,
        |operation, emit| {
            for row in 0..height {
                let row_bits = operation.mask().row_bits(row);
                for x in 0..StructureBoard::WIDTH {
                    if row_bits & (1_u16 << x) != 0 {
                        emit(usize::from(row) * width + usize::from(x));
                    }
                }
            }
        },
    )
}

fn compile_csr(
    domain_len: usize,
    operations: &[StructureOperation],
    mut memberships: impl FnMut(&StructureOperation, &mut dyn FnMut(usize)),
) -> Result<OperationCsr, OperationCatalogError> {
    let mut counts = vec![0_u32; domain_len];
    for operation in operations {
        memberships(operation, &mut |key| {
            counts[key] = counts[key]
                .checked_add(1)
                .expect("catalog operation count was checked");
        });
    }

    let mut offsets = vec![0_u32; domain_len + 1];
    for key in 0..domain_len {
        offsets[key + 1] = offsets[key]
            .checked_add(counts[key])
            .ok_or(OperationCatalogError::OperationCountOverflow)?;
    }
    let edge_count = offsets[domain_len] as usize;
    let mut operation_ids = vec![CatalogOperationId(0); edge_count];
    let mut cursors = offsets[..domain_len].to_vec();
    for (operation_index, operation) in operations.iter().enumerate() {
        let operation_id = CatalogOperationId(
            u32::try_from(operation_index)
                .map_err(|_| OperationCatalogError::OperationCountOverflow)?,
        );
        memberships(operation, &mut |key| {
            let cursor = cursors[key] as usize;
            operation_ids[cursor] = operation_id;
            cursors[key] += 1;
        });
    }

    Ok(OperationCsr {
        offsets: offsets.into_boxed_slice(),
        operation_ids: operation_ids.into_boxed_slice(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inventory(pieces: &[PieceKind]) -> PieceInventory {
        PieceInventory::from_pieces(pieces.iter().copied()).expect("bounded inventory")
    }

    fn occupied_cell_count(board: StructureBoard, height: u8) -> u32 {
        (0..height)
            .map(|row| board.row_bits(row).count_ones())
            .sum()
    }

    #[test]
    fn symmetric_geometry_rotations_are_emitted_once() {
        let catalog = LogicalOperationCatalog::compile(
            4,
            StructureBoard::EMPTY,
            inventory(&[PieceKind::I, PieceKind::O, PieceKind::S, PieceKind::Z]),
        )
        .expect("catalog");

        assert_eq!(catalog.operations_for_piece(PieceKind::I).len(), 38);
        assert_eq!(catalog.operations_for_piece(PieceKind::O).len(), 54);
        assert_eq!(catalog.operations_for_piece(PieceKind::S).len(), 84);
        assert_eq!(catalog.operations_for_piece(PieceKind::Z).len(), 84);
        assert!(catalog.operations().iter().all(|operation| {
            !matches!(
                operation.rotation(),
                RotationState::Two | RotationState::Left
            )
        }));
        assert!(catalog
            .operations_for_piece(PieceKind::O)
            .iter()
            .all(|operation| operation.rotation() == RotationState::Zero));
    }

    #[test]
    fn every_operation_is_bounded_by_the_catalog_height() {
        let catalog =
            LogicalOperationCatalog::compile(4, StructureBoard::EMPTY, inventory(&[PieceKind::T]))
                .expect("catalog");

        assert_eq!(catalog.height(), 4);
        assert_eq!(catalog.operations().len(), 168);
        for operation in catalog.operations() {
            assert_eq!(occupied_cell_count(operation.mask(), catalog.height()), 4);
            assert!((catalog.height()..StructureBoard::MAX_HEIGHT)
                .all(|row| operation.mask().row_bits(row) == 0));
        }
        assert_eq!(
            LogicalOperationCatalog::compile(
                StructureBoard::MAX_HEIGHT + 1,
                StructureBoard::EMPTY,
                inventory(&[PieceKind::T]),
            ),
            Err(OperationCatalogError::InvalidHeight(
                StructureBoard::MAX_HEIGHT + 1
            ))
        );
    }

    #[test]
    fn deleted_gap_is_part_of_the_operation_identity() {
        let catalog =
            LogicalOperationCatalog::compile(4, StructureBoard::EMPTY, inventory(&[PieceKind::T]))
                .expect("catalog");
        let geometry = catalog.operations_for_geometry(OperationGeometryKey::new(
            PieceKind::T,
            RotationState::Zero,
            0,
        ));
        let contiguous_mask = StructureBoard::from_rows(&[0b111, 0b010]).expect("T mask");
        let gapped_mask = StructureBoard::from_rows(&[0b111, 0, 0b010]).expect("gapped T");
        let contiguous = geometry
            .iter()
            .find(|operation| operation.mask() == contiguous_mask)
            .expect("contiguous projection");
        let gapped = geometry
            .iter()
            .find(|operation| operation.mask() == gapped_mask)
            .expect("gapped projection");

        assert_eq!(contiguous.y(), 0);
        assert_eq!(gapped.y(), 0);
        assert_eq!(contiguous.need_deleted_rows(), 0);
        assert_eq!(gapped.need_deleted_rows(), 1 << 1);
        assert_ne!(contiguous, gapped);
    }

    #[test]
    fn inventory_filters_piece_families_without_multiplicity_duplication() {
        let one =
            LogicalOperationCatalog::compile(4, StructureBoard::EMPTY, inventory(&[PieceKind::T]))
                .expect("single T catalog");
        let two = LogicalOperationCatalog::compile(
            4,
            StructureBoard::EMPTY,
            inventory(&[PieceKind::T, PieceKind::T]),
        )
        .expect("double T catalog");

        assert_eq!(one.operations(), two.operations());
        assert_eq!(one.operations_for_piece(PieceKind::T).len(), 168);
        for piece in PieceKind::STANDARD_TETROMINOES {
            if piece != PieceKind::T {
                assert!(one.operations_for_piece(piece).is_empty(), "{piece:?}");
            }
        }
    }

    #[test]
    fn t_projection_can_span_an_initially_full_logical_row() {
        let initial = StructureBoard::from_rows(&[0, 0x03ff]).expect("full logical row 1");
        let catalog = LogicalOperationCatalog::compile(4, initial, inventory(&[PieceKind::T]))
            .expect("catalog");
        let expected_mask =
            StructureBoard::from_rows(&[0b111, 0, 0b010]).expect("projected T mask");
        let geometry_key = OperationGeometryKey::new(PieceKind::T, RotationState::Zero, 0);
        let expected = catalog
            .operations_for_geometry(geometry_key)
            .iter()
            .copied()
            .find(|operation| operation.mask() == expected_mask)
            .expect("T operation spanning deleted row 1");

        assert_eq!(catalog.operations().len(), 66);
        assert_eq!(expected.need_deleted_rows(), 1 << 1);
        assert_eq!(expected.rotation(), RotationState::Zero);
        assert_eq!(expected.x(), 0);
        assert_eq!(expected.y(), 0);

        let expected_id = CatalogOperationId(
            catalog
                .operations()
                .binary_search(&expected)
                .expect("operation stored") as u32,
        );
        assert!(catalog.operation_ids_for_row(0).contains(&expected_id));
        assert!(!catalog.operation_ids_for_row(1).contains(&expected_id));
        assert!(catalog.operation_ids_for_row(2).contains(&expected_id));
        assert!(catalog.operation_ids_for_cell(1, 2).contains(&expected_id));
        assert_eq!(catalog.operation(expected_id), Some(expected));

        assert!(catalog
            .operations()
            .iter()
            .all(|operation| !operation.mask().intersects(initial)));
    }
}
