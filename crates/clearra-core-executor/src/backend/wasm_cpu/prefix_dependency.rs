//! A negative-only ILC dependency closure for every supported field height.
//!
//! Unselected operations are optimistically already present. A completed
//! target row can be deleted once all its selected contributors have been
//! placed. Every real placement order is contained in this relaxation, so a
//! stalled closure proves impossibility. Success provides no movement,
//! grounding, queue, coverage, or execution-multiplicity evidence.

use super::{
    catalog::GeometryCatalog, extended_inverse_catalog::ExtendedInverseCatalog, MAX_BOARD64_PIECES,
};

const MAX_HEIGHT: usize = 24;
const MAX_OPERATIONS: usize = 60;

#[derive(Clone, Copy)]
enum PrefixCatalog<'a> {
    Compact(&'a GeometryCatalog),
    FullHeight(&'a ExtendedInverseCatalog),
}

impl PrefixCatalog<'_> {
    fn dimensions(self) -> (u8, u8, usize) {
        match self {
            Self::Compact(catalog) => (catalog.width(), catalog.height(), MAX_BOARD64_PIECES),
            Self::FullHeight(catalog) => (catalog.width(), catalog.height(), MAX_OPERATIONS),
        }
    }

    fn occupied_rows(self, row_id: u32) -> Option<u32> {
        match self {
            Self::Compact(catalog) => {
                catalog.try_skeleton(row_id)?;
                Some(u32::from(catalog.skeleton_occupied_rows(row_id)))
            }
            Self::FullHeight(catalog) => {
                catalog.skeletons().get(row_id as usize)?;
                Some(catalog.skeleton_occupied_rows(row_id))
            }
        }
    }

    fn completed_rows(self, width: u8, height: u8) -> u32 {
        let row_mask = u64::MAX >> (64 - width);
        let mut completed = 0_u32;
        match self {
            Self::Compact(catalog) => {
                let target = catalog.initial_board() | catalog.required_cells();
                for row in 0..height {
                    if (target >> (u32::from(row) * u32::from(width))) & row_mask == row_mask {
                        completed |= 1 << row;
                    }
                }
            }
            Self::FullHeight(catalog) => {
                let target = catalog.initial_board().union(catalog.required_cells());
                for row in 0..height {
                    if u64::from(target.row_bits(width, row)) == row_mask {
                        completed |= 1 << row;
                    }
                }
            }
        }
        completed
    }

    fn requirement_is_satisfied(self, row_id: u32, deleted_rows: u32) -> bool {
        match self {
            Self::Compact(catalog) => {
                catalog.realization_requirement_is_satisfied(row_id, deleted_rows as u16)
            }
            Self::FullHeight(catalog) => {
                catalog.realization_requirement_is_satisfied(row_id, deleted_rows)
            }
        }
    }
}

pub(super) fn selected_compact_prefix_stalls(catalog: &GeometryCatalog, rows: &[u32]) -> bool {
    selected_prefix_stalls(PrefixCatalog::Compact(catalog), rows)
}

pub(super) fn selected_full_height_prefix_stalls(
    catalog: &ExtendedInverseCatalog,
    rows: &[u32],
) -> bool {
    selected_prefix_stalls(PrefixCatalog::FullHeight(catalog), rows)
}

fn selected_prefix_stalls(catalog: PrefixCatalog<'_>, rows: &[u32]) -> bool {
    let (width, height, operation_limit) = catalog.dimensions();
    if rows.is_empty()
        || rows.len() > operation_limit
        || height == 0
        || usize::from(height) > MAX_HEIGHT
        || width == 0
        || width > 64
        || matches!(catalog, PrefixCatalog::FullHeight(_)) && width > 10
        || matches!(catalog, PrefixCatalog::Compact(_))
            && (height > 16 || u16::from(width) * u16::from(height) > 64)
    {
        return false;
    }
    let mut contributors = [0_u64; MAX_HEIGHT];
    for (operation, &row_id) in rows.iter().enumerate() {
        let Some(mut occupied) = catalog.occupied_rows(row_id) else {
            return false;
        };
        while occupied != 0 {
            let row = occupied.trailing_zeros() as usize;
            occupied &= occupied - 1;
            let Some(contributors) = contributors[..usize::from(height)].get_mut(row) else {
                return false;
            };
            *contributors |= 1_u64 << operation;
        }
    }
    dependency_closure_stalls(
        &contributors[..usize::from(height)],
        catalog.completed_rows(width, height),
        rows.len(),
        |operation, deleted| catalog.requirement_is_satisfied(rows[operation], deleted),
    )
}

fn dependency_closure_stalls(
    contributors: &[u64],
    completed_rows: u32,
    operation_count: usize,
    mut requirement_is_satisfied: impl FnMut(usize, u32) -> bool,
) -> bool {
    let all = u64::MAX >> (64 - operation_count);
    let mut placed = 0_u64;
    while placed != all {
        let mut deleted = 0_u32;
        for (row, contributors) in contributors.iter().copied().enumerate() {
            if completed_rows & (1 << row) != 0 && contributors & !placed == 0 {
                deleted |= 1 << row;
            }
        }
        let mut ready = 0_u64;
        let mut remaining = all & !placed;
        while remaining != 0 {
            let operation = remaining.trailing_zeros() as usize;
            let bit = 1_u64 << operation;
            remaining &= remaining - 1;
            if requirement_is_satisfied(operation, deleted) {
                ready |= bit;
            }
        }
        if ready == 0 {
            return true;
        }
        placed |= ready;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::wasm_cpu::extended_board::ExtendedBoard;
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_problem::BuildProbabilityField;

    fn shifted(mask: u64, offset: u8) -> ExtendedBoard {
        let mut result = ExtendedBoard::EMPTY;
        for cell in 0..40 {
            if mask & (1_u64 << cell) != 0 {
                result.insert(cell + u16::from(offset) * 10);
            }
        }
        result
    }

    #[test]
    fn shared_prefix_dependency_preserves_valid_and_cyclic_fields_across_words() {
        for (cells, cyclic) in [
            ([3 | (3 << 10), (12 << 20) | (12 << 30)], false),
            // The first O needs row 1 cleared, but the second O contributes
            // to row 1 and itself needs row 2, which contains the first O.
            ([3 | (3 << 20), (12 << 10) | (12 << 30)], true),
        ] {
            let required = cells[0] | cells[1];
            let initial = ((1_u64 << 40) - 1) & !required;
            let compact =
                GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, initial, required)
                    .unwrap();
            let compact_rows = cells.map(|cells| compact.skeleton_id(PieceKind::O, cells).unwrap());
            for (height, offset) in [(7, 0), (7, 3), (13, 9), (24, 20)] {
                let field = BuildProbabilityField::from_words_preserving_height(
                    height,
                    shifted(initial, offset).words(),
                    shifted(required, offset).words(),
                )
                .unwrap();
                let full = ExtendedInverseCatalog::compile(field).unwrap();
                let full_rows = cells.map(|cells| {
                    full.skeletons()
                        .iter()
                        .position(|row| {
                            row.piece == PieceKind::O && row.cells == shifted(cells, offset)
                        })
                        .unwrap() as u32
                });
                for selected in [vec![], vec![0], vec![1], vec![0, 1]] {
                    let compact_rows = selected
                        .iter()
                        .map(|&index| compact_rows[index])
                        .collect::<Vec<_>>();
                    let full_rows = selected
                        .iter()
                        .map(|&index| full_rows[index])
                        .collect::<Vec<_>>();
                    let expected = cyclic && selected.len() == 2;
                    assert_eq!(
                        selected_compact_prefix_stalls(&compact, &compact_rows),
                        expected
                    );
                    assert_eq!(
                        selected_full_height_prefix_stalls(&full, &full_rows),
                        expected,
                        "height={height}, offset={offset}, cyclic={cyclic}, selected={selected:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn shared_prefix_dependency_handles_sixty_operations_and_high_rows() {
        for (first_row, second_row) in [(0, 1), (20, 23)] {
            let mut contributors = [0_u64; MAX_HEIGHT];
            contributors[first_row] = 1_u64 << 59;
            contributors[second_row] = (1_u64 << 59) - 1;
            let completed = (1_u32 << first_row) | (1_u32 << second_row);
            for cyclic in [false, true] {
                assert_eq!(
                    dependency_closure_stalls(
                        &contributors,
                        completed,
                        60,
                        |operation, deleted| {
                            let required = if operation == 59 {
                                if cyclic {
                                    1 << second_row
                                } else {
                                    0
                                }
                            } else {
                                1 << first_row
                            };
                            required & !deleted == 0
                        }
                    ),
                    cyclic
                );
            }
        }
    }
}
