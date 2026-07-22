use clearra_core_domain::board::cell::{CellCoord, CellCoordError};

use super::{board128_layout::Board128Layout, board64_layout::Board64Layout};

pub fn cell_index(layout: Board64Layout, coord: CellCoord) -> u8 {
    (u32::from(coord.y()) * u32::from(layout.width()) + u32::from(coord.x())) as u8
}

pub fn try_cell_index(layout: Board64Layout, x: u16, y: u16) -> Result<u8, CellCoordError> {
    let coord = CellCoord::new(x, y, layout.size())?;
    Ok(cell_index(layout, coord))
}

pub fn coord_for_index(layout: Board64Layout, index: u8) -> Option<CellCoord> {
    if index >= layout.cell_count() {
        return None;
    }

    let width = layout.width();
    let x = u16::from(index) % width;
    let y = u16::from(index) / width;
    Some(CellCoord::new_unchecked(x, y))
}

pub fn cell_index128(layout: Board128Layout, coord: CellCoord) -> u8 {
    (u32::from(coord.y()) * u32::from(layout.width()) + u32::from(coord.x())) as u8
}

pub fn try_cell_index128(layout: Board128Layout, x: u16, y: u16) -> Result<u8, CellCoordError> {
    let coord = CellCoord::new(x, y, layout.size())?;
    Ok(cell_index128(layout, coord))
}

pub fn coord_for_index128(layout: Board128Layout, index: u8) -> Option<CellCoord> {
    if index >= layout.cell_count() {
        return None;
    }

    let width = layout.width();
    let x = u16::from(index) % width;
    let y = u16::from(index) / width;
    Some(CellCoord::new_unchecked(x, y))
}

#[cfg(test)]
#[path = "cell_indexer_tests.rs"]
mod tests;
