//! Typed, user-facing finesse results.
//!
//! Numeric aggregates are kept as stable decimal strings so incomplete
//! materialized universes retain their explicit completeness and raw mass.

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_finesse::{ClassicInputAction, FinesseSequenceInput, GeometryActionKey};
use clearra_problem::{BuildProbabilityField, FinesseScoreRequest};

use crate::core_execution_result::CorePathStep;

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

    fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = self.policy.capacity() as u128;
        bytes = bytes.checked_add(
            self.solution_key
                .as_ref()
                .map_or(0, |value| value.capacity() as u128),
        )?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes::<usize>(&self.pattern_ids)?)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes::<PieceKind>(&self.queue)?)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes::<FinesseReportInput>(
            &self.input_sequence,
        )?)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes::<FinesseReportPlacement>(
            &self.placements,
        )?)?;
        Some(bytes)
    }

    fn checked_clone_nested_bytes(&self) -> Option<u128> {
        let mut bytes = self.policy.len() as u128;
        bytes = bytes.checked_add(
            self.solution_key
                .as_ref()
                .map_or(0, |value| value.len() as u128),
        )?;
        bytes = bytes.checked_add(checked_vec_len_bytes::<usize>(&self.pattern_ids)?)?;
        bytes = bytes.checked_add(checked_vec_len_bytes::<PieceKind>(&self.queue)?)?;
        bytes = bytes.checked_add(checked_vec_len_bytes::<FinesseReportInput>(
            &self.input_sequence,
        )?)?;
        bytes = bytes.checked_add(checked_vec_len_bytes::<FinesseReportPlacement>(
            &self.placements,
        )?)?;
        Some(bytes)
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

    fn checked_nested_retained_bytes(&self) -> Option<u128> {
        (self.solution_key.capacity() as u128).checked_add(self.average_inputs.capacity() as u128)
    }

    fn checked_clone_nested_bytes(&self) -> Option<u128> {
        (self.solution_key.len() as u128).checked_add(self.average_inputs.len() as u128)
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

    fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = (self.policy.capacity() as u128)
            .checked_add(self.overall_average_inputs.capacity() as u128)?;
        for value in [
            &self.oracle_on_covered_average_inputs,
            &self.information_penalty_inputs,
            &self.success_probability_gap,
            &self.successful_probability_mass,
        ] {
            bytes =
                bytes.checked_add(value.as_ref().map_or(0, |value| value.capacity() as u128))?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes::<FinesseSolutionAverage>(
            &self.solution_averages,
        )?)?;
        for average in &self.solution_averages {
            bytes = bytes.checked_add(average.checked_nested_retained_bytes()?)?;
        }
        Some(bytes)
    }

    fn checked_clone_nested_bytes(&self) -> Option<u128> {
        let mut bytes =
            (self.policy.len() as u128).checked_add(self.overall_average_inputs.len() as u128)?;
        for value in [
            &self.oracle_on_covered_average_inputs,
            &self.information_penalty_inputs,
            &self.success_probability_gap,
            &self.successful_probability_mass,
        ] {
            bytes = bytes.checked_add(value.as_ref().map_or(0, |value| value.len() as u128))?;
        }
        bytes = bytes.checked_add(checked_vec_len_bytes::<FinesseSolutionAverage>(
            &self.solution_averages,
        )?)?;
        for average in &self.solution_averages {
            bytes = bytes.checked_add(average.checked_clone_nested_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Eq, PartialEq)]
struct FinesseScoreRequestAuthority {
    field_base_words: [u64; 4],
    field_height: u8,
    request_placements: Vec<clearra_problem::FinessePlacement>,
    initial_cleared_rows: u32,
    materialized_pattern_count: usize,
    expected_success_path: Vec<CorePathStep>,
}

impl core::fmt::Debug for FinesseScoreRequestAuthority {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("FinesseScoreRequestAuthority")
    }
}

impl FinesseScoreRequestAuthority {
    fn new(
        field: BuildProbabilityField,
        request: &FinesseScoreRequest,
        materialized_pattern_count: usize,
        expected_success_path: &[CorePathStep],
    ) -> Self {
        Self {
            field_base_words: field.base_words(),
            field_height: field.height(),
            request_placements: request.placements().to_vec(),
            initial_cleared_rows: request.initial_cleared_rows(),
            materialized_pattern_count,
            expected_success_path: expected_success_path.to_vec(),
        }
    }

    fn matches(
        &self,
        field: BuildProbabilityField,
        request: &FinesseScoreRequest,
        materialized_pattern_count: usize,
        actual_path: &[CorePathStep],
        any_policy_succeeds: bool,
    ) -> bool {
        self.field_base_words == field.base_words()
            && self.field_height == field.height()
            && self.request_placements.as_slice() == request.placements()
            && self.initial_cleared_rows == request.initial_cleared_rows()
            && self.materialized_pattern_count == materialized_pattern_count
            && self.expected_success_path.len() == request.placements().len()
            && if any_policy_succeeds {
                !actual_path.is_empty() && actual_path == self.expected_success_path.as_slice()
            } else {
                actual_path.is_empty()
            }
    }

    fn checked_nested_retained_bytes(&self) -> Option<u128> {
        checked_vec_capacity_bytes::<clearra_problem::FinessePlacement>(&self.request_placements)
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_capacity_bytes::<CorePathStep>(
                    &self.expected_success_path,
                )?)
            })
    }

    fn checked_clone_nested_bytes(&self) -> Option<u128> {
        checked_vec_len_bytes::<clearra_problem::FinessePlacement>(&self.request_placements)
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_len_bytes::<CorePathStep>(
                    &self.expected_success_path,
                )?)
            })
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
    score_request_authority: Option<FinesseScoreRequestAuthority>,
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
            score_request_authority: None,
        }
    }

    pub fn with_representative_witness(mut self, witness: FinesseRepresentativeWitness) -> Self {
        self.representative_witness = Some(witness);
        // Public report mutation must invalidate the Core-only score authority.
        // The canonical producer attaches that authority only after the report
        // and witness have reached their terminal shape.
        self.score_request_authority = None;
        self
    }

    pub(crate) fn with_score_request_authority(
        mut self,
        field: BuildProbabilityField,
        request: &FinesseScoreRequest,
        materialized_pattern_count: usize,
        expected_success_path: &[CorePathStep],
    ) -> Self {
        self.score_request_authority = Some(FinesseScoreRequestAuthority::new(
            field,
            request,
            materialized_pattern_count,
            expected_success_path,
        ));
        self
    }

    /// Allocation-free proof check for the private query authority attached by
    /// the canonical finesse-score producer. The original request, producer
    /// pattern denominator, and expected path remain private; callers can only
    /// ask whether their retained query and public result metadata match them
    /// exactly.
    #[doc(hidden)]
    pub fn matches_score_request_authority(
        &self,
        field: BuildProbabilityField,
        request: &FinesseScoreRequest,
        materialized_pattern_count: usize,
        actual_path: &[CorePathStep],
        any_policy_succeeds: bool,
    ) -> bool {
        self.score_request_authority
            .as_ref()
            .is_some_and(|authority| {
                authority.matches(
                    field,
                    request,
                    materialized_pattern_count,
                    actual_path,
                    any_policy_succeeds,
                )
            })
    }

    pub(crate) fn checked_score_request_authority_clone_bytes(
        request: &FinesseScoreRequest,
        expected_success_path: &[CorePathStep],
    ) -> Option<u128> {
        checked_vec_len_bytes::<clearra_problem::FinessePlacement>(request.placements()).and_then(
            |bytes| {
                bytes.checked_add(checked_vec_len_bytes::<CorePathStep>(
                    expected_success_path,
                )?)
            },
        )
    }

    pub(crate) fn checked_score_request_authority_retained_bytes(&self) -> Option<u128> {
        self.score_request_authority.as_ref().map_or(
            Some(0),
            FinesseScoreRequestAuthority::checked_nested_retained_bytes,
        )
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

    /// Checked heap backing retained by the typed finesse report. Every owned
    /// `String` uses its actual capacity and every `Vec` includes its outer
    /// slot buffer as well as nested payloads.
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = (self.mode.capacity() as u128)
            .checked_add(self.metric.capacity() as u128)?
            .checked_add(self.pattern_knowledge.capacity() as u128)?;
        bytes = bytes.checked_add(
            self.exact_total_inputs
                .as_ref()
                .map_or(0, |value| value.capacity() as u128),
        )?;
        if let Some(witness) = &self.representative_witness {
            bytes = bytes.checked_add(witness.checked_nested_retained_bytes()?)?;
        }
        if let Some(authority) = &self.score_request_authority {
            bytes = bytes.checked_add(authority.checked_nested_retained_bytes()?)?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes::<FinessePolicyResult>(
            &self.policy_results,
        )?)?;
        for policy in &self.policy_results {
            bytes = bytes.checked_add(policy.checked_nested_retained_bytes()?)?;
        }
        Some(bytes)
    }

    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        let mut bytes = (self.mode.len() as u128)
            .checked_add(self.metric.len() as u128)?
            .checked_add(self.pattern_knowledge.len() as u128)?;
        bytes = bytes.checked_add(
            self.exact_total_inputs
                .as_ref()
                .map_or(0, |value| value.len() as u128),
        )?;
        if let Some(witness) = &self.representative_witness {
            bytes = bytes.checked_add(witness.checked_clone_nested_bytes()?)?;
        }
        if let Some(authority) = &self.score_request_authority {
            bytes = bytes.checked_add(authority.checked_clone_nested_bytes()?)?;
        }
        bytes = bytes.checked_add(checked_vec_len_bytes::<FinessePolicyResult>(
            &self.policy_results,
        )?)?;
        for policy in &self.policy_results {
            bytes = bytes.checked_add(policy.checked_clone_nested_bytes()?)?;
        }
        Some(bytes)
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()?
            .checked_add(self.checked_clone_nested_bytes()?)
    }
}

fn checked_vec_capacity_bytes<T>(values: &Vec<T>) -> Option<u128> {
    (values.capacity() as u128).checked_mul(core::mem::size_of::<T>() as u128)
}

fn checked_vec_len_bytes<T>(values: &[T]) -> Option<u128> {
    (values.len() as u128).checked_mul(core::mem::size_of::<T>() as u128)
}

#[cfg(test)]
mod memory_projection_tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_problem::{BuildProbabilityField, FinessePlacement, FinesseScoreRequest};

    use super::{
        FinessePolicyResult, FinesseReport, FinesseReportInput, FinesseReportPlacement,
        FinesseRepresentativeWitness, FinesseSolutionAverage,
    };
    use crate::CorePathStep;

    fn reserved(value: &str, capacity: usize) -> String {
        let mut result = String::with_capacity(capacity);
        result.push_str(value);
        result
    }

    #[test]
    fn report_projection_counts_actual_nested_capacities_and_clone_peak() {
        let mut pattern_ids = Vec::with_capacity(5);
        pattern_ids.push(0);
        let mut queue = Vec::with_capacity(7);
        queue.push(PieceKind::T);
        let mut inputs = Vec::with_capacity(11);
        inputs.push(FinesseReportInput::HardDrop);
        let mut placements = Vec::with_capacity(3);
        placements.push(FinesseReportPlacement::new(
            PieceKind::T,
            clearra_core_domain::piece::rotation::RotationState::Zero,
            0,
            0,
        ));
        let witness = FinesseRepresentativeWitness::new(
            reserved("policy", 31),
            Some(reserved("solution", 37)),
            pattern_ids,
            queue,
            1,
            inputs,
            placements,
        );

        let average =
            FinesseSolutionAverage::new(reserved("solution", 41), reserved("1", 43), true);
        let mut averages = Vec::with_capacity(4);
        averages.push(average);
        let policy = FinessePolicyResult {
            policy: reserved("policy", 47),
            overall_average_inputs: reserved("1", 53),
            complete: true,
            oracle_on_covered_average_inputs: Some(reserved("1", 59)),
            information_penalty_inputs: Some(reserved("0", 61)),
            success_probability_gap: Some(reserved("0", 67)),
            successful_probability_mass: Some(reserved("1", 71)),
            successful_unique_queue_count: Some(1),
            total_unique_queue_count: Some(1),
            solution_averages: averages,
        };
        let mut policies = Vec::with_capacity(6);
        policies.push(policy);
        let report = FinesseReport {
            mode: reserved("score", 73),
            metric: reserved("inputs", 79),
            pattern_knowledge: reserved("oracle", 83),
            complete: true,
            exact_total_inputs: Some(reserved("1", 89)),
            representative_witness: Some(witness),
            policy_results: policies,
            score_request_authority: None,
        };

        let retained = report
            .checked_nested_retained_bytes()
            .expect("checked retained finesse bytes");
        let clone = report
            .checked_clone_nested_bytes()
            .expect("checked clone finesse bytes");
        assert!(retained > clone);
        assert_eq!(
            report.checked_clone_peak_bytes(),
            retained.checked_add(clone)
        );
        assert!(
            retained
                >= (6 * core::mem::size_of::<FinessePolicyResult>()
                    + 4 * core::mem::size_of::<FinesseSolutionAverage>()
                    + 5 * core::mem::size_of::<usize>()
                    + 7 * core::mem::size_of::<PieceKind>()
                    + 11 * core::mem::size_of::<FinesseReportInput>()
                    + 3 * core::mem::size_of::<FinesseReportPlacement>())
                    as u128
        );
    }

    #[test]
    fn score_request_authority_is_private_exact_and_counted_in_report_heap_projections() {
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("score field");
        let request = FinesseScoreRequest::new(vec![FinessePlacement::new(
            PieceKind::I,
            RotationState::Zero,
            0,
            0,
        )])
        .expect("score request");
        let path = vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)];
        let base = FinesseReport::new("score", "oracle", true, None, Vec::new());
        let retained_before = base.checked_nested_retained_bytes().unwrap();
        let clone_before = base.checked_clone_nested_bytes().unwrap();
        let report = base.with_score_request_authority(field, &request, 1, &path);

        assert!(report.matches_score_request_authority(field, &request, 1, &path, true));
        assert!(report.matches_score_request_authority(field, &request, 1, &[], false));
        assert!(!report.matches_score_request_authority(field, &request, 2, &path, true));
        let different_request = FinesseScoreRequest::new(vec![FinessePlacement::new(
            PieceKind::I,
            RotationState::Zero,
            1,
            0,
        )])
        .unwrap();
        assert!(!report.matches_score_request_authority(field, &different_request, 1, &path, true));
        assert!(
            report.checked_nested_retained_bytes().unwrap() - retained_before
                >= (core::mem::size_of::<FinessePlacement>() + core::mem::size_of::<CorePathStep>())
                    as u128
        );
        assert_eq!(
            report.checked_clone_nested_bytes().unwrap() - clone_before,
            (core::mem::size_of::<FinessePlacement>() + core::mem::size_of::<CorePathStep>())
                as u128
        );
    }
}
