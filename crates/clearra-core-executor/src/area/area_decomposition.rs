use std::collections::BTreeSet;

use clearra_core_domain::board::board_size::BoardSize;

use crate::board::board_state_backend::BoardStateBackend;

use super::area_component::{AreaComponent, RegionKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AreaScope {
    AllCells,
    RowsBelow { rows: u16 },
    Cells(BTreeSet<u32>),
}

impl AreaScope {
    pub fn all_cells() -> Self {
        Self::AllCells
    }
}
impl AreaScope {
    pub fn rows_below(rows: u16) -> Self {
        Self::RowsBelow { rows }
    }
}
impl AreaScope {
    pub fn cells(cells: impl IntoIterator<Item = u32>) -> Self {
        Self::Cells(cells.into_iter().collect())
    }
}
impl AreaScope {
    pub fn contains(&self, cell_index: u32, size: BoardSize) -> bool {
        if cell_index >= size.area() {
            return false;
        }

        match self {
            Self::AllCells => true,
            Self::RowsBelow { rows } => {
                let width = u32::from(size.width());
                let y = cell_index / width;
                y < u32::from((*rows).min(size.height()))
            }
            Self::Cells(cells) => cells.contains(&cell_index),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreaDecomposition {
    kind: RegionKind,
    scope: AreaScope,
    components: Vec<AreaComponent>,
}

impl AreaDecomposition {
    pub fn new(kind: RegionKind, scope: AreaScope, components: Vec<AreaComponent>) -> Self {
        Self {
            kind,
            scope,
            components,
        }
    }
}
impl AreaDecomposition {
    pub fn kind(&self) -> RegionKind {
        self.kind
    }
}
impl AreaDecomposition {
    pub fn scope(&self) -> &AreaScope {
        &self.scope
    }
}
impl AreaDecomposition {
    pub fn components(&self) -> &[AreaComponent] {
        &self.components
    }
}
impl AreaDecomposition {
    pub fn component_areas(&self) -> Vec<usize> {
        self.components.iter().map(AreaComponent::area).collect()
    }
}

pub struct AreaDecomposer;

impl AreaDecomposer {
    pub fn decompose<B: BoardStateBackend>(board: &B, kind: RegionKind) -> AreaDecomposition {
        Self::decompose_in_scope(board, kind, AreaScope::all_cells())
    }
}
impl AreaDecomposer {
    pub fn decompose_in_scope<B: BoardStateBackend>(
        board: &B,
        kind: RegionKind,
        scope: AreaScope,
    ) -> AreaDecomposition {
        let size = board.size();
        let cell_count = size.area();
        let mut visited = vec![false; cell_count as usize];
        let mut components = Vec::new();

        for cell_index in 0..cell_count {
            let visited_index = cell_index as usize;
            if visited[visited_index] {
                continue;
            }
            if !scope.contains(cell_index, size) || !is_cell_in_region(board, cell_index, kind) {
                visited[visited_index] = true;
                continue;
            }

            let mut cells = Vec::new();
            let mut stack = vec![cell_index];
            visited[visited_index] = true;

            while let Some(current) = stack.pop() {
                cells.push(current);

                for neighbor in neighbors(current, size) {
                    let neighbor_index = neighbor as usize;
                    if visited[neighbor_index] {
                        continue;
                    }
                    if !scope.contains(neighbor, size) || !is_cell_in_region(board, neighbor, kind)
                    {
                        visited[neighbor_index] = true;
                        continue;
                    }

                    visited[neighbor_index] = true;
                    stack.push(neighbor);
                }
            }

            components.push(AreaComponent::new(kind, cells));
        }

        AreaDecomposition::new(kind, scope, components)
    }
}
impl AreaDecomposer {
    pub fn empty_components<B: BoardStateBackend>(board: &B) -> AreaDecomposition {
        Self::decompose(board, RegionKind::Empty)
    }
}
impl AreaDecomposer {
    pub fn occupied_components<B: BoardStateBackend>(board: &B) -> AreaDecomposition {
        Self::decompose(board, RegionKind::Occupied)
    }
}

fn is_cell_in_region<B: BoardStateBackend>(board: &B, cell_index: u32, kind: RegionKind) -> bool {
    let Some(mask) = board.singleton_mask(cell_index) else {
        return false;
    };
    let occupied = board.collides_mask(&mask);
    match kind {
        RegionKind::Empty => !occupied,
        RegionKind::Occupied => occupied,
    }
}

fn neighbors(cell_index: u32, size: BoardSize) -> Vec<u32> {
    let width = u32::from(size.width());
    let height = u32::from(size.height());
    let x = cell_index % width;
    let y = cell_index / width;
    let mut result = Vec::with_capacity(4);

    if x > 0 {
        result.push(cell_index - 1);
    }
    if x + 1 < width {
        result.push(cell_index + 1);
    }
    if y > 0 {
        result.push(cell_index - width);
    }
    if y + 1 < height {
        result.push(cell_index + width);
    }

    result
}
