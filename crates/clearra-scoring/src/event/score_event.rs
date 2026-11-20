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
        let b2b_after = b2b_after_clear(b2b_before, clear, spin, b2b_chain_rule);
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
        Self::from_step_with_b2b_chain(
            step,
            spin_rule,
            combo_before,
            u32::from(b2b_before),
            B2BChainRule::UnderlyingDifficultClearOnly,
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
        let clear = ClearEvent::from_placement_step(step);
        let spin = SpinDetector::detect_replay_step(trace, step, spin_rule, clear.lines());
        Self::from_classified_clear_with_b2b_chain(
            step.step_index(),
            clear,
            spin,
            combo_before,
            b2b_before,
            b2b_chain_rule,
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
        let clear = ClearEvent::from_placement_step(step);
        let spin = SpinDetector::detect(step, spin_rule, clear.lines());
        Self::from_classified_clear_with_b2b_chain(
            step.step_index(),
            clear,
            spin,
            combo_before,
            b2b_before,
            b2b_chain_rule,
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
        self.clear.is_four_line_clear() || self.spin.is_some_and(|spin| spin.lines() > 0)
    }
}

fn b2b_after_clear(
    b2b_before: u32,
    clear: ClearEvent,
    spin: Option<SpinEvent>,
    chain_rule: B2BChainRule,
) -> u32 {
    if !clear.clears_lines() {
        return b2b_before;
    }
    let underlying_transition = if clear.is_four_line_clear() || spin.is_some() {
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
        ScoringExecutionEdge, ScoringLockEvidence,
    };

    use super::*;
    use crate::{
        b2b_preservation::BackToBackPreservationPolicy,
        profile::{SpinProfile, SpinProfileId},
    };

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

    #[test]
    fn clear_fact_parity_four_lines_need_not_be_a_perfect_clear() {
        let layout = Board64Layout::standard_10_by_lines(6).unwrap();
        let table = PlacementTable::generate(layout, standard_tetromino_registry()).unwrap();
        let vertical_i = (1 << 30) | (1 << 20) | (1 << 10) | 1;
        let placement = table
            .placements_for(PieceKind::I)
            .find(|placement| placement.mask() == vertical_i)
            .expect("vertical I placement in column zero");
        let full_bottom = (1_u64 << 40) - 1;
        for survivor in [0, 1_u64 << 49] {
            let board_before =
                Board64State::new(layout, (full_bottom ^ vertical_i) | survivor).unwrap();
            let after_lock = Board64State::new(layout, full_bottom | survivor).unwrap();
            let after_clear = Board64State::new(layout, survivor >> 40).unwrap();
            let step = PlacementStep::new(
                0,
                PieceDecision::new(PieceKind::I, 0, 1, None, None, HoldDecision::None),
                placement,
                board_before,
                BoardAfterStep::new(after_lock, after_clear),
                LineClearEvent::new(4),
            );

            let event = ScoreEvent::from_step_with_b2b_chain(
                step,
                SpinRuleId::TSpinSimple,
                ComboState::default(),
                3,
                B2BChainRule::UnderlyingDifficultClearOnly,
            );
            let graph_edge = non_spin_edge(4, survivor == 0);
            assert_eq!(event.clear(), ClearEvent::from_execution_edge(graph_edge));
            assert_eq!(event.clear().is_perfect_clear(), survivor == 0);
            assert!(event.is_difficult_clear());
            assert_eq!(event.b2b_after_chain(), 4);
            assert!(!BackToBackPreservationPolicy::requires_recognized_spin(
                graph_edge
            ));
        }
    }

    #[test]
    fn clear_fact_parity_b2b_save_pc_exception_is_not_a_numeric_chain_increment() {
        let save = BackToBackPreservationPolicy::new(SpinProfile::builtin(SpinProfileId::TSpins));
        for (lines, perfect_clear, save_allowed, chain_after) in [
            (0, false, true, 3),
            (2, false, false, 0),
            (2, true, true, 0),
            (4, false, true, 4),
            (4, true, true, 4),
        ] {
            let edge = non_spin_edge(lines, perfect_clear);
            let clear = ClearEvent::from_execution_edge(edge);
            let event = ScoreEvent::from_classified_clear_with_b2b_chain(
                0,
                clear,
                None,
                ComboState::default(),
                3,
                B2BChainRule::UnderlyingDifficultClearOnly,
            );
            assert_eq!(clear, ClearEvent::new(lines, perfect_clear));
            assert_eq!(save.allows(edge), save_allowed);
            assert_eq!(event.b2b_after_chain(), chain_after);
        }
    }

    fn non_spin_edge(cleared_lines: u8, perfect_clear: bool) -> ScoringExecutionEdge {
        ScoringExecutionEdge::new(
            0,
            0,
            PieceKind::I,
            RotationState::Zero,
            0,
            0,
            cleared_lines,
            0,
            0,
            ScoringLockEvidence::no_rotation(RotationState::Zero),
        )
        .with_perfect_clear(perfect_clear)
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
