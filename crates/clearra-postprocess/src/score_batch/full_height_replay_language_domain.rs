//! Four-word physical projection and trk2 labels for the shared exact language.
//! The constructor/header is not PC completeness authority; consumers must
//! validate the purpose-specific request-owned producer proof separately.
use std::mem::size_of;

use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind, solution::ExtendedTilingSolutionKey,
};
use clearra_replay::{
    FullHeightExecutionBatch, FullHeightReplayBuildError, FullHeightReplayError,
    FullHeightReplayProjector, FullHeightReplayTrace, HoldDecision, PieceDecision,
    ScoringExecutionEdge, SpinCoverageExecutionGraph,
};

use super::{
    exact_replay_language::{admit, Label},
    replay_language_domain::{ReplayLanguageBatch, ReplayLanguageDomain, ReplayLanguageOutput},
    ExactReplayMaterializationError as Error, ExactReplayMaterializationLimits as Limits,
};

/// One selected, physically and supply-validated member, not a complete family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullHeightCandidateExecution {
    pattern_id: usize,
    trace_identity: String,
    replay_trace: FullHeightReplayTrace,
}

impl FullHeightCandidateExecution {
    pub const fn pattern_id(&self) -> usize {
        self.pattern_id
    }
    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }
    pub const fn replay_trace(&self) -> &FullHeightReplayTrace {
        &self.replay_trace
    }
    pub fn into_parts(self) -> (usize, String, FullHeightReplayTrace) {
        (self.pattern_id, self.trace_identity, self.replay_trace)
    }
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        (self.trace_identity.capacity() as u128)
            .checked_add(self.replay_trace.checked_nested_retained_bytes()?)
    }
}
impl ReplayLanguageOutput for FullHeightCandidateExecution {
    fn identity(&self) -> &str {
        &self.trace_identity
    }
    fn checked_owned_bytes(&self) -> Option<u128> {
        (size_of::<Self>() as u128).checked_add(self.checked_nested_retained_bytes()?)
    }
}

impl ReplayLanguageBatch for FullHeightExecutionBatch {
    type Graph = SpinCoverageExecutionGraph;
    fn graph(&self, index: usize) -> Option<&Self::Graph> {
        self.execution().graphs().get(index)
    }
    fn patterns(&self) -> &[Vec<PieceKind>] {
        self.execution().patterns()
    }
    fn initial_cursor(&self) -> u16 {
        self.execution().initial_cursor()
    }
    fn initial_hold(&self) -> Option<PieceKind> {
        self.execution().initial_hold()
    }
    fn complete(&self) -> bool {
        self.execution().complete()
    }
    fn kick_table_id(&self) -> u64 {
        self.execution().kick_table_id()
    }
    fn rule_profile_id(&self) -> u64 {
        self.execution().rule_profile_id()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct PhysicalState {
    board: Board256Mask,
    operations: u64,
    required_operations: u64,
}

#[derive(Debug)]
pub(super) struct FullHeightReplayDomain;

impl ReplayLanguageDomain for FullHeightReplayDomain {
    type Batch = FullHeightExecutionBatch;
    type State = PhysicalState;
    type Mask = Board256Mask;
    type Output = FullHeightCandidateExecution;

    fn same_frame(batch: &Self::Batch, other: &Self::Batch) -> bool {
        batch.height() == other.height() && batch.initial() == other.initial()
    }
    fn initial(
        batch: &Self::Batch,
        graph: &SpinCoverageExecutionGraph,
    ) -> Result<PhysicalState, Error> {
        FullHeightReplayProjector::validate_board(batch.height(), batch.initial())
            .map_err(|_| Error::InvalidEvidence)?;
        let key = ExtendedTilingSolutionKey::parse_canonical(graph.candidate_key())
            .map_err(|_| Error::InvalidEvidence)?;
        let count = key.placement_count();
        if count == 0
            || count > 60
            || key.height() != batch.height()
            || key.initial_board() != batch.initial()
        {
            return Err(Error::InvalidEvidence);
        }
        Ok(PhysicalState {
            board: batch.initial(),
            operations: 0,
            required_operations: (1_u64 << count) - 1,
        })
    }
    fn transition(
        batch: &Self::Batch,
        state: PhysicalState,
        edge: ScoringExecutionEdge,
    ) -> Result<(Board256Mask, PhysicalState), Error> {
        let bit = 1_u64
            .checked_shl(u32::from(edge.operation_index()))
            .ok_or(Error::InvalidEvidence)?;
        if bit & state.required_operations == 0 || bit & state.operations != 0 {
            return Err(Error::InvalidEvidence);
        }
        let transition =
            FullHeightReplayProjector::project_scoring_step(batch.height(), state.board, edge)
                .map_err(|_| Error::InvalidEvidence)?;
        Ok((
            transition.placement(),
            PhysicalState {
                board: transition.after_line_clear(),
                operations: state.operations | bit,
                ..state
            },
        ))
    }
    fn terminal(state: PhysicalState, depth: usize) -> Result<(), Error> {
        if depth == 0 || !state.board.is_empty() || state.operations != state.required_operations {
            Err(Error::InvalidEvidence)
        } else {
            Ok(())
        }
    }
    fn label(
        edge: ScoringExecutionEdge,
        decision: PieceDecision,
        mask: Board256Mask,
    ) -> Result<Label, Error> {
        Label::from_writer(|writer| {
            FullHeightReplayTrace::write_step_key_with_decision(writer, edge, decision, mask)
        })
    }
    fn step_suffix<'a>(batch: &Self::Batch, identity: &'a str) -> Result<&'a str, Error> {
        let prefix = Label::from_writer(|writer| {
            FullHeightReplayTrace::write_canonical_prefix(writer, batch.height(), batch.initial())
        })?;
        identity
            .strip_prefix(prefix.as_str()?)
            .and_then(|text| text.strip_prefix('~'))
            .ok_or(Error::InvalidEvidence)
    }

    fn project_selected(
        batch: &Self::Batch,
        _: &SpinCoverageExecutionGraph,
        pattern_id: usize,
        path: &[ScoringExecutionEdge],
        holds: &[HoldDecision],
        control: &ExecutionControl,
        baseline: u128,
        limits: Limits,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<FullHeightCandidateExecution, Error> {
        if control.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if path.len() != holds.len() || path.len() > 60 || path.is_empty() {
            return Err(Error::InvalidEvidence);
        }
        let baseline = baseline
            .checked_add(size_of::<FullHeightCandidateExecution>() as u128)
            .ok_or(Error::ProjectionOverflow)?;
        let requested_pairs = (path.len() as u128)
            .checked_mul(size_of::<(ScoringExecutionEdge, HoldDecision)>() as u128)
            .ok_or(Error::ProjectionOverflow)?;
        admit(
            limits,
            baseline
                .checked_add(requested_pairs)
                .ok_or(Error::ProjectionOverflow)?,
            guard,
        )?;
        let mut pairs = Vec::new();
        pairs
            .try_reserve_exact(path.len())
            .map_err(|_| Error::AllocationFailed)?;
        let pairs_bytes = (pairs.capacity() as u128)
            .checked_mul(size_of::<(ScoringExecutionEdge, HoldDecision)>() as u128)
            .ok_or(Error::ProjectionOverflow)?;
        let trace_baseline = baseline
            .checked_add(pairs_bytes)
            .ok_or(Error::ProjectionOverflow)?;
        admit(limits, trace_baseline, guard)?;
        pairs.extend(path.iter().copied().zip(holds.iter().copied()));
        let trace = FullHeightReplayTrace::from_selected_path(
            batch.height(),
            batch.initial(),
            usize::from(batch.initial_cursor()),
            batch.initial_hold(),
            &pairs,
            control,
            |bytes| {
                admit(
                    limits,
                    trace_baseline
                        .checked_add(bytes)
                        .ok_or(Error::ProjectionOverflow)?,
                    guard,
                )
            },
        )
        .map_err(|error| match error {
            FullHeightReplayBuildError::MemoryGuard(error) => error,
            FullHeightReplayBuildError::Replay(FullHeightReplayError::Cancelled) => {
                Error::Cancelled
            }
            FullHeightReplayBuildError::Replay(FullHeightReplayError::AllocationFailed) => {
                Error::AllocationFailed
            }
            FullHeightReplayBuildError::Replay(FullHeightReplayError::ProjectionOverflow) => {
                Error::ProjectionOverflow
            }
            FullHeightReplayBuildError::Replay(_) => Error::InvalidEvidence,
        })?;
        let trace_bytes = trace
            .checked_nested_retained_bytes()
            .ok_or(Error::ProjectionOverflow)?;
        let key_bytes = trace
            .checked_canonical_key_requested_bytes()
            .ok_or(Error::ProjectionOverflow)?;
        let output_baseline = trace_baseline
            .checked_add(trace_bytes)
            .ok_or(Error::ProjectionOverflow)?;
        admit(
            limits,
            output_baseline
                .checked_add(key_bytes)
                .ok_or(Error::ProjectionOverflow)?,
            guard,
        )?;
        let mut identity = String::new();
        identity
            .try_reserve_exact(usize::try_from(key_bytes).map_err(|_| Error::ProjectionOverflow)?)
            .map_err(|_| Error::AllocationFailed)?;
        admit(
            limits,
            output_baseline
                .checked_add(identity.capacity() as u128)
                .ok_or(Error::ProjectionOverflow)?,
            guard,
        )?;
        trace
            .write_canonical_key(&mut identity)
            .map_err(|_| Error::ProjectionOverflow)?;
        Ok(FullHeightCandidateExecution {
            pattern_id,
            trace_identity: identity,
            replay_trace: trace,
        })
    }
}
