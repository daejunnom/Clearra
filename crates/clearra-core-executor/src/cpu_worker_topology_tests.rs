use clearra_core_ffi::native_thread_affinity::NativeNumaNode;
use clearra_core_ffi::native_thread_affinity::NativeThreadAffinity;

use super::CpuWorkerTopology;
use super::TopologyFallback;

fn affinity(group: u16, cpus: &[u32]) -> NativeThreadAffinity {
    NativeThreadAffinity::from_cpu_ids(group, cpus.to_vec()).expect("valid fixture CPU set")
}

#[test]
fn numa_topology_respects_sparse_allowed_cpus_and_balances_ready_workers() {
    let allowed = affinity(0, &[2, 4, 8, 20, 21, 22]);
    let topology = CpuWorkerTopology::from_observations(
        &allowed,
        vec![
            NativeNumaNode::new(7, affinity(0, &[0, 2, 4, 8])),
            NativeNumaNode::new(19, affinity(0, &[20, 21])),
            NativeNumaNode::new(33, affinity(0, &[22, 24])),
        ],
    )
    .expect("complete multi-node topology");
    assert_eq!(topology.worker_nodes, [0, 1, 2, 0, 1, 0]);
    let mut weights = [0; 3];
    for worker in 0..allowed.cpu_ids().len() * 2 {
        let placement = topology.affinity_for_worker(worker);
        assert_eq!(placement.processor_group(), allowed.processor_group());
        assert!(placement
            .cpu_ids()
            .iter()
            .all(|cpu| allowed.cpu_ids().contains(cpu)));
        weights[topology.worker_nodes[worker % topology.worker_nodes.len()]] += 1;
    }
    assert_eq!(weights, [6, 4, 2]);
}

#[test]
fn numa_topology_single_node_and_restricted_single_node_use_original_scheduler() {
    for allowed in [affinity(0, &[0, 1, 2, 3]), affinity(0, &[1, 3])] {
        let result = CpuWorkerTopology::from_observations(
            &allowed,
            vec![
                NativeNumaNode::new(0, affinity(0, &[0, 1, 2, 3])),
                NativeNumaNode::new(1, affinity(0, &[8, 9])),
            ],
        );
        assert!(matches!(result, Err(TopologyFallback::SingleNode)));
    }
}

#[test]
fn numa_topology_incomplete_or_overlapping_membership_falls_back_without_clamping() {
    let allowed = affinity(0, &[0, 1, 2, 3]);
    for nodes in [
        Vec::new(),
        vec![NativeNumaNode::new(0, affinity(0, &[0, 1]))],
        vec![
            NativeNumaNode::new(0, affinity(0, &[0, 1, 2])),
            NativeNumaNode::new(1, affinity(0, &[2, 3])),
        ],
        vec![
            NativeNumaNode::new(0, affinity(0, &[0, 1])),
            NativeNumaNode::new(0, affinity(0, &[2, 3])),
        ],
    ] {
        assert!(matches!(
            CpuWorkerTopology::from_observations(&allowed, nodes),
            Err(TopologyFallback::IncompleteOrOverlappingNodes)
        ));
        assert_eq!(allowed.cpu_ids(), [0, 1, 2, 3]);
    }
}

#[test]
fn numa_topology_keeps_group_relative_ids_distinct_and_unknown_group_is_fallback() {
    let allowed = affinity(2, &[0, 1, 62, 63]);
    let wrong_group = vec![NativeNumaNode::new(0, affinity(0, &[0, 1, 62, 63]))];
    assert!(matches!(
        CpuWorkerTopology::from_observations(&allowed, wrong_group),
        Err(TopologyFallback::IncompleteOrOverlappingNodes)
    ));
    let topology = CpuWorkerTopology::from_observations(
        &allowed,
        vec![
            NativeNumaNode::new(0, affinity(2, &[0, 1])),
            NativeNumaNode::new(1, affinity(2, &[62, 63])),
        ],
    )
    .expect("complete group-relative topology");
    assert_eq!(topology.affinity_for_worker(1).cpu_ids(), &[62, 63]);
    assert_eq!(topology.affinity_for_worker(1).processor_group(), 2);
}
