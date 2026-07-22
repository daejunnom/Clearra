use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};

use crate::query::{BuildQuery, ScenarioQuery};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildPreset {
    query: BuildQuery,
}

impl BuildPreset {
    pub fn from_query(query: BuildQuery) -> Self {
        Self { query }
    }
}
impl BuildPreset {
    pub fn query(&self) -> &BuildQuery {
        &self.query
    }
}
impl BuildPreset {
    pub fn into_scenario_query(self) -> ScenarioQuery {
        let board_size = self.query.template().board_size();
        let core_query = PcScenarioQuery::new(
            PcScenarioBoard::new(board_size.width(), board_size.height(), 0),
            PcQueueInput::default(),
            PieceWindow::new(self.query.template().slot_count()),
        )
        .with_allow_hold(false)
        .with_retained_trace_limit(1);

        ScenarioQuery::build_preset(core_query, self.query)
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::board::board_size::BoardSize;

    use super::*;
    use crate::query::{BuildProblemLimits, BuildTemplateBridge};

    #[test]
    fn build_preset_lowers_coverage_bridge_to_search_problem_shape() {
        let query = BuildQuery::coverage_bridge(
            BuildTemplateBridge::new("template-a", BoardSize::new(10, 4).expect("board"), 2),
            8,
            BuildProblemLimits::new(16, 8),
        );
        let scenario = BuildPreset::from_query(query).into_scenario_query();

        assert_eq!(scenario.source().as_str(), "build-preset");
        assert_eq!(scenario.initial_board().width(), 10);
        assert_eq!(scenario.initial_board().visible_height(), 4);
        assert_eq!(scenario.piece_window().max_pieces(), 2);
        assert!(scenario.build_query().is_some());
    }
}
