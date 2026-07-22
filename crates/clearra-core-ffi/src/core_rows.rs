#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CBuildUpResult {
    pub candidate_id: u64,
    pub success: u8,
    pub cleared_lines: u8,
    pub reserved: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CCoverageRow {
    pub row_id: u64,
    pub pattern_bits_low: u64,
    pub probability_numerator: u64,
    pub probability_denominator: u64,
}
