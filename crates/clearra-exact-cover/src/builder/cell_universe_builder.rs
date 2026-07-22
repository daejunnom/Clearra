use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellUniverse {
    cells: Vec<u32>,
    compact_by_cell: BTreeMap<u32, usize>,
}

impl CellUniverse {
    pub fn new(cells: Vec<u32>) -> Result<Self, CellUniverseBuilderError> {
        if cells.is_empty() {
            return Err(CellUniverseBuilderError::EmptyUniverse);
        }

        let mut compact_by_cell = BTreeMap::new();
        for (compact_index, cell) in cells.iter().copied().enumerate() {
            if compact_by_cell.insert(cell, compact_index).is_some() {
                return Err(CellUniverseBuilderError::DuplicateCell { cell });
            }
        }

        Ok(Self {
            cells,
            compact_by_cell,
        })
    }
}
impl CellUniverse {
    pub fn cells(&self) -> &[u32] {
        &self.cells
    }
}
impl CellUniverse {
    pub fn column_count(&self) -> usize {
        self.cells.len()
    }
}
impl CellUniverse {
    pub fn compact_column_for_cell(&self, cell: u32) -> Option<usize> {
        self.compact_by_cell.get(&cell).copied()
    }
}
impl CellUniverse {
    pub fn compact_columns_for_cells(
        &self,
        cells: impl IntoIterator<Item = u32>,
    ) -> Result<Vec<usize>, u32> {
        cells
            .into_iter()
            .map(|cell| self.compact_column_for_cell(cell).ok_or(cell))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CellUniverseBuilder;

impl CellUniverseBuilder {
    pub fn from_mask(mask: u64) -> Vec<usize> {
        (0..64)
            .filter(|index| (mask & (1_u64 << index)) != 0)
            .map(|index| index as usize)
            .collect()
    }
}
impl CellUniverseBuilder {
    pub fn universe_from_mask(mask: u64) -> Result<CellUniverse, CellUniverseBuilderError> {
        let cells = Self::from_mask(mask)
            .into_iter()
            .map(|cell| cell as u32)
            .collect();
        CellUniverse::new(cells)
    }
}
impl CellUniverseBuilder {
    pub fn universe_from_cells(
        cells: impl IntoIterator<Item = u32>,
    ) -> Result<CellUniverse, CellUniverseBuilderError> {
        CellUniverse::new(cells.into_iter().collect())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CellUniverseBuilderError {
    EmptyUniverse,
    DuplicateCell { cell: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_universe_builder_remaps_sparse_cells_to_compact_columns() {
        let universe = CellUniverseBuilder::universe_from_cells([4, 9, 20]).expect("universe");

        assert_eq!(universe.cells(), &[4, 9, 20]);
        assert_eq!(universe.column_count(), 3);
        assert_eq!(universe.compact_column_for_cell(9), Some(1));
        assert_eq!(
            universe
                .compact_columns_for_cells([20, 4])
                .expect("columns"),
            vec![2, 0]
        );
        assert_eq!(universe.compact_columns_for_cells([8]), Err(8));
    }
}
