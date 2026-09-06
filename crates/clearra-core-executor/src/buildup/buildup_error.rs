use clearra_core_domain::solution::NormalizedTilingSolutionError;
use clearra_core_ffi::{CBuildVariantViewError, FfiProblemError, NativeCoreError};
use clearra_coverage::{
    matrix::coverage_row_bridge::CoverageRowBridgeError,
    pattern::{pattern_bitset::PatternBitSetError, weighted_pattern_set::WeightedPatternSetError},
};

use crate::buildup::buildup_execution_mode::BuildUpExecutionMode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildUpRunnerError {
    Ffi(FfiProblemError),
    Native(NativeCoreError),
    NativeView(CBuildVariantViewError),
    CoveragePatternIdOutOfRange {
        pattern_id: usize,
        pattern_count: usize,
        source: &'static str,
    },
    UnsupportedPieceSource {
        reason: &'static str,
    },
    CoverageBridge(CoverageRowBridgeError),
    Pattern(PatternBitSetError),
    Weights(WeightedPatternSetError),
    CoverageSourceModeRejected {
        mode: BuildUpExecutionMode,
    },
    CoveragePatternVerificationMismatch {
        variant_pattern_id: u32,
        verified_pattern_id: u32,
    },
    DuplicateObjectiveCoverageCandidate {
        candidate_id: u64,
    },
    CoverageCandidateOrderViolation {
        previous_candidate_id: u64,
        candidate_id: u64,
    },
    Objective,
    VariantCountOverflow {
        count: u64,
    },
    BuildVariantCountOverflow,
    PatternVerifiedExecutionCountOverflow,
    PackingCandidateUnavailable {
        candidate_index: usize,
    },
    SolutionProbabilityCandidateUnavailable {
        candidate_id: u64,
    },
    BuildUpResultCountMismatch {
        candidate_count: usize,
        result_count: usize,
    },
    BuildUpCandidateIdentityMismatch {
        candidate_index: usize,
        candidate_id: u64,
        result_candidate_id: u64,
    },
    GeometryLanguageIdentityMismatch {
        candidate_id: u64,
        language_candidate_id: u64,
    },
    GeometryLanguageTraceMismatch {
        candidate_id: u64,
        pattern_id: u32,
    },
    ExactGeometryLanguageRequired {
        candidate_id: u64,
        binding_count: usize,
    },
    PreverifiedBuildabilityMismatch {
        candidate_id: u64,
    },
    InvalidGeometryLanguage,
    PatternProductStorageUnavailable,
    UnknownPackingPieceCode {
        code: u8,
    },
    SolutionSetAllocationFailed,
    ParallelWorkerPanicked,
    NormalizedTiling(NormalizedTilingSolutionError),
    ExecutionCancelled,
}
