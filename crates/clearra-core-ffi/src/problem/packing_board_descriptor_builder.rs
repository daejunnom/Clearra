use clearra_core_domain::board::board_size::BoardSize;
use clearra_geometry::layout::board_backend::{backend_kind_for_size, BoardBackendKind};
use clearra_problem::{SearchProblem, SearchProblemPreset};

use crate::board::{
    C_BOARD_BACKEND_BOARD128, C_BOARD_BACKEND_BOARD256, C_BOARD_BACKEND_BOARD64,
    C_BOARD_BACKEND_WIDE,
};

use super::{CBoardDescriptor, FfiProblemError};

pub(crate) fn board_descriptor(
    problem: &SearchProblem,
) -> Result<CBoardDescriptor, FfiProblemError> {
    let width = problem.initial_board().width();
    let packing_height = packing_board_height(problem);
    let size =
        BoardSize::new(width, packing_height).map_err(|_| FfiProblemError::InvalidBoardLayout {
            width,
            height: packing_height,
        })?;
    let cell_count = size.area();
    let backend_kind = backend_kind_for_size(size);
    if backend_kind != BoardBackendKind::Board64 {
        return Err(FfiProblemError::UnsupportedBoardBackend {
            backend_kind: board_backend_name(backend_kind),
            cell_count,
        });
    }

    Ok(CBoardDescriptor {
        width,
        visible_height: packing_height,
        search_height: packing_height,
        reserved: 0,
        initial_mask: problem.initial_board().occupied_mask(),
        initial_mask_hi: 0,
        backend_kind: board_backend_code(backend_kind),
        cell_count,
    })
}

pub(crate) fn packing_board_height(problem: &SearchProblem) -> u16 {
    let width = usize::from(problem.initial_board().width()).max(1);
    let visible_height = usize::from(problem.visible_height()).max(1);
    let rows = active_packing_rows(
        problem,
        width,
        visible_height,
        problem.initial_board().occupied_mask(),
    );
    u16::try_from(rows).unwrap_or(problem.visible_height())
}

pub(crate) fn active_packing_rows(
    problem: &SearchProblem,
    width: usize,
    visible_height: usize,
    initial_mask: u64,
) -> usize {
    match problem.preset() {
        SearchProblemPreset::OpeningPc => visible_height,
        SearchProblemPreset::ScenarioPc
        | SearchProblemPreset::Setup
        | SearchProblemPreset::Build => {
            let piece_count = problem
                .exact_pieces()
                .unwrap_or_else(|| problem.piece_window().max_pieces());
            let occupied_cells = initial_mask.count_ones() as usize;
            let cells_after_pack = occupied_cells.saturating_add(piece_count.saturating_mul(4));
            let rows_by_area = cells_after_pack.div_ceil(width).max(1);
            highest_occupied_row(initial_mask, width)
                .map(|row| rows_by_area.max(row + 1))
                .unwrap_or(rows_by_area)
                .min(visible_height)
                .max(1)
        }
    }
}

pub(crate) fn low_mask(bit_count: usize) -> u64 {
    if bit_count >= 64 {
        u64::MAX
    } else {
        (1_u64 << bit_count) - 1
    }
}

fn highest_occupied_row(mask: u64, width: usize) -> Option<usize> {
    if mask == 0 {
        return None;
    }
    let highest_bit = usize::try_from(63 - mask.leading_zeros()).expect("u32 fits usize");
    Some(highest_bit / width)
}

fn board_backend_code(kind: BoardBackendKind) -> u32 {
    match kind {
        BoardBackendKind::Board64 => C_BOARD_BACKEND_BOARD64,
        BoardBackendKind::Board128 => C_BOARD_BACKEND_BOARD128,
        BoardBackendKind::Board256 => C_BOARD_BACKEND_BOARD256,
        BoardBackendKind::Wide => C_BOARD_BACKEND_WIDE,
    }
}

fn board_backend_name(kind: BoardBackendKind) -> &'static str {
    match kind {
        BoardBackendKind::Board64 => "board64",
        BoardBackendKind::Board128 => "board128",
        BoardBackendKind::Board256 => "board256",
        BoardBackendKind::Wide => "wide",
    }
}
