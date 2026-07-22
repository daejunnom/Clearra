pub const STANDARD_PC_BOARD_WIDTH: u16 = 10;
pub const STANDARD_PC_COMPACT_MAX_LINES: u8 = 6;
pub const STANDARD_PC_EXTENDED_MIN_LINES: u8 = STANDARD_PC_COMPACT_MAX_LINES + 1;
pub const STANDARD_PC_MAX_LINES: u8 = 24;
pub const BOARD256_WORD_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StandardPcBoardStorageKind {
    Board64,
    Board128,
    Board256,
}

impl StandardPcBoardStorageKind {
    pub fn for_lines(lines: u8) -> Result<Self, StandardPcBoardError> {
        match lines {
            1..=STANDARD_PC_COMPACT_MAX_LINES => Ok(Self::Board64),
            STANDARD_PC_EXTENDED_MIN_LINES..=12 => Ok(Self::Board128),
            13..=STANDARD_PC_MAX_LINES => Ok(Self::Board256),
            0 => Err(StandardPcBoardError::ZeroLines),
            _ => Err(StandardPcBoardError::TooManyLines {
                lines,
                maximum: STANDARD_PC_MAX_LINES,
            }),
        }
    }

    pub const fn cpu_word_count(self) -> u8 {
        match self {
            Self::Board64 => 1,
            Self::Board128 => 2,
            Self::Board256 => 4,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Board256Mask {
    words: [u64; BOARD256_WORD_COUNT],
}

impl Board256Mask {
    pub const EMPTY: Self = Self {
        words: [0; BOARD256_WORD_COUNT],
    };

    pub const fn from_words(words: [u64; BOARD256_WORD_COUNT]) -> Self {
        Self { words }
    }

    pub const fn words(self) -> [u64; BOARD256_WORD_COUNT] {
        self.words
    }

    pub fn all_cells(cell_count: u16) -> Result<Self, Board256MaskError> {
        validate_cell_count(cell_count)?;
        let mut words = [0_u64; BOARD256_WORD_COUNT];
        let full_words = usize::from(cell_count / u64::BITS as u16);
        let tail_bits = u32::from(cell_count % u64::BITS as u16);
        words[..full_words].fill(u64::MAX);
        if tail_bits != 0 {
            words[full_words] = (1_u64 << tail_bits) - 1;
        }
        Ok(Self { words })
    }

    pub fn singleton(cell_index: u16) -> Result<Self, Board256MaskError> {
        if usize::from(cell_index) >= BOARD256_WORD_COUNT * u64::BITS as usize {
            return Err(Board256MaskError::CellOutOfRange { cell_index });
        }
        let mut words = [0_u64; BOARD256_WORD_COUNT];
        words[usize::from(cell_index) / u64::BITS as usize] =
            1_u64 << (u32::from(cell_index) % u64::BITS);
        Ok(Self { words })
    }

    pub fn row(width: u16, height: u16, y: u16) -> Result<Self, Board256MaskError> {
        let cell_count =
            width
                .checked_mul(height)
                .ok_or(Board256MaskError::CellCountOutOfRange {
                    cell_count: u16::MAX,
                })?;
        validate_cell_count(cell_count)?;
        if y >= height {
            return Err(Board256MaskError::RowOutOfRange { y, height });
        }
        let mut row = Self::EMPTY;
        let start = y * width;
        for offset in 0..width {
            row = row.union(Self::singleton(start + offset)?);
        }
        Ok(row)
    }

    pub const fn contains_index(self, cell_index: u16) -> bool {
        if cell_index >= 256 {
            return false;
        }
        let word = self.words[cell_index as usize / u64::BITS as usize];
        word & (1_u64 << (cell_index as u32 % u64::BITS)) != 0
    }

    pub const fn intersects(self, other: Self) -> bool {
        (self.words[0] & other.words[0]) != 0
            || (self.words[1] & other.words[1]) != 0
            || (self.words[2] & other.words[2]) != 0
            || (self.words[3] & other.words[3]) != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self::from_words([
            self.words[0] | other.words[0],
            self.words[1] | other.words[1],
            self.words[2] | other.words[2],
            self.words[3] | other.words[3],
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

    pub const fn is_empty(self) -> bool {
        self.words[0] == 0 && self.words[1] == 0 && self.words[2] == 0 && self.words[3] == 0
    }

    pub const fn count_ones(self) -> u32 {
        self.words[0].count_ones()
            + self.words[1].count_ones()
            + self.words[2].count_ones()
            + self.words[3].count_ones()
    }

    pub fn fits_cell_count(self, cell_count: u16) -> Result<bool, Board256MaskError> {
        let allowed = Self::all_cells(cell_count)?;
        Ok(self.without(allowed).is_empty())
    }

    pub fn mirrored_horizontally(self, width: u16, height: u16) -> Result<Self, Board256MaskError> {
        let cell_count =
            width
                .checked_mul(height)
                .ok_or(Board256MaskError::CellCountOutOfRange {
                    cell_count: u16::MAX,
                })?;
        validate_cell_count(cell_count)?;
        if width == 0 || !self.fits_cell_count(cell_count)? {
            return Err(Board256MaskError::MaskOutsideField { cell_count });
        }

        let mut words = [0_u64; BOARD256_WORD_COUNT];
        for y in 0..height {
            for x in 0..width {
                let source = y * width + x;
                if !self.contains_index(source) {
                    continue;
                }
                let target = y * width + (width - x - 1);
                words[usize::from(target) / u64::BITS as usize] |=
                    1_u64 << (u32::from(target) % u64::BITS);
            }
        }
        Ok(Self::from_words(words))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StandardPcBoardMask {
    Board64(u64),
    Board128(u128),
    Board256(Board256Mask),
}

impl StandardPcBoardMask {
    pub fn from_words(
        lines: u8,
        words: [u64; BOARD256_WORD_COUNT],
    ) -> Result<Self, StandardPcBoardError> {
        let storage = StandardPcBoardStorageKind::for_lines(lines)?;
        let cell_count = u16::from(lines) * STANDARD_PC_BOARD_WIDTH;
        let mask = Board256Mask::from_words(words);
        if !mask
            .fits_cell_count(cell_count)
            .map_err(StandardPcBoardError::Mask)?
        {
            return Err(StandardPcBoardError::MaskOutsideBoard { lines });
        }
        Ok(match storage {
            StandardPcBoardStorageKind::Board64 => Self::Board64(words[0]),
            StandardPcBoardStorageKind::Board128 => {
                Self::Board128(u128::from(words[0]) | (u128::from(words[1]) << 64))
            }
            StandardPcBoardStorageKind::Board256 => Self::Board256(mask),
        })
    }

    pub const fn storage_kind(self) -> StandardPcBoardStorageKind {
        match self {
            Self::Board64(_) => StandardPcBoardStorageKind::Board64,
            Self::Board128(_) => StandardPcBoardStorageKind::Board128,
            Self::Board256(_) => StandardPcBoardStorageKind::Board256,
        }
    }

    pub const fn words(self) -> [u64; BOARD256_WORD_COUNT] {
        match self {
            Self::Board64(mask) => [mask, 0, 0, 0],
            Self::Board128(mask) => [mask as u64, (mask >> 64) as u64, 0, 0],
            Self::Board256(mask) => mask.words(),
        }
    }

    pub const fn compact_board64(self) -> Option<u64> {
        match self {
            Self::Board64(mask) => Some(mask),
            Self::Board128(_) | Self::Board256(_) => None,
        }
    }

    pub const fn count_ones(self) -> u32 {
        match self {
            Self::Board64(mask) => mask.count_ones(),
            Self::Board128(mask) => mask.count_ones(),
            Self::Board256(mask) => mask.count_ones(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StandardPcBoard {
    lines: u8,
    occupied: StandardPcBoardMask,
}

impl StandardPcBoard {
    pub fn empty(lines: u8) -> Result<Self, StandardPcBoardError> {
        Self::from_words(lines, [0; BOARD256_WORD_COUNT])
    }

    pub fn from_words(
        lines: u8,
        words: [u64; BOARD256_WORD_COUNT],
    ) -> Result<Self, StandardPcBoardError> {
        Ok(Self {
            lines,
            occupied: StandardPcBoardMask::from_words(lines, words)?,
        })
    }

    pub const fn lines(self) -> u8 {
        self.lines
    }

    pub const fn width(self) -> u16 {
        STANDARD_PC_BOARD_WIDTH
    }

    pub const fn cell_count(self) -> u16 {
        self.lines as u16 * STANDARD_PC_BOARD_WIDTH
    }

    pub const fn occupied(self) -> StandardPcBoardMask {
        self.occupied
    }

    pub const fn is_compact_board64(self) -> bool {
        matches!(self.occupied, StandardPcBoardMask::Board64(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardPcBoardError {
    ZeroLines,
    TooManyLines { lines: u8, maximum: u8 },
    MaskOutsideBoard { lines: u8 },
    Mask(Board256MaskError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Board256MaskError {
    CellCountOutOfRange { cell_count: u16 },
    CellOutOfRange { cell_index: u16 },
    RowOutOfRange { y: u16, height: u16 },
    MaskOutsideField { cell_count: u16 },
}

fn validate_cell_count(cell_count: u16) -> Result<(), Board256MaskError> {
    if cell_count == 0 || usize::from(cell_count) > BOARD256_WORD_COUNT * u64::BITS as usize {
        return Err(Board256MaskError::CellCountOutOfRange { cell_count });
    }
    Ok(())
}
