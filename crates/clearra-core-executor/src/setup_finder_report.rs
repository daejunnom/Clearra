use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::CorePathStep;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupFinderReport {
    cycle: u8,
    remaining_pieces: String,
    post_cycle_borrow_enabled: bool,
    geometry_family_count: String,
    partial_build_node_count: usize,
    complete: bool,
    hold_conditions: Vec<SetupHoldConditionReport>,
}

impl SetupFinderReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cycle: u8,
        remaining_pieces: String,
        post_cycle_borrow_enabled: bool,
        geometry_family_count: String,
        partial_build_node_count: usize,
        complete: bool,
        hold_conditions: Vec<SetupHoldConditionReport>,
    ) -> Self {
        Self {
            cycle,
            remaining_pieces,
            post_cycle_borrow_enabled,
            geometry_family_count,
            partial_build_node_count,
            complete,
            hold_conditions,
        }
    }

    pub fn cycle(&self) -> u8 {
        self.cycle
    }

    pub fn remaining_pieces(&self) -> &str {
        &self.remaining_pieces
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
        "oracle"
    }

    pub fn hold_conditions(&self) -> &[SetupHoldConditionReport] {
        &self.hold_conditions
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
        }
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
}
