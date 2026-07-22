use clearra_core_domain::{
    operation::operation::OperationId,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_core_ffi::problem::{
    C_PIECE_I, C_PIECE_J, C_PIECE_L, C_PIECE_O, C_PIECE_S, C_PIECE_T, C_PIECE_Z,
};
use clearra_core_ffi::{
    CBuildVariantView, CBuildVariantViewError, CNativeBuildVariantView, CPackingCandidate,
    C_BUILDUP_HOLD_BRANCH_CURRENT, C_BUILDUP_HOLD_BRANCH_STORE_CURRENT,
    C_BUILDUP_HOLD_BRANCH_SWAP_HELD,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{BuildVariantOperation, HoldDecision};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildVariantReplayEvidenceError {
    MissingOperationBasis,
    OperationCountTooLarge {
        operation_count: usize,
        max: usize,
    },
    UnsupportedPiece {
        piece: u8,
    },
    UnsupportedRotation {
        rotation: u8,
    },
    NegativeCoordinate {
        x: i8,
        y: i8,
    },
    NativeVariant(CBuildVariantViewError),
    CandidateIdentityMismatch {
        variant: u64,
        candidate: u64,
    },
    TraceOperationIndexOutOfRange {
        index: usize,
        operation_count: usize,
    },
    TraceOperationMismatch {
        step_index: usize,
    },
    UnsupportedHoldBranch {
        branch_kind: u8,
    },
    MissingHeldPiece {
        step_index: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildVariantReplayEvidence {
    variant: CBuildVariantView,
    layout: Board64Layout,
    initial_board: u64,
    operations: Vec<BuildVariantOperation>,
    representative_order: Vec<usize>,
    hold_decisions: Vec<HoldDecision>,
}

impl BuildVariantReplayEvidence {
    pub fn new(
        native_variant: CNativeBuildVariantView,
        layout: Board64Layout,
        initial_board: u64,
        operations: Vec<BuildVariantOperation>,
    ) -> Result<Self, BuildVariantReplayEvidenceError> {
        let variant = CBuildVariantView::from_native(&native_variant)
            .map_err(BuildVariantReplayEvidenceError::NativeVariant)?;
        Ok(Self::from_owned(variant, layout, initial_board, operations))
    }
}
impl BuildVariantReplayEvidence {
    fn from_owned(
        variant: CBuildVariantView,
        layout: Board64Layout,
        initial_board: u64,
        operations: Vec<BuildVariantOperation>,
    ) -> Self {
        let representative_order = (0..operations.len()).collect();
        let hold_decisions = vec![HoldDecision::None; operations.len()];
        Self {
            variant,
            layout,
            initial_board,
            operations,
            representative_order,
            hold_decisions,
        }
    }
}
impl BuildVariantReplayEvidence {
    pub fn from_native_build_variant_and_candidate(
        native_variant: CNativeBuildVariantView,
        layout: Board64Layout,
        initial_board: u64,
        candidate: &CPackingCandidate,
    ) -> Result<Self, BuildVariantReplayEvidenceError> {
        let variant = CBuildVariantView::from_native(&native_variant)
            .map_err(BuildVariantReplayEvidenceError::NativeVariant)?;
        Self::from_build_variant_and_candidate(variant, layout, initial_board, candidate)
    }
}
impl BuildVariantReplayEvidence {
    pub fn from_build_variant_and_candidate(
        variant: CBuildVariantView,
        layout: Board64Layout,
        initial_board: u64,
        candidate: &CPackingCandidate,
    ) -> Result<Self, BuildVariantReplayEvidenceError> {
        if variant.candidate_id() != 0
            && candidate.candidate_id != 0
            && variant.candidate_id() != candidate.candidate_id
        {
            return Err(BuildVariantReplayEvidenceError::CandidateIdentityMismatch {
                variant: variant.candidate_id(),
                candidate: candidate.candidate_id,
            });
        }
        if variant.trace_steps().is_empty() {
            return Ok(Self::from_owned(
                variant,
                layout,
                initial_board,
                replay_operations_from_candidate(candidate)?,
            ));
        }
        let operations = replay_operations_from_trace_steps(&variant, candidate)?;
        let hold_decisions = hold_decisions_from_trace_steps(&variant)?;
        let mut evidence = Self::from_owned(variant, layout, initial_board, operations);
        evidence.hold_decisions = hold_decisions;
        Ok(evidence)
    }
}
impl BuildVariantReplayEvidence {
    pub fn with_representative_order(mut self, representative_order: Vec<usize>) -> Self {
        self.representative_order = representative_order;
        self
    }
}
impl BuildVariantReplayEvidence {
    pub fn variant(&self) -> &CBuildVariantView {
        &self.variant
    }
}
impl BuildVariantReplayEvidence {
    pub fn layout(&self) -> Board64Layout {
        self.layout
    }
}
impl BuildVariantReplayEvidence {
    pub fn initial_board(&self) -> u64 {
        self.initial_board
    }
}
impl BuildVariantReplayEvidence {
    pub fn operations(&self) -> &[BuildVariantOperation] {
        &self.operations
    }
}
impl BuildVariantReplayEvidence {
    pub fn representative_order(&self) -> &[usize] {
        &self.representative_order
    }
}
impl BuildVariantReplayEvidence {
    pub fn hold_decisions(&self) -> &[HoldDecision] {
        &self.hold_decisions
    }
}

fn replay_operations_from_candidate(
    candidate: &CPackingCandidate,
) -> Result<Vec<BuildVariantOperation>, BuildVariantReplayEvidenceError> {
    let operation_count = usize::from(candidate.operation_count);
    if operation_count == 0 {
        return Err(BuildVariantReplayEvidenceError::MissingOperationBasis);
    }
    if operation_count > candidate.operations.len() {
        return Err(BuildVariantReplayEvidenceError::OperationCountTooLarge {
            operation_count,
            max: candidate.operations.len(),
        });
    }

    let mut operations = Vec::with_capacity(operation_count);
    for operation in candidate.operations.iter().take(operation_count) {
        let piece = replay_piece_from_c(operation.piece)?;
        let rotation = RotationState::from_quarter_turns(operation.rotation).map_err(|_| {
            BuildVariantReplayEvidenceError::UnsupportedRotation {
                rotation: operation.rotation,
            }
        })?;
        if operation.x < 0 || operation.y < 0 {
            return Err(BuildVariantReplayEvidenceError::NegativeCoordinate {
                x: operation.x,
                y: operation.y,
            });
        }

        let mut replay_operation =
            BuildVariantOperation::new(piece, rotation, operation.x as u16, operation.y as u16)
                .with_operation_id(OperationId(operation.operation_id));
        if operation.mask != 0 {
            replay_operation = replay_operation.with_mask(operation.mask);
        }
        operations.push(replay_operation);
    }

    Ok(operations)
}

fn replay_piece_from_c(piece: u8) -> Result<PieceKind, BuildVariantReplayEvidenceError> {
    match piece {
        C_PIECE_I => Ok(PieceKind::I),
        C_PIECE_O => Ok(PieceKind::O),
        C_PIECE_T => Ok(PieceKind::T),
        C_PIECE_S => Ok(PieceKind::S),
        C_PIECE_Z => Ok(PieceKind::Z),
        C_PIECE_J => Ok(PieceKind::J),
        C_PIECE_L => Ok(PieceKind::L),
        _ => Err(BuildVariantReplayEvidenceError::UnsupportedPiece { piece }),
    }
}

fn replay_operations_from_trace_steps(
    variant: &CBuildVariantView,
    candidate: &CPackingCandidate,
) -> Result<Vec<BuildVariantOperation>, BuildVariantReplayEvidenceError> {
    let mut operations = Vec::with_capacity(variant.trace_steps().len());
    for (step_index, step) in variant.trace_steps().iter().enumerate() {
        let operation_index = usize::from(step.operation_index);
        let operation = candidate.operations.get(operation_index).ok_or(
            BuildVariantReplayEvidenceError::TraceOperationIndexOutOfRange {
                index: operation_index,
                operation_count: usize::from(candidate.operation_count),
            },
        )?;
        let domain_bit = 1_u16 << operation_index;
        let has_geometry_variant_domain = candidate.geometry_variant_domains & domain_bit != 0;
        let representative_matches_geometry =
            operation.piece == step.piece && operation.mask == step.target_frame_mask;
        let exact_representative_matches =
            operation.operation_id == step.operation_id && operation.rotation == step.rotation;
        if operation_index >= usize::from(candidate.operation_count)
            || !representative_matches_geometry
            || (!has_geometry_variant_domain && !exact_representative_matches)
        {
            return Err(BuildVariantReplayEvidenceError::TraceOperationMismatch { step_index });
        }
        if step.adjusted_x < 0 || step.adjusted_y < 0 {
            return Err(BuildVariantReplayEvidenceError::NegativeCoordinate {
                x: step.adjusted_x,
                y: step.adjusted_y,
            });
        }
        let piece = replay_piece_from_c(step.piece)?;
        let rotation = RotationState::from_quarter_turns(step.rotation).map_err(|_| {
            BuildVariantReplayEvidenceError::UnsupportedRotation {
                rotation: step.rotation,
            }
        })?;
        operations.push(
            BuildVariantOperation::new(
                piece,
                rotation,
                step.adjusted_x as u16,
                step.adjusted_y as u16,
            )
            .with_operation_id(OperationId(step.operation_id))
            .with_cleared_row_mask(step.cleared_row_mask),
        );
    }
    Ok(operations)
}

fn hold_decisions_from_trace_steps(
    variant: &CBuildVariantView,
) -> Result<Vec<HoldDecision>, BuildVariantReplayEvidenceError> {
    variant
        .trace_steps()
        .iter()
        .enumerate()
        .map(|(step_index, step)| match step.hold_branch_kind {
            C_BUILDUP_HOLD_BRANCH_CURRENT => Ok(HoldDecision::None),
            C_BUILDUP_HOLD_BRANCH_SWAP_HELD => {
                let incoming_piece = replay_piece_from_c(step.incoming_piece)?;
                let held_piece = replay_piece_from_c(step.held_piece_before).map_err(|_| {
                    BuildVariantReplayEvidenceError::MissingHeldPiece { step_index }
                })?;
                Ok(HoldDecision::SwapWithHold {
                    incoming_piece,
                    held_piece,
                })
            }
            C_BUILDUP_HOLD_BRANCH_STORE_CURRENT => Ok(HoldDecision::StoreIncoming {
                stored_piece: replay_piece_from_c(step.incoming_piece)?,
                drawn_piece: replay_piece_from_c(step.piece)?,
            }),
            branch_kind => {
                Err(BuildVariantReplayEvidenceError::UnsupportedHoldBranch { branch_kind })
            }
        })
        .collect()
}
