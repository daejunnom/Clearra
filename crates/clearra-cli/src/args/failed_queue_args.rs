use super::PcArgs;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailedQueueArgs {
    pc: PcArgs,
    patterns: Option<String>,
    failed_pattern_limit: usize,
}

impl FailedQueueArgs {
    pub fn new(pc: PcArgs, patterns: Option<String>, failed_pattern_limit: usize) -> Self {
        Self {
            pc,
            patterns,
            failed_pattern_limit,
        }
    }

    pub fn pc(&self) -> &PcArgs {
        &self.pc
    }

    pub fn patterns(&self) -> Option<&str> {
        self.patterns.as_deref()
    }

    pub const fn failed_pattern_limit(&self) -> usize {
        self.failed_pattern_limit
    }
}
