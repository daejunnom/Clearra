//! Full-height PC execution through the shared ILC/BuildUp/language primitives.
//! The typed minimum producer retains coverage proof for the common reducer;
//! score and replay authorities are not interchangeable with that family.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    objective::objective_kind::ObjectiveKind,
};
use clearra_pc_graph::request::PcCountPolicy;
use clearra_problem::{
    BuildProbabilityField, PcChanceEvidencePolicy, SearchOutputPolicy, SearchProblem,
    SearchProblemPreset,
};

use super::{
    build_probability::BuildProbabilityAdvance,
    extended_build_probability::ExtendedBuildProbabilitySession, ExactSearchAdvance,
    WasmExactSearchError,
};
use crate::{
    resource::{
        admit_budget_bound_search_execution,
        admit_budget_bound_search_execution_under_terminal_authority, ExecutionAdmission,
        WasmCpuTerminalResourceAuthority,
    },
    CoreExecutionResult,
};

pub(crate) struct ExtendedPcSearchSession {
    engine: ExtendedBuildProbabilitySession,
    _admission: ExecutionAdmission,
    terminal_authority_supplied: bool,
}

pub(super) fn validate_pc_family_problem(
    problem: &SearchProblem,
) -> Result<(), WasmExactSearchError> {
    let ordinary_source = problem.pc_chance_evidence_policy() == PcChanceEvidencePolicy::Disabled
        && !problem.objective().score().requested()
        && matches!(
            problem.objective().kind(),
            ObjectiveKind::All | ObjectiveKind::Unique
        );
    let minimum_source = problem.pc_chance_evidence_policy()
        == PcChanceEvidencePolicy::PcMinimumCoverV2
        && problem.objective().kind() == ObjectiveKind::MinimumCover
        && matches!(
            problem.count_policy(),
            PcCountPolicy::CountAll | PcCountPolicy::CountUnique
        )
        && problem.output_policy() == SearchOutputPolicy::Trace;
    let chance_source = problem.pc_chance_evidence_policy()
        == PcChanceEvidencePolicy::PcProbabilityV2
        && problem.objective().kind() == ObjectiveKind::Unique
        && problem.count_policy() == PcCountPolicy::CountUnique
        && problem.output_policy() == SearchOutputPolicy::CoverageSummary
        && !problem.solution_probability_policy().requested();
    // The canonical failed-queue command deliberately retains its existing
    // All + CountAll input contract. Its complement proof is purpose-separated
    // from chance; do not normalize the user's query into a different source.
    let failed_source = problem
        .pc_chance_evidence_policy()
        .pc_failed_queue_example_limit()
        .is_some()
        && matches!(
            (problem.objective().kind(), problem.count_policy()),
            (ObjectiveKind::All, PcCountPolicy::CountAll)
                | (ObjectiveKind::Unique, PcCountPolicy::CountUnique)
        )
        && problem.output_policy() == SearchOutputPolicy::CoverageSummary
        && !problem.solution_probability_policy().requested();
    let score_source = problem.objective().score().requested()
        && problem.count_policy() == PcCountPolicy::CountAll
        && problem.output_policy() == SearchOutputPolicy::Trace
        && match problem.pc_chance_evidence_policy() {
            PcChanceEvidencePolicy::Disabled => problem.objective().kind() == ObjectiveKind::All,
            PcChanceEvidencePolicy::PcScorePortfolioV2 => {
                problem.objective().kind() == ObjectiveKind::MinimumCover
            }
            _ => false,
        };
    let path_source = problem
        .pc_chance_evidence_policy()
        .retains_pc_path_v2_evidence()
        && problem.objective().kind() == ObjectiveKind::All
        && problem.count_policy() == PcCountPolicy::CountAll
        && problem.output_policy() == SearchOutputPolicy::Trace
        && !problem.objective().score().requested();
    if !matches!(
        problem.preset(),
        SearchProblemPreset::ScenarioPc | SearchProblemPreset::OpeningPc
    ) || (problem.preset() == SearchProblemPreset::OpeningPc
        && problem.initial_board().occupied_words() != [0; 4])
        || problem.goal().as_str() != "clear-to-empty"
        || problem.initial_board().width() != 10
        || !(7..=24).contains(&problem.visible_height())
        || !(ordinary_source
            || minimum_source
            || chance_source
            || failed_source
            || score_source
            || path_source)
        || !matches!(
            problem.count_policy(),
            PcCountPolicy::CountAll | PcCountPolicy::CountUnique
        )
        || !(matches!(
            problem.output_policy(),
            SearchOutputPolicy::Summary | SearchOutputPolicy::Trace
        ) || chance_source
            || failed_source)
        || (problem.objective().score().requested() && !score_source)
        || problem.objective().execution_constraints().requested()
        || problem.allowed_colored_solution_identities().is_some()
        || problem
            .queue_observation_policy()
            .requires_observation_policy()
        || problem.backend_policy().tablebase_requested()
        || problem.backend_policy().precompute_build_dependencies()
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "extended_pc_family_contract_not_connected",
        ));
    }
    super::ensure_connected_kick_profile(problem)
}

pub(super) fn checked_pc_family_problem_nested_retained_bytes(
    problem: &SearchProblem,
) -> Option<u128> {
    // Constructor validation owns execution compatibility. This projection
    // only counts owners and does not revalidate kicks in the solver loop.
    let pointee = if problem.objective().score().requested() {
        problem.checked_pc_score_pointee_retained_bytes()?
    } else {
        problem.checked_pc_family_pointee_retained_bytes()?
    };
    pointee.checked_sub(core::mem::size_of::<SearchProblem>() as u128)
}

impl ExtendedPcSearchSession {
    pub(super) fn failed_queue_memory_bound(&self) -> crate::resource::ExecutionMemoryBound {
        self._admission.memory_bound()
    }

    pub(super) fn checked_failed_queue_retained_bytes(&self) -> Option<u128> {
        self.engine.checked_retained_bytes_with_coexisting_owners()
    }

    pub fn new(problem: &SearchProblem) -> Result<Self, WasmExactSearchError> {
        Self::new_with_coexisting_retained_bytes(problem, 0)
    }

    pub(super) fn new_with_coexisting_retained_bytes(
        problem: &SearchProblem,
        additional_coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_admission(problem, additional_coexisting_retained_bytes, None)
    }

    pub(crate) fn new_under_authority(
        problem: &SearchProblem,
        external_retained_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmExactSearchError> {
        if !problem.objective().score().requested()
            && !problem
                .pc_chance_evidence_policy()
                .retains_pc_path_v2_evidence()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_terminal_authority_requires_score_or_path",
            ));
        }
        Self::new_with_admission(problem, external_retained_bytes, Some(authority))
    }

    fn new_with_admission(
        problem: &SearchProblem,
        additional_coexisting_retained_bytes: u128,
        authority: Option<&WasmCpuTerminalResourceAuthority>,
    ) -> Result<Self, WasmExactSearchError> {
        validate_pc_family_problem(problem)?;
        // Do not silently turn a multiworker product into a serial search.
        if problem.backend_policy().workers() != 1
            && !cfg!(all(feature = "parallel", not(target_family = "wasm")))
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_family_parallel_not_connected",
            ));
        }
        // A finite node cap is shared across Geometry and all BuildUp graphs.
        // Until that global credit is connected, refuse rather than grant each
        // worker a duplicate cap or silently downgrade the request.
        if problem.backend_policy().workers() > 1 && problem.backend_request().max_nodes() != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_family_parallel_node_budget_not_connected",
            ));
        }
        let height = problem.visible_height() as u8;
        let initial = Board256Mask::from_words(problem.initial_board().occupied_words());
        let full = Board256Mask::all_cells(u16::from(height) * 10)
            .map_err(|_| WasmExactSearchError::InvalidProblem("extended_pc_height_invalid"))?;
        let required = full.without(initial);
        if required.is_empty()
            || !required.count_ones().is_multiple_of(4)
            || problem.exact_pieces() != Some(required.count_ones() as usize / 4)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_area_invalid",
            ));
        }
        let field = BuildProbabilityField::from_words_preserving_height(
            height,
            initial.words(),
            required.words(),
        )
        .map_err(|_| WasmExactSearchError::InvalidProblem("extended_pc_field_invalid"))?;
        let admission = if let Some(authority) = authority {
            admit_budget_bound_search_execution_under_terminal_authority(
                problem,
                additional_coexisting_retained_bytes,
                authority,
                problem.backend_policy().workers(),
            )
        } else {
            admit_budget_bound_search_execution(problem, problem.backend_policy().workers())
        }
        .map_err(WasmExactSearchError::resource_admission)?;
        // The legacy ordinary execution API borrows a caller-owned problem;
        // both that input and the engine's owned snapshot coexist.
        let external = checked_pc_family_problem_nested_retained_bytes(problem)
            .and_then(|bytes| bytes.checked_add(additional_coexisting_retained_bytes))
            .and_then(|bytes| {
                bytes.checked_add(
                    core::mem::size_of::<Self>() as u128
                        + core::mem::size_of::<SearchProblem>() as u128,
                )
            })
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_memory_projection_unavailable",
            ))?;
        let engine = ExtendedBuildProbabilitySession::new_pc_family(
            problem,
            field,
            admission.memory_bound(),
            external,
        )?;
        Ok(Self {
            engine,
            _admission: admission,
            terminal_authority_supplied: authority.is_some(),
        })
    }

    pub fn advance(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
        if self.requested_workers() > 1 {
            let parallel_result =
                self.execute_parallel_if_worthwhile(self.requested_workers(), control)?;
            if let Some(result) = parallel_result {
                return Ok(ExactSearchAdvance::Completed(result));
            }
        }
        let advance = self.engine.advance(budget, control)?;
        let progress = self.engine.distributed_progress();
        control.report_progress("geometry", progress.geometry_nodes as u64, None);
        control.report_progress("buildup", progress.build_nodes as u64, None);
        Ok(match advance {
            BuildProbabilityAdvance::Pending => ExactSearchAdvance::Pending,
            BuildProbabilityAdvance::Completed(result) => ExactSearchAdvance::Completed(result),
            BuildProbabilityAdvance::Cancelled => ExactSearchAdvance::Cancelled,
        })
    }

    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    fn requested_workers(&self) -> usize {
        self.engine.requested_pc_workers()
    }

    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    pub fn execute_parallel_if_worthwhile(
        &mut self,
        requested_workers: usize,
        control: &ExecutionControl,
    ) -> Result<Option<CoreExecutionResult>, WasmExactSearchError> {
        self.engine
            .execute_pc_family_parallel(requested_workers, control)
    }

    pub fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        future: u128,
    ) -> Result<(), WasmExactSearchError> {
        // This ordinary borrowed-input path never manufactures the parent
        // authority required by typed score/Tiling terminal callbacks.
        if !self.terminal_authority_supplied {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_family_parent_authority_not_supplied",
            ));
        }
        let retained = self
            .engine
            .checked_retained_bytes_with_coexisting_owners()
            .and_then(|bytes| bytes.checked_add(result.checked_resource_retained_bytes()?))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_memory_projection_unavailable",
            ))?;
        self._admission
            .ensure_memory_bound(retained, future)
            .map_err(WasmExactSearchError::resource_admission)
    }
}
