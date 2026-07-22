use clearra_pc_graph::request::{OpeningPcSearchQuery, PcContinuationToken, PcScenarioQuery};

use crate::{
    compile::compile_error::ProblemCompileError,
    preset::{
        BuildPreset, ContinuationPreset, OpeningPreset, OpeningPresetError, ScenarioPreset,
        SetupPostPcPreset,
    },
    query::{BuildQuery, PcQuery, SetupSearchQuery},
    search_problem::{SearchProblem, SearchProblemPreset},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProblemCompiler;

impl ProblemCompiler {
    pub fn compile_opening_pc(
        query: &OpeningPcSearchQuery,
    ) -> Result<SearchProblem, ProblemCompileError> {
        let pc_query = PcQuery::from_opening_query(query);
        let preset = OpeningPreset::try_from_pc_query(pc_query).map_err(|error| match error {
            OpeningPresetError::UnsupportedTarget { lines } => {
                ProblemCompileError::UnsupportedOpeningPreset { lines }
            }
        })?;
        SearchProblem::new(SearchProblemPreset::OpeningPc, preset.into_scenario_query())
    }
}
impl ProblemCompiler {
    pub fn compile_scenario_pc(
        query: &PcScenarioQuery,
    ) -> Result<SearchProblem, ProblemCompileError> {
        SearchProblem::new(
            SearchProblemPreset::ScenarioPc,
            ScenarioPreset::from_query(query.clone()).into_scenario_query(),
        )
    }
}
impl ProblemCompiler {
    pub fn compile_setup(query: &SetupSearchQuery) -> Result<SearchProblem, ProblemCompileError> {
        SearchProblem::new(
            SearchProblemPreset::Setup,
            SetupPostPcPreset::from_query(query.clone()).into_scenario_query(),
        )
    }
}
impl ProblemCompiler {
    pub fn compile_build(query: &BuildQuery) -> Result<SearchProblem, ProblemCompileError> {
        SearchProblem::new(
            SearchProblemPreset::Build,
            BuildPreset::from_query(query.clone()).into_scenario_query(),
        )
    }
}
impl ProblemCompiler {
    pub fn compile_continuation_token(
        token: &PcContinuationToken,
    ) -> Result<SearchProblem, ProblemCompileError> {
        match ContinuationPreset::from_token(token) {
            ContinuationPreset::Opening(query) => Self::compile_opening_pc(&query),
            ContinuationPreset::Scenario(query) => Self::compile_scenario_pc(&query),
        }
    }
}

#[cfg(test)]
#[path = "problem_compiler_tests.rs"]
mod tests;
