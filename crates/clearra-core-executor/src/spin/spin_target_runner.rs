use clearra_coverage::{
    matrix::spin_coverage_matrix::SpinCoverageMatrix,
    universe::{PatternUniverseId, PatternWeightModelId},
};
use clearra_scoring::{
    profile::ScoreProfile,
    spin::{SpinAccuracy, SpinClassifier, SpinTarget, SpinTargetPredicate, TraceCompleteness},
};

use crate::spin::{
    build_variant_mapper::BuildVariantMapper,
    build_variant_replay_evidence::BuildVariantReplayEvidence,
    spin_input_from_replay::spin_input_from_replay,
    spin_target_coverage_bridge::SpinTargetCoverageBridge,
    spin_target_execution_report::SpinTargetExecutionReport,
    spin_target_result_reducer::SpinTargetResultReducer,
    spin_target_runner_error::SpinTargetRunnerError, spin_target_threshold::threshold_satisfied,
    SpinProbabilityResult,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SpinTargetRunResult {
    coverage_matrix: SpinCoverageMatrix,
    probability_result: SpinProbabilityResult,
    execution_report: SpinTargetExecutionReport,
    threshold_satisfied: Option<bool>,
}

impl SpinTargetRunResult {
    pub fn new(
        coverage_matrix: SpinCoverageMatrix,
        probability_result: SpinProbabilityResult,
        execution_report: SpinTargetExecutionReport,
        threshold_satisfied: Option<bool>,
    ) -> Self {
        Self {
            coverage_matrix,
            probability_result,
            execution_report,
            threshold_satisfied,
        }
    }
}
impl SpinTargetRunResult {
    pub fn coverage_matrix(&self) -> &SpinCoverageMatrix {
        &self.coverage_matrix
    }
}
impl SpinTargetRunResult {
    pub fn probability_result(&self) -> &SpinProbabilityResult {
        &self.probability_result
    }
}
impl SpinTargetRunResult {
    pub fn execution_report(&self) -> &SpinTargetExecutionReport {
        &self.execution_report
    }
}
impl SpinTargetRunResult {
    pub fn threshold_satisfied(&self) -> Option<bool> {
        self.threshold_satisfied
    }
}

pub struct SpinTargetRunner;

impl SpinTargetRunner {
    pub fn run(
        spin_target: &SpinTarget,
        build_variants: &[BuildVariantReplayEvidence],
        classifier: Option<&dyn SpinClassifier>,
        score_profile: &ScoreProfile,
        piece_source_id: u64,
        pattern_count: usize,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
    ) -> Result<SpinTargetRunResult, SpinTargetRunnerError> {
        let classifier = classifier.ok_or(SpinTargetRunnerError::MissingSpinClassifier)?;
        let predicate = SpinTargetPredicate::new(spin_target.clone());
        let mut matrix = SpinCoverageMatrix::new(
            spin_target.id().clone(),
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
        );
        let mut evaluated = 0_usize;
        let mut satisfied = 0_usize;
        let mut exact = true;
        let mut probability_complete = true;
        let mut trace_completeness = TraceCompleteness::Full;
        let mut diagnostic_code = None;

        for replay_evidence in build_variants {
            if replay_evidence.operations().is_empty() {
                return Err(SpinTargetRunnerError::MissingSpinBasis);
            }
            let variant = replay_evidence.variant();
            let replay = BuildVariantMapper::to_replay_trace(variant, replay_evidence)
                .map_err(SpinTargetRunnerError::Replay)?;
            let input =
                spin_input_from_replay(&replay).ok_or(SpinTargetRunnerError::MissingSpinBasis)?;
            let input_trace_completeness = input.trace_completeness;
            let classification = classifier.classify(input, score_profile);
            let spin_accuracy = classification.result().accuracy();
            exact &= spin_accuracy.is_exact();
            if input_trace_completeness == TraceCompleteness::MissingKickEvidence
                || spin_accuracy == SpinAccuracy::KickSensitiveUnavailable
            {
                probability_complete = false;
                exact = false;
                trace_completeness = TraceCompleteness::MissingKickEvidence;
                diagnostic_code = Some("W_SPIN_TARGET_PROBABILITY_INCOMPLETE");
            } else if input_trace_completeness != TraceCompleteness::Full {
                probability_complete = false;
                exact = false;
                trace_completeness = input_trace_completeness;
                diagnostic_code = Some("W_SPIN_TARGET_PROBABILITY_INCOMPLETE");
            }
            evaluated += 1;

            if predicate
                .evaluate(&replay, &classification.result(), score_profile)
                .satisfied()
            {
                let row = SpinTargetCoverageBridge::row_from_build_variant(
                    spin_target.id(),
                    variant,
                    piece_source_id,
                    pattern_count,
                    pattern_universe_id,
                    pattern_weight_model_id,
                )?;
                matrix
                    .push(row)
                    .map_err(SpinTargetRunnerError::CoverageMatrix)?;
                satisfied += 1;
            }
        }

        let probability_result = SpinTargetResultReducer::reduce_uniform_with_completeness(
            &matrix,
            probability_complete,
            (!probability_complete).then(|| trace_completeness.as_str().to_owned()),
        )
        .map_err(SpinTargetRunnerError::ResultReducer)?;
        let threshold_satisfied = threshold_satisfied(spin_target, &probability_result);
        let report = SpinTargetExecutionReport::new(
            build_variants.len(),
            evaluated,
            satisfied,
            matrix.rows().len(),
            true,
            BuildVariantMapper::REPLAY_BASIS,
            probability_result.probability_complete(),
            exact,
            trace_completeness.as_str(),
            diagnostic_code,
        );

        Ok(SpinTargetRunResult::new(
            matrix,
            probability_result,
            report,
            threshold_satisfied,
        ))
    }
}

#[cfg(test)]
#[path = "spin_target_runner_tests.rs"]
mod spin_target_runner_tests;
