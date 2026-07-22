use clearra_core_domain::pc::pc_target::PcTarget;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContinuationHint {
    next_target: Option<PcTarget>,
    min_required_pieces: usize,
}

impl ContinuationHint {
    pub fn for_remaining_queue(remaining_queue_len: usize) -> Self {
        if remaining_queue_len >= 15 {
            Self::available(PcTarget::six_lines(), 15)
        } else if remaining_queue_len >= 10 {
            Self::available(PcTarget::four_lines(), 10)
        } else if remaining_queue_len >= 5 {
            Self::available(PcTarget::two_lines(), 5)
        } else {
            Self {
                next_target: None,
                min_required_pieces: 5,
            }
        }
    }
}
impl ContinuationHint {
    pub fn is_available(&self) -> bool {
        self.next_target.is_some()
    }
}
impl ContinuationHint {
    pub fn next_target(&self) -> Option<PcTarget> {
        self.next_target
    }
}
impl ContinuationHint {
    pub fn next_label(&self) -> &'static str {
        match self.next_target.map(|target| target.lines()) {
            Some(2) => "2L",
            Some(4) => "4L",
            Some(6) => "6L",
            _ => "none",
        }
    }
}
impl ContinuationHint {
    pub fn min_required_pieces(&self) -> usize {
        self.min_required_pieces
    }
}
impl ContinuationHint {
    fn available(target: PcTarget, min_required_pieces: usize) -> Self {
        Self {
            next_target: Some(target),
            min_required_pieces,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_hint_classifies_next_pc_from_remaining_queue() {
        assert_eq!(
            ContinuationHint::for_remaining_queue(4).next_label(),
            "none"
        );
        assert_eq!(ContinuationHint::for_remaining_queue(5).next_label(), "2L");
        assert_eq!(ContinuationHint::for_remaining_queue(10).next_label(), "4L");
        assert_eq!(ContinuationHint::for_remaining_queue(15).next_label(), "6L");
    }
}
