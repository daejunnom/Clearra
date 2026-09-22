//! Finite public evidence for one fixed-queue boundary-recovery search.

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BoundaryRecoveryStepPayload {
    pub source_queue_index: u8,
    pub piece: String,
    pub rotation: u8,
    pub x: i8,
    pub y: i8,
    pub hold_decision: String,
    pub placement_mask: String,
    pub cleared_row_mask: u32,
    pub board_after_mask: String,
    pub cleared_lines: u8,
    pub recognized_spin: bool,
    pub b2b_active_after: bool,
    pub stage_one_complete_after: bool,
}

impl BoundaryRecoveryStepPayload {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        [
            self.piece.capacity(),
            self.hold_decision.capacity(),
            self.placement_mask.capacity(),
            self.board_after_mask.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, bytes| total.checked_add(bytes as u128))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BoundaryRecoveryPayload {
    pub status: String,
    pub knowledge_basis: String,
    pub placement_role_scope: String,
    pub max_early_placements: u8,
    pub borrow_source_index: u8,
    pub borrow_placement_mask: String,
    pub normal_states: usize,
    pub recovery_states: usize,
    pub stage_one_checkpoint_step: Option<usize>,
    pub checkpoint_is_pc: Option<bool>,
    pub borrowed_stage_two_count: usize,
    pub steps: Vec<BoundaryRecoveryStepPayload>,
}

impl BoundaryRecoveryPayload {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.status.capacity() as u128)
            .checked_add(self.knowledge_basis.capacity() as u128)?
            .checked_add(self.placement_role_scope.capacity() as u128)?
            .checked_add(self.borrow_placement_mask.capacity() as u128)?
            .checked_add(
                (self.steps.capacity() as u128)
                    .checked_mul(core::mem::size_of::<BoundaryRecoveryStepPayload>() as u128)?,
            )?;
        for step in &self.steps {
            bytes = bytes.checked_add(step.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}
