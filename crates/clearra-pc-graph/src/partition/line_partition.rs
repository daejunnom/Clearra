use clearra_core_domain::pc::pc_target::PcTarget;

use super::phase_increment::{PhaseIncrement, PhaseIncrementError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinePartition {
    increments: Vec<PhaseIncrement>,
}

impl LinePartition {
    pub fn new(lines: &[u8]) -> Result<Self, LinePartitionError> {
        let increments = lines
            .iter()
            .copied()
            .map(PhaseIncrement::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(LinePartitionError::UnsupportedIncrement)?;
        Ok(Self { increments })
    }
}
impl LinePartition {
    pub fn increments(&self) -> &[PhaseIncrement] {
        &self.increments
    }
}
impl LinePartition {
    pub fn total_lines(&self) -> u8 {
        self.increments
            .iter()
            .map(|increment| increment.lines())
            .sum()
    }

    /// Returns only the heap payload retained by the increment buffer,
    /// measured by allocation capacity. The inline partition is excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        checked_count_bytes(
            self.increments.capacity() as u128,
            core::mem::size_of::<PhaseIncrement>() as u128,
        )
    }
}
impl LinePartition {
    pub fn label(&self) -> String {
        self.increments
            .iter()
            .map(|increment| increment.lines().to_string())
            .collect::<Vec<_>>()
            .join("+")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinePartitionError {
    UnsupportedTarget { lines: u8 },
    UnsupportedIncrement(PhaseIncrementError),
}

pub fn partitions_for_target(target: PcTarget) -> Result<Vec<LinePartition>, LinePartitionError> {
    // This is bounded label metadata, not execution of intermediate PCs.
    // Compositions of n half-lines number 2^(n-1), at most 2048 for 24L.
    // Length-then-lexicographic order preserves every legacy 2/4/6L label.
    let mut partitions = Vec::with_capacity(1 << (target.lines() / 2 - 1));
    let mut prefix = Vec::with_capacity(usize::from(target.lines() / 2));
    append_partitions(target.lines(), &mut prefix, &mut partitions)?;
    partitions.sort_unstable_by(|left, right| {
        left.increments
            .len()
            .cmp(&right.increments.len())
            .then_with(|| left.increments.cmp(&right.increments))
    });
    Ok(partitions)
}

fn append_partitions(
    remaining: u8,
    prefix: &mut Vec<u8>,
    partitions: &mut Vec<LinePartition>,
) -> Result<(), LinePartitionError> {
    if remaining == 0 {
        partitions.push(LinePartition::new(prefix)?);
        return Ok(());
    }
    for lines in (2..=remaining).step_by(2) {
        prefix.push(lines);
        append_partitions(remaining - lines, prefix, partitions)?;
        prefix.pop();
    }
    Ok(())
}

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
}

#[cfg(test)]
#[path = "line_partition_tests.rs"]
mod tests;

#[cfg(test)]
mod retained_capacity_tests {
    use super::{checked_count_bytes, LinePartition};

    #[test]
    fn partition_retained_capacity_counts_allocated_increment_slots() {
        let partition = LinePartition::new(&[2, 2, 2]).expect("partition");
        let expected = (partition.increments.capacity() as u128)
            .checked_mul(core::mem::size_of_val(&partition.increments[0]) as u128);

        assert_eq!(partition.checked_retained_capacity_bytes(), expected);
    }

    #[test]
    fn partition_capacity_arithmetic_fails_closed_on_overflow() {
        assert_eq!(checked_count_bytes(u128::MAX, 2), None);
    }
}
