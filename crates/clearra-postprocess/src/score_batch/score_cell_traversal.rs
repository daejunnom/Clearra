//! Shared exact graph/supply/score traversal. Board projection is a typed
//! adapter: compact scoring retains its old zero-cost projection, while the
//! full-height adapter validates physical locks instead of truncating boards.
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_replay::{
    ExactScoringExecutionGraph, HoldDecision, ScoringExecutionEdge, ScoringExecutionNode,
    SpinCoverageExecutionGraph,
};
use clearra_scoring::{
    event::SpinDetector, model::ScoreModelEvaluator, profile::ScoreProfile, state::ScoreState,
};

use super::{
    exact_scoring_execution_materializer::ExactScoringExecutionCancelled,
    execution_supply::{
        first_standard_bag_lookahead, for_each_supply_successor, terminal_supply_state_is_accepted,
        ExecutionSupplyBatch, SupplyState,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ScoreCellTraversalError {
    Cancelled,
    InvalidEvidence,
    ScratchCapacity,
}

pub(super) trait ScoreCellGraph {
    fn node(&self, index: u32) -> Option<ScoringExecutionNode>;
    fn edges(&self, node: ScoringExecutionNode) -> Option<&[ScoringExecutionEdge]>;
}

impl ScoreCellGraph for ExactScoringExecutionGraph {
    fn node(&self, index: u32) -> Option<ScoringExecutionNode> {
        self.node(index)
    }
    fn edges(&self, node: ScoringExecutionNode) -> Option<&[ScoringExecutionEdge]> {
        self.checked_edges(node)
    }
}

impl ScoreCellGraph for SpinCoverageExecutionGraph {
    fn node(&self, index: u32) -> Option<ScoringExecutionNode> {
        self.node(index)
    }
    fn edges(&self, node: ScoringExecutionNode) -> Option<&[ScoringExecutionEdge]> {
        self.checked_edges(node)
    }
}

pub(super) trait ScoreCellPhysicalProjection {
    type State: Copy;
    fn advance(
        &self,
        state: Self::State,
        edge: ScoringExecutionEdge,
    ) -> Result<Self::State, ScoreCellTraversalError>;
    fn validate_terminal(&self, state: Self::State) -> Result<(), ScoreCellTraversalError>;
}

pub(super) struct CompactScoreCellProjection;
impl ScoreCellPhysicalProjection for CompactScoreCellProjection {
    type State = ();
    fn advance(&self, _: (), _: ScoringExecutionEdge) -> Result<(), ScoreCellTraversalError> {
        Ok(())
    }
    fn validate_terminal(&self, _: ()) -> Result<(), ScoreCellTraversalError> {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn visit_score_cell_paths<B, G, P, F>(
    batch: &B,
    graph: &G,
    sequence: &[PieceKind],
    state: SupplyState,
    physical_state: P::State,
    projection: &P,
    path: &mut Vec<ScoringExecutionEdge>,
    holds: &mut Vec<HoldDecision>,
    max_path_len: usize,
    profile: &ScoreProfile,
    score_state: ScoreState,
    terminal: &mut F,
    control: &ExecutionControl,
) -> Result<bool, ScoreCellTraversalError>
where
    B: ExecutionSupplyBatch,
    G: ScoreCellGraph,
    P: ScoreCellPhysicalProjection,
    F: FnMut(
        &[ScoringExecutionEdge],
        &[HoldDecision],
        ScoreState,
    ) -> Result<(), ScoreCellTraversalError>,
{
    if control.is_cancelled() {
        return Err(ScoreCellTraversalError::Cancelled);
    }
    let Some(node) = graph.node(state.node) else {
        return Ok(false);
    };
    if node.accepting() {
        if terminal_supply_state_is_accepted(batch, sequence, state) {
            projection.validate_terminal(physical_state)?;
            terminal(path, holds, score_state)?;
        }
        return Ok(true);
    }
    let mut complete = true;
    for &edge in graph
        .edges(node)
        .ok_or(ScoreCellTraversalError::InvalidEvidence)?
    {
        let next_physical = projection.advance(physical_state, edge)?;
        let next_score = ScoreModelEvaluator::evaluate_classified_lock(
            profile,
            score_state,
            path.len(),
            edge.cleared_lines(),
            edge.perfect_clear(),
            SpinDetector::detect_scoring_edge_with_profile(edge, profile.spin_profile()),
        );
        let mut visit = |decision: HoldDecision, next: SupplyState| {
            if path.len() >= max_path_len
                || holds.len() >= max_path_len
                || path.len() == path.capacity()
                || holds.len() == holds.capacity()
            {
                return Err(ScoreCellTraversalError::ScratchCapacity);
            }
            path.push(edge);
            holds.push(decision);
            let result = visit_score_cell_paths(
                batch,
                graph,
                sequence,
                SupplyState {
                    node: edge.to(),
                    ..next
                },
                next_physical,
                projection,
                path,
                holds,
                max_path_len,
                profile,
                next_score,
                terminal,
                control,
            );
            path.pop();
            holds.pop();
            complete &= result?;
            Ok(())
        };
        if batch.projects_unplaced_lookahead()
            && batch.hold_enabled()
            && state.cursor as usize == sequence.len()
            && state.hold == Some(edge.piece())
            && graph.node(edge.to()).is_some_and(|child| child.accepting())
            && (!batch.projects_standard_bag_lookahead()
                || first_standard_bag_lookahead(sequence).is_none())
        {
            visit(
                HoldDecision::ReleaseHeldAtTerminal {
                    held_piece: edge.piece(),
                },
                SupplyState {
                    node: edge.to(),
                    cursor: state.cursor.saturating_add(1),
                    hold: state.hold,
                },
            )?;
        }
        let mut branch_error = None;
        let result =
            for_each_supply_successor(batch, sequence, state, edge.piece(), |decision, next| {
                visit(decision, next).map_err(|error| {
                    branch_error = Some(error);
                    ExactScoringExecutionCancelled
                })
            });
        if let Some(error) = branch_error {
            return Err(error);
        }
        if result.is_err() {
            return Err(ScoreCellTraversalError::Cancelled);
        }
    }
    Ok(complete)
}
