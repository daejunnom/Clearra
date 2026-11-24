use super::*;
use crate::cooperative_sort::CooperativeSort;

struct Projection {
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    cells: [ShapeCell; 4],
    rows: [i8; 4],
    row_count: usize,
    target_rows: [u8; 4],
    started: bool,
}

enum Phase {
    Project,
    Sort,
    Deduplicate,
    Count,
    Allocate,
    Fill,
    Complete,
}

/// Every step emits at most one projection or indexes one operation. The
/// only fixed-size loops are bounded by four cells or the physical field.
pub(crate) struct CatalogCursor {
    height: u8,
    initial: StructureBoard,
    projections: Vec<Projection>,
    projection: usize,
    operations: Vec<StructureOperation>,
    phase: Phase,
    sort: CooperativeSort,
    read: usize,
    write: usize,
    piece_offsets: [u32; 8],
    buckets: Vec<GeometryBucket>,
    row_offsets: Vec<u32>,
    cell_offsets: Vec<u32>,
    row_cursors: Vec<u32>,
    cell_cursors: Vec<u32>,
    row_ids: Vec<CatalogOperationId>,
    cell_ids: Vec<CatalogOperationId>,
}

impl CatalogCursor {
    pub(crate) fn new(
        height: u8,
        initial: StructureBoard,
        inventory: PieceInventory,
    ) -> Result<Self, OperationCatalogError> {
        if height == 0 || height > StructureBoard::MAX_HEIGHT {
            return Err(OperationCatalogError::InvalidHeight(height));
        }
        if initial.has_cells_at_or_above(height) {
            return Err(OperationCatalogError::InitialBoardOutsideHeight);
        }
        let registry = standard_tetromino_registry();
        let mut projections = Vec::new();
        for piece in PieceKind::STANDARD_TETROMINOES {
            if inventory.count(piece) == 0 {
                continue;
            }
            for rotation in RotationState::ALL {
                if canonical_geometry_rotation(piece, rotation) != rotation {
                    continue;
                }
                let cells = registry
                    .get(piece)
                    .expect("standard piece")
                    .shape(rotation)
                    .cells();
                let first = -cells.iter().map(|cell| cell.x()).min().unwrap();
                let last = StructureBoard::WIDTH as i8
                    - 1
                    - cells.iter().map(|cell| cell.x()).max().unwrap();
                let mut rows = cells.map(|cell| cell.y());
                rows.sort_unstable();
                let row_count = deduplicate_sorted_rows(&mut rows);
                for x in first..=last {
                    projections.push(Projection {
                        piece,
                        rotation,
                        x,
                        cells,
                        rows,
                        row_count,
                        target_rows: [0; 4],
                        started: false,
                    });
                }
            }
        }
        Ok(Self {
            height,
            initial,
            projections,
            projection: 0,
            operations: Vec::new(),
            phase: Phase::Project,
            sort: CooperativeSort::new(0),
            read: 0,
            write: 0,
            piece_offsets: [0; 8],
            buckets: Vec::new(),
            row_offsets: vec![0; usize::from(height) + 1],
            cell_offsets: vec![0; usize::from(height) * 10 + 1],
            row_cursors: Vec::new(),
            cell_cursors: Vec::new(),
            row_ids: Vec::new(),
            cell_ids: Vec::new(),
        })
    }

    pub(crate) fn step(&mut self) -> Result<bool, OperationCatalogError> {
        match self.phase {
            Phase::Project => {
                let Some(projection) = self.projections.get_mut(self.projection) else {
                    self.sort = CooperativeSort::new(self.operations.len());
                    self.phase = Phase::Sort;
                    return Ok(false);
                };
                let n = projection.row_count;
                if !projection.started {
                    projection.started = true;
                    for row in 0..n {
                        projection.target_rows[row] =
                            (projection.rows[row] - projection.rows[0]) as u8;
                    }
                    if projection.target_rows[n - 1] >= self.height {
                        self.projection += 1;
                        return Ok(false);
                    }
                } else {
                    let mut advanced = false;
                    for row in (0..n).rev() {
                        let remaining = (projection.rows[n - 1] - projection.rows[row]) as u8;
                        if projection.target_rows[row] < self.height - 1 - remaining {
                            projection.target_rows[row] += 1;
                            for later in row + 1..n {
                                projection.target_rows[later] = projection.target_rows[later - 1]
                                    + (projection.rows[later] - projection.rows[later - 1]) as u8;
                            }
                            advanced = true;
                            break;
                        }
                    }
                    if !advanced {
                        self.projection += 1;
                        return Ok(false);
                    }
                }
                emit_projection(
                    self.initial,
                    projection.piece,
                    projection.rotation,
                    projection.cells,
                    &projection.rows[..n],
                    &projection.target_rows,
                    projection.x,
                    &mut self.operations,
                );
                if self.operations.len() > u32::MAX as usize {
                    return Err(OperationCatalogError::OperationCountOverflow);
                }
            }
            Phase::Sort => {
                if self.sort.step(&mut self.operations, Ord::cmp) {
                    self.phase = Phase::Deduplicate;
                }
            }
            Phase::Deduplicate => {
                if self.read == self.operations.len() {
                    self.operations.truncate(self.write);
                    self.read = 0;
                    self.phase = Phase::Count;
                } else {
                    if self.write == 0
                        || self.operations[self.read] != self.operations[self.write - 1]
                    {
                        self.operations.swap(self.read, self.write);
                        self.write += 1;
                    }
                    self.read += 1;
                }
            }
            Phase::Count => {
                if self.read == self.operations.len() {
                    for values in [&mut self.row_offsets, &mut self.cell_offsets] {
                        for index in 1..values.len() {
                            values[index] = values[index]
                                .checked_add(values[index - 1])
                                .ok_or(OperationCatalogError::OperationCountOverflow)?;
                        }
                    }
                    for index in 1..self.piece_offsets.len() {
                        self.piece_offsets[index] += self.piece_offsets[index - 1];
                    }
                    self.row_cursors = self.row_offsets.clone();
                    self.cell_cursors = self.cell_offsets.clone();
                    self.row_ids
                        .reserve_exact(*self.row_offsets.last().unwrap() as usize);
                    self.cell_ids
                        .reserve_exact(*self.cell_offsets.last().unwrap() as usize);
                    self.phase = Phase::Allocate;
                } else {
                    let operation = self.operations[self.read];
                    self.piece_offsets[piece_index(operation.piece()) + 1] += 1;
                    let key = OperationGeometryKey::new(
                        operation.piece(),
                        operation.rotation(),
                        operation.x(),
                    );
                    if self.buckets.last().is_none_or(|bucket| bucket.key != key) {
                        self.buckets.push(GeometryBucket {
                            key,
                            start: self.read as u32,
                            end: self.read as u32 + 1,
                        });
                    } else {
                        self.buckets.last_mut().unwrap().end += 1;
                    }
                    for row in 0..self.height {
                        let bits = operation.mask().row_bits(row);
                        if bits != 0 {
                            self.row_offsets[usize::from(row) + 1] += 1;
                        }
                        for x in 0..10_u8 {
                            if bits & (1 << x) != 0 {
                                self.cell_offsets[usize::from(row) * 10 + usize::from(x) + 1] += 1;
                            }
                        }
                    }
                    self.read += 1;
                }
            }
            Phase::Allocate => {
                if self.row_ids.len() < *self.row_offsets.last().unwrap() as usize {
                    self.row_ids.push(CatalogOperationId(0));
                } else if self.cell_ids.len() < *self.cell_offsets.last().unwrap() as usize {
                    self.cell_ids.push(CatalogOperationId(0));
                } else {
                    self.read = 0;
                    self.phase = Phase::Fill;
                }
            }
            Phase::Fill => {
                if self.read == self.operations.len() {
                    self.phase = Phase::Complete;
                    return Ok(true);
                }
                let operation = self.operations[self.read];
                let id = CatalogOperationId(self.read as u32);
                for row in 0..self.height {
                    let bits = operation.mask().row_bits(row);
                    if bits != 0 {
                        let cursor = &mut self.row_cursors[usize::from(row)];
                        self.row_ids[*cursor as usize] = id;
                        *cursor += 1;
                    }
                    for x in 0..10_u8 {
                        if bits & (1 << x) != 0 {
                            let cursor =
                                &mut self.cell_cursors[usize::from(row) * 10 + usize::from(x)];
                            self.cell_ids[*cursor as usize] = id;
                            *cursor += 1;
                        }
                    }
                }
                self.read += 1;
            }
            Phase::Complete => return Ok(true),
        }
        Ok(false)
    }

    pub(crate) fn finish(self) -> Option<LogicalOperationCatalog> {
        matches!(self.phase, Phase::Complete).then(|| LogicalOperationCatalog {
            height: self.height,
            operations: self.operations.into_boxed_slice(),
            piece_offsets: self.piece_offsets,
            row_index: OperationCsr {
                offsets: self.row_offsets.into_boxed_slice(),
                operation_ids: self.row_ids.into_boxed_slice(),
            },
            cell_index: OperationCsr {
                offsets: self.cell_offsets.into_boxed_slice(),
                operation_ids: self.cell_ids.into_boxed_slice(),
            },
            geometry_buckets: self.buckets.into_boxed_slice(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incremental_catalog_matches_every_index_of_the_recursive_catalog() {
        for height in [1, 2, 4, 8] {
            let inventory = PieceInventory::from_pieces(PieceKind::STANDARD_TETROMINOES).unwrap();
            let initial = StructureBoard::EMPTY.with_cell(3, 0).unwrap();
            let expected = LogicalOperationCatalog::compile(height, initial, inventory).unwrap();
            let mut cursor = CatalogCursor::new(height, initial, inventory).unwrap();
            while !cursor.step().unwrap() {}
            assert_eq!(cursor.finish().unwrap(), expected);
        }
    }
}
