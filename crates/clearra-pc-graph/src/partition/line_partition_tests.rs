use clearra_core_domain::pc::pc_target::PcTarget;

use super::*;

#[test]
fn creates_expected_six_line_partitions() {
    let partitions = partitions_for_target(PcTarget::six_lines()).expect("6L is supported");
    let lines = partitions
        .iter()
        .map(|partition| {
            partition
                .increments()
                .iter()
                .map(|increment| increment.lines())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(lines, vec![vec![6], vec![2, 4], vec![4, 2], vec![2, 2, 2]]);
    assert_eq!(
        partitions
            .iter()
            .map(LinePartition::label)
            .collect::<Vec<_>>(),
        vec!["6", "2+4", "4+2", "2+2+2"]
    );
}

#[test]
fn rejects_non_mvp_target() {
    let target = PcTarget::new(8).expect("core-domain allows even positive target");

    assert_eq!(
        partitions_for_target(target),
        Err(LinePartitionError::UnsupportedTarget { lines: 8 })
    );
}
