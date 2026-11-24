use super::*;
use crate::SpinStructureError;
use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    parallel_work::{try_for_each_chunk, ParallelWorkError},
};
use std::sync::Mutex;

impl LogicalOperationCatalog {
    /// A geometry key owns one independent monotone row-projection domain.
    /// Sorting those domains on their workers preserves the exact global
    /// operation IDs while avoiding a single large catalog-generation task.
    pub(crate) fn compile_with_control(
        height: u8,
        initial_board: StructureBoard,
        inventory: PieceInventory,
        control: &ExecutionControl,
    ) -> Result<Self, SpinStructureError> {
        if height == 0
            || height > StructureBoard::MAX_HEIGHT
            || initial_board.has_cells_at_or_above(height)
        {
            return Err(catalog_error());
        }
        let registry = standard_tetromino_registry();
        let mut geometry = Vec::new();
        for piece in inventory.available() {
            let definition = registry.get(piece).expect("standard tetromino exists");
            for rotation in RotationState::ALL {
                if canonical_geometry_rotation(piece, rotation) != rotation {
                    continue;
                }
                let cells = definition.shape(rotation).cells();
                let minimum_x = cells.iter().map(|cell| cell.x()).min().unwrap();
                let maximum_x = cells.iter().map(|cell| cell.x()).max().unwrap();
                for x in -i16::from(minimum_x)
                    ..=i16::from(StructureBoard::WIDTH - 1) - i16::from(maximum_x)
                {
                    geometry.push(OperationGeometryKey::new(piece, rotation, x as i8));
                }
            }
        }
        geometry.sort_unstable();
        let rows = (0..geometry.len())
            .map(|_| Mutex::new(None))
            .collect::<Vec<_>>();
        try_for_each_chunk(
            control,
            geometry.len(),
            1,
            || Ok::<_, SpinStructureError>(()),
            |_, range| {
                for index in range {
                    let key = geometry[index];
                    let cells = registry.get(key.piece).unwrap().shape(key.rotation).cells();
                    let mut local_rows = cells.map(|cell| cell.y());
                    local_rows.sort_unstable();
                    let count = deduplicate_sorted_rows(&mut local_rows);
                    let mut operations = Vec::new();
                    enumerate_row_projections(
                        height,
                        initial_board,
                        key.piece,
                        key.rotation,
                        cells,
                        &local_rows[..count],
                        &mut [0; 4],
                        0,
                        key.x,
                        &mut operations,
                    );
                    operations.sort_unstable();
                    operations.dedup();
                    if control.is_cancelled() {
                        return Err(SpinStructureError::Cancelled);
                    }
                    *rows[index].lock().map_err(|_| catalog_error())? = Some(operations);
                }
                Ok(())
            },
        )
        .map_err(parallel_error)?;

        let mut parts = Vec::with_capacity(rows.len());
        let mut length = 0_usize;
        for row in rows {
            let row = row
                .into_inner()
                .map_err(|_| catalog_error())?
                .ok_or_else(catalog_error)?;
            length = length.checked_add(row.len()).ok_or_else(catalog_error)?;
            if length > u32::MAX as usize {
                return Err(catalog_error());
            }
            parts.push(row);
        }
        let mut operations = Vec::with_capacity(length);
        for mut part in parts {
            if control.is_cancelled() {
                return Err(SpinStructureError::Cancelled);
            }
            operations.append(&mut part);
        }
        let piece_offsets = compile_piece_offsets(&operations);
        let geometry_buckets =
            compile_geometry_buckets(&operations).map_err(|_| catalog_error())?;
        let indices = [Mutex::new(None), Mutex::new(None)];
        try_for_each_chunk(
            control,
            2,
            1,
            || Ok::<_, SpinStructureError>(()),
            |_, range| {
                for index in range {
                    let csr = if index == 0 {
                        compile_row_index(height, &operations)
                    } else {
                        compile_cell_index(height, &operations)
                    }
                    .map_err(|_| catalog_error())?;
                    *indices[index].lock().map_err(|_| catalog_error())? = Some(csr);
                }
                Ok(())
            },
        )
        .map_err(parallel_error)?;
        let [row_index, cell_index] = indices;
        Ok(Self {
            height,
            operations: operations.into_boxed_slice(),
            piece_offsets,
            geometry_buckets,
            row_index: row_index
                .into_inner()
                .map_err(|_| catalog_error())?
                .ok_or_else(catalog_error)?,
            cell_index: cell_index
                .into_inner()
                .map_err(|_| catalog_error())?
                .ok_or_else(catalog_error)?,
        })
    }
}

fn catalog_error() -> SpinStructureError {
    SpinStructureError::ParallelExecutionFailed("spin_catalog_prepare_failed")
}
fn parallel_error(error: ParallelWorkError<SpinStructureError>) -> SpinStructureError {
    match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => SpinStructureError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed => {
            SpinStructureError::ParallelExecutionFailed("spin_catalog_worker_spawn_failed")
        }
        ParallelWorkError::WorkerPanicked => {
            SpinStructureError::ParallelExecutionFailed("spin_catalog_worker_panicked")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geometry_domains_preserve_all_catalog_ids_and_adjacency_for_every_worker_count() {
        let inventory = PieceInventory::from_pieces(PieceKind::STANDARD_TETROMINOES).unwrap();
        for height in [4, 8, 12] {
            let board = StructureBoard::from_rows(&[0x2d0]).unwrap();
            let expected = LogicalOperationCatalog::compile(height, board, inventory).unwrap();
            for workers in [1, 2, 4, 8] {
                let control = ExecutionControl::default().with_parallelism(workers);
                assert_eq!(
                    LogicalOperationCatalog::compile_with_control(
                        height, board, inventory, &control
                    )
                    .unwrap(),
                    expected
                );
            }
        }
    }
}
