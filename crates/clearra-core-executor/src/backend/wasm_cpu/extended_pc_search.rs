//! Ordinary full-height PC execution through the shared ILC/BuildUp/language
//! primitives. Product-specific minimum, score and replay authorities are not
//! interchangeable with an ordinary complete PC family.
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
    build_probability::{
        checked_build_probability_problem_nested_retained_bytes, BuildProbabilityAdvance,
    },
    extended_build_probability::ExtendedBuildProbabilitySession,
    ExactSearchAdvance, WasmExactSearchError,
};
use crate::{
    resource::{admit_budget_bound_search_execution, ExecutionAdmission},
    CoreExecutionResult,
};

pub(crate) struct ExtendedPcSearchSession {
    engine: ExtendedBuildProbabilitySession,
    _admission: ExecutionAdmission,
}

pub(super) fn validate_pc_family_problem(
    problem: &SearchProblem,
) -> Result<(), WasmExactSearchError> {
    if problem.preset() != SearchProblemPreset::ScenarioPc
        || problem.goal().as_str() != "clear-to-empty"
        || problem.initial_board().width() != 10
        || !(7..=24).contains(&problem.visible_height())
        || !matches!(
            problem.objective().kind(),
            ObjectiveKind::All | ObjectiveKind::Unique
        )
        || !matches!(
            problem.count_policy(),
            PcCountPolicy::CountAll | PcCountPolicy::CountUnique
        )
        || !matches!(
            problem.output_policy(),
            SearchOutputPolicy::Summary | SearchOutputPolicy::Trace
        )
        || problem.objective().score().requested()
        || problem.objective().execution_constraints().requested()
        || problem.pc_chance_evidence_policy() != PcChanceEvidencePolicy::Disabled
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

impl ExtendedPcSearchSession {
    pub fn new(problem: &SearchProblem) -> Result<Self, WasmExactSearchError> {
        validate_pc_family_problem(problem)?;
        // Do not silently turn a multiworker product into a serial search.
        if problem.backend_policy().workers() != 1 {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_family_parallel_not_connected",
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
        let admission = admit_budget_bound_search_execution(problem, 1)
            .map_err(WasmExactSearchError::resource_admission)?;
        // The legacy ordinary execution API borrows a caller-owned problem;
        // both that input and the engine's owned snapshot coexist.
        let external = checked_build_probability_problem_nested_retained_bytes(problem)
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
        })
    }

    pub fn advance(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
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

    pub fn validate_public_result_memory_with_future(
        &self,
        _result: &CoreExecutionResult,
        _future: u128,
    ) -> Result<(), WasmExactSearchError> {
        // This ordinary borrowed-input path never manufactures the parent
        // authority required by typed score/Tiling terminal callbacks.
        Err(WasmExactSearchError::InvalidProblem(
            "extended_pc_family_parent_authority_not_supplied",
        ))
    }
}
