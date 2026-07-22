use crate::native::{CNativeBuildVariantView, C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT};
use crate::problem::C_BUILDUP_MAX_OPERATIONS;

use super::{kick_evidence_view::CKickEvidenceView, trace_step_view::CBuildUpTraceStep};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CBuildVariantView {
    candidate_id: u64,
    build_variant_id: u64,
    canonical_operation_set_id: u64,
    operation_set_hash: u64,
    final_board: u64,
    coverage_pattern_id: u32,
    placed_count: u16,
    queue_cursor: u16,
    hold_piece: u8,
    hold_empty: bool,
    cleared_lines: u8,
    hold_branch_kind: u8,
    trace_identity: u64,
    operation_order_ids: Vec<u16>,
    trace_steps: Vec<CBuildUpTraceStep>,
    kick_evidence: Vec<CKickEvidenceView>,
    trace_completeness_flags: u32,
}

impl CBuildVariantView {
    pub fn from_native(native: &CNativeBuildVariantView) -> Result<Self, CBuildVariantViewError> {
        let kick_evidence_count = native.kick_evidence_count as usize;
        if kick_evidence_count > C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT {
            return Err(CBuildVariantViewError::KickEvidenceCountExceeded {
                count: native.kick_evidence_count,
                max: C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT,
            });
        }

        let kick_evidence =
            crate::raw::native_slice::copy_native_slice(native.kick_evidence, kick_evidence_count)
                .ok_or(CBuildVariantViewError::MissingKickEvidencePointer {
                    count: native.kick_evidence_count,
                })?;

        let operation_order_count = usize::from(native.operation_order_count);
        if operation_order_count > C_BUILDUP_MAX_OPERATIONS {
            return Err(CBuildVariantViewError::OperationOrderCountExceeded {
                count: native.operation_order_count,
                max: C_BUILDUP_MAX_OPERATIONS,
            });
        }
        let trace_step_count = usize::from(native.trace_step_count);
        if trace_step_count > C_BUILDUP_MAX_OPERATIONS {
            return Err(CBuildVariantViewError::TraceStepCountExceeded {
                count: native.trace_step_count,
                max: C_BUILDUP_MAX_OPERATIONS,
            });
        }
        if native.operation_order_count != native.trace_step_count {
            return Err(CBuildVariantViewError::TraceCountMismatch {
                operation_count: native.operation_order_count,
                trace_count: native.trace_step_count,
            });
        }
        let operation_order_ids = crate::raw::native_slice::copy_native_slice(
            native.operation_order_ids,
            operation_order_count,
        )
        .ok_or(CBuildVariantViewError::MissingOperationOrderPointer {
            count: native.operation_order_count,
        })?;
        let trace_steps =
            crate::raw::native_slice::copy_native_slice(native.trace_steps, trace_step_count)
                .ok_or(CBuildVariantViewError::MissingTraceStepsPointer {
                    count: native.trace_step_count,
                })?;
        for (step_index, step) in trace_steps.iter().enumerate() {
            if operation_order_ids[step_index] != step.operation_id {
                return Err(CBuildVariantViewError::OperationOrderMismatch {
                    step_index,
                    order_operation_id: operation_order_ids[step_index],
                    trace_operation_id: step.operation_id,
                });
            }
            if step.kick_evidence_index != u8::MAX
                && usize::from(step.kick_evidence_index) >= kick_evidence.len()
            {
                return Err(CBuildVariantViewError::KickEvidenceIndexOutOfRange {
                    step_index,
                    evidence_index: step.kick_evidence_index,
                    evidence_count: kick_evidence.len(),
                });
            }
        }

        Ok(Self {
            candidate_id: native.candidate_id,
            build_variant_id: native.build_variant_id,
            canonical_operation_set_id: native.canonical_operation_set_id,
            operation_set_hash: native.operation_set_hash,
            final_board: native.final_board,
            coverage_pattern_id: native.coverage_pattern_id,
            placed_count: native.placed_count,
            queue_cursor: native.queue_cursor,
            hold_piece: native.hold_piece,
            hold_empty: native.hold_empty != 0,
            cleared_lines: native.cleared_lines,
            hold_branch_kind: native.hold_branch_kind,
            trace_identity: native.trace_identity,
            operation_order_ids,
            trace_steps,
            kick_evidence,
            trace_completeness_flags: native.trace_completeness_flags,
        })
    }
}
impl CBuildVariantView {
    pub fn candidate_id(&self) -> u64 {
        self.candidate_id
    }
}
impl CBuildVariantView {
    pub fn build_variant_id(&self) -> u64 {
        self.build_variant_id
    }
}
impl CBuildVariantView {
    pub fn canonical_operation_set_id(&self) -> u64 {
        self.canonical_operation_set_id
    }
}
impl CBuildVariantView {
    pub fn operation_set_hash(&self) -> u64 {
        self.operation_set_hash
    }
}
impl CBuildVariantView {
    pub fn final_board(&self) -> u64 {
        self.final_board
    }
}
impl CBuildVariantView {
    pub fn coverage_pattern_id(&self) -> u32 {
        self.coverage_pattern_id
    }
}
impl CBuildVariantView {
    pub fn placed_count(&self) -> u16 {
        self.placed_count
    }
}
impl CBuildVariantView {
    pub fn queue_cursor(&self) -> u16 {
        self.queue_cursor
    }
}
impl CBuildVariantView {
    pub fn hold_piece(&self) -> u8 {
        self.hold_piece
    }
}
impl CBuildVariantView {
    pub fn hold_empty(&self) -> bool {
        self.hold_empty
    }
}
impl CBuildVariantView {
    pub fn cleared_lines(&self) -> u8 {
        self.cleared_lines
    }
}
impl CBuildVariantView {
    pub fn hold_branch_kind(&self) -> u8 {
        self.hold_branch_kind
    }
}
impl CBuildVariantView {
    pub fn trace_identity(&self) -> u64 {
        self.trace_identity
    }
}
impl CBuildVariantView {
    pub fn operation_order_ids(&self) -> &[u16] {
        &self.operation_order_ids
    }
}
impl CBuildVariantView {
    pub fn trace_steps(&self) -> &[CBuildUpTraceStep] {
        &self.trace_steps
    }
}
impl CBuildVariantView {
    pub fn kick_evidence(&self) -> &[CKickEvidenceView] {
        &self.kick_evidence
    }
}
impl CBuildVariantView {
    pub fn trace_completeness_flags(&self) -> u32 {
        self.trace_completeness_flags
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CBuildVariantViewError {
    MissingKickEvidencePointer {
        count: u32,
    },
    KickEvidenceCountExceeded {
        count: u32,
        max: usize,
    },
    MissingOperationOrderPointer {
        count: u16,
    },
    MissingTraceStepsPointer {
        count: u16,
    },
    OperationOrderCountExceeded {
        count: u16,
        max: usize,
    },
    TraceStepCountExceeded {
        count: u16,
        max: usize,
    },
    TraceCountMismatch {
        operation_count: u16,
        trace_count: u16,
    },
    OperationOrderMismatch {
        step_index: usize,
        order_operation_id: u16,
        trace_operation_id: u16,
    },
    KickEvidenceIndexOutOfRange {
        step_index: usize,
        evidence_index: u8,
        evidence_count: usize,
    },
}

#[cfg(test)]
#[path = "build_variant_view_tests.rs"]
mod tests;
