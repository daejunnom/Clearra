use clearra_supply::queue::{
    bag_aligned_pattern::BagAlignedPattern, fixed_queue::FixedQueue, fixed_sequence::FixedSequence,
    observed_queue::ObservedQueue, queue_pattern_expression::QueuePatternExpression,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcQueueInput {
    FixedSequence(FixedSequence),
    BagAlignedPattern(BagAlignedPattern),
    PatternExpression(QueuePatternExpression),
    Standard7Bag,
    Observed(ObservedQueue),
}

impl PcQueueInput {
    pub fn fixed(queue: FixedQueue) -> Self {
        Self::fixed_sequence(queue)
    }
}
impl PcQueueInput {
    pub fn fixed_sequence(sequence: FixedSequence) -> Self {
        Self::FixedSequence(sequence)
    }
}
impl PcQueueInput {
    pub fn bag_aligned_pattern(pattern: BagAlignedPattern) -> Self {
        Self::BagAlignedPattern(pattern)
    }
}
impl PcQueueInput {
    pub fn pattern_expression(expression: QueuePatternExpression) -> Self {
        Self::PatternExpression(expression)
    }
}
impl PcQueueInput {
    pub const fn standard_7_bag() -> Self {
        Self::Standard7Bag
    }
}
impl PcQueueInput {
    pub fn observed(queue: ObservedQueue) -> Self {
        Self::Observed(queue)
    }
}
impl PcQueueInput {
    pub fn mode(&self) -> &'static str {
        match self {
            Self::FixedSequence(_) => "fixed",
            Self::BagAlignedPattern(_) => "bag-aligned-pattern",
            Self::PatternExpression(_) => "materialized-pattern-expression",
            Self::Standard7Bag => "standard-7-bag",
            Self::Observed(_) => "observed",
        }
    }
}
impl PcQueueInput {
    pub fn len(&self) -> usize {
        match self {
            Self::FixedSequence(queue) => queue.len(),
            Self::BagAlignedPattern(pattern) => pattern.len(),
            Self::PatternExpression(expression) => expression.sequence_len(),
            Self::Standard7Bag => 7,
            Self::Observed(queue) => queue.len(),
        }
    }
}
impl PcQueueInput {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl PcQueueInput {
    pub fn fixed_queue(&self) -> Option<&FixedQueue> {
        self.as_fixed_sequence()
    }
}
impl PcQueueInput {
    pub fn as_fixed_sequence(&self) -> Option<&FixedSequence> {
        match self {
            Self::FixedSequence(sequence) => Some(sequence),
            Self::BagAlignedPattern(_)
            | Self::PatternExpression(_)
            | Self::Standard7Bag
            | Self::Observed(_) => None,
        }
    }
}
impl PcQueueInput {
    pub fn as_bag_aligned_pattern(&self) -> Option<&BagAlignedPattern> {
        match self {
            Self::BagAlignedPattern(pattern) => Some(pattern),
            Self::FixedSequence(_)
            | Self::PatternExpression(_)
            | Self::Standard7Bag
            | Self::Observed(_) => None,
        }
    }
}
impl PcQueueInput {
    pub fn as_pattern_expression(&self) -> Option<&QueuePatternExpression> {
        match self {
            Self::PatternExpression(expression) => Some(expression),
            Self::FixedSequence(_)
            | Self::BagAlignedPattern(_)
            | Self::Standard7Bag
            | Self::Observed(_) => None,
        }
    }
}
impl PcQueueInput {
    pub fn observed_queue(&self) -> Option<&ObservedQueue> {
        match self {
            Self::FixedSequence(_)
            | Self::BagAlignedPattern(_)
            | Self::PatternExpression(_)
            | Self::Standard7Bag => None,
            Self::Observed(queue) => Some(queue),
        }
    }
}

impl Default for PcQueueInput {
    fn default() -> Self {
        Self::Observed(ObservedQueue::default())
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use super::*;

    #[test]
    fn pc_queue_input_distinguishes_fixed_and_observed_queues() {
        let fixed =
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::O]));
        let bag_pattern = PcQueueInput::bag_aligned_pattern(BagAlignedPattern::new(vec![
            PieceKind::I,
            PieceKind::O,
        ]));
        let observed = PcQueueInput::observed(ObservedQueue::new(vec![PieceKind::T]));

        assert_eq!(fixed.mode(), "fixed");
        assert_eq!(fixed.len(), 2);
        assert!(fixed.as_fixed_sequence().is_some());
        assert_eq!(bag_pattern.mode(), "bag-aligned-pattern");
        assert!(bag_pattern.as_bag_aligned_pattern().is_some());
        assert_eq!(observed.mode(), "observed");
        assert_eq!(observed.len(), 1);
        assert!(observed.observed_queue().is_some());
    }
}
