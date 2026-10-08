//! Physical header around the existing board-independent, exact lock DAG.
//! This is execution input, not an attachable PC-family completeness proof.

use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, solution::ExtendedTilingSolutionKey,
};

use crate::{FullHeightReplayProjector, SpinCoverageExecutionBatch};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FullHeightExecutionBatchError {
    InvalidBoard,
    InvalidCandidate,
    SnapshotMismatch,
    InvalidGraph,
}

/// Reuses the actual BuildUp lock graph, including ordered-kick evidence and
/// every physical realization. Does not convert a four-word board to Board64,
/// clone the graph into a second representation, or reinterpret one witness as
/// all executions. The producer still owns query/completeness authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullHeightExecutionBatch {
    height: u8,
    initial: Board256Mask,
    execution: SpinCoverageExecutionBatch,
}

impl FullHeightExecutionBatch {
    pub fn from_spin_coverage(
        height: u8,
        initial: Board256Mask,
        execution: SpinCoverageExecutionBatch,
    ) -> Result<Self, FullHeightExecutionBatchError> {
        use FullHeightExecutionBatchError::*;
        if !(7..=24).contains(&height)
            || FullHeightReplayProjector::validate_board(height, initial).is_err()
        {
            return Err(InvalidBoard);
        }
        let full = Board256Mask::all_cells(u16::from(height) * 10).map_err(|_| InvalidBoard)?;
        for graph in execution.graphs() {
            let key = ExtendedTilingSolutionKey::parse_canonical(graph.candidate_key())
                .map_err(|_| InvalidCandidate)?;
            if key.height() != height || key.initial_board() != initial {
                return Err(SnapshotMismatch);
            }
            let target = key
                .placements()
                .fold(initial, |board, placement| board.union(placement.cells()));
            if target != full {
                return Err(InvalidCandidate);
            }
            if graph.node_count() > u32::MAX as usize || graph.node(graph.root()).is_none() {
                return Err(InvalidGraph);
            }
            for index in 0..graph.node_count() as u32 {
                let node = graph.node(index).ok_or(InvalidGraph)?;
                let edges = graph.checked_edges(node).ok_or(InvalidGraph)?;
                if node.accepting() && !edges.is_empty() {
                    return Err(InvalidGraph);
                }
                for edge in edges {
                    if edge.to() <= index
                        || graph.node(edge.to()).is_none()
                        || usize::from(edge.operation_index()) >= key.placement_count()
                    {
                        return Err(InvalidGraph);
                    }
                }
            }
        }
        Ok(Self {
            height,
            initial,
            execution,
        })
    }

    pub const fn height(&self) -> u8 {
        self.height
    }
    pub const fn initial(&self) -> Board256Mask {
        self.initial
    }
    pub const fn execution(&self) -> &SpinCoverageExecutionBatch {
        &self.execution
    }
    pub fn into_execution(self) -> SpinCoverageExecutionBatch {
        self.execution
    }
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        self.execution.checked_nested_retained_bytes()
    }
    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        self.execution.checked_clone_nested_bytes()
    }
    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.execution.checked_clone_peak_bytes()
    }
}
