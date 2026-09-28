//! Paired logical Build output; source counts use decimal strings on every host.

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RecoveryBuildStepPayload {
    pub source_index: String,
    pub result_target: bool,
    pub piece: String,
    pub rotation: u8,
    pub x: i8,
    pub y: i8,
    pub hold_decision: String,
    pub board_before_mask: String,
    pub placement_mask: String,
    pub board_after_mask: String,
    pub cleared_rows: u32,
    pub cleared_lines: u8,
    pub recognized_spin: bool,
    pub b2b_active: bool,
    pub middle_complete: bool,
}
impl RecoveryBuildStepPayload {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = 0_u128;
        bytes = bytes.checked_add(self.source_index.capacity() as u128)?;
        bytes = bytes.checked_add(self.piece.capacity() as u128)?;
        bytes = bytes.checked_add(self.hold_decision.capacity() as u128)?;
        bytes = bytes.checked_add(self.board_before_mask.capacity() as u128)?;
        bytes = bytes.checked_add(self.placement_mask.capacity() as u128)?;
        bytes = bytes.checked_add(self.board_after_mask.capacity() as u128)?;
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RecoveryBuildExamplePayload {
    pub first_pattern: String,
    pub second_pattern: String,
    pub first_queue: String,
    pub second_queue: String,
    pub status: String,
    pub terminal_board_mask: String,
    pub result_target_mask: String,
    pub effective_max_early: String,
    pub actual_early: String,
    pub exchange_balance: Vec<i16>,
    pub steps: Vec<RecoveryBuildStepPayload>,
}
impl RecoveryBuildExamplePayload {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = 0_u128;
        bytes = bytes.checked_add(self.first_pattern.capacity() as u128)?;
        bytes = bytes.checked_add(self.second_pattern.capacity() as u128)?;
        bytes = bytes.checked_add(self.first_queue.capacity() as u128)?;
        bytes = bytes.checked_add(self.second_queue.capacity() as u128)?;
        bytes = bytes.checked_add(self.status.capacity() as u128)?;
        bytes = bytes.checked_add(self.terminal_board_mask.capacity() as u128)?;
        bytes = bytes.checked_add(self.result_target_mask.capacity() as u128)?;
        bytes = bytes.checked_add(self.effective_max_early.capacity() as u128)?;
        bytes = bytes.checked_add(self.actual_early.capacity() as u128)?;
        bytes = bytes.checked_add(
            (self.exchange_balance.capacity() as u128)
                .checked_mul(core::mem::size_of::<i16>() as u128)?,
        )?;
        bytes = bytes.checked_add(
            (self.steps.capacity() as u128)
                .checked_mul(core::mem::size_of::<RecoveryBuildStepPayload>() as u128)?,
        )?;
        for value in &self.steps {
            bytes = bytes.checked_add(value.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RecoveryBuildPayload {
    pub input_identity: String,
    pub height: u8,
    pub start_board_mask: String,
    pub middle_target_mask: String,
    pub result_target_mask: String,
    pub first_supply: String,
    pub second_supply: String,
    pub early_limit: Option<String>,
    pub allow_piece_exchange: bool,
    pub hold_enabled: bool,
    pub preserve_b2b: bool,
    pub initial_b2b: bool,
    pub rule_profile: String,
    pub spin_profile: String,
    pub complete: bool,
    pub pattern_count: String,
    pub evaluated_pattern_count: String,
    pub normal_count: String,
    pub recovery_count: String,
    pub no_path_count: String,
    pub state_count: String,
    pub normal_probability: String,
    pub recovery_probability: String,
    pub no_path_probability: String,
    pub all_paths_enumerated: bool,
    pub examples: Vec<RecoveryBuildExamplePayload>,
}
impl RecoveryBuildPayload {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = 0_u128;
        bytes = bytes.checked_add(self.input_identity.capacity() as u128)?;
        bytes = bytes.checked_add(self.start_board_mask.capacity() as u128)?;
        bytes = bytes.checked_add(self.middle_target_mask.capacity() as u128)?;
        bytes = bytes.checked_add(self.result_target_mask.capacity() as u128)?;
        bytes = bytes.checked_add(self.first_supply.capacity() as u128)?;
        bytes = bytes.checked_add(self.second_supply.capacity() as u128)?;
        bytes = bytes.checked_add(
            self.early_limit
                .as_ref()
                .map_or(0, |value| value.capacity()) as u128,
        )?;
        bytes = bytes.checked_add(self.rule_profile.capacity() as u128)?;
        bytes = bytes.checked_add(self.spin_profile.capacity() as u128)?;
        bytes = bytes.checked_add(self.pattern_count.capacity() as u128)?;
        bytes = bytes.checked_add(self.evaluated_pattern_count.capacity() as u128)?;
        bytes = bytes.checked_add(self.normal_count.capacity() as u128)?;
        bytes = bytes.checked_add(self.recovery_count.capacity() as u128)?;
        bytes = bytes.checked_add(self.no_path_count.capacity() as u128)?;
        bytes = bytes.checked_add(self.state_count.capacity() as u128)?;
        bytes = bytes.checked_add(self.normal_probability.capacity() as u128)?;
        bytes = bytes.checked_add(self.recovery_probability.capacity() as u128)?;
        bytes = bytes.checked_add(self.no_path_probability.capacity() as u128)?;
        bytes = bytes.checked_add(
            (self.examples.capacity() as u128)
                .checked_mul(core::mem::size_of::<RecoveryBuildExamplePayload>() as u128)?,
        )?;
        for value in &self.examples {
            bytes = bytes.checked_add(value.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}
