use clearra_pc_graph::request::{OpeningPcSearchQuery, PcContinuationToken, PcScenarioQuery};

use crate::{
    compile::compile_error::ProblemCompileError,
    preset::{
        BuildPreset, ContinuationPreset, OpeningPreset, OpeningPresetError, ScenarioPreset,
        SetupPostPcPreset,
    },
    query::{BuildQuery, PcQuery, SetupSearchQuery},
    search_problem::{SearchOutputPolicy, SearchProblem, SearchProblemPreset},
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
    /// Compiles the canonical `pc.tiling` opening preset with a distinct output
    /// policy. App remains the authority for validating the closed product
    /// request; this compiler prevents the generic tiling objective from
    /// acquiring the typed result identity by coincidence.
    pub fn compile_opening_pc_tiling(
        query: &OpeningPcSearchQuery,
    ) -> Result<SearchProblem, ProblemCompileError> {
        Self::compile_opening_pc(query)
            .map(|problem| problem.with_output_policy(SearchOutputPolicy::TilingOnly))
    }

    /// Compiles the closed terminal-supply evidence producer used by both
    /// save products. Generic PC Trace requests never acquire this marker.
    pub fn compile_opening_pc_save(
        query: &OpeningPcSearchQuery,
    ) -> Result<SearchProblem, ProblemCompileError> {
        Self::compile_opening_pc(query).map(SearchProblem::with_pc_save_groups_v2_evidence)
    }

    /// Scenario counterpart to `compile_opening_pc_tiling`.
    pub fn compile_scenario_pc_tiling(
        query: &PcScenarioQuery,
    ) -> Result<SearchProblem, ProblemCompileError> {
        Self::compile_scenario_pc(query)
            .map(|problem| problem.with_output_policy(SearchOutputPolicy::TilingOnly))
    }

    pub fn compile_scenario_pc_save(
        query: &PcScenarioQuery,
    ) -> Result<SearchProblem, ProblemCompileError> {
        Self::compile_scenario_pc(query).map(SearchProblem::with_pc_save_groups_v2_evidence)
    }
}
impl ProblemCompiler {
    pub fn compile_scenario_pc(
        query: &PcScenarioQuery,
    ) -> Result<SearchProblem, ProblemCompileError> {
        let query = normalize_standard_pc_initial_board(query);
        SearchProblem::new(
            SearchProblemPreset::ScenarioPc,
            ScenarioPreset::from_query(query).into_scenario_query(),
        )
    }
}
fn normalize_standard_pc_initial_board(query: &PcScenarioQuery) -> PcScenarioQuery {
    query
        .clone()
        .with_initial_board(query.initial_board().after_initial_line_clear())
}
impl ProblemCompiler {
    pub fn compile_scenario_percent(
        query: &PcScenarioQuery,
    ) -> Result<SearchProblem, ProblemCompileError> {
        Self::compile_scenario_pc(query)
            .map(|problem| problem.with_output_policy(SearchOutputPolicy::CoverageSummary))
    }

    pub fn compile_opening_percent(
        query: &OpeningPcSearchQuery,
    ) -> Result<SearchProblem, ProblemCompileError> {
        Self::compile_opening_pc(query)
            .map(|problem| problem.with_output_policy(SearchOutputPolicy::CoverageSummary))
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
