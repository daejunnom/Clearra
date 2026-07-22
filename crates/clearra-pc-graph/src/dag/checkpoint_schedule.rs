use clearra_core_domain::pc::pc_target::PcTarget;

use crate::partition::line_partition::{partitions_for_target, LinePartition, LinePartitionError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckpointSchedule {
    target: PcTarget,
    label: String,
    partitions: Vec<LinePartition>,
}

impl CheckpointSchedule {
    pub fn for_opening_target(target: PcTarget) -> Result<Self, LinePartitionError> {
        Ok(Self {
            target,
            label: format!("{}L", target.lines()),
            partitions: partitions_for_target(target)?,
        })
    }
}
impl CheckpointSchedule {
    pub fn target(&self) -> PcTarget {
        self.target
    }
}
impl CheckpointSchedule {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl CheckpointSchedule {
    pub fn partitions(&self) -> &[LinePartition] {
        &self.partitions
    }
}
impl CheckpointSchedule {
    pub fn partition_labels(&self) -> Vec<String> {
        self.partitions.iter().map(LinePartition::label).collect()
    }
}
impl CheckpointSchedule {
    pub fn checkpoint_count(&self) -> usize {
        self.partitions
            .iter()
            .map(|partition| partition.increments().len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::pc::pc_target::PcTarget;

    use super::*;

    #[test]
    fn opening_schedule_is_label_metadata_not_executor_state() {
        let schedule =
            CheckpointSchedule::for_opening_target(PcTarget::six_lines()).expect("schedule");

        assert_eq!(schedule.label(), "6L");
        assert_eq!(
            schedule.partition_labels(),
            vec!["6", "2+4", "4+2", "2+2+2"]
        );
        assert_eq!(schedule.checkpoint_count(), 8);
    }
}
