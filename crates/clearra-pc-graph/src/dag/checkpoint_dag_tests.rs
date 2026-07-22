use clearra_core_domain::pc::pc_target::PcTarget;

use crate::request::opening_pc_search_query::OpeningPcSearchQuery;

use super::*;

#[test]
fn dag_contains_nodes_and_edges_for_all_partitions() {
    let query = OpeningPcSearchQuery::new(PcTarget::six_lines());
    let dag = CheckpointDag::from_opening_query(&query).expect("6L DAG");

    assert_eq!(dag.partitions().len(), 4);
    assert_eq!(dag.nodes().len(), 8);
    assert_eq!(dag.edges().len(), 4);
}

#[test]
fn continuation_tokens_are_deterministic() {
    let edge = ContinuationEdge::new(3, CheckpointId::new(4), CheckpointId::new(5));

    assert_eq!(
        edge.token(),
        ContinuationEdge::new(3, CheckpointId::new(4), CheckpointId::new(5)).token()
    );
}
