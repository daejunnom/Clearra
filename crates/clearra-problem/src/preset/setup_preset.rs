use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};

use crate::query::{ScenarioQuery, SetupQueueInput, SetupSearchQuery};

#[derive(Clone, Debug, PartialEq)]
pub struct SetupPreset {
    query: SetupSearchQuery,
}

impl SetupPreset {
    pub fn from_query(query: SetupSearchQuery) -> Self {
        Self { query }
    }
}
impl SetupPreset {
    pub fn query(&self) -> &SetupSearchQuery {
        &self.query
    }
}
impl SetupPreset {
    pub fn into_scenario_query(self) -> ScenarioQuery {
        let queue = match self.query.queue().clone() {
            SetupQueueInput::FixedSequence(sequence) => PcQueueInput::fixed_sequence(sequence),
            SetupQueueInput::BagAlignedPattern(pattern) => {
                PcQueueInput::bag_aligned_pattern(pattern)
            }
            SetupQueueInput::Observed(queue) => PcQueueInput::observed(queue),
        };
        let board = PcScenarioBoard::new(
            self.query.board_size().width(),
            self.query.target().lines().into(),
            0,
        );
        let core_query = PcScenarioQuery::new(
            board,
            queue,
            PieceWindow::new(self.query.piece_budget().max_piece_count() as usize),
        )
        .with_hold_piece(self.query.hold_policy().initial_piece())
        .with_allow_hold(self.query.hold_policy().is_enabled())
        .with_retained_trace_limit(self.query.limits().post_pc_retained_trace_limit());

        ScenarioQuery::setup_preset(core_query, self.query)
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;
    use crate::query::{SetupHoldPolicy, SetupQueueInput};

    #[test]
    fn setup_preset_lowers_to_scenario_shaped_problem_input() {
        let setup = SetupSearchQuery::default()
            .with_queue(SetupQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
            ])))
            .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::T));
        let scenario = SetupPreset::from_query(setup).into_scenario_query();

        assert_eq!(scenario.source().as_str(), "setup-preset");
        assert_eq!(scenario.goal().as_str(), "clear-to-empty");
        assert_eq!(scenario.initial_board().width(), 10);
        assert_eq!(
            scenario.initial_board().visible_height(),
            u16::from(PcTarget::two_lines().lines())
        );
        assert_eq!(scenario.piece_window().max_pieces(), 7);
        assert_eq!(
            scenario.core_query().hold_state().piece(),
            Some(PieceKind::T)
        );
    }
}
