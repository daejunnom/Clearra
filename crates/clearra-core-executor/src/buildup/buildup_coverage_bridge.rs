mod constants {
    pub(crate) const OBSERVED_MATERIALIZED_PATTERN_SPECIFIC: &str =
        "observed-materialized-pattern-specific";
    pub(crate) const COVERED_PATTERN_BASIS_COMPLETE_PATTERN_UNIVERSE: &str =
        "complete_pattern_universe";
    pub(crate) const COVERED_PATTERN_BASIS_MATERIALIZED_PATTERN_UNIVERSE: &str =
        "materialized_pattern_universe";
}
mod coverage_identity {
    use clearra_problem::{SearchProblem, SearchProblemPreset};

    use crate::buildup::buildup_coverage_bridge::{
        coverage_pattern_selection::{coverage_pattern_id_source, pattern_count},
        coverage_universe_identity::CoverageUniverseIdentity,
    };

    pub(crate) fn coverage_universe_identity(problem: &SearchProblem) -> CoverageUniverseIdentity {
        let material = coverage_universe_material(problem);
        let source = problem.piece_source();
        let use_supply_universe = problem.preset() != SearchProblemPreset::Build;
        CoverageUniverseIdentity {
            piece_source_id: source.id().get(),
            pattern_universe_id: use_supply_universe
                .then(|| source.pattern_universe_id())
                .flatten()
                .map_or_else(
                    || stable_nonzero_identity(&format!("clearra:coverage-universe:{material}")),
                    |identity| identity.get(),
                ),
            pattern_weight_model_id: use_supply_universe
                .then(|| source.pattern_weight_model_id())
                .flatten()
                .map_or_else(
                    || {
                        stable_nonzero_identity(&format!(
                            "clearra:coverage-weight-model:{material}"
                        ))
                    },
                    |identity| identity.get(),
                ),
        }
    }

    fn coverage_universe_material(problem: &SearchProblem) -> String {
        let queue = problem.core_query().remaining_queue();
        let mut material = format!(
            "preset={};source={};patterns={};queue_mode={};queue_len={};max_pieces={}",
            problem.preset().as_str(),
            coverage_pattern_id_source(problem),
            pattern_count(problem),
            queue.mode(),
            queue.len(),
            problem.piece_window().max_pieces()
        );

        if let Some(query) = problem.build_query() {
            material.push_str(";build_template=");
            material.push_str(query.template().id());
            material.push_str(";build_slots=");
            material.push_str(&query.template().slot_count().to_string());
            material.push_str(";build_pattern_count=");
            material.push_str(&query.pattern_count().to_string());
        }
        if let Some(query) = problem.setup_query() {
            material.push_str(";setup_width=");
            material.push_str(&query.board_size().width().to_string());
            material.push_str(";setup_height=");
            material.push_str(&query.board_size().height().to_string());
            material.push_str(";setup_target_lines=");
            material.push_str(&query.target().lines().to_string());
        }
        material
    }

    fn stable_nonzero_identity(material: &str) -> u64 {
        const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
        const FNV_PRIME: u64 = 1_099_511_628_211;

        let mut hash = FNV_OFFSET;
        for byte in material.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash.max(1)
    }
}
mod coverage_pattern_selection {
    use clearra_pc_graph::request::PcQueueInput;
    use clearra_problem::{SearchProblem, SearchProblemPreset};

    use crate::buildup::buildup_error::BuildUpRunnerError;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum CoveragePatternSelection {
        Range { end_exclusive: usize },
        Single(u32),
    }

    impl CoveragePatternSelection {
        pub(crate) fn len(self) -> usize {
            match self {
                Self::Range { end_exclusive } => end_exclusive,
                Self::Single(_) => 1,
            }
        }

        fn first(self) -> Option<u32> {
            match self {
                Self::Range { end_exclusive } => (end_exclusive != 0).then_some(0),
                Self::Single(pattern_id) => Some(pattern_id),
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn coverage_pattern_id_for_problem(
        problem: &SearchProblem,
        pattern_count: usize,
    ) -> Result<u32, BuildUpRunnerError> {
        coverage_pattern_selection_for_problem(problem, pattern_count)?
            .first()
            .ok_or_else(|| pattern_out_of_range(problem, 0, pattern_count))
    }

    pub(crate) fn coverage_pattern_selection_for_problem(
        problem: &SearchProblem,
        pattern_count: usize,
    ) -> Result<CoveragePatternSelection, BuildUpRunnerError> {
        if pattern_count == 0 {
            return Err(pattern_out_of_range(problem, 0, pattern_count));
        }
        let selection = match problem.preset() {
            SearchProblemPreset::Build => {
                let query = problem.build_query();
                if let Some(pattern_id) = query.and_then(|query| query.selected_pattern_id()) {
                    if pattern_id >= pattern_count {
                        return Err(pattern_out_of_range(problem, pattern_id, pattern_count));
                    }
                    CoveragePatternSelection::Single(pattern_id as u32)
                } else {
                    let materialized_count = problem
                        .piece_source()
                        .materialized_universe()
                        .map_or(0, |universe| universe.pattern_count());
                    CoveragePatternSelection::Range {
                        end_exclusive: pattern_count.min(materialized_count),
                    }
                }
            }
            SearchProblemPreset::OpeningPc
            | SearchProblemPreset::ScenarioPc
            | SearchProblemPreset::Setup => CoveragePatternSelection::Range {
                end_exclusive: pattern_count,
            },
        };
        Ok(selection)
    }

    pub(crate) fn verified_pattern_count_for_problem(
        problem: &SearchProblem,
        pattern_count: usize,
    ) -> Result<usize, BuildUpRunnerError> {
        coverage_pattern_selection_for_problem(problem, pattern_count)
            .map(CoveragePatternSelection::len)
    }

    pub(crate) fn verified_pattern_count_for_execution(
        problem: &SearchProblem,
        pattern_count: usize,
        execution_complete: bool,
    ) -> Result<usize, BuildUpRunnerError> {
        if !execution_complete {
            return Ok(0);
        }
        verified_pattern_count_for_problem(problem, pattern_count)
    }

    pub(crate) fn coverage_pattern_id_source(problem: &SearchProblem) -> &'static str {
        match problem.preset() {
            SearchProblemPreset::Build => {
                if problem
                    .build_query()
                    .and_then(|query| query.selected_pattern_id())
                    .is_some()
                {
                    "build-selected-pattern-id"
                } else {
                    "build-materialized-pattern-universe"
                }
            }
            SearchProblemPreset::Setup => "setup-family-pattern-id",
            SearchProblemPreset::OpeningPc | SearchProblemPreset::ScenarioPc => {
                match problem.core_query().remaining_queue() {
                    PcQueueInput::FixedSequence(_) => "supply-provenance-pattern-id",
                    PcQueueInput::BagAlignedPattern(_) => "bag-aligned-pattern-id",
                    PcQueueInput::PatternExpression(_) => "materialized-pattern-expression-id",
                    PcQueueInput::Standard7Bag => "standard-7-bag-pattern-id",
                    PcQueueInput::Observed(_) => "observed-expansion-pattern-id",
                }
            }
        }
    }

    pub(crate) fn pattern_count(problem: &SearchProblem) -> usize {
        if problem.preset() == SearchProblemPreset::Build {
            return problem
                .build_query()
                .map(|query| query.pattern_count())
                .unwrap_or(1);
        }
        problem
            .piece_source()
            .materialized_universe()
            .map_or(0, |universe| universe.pattern_count())
    }

    fn pattern_out_of_range(
        problem: &SearchProblem,
        pattern_id: usize,
        pattern_count: usize,
    ) -> BuildUpRunnerError {
        BuildUpRunnerError::CoveragePatternIdOutOfRange {
            pattern_id,
            pattern_count,
            source: coverage_pattern_id_source(problem),
        }
    }
}
mod coverage_row_builder {
    use clearra_coverage::{
        matrix::coverage_row_bridge::CoverageRowBridgeError,
        row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
        universe::{
            pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
        },
    };

    use crate::buildup::{
        buildup_coverage_bridge::{
            coverage_universe_identity::CoverageUniverseIdentity,
            pattern_verified_candidate_coverage::PatternVerifiedCandidateCoverage,
            witnessed_pattern_coverage_accumulator::WitnessedPatternCoverageAccumulator,
        },
        buildup_error::BuildUpRunnerError,
        buildup_execution_mode::BuildUpExecutionMode,
    };

    #[cfg(test)]
    pub(crate) fn coverage_rows_from_build_variants(
        source_mode: BuildUpExecutionMode,
        variants: &[super::pattern_verified_build_variant::PatternVerifiedBuildVariant],
        pattern_count: usize,
        identity: CoverageUniverseIdentity,
    ) -> Result<Vec<CoverageRow>, BuildUpRunnerError> {
        coverage_rows_from_build_variants_with_cancellation(
            source_mode,
            variants,
            pattern_count,
            identity,
            &clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new(),
        )
    }

    #[cfg(test)]
    fn coverage_rows_from_build_variants_with_cancellation(
        source_mode: BuildUpExecutionMode,
        variants: &[super::pattern_verified_build_variant::PatternVerifiedBuildVariant],
        pattern_count: usize,
        identity: CoverageUniverseIdentity,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<Vec<CoverageRow>, BuildUpRunnerError> {
        let verifications = variants
            .iter()
            .map(|verified| {
                PatternVerifiedCandidateCoverage::new(
                    verified.variant().candidate_id(),
                    verified.verification(),
                )
            })
            .collect::<Vec<_>>();
        coverage_rows_from_pattern_verifications_with_cancellation(
            source_mode,
            &verifications,
            pattern_count,
            identity,
            cancellation,
        )
    }

    pub(crate) fn coverage_rows_from_pattern_verifications_with_cancellation(
        source_mode: BuildUpExecutionMode,
        verifications: &[PatternVerifiedCandidateCoverage],
        pattern_count: usize,
        identity: CoverageUniverseIdentity,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<Vec<CoverageRow>, BuildUpRunnerError> {
        if cancellation.is_cancelled() {
            return Err(BuildUpRunnerError::ExecutionCancelled);
        }
        validate_coverage_source(source_mode, identity)?;
        let mut rows = Vec::with_capacity(verifications.len());
        let mut current: Option<WitnessedPatternCoverageAccumulator> = None;
        for verified in verifications {
            if cancellation.is_cancelled() {
                return Err(BuildUpRunnerError::ExecutionCancelled);
            }
            let candidate_id = verified.candidate_id();
            if current
                .as_ref()
                .is_some_and(|accumulator| accumulator.candidate_id != candidate_id)
            {
                let completed = current.take().expect("a candidate accumulator was present");
                if candidate_id < completed.candidate_id {
                    return Err(BuildUpRunnerError::CoverageCandidateOrderViolation {
                        previous_candidate_id: completed.candidate_id,
                        candidate_id,
                    });
                }
                push_coverage_row(&mut rows, completed, identity)?;
            }
            let accumulator = current.get_or_insert_with(|| {
                WitnessedPatternCoverageAccumulator::new(candidate_id, pattern_count)
            });
            accumulator.record_verified_candidate(verified)?;
        }
        if let Some(completed) = current {
            push_coverage_row(&mut rows, completed, identity)?;
        }
        Ok(rows)
    }

    fn push_coverage_row(
        rows: &mut Vec<CoverageRow>,
        accumulator: WitnessedPatternCoverageAccumulator,
        identity: CoverageUniverseIdentity,
    ) -> Result<(), BuildUpRunnerError> {
        let candidate_id = accumulator.candidate_id;
        let coverage = accumulator.into_coverage_bits()?;
        if !coverage.is_empty() {
            rows.push(CoverageRow::new_with_piece_source(
                candidate_id,
                CoverageRowKind::Build,
                identity.piece_source_id,
                PatternUniverseId::new(identity.pattern_universe_id),
                PatternWeightModelId::new(identity.pattern_weight_model_id),
                coverage,
            ));
        }
        Ok(())
    }

    fn validate_coverage_source(
        source_mode: BuildUpExecutionMode,
        identity: CoverageUniverseIdentity,
    ) -> Result<(), BuildUpRunnerError> {
        if !source_mode.can_source_coverage() {
            return Err(BuildUpRunnerError::CoverageSourceModeRejected { mode: source_mode });
        }
        if identity.pattern_universe_id == 0 {
            return Err(BuildUpRunnerError::CoverageBridge(
                CoverageRowBridgeError::MissingPatternUniverseIdentity,
            ));
        }
        if identity.piece_source_id == 0 {
            return Err(BuildUpRunnerError::CoverageBridge(
                CoverageRowBridgeError::MissingPieceSourceIdentity,
            ));
        }
        if identity.pattern_weight_model_id == 0 {
            return Err(BuildUpRunnerError::CoverageBridge(
                CoverageRowBridgeError::MissingPatternWeightModelIdentity,
            ));
        }
        Ok(())
    }
}
mod coverage_source {
    use clearra_problem::SearchProblem;

    use crate::buildup::buildup_coverage_bridge::constants::{
        COVERED_PATTERN_BASIS_COMPLETE_PATTERN_UNIVERSE,
        COVERED_PATTERN_BASIS_MATERIALIZED_PATTERN_UNIVERSE,
    };

    pub(crate) fn coverage_source_for_problem(
        _problem: &SearchProblem,
        base_source: &'static str,
    ) -> &'static str {
        base_source
    }

    pub(crate) fn covered_pattern_count_basis_for_problem(problem: &SearchProblem) -> &'static str {
        if problem.piece_source().complete() {
            COVERED_PATTERN_BASIS_COMPLETE_PATTERN_UNIVERSE
        } else {
            COVERED_PATTERN_BASIS_MATERIALIZED_PATTERN_UNIVERSE
        }
    }
}
mod coverage_universe_identity {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) struct CoverageUniverseIdentity {
        pub(crate) piece_source_id: u64,
        pub(crate) pattern_universe_id: u64,
        pub(crate) pattern_weight_model_id: u64,
    }
}
mod pattern_coverage_verification {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) struct PatternCoverageVerification {
        pattern_id: u32,
    }

    impl PatternCoverageVerification {
        pub(crate) const fn pattern_specific_buildup(pattern_id: u32) -> Self {
            Self { pattern_id }
        }

        pub(crate) const fn pattern_id(self) -> u32 {
            self.pattern_id
        }
    }
}
mod pattern_verified_candidate_coverage {
    use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

    use crate::buildup::buildup_coverage_bridge::pattern_coverage_verification::PatternCoverageVerification;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum CandidateCoverageEvidence {
        Single(PatternCoverageVerification),
        ExactGeometryHoldProduct(PatternBitSet),
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(crate) struct PatternVerifiedCandidateCoverage {
        candidate_id: u64,
        evidence: CandidateCoverageEvidence,
    }

    impl PatternVerifiedCandidateCoverage {
        pub(crate) const fn new(
            candidate_id: u64,
            verification: PatternCoverageVerification,
        ) -> Self {
            Self {
                candidate_id,
                evidence: CandidateCoverageEvidence::Single(verification),
            }
        }

        pub(crate) fn exact_geometry_hold_product(
            candidate_id: u64,
            coverage_bits: PatternBitSet,
        ) -> Self {
            Self {
                candidate_id,
                evidence: CandidateCoverageEvidence::ExactGeometryHoldProduct(coverage_bits),
            }
        }

        pub(crate) const fn candidate_id(&self) -> u64 {
            self.candidate_id
        }

        pub(crate) fn single_verification(&self) -> Option<PatternCoverageVerification> {
            match self.evidence {
                CandidateCoverageEvidence::Single(verification) => Some(verification),
                CandidateCoverageEvidence::ExactGeometryHoldProduct(_) => None,
            }
        }

        pub(crate) fn exact_product_coverage_bits(&self) -> Option<&PatternBitSet> {
            match &self.evidence {
                CandidateCoverageEvidence::Single(_) => None,
                CandidateCoverageEvidence::ExactGeometryHoldProduct(bits) => Some(bits),
            }
        }
    }
}
#[cfg(test)]
mod pattern_verified_build_variant {
    use clearra_core_ffi::CBuildVariantView;

    use crate::buildup::{
        buildup_coverage_bridge::pattern_coverage_verification::PatternCoverageVerification,
        buildup_error::BuildUpRunnerError,
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(crate) struct PatternVerifiedBuildVariant {
        variant: CBuildVariantView,
        verification: PatternCoverageVerification,
    }

    impl PatternVerifiedBuildVariant {
        pub(crate) fn try_new(
            variant: CBuildVariantView,
            verification: PatternCoverageVerification,
        ) -> Result<Self, BuildUpRunnerError> {
            if variant.coverage_pattern_id() != verification.pattern_id() {
                return Err(BuildUpRunnerError::CoveragePatternVerificationMismatch {
                    variant_pattern_id: variant.coverage_pattern_id(),
                    verified_pattern_id: verification.pattern_id(),
                });
            }
            Ok(Self {
                variant,
                verification,
            })
        }

        pub(crate) fn variant(&self) -> &CBuildVariantView {
            &self.variant
        }

        pub(crate) const fn verification(&self) -> PatternCoverageVerification {
            self.verification
        }
    }
}
mod witnessed_pattern_coverage_accumulator {
    use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

    use crate::buildup::{
        buildup_coverage_bridge::pattern_verified_candidate_coverage::PatternVerifiedCandidateCoverage,
        buildup_error::BuildUpRunnerError,
    };

    pub(super) struct WitnessedPatternCoverageAccumulator {
        pub(super) candidate_id: u64,
        pattern_count: usize,
        verified_pattern_bits: Option<PatternBitSet>,
        verified_single_pattern_ids: Vec<u32>,
    }

    impl WitnessedPatternCoverageAccumulator {
        pub(super) fn new(candidate_id: u64, pattern_count: usize) -> Self {
            Self {
                candidate_id,
                pattern_count,
                verified_pattern_bits: None,
                verified_single_pattern_ids: Vec::new(),
            }
        }

        #[cfg(test)]
        pub(super) fn record_verified_variant(
            &mut self,
            verified: &super::pattern_verified_build_variant::PatternVerifiedBuildVariant,
        ) -> Result<(), BuildUpRunnerError> {
            self.record_pattern_id(verified.verification().pattern_id())
        }

        pub(super) fn record_verified_candidate(
            &mut self,
            verified: &PatternVerifiedCandidateCoverage,
        ) -> Result<(), BuildUpRunnerError> {
            if let Some(verification) = verified.single_verification() {
                self.record_pattern_id(verification.pattern_id())
            } else if let Some(bits) = verified.exact_product_coverage_bits() {
                match &mut self.verified_pattern_bits {
                    Some(coverage) => coverage
                        .union_with(bits)
                        .map_err(BuildUpRunnerError::Pattern),
                    None if bits.pattern_count() == self.pattern_count => {
                        self.verified_pattern_bits = Some(bits.clone());
                        Ok(())
                    }
                    None => Err(BuildUpRunnerError::Pattern(
                        clearra_coverage::pattern::pattern_bitset::PatternBitSetError::PatternUniverseMismatch {
                            left: self.pattern_count,
                            right: bits.pattern_count(),
                        },
                    )),
                }
            } else {
                Ok(())
            }
        }

        fn record_pattern_id(&mut self, pattern_id: u32) -> Result<(), BuildUpRunnerError> {
            if pattern_id as usize >= self.pattern_count {
                return Err(BuildUpRunnerError::Pattern(
                    clearra_coverage::pattern::pattern_bitset::PatternBitSetError::PatternOutOfRange {
                        index: pattern_id as usize,
                        pattern_count: self.pattern_count,
                    },
                ));
            }
            self.verified_single_pattern_ids.push(pattern_id);
            Ok(())
        }

        pub(super) fn into_coverage_bits(self) -> Result<PatternBitSet, BuildUpRunnerError> {
            let scalar_bits = PatternBitSet::from_pattern_indices(
                self.pattern_count,
                self.verified_single_pattern_ids,
            )
            .map_err(BuildUpRunnerError::Pattern)?;
            match self.verified_pattern_bits {
                Some(mut product_bits) => {
                    product_bits
                        .union_with(&scalar_bits)
                        .map_err(BuildUpRunnerError::Pattern)?;
                    Ok(product_bits)
                }
                None => Ok(scalar_bits),
            }
        }
    }
}

pub(crate) use constants::{
    COVERED_PATTERN_BASIS_COMPLETE_PATTERN_UNIVERSE,
    COVERED_PATTERN_BASIS_MATERIALIZED_PATTERN_UNIVERSE, OBSERVED_MATERIALIZED_PATTERN_SPECIFIC,
};
pub(crate) use coverage_identity::coverage_universe_identity;
pub(crate) use coverage_pattern_selection::{
    coverage_pattern_selection_for_problem, pattern_count, verified_pattern_count_for_execution,
    CoveragePatternSelection,
};
#[cfg(test)]
pub(crate) use coverage_row_builder::coverage_rows_from_build_variants;
pub(crate) use coverage_row_builder::coverage_rows_from_pattern_verifications_with_cancellation;
pub(crate) use coverage_source::{
    coverage_source_for_problem, covered_pattern_count_basis_for_problem,
};
pub(crate) use coverage_universe_identity::CoverageUniverseIdentity;
pub(crate) use pattern_coverage_verification::PatternCoverageVerification;
#[cfg(test)]
pub(crate) use pattern_verified_build_variant::PatternVerifiedBuildVariant;
pub(crate) use pattern_verified_candidate_coverage::PatternVerifiedCandidateCoverage;

#[cfg(test)]
use clearra_core_ffi::{CBuildVariantView, CNativeBuildVariantView};
#[cfg(test)]
use clearra_coverage::pattern::pattern_id::PatternId;
#[cfg(test)]
use witnessed_pattern_coverage_accumulator::WitnessedPatternCoverageAccumulator;
#[cfg(test)]
#[path = "buildup_coverage_bridge_tests.rs"]
mod tests;
