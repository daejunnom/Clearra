use clearra_replay::trace::PlacementStep;
#[cfg(test)]
use clearra_replay::ScoringExecutionEdge;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClearEvent {
    lines: u8,
    perfect_clear: bool,
}

impl ClearEvent {
    pub const fn new(lines: u8, perfect_clear: bool) -> Self {
        Self {
            lines,
            perfect_clear,
        }
    }

    /// Projects facts from replay's already-accepted physical lock. A
    /// Perfect Clear needs both a line clear and the empty post-clear board;
    /// it is independent of how many lines were cleared.
    pub(crate) fn from_placement_step(step: PlacementStep) -> Self {
        let lines = step.line_clear().cleared_lines();
        Self::new(
            lines,
            lines > 0 && step.board_after().after_line_clear().is_empty(),
        )
    }

    /// The graph producer has already determined emptiness after line clear.
    #[cfg(test)]
    pub(crate) const fn from_execution_edge(edge: ScoringExecutionEdge) -> Self {
        Self::new(edge.cleared_lines(), edge.perfect_clear())
    }
}
impl ClearEvent {
    pub const fn lines(self) -> u8 {
        self.lines
    }
}
impl ClearEvent {
    pub const fn is_perfect_clear(self) -> bool {
        self.perfect_clear
    }

    pub(crate) const fn clears_lines(self) -> bool {
        self.lines > 0
    }

    pub(crate) const fn is_four_line_clear(self) -> bool {
        self.lines == 4
    }
}
