use crate::pc::pc_target::PcTarget;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcResultStatus {
    Complete,
    Partial,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcResult {
    target: PcTarget,
    cleared_lines: u8,
    status: PcResultStatus,
}

impl PcResult {
    pub fn new(target: PcTarget, cleared_lines: u8, status: PcResultStatus) -> Self {
        Self {
            target,
            cleared_lines,
            status,
        }
    }
}
impl PcResult {
    pub fn complete(target: PcTarget) -> Self {
        Self::new(target, target.lines(), PcResultStatus::Complete)
    }
}
impl PcResult {
    pub fn target(&self) -> PcTarget {
        self.target
    }
}
impl PcResult {
    pub fn cleared_lines(&self) -> u8 {
        self.cleared_lines
    }
}
impl PcResult {
    pub fn status(&self) -> PcResultStatus {
        self.status
    }
}
impl PcResult {
    pub fn is_complete(&self) -> bool {
        self.status == PcResultStatus::Complete
    }
}
