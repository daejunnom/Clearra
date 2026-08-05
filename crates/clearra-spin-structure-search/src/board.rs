use std::fmt;

/// A standard-width field stored in bottom-to-top row-major order.
///
/// The search contract fixes the width at ten and supports at most 24 rows,
/// which keeps the exact identity in four machine words without allocation.
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StructureBoard {
    words: [u64; 4],
}

impl StructureBoard {
    pub const WIDTH: u8 = 10;
    pub const MAX_HEIGHT: u8 = 24;
    pub const EMPTY: Self = Self { words: [0; 4] };

    pub const fn from_words(words: [u64; 4]) -> Self {
        Self { words }
    }

    pub const fn words(self) -> [u64; 4] {
        self.words
    }

    pub const fn is_empty(self) -> bool {
        self.words[0] == 0 && self.words[1] == 0 && self.words[2] == 0 && self.words[3] == 0
    }

    pub const fn contains_index(self, index: u16) -> bool {
        if index >= 256 {
            return false;
        }
        self.words[index as usize / 64] & (1_u64 << (index as usize % 64)) != 0
    }

    pub fn contains(self, x: u8, y: u8) -> bool {
        x < Self::WIDTH
            && y < Self::MAX_HEIGHT
            && self.contains_index(u16::from(y) * u16::from(Self::WIDTH) + u16::from(x))
    }

    pub fn with_cell(mut self, x: u8, y: u8) -> Option<Self> {
        (x < Self::WIDTH && y < Self::MAX_HEIGHT).then(|| {
            self.insert_index(u16::from(y) * u16::from(Self::WIDTH) + u16::from(x));
            self
        })
    }

    pub fn from_rows(rows: &[u16]) -> Option<Self> {
        if rows.len() > usize::from(Self::MAX_HEIGHT) || rows.iter().any(|row| row & !0x03ff != 0) {
            return None;
        }
        let mut board = Self::EMPTY;
        for (y, row) in rows.iter().copied().enumerate() {
            for x in 0..Self::WIDTH {
                if row & (1_u16 << x) != 0 {
                    board.insert_index(y as u16 * u16::from(Self::WIDTH) + u16::from(x));
                }
            }
        }
        Some(board)
    }

    pub fn row_bits(self, row: u8) -> u16 {
        let mut bits = 0_u16;
        for x in 0..Self::WIDTH {
            if self.contains(x, row) {
                bits |= 1_u16 << x;
            }
        }
        bits
    }

    pub(crate) fn intersects(self, other: Self) -> bool {
        self.words
            .iter()
            .zip(other.words)
            .any(|(left, right)| left & right != 0)
    }

    pub(crate) const fn union(self, other: Self) -> Self {
        Self::from_words([
            self.words[0] | other.words[0],
            self.words[1] | other.words[1],
            self.words[2] | other.words[2],
            self.words[3] | other.words[3],
        ])
    }

    pub(crate) fn insert_index(&mut self, index: u16) -> bool {
        if index >= 256 {
            return false;
        }
        self.words[index as usize / 64] |= 1_u64 << (index as usize % 64);
        true
    }

    pub(crate) fn has_cells_at_or_above(self, height: u8) -> bool {
        (height..Self::MAX_HEIGHT).any(|row| self.row_bits(row) != 0)
    }
}

impl fmt::Debug for StructureBoard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("StructureBoard")
            .field(&self.words)
            .finish()
    }
}

pub(crate) fn place_and_clear(height: u8, board: StructureBoard) -> (StructureBoard, u32, u8) {
    let mut compacted = StructureBoard::EMPTY;
    let mut cleared_rows = 0_u32;
    let mut output_row = 0_u8;
    for input_row in 0..height {
        let row = board.row_bits(input_row);
        if row == 0x03ff {
            cleared_rows |= 1_u32 << input_row;
            continue;
        }
        for x in 0..StructureBoard::WIDTH {
            if row & (1_u16 << x) != 0 {
                compacted.insert_index(
                    u16::from(output_row) * u16::from(StructureBoard::WIDTH) + u16::from(x),
                );
            }
        }
        output_row += 1;
    }
    (compacted, cleared_rows, cleared_rows.count_ones() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_clear_compacts_rows_exactly() {
        let board = StructureBoard::from_rows(&[0x03ff, 0b11, 0b100]).expect("field");
        let (after, rows, lines) = place_and_clear(4, board);
        assert_eq!(rows, 1);
        assert_eq!(lines, 1);
        assert_eq!(after.row_bits(0), 0b11);
        assert_eq!(after.row_bits(1), 0b100);
    }
}
