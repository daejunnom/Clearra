use clearra_core_ffi::native_thread_affinity::NativeAffinityError;
use clearra_core_ffi::native_thread_affinity::NativeNumaNode;
use clearra_core_ffi::native_thread_affinity::NativeThreadAffinity;

/// Scheduling-only metadata. It neither owns nor duplicates search data and
/// cannot change the requested logical worker count or memory authority.
pub(super) struct CpuWorkerTopology {
    nodes: Vec<NativeNumaNode>,
    worker_nodes: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TopologyFallback {
    Discovery(NativeAffinityError),
    IncompleteOrOverlappingNodes,
    SingleNode,
}

impl CpuWorkerTopology {
    /// Called once when the persistent pool is created. A single/unknown node
    /// leaves no placement object in the pool and adds no work to job dispatch.
    pub(super) fn detect() -> Result<Self, TopologyFallback> {
        let allowed = NativeThreadAffinity::current().map_err(TopologyFallback::Discovery)?;
        let nodes = NativeThreadAffinity::numa_nodes().map_err(TopologyFallback::Discovery)?;
        Self::from_observations(&allowed, nodes)
    }

    pub(super) fn from_observations(
        allowed: &NativeThreadAffinity,
        mut observed: Vec<NativeNumaNode>,
    ) -> Result<Self, TopologyFallback> {
        observed.sort_unstable_by_key(NativeNumaNode::node_id);
        if observed
            .windows(2)
            .any(|nodes| nodes[0].node_id() == nodes[1].node_id())
        {
            return Err(TopologyFallback::IncompleteOrOverlappingNodes);
        }
        let mut nodes = Vec::new();
        let mut covered = Vec::new();
        for node in observed {
            if let Some(affinity) = node.affinity().intersection(allowed) {
                covered.extend_from_slice(affinity.cpu_ids());
                nodes.push(NativeNumaNode::new(node.node_id(), affinity));
            }
        }
        covered.sort_unstable();
        // Equality proves complete, disjoint membership. Missing CPUs must not
        // disappear, and duplicate membership must not over-weight a node.
        if covered != allowed.cpu_ids() {
            return Err(TopologyFallback::IncompleteOrOverlappingNodes);
        }
        if nodes.len() <= 1 {
            return Err(TopologyFallback::SingleNode);
        }

        // Spread initial ready workers across nodes; each full cycle then gives
        // each node exactly as many slots as it has permitted logical CPUs.
        // CPUs are not confused with physical cores or collapsed SMT siblings.
        let mut worker_nodes = Vec::with_capacity(covered.len());
        let mut offset = 0;
        while worker_nodes.len() < covered.len() {
            for (node_index, node) in nodes.iter().enumerate() {
                if offset < node.affinity().cpu_ids().len() {
                    worker_nodes.push(node_index);
                }
            }
            offset += 1;
        }
        Ok(Self {
            nodes,
            worker_nodes,
        })
    }

    pub(super) fn affinity_for_worker(&self, worker: usize) -> &NativeThreadAffinity {
        self.nodes[self.worker_nodes[worker % self.worker_nodes.len()]].affinity()
    }
}

#[cfg(test)]
#[path = "cpu_worker_topology_tests.rs"]
mod tests;
