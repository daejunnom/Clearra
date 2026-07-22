use clearra_core_domain::board::standard_pc_board::Board256Mask;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct ExtendedBoard {
    words: [u64; 4],
}

impl ExtendedBoard {
    pub const EMPTY: Self = Self { words: [0; 4] };

    pub const fn from_mask(mask: Board256Mask) -> Self {
        Self {
            words: mask.words(),
        }
    }

    pub const fn from_words(words: [u64; 4]) -> Self {
        Self { words }
    }

    pub const fn words(self) -> [u64; 4] {
        self.words
    }

    pub const fn is_empty(self) -> bool {
        self.words[0] == 0 && self.words[1] == 0 && self.words[2] == 0 && self.words[3] == 0
    }

    pub const fn count_ones(self) -> u32 {
        self.words[0].count_ones()
            + self.words[1].count_ones()
            + self.words[2].count_ones()
            + self.words[3].count_ones()
    }

    pub const fn union(self, other: Self) -> Self {
        Self::from_words([
            self.words[0] | other.words[0],
            self.words[1] | other.words[1],
            self.words[2] | other.words[2],
            self.words[3] | other.words[3],
        ])
    }

    pub const fn intersection(self, other: Self) -> Self {
        Self::from_words([
            self.words[0] & other.words[0],
            self.words[1] & other.words[1],
            self.words[2] & other.words[2],
            self.words[3] & other.words[3],
        ])
    }

    pub const fn without(self, other: Self) -> Self {
        Self::from_words([
            self.words[0] & !other.words[0],
            self.words[1] & !other.words[1],
            self.words[2] & !other.words[2],
            self.words[3] & !other.words[3],
        ])
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.words[0] & other.words[0] != 0
            || self.words[1] & other.words[1] != 0
            || self.words[2] & other.words[2] != 0
            || self.words[3] & other.words[3] != 0
    }

    pub const fn is_subset_of(self, other: Self) -> bool {
        self.words[0] & !other.words[0] == 0
            && self.words[1] & !other.words[1] == 0
            && self.words[2] & !other.words[2] == 0
            && self.words[3] & !other.words[3] == 0
    }

    pub const fn contains(self, index: u16) -> bool {
        if index >= 256 {
            return false;
        }
        self.words[index as usize / 64] & (1_u64 << (index as usize % 64)) != 0
    }

    pub fn insert(&mut self, index: u16) -> bool {
        if index >= 256 {
            return false;
        }
        self.words[index as usize / 64] |= 1_u64 << (index as usize % 64);
        true
    }

    pub fn cells(self) -> ExtendedCellIter {
        ExtendedCellIter {
            words: self.words,
            word_index: 0,
        }
    }

    pub fn row_bits(self, width: u8, row: u8) -> u16 {
        let mut bits = 0_u16;
        for x in 0..width {
            if self.contains(u16::from(row) * u16::from(width) + u16::from(x)) {
                bits |= 1_u16 << x;
            }
        }
        bits
    }
}

pub(super) struct ExtendedCellIter {
    words: [u64; 4],
    word_index: usize,
}

impl Iterator for ExtendedCellIter {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        while self.word_index < self.words.len() {
            let word = &mut self.words[self.word_index];
            if *word != 0 {
                let bit = word.trailing_zeros() as usize;
                *word &= *word - 1;
                return Some((self.word_index * 64 + bit) as u16);
            }
            self.word_index += 1;
        }
        None
    }
}

pub(super) fn place_and_clear(
    width: u8,
    height: u8,
    board: ExtendedBoard,
) -> (ExtendedBoard, u32, u8) {
    let full_row = (1_u16 << width) - 1;
    let mut compacted = ExtendedBoard::EMPTY;
    let mut cleared = 0_u32;
    let mut output_row = 0_u8;
    for input_row in 0..height {
        let row = board.row_bits(width, input_row);
        if row == full_row {
            cleared |= 1_u32 << input_row;
            continue;
        }
        for x in 0..width {
            if row & (1_u16 << x) != 0 {
                compacted.insert(u16::from(output_row) * u16::from(width) + u16::from(x));
            }
        }
        output_row += 1;
    }
    (compacted, cleared, cleared.count_ones() as u8)
}

pub(super) fn compact_logical_board(
    width: u8,
    height: u8,
    board: ExtendedBoard,
    deleted_rows: u32,
) -> ExtendedBoard {
    let mut compacted = ExtendedBoard::EMPTY;
    let mut output_row = 0_u8;
    for logical_row in 0..height {
        if deleted_rows & (1_u32 << logical_row) != 0 {
            continue;
        }
        let row = board.row_bits(width, logical_row);
        for x in 0..width {
            if row & (1_u16 << x) != 0 {
                compacted.insert(u16::from(output_row) * u16::from(width) + u16::from(x));
            }
        }
        output_row += 1;
    }
    compacted
}

pub(super) fn merge_deleted_rows(height: u8, previous: u32, current_physical: u32) -> Option<u32> {
    let mut original = 0_u32;
    for physical_row in 0..height {
        if current_physical & (1_u32 << physical_row) == 0 {
            continue;
        }
        original |= 1_u32 << logical_row_for_physical(height, previous, physical_row)?;
    }
    Some(previous | original)
}

pub(super) fn logical_row_for_physical(
    height: u8,
    deleted_rows: u32,
    physical_row: u8,
) -> Option<u8> {
    let mut visible = 0_u8;
    for logical_row in 0..height {
        if deleted_rows & (1_u32 << logical_row) != 0 {
            continue;
        }
        if visible == physical_row {
            return Some(logical_row);
        }
        visible += 1;
    }
    None
}

pub(super) const fn lower_row_mask(row: u8) -> u32 {
    if row == 0 {
        0
    } else {
        (1_u32 << row) - 1
    }
}

pub(super) fn words_hex(words: [u64; 4]) -> String {
    let highest = words.iter().rposition(|word| *word != 0).unwrap_or(0);
    let mut output = format!("0x{:x}", words[highest]);
    for word in words[..highest].iter().rev() {
        output.push_str(&format!("{word:016x}"));
    }
    output
}
