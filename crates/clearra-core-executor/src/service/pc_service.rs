use clearra_core_domain::execution_cancellation::{ExecutionCancellationToken, ExecutionControl};
use clearra_problem::{SearchProblem, SearchProblemPreset};

use crate::{
    buildup::{BuildUpRunner, BuildUpRunnerError},
    core_execution_result::CoreExecutionResult,
    packing::{PackingRunner, PackingRunnerError},
    performance::{ExecutorSearchStage, SearchStageSpan},
    service::pc_output_model_builder::{render_opening, render_scenario},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcServiceError {
    UnsupportedPreset,
    Packing(PackingRunnerError),
    BuildUp(BuildUpRunnerError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcService;

impl PcService {
    pub fn execute(problem: &SearchProblem) -> Result<CoreExecutionResult, PcServiceError> {
        Self::execute_with_cancellation(problem, &ExecutionCancellationToken::new())
    }

    pub fn execute_with_cancellation(
        problem: &SearchProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<CoreExecutionResult, PcServiceError> {
        Self::execute_with_control(problem, &ExecutionControl::new(cancellation.clone()))
    }

    pub fn execute_with_control(
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, PcServiceError> {
        if !matches!(
            problem.preset(),
            SearchProblemPreset::OpeningPc | SearchProblemPreset::ScenarioPc
        ) {
            return Err(PcServiceError::UnsupportedPreset);
        }
        let packing_span = SearchStageSpan::begin(ExecutorSearchStage::PcPacking);
        let packing =
            PackingRunner::run_with_control(problem, control).map_err(PcServiceError::Packing)?;
        packing_span.finish(packing.candidate_count() as u64);

        let buildup_span = SearchStageSpan::begin(ExecutorSearchStage::PcBuildUp);
        let buildup = BuildUpRunner::run_with_control(problem, &packing, control)
            .map_err(PcServiceError::BuildUp)?;
        buildup_span.finish(buildup.build_variant_count() as u64);

        let output_span = SearchStageSpan::begin(ExecutorSearchStage::PcOutput);
        let result = match problem.preset() {
            SearchProblemPreset::OpeningPc => Ok(render_opening(problem, &packing, &buildup)),
            SearchProblemPreset::ScenarioPc => Ok(render_scenario(problem, &packing, &buildup)),
            SearchProblemPreset::Setup | SearchProblemPreset::Build => {
                Err(PcServiceError::UnsupportedPreset)
            }
        };
        output_span.finish(1);
        result
    }
}
