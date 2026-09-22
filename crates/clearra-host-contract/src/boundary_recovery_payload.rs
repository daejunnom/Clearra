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
    /// Present only for canonical weighted pattern searches. Fixed-queue
    /// results retain the original v1 fields and omit this extension.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub population: Option<Box<BoundaryRecoveryPopulationPayload>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BoundaryRecoveryPopulationExamplePayload {
    pub pattern_index: usize,
    pub queue: String,
    pub status: String,
    pub stage_one_checkpoint_step: Option<usize>,
    pub checkpoint_is_pc: Option<bool>,
    pub borrowed_stage_two_count: usize,
    pub steps: Vec<BoundaryRecoveryStepPayload>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BoundaryRecoveryPopulationPayload {
    pub materialized_pattern_count: usize,
    pub total_possible_pattern_count: String,
    pub evaluated_pattern_count: usize,
    pub state_count: usize,
    pub complete: bool,
    pub normal_count: usize,
    pub pc_preserving_recovery_count: usize,
    pub non_pc_recovery_count: usize,
    pub no_path_count: usize,
    pub incomplete_count: usize,
    pub diagram_unavailable_count: usize,
    pub normal_probability: String,
    pub pc_preserving_recovery_probability: String,
    pub non_pc_recovery_probability: String,
    pub additional_recovery_probability: String,
    pub total_response_probability: String,
    pub no_path_probability: String,
    pub unknown_probability: String,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub normal_example: Option<BoundaryRecoveryPopulationExamplePayload>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub recovery_example: Option<BoundaryRecoveryPopulationExamplePayload>,
}

impl BoundaryRecoveryPopulationExamplePayload {
    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.queue.capacity() as u128)
            .checked_add(self.status.capacity() as u128)?
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

impl BoundaryRecoveryPopulationPayload {
    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let strings = [
            &self.total_possible_pattern_count,
            &self.normal_probability,
            &self.pc_preserving_recovery_probability,
            &self.non_pc_recovery_probability,
            &self.additional_recovery_probability,
            &self.total_response_probability,
            &self.no_path_probability,
            &self.unknown_probability,
        ];
        let mut bytes = strings.into_iter().try_fold(0_u128, |total, string| {
            total.checked_add(string.capacity() as u128)
        })?;
        if let Some(example) = &self.normal_example {
            bytes = bytes.checked_add(example.checked_retained_capacity_bytes()?)?;
        }
        if let Some(example) = &self.recovery_example {
            bytes = bytes.checked_add(example.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
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
        if let Some(population) = &self.population {
            bytes = bytes
                .checked_add(core::mem::size_of::<BoundaryRecoveryPopulationPayload>() as u128)?;
            bytes = bytes.checked_add(population.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}
