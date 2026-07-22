use clearra_core_executor::CoreExecutor;
use clearra_pc_graph::request::PcScenarioQuery;
use clearra_problem::ProblemCompiler;
use clearra_scoring::profile::ScoreProfile;

use super::{
    post_pc_error_reason::scenario_error_reason, post_pc_evaluation::PostPcEvaluation,
    post_pc_evaluation_summary::PostPcEvaluationSummary,
    post_pc_scenario_input::PostPcScenarioInput, post_pc_score_evaluator::PostPcScoreEvaluator,
    post_pc_score_summary::PostPcScoreSummary,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PostPcEvaluator;

impl PostPcEvaluator {
    pub fn evaluate_input(input: PostPcScenarioInput) -> PostPcEvaluation {
        Self::evaluate_query(&input.into_query())
    }
}
impl PostPcEvaluator {
    pub fn evaluate_input_with_score_profile(
        input: PostPcScenarioInput,
        profile: &ScoreProfile,
    ) -> PostPcEvaluation {
        Self::evaluate_query_with_score_profile(&input.into_query(), profile)
    }
}
impl PostPcEvaluator {
    pub fn evaluate_query(query: &PcScenarioQuery) -> PostPcEvaluation {
        Self::evaluate_query_internal(query, None)
    }
}
impl PostPcEvaluator {
    pub fn evaluate_query_with_score_profile(
        query: &PcScenarioQuery,
        profile: &ScoreProfile,
    ) -> PostPcEvaluation {
        Self::evaluate_query_internal(query, Some(profile))
    }
}
impl PostPcEvaluator {
    fn evaluate_query_internal(
        query: &PcScenarioQuery,
        profile: Option<&ScoreProfile>,
    ) -> PostPcEvaluation {
        if let Some(reason) = preflight_unsupported_reason(query) {
            return PostPcEvaluation::Unsupported { reason };
        }

        let problem = match ProblemCompiler::compile_scenario_pc(query) {
            Ok(problem) => problem,
            Err(_) => {
                return PostPcEvaluation::Unsupported {
                    reason: "scenario PC problem compilation unsupported",
                };
            }
        };
        let result = match CoreExecutor::execute(&problem) {
            Ok(result) => result,
            Err(error) => {
                return PostPcEvaluation::Unsupported {
                    reason: scenario_error_reason(error),
                };
            }
        };
        let score = profile.map_or_else(PostPcScoreSummary::none, |profile| {
            PostPcScoreEvaluator::score_retained_traces(&result, profile)
        });

        PostPcEvaluation::Evaluated(PostPcEvaluationSummary::from_query_result(
            query, &result, score,
        ))
    }
}

fn preflight_unsupported_reason(query: &PcScenarioQuery) -> Option<&'static str> {
    if query.remaining_queue().is_empty() {
        return Some("empty scenario queue");
    }
    if query.remaining_queue().observed_queue().is_some() {
        return Some("observed scenario queues must be expanded before post-PC evaluation");
    }
    if query.remaining_queue().as_bag_aligned_pattern().is_some() {
        return Some("bag-aligned scenario patterns must be expanded before post-PC evaluation");
    }
    None
}

#[cfg(all(test, feature = "native-c-core"))]
#[path = "post_pc_evaluator_tests.rs"]
mod tests;
