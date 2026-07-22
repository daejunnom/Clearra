use clearra_pc_graph::request::{PcScenarioBoard, PcScenarioQuery, PieceWindow};

use crate::query::{PcQuery, ScenarioQuery};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpeningPresetError {
    UnsupportedTarget { lines: u8 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpeningPreset {
    query: PcQuery,
}

impl OpeningPreset {
    pub fn try_from_pc_query(query: PcQuery) -> Result<Self, OpeningPresetError> {
        if matches!(query.target().lines(), 2 | 4 | 6) {
            Ok(Self { query })
        } else {
            Err(OpeningPresetError::UnsupportedTarget {
                lines: query.target().lines(),
            })
        }
    }
}
impl OpeningPreset {
    pub fn query(&self) -> &PcQuery {
        &self.query
    }
}
impl OpeningPreset {
    pub fn into_scenario_query(self) -> ScenarioQuery {
        let exact_pieces = self.query.exact_piece_count();
        let mut core_query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(self.query.target().lines().into(), 0),
            self.query.queue().clone(),
            PieceWindow::new(exact_pieces),
        )
        .with_hold_piece(self.query.hold_policy().initial_piece())
        .with_exact_pieces(Some(exact_pieces))
        .with_allow_hold(self.query.hold_policy().is_enabled())
        .with_rule(self.query.rule())
        .with_count_policy(self.query.count_policy())
        .with_objective(self.query.objective())
        .with_solution_probability_policy(self.query.solution_probability_policy())
        .with_execution_policy(self.query.execution_policy().clone());

        if let Some(supply_window_size) = self.query.supply_window_size() {
            core_query = core_query.with_supply_window_size(supply_window_size);
        }

        if let Some(profile) = self.query.verified_kick_profile() {
            core_query = core_query.with_verified_kick_table_profile(profile.clone());
        }

        ScenarioQuery::opening_preset(
            core_query,
            self.query.target(),
            self.query.opening_labels(),
            self.query,
        )
    }
}
