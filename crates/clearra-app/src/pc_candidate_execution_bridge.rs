// SRP rationale: this module has one change reason: admitting one immutable,
// request-bound complete PC4 candidate universe into Core's ordinary exact
// reducer. Transport, tablebase parsing, completeness minting, product
// projection, and fallback remain owned by their existing layers.

use clearra_core_domain::{
    board::standard_pc_board::StandardPcBoard, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_core_executor::{CoreExecutionResult, WasmCpuSearchBackend, WasmCpuSearchError};
use clearra_pc4_tablebase::{FixedQueueHoldState, Pc4GraphPiece, Pc4TerminalUseCase};
use clearra_problem::{SearchProblem, SearchProblemId, SearchProblemKind};
use clearra_supply::hold::hold_slot::HoldSlot;

use crate::{
    pc4_search_problem_compatibility::{
        validate_pc4_search_problem_compatibility, Pc4SearchProblemCompatibility,
        Pc4SearchProblemCompatibilityError,
    },
    pc_candidate_page_boundary::{
        PcCandidateBoundaryError, PcCandidateProviderKind, PcCandidateReducerInput,
        PcCandidateRequestIdentity, PcCandidateRequestIdentityError, PcCandidateSetDigest,
        PcCandidateUniverseIdentity,
    },
};

/// Read-only proof that one request-bound complete candidate universe was
/// reduced by Core for the exact compiled problem recorded here.
///
/// This type does not own or decorate the Core result, mint source
/// completeness, or grant transport, snapshot-freshness, product, or fallback
/// authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPcCandidateExecutionEvidence {
    universe_identity: PcCandidateUniverseIdentity,
    compatibility: Pc4SearchProblemCompatibility,
    problem_id: SearchProblemId,
}

impl ValidatedPcCandidateExecutionEvidence {
    pub const fn universe_identity(&self) -> &PcCandidateUniverseIdentity {
        &self.universe_identity
    }

    pub const fn compatibility(&self) -> Pc4SearchProblemCompatibility {
        self.compatibility
    }

    pub const fn problem_id(&self) -> &SearchProblemId {
        &self.problem_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcCandidateExecutionError {
    Cancelled,
    PartitionedExecutionControl,
    QualifiedOnlineSourceRequired,
    QualifiedTargetRequired,
    SourceTargetSnapshotMismatch,
    ProfileMismatch,
    TargetProfileMismatch,
    TargetUseCaseMismatch,
    UnsupportedProblemKind,
    SearchProblemCompatibility(Pc4SearchProblemCompatibilityError),
    CompatibilityTokenMismatch,
    ProblemBoardDomainMismatch,
    TargetLinesMismatch,
    InitialBoardMismatch,
    CandidateInitialBoardMismatch,
    UnsupportedRequestSource,
    RequestIdentity(PcCandidateRequestIdentityError),
    RequestIdentityMismatch,
    CandidatesNotStrictlyCanonical,
    CandidateCountOverflow,
    CandidateCountMismatch,
    CandidateDigest(PcCandidateBoundaryError),
    CandidateDigestMismatch,
    Core(WasmCpuSearchError),
}

impl PcCandidateExecutionError {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc_candidate_execution_cancelled",
            Self::PartitionedExecutionControl => {
                "pc_candidate_execution_partitioned_control_not_allowed"
            }
            Self::QualifiedOnlineSourceRequired => {
                "pc_candidate_execution_qualified_online_source_required"
            }
            Self::QualifiedTargetRequired => "pc_candidate_execution_qualified_target_required",
            Self::SourceTargetSnapshotMismatch => {
                "pc_candidate_execution_source_target_snapshot_mismatch"
            }
            Self::ProfileMismatch => "pc_candidate_execution_profile_mismatch",
            Self::TargetProfileMismatch => "pc_candidate_execution_target_profile_mismatch",
            Self::TargetUseCaseMismatch => "pc_candidate_execution_target_use_case_mismatch",
            Self::UnsupportedProblemKind => "pc_candidate_execution_problem_kind_unsupported",
            Self::SearchProblemCompatibility(error) => error.reason(),
            Self::CompatibilityTokenMismatch => {
                "pc_candidate_execution_compatibility_token_mismatch"
            }
            Self::ProblemBoardDomainMismatch => {
                "pc_candidate_execution_problem_board_domain_mismatch"
            }
            Self::TargetLinesMismatch => "pc_candidate_execution_target_lines_mismatch",
            Self::InitialBoardMismatch => "pc_candidate_execution_initial_board_mismatch",
            Self::CandidateInitialBoardMismatch => {
                "pc_candidate_execution_candidate_initial_board_mismatch"
            }
            Self::UnsupportedRequestSource => "pc_candidate_execution_request_source_unsupported",
            Self::RequestIdentity(error) => error.reason(),
            Self::RequestIdentityMismatch => "pc_candidate_execution_request_identity_mismatch",
            Self::CandidatesNotStrictlyCanonical => {
                "pc_candidate_execution_candidates_not_strictly_canonical"
            }
            Self::CandidateCountOverflow => "pc_candidate_execution_candidate_count_overflow",
            Self::CandidateCountMismatch => "pc_candidate_execution_candidate_count_mismatch",
            Self::CandidateDigest(error) => error.reason(),
            Self::CandidateDigestMismatch => "pc_candidate_execution_candidate_digest_mismatch",
            Self::Core(error) => error.reason(),
        }
    }
}

impl core::fmt::Display for PcCandidateExecutionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for PcCandidateExecutionError {}

/// Validates and executes one complete, fixed-queue PC-search candidate
/// universe. All App-owned identities are checked before Core acquires an
/// execution lease or mutates reducer state. The bridge never selects or
/// prunes a graph candidate; Core remains the sole owner of the requested
/// BuildUp, coverage, replay, scoring, and objective reductions.
pub fn execute_validated_pc_candidate_input(
    input: &PcCandidateReducerInput,
    compatibility: Pc4SearchProblemCompatibility,
    problem: &SearchProblem,
    control: &ExecutionControl,
) -> Result<(CoreExecutionResult, ValidatedPcCandidateExecutionEvidence), PcCandidateExecutionError>
{
    if control.is_cancelled() {
        return Err(PcCandidateExecutionError::Cancelled);
    }
    if control.partition().count() != 1 || control.partition().index() != 0 {
        return Err(PcCandidateExecutionError::PartitionedExecutionControl);
    }

    let universe = input.universe_identity();
    if universe.source().provider_kind() != PcCandidateProviderKind::OnlinePc4 {
        return Err(PcCandidateExecutionError::QualifiedOnlineSourceRequired);
    }
    let target = universe
        .qualified_target()
        .ok_or(PcCandidateExecutionError::QualifiedTargetRequired)?;
    let source_snapshot = universe
        .qualified_snapshot()
        .ok_or(PcCandidateExecutionError::QualifiedOnlineSourceRequired)?;
    if source_snapshot != target.snapshot() {
        return Err(PcCandidateExecutionError::SourceTargetSnapshotMismatch);
    }
    if compatibility.profile() != universe.profile() {
        return Err(PcCandidateExecutionError::ProfileMismatch);
    }
    if target.profile() != universe.profile() {
        return Err(PcCandidateExecutionError::TargetProfileMismatch);
    }
    if target.use_case() != Pc4TerminalUseCase::PcSearch {
        return Err(PcCandidateExecutionError::TargetUseCaseMismatch);
    }
    if !matches!(
        problem.problem_kind(),
        SearchProblemKind::OpeningPc | SearchProblemKind::ScenarioPc
    ) {
        return Err(PcCandidateExecutionError::UnsupportedProblemKind);
    }

    let checked_compatibility =
        validate_pc4_search_problem_compatibility(universe.profile(), problem)
            .map_err(PcCandidateExecutionError::SearchProblemCompatibility)?;
    if checked_compatibility != compatibility {
        return Err(PcCandidateExecutionError::CompatibilityTokenMismatch);
    }

    let target_lines = target.target_lines().get();
    let initial = problem.initial_board();
    if initial.width() != 10 {
        return Err(PcCandidateExecutionError::ProblemBoardDomainMismatch);
    }
    if initial.visible_height() != u16::from(target_lines)
        || problem.visible_height() != u16::from(target_lines)
    {
        return Err(PcCandidateExecutionError::TargetLinesMismatch);
    }
    if universe.initial_board_mask() != initial.occupied_mask() {
        return Err(PcCandidateExecutionError::InitialBoardMismatch);
    }
    if input
        .candidates()
        .iter()
        .any(|candidate| candidate.initial_board_mask() != initial.occupied_mask())
    {
        return Err(PcCandidateExecutionError::CandidateInitialBoardMismatch);
    }
    if input.candidates().windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(PcCandidateExecutionError::CandidatesNotStrictlyCanonical);
    }

    let actual_count = u64::try_from(input.candidates().len())
        .map_err(|_| PcCandidateExecutionError::CandidateCountOverflow)?;
    if actual_count != universe.exact_candidate_count() {
        return Err(PcCandidateExecutionError::CandidateCountMismatch);
    }
    let actual_digest = PcCandidateSetDigest::calculate(input.candidates())
        .map_err(PcCandidateExecutionError::CandidateDigest)?;
    if actual_digest != universe.candidate_set_digest() {
        return Err(PcCandidateExecutionError::CandidateDigestMismatch);
    }

    let queue = problem
        .core_query()
        .remaining_queue()
        .as_fixed_sequence()
        .ok_or(PcCandidateExecutionError::UnsupportedRequestSource)?;
    let queue = queue
        .pieces()
        .iter()
        .copied()
        .map(core_piece_to_graph)
        .collect::<Vec<_>>();
    let initial_board =
        StandardPcBoard::from_words(target_lines, [initial.occupied_mask(), 0, 0, 0])
            .map_err(|_| PcCandidateExecutionError::ProblemBoardDomainMismatch)?;
    let initial_hold = fixed_queue_hold_state(
        problem.core_query().allow_hold(),
        problem.core_query().hold_state(),
    );
    let request_identity = PcCandidateRequestIdentity::derive_pc4_fixed_queue_candidate_universe(
        target,
        initial_board,
        initial_hold,
        &queue,
    )
    .map_err(PcCandidateExecutionError::RequestIdentity)?;
    if request_identity != universe.request_identity() {
        return Err(PcCandidateExecutionError::RequestIdentityMismatch);
    }

    let core_result =
        WasmCpuSearchBackend::execute_complete_precomputed_geometry_candidates_with_control(
            problem,
            input.candidates(),
            control,
        )
        .map_err(PcCandidateExecutionError::Core)?;
    Ok((
        core_result,
        ValidatedPcCandidateExecutionEvidence {
            universe_identity: universe.clone(),
            compatibility,
            problem_id: problem.problem_id().clone(),
        },
    ))
}

fn fixed_queue_hold_state(allow_hold: bool, hold: HoldSlot) -> FixedQueueHoldState {
    if !allow_hold {
        return FixedQueueHoldState::Disabled;
    }
    match hold {
        HoldSlot::Empty => FixedQueueHoldState::Empty,
        HoldSlot::Occupied(piece) => FixedQueueHoldState::Occupied(core_piece_to_graph(piece)),
    }
}

const fn core_piece_to_graph(piece: PieceKind) -> Pc4GraphPiece {
    match piece {
        PieceKind::I => Pc4GraphPiece::I,
        PieceKind::O => Pc4GraphPiece::O,
        PieceKind::T => Pc4GraphPiece::T,
        PieceKind::S => Pc4GraphPiece::S,
        PieceKind::Z => Pc4GraphPiece::Z,
        PieceKind::J => Pc4GraphPiece::J,
        PieceKind::L => Pc4GraphPiece::L,
    }
}
