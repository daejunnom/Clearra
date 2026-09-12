//! Path-local correspondence between compact physical rows and original rows.
// SRP rationale: only row identity across clears is owned here. This module
// does not decide reachability, graph completeness, candidates, or probability.

const WIDTH: u32 = 10;
const ROW: u64 = (1 << WIDTH) - 1;
const ROWS: u8 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4RowFrameError {
    ClearedPrefixOutsideDomain,
    PhysicalCellsOutsideRemainingRows,
    ClearMaskOutsideRemainingRows,
}

/// Original row IDs are never reassigned when a graph field normalizes its
/// cleared rows to the bottom. Keep one frame per concrete path, not per node:
/// converging graph paths can have different original-row correspondences.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4RowFrame {
    surviving_original_rows: [u8; 4],
    remaining: u8,
}

impl Pc4RowFrame {
    /// The request's original frame is its starting normalized graph frame.
    pub fn new(cleared_bottom_prefix: u8) -> Result<Self, Pc4RowFrameError> {
        if cleared_bottom_prefix > ROWS {
            return Err(Pc4RowFrameError::ClearedPrefixOutsideDomain);
        }
        let mut surviving_original_rows = [0; 4];
        let remaining = ROWS - cleared_bottom_prefix;
        for row in 0..remaining {
            surviving_original_rows[usize::from(row)] = cleared_bottom_prefix + row;
        }
        Ok(Self {
            surviving_original_rows,
            remaining,
        })
    }

    pub const fn remaining_rows(self) -> u8 {
        self.remaining
    }

    /// Lift a physical pre-clear placement into the request's fixed frame.
    /// This is not a graph-normalized mask and must not be recomputed by
    /// subtracting consecutive graph hashes.
    pub fn lift_physical_cells(self, cells: u64) -> Result<u64, Pc4RowFrameError> {
        self.check_cells(cells)?;
        let mut lifted = 0;
        for physical in 0..self.remaining {
            let row = (cells >> (u32::from(physical) * WIDTH)) & ROW;
            let original = self.surviving_original_rows[usize::from(physical)];
            lifted |= row << (u32::from(original) * WIDTH);
        }
        Ok(lifted)
    }

    /// Apply a physical line-clear mask after lifting the current placement.
    /// Returning a new frame leaves the old branch available for rollback.
    pub fn after_clear(self, physical_rows: u8) -> Result<Self, Pc4RowFrameError> {
        if u16::from(physical_rows) >= (1_u16 << self.remaining) {
            return Err(Pc4RowFrameError::ClearMaskOutsideRemainingRows);
        }
        let mut next = Self {
            surviving_original_rows: [0; 4],
            remaining: 0,
        };
        for physical in 0..self.remaining {
            if physical_rows & (1 << physical) == 0 {
                next.surviving_original_rows[usize::from(next.remaining)] =
                    self.surviving_original_rows[usize::from(physical)];
                next.remaining += 1;
            }
        }
        Ok(next)
    }

    /// Reconstruct the graph's bottom-filled normalized mask from an already
    /// compacted physical board. Horizontal hash bit reversal is separate.
    pub fn normalized_graph_cells(self, physical_cells: u64) -> Result<u64, Pc4RowFrameError> {
        self.check_cells(physical_cells)?;
        let shift = u32::from(ROWS - self.remaining) * WIDTH;
        Ok((physical_cells << shift) | ((1_u64 << shift) - 1))
    }

    fn check_cells(self, cells: u64) -> Result<(), Pc4RowFrameError> {
        if cells >> (u32::from(self.remaining) * WIDTH) != 0 {
            Err(Pc4RowFrameError::PhysicalCellsOutsideRemainingRows)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_bottom_clear_changes_graph_coordinates_but_not_original_cell_identity() {
        let frame = Pc4RowFrame::new(0).unwrap();
        let source = 0b11000000 | (0b111111 << 10);
        let lock = 0b1111000000 << 10;
        assert_eq!(source & lock, 0);
        assert_ne!(source & (lock >> 10), 0, "horizontal I is supported");
        let next = frame.after_clear(0b0010).unwrap();
        let target = next.normalized_graph_cells(0b11000000).unwrap();
        assert_ne!(
            source & !target,
            0,
            "graph fields are not monotone ILC masks"
        );
        assert_eq!(target.count_ones(), source.count_ones() + 4);
        assert_eq!(frame.lift_physical_cells(lock).unwrap(), lock);
        assert_eq!(next.lift_physical_cells(0b11000000).unwrap(), 0b11000000);
        assert_eq!(next.lift_physical_cells(1 << 10).unwrap(), 1 << 20);
    }

    #[test]
    fn every_two_clear_history_preserves_surviving_original_row_ids() {
        for prefix in 0..=4 {
            let frame = Pc4RowFrame::new(prefix).unwrap();
            for first in 0..(1_u8 << frame.remaining_rows()) {
                let next = frame.after_clear(first).unwrap();
                let survivors: Vec<_> = (prefix..4)
                    .filter(|r| first & (1 << (r - prefix)) == 0)
                    .collect();
                for second in 0..(1_u8 << next.remaining_rows()) {
                    let end = next.after_clear(second).unwrap();
                    let expected: Vec<_> = survivors
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| second & (1 << i) == 0)
                        .map(|(_, r)| *r)
                        .collect();
                    assert_eq!(usize::from(end.remaining_rows()), expected.len());
                    for (physical, original) in expected.into_iter().enumerate() {
                        for x in 0..10 {
                            assert_eq!(
                                end.lift_physical_cells(1 << (physical * 10 + x)).unwrap(),
                                1 << (usize::from(original) * 10 + x)
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_inputs_and_terminal_empty_frame_are_explicit() {
        assert_eq!(
            Pc4RowFrame::new(5),
            Err(Pc4RowFrameError::ClearedPrefixOutsideDomain)
        );
        let empty = Pc4RowFrame::new(4).unwrap();
        assert_eq!(empty.normalized_graph_cells(0).unwrap(), (1 << 40) - 1);
        assert_eq!(empty.after_clear(0).unwrap(), empty);
        assert_eq!(
            empty.after_clear(1),
            Err(Pc4RowFrameError::ClearMaskOutsideRemainingRows)
        );
        assert_eq!(
            empty.lift_physical_cells(1),
            Err(Pc4RowFrameError::PhysicalCellsOutsideRemainingRows)
        );
        let frame = Pc4RowFrame::new(1).unwrap();
        assert!(frame.lift_physical_cells(1 << 30).is_err());
        assert!(frame.after_clear(8).is_err());
    }
}
