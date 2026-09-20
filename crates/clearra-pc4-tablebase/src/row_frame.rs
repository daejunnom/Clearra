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
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Pc4RowFrame {
    // Physical rows always retain the ascending order of their original row
    // IDs. The frame is therefore exactly one subset of four rows, not an
    // arbitrary four-byte permutation plus a separate length. Bit `r` means
    // original row `r` still survives; zero is the terminal empty frame.
    surviving_original_rows: u8,
}

const _: [(); 1] = [(); core::mem::size_of::<Pc4RowFrame>()];

impl Pc4RowFrame {
    /// The request's original frame is its starting normalized graph frame.
    pub fn new(cleared_bottom_prefix: u8) -> Result<Self, Pc4RowFrameError> {
        if cleared_bottom_prefix > ROWS {
            return Err(Pc4RowFrameError::ClearedPrefixOutsideDomain);
        }
        let cleared = (1_u8 << cleared_bottom_prefix).wrapping_sub(1);
        Ok(Self {
            surviving_original_rows: ((1_u8 << ROWS) - 1) & !cleared,
        })
    }

    pub fn remaining_rows(self) -> u8 {
        self.surviving_original_rows.count_ones() as u8
    }

    /// Exact four-bit wire/storage form of this row correspondence.
    ///
    /// Each bit identifies one surviving row in the request's original
    /// four-row frame. This is deliberately exposed as a semantic encoding,
    /// rather than exposing the representation field itself, so compact
    /// frontier owners can pack it without recreating row-frame rules.
    pub const fn surviving_original_rows_mask(self) -> u8 {
        self.surviving_original_rows
    }

    /// Reconstruct a row frame from its exact four-bit semantic encoding.
    pub const fn from_surviving_original_rows_mask(mask: u8) -> Option<Self> {
        if mask < (1_u8 << ROWS) {
            Some(Self {
                surviving_original_rows: mask,
            })
        } else {
            None
        }
    }

    /// Lift a physical pre-clear placement into the request's fixed frame.
    /// This is not a graph-normalized mask and must not be recomputed by
    /// subtracting consecutive graph hashes.
    pub fn lift_physical_cells(self, cells: u64) -> Result<u64, Pc4RowFrameError> {
        self.check_cells(cells)?;
        let mut lifted = 0;
        for (physical, original) in (0_u32..).zip(self.original_rows()) {
            let row = (cells >> (physical * WIDTH)) & ROW;
            lifted |= row << (u32::from(original) * WIDTH);
        }
        Ok(lifted)
    }

    /// Apply a physical line-clear mask after lifting the current placement.
    /// Returning a new frame leaves the old branch available for rollback.
    pub fn after_clear(self, physical_rows: u8) -> Result<Self, Pc4RowFrameError> {
        if u16::from(physical_rows) >= (1_u16 << self.remaining_rows()) {
            return Err(Pc4RowFrameError::ClearMaskOutsideRemainingRows);
        }
        let mut survivors = 0_u8;
        for (physical, original) in (0_u8..).zip(self.original_rows()) {
            if physical_rows & (1 << physical) == 0 {
                survivors |= 1 << original;
            }
        }
        Ok(Self {
            surviving_original_rows: survivors,
        })
    }

    /// Reconstruct the graph's bottom-filled normalized mask from an already
    /// compacted physical board. Horizontal hash bit reversal is separate.
    pub fn normalized_graph_cells(self, physical_cells: u64) -> Result<u64, Pc4RowFrameError> {
        self.check_cells(physical_cells)?;
        let shift = u32::from(ROWS - self.remaining_rows()) * WIDTH;
        Ok((physical_cells << shift) | ((1_u64 << shift) - 1))
    }

    fn check_cells(self, cells: u64) -> Result<(), Pc4RowFrameError> {
        if cells >> (u32::from(self.remaining_rows()) * WIDTH) != 0 {
            Err(Pc4RowFrameError::PhysicalCellsOutsideRemainingRows)
        } else {
            Ok(())
        }
    }

    fn original_rows(self) -> impl Iterator<Item = u8> {
        (0..ROWS).filter(move |&row| self.surviving_original_rows & (1 << row) != 0)
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
        assert_eq!(core::mem::size_of::<Pc4RowFrame>(), 1);
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

    #[test]
    fn every_exact_storage_mask_round_trips() {
        for mask in 0..(1_u8 << ROWS) {
            let frame = Pc4RowFrame::from_surviving_original_rows_mask(mask).unwrap();
            assert_eq!(frame.surviving_original_rows_mask(), mask);
            assert_eq!(frame.remaining_rows(), mask.count_ones() as u8);
        }
        assert!(Pc4RowFrame::from_surviving_original_rows_mask(1_u8 << ROWS).is_none());
        assert!(Pc4RowFrame::from_surviving_original_rows_mask(u8::MAX).is_none());
    }
}
