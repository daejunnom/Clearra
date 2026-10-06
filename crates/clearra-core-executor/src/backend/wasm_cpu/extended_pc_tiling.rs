//! Full-height, geometry-only PC execution. This owns the same extended ILC
//! catalog used by Build, but never runs BuildUp, coverage, or path reducers.
//! It cannot be used as evidence for CountAll, minimum, score, or replay.

use std::{mem::size_of, sync::Arc};

use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    execution_cancellation::ExecutionControl,
    objective::objective_kind::ObjectiveKind,
    solution::{
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM, NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
    },
};
use clearra_problem::{
    BuildProbabilityField, SearchOutputPolicy, SearchProblem, SearchProblemPreset,
};
use clearra_supply::pattern_universe::PackingPatternMembershipKind;

use crate::{
    resource::{
        admit_budget_bound_search_execution_under_terminal_authority, ExecutionAdmission,
        WasmCpuTerminalResourceAuthority,
    },
    CoreExecutionResult, PcTilingMemoryAdmissionEvidence, TilingSolutionPageStore,
};

use super::{
    extended_buildup::ExtendedTilingKey,
    extended_geometry::{ExtendedGeometryAdvance, ExtendedGeometrySearch},
    extended_inverse_catalog::ExtendedInverseCatalog,
    ExactSearchAdvance, WasmExactSearchError,
};

pub(crate) struct ExtendedPcTilingSession {
    problem: Arc<SearchProblem>,
    catalog: ExtendedInverseCatalog,
    geometry: ExtendedGeometrySearch,
    keys: Vec<String>,
    execution_admission: ExecutionAdmission,
    external_retained_bytes: u128,
    finished: bool,
}

impl ExtendedPcTilingSession {
    pub fn new_under_authority(
        problem: Arc<SearchProblem>,
        external_retained_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmExactSearchError> {
        if !matches!(
            problem.preset(),
            SearchProblemPreset::ScenarioPc | SearchProblemPreset::OpeningPc
        ) || (problem.preset() == SearchProblemPreset::OpeningPc
            && problem.initial_board().occupied_words() != [0; 4])
            || problem.output_policy() != SearchOutputPolicy::TilingOnly
            || problem.objective().kind() != ObjectiveKind::Tiling
            || problem.goal().as_str() != "clear-to-empty"
            || problem.objective().execution_constraints().requested()
            || problem.solution_probability_policy().requested()
            || problem.initial_board().width() != 10
            || !(7..=24).contains(&problem.visible_height())
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_tiling_contract_invalid",
            ));
        }
        super::ensure_connected_kick_profile(&problem)?;
        let height = problem.visible_height() as u8;
        let initial = Board256Mask::from_words(problem.initial_board().occupied_words());
        let full = Board256Mask::all_cells(u16::from(height) * 10).map_err(|_| {
            WasmExactSearchError::InvalidProblem("extended_pc_tiling_height_invalid")
        })?;
        let required = full.without(initial);
        if required.is_empty()
            || !required.count_ones().is_multiple_of(4)
            || problem.exact_pieces() != Some(required.count_ones() as usize / 4)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_tiling_area_invalid",
            ));
        }
        let field = BuildProbabilityField::from_words_preserving_height(
            height,
            initial.words(),
            required.words(),
        )
        .map_err(|_| WasmExactSearchError::InvalidProblem("extended_pc_tiling_field_invalid"))?;
        let execution_admission = admit_budget_bound_search_execution_under_terminal_authority(
            &problem,
            external_retained_bytes,
            authority,
            1,
        )
        .map_err(WasmExactSearchError::resource_admission)?;
        let universe = problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let family = universe.packing_multiset_family_for_execution(
            field.target_piece_count(),
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            super::packing_hold_projection(&problem),
        );
        if !universe.complete()
            && family.membership_kind() != PackingPatternMembershipKind::ExactSymbolicStandardBag
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_tiling_supply_incomplete",
            ));
        }
        let fixed_catalog_peak = external_retained_bytes
            .checked_add(size_of::<Self>() as u128)
            .and_then(|bytes| bytes.checked_add(32 * 1024))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_tiling_memory_projection_unavailable",
            ))?;
        execution_admission
            .ensure_memory_bound(fixed_catalog_peak, 0)
            .map_err(WasmExactSearchError::resource_admission)?;
        let catalog_credit = execution_admission.memory_cap_bytes() - fixed_catalog_peak;
        let maximum_realizations = usize::try_from(catalog_credit / 2048).unwrap_or(usize::MAX);
        let catalog = ExtendedInverseCatalog::compile_bounded(field, maximum_realizations)
            .map_err(|error| {
                if error.reason() == "extended_catalog_memory_budget_exceeded" {
                    let required = catalog_credit.saturating_add(2048);
                    match execution_admission.ensure_memory_bound(fixed_catalog_peak, required) {
                        Err(report) => WasmExactSearchError::resource_admission(report),
                        Ok(()) => error,
                    }
                } else {
                    error
                }
            })?;
        let geometry = ExtendedGeometrySearch::new(universe, &family, &catalog)?;
        let session = Self {
            problem,
            catalog,
            geometry,
            keys: Vec::new(),
            execution_admission,
            external_retained_bytes,
            finished: false,
        };
        session.ensure_memory(0)?;
        Ok(session)
    }

    pub fn advance(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_tiling_already_finished",
            ));
        }
        for _ in 0..budget.max(1) {
            if control.is_cancelled() {
                return Ok(ExactSearchAdvance::Cancelled);
            }
            self.ensure_memory(0)?;
            let limits = self.problem.backend_request();
            if limits.max_nodes() != 0 && self.geometry.expanded_nodes() >= limits.max_nodes() {
                return Err(WasmExactSearchError::InvalidProblem("node_budget_exceeded"));
            }
            match self.geometry.advance(&self.catalog) {
                ExtendedGeometryAdvance::Pending => {}
                ExtendedGeometryAdvance::Candidate(candidate) => {
                    if limits.max_candidates() != 0 && self.keys.len() >= limits.max_candidates() {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "candidate_budget_exceeded",
                        ));
                    }
                    // Cover both the temporary placement owner and a possible
                    // vector growth before either allocation. No CTK3/render
                    // work is performed in this exact Geometry loop.
                    let placement_bytes = (candidate.row_ids().len() as u128) * 64;
                    let key_bytes = 128 + (candidate.row_ids().len() as u128) * 67;
                    let vector_growth = if self.keys.len() == self.keys.capacity() {
                        ((self.keys.len() + 1).saturating_mul(2).max(4) as u128)
                            * size_of::<String>() as u128
                    } else {
                        0
                    };
                    self.ensure_memory(placement_bytes + key_bytes + vector_growth)?;
                    self.keys.try_reserve(1).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "extended_pc_tiling_storage_unavailable",
                        )
                    })?;
                    let tiling = ExtendedTilingKey::from_candidate(&self.catalog, &candidate);
                    self.keys.push(
                        tiling.canonical_key(self.catalog.initial_board(), self.catalog.height()),
                    );
                    self.ensure_memory(0)?;
                }
                ExtendedGeometryAdvance::Complete => return self.finish(),
                ExtendedGeometryAdvance::ResourceIncomplete(reason) => {
                    return Err(WasmExactSearchError::InvalidProblem(reason));
                }
            }
        }
        control.report_progress("geometry", self.geometry.expanded_nodes() as u64, None);
        Ok(ExactSearchAdvance::Pending)
    }

    fn finish(&mut self) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        // A bounded first page can contain sixty placements per identity.
        // Count field/report backing and projection workspaces as well as page
        // copies while all Geometry state and the complete family still live.
        let page_count = self.keys.len().min(100);
        let future = 64 * 1024
            + (page_count as u128) * (128 + 60 * 67 + size_of::<String>() as u128)
            + TilingSolutionPageStore::checked_retained_capacity_projection_workspace_inline_bytes(
            )
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_tiling_memory_projection_unavailable",
            ))?;
        self.ensure_memory(future)?;
        self.keys.sort_unstable();
        self.keys.dedup();
        let store = Arc::new(
            TilingSolutionPageStore::from_extended_keys(
                self.catalog.height(),
                Board256Mask::from_words(self.catalog.initial_board().words()),
                core::mem::take(&mut self.keys),
            )
            .map_err(WasmExactSearchError::InvalidProblem)?,
        );
        let keys = store
            .page_keys(0, 100)
            .map_err(WasmExactSearchError::InvalidProblem)?;
        let count = store.len();
        let mut fields = Vec::new();
        let mut set = |key: &str, value: String| fields.push((key.to_owned(), value));
        for (key, value) in [
            ("problem_preset", self.problem.preset().as_str()),
            ("compiled_goal", "clear-to-empty"),
            ("search_output_policy", "tiling-only"),
            ("objective", "tiling"),
            ("actual_solution_set_contract", "normalized-tiling-set"),
            (
                "backend_requested",
                self.problem.backend_policy().requested_backend().as_str(),
            ),
            ("backend_selected", "wasm-cpu-pc-tiling-extended"),
            ("actual_backend", "wasm-cpu-pc-tiling-extended"),
            (
                "cpu_parallel_decision_reason",
                "direct-typed-tiling-session",
            ),
            (
                "normalized_solution_key_algorithm",
                NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
            ),
            (
                "normalized_solution_set_hash_algorithm",
                NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
            ),
            ("coverage_probability", "not-calculated"),
            ("tiling_materialization_incomplete_reason", "none"),
            ("tiling_family_incomplete_reason", "none"),
            ("resource_truncation_reason", "none"),
            ("count_truncated_reason", "none"),
        ] {
            set(key, value.to_owned());
        }
        for key in [
            "packing_source_raw_geometry",
            "tiling_objective_canonical",
            "tiling_materialization_memory_admission_accounted",
            "tiling_materialization_complete",
            "tiling_family_complete",
            "tiling_initial_page_complete",
            "count_complete",
            "solution_count_calculated",
            "solution_set_materialized",
            "objective_complete",
        ] {
            set(key, "true".to_owned());
        }
        for key in [
            "packing_source_buildability_preverified",
            "buildup_executed",
            "additional_buildup_executed",
            "buildability_verified",
            "coverage_calculated",
            "probability_calculated",
            "resource_truncated",
            "solution_probabilities_requested",
            "cpu_parallel_execution",
        ] {
            set(key, "false".to_owned());
        }
        for key in [
            "normalized_unique_solution_count",
            "actual_normalized_unique_solution_count",
            "total_solution_count",
            "unique_solution_count",
        ] {
            set(key, count.to_string());
        }
        for key in [
            "solution_keys_materialized_count",
            "tiling_initial_page_count",
        ] {
            set(key, keys.len().to_string());
        }
        for key in [
            "tiling_initial_page_covers_family",
            "solution_keys_complete",
        ] {
            set(key, (keys.len() == count).to_string());
        }
        set("solution_page_available", (keys.len() < count).to_string());
        for key in [
            "normalized_solution_set_hash",
            "actual_normalized_solution_set_hash",
        ] {
            set(key, store.normalized_hash().to_owned());
        }
        set(
            "workers_requested",
            self.problem.backend_policy().workers().to_string(),
        );
        set("workers_used", "1".to_owned());
        set("board_height", self.catalog.height().to_string());
        set(
            "packing_candidate_count",
            self.geometry.candidate_count().to_string(),
        );
        set(
            "searched_geometry_nodes",
            self.geometry.expanded_nodes().to_string(),
        );
        let result = CoreExecutionResult::new(fields, Vec::new())
            .with_normalized_solution_keys(keys)
            .with_tiling_solution_page_store(store)
            .with_pc_tiling_memory_admission_evidence(
                PcTilingMemoryAdmissionEvidence::WasmTerminalAuthority,
            );
        self.validate_public_result_memory_with_future(&result, 0)?;
        if !result.pc_tiling_family_publication_contract_is_valid() {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_tiling_publication_invalid",
            ));
        }
        self.finished = true;
        Ok(ExactSearchAdvance::Completed(result))
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        let mut bytes = (size_of::<Self>() as u128).checked_add(self.external_retained_bytes)?;
        bytes = bytes.checked_add(self.catalog.retained_bytes() as u128)?;
        bytes = bytes.checked_add(self.geometry.retained_bytes() as u128)?;
        bytes = bytes.checked_add(
            (self.keys.capacity() as u128).checked_mul(size_of::<String>() as u128)?,
        )?;
        for key in &self.keys {
            bytes = bytes.checked_add(key.capacity() as u128)?;
        }
        Some(bytes)
    }

    fn ensure_memory(&self, future: u128) -> Result<(), WasmExactSearchError> {
        let bytes = self
            .checked_retained_bytes()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_tiling_memory_projection_unavailable",
            ))?;
        self.execution_admission
            .ensure_memory_bound(bytes, future)
            .map_err(WasmExactSearchError::resource_admission)
    }

    pub fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        future: u128,
    ) -> Result<(), WasmExactSearchError> {
        let result_bytes = result.checked_resource_retained_bytes().ok_or(
            WasmExactSearchError::InvalidProblem(
                "extended_pc_tiling_memory_projection_unavailable",
            ),
        )?;
        self.ensure_memory(result_bytes.checked_add(future).ok_or(
            WasmExactSearchError::InvalidProblem(
                "extended_pc_tiling_memory_projection_unavailable",
            ),
        )?)
    }

    #[cfg(test)]
    pub fn admitted_memory_cap_bytes(&self) -> u128 {
        self.execution_admission.memory_cap_bytes()
    }
    #[cfg(test)]
    pub fn shares_problem_arc(&self, problem: &Arc<SearchProblem>) -> bool {
        Arc::ptr_eq(&self.problem, problem)
    }
    #[cfg(test)]
    pub fn checked_terminal_retained_bytes(&self, result: &CoreExecutionResult) -> Option<u128> {
        self.checked_retained_bytes()?
            .checked_add(result.checked_resource_retained_bytes()?)
    }
}
