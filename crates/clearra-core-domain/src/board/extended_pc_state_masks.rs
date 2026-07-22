use super::standard_pc_board::{
    STANDARD_PC_BOARD_WIDTH, STANDARD_PC_EXTENDED_MIN_LINES, STANDARD_PC_MAX_LINES,
};

pub const EXTENDED_PC_MAX_PLACEMENTS: u8 =
    (STANDARD_PC_BOARD_WIDTH as u8 * STANDARD_PC_MAX_LINES) / 4;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExtendedPcDeletedRowMask(u32);

impl ExtendedPcDeletedRowMask {
    pub const EMPTY: Self = Self(0);

    pub fn from_bits(target_lines: u8, bits: u32) -> Result<Self, ExtendedPcStateMaskError> {
        validate_extended_lines(target_lines)?;
        if bits & !low_u32_mask(target_lines) != 0 {
            return Err(ExtendedPcStateMaskError::DeletedRowOutsideTarget { target_lines, bits });
        }
        Ok(Self(bits))
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, row: u8) -> bool {
        row < u32::BITS as u8 && self.0 & (1_u32 << row) != 0
    }

    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExtendedPcOperationBitSet(u64);

impl ExtendedPcOperationBitSet {
    pub const EMPTY: Self = Self(0);

    pub fn all(operation_count: u8) -> Result<Self, ExtendedPcStateMaskError> {
        validate_operation_count(operation_count)?;
        Ok(Self(low_u64_mask(operation_count)))
    }

    pub fn from_bits(operation_count: u8, bits: u64) -> Result<Self, ExtendedPcStateMaskError> {
        validate_operation_count(operation_count)?;
        if bits & !low_u64_mask(operation_count) != 0 {
            return Err(ExtendedPcStateMaskError::OperationOutsideLayout {
                operation_count,
                bits,
            });
        }
        Ok(Self(bits))
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub const fn contains(self, operation_index: u8) -> bool {
        operation_index < u64::BITS as u8 && self.0 & (1_u64 << operation_index) != 0
    }

    pub fn without(
        self,
        operation_count: u8,
        operation_index: u8,
    ) -> Result<Self, ExtendedPcStateMaskError> {
        validate_operation_count(operation_count)?;
        if operation_index >= operation_count {
            return Err(ExtendedPcStateMaskError::OperationIndexOutOfRange {
                operation_index,
                operation_count,
            });
        }
        Ok(Self(self.0 & !(1_u64 << operation_index)))
    }

    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtendedPcStateMaskError {
    CompactLinesRequireLegacyContract {
        target_lines: u8,
    },
    TooManyLines {
        target_lines: u8,
        maximum: u8,
    },
    DeletedRowOutsideTarget {
        target_lines: u8,
        bits: u32,
    },
    TooManyOperations {
        operation_count: u8,
        maximum: u8,
    },
    OperationOutsideLayout {
        operation_count: u8,
        bits: u64,
    },
    OperationIndexOutOfRange {
        operation_index: u8,
        operation_count: u8,
    },
}

fn validate_extended_lines(target_lines: u8) -> Result<(), ExtendedPcStateMaskError> {
    if target_lines < STANDARD_PC_EXTENDED_MIN_LINES {
        return Err(ExtendedPcStateMaskError::CompactLinesRequireLegacyContract { target_lines });
    }
    if target_lines > STANDARD_PC_MAX_LINES {
        return Err(ExtendedPcStateMaskError::TooManyLines {
            target_lines,
            maximum: STANDARD_PC_MAX_LINES,
        });
    }
    Ok(())
}

fn validate_operation_count(operation_count: u8) -> Result<(), ExtendedPcStateMaskError> {
    if operation_count > EXTENDED_PC_MAX_PLACEMENTS {
        return Err(ExtendedPcStateMaskError::TooManyOperations {
            operation_count,
            maximum: EXTENDED_PC_MAX_PLACEMENTS,
        });
    }
    Ok(())
}

const fn low_u32_mask(bit_count: u8) -> u32 {
    if bit_count == u32::BITS as u8 {
        u32::MAX
    } else {
        (1_u32 << bit_count) - 1
    }
}

const fn low_u64_mask(bit_count: u8) -> u64 {
    if bit_count == u64::BITS as u8 {
        u64::MAX
    } else if bit_count == 0 {
        0
    } else {
        (1_u64 << bit_count) - 1
    }
}
