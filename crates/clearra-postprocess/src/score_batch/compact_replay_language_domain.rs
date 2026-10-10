//! The unchanged Board64/trk1 adapter to shared count and lexical selection.
use std::mem::size_of;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_replay::{
    trace::solution_trace_builder::SolutionTraceBuilder, ExactScoringExecutionBatch,
    ExactScoringExecutionGraph, HoldDecision, PieceDecision, ScoringExecutionEdge,
};

use super::{
    exact_replay_language::{admit, replay_projection, Label},
    exact_scoring_execution_materializer::replay_path,
    replay_language_domain::{ReplayLanguageDomain, ReplayLanguageOutput},
    CandidateExecution, ExactReplayMaterializationError as Error,
    ExactReplayMaterializationLimits as Limits,
};

#[derive(Debug)]
pub(super) struct CompactReplayDomain;

impl ReplayLanguageOutput for CandidateExecution {
    fn identity(&self) -> &str {
        self.trace_identity()
    }
    fn checked_owned_bytes(&self) -> Option<u128> {
        (size_of::<Self>() as u128).checked_add(self.checked_nested_retained_bytes()?)
    }
}

impl ReplayLanguageDomain for CompactReplayDomain {
    type Batch = ExactScoringExecutionBatch;
    type State = u64;
    type Mask = u64;
    type Output = CandidateExecution;

    fn same_frame(batch: &Self::Batch, other: &Self::Batch) -> bool {
        batch.layout() == other.layout() && batch.initial_occupied() == other.initial_occupied()
    }
    fn initial(batch: &Self::Batch, _: &ExactScoringExecutionGraph) -> Result<u64, Error> {
        if batch.initial_occupied() & !batch.layout().all_cells_mask() != 0 {
            return Err(Error::InvalidEvidence);
        }
        Ok(batch.initial_occupied())
    }
    fn transition(
        batch: &Self::Batch,
        state: u64,
        edge: ScoringExecutionEdge,
    ) -> Result<(u64, u64), Error> {
        SolutionTraceBuilder::project_scoring_step(batch.layout(), state, edge)
            .ok_or(Error::InvalidEvidence)
    }
    fn terminal(state: u64, depth: usize) -> Result<(), Error> {
        if depth == 0 || state != 0 {
            Err(Error::InvalidEvidence)
        } else {
            Ok(())
        }
    }
    fn label(
        edge: ScoringExecutionEdge,
        decision: PieceDecision,
        mask: u64,
    ) -> Result<Label, Error> {
        Label::new(edge, decision, mask)
    }
    fn step_suffix<'a>(_: &Self::Batch, identity: &'a str) -> Result<&'a str, Error> {
        identity.strip_prefix("trk1:").ok_or(Error::InvalidEvidence)
    }

    fn project_selected(
        batch: &Self::Batch,
        graph: &ExactScoringExecutionGraph,
        pattern_id: usize,
        path: &[ScoringExecutionEdge],
        holds: &[HoldDecision],
        control: &ExecutionControl,
        baseline: u128,
        limits: Limits,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<CandidateExecution, Error> {
        if control.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let projection = replay_projection(path.len(), usize::from(batch.layout().cell_count()))?;
        admit(
            limits,
            baseline
                .checked_add(projection)
                .ok_or(Error::ProjectionOverflow)?,
            guard,
        )?;
        let trace =
            replay_path(batch, graph, pattern_id, path, holds).ok_or(Error::InvalidEvidence)?;
        let trace_bytes = (size_of::<clearra_replay::ReplayTrace>() as u128)
            .checked_add(
                trace
                    .checked_nested_retained_bytes()
                    .ok_or(Error::ProjectionOverflow)?,
            )
            .ok_or(Error::ProjectionOverflow)?;
        let key_bytes = trace
            .checked_canonical_key_requested_bytes()
            .ok_or(Error::ProjectionOverflow)?;
        admit(
            limits,
            baseline
                .checked_add(trace_bytes)
                .and_then(|n| n.checked_add(key_bytes))
                .ok_or(Error::ProjectionOverflow)?,
            guard,
        )?;
        Ok(CandidateExecution::new(
            pattern_id,
            trace.canonical_key(),
            trace,
        ))
    }
}
