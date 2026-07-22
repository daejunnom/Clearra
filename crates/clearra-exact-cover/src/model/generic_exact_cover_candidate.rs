use crate::builder::cell_universe_builder::CellUniverse;

use super::ExactCoverCandidate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericExactCoverCandidate {
    candidate_id: usize,
    piece_id: String,
    piece_area: usize,
    cells: Vec<u32>,
    columns: Vec<usize>,
}

impl GenericExactCoverCandidate {
    pub fn from_cells(
        candidate_id: usize,
        piece_id: impl Into<String>,
        piece_area: usize,
        cells: Vec<u32>,
        universe: &CellUniverse,
    ) -> Result<Self, GenericExactCoverCandidateError> {
        if cells.is_empty() {
            return Err(GenericExactCoverCandidateError::EmptyCells { candidate_id });
        }
        if piece_area == 0 {
            return Err(GenericExactCoverCandidateError::ZeroPieceArea { candidate_id });
        }
        if piece_area != cells.len() {
            return Err(
                GenericExactCoverCandidateError::PieceAreaDoesNotMatchCells {
                    candidate_id,
                    piece_area,
                    cell_count: cells.len(),
                },
            );
        }

        let columns = universe
            .compact_columns_for_cells(cells.iter().copied())
            .map_err(
                |cell| GenericExactCoverCandidateError::CellOutsideUniverse { candidate_id, cell },
            )?;

        Ok(Self {
            candidate_id,
            piece_id: piece_id.into(),
            piece_area,
            cells,
            columns,
        })
    }
}
impl GenericExactCoverCandidate {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }
}
impl GenericExactCoverCandidate {
    pub fn piece_id(&self) -> &str {
        &self.piece_id
    }
}
impl GenericExactCoverCandidate {
    pub fn piece_area(&self) -> usize {
        self.piece_area
    }
}
impl GenericExactCoverCandidate {
    pub fn cells(&self) -> &[u32] {
        &self.cells
    }
}
impl GenericExactCoverCandidate {
    pub fn columns(&self) -> &[usize] {
        &self.columns
    }
}
impl GenericExactCoverCandidate {
    pub fn to_exact_cover_candidate(&self) -> ExactCoverCandidate {
        ExactCoverCandidate::new(self.candidate_id, self.columns.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericExactCoverCandidateError {
    EmptyCells {
        candidate_id: usize,
    },
    ZeroPieceArea {
        candidate_id: usize,
    },
    PieceAreaDoesNotMatchCells {
        candidate_id: usize,
        piece_area: usize,
        cell_count: usize,
    },
    CellOutsideUniverse {
        candidate_id: usize,
        cell: u32,
    },
}

#[cfg(test)]
#[path = "generic_exact_cover_candidate_tests.rs"]
mod tests;
