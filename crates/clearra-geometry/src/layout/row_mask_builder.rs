use super::{
    board128_layout::Board128Layout,
    board64_layout::Board64Layout,
    cell_indexer::{try_cell_index, try_cell_index128},
};

pub fn row_mask(layout: Board64Layout, y: u16) -> Option<u64> {
    if y >= layout.height() {
        return None;
    }

    let width = layout.width();
    let start = try_cell_index(layout, 0, y).ok()?;
    let row_bits = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };

    Some(row_bits << start)
}

pub fn row_mask128(layout: Board128Layout, y: u16) -> Option<u128> {
    if y >= layout.height() {
        return None;
    }

    let width = layout.width();
    let start = try_cell_index128(layout, 0, y).ok()?;
    let row_bits = if width == 128 {
        u128::MAX
    } else {
        (1_u128 << width) - 1
    };

    Some(row_bits << start)
}

#[cfg(test)]
#[path = "row_mask_builder_tests.rs"]
mod tests;
