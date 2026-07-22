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
    match target.lines() {
        2 => Ok(vec![LinePartition::new(&[2])?]),
        4 => Ok(vec![
            LinePartition::new(&[4])?,
            LinePartition::new(&[2, 2])?,
        ]),
        6 => Ok(vec![
            LinePartition::new(&[6])?,
            LinePartition::new(&[2, 4])?,
            LinePartition::new(&[4, 2])?,
            LinePartition::new(&[2, 2, 2])?,
        ]),
        lines => Err(LinePartitionError::UnsupportedTarget { lines }),
    }
}

#[cfg(test)]
#[path = "line_partition_tests.rs"]
mod tests;
