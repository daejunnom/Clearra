//! SRP rationale: owned PC4 candidates cross the existing typed-score terminal
//! lease without creating a second score or score-minimum implementation.

use clearra_core_executor::{CoreExecutionError, WasmCpuSearchBackend, WasmCpuSearchSession};

use super::{
    check_source, prepare_request, AppCommand, AppContext, AppRequest,
    CooperativeSearchResponseKind, ExecutionControl, Pc4CandidateProductError,
    Pc4CandidateProductExecution, Pc4CandidateProductState, PcCandidateExecutionError,
    PcCandidatePageGuard, PcCandidateReducerInput, PreparedDistributedSearch,
};
use crate::{
    cooperative_execution::CooperativePcScoreProduct,
    pc_candidate_execution_bridge::validate_pc_candidate_input,
    validate_pc4_search_problem_compatibility,
};

impl AppContext {
    /// Consumes the candidate universe before typed score execution. A caller
    /// with a live Range session should first use its consuming completion
    /// handoff, releasing graph/replay caches before this product admission.
    /// Other supported products reuse the compatibility entrypoint unchanged.
    pub fn start_pc4_owned_candidate_product<G: PcCandidatePageGuard>(
        &self,
        request: AppRequest,
        input: PcCandidateReducerInput,
        guard: &G,
        control: &ExecutionControl,
    ) -> Result<Pc4CandidateProductExecution, Pc4CandidateProductError> {
        let projection = match request.command() {
            AppCommand::Pc(command) => command.result_projection(),
            AppCommand::Scenario(command) => command.result_projection(),
            _ => return Err(Pc4CandidateProductError::UnsupportedProduct),
        };
        if projection.score_origin().is_none() && projection.score_minimals_origin().is_none() {
            return self.start_pc4_candidate_product(request, &input, guard, control);
        }
        check_source(input.universe_identity(), guard, control)?;
        let prepared = prepare_request(self, request)?;
        prepared.start_pc4_owned_score(input, guard, control)
    }
}

impl PreparedDistributedSearch {
    fn start_pc4_owned_score<G: PcCandidatePageGuard>(
        self,
        input: PcCandidateReducerInput,
        guard: &G,
        control: &ExecutionControl,
    ) -> Result<Pc4CandidateProductExecution, Pc4CandidateProductError> {
        let portfolio = match &self.response_kind {
            CooperativeSearchResponseKind::PcScore { product, .. }
            | CooperativeSearchResponseKind::ScenarioScore { product, .. } => {
                *product == CooperativePcScoreProduct::Portfolio
            }
            _ => return Err(Pc4CandidateProductError::UnsupportedProduct),
        };
        let request_cap = [
            self.resource_budget.memory_mib(),
            self.resource_budget.max_memory_mib().map(u64::from),
        ]
        .into_iter()
        .flatten()
        .min();
        if request_cap.is_some_and(|cap| {
            self.problem
                .backend_policy()
                .max_memory_mib()
                .is_none_or(|compiled| compiled > cap)
        }) {
            return Err(Pc4CandidateProductError::Terminal(
                "pc4_score_request_memory_limit_binding_mismatch",
            ));
        }
        check_source(input.universe_identity(), guard, control)?;
        let compatibility = validate_pc4_search_problem_compatibility(
            input.universe_identity().profile(),
            &self.problem,
        )
        .map_err(Pc4CandidateProductError::Compatibility)?;
        let evidence = validate_pc_candidate_input(&input, compatibility, &self.problem, control)
            .map_err(Pc4CandidateProductError::Candidate)?;
        let external_bytes = input
            .checked_retained_capacity_bytes()
            .and_then(|bytes| bytes.checked_add(evidence.checked_retained_capacity_bytes()?))
            .and_then(|bytes| {
                bytes.checked_add(core::mem::size_of::<Pc4CandidateProductExecution>() as u128)
            })
            .and_then(|bytes| {
                bytes.checked_add((2 * core::mem::size_of::<WasmCpuSearchSession>()) as u128)
            })
            .ok_or(Pc4CandidateProductError::Terminal(
                "pc4_score_candidate_owner_projection_unavailable",
            ))?;
        let (result, session) = {
            let (parent, checked_external_bound) = self
                .pc_score_terminal_resource_authority_with_external_bytes(external_bytes)
                .map_err(Pc4CandidateProductError::Terminal)?
                .ok_or(Pc4CandidateProductError::Terminal(
                    "pc4_score_candidate_parent_authority_missing",
                ))?;
            WasmCpuSearchBackend::execute_complete_precomputed_candidates_under_authority(
                self.problem_arc(),
                input.candidates(),
                checked_external_bound,
                parent,
                control,
            )
            .map_err(|error| {
                Pc4CandidateProductError::Candidate(PcCandidateExecutionError::Core(error))
            })?
        };
        check_source(evidence.universe_identity(), guard, control)?;
        // The existing score pipeline owns exact replay scoring and portfolio
        // preparation. Every temporary/future allocation is checked against
        // the still-live verifier, including its pre-reserved PC4 owners.
        let completion =
            self.complete_pc_score_with_memory_guard(result, control, |result, future| {
                check_source(evidence.universe_identity(), guard, control).map_err(|error| {
                    CoreExecutionError::RuntimeUnavailable {
                        component: error.reason(),
                    }
                })?;
                session
                    .validate_public_result_memory_with_future(result, future)
                    .map_err(|error| error.into_core_execution_error())
            });
        // Retention is deliberately separated from rich response creation.
        // A summary is ready; a score-minimum still uses the shared lazy-first
        // exact portfolio cursor after the compute child has been dropped.
        drop(session);
        drop(input);
        check_source(evidence.universe_identity(), guard, control)?;
        let execution = if portfolio {
            Pc4CandidateProductState::Cooperative(
                completion
                    .into_cooperative_product_completion()
                    .map_err(Pc4CandidateProductError::Terminal)?,
            )
        } else {
            Pc4CandidateProductState::Ready(completion.complete())
        };
        check_source(evidence.universe_identity(), guard, control)?;
        Ok(Pc4CandidateProductExecution {
            execution: Some(execution),
            evidence,
        })
    }
}
