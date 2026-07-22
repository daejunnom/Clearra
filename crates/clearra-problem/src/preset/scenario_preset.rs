use clearra_pc_graph::request::PcScenarioQuery;

use crate::query::ScenarioQuery;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioPreset {
    query: PcScenarioQuery,
}

impl ScenarioPreset {
    pub fn from_query(query: PcScenarioQuery) -> Self {
        Self { query }
    }
}
impl ScenarioPreset {
    pub fn into_scenario_query(self) -> ScenarioQuery {
        ScenarioQuery::scenario_preset(self.query)
    }
}
