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

    /// Returns only heap payload retained by a queue representation accepted
    /// by the typed `pc.score` contract.
    ///
    /// Fixed and pattern-expression buffers are measured by allocation
    /// capacity. `Standard7Bag` is inline. Queue kinds outside the score
    /// contract return `None` so callers fail closed rather than treating an
    /// unmeasured owner as zero bytes. The inline `PcQueueInput` is excluded.
    pub fn checked_pc_score_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::FixedSequence(sequence) => sequence.checked_retained_capacity_bytes(),
            Self::PatternExpression(expression) => expression.checked_retained_capacity_bytes(),
            Self::Standard7Bag => Some(0),
            Self::BagAlignedPattern(_) | Self::Observed(_) => None,
        }
    }

    /// Returns the heap payload retained by a queue emitted by the Web
    /// Build-probability parser. Fixed and pattern-expression buffers are
    /// measured by allocation capacity and `Standard7Bag` is inline. Other
    /// queue variants fail closed because they are outside that finite ingress
    /// contract. The inline enum owner is excluded.
    pub fn checked_build_probability_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::FixedSequence(sequence) => sequence.checked_retained_capacity_bytes(),
            Self::PatternExpression(expression) => expression.checked_retained_capacity_bytes(),
            Self::Standard7Bag => Some(0),
            Self::BagAlignedPattern(_) | Self::Observed(_) => None,
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

    #[test]
    fn pc_score_retained_capacity_delegates_to_accepted_queue_owner() {
        let mut pieces = Vec::with_capacity(128);
        pieces.push(PieceKind::I);
        let expected =
            (pieces.capacity() as u128).checked_mul(core::mem::size_of::<PieceKind>() as u128);
        let fixed = PcQueueInput::fixed_sequence(FixedSequence::new(pieces));
        assert_eq!(fixed.checked_pc_score_retained_capacity_bytes(), expected);

        let expression =
            QueuePatternExpression::parse("P7P7P2", 1_066_867_200).expect("factorized expression");
        let expected = expression.checked_retained_capacity_bytes();
        let expression = PcQueueInput::pattern_expression(expression);
        assert_eq!(
            expression.checked_pc_score_retained_capacity_bytes(),
            expected
        );
        assert_eq!(
            PcQueueInput::Standard7Bag.checked_pc_score_retained_capacity_bytes(),
            Some(0)
        );
    }

    #[test]
    fn pc_score_retained_capacity_fails_closed_for_rejected_queue_kinds() {
        let bag = PcQueueInput::bag_aligned_pattern(BagAlignedPattern::new(vec![PieceKind::I]));
        let observed = PcQueueInput::observed(ObservedQueue::new(vec![PieceKind::I]));

        assert_eq!(bag.checked_pc_score_retained_capacity_bytes(), None);
        assert_eq!(observed.checked_pc_score_retained_capacity_bytes(), None);
    }

    #[test]
    fn build_probability_retained_capacity_accepts_only_web_parser_queue_owners() {
        let mut fixed_pieces = Vec::with_capacity(17);
        fixed_pieces.push(PieceKind::I);
        let fixed_capacity = fixed_pieces.capacity();
        let fixed = PcQueueInput::fixed_sequence(FixedSequence::new(fixed_pieces));

        let expression =
            QueuePatternExpression::parse("P7P7P2", 1_066_867_200).expect("factorized expression");
        let expression_capacity = expression.checked_retained_capacity_bytes();
        let expression = PcQueueInput::pattern_expression(expression);

        let piece_size = core::mem::size_of::<PieceKind>() as u128;
        assert_eq!(
            fixed.checked_build_probability_retained_capacity_bytes(),
            (fixed_capacity as u128).checked_mul(piece_size)
        );
        assert_eq!(
            expression.checked_build_probability_retained_capacity_bytes(),
            expression_capacity
        );
        assert_eq!(
            PcQueueInput::Standard7Bag.checked_build_probability_retained_capacity_bytes(),
            Some(0)
        );

        let bag = PcQueueInput::bag_aligned_pattern(BagAlignedPattern::new(vec![PieceKind::O]));
        let observed = PcQueueInput::observed(ObservedQueue::new(vec![PieceKind::T]));
        assert_eq!(
            bag.checked_build_probability_retained_capacity_bytes(),
            None
        );
        assert_eq!(
            observed.checked_build_probability_retained_capacity_bytes(),
            None
        );
    }
}
