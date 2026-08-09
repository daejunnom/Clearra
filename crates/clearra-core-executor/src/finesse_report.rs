//! Typed, user-facing finesse results.
//!
//! Numeric aggregates are kept as stable decimal strings so incomplete
//! materialized universes retain their explicit completeness and raw mass.

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_finesse::{ClassicInputAction, FinesseSequenceInput, GeometryActionKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinesseReportInput {
    Hold,
    TapLeft,
    TapRight,
    DasLeft,
    DasRight,
    RotateClockwise,
    RotateCounterClockwise,
    Rotate180,
    SoftDrop,
    HardDrop,
}

impl FinesseReportInput {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::TapLeft => "tap-left",
            Self::TapRight => "tap-right",
            Self::DasLeft => "das-left",
            Self::DasRight => "das-right",
            Self::RotateClockwise => "rotate-clockwise",
            Self::RotateCounterClockwise => "rotate-counter-clockwise",
            Self::Rotate180 => "rotate-180",
            Self::SoftDrop => "soft-drop",
            Self::HardDrop => "hard-drop",
        }
    }
}

impl From<FinesseSequenceInput> for FinesseReportInput {
    fn from(value: FinesseSequenceInput) -> Self {
        match value {
            FinesseSequenceInput::Hold => Self::Hold,
            FinesseSequenceInput::Movement(action) => match action {
                ClassicInputAction::TapLeft => Self::TapLeft,
                ClassicInputAction::TapRight => Self::TapRight,
                ClassicInputAction::DasLeft => Self::DasLeft,
                ClassicInputAction::DasRight => Self::DasRight,
                ClassicInputAction::RotateClockwise => Self::RotateClockwise,
                ClassicInputAction::RotateCounterClockwise => Self::RotateCounterClockwise,
                ClassicInputAction::Rotate180 => Self::Rotate180,
                ClassicInputAction::SoftDrop => Self::SoftDrop,
                ClassicInputAction::HardDrop => Self::HardDrop,
            },
        }
    }
}

/// One exact lock selected by the representative policy replay. Coordinates
/// use the same lower-left board convention as the search geometry contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinesseReportPlacement {
    piece: PieceKind,
    rotation: RotationState,
    x: i16,
    y: i16,
}

impl From<GeometryActionKey> for FinesseReportPlacement {
    fn from(value: GeometryActionKey) -> Self {
        Self {
            piece: value.piece(),
            rotation: value.rotation(),
            x: value.x(),
            y: value.y(),
        }
    }
}

impl FinesseReportPlacement {
    pub const fn new(piece: PieceKind, rotation: RotationState, x: i16, y: i16) -> Self {
        Self {
            piece,
            rotation,
            x,
            y,
        }
    }

    pub const fn piece(self) -> PieceKind {
        self.piece
    }

    pub const fn rotation(self) -> RotationState {
        self.rotation
    }

    pub const fn x(self) -> i16 {
        self.x
    }

    pub const fn y(self) -> i16 {
        self.y
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinesseRepresentativeWitness {
    policy: String,
    solution_key: Option<String>,
    pattern_ids: Vec<usize>,
    queue: Vec<PieceKind>,
    total_inputs: u32,
    input_sequence: Vec<FinesseReportInput>,
    placements: Vec<FinesseReportPlacement>,
}

impl FinesseRepresentativeWitness {
    pub fn new(
        policy: impl Into<String>,
        solution_key: Option<String>,
        pattern_ids: Vec<usize>,
        queue: Vec<PieceKind>,
        total_inputs: u32,
        input_sequence: Vec<FinesseReportInput>,
        placements: Vec<FinesseReportPlacement>,
    ) -> Self {
        Self {
            policy: policy.into(),
            solution_key,
            pattern_ids,
            queue,
            total_inputs,
            input_sequence,
            placements,
        }
    }

    pub fn policy(&self) -> &str {
        &self.policy
    }

    pub fn solution_key(&self) -> Option<&str> {
        self.solution_key.as_deref()
    }

    pub fn pattern_ids(&self) -> &[usize] {
        &self.pattern_ids
    }

    pub fn queue(&self) -> &[PieceKind] {
        &self.queue
    }

    pub const fn total_inputs(&self) -> u32 {
        self.total_inputs
    }

    pub fn input_sequence(&self) -> &[FinesseReportInput] {
        &self.input_sequence
    }

    pub fn placements(&self) -> &[FinesseReportPlacement] {
        &self.placements
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinesseSolutionAverage {
    solution_key: String,
    average_inputs: String,
    complete: bool,
}

impl FinesseSolutionAverage {
    pub fn new(
        solution_key: impl Into<String>,
        average_inputs: impl Into<String>,
        complete: bool,
    ) -> Self {
        Self {
            solution_key: solution_key.into(),
            average_inputs: average_inputs.into(),
            complete,
        }
    }

    pub fn solution_key(&self) -> &str {
        &self.solution_key
    }

    pub fn average_inputs(&self) -> &str {
        &self.average_inputs
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinessePolicyResult {
    policy: String,
    overall_average_inputs: String,
    complete: bool,
    oracle_on_covered_average_inputs: Option<String>,
    information_penalty_inputs: Option<String>,
    success_probability_gap: Option<String>,
    successful_probability_mass: Option<String>,
    successful_unique_queue_count: Option<usize>,
    total_unique_queue_count: Option<usize>,
    solution_averages: Vec<FinesseSolutionAverage>,
}

impl FinessePolicyResult {
    pub fn new(
        policy: impl Into<String>,
        overall_average_inputs: impl Into<String>,
        complete: bool,
        solution_averages: Vec<FinesseSolutionAverage>,
    ) -> Self {
        Self {
            policy: policy.into(),
            overall_average_inputs: overall_average_inputs.into(),
            complete,
            oracle_on_covered_average_inputs: None,
            information_penalty_inputs: None,
            success_probability_gap: None,
            successful_probability_mass: None,
            successful_unique_queue_count: None,
            total_unique_queue_count: None,
            solution_averages,
        }
    }

    pub fn with_success_summary(
        mut self,
        successful_probability_mass: impl Into<String>,
        successful_unique_queue_count: usize,
        total_unique_queue_count: usize,
    ) -> Self {
        self.successful_probability_mass = Some(successful_probability_mass.into());
        self.successful_unique_queue_count = Some(successful_unique_queue_count);
        self.total_unique_queue_count = Some(total_unique_queue_count);
        self
    }

    pub fn with_comparison(
        mut self,
        oracle_on_covered_average_inputs: Option<String>,
        information_penalty_inputs: Option<String>,
        success_probability_gap: Option<String>,
    ) -> Self {
        self.oracle_on_covered_average_inputs = oracle_on_covered_average_inputs;
        self.information_penalty_inputs = information_penalty_inputs;
        self.success_probability_gap = success_probability_gap;
        self
    }

    pub fn policy(&self) -> &str {
        &self.policy
    }

    pub fn overall_average_inputs(&self) -> &str {
        &self.overall_average_inputs
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn oracle_on_covered_average_inputs(&self) -> Option<&str> {
        self.oracle_on_covered_average_inputs.as_deref()
    }

    pub fn information_penalty_inputs(&self) -> Option<&str> {
        self.information_penalty_inputs.as_deref()
    }

    pub fn success_probability_gap(&self) -> Option<&str> {
        self.success_probability_gap.as_deref()
    }

    pub fn successful_probability_mass(&self) -> Option<&str> {
        self.successful_probability_mass.as_deref()
    }

    pub const fn successful_unique_queue_count(&self) -> Option<usize> {
        self.successful_unique_queue_count
    }

    pub const fn total_unique_queue_count(&self) -> Option<usize> {
        self.total_unique_queue_count
    }

    pub fn solution_averages(&self) -> &[FinesseSolutionAverage] {
        &self.solution_averages
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinesseReport {
    mode: String,
    metric: String,
    pattern_knowledge: String,
    complete: bool,
    exact_total_inputs: Option<String>,
    representative_witness: Option<FinesseRepresentativeWitness>,
    policy_results: Vec<FinessePolicyResult>,
}

impl FinesseReport {
    pub fn new(
        mode: impl Into<String>,
        pattern_knowledge: impl Into<String>,
        complete: bool,
        exact_total_inputs: Option<String>,
        policy_results: Vec<FinessePolicyResult>,
    ) -> Self {
        Self {
            mode: mode.into(),
            metric: "inputs".to_owned(),
            pattern_knowledge: pattern_knowledge.into(),
            complete,
            exact_total_inputs,
            representative_witness: None,
            policy_results,
        }
    }

    pub fn with_representative_witness(mut self, witness: FinesseRepresentativeWitness) -> Self {
        self.representative_witness = Some(witness);
        self
    }

    pub fn mode(&self) -> &str {
        &self.mode
    }

    pub fn metric(&self) -> &str {
        &self.metric
    }

    pub fn pattern_knowledge(&self) -> &str {
        &self.pattern_knowledge
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn exact_total_inputs(&self) -> Option<&str> {
        self.exact_total_inputs.as_deref()
    }

    pub fn representative_witness(&self) -> Option<&FinesseRepresentativeWitness> {
        self.representative_witness.as_ref()
    }

    pub fn policy_results(&self) -> &[FinessePolicyResult] {
        &self.policy_results
    }
}
