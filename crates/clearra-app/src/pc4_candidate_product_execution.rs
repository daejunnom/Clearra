//! SRP rationale: transfers a complete online PC4 candidate universe into the
//! ordinary App product finalizer without giving a transport its own reducer.
//!
//! The shared search preparation compiles and validates the typed request; it
//! does not start distributed workers or enumerate Geometry. Core verifies the
//! supplied candidates and App retains its existing cooperative minimum/replay
//! finalization. Transport, consent, fallback and dataset qualification remain
//! outside this module. The borrowed compatibility entrypoint rejects finite
//! memory and typed score/tiling terminals. Typed scores use the owned child
//! handoff and retain the existing request-level terminal memory authority;
//! other finite-memory product handoffs remain explicitly unsupported.

use clearra_core_domain::execution_cancellation::ExecutionControl;

#[path = "pc4_score_candidate_product_execution.rs"]
mod score;

use super::{
    AppContext, AppRequest, AppResponse, CooperativeSearchResponseKind,
    DistributedSearchPreparation, PreparedDistributedSearch,
};
use crate::{
    app_command::AppCommand,
    pc4_search_problem_compatibility::validate_pc4_search_problem_compatibility,
    pc_candidate_execution_bridge::{
        execute_validated_pc_candidate_input, PcCandidateExecutionError,
        ValidatedPcCandidateExecutionEvidence,
    },
    pc_candidate_page_boundary::{
        PcCandidateBoundaryError, PcCandidatePageGuard, PcCandidateReducerInput,
        PcCandidateUniverseIdentity,
    },
    CooperativeAppAdvance, CooperativeAppExecution, Pc4SearchProblemCompatibilityError,
    PcResultProjection,
};

#[derive(Debug)]
pub enum Pc4CandidateProductError {
    RequestRejected(Box<AppResponse>),
    UnsupportedProduct,
    FiniteMemoryAuthorityRequired,
    Source(PcCandidateBoundaryError),
    Compatibility(Pc4SearchProblemCompatibilityError),
    Candidate(PcCandidateExecutionError),
    Terminal(&'static str),
    AlreadyFinished,
}

impl Pc4CandidateProductError {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::RequestRejected(_) => "pc4_candidate_product_request_rejected",
            Self::UnsupportedProduct => "pc4_candidate_product_not_supported",
            Self::FiniteMemoryAuthorityRequired => {
                "pc4_candidate_product_finite_memory_handoff_required"
            }
            Self::Source(error) => error.reason(),
            Self::Compatibility(error) => error.reason(),
            Self::Candidate(error) => error.reason(),
            Self::Terminal(reason) => reason,
            Self::AlreadyFinished => "pc4_candidate_product_already_finished",
        }
    }
}

impl core::fmt::Display for Pc4CandidateProductError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4CandidateProductError {}

/// An ordinary App finalizer plus the exact online source it may expose.
/// Freshness is sampled before and after every advance, including completion;
/// a stale source cannot be laundered by a long minimum-cover proof.
pub struct Pc4CandidateProductExecution {
    execution: Option<Pc4CandidateProductState>,
    evidence: ValidatedPcCandidateExecutionEvidence,
}

// One result owner exists at a time; score summaries are already finalized,
// while minimum/replay products retain the ordinary resumable cursor.
#[allow(clippy::large_enum_variant)]
enum Pc4CandidateProductState {
    Cooperative(CooperativeAppExecution),
    Ready(AppResponse),
}

impl AppContext {
    /// Starts product reduction only from source-sealed, complete candidates.
    /// No online I/O or implicit offline fallback occurs here. The supplied
    /// input may serve multiple objectives, but never a different field,
    /// queue/hold, target or rule profile. Rejections preserve the ordinary
    /// App validation response rather than translating it into a fake miss.
    pub fn start_pc4_candidate_product<G: PcCandidatePageGuard>(
        &self,
        request: AppRequest,
        input: &PcCandidateReducerInput,
        guard: &G,
        control: &ExecutionControl,
    ) -> Result<Pc4CandidateProductExecution, Pc4CandidateProductError> {
        check_source(input.universe_identity(), guard, control)?;
        if !matches!(
            request.command(),
            AppCommand::Pc(_) | AppCommand::Scenario(_)
        ) {
            return Err(Pc4CandidateProductError::UnsupportedProduct);
        }
        if request.resource_budget().max_memory_mib().is_some()
            || request.resource_budget().memory_mib().is_some()
        {
            return Err(Pc4CandidateProductError::FiniteMemoryAuthorityRequired);
        }
        let prepared = prepare_request(self, request)?;
        prepared.start_pc4_candidate_product(input, guard, control)
    }
}

impl PreparedDistributedSearch {
    fn start_pc4_candidate_product<G: PcCandidatePageGuard>(
        self,
        input: &PcCandidateReducerInput,
        guard: &G,
        control: &ExecutionControl,
    ) -> Result<Pc4CandidateProductExecution, Pc4CandidateProductError> {
        // Typed score and tiling results retain their session's terminal
        // authority. Do not route them through a generic unguarded result.
        let supported = match &self.response_kind {
            CooperativeSearchResponseKind::Pc(projection)
            | CooperativeSearchResponseKind::Scenario {
                result_projection: projection,
                render_contract: None,
            } => matches!(
                projection.projection(),
                PcResultProjection::Standard
                    | PcResultProjection::MinimumCoverV2(_)
                    | PcResultProjection::PathFamilyV2(_)
            ),
            CooperativeSearchResponseKind::PcChance { .. }
            | CooperativeSearchResponseKind::ScenarioChance { .. } => true,
            _ => false,
        };
        if !supported {
            return Err(Pc4CandidateProductError::UnsupportedProduct);
        }
        if self.problem.backend_policy().max_memory_mib().is_some() {
            return Err(Pc4CandidateProductError::FiniteMemoryAuthorityRequired);
        }
        check_source(input.universe_identity(), guard, control)?;
        let compatibility = validate_pc4_search_problem_compatibility(
            input.universe_identity().profile(),
            &self.problem,
        )
        .map_err(Pc4CandidateProductError::Compatibility)?;
        let (result, evidence) =
            execute_validated_pc_candidate_input(input, compatibility, &self.problem, control)
                .map_err(Pc4CandidateProductError::Candidate)?;
        check_source(evidence.universe_identity(), guard, control)?;
        let Self {
            context,
            problem,
            response_kind,
            command_kind,
            output_policy,
            resource_budget,
            validation_report,
            product_capability_contract,
        } = self;
        drop(problem);
        Ok(Pc4CandidateProductExecution {
            execution: Some(Pc4CandidateProductState::Cooperative(
                CooperativeAppExecution::from_precomputed_product_result(
                    context,
                    result,
                    response_kind,
                    command_kind,
                    output_policy,
                    validation_report,
                    resource_budget,
                    product_capability_contract,
                ),
            )),
            evidence,
        })
    }
}

impl Pc4CandidateProductExecution {
    pub const fn evidence(&self) -> &ValidatedPcCandidateExecutionEvidence {
        &self.evidence
    }

    pub fn advance<G: PcCandidatePageGuard>(
        &mut self,
        work_budget: usize,
        guard: &G,
        control: &ExecutionControl,
    ) -> Result<CooperativeAppAdvance, Pc4CandidateProductError> {
        let state = self
            .execution
            .take()
            .ok_or(Pc4CandidateProductError::AlreadyFinished)?;
        check_source(self.evidence.universe_identity(), guard, control)?;
        let (advance, next_state) = match state {
            Pc4CandidateProductState::Cooperative(mut execution) => {
                let advance = execution.advance(work_budget, control);
                let next = matches!(
                    &advance,
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress
                )
                .then_some(Pc4CandidateProductState::Cooperative(execution));
                (advance, next)
            }
            Pc4CandidateProductState::Ready(response) => {
                (CooperativeAppAdvance::Completed(response), None)
            }
        };
        check_source(self.evidence.universe_identity(), guard, control)?;
        self.execution = next_state;
        Ok(advance)
    }
}

fn prepare_request(
    context: &AppContext,
    request: AppRequest,
) -> Result<PreparedDistributedSearch, Pc4CandidateProductError> {
    let mut context = context
        .clone()
        .with_language(request.language().unwrap_or(context.language()));
    if let Some(policy) = request.file_policy() {
        context = context.with_file_policy(policy.clone());
    }
    match context.prepare_distributed_search(request) {
        DistributedSearchPreparation::Ready(response) => Err(
            Pc4CandidateProductError::RequestRejected(Box::new(response)),
        ),
        DistributedSearchPreparation::Search(prepared) => Ok(prepared),
    }
}

fn check_source<G: PcCandidatePageGuard>(
    universe: &PcCandidateUniverseIdentity,
    guard: &G,
    control: &ExecutionControl,
) -> Result<(), Pc4CandidateProductError> {
    if guard.is_cancelled() || control.is_cancelled() {
        return Err(Pc4CandidateProductError::Source(
            PcCandidateBoundaryError::Cancelled,
        ));
    }
    if control.partition().count() != 1 || control.partition().index() != 0 {
        return Err(Pc4CandidateProductError::Candidate(
            PcCandidateExecutionError::PartitionedExecutionControl,
        ));
    }
    if !guard.is_current_source(universe.source()) {
        return Err(Pc4CandidateProductError::Source(
            PcCandidateBoundaryError::StaleSession,
        ));
    }
    let snapshot = universe
        .qualified_snapshot()
        .ok_or(Pc4CandidateProductError::Candidate(
            PcCandidateExecutionError::QualifiedOnlineSourceRequired,
        ))?;
    if !guard.is_current_snapshot(snapshot) {
        return Err(Pc4CandidateProductError::Source(
            PcCandidateBoundaryError::StaleSnapshot,
        ));
    }
    Ok(())
}
