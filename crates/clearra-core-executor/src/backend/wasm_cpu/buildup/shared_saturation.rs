//! Native worker bridge for an exact, request-bound shared completion proof.
//! The immutable consumer scope is checked once per branch. Owner switches
//! occur only at candidate boundaries after the previous result was accepted.

use super::super::parallel_coverage::SharedCoverage;
use super::*;

impl BuildUpWorkspace {
    pub fn shared_saturation_request_eligible(
        problem: &SearchProblem,
        exact_candidate_coverage: bool,
    ) -> bool {
        !exact_candidate_coverage
            && problem.output_policy() == clearra_problem::SearchOutputPolicy::Summary
            && problem.count_policy() == PcCountPolicy::CountUnique
            && !problem.solution_probability_policy().requested()
            && !problem
                .pc_chance_evidence_policy()
                .retains_pc_coverage_evidence()
            && !problem.objective().execution_constraints().requested()
            && !problem
                .queue_observation_policy()
                .requires_observation_policy()
            && super::super::uses_pc_placement_dependency(problem)
    }

    fn shared_saturation_owner_enabled(&self) -> bool {
        #[cfg(any(test, feature = "local-search-ab"))]
        if self.operation_mask_experiment_active() {
            return false;
        }
        self.standard_bag_coverage.as_ref().is_some_and(|owner| {
            owner.boolean_membership_enabled() && owner.shared_saturation_enabled()
        })
    }

    /// Boolean-only authority; it never changes the local coverage query or
    /// the minimum-original-ordinal capability used by Trace consumers.
    pub fn shared_boolean_saturation_available(&self) -> bool {
        self.shared_saturation_owner_enabled()
            && self.standard_bag_coverage.as_ref().is_some_and(|owner| {
                owner.has_shared_boolean_saturation() && owner.can_use_boolean_pattern_membership()
            })
    }

    /// The caller has checked the immutable Summary scope and has no pending
    /// candidate roots. Disabled owners do not even inspect the shared latch.
    pub fn adopt_shared_standard_bag_saturation(
        &mut self,
        source: &SharedCoverage,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        if !self.shared_saturation_owner_enabled() {
            return Ok(false);
        }
        let owner = self.standard_bag_coverage.as_mut().expect("enabled owner");
        if owner.has_shared_boolean_saturation() || owner.global_is_complete() {
            return Ok(false);
        }
        let Some(proof) = source.standard_bag_complete_proof() else {
            return Ok(false);
        };
        if !self
            .shared_standard_bag_request
            .as_ref()
            .is_some_and(|request| Arc::ptr_eq(request, proof.request()))
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_shared_saturation_request_mismatch",
            ));
        }
        owner.adopt_shared_boolean_saturation(&proof, control)
    }

    /// Invoked only after capture/materialization, local union, and aggregate
    /// acceptance. Each eligible local owner publishes its exact Ω once.
    pub fn publish_early_standard_bag_saturation(
        &mut self,
        destination: &SharedCoverage,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        if !self.shared_saturation_owner_enabled() || self.shared_standard_bag_request.is_none() {
            return Ok(false);
        }
        let owner = self.standard_bag_coverage.as_mut().expect("enabled owner");
        if !owner.needs_early_shared_publication() {
            return Ok(false);
        }
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let span = SearchStageSpan::begin(ExecutorSearchStage::WasmSharedCoveragePublish);
        let (count, words) = owner.flushed_global_words()?;
        destination.union_original_words_for_request(
            self.shared_standard_bag_request.as_ref(),
            count,
            words,
        )?;
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        owner.record_early_shared_publication();
        span.finish(1);
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
