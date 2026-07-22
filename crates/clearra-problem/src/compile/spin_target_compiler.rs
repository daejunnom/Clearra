use crate::{
    compile::{compile_error::ProblemCompileError, problem_compiler::ProblemCompiler},
    goal::{
        search_goal_request::{CompositeGoal, SearchGoal},
        spin_target_requires_score_profile, SpinTargetRequest,
    },
    query::{SpinTargetBaseQuery, SpinTargetQuery},
    search_problem::SearchProblem,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpinTargetCompiler;

impl SpinTargetCompiler {
    pub fn compile(query: &SpinTargetQuery) -> Result<SearchProblem, ProblemCompileError> {
        Self::validate_spin_target(query.spin_target())?;

        let problem = match query.base_query() {
            SpinTargetBaseQuery::Percent(base_query) => {
                ProblemCompiler::compile_scenario_pc(base_query)?
                    .with_search_goal(SearchGoal::SpinTarget(query.spin_target().clone()))
            }
            SpinTargetBaseQuery::Setup(base_query) => ProblemCompiler::compile_setup(base_query)?
                .with_search_goal(SearchGoal::SpinTarget(query.spin_target().clone())),
            SpinTargetBaseQuery::PcThenSpin(base_query) => ProblemCompiler::compile_opening_pc(
                base_query,
            )?
            .with_search_goal(SearchGoal::Composite(CompositeGoal::clear_then_spin(
                query.spin_target().clone(),
            ))),
        };

        Ok(problem)
    }
}
impl SpinTargetCompiler {
    fn validate_spin_target(spin_target: &SpinTargetRequest) -> Result<(), ProblemCompileError> {
        if spin_target_requires_score_profile(spin_target)
            && spin_target.required_score_profile().is_none()
        {
            return Err(ProblemCompileError::ProfileSpecificSpinTargetRequiresScoreProfile);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "spin_target_compiler_tests.rs"]
mod tests;
