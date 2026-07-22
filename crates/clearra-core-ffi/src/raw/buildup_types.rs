use crate::buildup::{CBuildUpTraceStep, CKickEvidenceView};

pub const C_NATIVE_BUILDUP_MAX_VARIANTS: usize = 512;
pub const C_NATIVE_BUILDUP_MAX_OPERATIONS: usize = 15;
pub const C_NATIVE_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CNativeBuildVariantView {
    pub candidate_id: u64,
    pub build_variant_id: u64,
    pub canonical_operation_set_id: u64,
    pub operation_set_hash: u64,
    pub final_board: u64,
    pub coverage_pattern_id: u32,
    pub placed_count: u16,
    pub queue_cursor: u16,
    pub hold_piece: u8,
    pub hold_empty: u8,
    pub cleared_lines: u8,
    pub hold_branch_kind: u8,
    pub trace_identity: u64,
    pub operation_order_ids: *const u16,
    pub trace_steps: *const CBuildUpTraceStep,
    pub operation_order_count: u16,
    pub trace_step_count: u16,
    pub kick_evidence: *const CKickEvidenceView,
    pub kick_evidence_count: u32,
    pub trace_completeness_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CNativeBuildVariantBuffer {
    pub count: u16,
    pub reserved: u16,
    pub total_variant_count: u64,
    pub count_complete: u8,
    pub trace_retention_truncated: u8,
    pub reserved2: [u8; 6],
    pub search_metrics: crate::native::CNativeBuildUpSearchMetrics,
    pub variants: [CNativeBuildVariantView; C_NATIVE_BUILDUP_MAX_VARIANTS],
    pub kick_evidence_storage: [[CKickEvidenceView; C_NATIVE_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT];
        C_NATIVE_BUILDUP_MAX_VARIANTS],
    pub operation_order_storage:
        [[u16; C_NATIVE_BUILDUP_MAX_OPERATIONS]; C_NATIVE_BUILDUP_MAX_VARIANTS],
    pub trace_step_storage:
        [[CBuildUpTraceStep; C_NATIVE_BUILDUP_MAX_OPERATIONS]; C_NATIVE_BUILDUP_MAX_VARIANTS],
}

impl Default for CNativeBuildVariantBuffer {
    fn default() -> Self {
        Self {
            count: 0,
            reserved: 0,
            total_variant_count: 0,
            count_complete: 0,
            trace_retention_truncated: 0,
            reserved2: [0; 6],
            search_metrics: crate::native::CNativeBuildUpSearchMetrics::default(),
            variants: [CNativeBuildVariantView::default(); C_NATIVE_BUILDUP_MAX_VARIANTS],
            kick_evidence_storage: [[CKickEvidenceView::default();
                C_NATIVE_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT];
                C_NATIVE_BUILDUP_MAX_VARIANTS],
            operation_order_storage: [[0; C_NATIVE_BUILDUP_MAX_OPERATIONS];
                C_NATIVE_BUILDUP_MAX_VARIANTS],
            trace_step_storage: [[CBuildUpTraceStep::default(); C_NATIVE_BUILDUP_MAX_OPERATIONS];
                C_NATIVE_BUILDUP_MAX_VARIANTS],
        }
    }
}
