use clearra_pc_graph::request::{
    PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_supply::queue::queue_parser::{
    parse_bag_aligned_pattern, parse_fixed_sequence, parse_observed_queue,
};

use crate::args::{PercentArgs, PercentQueueMode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PercentQueryAssembly {
    query: PcScenarioQuery,
}

impl PercentQueryAssembly {
    pub fn query(&self) -> &PcScenarioQuery {
        &self.query
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PercentQueryAssemblyError {
    InvalidObservedQueue,
    InvalidBagAlignedPattern,
    InvalidFixedSequence,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PercentQueryAssembler;

impl PercentQueryAssembler {
    pub fn assemble(args: &PercentArgs) -> Result<PercentQueryAssembly, PercentQueryAssemblyError> {
        let queue = match args.mode() {
            PercentQueueMode::Observed => PcQueueInput::observed(
                parse_observed_queue(args.queue())
                    .map_err(|_| PercentQueryAssemblyError::InvalidObservedQueue)?,
            ),
            PercentQueueMode::BagAligned => PcQueueInput::bag_aligned_pattern(
                parse_bag_aligned_pattern(args.queue())
                    .map_err(|_| PercentQueryAssemblyError::InvalidBagAlignedPattern)?,
            ),
            PercentQueueMode::Fixed => PcQueueInput::fixed_sequence(
                parse_fixed_sequence(args.queue())
                    .map_err(|_| PercentQueryAssemblyError::InvalidFixedSequence)?,
            ),
        };
        let minimum_len = args.minimum_len().unwrap_or(queue.len());
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0x3f0),
            queue,
            PieceWindow::new(minimum_len.max(1)),
        )
        .with_execution_policy(
            PcExecutionPolicy::mvp_default().with_max_patterns(args.max_patterns()),
        );

        Ok(PercentQueryAssembly { query })
    }
}

#[cfg(test)]
#[path = "percent_query_assembler_tests.rs"]
mod tests;
