use clearra_supply::queue::{
    bag_aligned_pattern::BagAlignedPattern, fixed_queue::FixedQueue, fixed_sequence::FixedSequence,
    observed_queue::ObservedQueue, queue_pattern_expression::QueuePatternExpression,
};
use std::borrow::Cow;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupQueueInput {
    FixedSequence(FixedSequence),
    PatternExpression(QueuePatternExpression),
    BagAlignedPattern(BagAlignedPattern),
    Observed(ObservedQueue),
}

impl SetupQueueInput {
    pub fn fixed(queue: FixedQueue) -> Self {
        Self::fixed_sequence(queue)
    }
}
impl SetupQueueInput {
    pub fn fixed_sequence(sequence: FixedSequence) -> Self {
        Self::FixedSequence(sequence)
    }
}
impl SetupQueueInput {
    pub fn pattern_expression(expression: QueuePatternExpression) -> Self {
        Self::PatternExpression(expression)
    }
}
impl SetupQueueInput {
    pub fn bag_aligned_pattern(pattern: BagAlignedPattern) -> Self {
        Self::BagAlignedPattern(pattern)
    }
}
impl SetupQueueInput {
    pub fn observed(queue: ObservedQueue) -> Self {
        Self::Observed(queue)
    }
}
impl SetupQueueInput {
    pub fn mode(&self) -> &'static str {
        match self {
            Self::FixedSequence(_) => "fixed",
            Self::PatternExpression(_) => "pattern-expression",
            Self::BagAlignedPattern(_) => "bag-aligned-pattern",
            Self::Observed(_) => "observed",
        }
    }
}
impl SetupQueueInput {
    pub fn len(&self) -> usize {
        match self {
            Self::FixedSequence(queue) => queue.len(),
            Self::PatternExpression(expression) => expression.sequence_len(),
            Self::BagAlignedPattern(pattern) => pattern.len(),
            Self::Observed(queue) => queue.len(),
        }
    }
}
impl SetupQueueInput {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl SetupQueueInput {
    pub fn fixed_queue(&self) -> Option<&FixedQueue> {
        self.as_fixed_sequence()
    }
}
impl SetupQueueInput {
    pub fn as_fixed_sequence(&self) -> Option<&FixedSequence> {
        match self {
            Self::FixedSequence(sequence) => Some(sequence),
            Self::PatternExpression(_) | Self::BagAlignedPattern(_) | Self::Observed(_) => None,
        }
    }
}
impl SetupQueueInput {
    pub fn as_pattern_expression(&self) -> Option<&QueuePatternExpression> {
        match self {
            Self::PatternExpression(expression) => Some(expression),
            Self::FixedSequence(_) | Self::BagAlignedPattern(_) | Self::Observed(_) => None,
        }
    }

    pub fn queue_based_source(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::FixedSequence(sequence) => Some(Cow::Owned(
                sequence
                    .pieces()
                    .iter()
                    .map(|piece| piece.as_ascii())
                    .collect(),
            )),
            Self::PatternExpression(expression) => Some(Cow::Borrowed(expression.source())),
            Self::BagAlignedPattern(_) | Self::Observed(_) => None,
        }
    }
}
impl SetupQueueInput {
    pub fn as_bag_aligned_pattern(&self) -> Option<&BagAlignedPattern> {
        match self {
            Self::BagAlignedPattern(pattern) => Some(pattern),
            Self::FixedSequence(_) | Self::PatternExpression(_) | Self::Observed(_) => None,
        }
    }
}
impl SetupQueueInput {
    pub fn observed_queue(&self) -> Option<&ObservedQueue> {
        match self {
            Self::FixedSequence(_) | Self::PatternExpression(_) | Self::BagAlignedPattern(_) => {
                None
            }
            Self::Observed(queue) => Some(queue),
        }
    }
}

impl Default for SetupQueueInput {
    fn default() -> Self {
        Self::Observed(ObservedQueue::default())
    }
}
