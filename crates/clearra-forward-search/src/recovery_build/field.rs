//! Build targets keep their logical rows after physical line clears. A lock is
//! lifted through the ACTUAL deletion history, never through one sample replay.
use clearra_core_domain::board::standard_pc_board::Board256Mask;
use crate::board::{place_and_clear, ForwardBoard};
use super::RecoveryBuildError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryBuildFields {
    pub height: u8,
    pub initial: Board256Mask,
    /// Cells to add to `initial`, in the first Build's logical frame.
    pub middle: Board256Mask,
    /// Cells to add AFTER the first Build's completed rows are removed.
    pub result: Board256Mask,
}

#[derive(Clone, Debug)]
pub(super) struct PreparedFields {
    pub height: u8,
    pub initial: ForwardBoard,
    pub middle: Vec<u16>,
    pub result: Vec<u16>,
    pub initially_deleted: Vec<bool>,
    pub terminal: ForwardBoard,
    pub middle_pieces: usize,
    pub result_pieces: usize,
}

fn rows(mask: Board256Mask, height: u8) -> Vec<u16> {
    let board = ForwardBoard::from_mask(mask);
    (0..height).map(|y| board.row_bits(10, y)).collect()
}
fn area(rows: &[u16]) -> usize {
    rows.iter().map(|row| row.count_ones() as usize).sum()
}
fn compact(rows: &[u16]) -> Vec<u16> {
    rows.iter().copied().filter(|row| *row != 1023).collect()
}
fn board(rows: &[u16], height: u8) -> ForwardBoard {
    let mut result = ForwardBoard::EMPTY;
    for (y, &row) in rows.iter().take(usize::from(height)).enumerate() {
        for x in 0..10 {
            if row & (1 << x) != 0 { result.insert((y * 10 + x) as u16); }
        }
    }
    result
}

impl RecoveryBuildFields {
    pub(super) fn prepare(&self) -> Result<PreparedFields, RecoveryBuildError> {
        if !(1..=24).contains(&self.height) { return Err(RecoveryBuildError::InvalidHeight); }
        if [self.initial, self.middle, self.result].iter()
            .any(|mask| mask.fits_cell_count(u16::from(self.height) * 10) != Ok(true)) {
            return Err(RecoveryBuildError::BoardOutsideField);
        }
        let initial = rows(self.initial, self.height);
        let mut middle = rows(self.middle, self.height);
        let target = rows(self.result, self.height);
        if initial.iter().zip(&middle).any(|(a, b)| a & b != 0) {
            return Err(RecoveryBuildError::MiddleOverlapsStart);
        }
        let first: Vec<_> = initial.iter().zip(&middle).map(|(a, b)| a | b).collect();
        let mut after_first = compact(&first);
        after_first.resize(usize::from(self.height), 0);
        if after_first.iter().zip(&target).any(|(a, b)| a & b != 0) {
            return Err(RecoveryBuildError::ResultOverlapsRetainedMiddle);
        }
        let (middle_area, result_area) = (area(&middle), area(&target));
        if middle_area == 0 || result_area == 0 || middle_area % 4 != 0 || result_area % 4 != 0 {
            return Err(RecoveryBuildError::TargetAreaNotTetrominoes);
        }
        // Insert the first Build's completed logical rows in the second frame.
        // Logical height may exceed physical height: do not clip or reject it.
        let mut lifted = Vec::new();
        let mut input_row = 0;
        let mut logical_row = 0;
        while input_row < target.len() {
            if first.get(logical_row) == Some(&1023) { lifted.push(0); }
            else { lifted.push(target[input_row]); input_row += 1; }
            logical_row += 1;
        }
        let logical_height = middle.len().max(lifted.len());
        middle.resize(logical_height, 0);
        lifted.resize(logical_height, 0);
        let initially_deleted = (0..logical_height).map(|row| initial.get(row) == Some(&1023)).collect();
        let (initial, _, _) = place_and_clear(10, self.height, ForwardBoard::from_mask(self.initial));
        let final_rows: Vec<_> = after_first.iter().zip(&target).map(|(a, b)| a | b).collect();
        Ok(PreparedFields { height: self.height, initial, middle, result: lifted,
            initially_deleted, terminal: board(&compact(&final_rows), self.height),
            middle_pieces: middle_area / 4, result_pieces: result_area / 4 })
    }
}

impl PreparedFields {
    /// Maps a legal PHYSICAL lock to the undeleted logical row positions.
    /// Failure means the lock lies outside both declared target regions.
    pub fn lift(&self, lock: ForwardBoard, deleted: &[bool]) -> Option<Vec<u16>> {
        let mut logical = vec![0_u16; self.middle.len()];
        let map: Vec<_> = deleted.iter().enumerate().filter_map(|(row, gone)| (!gone).then_some(row)).collect();
        for y in 0..self.height {
            let bits = lock.row_bits(10, y);
            if bits == 0 { continue; }
            let row = *map.get(usize::from(y))?;
            logical[row] = bits;
        }
        Some(logical)
    }
    pub fn delete_rows(&self, physical_rows: u32, deleted: &[bool]) -> Vec<bool> {
        let mut next = deleted.to_vec();
        for (physical, logical) in deleted.iter().enumerate().filter_map(|(row, gone)| (!gone).then_some(row)).enumerate() {
            if physical < 32 && physical_rows & (1_u32 << physical) != 0 { next[logical] = true; }
        }
        next
    }
}

pub(super) fn available(lock: &[u16], target: &[u16], used: &[u16]) -> bool {
    lock.iter().zip(target).zip(used).all(|((&cells, &required), &filled)| cells & !required == 0 && cells & filled == 0)
}
pub(super) fn joined(left: &[u16], right: &[u16]) -> Vec<u16> {
    left.iter().zip(right).map(|(a, b)| a | b).collect()
}
