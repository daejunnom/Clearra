#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuPackingCandidate {
    candidate_id: u64,
    shape_key: u64,
    tiling_key: u64,
    operation_set_key: u64,
    final_board_mask: u64,
    coverage_bits: u128,
    required_candidate: bool,
}

impl GpuPackingCandidate {
    pub const fn new(
        candidate_id: u64,
        shape_key: u64,
        tiling_key: u64,
        operation_set_key: u64,
        final_board_mask: u64,
        coverage_bits: u128,
        required_candidate: bool,
    ) -> Self {
        Self {
            candidate_id,
            shape_key,
            tiling_key,
            operation_set_key,
            final_board_mask,
            coverage_bits,
            required_candidate,
        }
    }
}
impl GpuPackingCandidate {
    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }
}
impl GpuPackingCandidate {
    pub const fn shape_key(&self) -> u64 {
        self.shape_key
    }
}
impl GpuPackingCandidate {
    pub const fn tiling_key(&self) -> u64 {
        self.tiling_key
    }
}
impl GpuPackingCandidate {
    pub const fn operation_set_key(&self) -> u64 {
        self.operation_set_key
    }
}
impl GpuPackingCandidate {
    pub const fn final_board_mask(&self) -> u64 {
        self.final_board_mask
    }
}
impl GpuPackingCandidate {
    pub const fn coverage_bits(&self) -> u128 {
        self.coverage_bits
    }
}
impl GpuPackingCandidate {
    pub const fn required_candidate(&self) -> bool {
        self.required_candidate
    }
}
impl GpuPackingCandidate {
    pub fn exact_identity_matches(&self, other: &Self) -> bool {
        self.shape_key == other.shape_key
            && self.tiling_key == other.tiling_key
            && self.operation_set_key == other.operation_set_key
            && self.final_board_mask == other.final_board_mask
            && self.coverage_bits == other.coverage_bits
    }
}
