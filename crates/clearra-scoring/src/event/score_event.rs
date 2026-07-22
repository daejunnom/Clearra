use clearra_replay::trace::PlacementStep;

use crate::{
    event::{clear_event::ClearEvent, spin_detector::SpinDetector, spin_event::SpinEvent},
    profile::{B2BChainRule, SpinRuleId},
    state::combo_state::ComboState,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreEvent {
    step_index: usize,
    clear: ClearEvent,
    spin: Option<SpinEvent>,
    combo_before: ComboState,
    combo_after: ComboState,
    b2b_before: u32,
    b2b_after: u32,
}

impl ScoreEvent {
    pub fn new(
        step_index: usize,
        clear: ClearEvent,
        spin: Option<SpinEvent>,
        combo_before: ComboState,
        combo_after: ComboState,
        b2b_before: bool,
        b2b_after: bool,
    ) -> Self {
        Self {
            step_index,
            clear,
            spin,
            combo_before,
            combo_after,
            b2b_before: u32::from(b2b_before),
            b2b_after: u32::from(b2b_after),
        }
    }
}
impl ScoreEvent {
    pub fn new_with_b2b_chain(
        step_index: usize,
        clear: ClearEvent,
        spin: Option<SpinEvent>,
        combo_before: ComboState,
        combo_after: ComboState,
        b2b_before: u32,
        b2b_after: u32,
    ) -> Self {
        Self {
            step_index,
            clear,
            spin,
            combo_before,
            combo_after,
            b2b_before,
            b2b_after,
        }
    }
}
impl ScoreEvent {
    pub fn from_classified_clear_with_b2b_chain(
        step_index: usize,
        clear: ClearEvent,
        spin: Option<SpinEvent>,
        combo_before: ComboState,
        b2b_before: u32,
        b2b_chain_rule: B2BChainRule,
    ) -> Self {
        let combo_after = combo_before.advance(clear.lines());
        let b2b_after = b2b_after_clear(b2b_before, clear.lines(), spin, b2b_chain_rule);
        Self::new_with_b2b_chain(
            step_index,
            clear,
            spin,
            combo_before,
            combo_after,
            b2b_before,
            b2b_after,
        )
    }
}
impl ScoreEvent {
    pub fn from_step(
        step: PlacementStep,
        spin_rule: SpinRuleId,
        combo_before: ComboState,
        b2b_before: bool,
    ) -> Self {
        // score_event_from_step_postprocess_only: this adapter consumes accepted replay
        // evidence and must never become a PackingCandidate pruning source.
        // score_must_not_prune_packing_candidate
        let lines = step.line_clear().cleared_lines();
        let clear = ClearEvent::new(
            lines,
            lines > 0 && step.board_after().after_line_clear().is_empty(),
        );
        let spin = SpinDetector::detect(step, spin_rule, lines);
        let combo_after = combo_before.advance(lines);
        let b2b_after = b2b_after_clear(
            u32::from(b2b_before),
            lines,
            spin,
            B2BChainRule::UnderlyingDifficultClearOnly,
        );

        Self::new_with_b2b_chain(
            step.step_index(),
            clear,
            spin,
            combo_before,
            combo_after,
            u32::from(b2b_before),
            b2b_after,
        )
    }
}
impl ScoreEvent {
    pub(crate) fn from_replay_step_with_b2b_chain(
        trace: &clearra_replay::ReplayTrace,
        step: PlacementStep,
        spin_rule: SpinRuleId,
        combo_before: ComboState,
        b2b_before: u32,
        b2b_chain_rule: B2BChainRule,
    ) -> Self {
        let lines = step.line_clear().cleared_lines();
        let clear = ClearEvent::new(
            lines,
            lines > 0 && step.board_after().after_line_clear().is_empty(),
        );
        let spin = SpinDetector::detect_replay_step(trace, step, spin_rule, lines);
        let combo_after = combo_before.advance(lines);
        let b2b_after = b2b_after_clear(b2b_before, lines, spin, b2b_chain_rule);
        Self::new_with_b2b_chain(
            step.step_index(),
            clear,
            spin,
            combo_before,
            combo_after,
            b2b_before,
            b2b_after,
        )
    }
}
impl ScoreEvent {
    pub(crate) fn from_step_with_b2b_chain(
        step: PlacementStep,
        spin_rule: SpinRuleId,
        combo_before: ComboState,
        b2b_before: u32,
        b2b_chain_rule: B2BChainRule,
    ) -> Self {
        let lines = step.line_clear().cleared_lines();
        let clear = ClearEvent::new(
            lines,
            lines > 0 && step.board_after().after_line_clear().is_empty(),
        );
        let spin = SpinDetector::detect(step, spin_rule, lines);
        let combo_after = combo_before.advance(lines);
        let b2b_after = b2b_after_clear(b2b_before, lines, spin, b2b_chain_rule);
        Self::new_with_b2b_chain(
            step.step_index(),
            clear,
            spin,
            combo_before,
            combo_after,
            b2b_before,
            b2b_after,
        )
    }
}
impl ScoreEvent {
    pub fn step_index(self) -> usize {
        self.step_index
    }
}
impl ScoreEvent {
    pub fn clear(self) -> ClearEvent {
        self.clear
    }
}
impl ScoreEvent {
    pub fn spin(self) -> Option<SpinEvent> {
        self.spin
    }
}
impl ScoreEvent {
    pub fn combo_before(self) -> ComboState {
        self.combo_before
    }
}
impl ScoreEvent {
    pub fn combo_after(self) -> ComboState {
        self.combo_after
    }
}
impl ScoreEvent {
    pub fn b2b_before(self) -> bool {
        self.b2b_before > 0
    }
}
impl ScoreEvent {
    pub fn b2b_before_chain(self) -> u32 {
        self.b2b_before
    }
}
impl ScoreEvent {
    pub fn b2b_after(self) -> bool {
        self.b2b_after > 0
    }
}
impl ScoreEvent {
    pub fn b2b_after_chain(self) -> u32 {
        self.b2b_after
    }
}
impl ScoreEvent {
    pub fn is_difficult_clear(self) -> bool {
        self.clear.lines() == 4 || self.spin.is_some_and(|spin| spin.lines() > 0)
    }
}

fn b2b_after_clear(
    b2b_before: u32,
    cleared_lines: u8,
    spin: Option<SpinEvent>,
    chain_rule: B2BChainRule,
) -> u32 {
    if cleared_lines == 0 {
        return b2b_before;
    }
    let underlying_transition = if cleared_lines == 4 || spin.is_some() {
        b2b_before.saturating_add(1)
    } else {
        0
    };
    match chain_rule {
        B2BChainRule::UnderlyingDifficultClearOnly => underlying_transition,
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_geometry::{
        layout::board64_layout::Board64Layout, placement::placement_table::PlacementTable,
    };
    use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
    use clearra_replay::{
        board::board64_state::Board64State,
        trace::{BoardAfterStep, HoldDecision, LineClearEvent, PieceDecision},
    };

    use super::*;

    #[test]
    fn score_event_extracts_perfect_clear_and_combo_state_from_step() {
        let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
        let table = PlacementTable::generate(layout, standard_tetromino_registry())
            .expect("placement table");
        let placement = table
            .placements_for(PieceKind::O)
            .find(|placement| {
                placement.rotation() == RotationState::Zero
                    && placement.x() == 0
                    && placement.y() == 0
            })
            .expect("O placement");
        let before = board_missing_o_corner(layout);
        let after_placement =
            Board64State::new(layout, before.occupied() | placement.mask()).expect("place O");
        let step = PlacementStep::new(
            0,
            PieceDecision::new(PieceKind::O, 0, 1, None, None, HoldDecision::None),
            placement,
            before,
            BoardAfterStep::new(after_placement, Board64State::empty(layout)),
            LineClearEvent::new(2),
        );

        let event =
            ScoreEvent::from_step(step, SpinRuleId::TSpinSimple, ComboState::default(), false);

        assert_eq!(event.clear().lines(), 2);
        assert!(event.clear().is_perfect_clear());
        assert_eq!(event.combo_after().combo(), 1);
        assert!(!event.b2b_after());
    }

    fn board_missing_o_corner(layout: Board64Layout) -> Board64State {
        let occupied = (0..2)
            .flat_map(|y| (0..10).map(move |x| (x, y)))
            .filter(|(x, _)| *x >= 2)
            .map(|(x, y)| y * 10 + x)
            .fold(0_u64, |mask, bit| mask | (1_u64 << bit));
        Board64State::new(layout, occupied).expect("board")
    }
}
