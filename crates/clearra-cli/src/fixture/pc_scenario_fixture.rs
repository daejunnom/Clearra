use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use super::{
    external_pc_fixture_materializer::ExternalPcFixtureMaterializer,
    pc_scenario_fixture_expected::ScenarioFixtureExpected,
    pc_scenario_fixture_field::{ScenarioFixtureInput, ScenarioFixtureSource},
    pc_scenario_fixture_loader::read_fixture_json,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PcScenarioFixture {
    name: String,
    source: ScenarioFixtureSource,
    scenario: ScenarioFixtureInput,
    expected: ScenarioFixtureExpected,
}

impl PcScenarioFixture {
    pub fn read(path: impl AsRef<Path>) -> Result<Self, String> {
        let contents = read_fixture_json(path.as_ref())?;
        match serde_json::from_str(&contents) {
            Ok(fixture) => Ok(fixture),
            Err(error) => {
                let standard_error = error.to_string();
                let value =
                    serde_json::from_str::<Value>(&contents).map_err(|_| standard_error.clone())?;
                if value.get("kind").and_then(Value::as_str) != Some("external-pc-worker-fixture") {
                    return Err(standard_error);
                }
                let materialized =
                    ExternalPcFixtureMaterializer::materialize(path.as_ref(), value)?;
                serde_json::from_value(materialized).map_err(|error| error.to_string())
            }
        }
    }
}
impl PcScenarioFixture {
    pub fn name(&self) -> &str {
        &self.name
    }
}
impl PcScenarioFixture {
    pub fn scenario(&self) -> &ScenarioFixtureInput {
        &self.scenario
    }
}
impl PcScenarioFixture {
    pub fn expected(&self) -> &ScenarioFixtureExpected {
        &self.expected
    }
}
impl PcScenarioFixture {
    pub fn source_fields(&self) -> Vec<(String, String)> {
        self.source.source_fields()
    }
}

#[cfg(test)]
#[path = "pc_scenario_fixture_tests.rs"]
mod tests;
