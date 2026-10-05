//! One typed codec for compact physical rows, bottom-prefix product keys, and
//! original-row replay coordinates. A caller may narrow the domain (for
//! example, local reachability excludes a terminal all-cleared frame), but
//! it must not reimplement the row correspondence.

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CompactedRowFrame {
    target_height: u8,
    deleted_original_rows: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompactedRowFrameError {
    PhysicalCellsOutsideSurvivingRows,
    MissingClearedBottomPrefix,
}

impl CompactedRowFrame {
    pub(crate) const fn new(target_height: u8, deleted_original_rows: u16) -> Option<Self> {
        if target_height < 1 || target_height > 6 || deleted_original_rows >> target_height != 0 {
            return None;
        }
        Some(Self {
            target_height,
            deleted_original_rows: deleted_original_rows as u8,
        })
    }

    pub(crate) const fn target_height(self) -> u8 {
        self.target_height
    }

    pub(crate) const fn deleted_original_rows(self) -> u8 {
        self.deleted_original_rows
    }

    pub(crate) const fn cleared_rows(self) -> u8 {
        self.deleted_original_rows.count_ones() as u8
    }

    pub(crate) const fn surviving_rows(self) -> u8 {
        self.target_height - self.cleared_rows()
    }

    pub(crate) const fn original_row_for_physical(self, physical_row: u8) -> Option<u8> {
        if physical_row >= self.surviving_rows() {
            return None;
        }
        let mut visible = 0_u8;
        let mut original = 0_u8;
        while original < self.target_height {
            if self.deleted_original_rows & (1_u8 << original) == 0 {
                if visible == physical_row {
                    return Some(original);
                }
                visible += 1;
            }
            original += 1;
        }
        None
    }

    pub(crate) const fn physical_row_for_original(self, original_row: u8) -> Option<u8> {
        if original_row >= self.target_height
            || self.deleted_original_rows & (1_u8 << original_row) != 0
        {
            return None;
        }
        let mut visible = 0_u8;
        let mut original = 0_u8;
        while original < original_row {
            if self.deleted_original_rows & (1_u8 << original) == 0 {
                visible += 1;
            }
            original += 1;
        }
        Some(visible)
    }

    pub(crate) const fn accepts_physical_board(self, width: u8, board: u64) -> bool {
        width == 10 && board >> (self.surviving_rows() as u32 * width as u32) == 0
    }

    pub(crate) fn bottom_prefix_board(
        self,
        physical_board: u64,
    ) -> Result<u64, CompactedRowFrameError> {
        if !self.accepts_physical_board(10, physical_board) {
            return Err(CompactedRowFrameError::PhysicalCellsOutsideSurvivingRows);
        }
        let prefix_bits = u32::from(self.cleared_rows()) * 10;
        let prefix = low_bits(prefix_bits);
        Ok((physical_board << prefix_bits) | prefix)
    }

    pub(crate) fn compact_from_bottom_prefix(
        self,
        product_board: u64,
    ) -> Result<u64, CompactedRowFrameError> {
        let total_bits = u32::from(self.target_height) * 10;
        if product_board >> total_bits != 0 {
            return Err(CompactedRowFrameError::PhysicalCellsOutsideSurvivingRows);
        }
        let prefix_bits = u32::from(self.cleared_rows()) * 10;
        let prefix = low_bits(prefix_bits);
        if product_board & prefix != prefix {
            return Err(CompactedRowFrameError::MissingClearedBottomPrefix);
        }
        let physical_board = product_board >> prefix_bits;
        if !self.accepts_physical_board(10, physical_board) {
            return Err(CompactedRowFrameError::PhysicalCellsOutsideSurvivingRows);
        }
        Ok(physical_board)
    }

    pub(crate) fn replay_frame_board(
        self,
        physical_board: u64,
    ) -> Result<u64, CompactedRowFrameError> {
        if !self.accepts_physical_board(10, physical_board) {
            return Err(CompactedRowFrameError::PhysicalCellsOutsideSurvivingRows);
        }
        let mut replay = 0_u64;
        for original in 0..self.target_height {
            let row = match self.physical_row_for_original(original) {
                Some(physical) => (physical_board >> (u32::from(physical) * 10)) & 0x3ff,
                None => 0x3ff,
            };
            replay |= row << (u32::from(original) * 10);
        }
        Ok(replay)
    }
}

const fn low_bits(count: u32) -> u64 {
    if count == 0 {
        0
    } else {
        (1_u64 << count) - 1
    }
}

#[cfg(test)]
mod tests {
    use super::CompactedRowFrame;

    #[test]
    fn every_one_to_six_row_mask_roundtrips_all_three_frames() {
        for height in 1..=6_u8 {
            for deleted in 0..(1_u16 << height) {
                let frame = CompactedRowFrame::new(height, deleted).unwrap();
                let mut physical = 0_u64;
                let mut expected_replay = 0_u64;
                for original in 0..height {
                    if let Some(row) = frame.physical_row_for_original(original) {
                        assert_eq!(frame.original_row_for_physical(row), Some(original));
                        let bit = 1_u64 << (u32::from(row) * 10);
                        physical |= bit;
                        expected_replay |= 1_u64 << (u32::from(original) * 10);
                    } else {
                        expected_replay |= 0x3ff_u64 << (u32::from(original) * 10);
                    }
                }
                let product = frame.bottom_prefix_board(physical).unwrap();
                assert_eq!(frame.compact_from_bottom_prefix(product), Ok(physical));
                assert_eq!(frame.replay_frame_board(physical), Ok(expected_replay));
                assert_eq!(
                    frame.original_row_for_physical(frame.surviving_rows()),
                    None
                );
            }
        }
    }
}
