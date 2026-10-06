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
fn full_height_opening_partitions_are_complete_bounded_and_canonical_metadata() {
    for lines in (2..=24).step_by(2) {
        let partitions = partitions_for_target(PcTarget::new(lines).unwrap()).unwrap();
        assert_eq!(partitions.len(), 1 << (lines / 2 - 1));
        assert_eq!(partitions[0].label(), lines.to_string());
        assert!(partitions
            .iter()
            .all(|partition| partition.total_lines() == lines));
        assert!(partitions.windows(2).all(|pair| {
            (pair[0].increments.len(), &pair[0].increments)
                < (pair[1].increments.len(), &pair[1].increments)
        }));
    }
    for invalid in [0, 1, 3, 7, 23, 25, 26] {
        assert!(PhaseIncrement::new(invalid).is_err());
    }
}
