use clearra_core_domain::ids::checkpoint_id::CheckpointId;

use crate::{
    dag::{checkpoint_node::CheckpointNode, continuation_edge::ContinuationEdge},
    partition::{
        line_partition::{partitions_for_target, LinePartition, LinePartitionError},
        phase_increment::PhaseIncrement,
    },
    request::opening_pc_search_query::OpeningPcSearchQuery,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckpointDag {
    partitions: Vec<LinePartition>,
    nodes: Vec<CheckpointNode>,
    edges: Vec<ContinuationEdge>,
}

impl CheckpointDag {
    pub fn from_opening_query(query: &OpeningPcSearchQuery) -> Result<Self, LinePartitionError> {
        Self::from_partitions(partitions_for_target(query.target())?)
    }
}
impl CheckpointDag {
    pub fn from_partitions(partitions: Vec<LinePartition>) -> Result<Self, LinePartitionError> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut next_id = 0_u32;

        for (partition_index, partition) in partitions.iter().enumerate() {
            let mut previous: Option<CheckpointNode> = None;
            let mut cumulative_lines = 0_u8;

            for (phase_index, increment) in partition.increments().iter().copied().enumerate() {
                let node = build_node(
                    next_id,
                    partition_index as u32,
                    phase_index as u32,
                    increment,
                    &mut cumulative_lines,
                );
                next_id += 1;

                if let Some(previous_node) = previous {
                    edges.push(ContinuationEdge::new(
                        partition_index as u32,
                        previous_node.id(),
                        node.id(),
                    ));
                }

                previous = Some(node);
                nodes.push(node);
            }
        }

        Ok(Self {
            partitions,
            nodes,
            edges,
        })
    }
}
impl CheckpointDag {
    pub fn partitions(&self) -> &[LinePartition] {
        &self.partitions
    }
}
impl CheckpointDag {
    pub fn nodes(&self) -> &[CheckpointNode] {
        &self.nodes
    }
}
impl CheckpointDag {
    pub fn edges(&self) -> &[ContinuationEdge] {
        &self.edges
    }
}
impl CheckpointDag {
    pub fn node(self: &Self, id: CheckpointId) -> Option<CheckpointNode> {
        self.nodes.iter().copied().find(|node| node.id() == id)
    }
}
impl CheckpointDag {
    pub fn nodes_for_partition(
        &self,
        partition_index: u32,
    ) -> impl Iterator<Item = CheckpointNode> + '_ {
        self.nodes
            .iter()
            .copied()
            .filter(move |node| node.partition_index() == partition_index)
    }
}

fn build_node(
    id: u32,
    partition_index: u32,
    phase_index: u32,
    increment: PhaseIncrement,
    cumulative_lines: &mut u8,
) -> CheckpointNode {
    *cumulative_lines += increment.lines();
    CheckpointNode::new(
        CheckpointId::new(id),
        partition_index,
        phase_index,
        increment.lines(),
        *cumulative_lines,
    )
}

#[cfg(test)]
#[path = "checkpoint_dag_tests.rs"]
mod tests;
