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

    /// Returns only heap payload retained by the checkpoint label, the outer
    /// partition buffer, and every nested increment buffer.
    ///
    /// All vector payloads are measured by allocation capacity. The inline
    /// `CheckpointSchedule` and inline partition owners are excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self.label.capacity() as u128;
        bytes = bytes.checked_add(checked_count_bytes(
            self.partitions.capacity() as u128,
            core::mem::size_of::<LinePartition>() as u128,
        )?)?;
        for partition in &self.partitions {
            bytes = bytes.checked_add(partition.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
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

    #[test]
    fn opening_schedule_retained_capacity_matches_outer_and_nested_buffers() {
        let schedule =
            CheckpointSchedule::for_opening_target(PcTarget::six_lines()).expect("schedule");
        let expected = (schedule.label.capacity() as u128)
            .checked_add(
                (schedule.partitions.capacity() as u128)
                    .checked_mul(core::mem::size_of::<LinePartition>() as u128)
                    .expect("outer partition storage fits u128"),
            )
            .and_then(|mut bytes| {
                for partition in &schedule.partitions {
                    bytes = bytes.checked_add(partition.checked_retained_capacity_bytes()?)?;
                }
                Some(bytes)
            });

        assert_eq!(schedule.checked_retained_capacity_bytes(), expected);
    }

    #[test]
    fn schedule_capacity_arithmetic_fails_closed_on_overflow() {
        assert_eq!(checked_count_bytes(u128::MAX, 2), None);
    }
}
