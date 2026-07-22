use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_core_ffi::problem::{
    C_PIECE_I, C_PIECE_J, C_PIECE_L, C_PIECE_O, C_PIECE_S, C_PIECE_T, C_PIECE_Z,
};
use clearra_core_ffi::{
    CBuildUpTraceStep, CBuildVariantView, CKickEvidenceView, CNativeBuildVariantView,
    CPackingCandidate, CPackingOperation, CReachabilityEvidenceView,
    CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING, C_BUILDUP_HOLD_BRANCH_CURRENT,
    C_BUILDUP_HOLD_BRANCH_SWAP_HELD,
};
use clearra_coverage::universe::{PatternUniverseId, PatternWeightModelId};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{BuildVariantOperation, HoldDecision, ReplayEvent};
use clearra_scoring::profile::ScoreProfile;
use clearra_scoring::spin::{
    ClassificationConfidence, SpinAccuracy, SpinClassification, SpinClassificationInput,
    SpinClassifier, SpinKind, SpinResult, SpinTarget,
    TraceCompleteness as ScoringTraceCompleteness,
};

use crate::spin::{
    build_variant_mapper::BuildVariantMapper, BuildVariantReplayEvidence,
    BuildVariantReplayEvidenceError, SpinTargetCoverageBridge, SpinTargetRunResult,
    SpinTargetRunner, SpinTargetRunnerError,
};

struct AlwaysTsdClassifier;

impl SpinClassifier for AlwaysTsdClassifier {
    fn classify(
        &self,
        _input: SpinClassificationInput,
        _profile: &ScoreProfile,
    ) -> SpinClassification {
        SpinClassification::new(
            SpinResult::new('T', SpinKind::TSpin, false, 2, true, SpinAccuracy::Exact),
            ClassificationConfidence::exact(),
        )
    }
}
struct NeverTsdClassifier;

impl SpinClassifier for NeverTsdClassifier {
    fn classify(
        &self,
        _input: SpinClassificationInput,
        _profile: &ScoreProfile,
    ) -> SpinClassification {
        SpinClassification::new(
            SpinResult::new('L', SpinKind::None, false, 0, false, SpinAccuracy::Exact),
            ClassificationConfidence::exact(),
        )
    }
}
struct RequiresKickEvidenceClassifier;

impl SpinClassifier for RequiresKickEvidenceClassifier {
    fn classify(
        &self,
        input: SpinClassificationInput,
        _profile: &ScoreProfile,
    ) -> SpinClassification {
        if input.kick_evidence.is_some()
            && input.trace_completeness == ScoringTraceCompleteness::Full
        {
            return SpinClassification::new(
                SpinResult::new('T', SpinKind::TSpin, false, 2, true, SpinAccuracy::Exact),
                ClassificationConfidence::exact(),
            );
        }

        SpinClassification::new(
            SpinResult::none('T', 2, SpinAccuracy::KickSensitiveUnavailable),
            ClassificationConfidence::new(0.0),
        )
    }
}
struct RequiresPriorBoardClassifier;

impl SpinClassifier for RequiresPriorBoardClassifier {
    fn classify(
        &self,
        input: SpinClassificationInput,
        _profile: &ScoreProfile,
    ) -> SpinClassification {
        if input.piece == 'T' && input.board_before != 0 {
            return SpinClassification::new(
                SpinResult::new('T', SpinKind::TSpin, false, 2, true, SpinAccuracy::Exact),
                ClassificationConfidence::exact(),
            );
        }

        SpinClassification::new(
            SpinResult::none(input.piece, 2, SpinAccuracy::Incomplete),
            ClassificationConfidence::new(0.0),
        )
    }
}
struct InputPieceClassifier;

impl SpinClassifier for InputPieceClassifier {
    fn classify(
        &self,
        input: SpinClassificationInput,
        _profile: &ScoreProfile,
    ) -> SpinClassification {
        if input.piece == 'T' {
            return SpinClassification::new(
                SpinResult::new('T', SpinKind::TSpin, false, 2, true, SpinAccuracy::Exact),
                ClassificationConfidence::exact(),
            );
        }

        SpinClassification::new(
            SpinResult::none(input.piece, input.cleared_lines, SpinAccuracy::Exact),
            ClassificationConfidence::exact(),
        )
    }
}

fn variant(key: u64, pattern_id: u32) -> CNativeBuildVariantView {
    CNativeBuildVariantView {
        candidate_id: key,
        build_variant_id: 1,
        canonical_operation_set_id: key,
        operation_set_hash: key,
        coverage_pattern_id: pattern_id,
        placed_count: 1,
        cleared_lines: 2,
        ..Default::default()
    }
}

fn replay_layout() -> Board64Layout {
    Board64Layout::standard_10_by_lines(4).expect("10x4 layout")
}

fn t_operation(x: u16, y: u16) -> BuildVariantOperation {
    BuildVariantOperation::new(PieceKind::T, RotationState::Zero, x, y)
}

fn t_right_operation(x: u16, y: u16) -> BuildVariantOperation {
    BuildVariantOperation::new(PieceKind::T, RotationState::Right, x, y)
}

fn o_operation(x: u16, y: u16) -> BuildVariantOperation {
    BuildVariantOperation::new(PieceKind::O, RotationState::Zero, x, y)
}

fn candidate_with_operations(operations: Vec<BuildVariantOperation>) -> CPackingCandidate {
    let mut candidate = CPackingCandidate {
        candidate_id: 7,
        operation_set_key: 0x777,
        operation_count: operations.len() as u16,
        ..Default::default()
    };

    for (index, operation) in operations.into_iter().enumerate() {
        candidate.operations[index] = CPackingOperation {
            piece: c_piece(operation.piece()),
            rotation: operation.rotation().quarter_turns(),
            x: operation.x() as i8,
            y: operation.y() as i8,
            operation_id: (index + 1) as u16,
            required_deleted_row_mask: 0,
            mask: operation.expected_mask().unwrap_or(0),
        };
    }

    candidate
}

fn c_piece(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => C_PIECE_I,
        PieceKind::O => C_PIECE_O,
        PieceKind::T => C_PIECE_T,
        PieceKind::S => C_PIECE_S,
        PieceKind::Z => C_PIECE_Z,
        PieceKind::J => C_PIECE_J,
        PieceKind::L => C_PIECE_L,
    }
}

fn evidence(key: u64, pattern_id: u32) -> BuildVariantReplayEvidence {
    evidence_with_operations(key, pattern_id, vec![t_operation(0, 0)])
}

fn evidence_with_operations(
    key: u64,
    pattern_id: u32,
    operations: Vec<BuildVariantOperation>,
) -> BuildVariantReplayEvidence {
    let mut native = variant(key, pattern_id);
    native.placed_count = operations.len() as u16;
    BuildVariantReplayEvidence::new(native, replay_layout(), 0, operations)
        .expect("owned replay evidence")
}

fn run_result(variants: &[BuildVariantReplayEvidence]) -> SpinTargetRunResult {
    SpinTargetRunner::run(
        &SpinTarget::tsd("tsd"),
        variants,
        Some(&AlwaysTsdClassifier),
        &ScoreProfile::new("guideline", "Guideline"),
        77,
        4,
        PatternUniverseId::new(10),
        PatternWeightModelId::new(20),
    )
    .expect("spin target result")
}

fn trace_step(
    operation_id: u16,
    operation_index: u16,
    piece: u8,
    rotation: u8,
    adjusted_x: i8,
    adjusted_y: i8,
    cleared_row_mask: u16,
) -> CBuildUpTraceStep {
    CBuildUpTraceStep {
        operation_id,
        operation_index,
        piece,
        rotation,
        hold_branch_kind: C_BUILDUP_HOLD_BRANCH_CURRENT,
        incoming_piece: piece,
        kick_evidence_index: u8::MAX,
        adjusted_x,
        adjusted_y,
        cleared_row_mask,
        reachability: CReachabilityEvidenceView {
            reachable: 1,
            exhaustive: 1,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[path = "spin_target_runner_behavior/coverage_probability.rs"]
mod coverage_probability;
#[path = "spin_target_runner_behavior/kick_evidence.rs"]
mod kick_evidence;
#[path = "spin_target_runner_behavior/recognition.rs"]
mod recognition;
#[path = "spin_target_runner_behavior/replay_execution.rs"]
mod replay_execution;
#[path = "spin_target_runner_behavior/unknown_policy.rs"]
mod unknown_policy;
