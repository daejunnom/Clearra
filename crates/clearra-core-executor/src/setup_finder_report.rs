use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_problem::SetupSearchMode;
use clearra_supply::QueueObservationPolicy;

use crate::CorePathStep;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupFinderReport {
    search_mode: SetupSearchMode,
    queue_observation_policy: QueueObservationPolicy,
    cycle: u8,
    remaining_pieces: String,
    queue_based_pieces: String,
    next_cycle_remaining_pieces: String,
    post_cycle_borrow_enabled: bool,
    geometry_family_count: String,
    partial_build_node_count: usize,
    complete: bool,
    hold_conditions: Vec<SetupHoldConditionReport>,
}

impl SetupFinderReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        search_mode: SetupSearchMode,
        queue_observation_policy: QueueObservationPolicy,
        cycle: u8,
        remaining_pieces: String,
        queue_based_pieces: String,
        next_cycle_remaining_pieces: String,
        post_cycle_borrow_enabled: bool,
        geometry_family_count: String,
        partial_build_node_count: usize,
        complete: bool,
        hold_conditions: Vec<SetupHoldConditionReport>,
    ) -> Self {
        Self {
            search_mode,
            queue_observation_policy,
            cycle,
            remaining_pieces,
            queue_based_pieces,
            next_cycle_remaining_pieces,
            post_cycle_borrow_enabled,
            geometry_family_count,
            partial_build_node_count,
            complete,
            hold_conditions,
        }
    }

    pub fn search_mode(&self) -> SetupSearchMode {
        self.search_mode
    }

    pub fn queue_observation_policy(&self) -> QueueObservationPolicy {
        self.queue_observation_policy
    }

    pub fn cycle(&self) -> u8 {
        self.cycle
    }

    pub fn remaining_pieces(&self) -> &str {
        &self.remaining_pieces
    }

    pub fn queue_based_pieces(&self) -> &str {
        &self.queue_based_pieces
    }

    pub fn next_cycle_remaining_pieces(&self) -> &str {
        &self.next_cycle_remaining_pieces
    }

    pub fn post_cycle_borrow_enabled(&self) -> bool {
        self.post_cycle_borrow_enabled
    }

    pub fn geometry_family_count(&self) -> &str {
        &self.geometry_family_count
    }

    pub fn partial_build_node_count(&self) -> usize {
        self.partial_build_node_count
    }

    pub fn complete(&self) -> bool {
        self.complete
    }

    pub fn coverage_semantics(&self) -> &'static str {
        self.queue_observation_policy.coverage_semantics()
    }

    pub const fn continuation_supply_semantics(&self) -> &'static str {
        "exact-post-setup-hold-queue-state"
    }

    pub fn hold_conditions(&self) -> &[SetupHoldConditionReport] {
        &self.hold_conditions
    }

    pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = 0_u128;
        for value in [
            &self.remaining_pieces,
            &self.queue_based_pieces,
            &self.next_cycle_remaining_pieces,
            &self.geometry_family_count,
        ] {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        bytes = bytes.checked_add(
            (self.hold_conditions.capacity() as u128)
                .checked_mul(core::mem::size_of::<SetupHoldConditionReport>() as u128)?,
        )?;
        for condition in &self.hold_conditions {
            bytes = bytes.checked_add(condition.checked_nested_retained_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupHoldConditionReport {
    condition_id: String,
    initial_hold: Option<PieceKind>,
    pattern_expression: String,
    pattern_count: usize,
    candidate_count: usize,
    result_truncated: bool,
    complete: bool,
    candidates: Vec<SetupCandidateReport>,
}

impl SetupHoldConditionReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        condition_id: String,
        initial_hold: Option<PieceKind>,
        pattern_expression: String,
        pattern_count: usize,
        candidate_count: usize,
        result_truncated: bool,
        complete: bool,
        candidates: Vec<SetupCandidateReport>,
    ) -> Self {
        Self {
            condition_id,
            initial_hold,
            pattern_expression,
            pattern_count,
            candidate_count,
            result_truncated,
            complete,
            candidates,
        }
    }

    pub fn condition_id(&self) -> &str {
        &self.condition_id
    }

    pub fn initial_hold(&self) -> Option<PieceKind> {
        self.initial_hold
    }

    pub fn pattern_expression(&self) -> &str {
        &self.pattern_expression
    }

    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub fn candidate_count(&self) -> usize {
        self.candidate_count
    }

    pub fn result_truncated(&self) -> bool {
        self.result_truncated
    }

    pub fn complete(&self) -> bool {
        self.complete
    }

    pub fn candidates(&self) -> &[SetupCandidateReport] {
        &self.candidates
    }

    pub(crate) fn path_detail(
        &self,
        setup_id: &str,
        solution_paths: Vec<Vec<CorePathStep>>,
    ) -> Option<Self> {
        let candidate = self
            .candidates
            .iter()
            .find(|candidate| candidate.setup_id() == setup_id)?
            .clone()
            .with_solution_paths(solution_paths);
        Some(Self::new(
            self.condition_id.clone(),
            self.initial_hold,
            self.pattern_expression.clone(),
            self.pattern_count,
            1,
            false,
            self.complete,
            vec![candidate],
        ))
    }

    fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = (self.condition_id.capacity() as u128)
            .checked_add(self.pattern_expression.capacity() as u128)?;
        bytes = bytes.checked_add(
            (self.candidates.capacity() as u128)
                .checked_mul(core::mem::size_of::<SetupCandidateReport>() as u128)?,
        )?;
        for candidate in &self.candidates {
            bytes = bytes.checked_add(candidate.checked_nested_retained_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupCandidateReport {
    setup_id: String,
    board_mask: u64,
    min_locks: u8,
    max_locks: u8,
    build_covered_patterns: usize,
    joint_covered_patterns: usize,
    build_probability: String,
    joint_probability: String,
    conditional_pc_probability: String,
    representative_path: Vec<CorePathStep>,
    solution_paths: Vec<Vec<CorePathStep>>,
    solution_paths_complete: bool,
}

impl SetupCandidateReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        setup_id: String,
        board_mask: u64,
        min_locks: u8,
        max_locks: u8,
        build_covered_patterns: usize,
        joint_covered_patterns: usize,
        build_probability: String,
        joint_probability: String,
        conditional_pc_probability: String,
        representative_path: Vec<CorePathStep>,
    ) -> Self {
        Self {
            setup_id,
            board_mask,
            min_locks,
            max_locks,
            build_covered_patterns,
            joint_covered_patterns,
            build_probability,
            joint_probability,
            conditional_pc_probability,
            representative_path,
            solution_paths: Vec::new(),
            solution_paths_complete: false,
        }
    }

    pub fn with_solution_paths(mut self, solution_paths: Vec<Vec<CorePathStep>>) -> Self {
        self.solution_paths = solution_paths;
        self.solution_paths_complete = true;
        self
    }

    pub fn setup_id(&self) -> &str {
        &self.setup_id
    }

    pub fn board_mask(&self) -> u64 {
        self.board_mask
    }

    pub fn min_locks(&self) -> u8 {
        self.min_locks
    }

    pub fn max_locks(&self) -> u8 {
        self.max_locks
    }

    pub fn build_covered_patterns(&self) -> usize {
        self.build_covered_patterns
    }

    pub fn joint_covered_patterns(&self) -> usize {
        self.joint_covered_patterns
    }

    pub fn build_probability(&self) -> &str {
        &self.build_probability
    }

    pub fn joint_probability(&self) -> &str {
        &self.joint_probability
    }

    pub fn conditional_pc_probability(&self) -> &str {
        &self.conditional_pc_probability
    }

    pub fn representative_path(&self) -> &[CorePathStep] {
        &self.representative_path
    }

    pub fn solution_paths(&self) -> &[Vec<CorePathStep>] {
        &self.solution_paths
    }

    pub fn solution_path_count(&self) -> usize {
        self.solution_paths.len()
    }

    pub fn solution_paths_complete(&self) -> bool {
        self.solution_paths_complete
    }

    fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = 0_u128;
        for value in [
            &self.setup_id,
            &self.build_probability,
            &self.joint_probability,
            &self.conditional_pc_probability,
        ] {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        bytes = bytes.checked_add(
            (self.representative_path.capacity() as u128)
                .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?,
        )?;
        bytes = bytes.checked_add(
            (self.solution_paths.capacity() as u128)
                .checked_mul(core::mem::size_of::<Vec<CorePathStep>>() as u128)?,
        )?;
        for path in &self.solution_paths {
            bytes = bytes.checked_add(
                (path.capacity() as u128)
                    .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?,
            )?;
        }
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use clearra_problem::SetupSearchMode;
    use clearra_supply::QueueObservationPolicy;

    use super::SetupFinderReport;

    #[test]
    fn setup_report_declares_that_continuation_keeps_the_exact_supply_state() {
        let report = SetupFinderReport::new(
            SetupSearchMode::ShapeOracle,
            QueueObservationPolicy::FullQueueOracle,
            5,
            "SZ".to_owned(),
            String::new(),
            String::new(),
            false,
            "0".to_owned(),
            0,
            true,
            Vec::new(),
        );

        assert_eq!(
            report.continuation_supply_semantics(),
            "exact-post-setup-hold-queue-state"
        );
    }
}
