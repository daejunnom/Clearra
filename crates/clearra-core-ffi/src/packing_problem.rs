pub use crate::problem::CPackingProblem;

pub const C_PACKING_MAX_OPERATIONS: usize = 15;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct CPackingOperation {
    pub piece: u8,
    pub rotation: u8,
    pub x: i8,
    pub y: i8,
    pub operation_id: u16,
    pub required_deleted_row_mask: u16,
    pub mask: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CPackingCandidate {
    pub candidate_id: u64,
    pub canonical_operation_set_id: u64,
    pub final_board: u64,
    pub shape_mask: u64,
    pub shape_key: u64,
    pub tiling_key: u64,
    pub operation_set_key: u64,
    pub operation_count: u16,
    pub geometry_variant_domains: u16,
    pub cleared_lines: u8,
    pub reserved: [u8; 3],
    pub operations: [CPackingOperation; C_PACKING_MAX_OPERATIONS],
}

impl Default for CPackingCandidate {
    fn default() -> Self {
        Self {
            candidate_id: 0,
            canonical_operation_set_id: 0,
            final_board: 0,
            shape_mask: 0,
            shape_key: 0,
            tiling_key: 0,
            operation_set_key: 0,
            operation_count: 0,
            geometry_variant_domains: 0,
            cleared_lines: 0,
            reserved: [0; 3],
            operations: [CPackingOperation::default(); C_PACKING_MAX_OPERATIONS],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_packing_candidate_uses_c_layout_without_solution_count() {
        let mut candidate = CPackingCandidate {
            candidate_id: 7,
            canonical_operation_set_id: 11,
            operation_count: 2,
            ..Default::default()
        };
        candidate.operations[0].piece = 2;
        candidate.operations[1].piece = 1;

        assert_eq!(core::mem::size_of::<CPackingOperation>(), 16);
        assert_eq!(core::mem::size_of::<CPackingCandidate>(), 304);
        assert_eq!(candidate.candidate_id, 7);
        assert_eq!(candidate.canonical_operation_set_id, 11);
        assert_eq!(candidate.operation_count, 2);
        assert_eq!(candidate.operations[0].piece, 2);
    }
}
