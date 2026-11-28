//! Negative-only predicates on the selected rows of one geometric candidate.
//!
//! Future rows are relaxed, not solved. A pass never certifies a physical
//! order, a kick, a queue, coverage, or execution multiplicity.

use super::{GeometryCatalog, MAX_BOARD64_PIECES};

pub(super) use super::super::prefix_dependency::selected_compact_prefix_stalls as selected_prefix_stalls;

/// Before the first clear every lock has the original row frame. Give all
/// unselected cells to the initial board and close selected locks under every
/// possible supporter. Failure to fill any target row is a negative proof.
pub(super) fn selected_first_clear_stalls(catalog: &GeometryCatalog, rows: &[u32]) -> bool {
    let width = usize::from(catalog.width());
    let height = usize::from(catalog.height());
    if rows.is_empty()
        || rows.len() > MAX_BOARD64_PIECES
        || width == 0
        || height == 0
        || height > 16
        || width * height > 64
    {
        return false;
    }
    let full = u64::MAX >> (64 - width * height);
    let target = catalog.initial_board() | catalog.required_cells();
    if target & !full != 0 {
        return false;
    }
    let floor = u64::MAX >> (64 - width);
    let mut completed_rows = 0_u16;
    for row in 0..height {
        if (target >> (row * width)) & floor == floor {
            completed_rows |= 1 << row;
        }
    }
    if completed_rows == 0 {
        return false;
    }
    let mut cells = [0_u64; MAX_BOARD64_PIECES];
    let mut contributors = [0_u16; 16];
    let mut owners = [u8::MAX; 64];
    let mut selected = 0_u64;
    for (operation, &row_id) in rows.iter().enumerate() {
        let Some(skeleton) = catalog.try_skeleton(row_id) else {
            return false;
        };
        if skeleton.cells.count_ones() != 4
            || skeleton.cells & !catalog.required_cells() != 0
            || skeleton.cells & (selected | catalog.initial_board()) != 0
        {
            return false;
        }
        cells[operation] = skeleton.cells;
        selected |= skeleton.cells;
        let mut occupied = catalog.skeleton_occupied_rows(row_id);
        while occupied != 0 {
            let row = occupied.trailing_zeros() as usize;
            occupied &= occupied - 1;
            contributors[row] |= 1 << operation;
        }
        let mut occupied = skeleton.cells;
        while occupied != 0 {
            let cell = occupied.trailing_zeros() as usize;
            occupied &= occupied - 1;
            owners[cell] = operation as u8;
        }
    }
    let mut pending_rows = completed_rows;
    while pending_rows != 0 {
        let row = pending_rows.trailing_zeros() as usize;
        pending_rows &= pending_rows - 1;
        if contributors[row] == 0 {
            return false;
        }
    }
    let optimistic_board = catalog.initial_board() | (target & !selected);
    let mut dependents = [0_u16; MAX_BOARD64_PIECES];
    let mut active = 0_u16;
    for (operation, &row_id) in rows.iter().enumerate() {
        if !has_frame_zero_realization(catalog, row_id) {
            continue;
        }
        let mask = cells[operation];
        let bottom = mask.checked_shr(width as u32).unwrap_or(0) & !mask;
        if mask & floor != 0 || bottom & optimistic_board != 0 {
            active |= 1 << operation;
        }
        let mut supporters = bottom & selected;
        while supporters != 0 {
            let cell = supporters.trailing_zeros() as usize;
            supporters &= supporters - 1;
            dependents[usize::from(owners[cell])] |= 1 << operation;
        }
    }
    let mut pending = active;
    while pending != 0 {
        let supporter = pending.trailing_zeros() as usize;
        pending &= pending - 1;
        let added = dependents[supporter] & !active;
        active |= added;
        pending |= added;
    }
    while completed_rows != 0 {
        let row = completed_rows.trailing_zeros() as usize;
        completed_rows &= completed_rows - 1;
        if contributors[row] & !active == 0 {
            return false;
        }
    }
    true
}

fn has_frame_zero_realization(catalog: &GeometryCatalog, row_id: u32) -> bool {
    let Some(row) = catalog.try_skeleton(row_id) else {
        return false;
    };
    if let Some(poses) = catalog.instantiated_realizations(row_id, 0) {
        return poses.iter().any(|pose| pose.lock_mask == row.cells);
    }
    catalog.realizations(row_id).iter().any(|&realization| {
        catalog
            .instantiate_realization(row.cells, realization, 0)
            .is_some_and(|pose| pose.lock_mask == row.cells)
    })
}
