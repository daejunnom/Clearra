//! Request-owned compact failed-queue evidence over the exact Oracle solver.

use std::sync::Arc;

use clearra_core_domain::{
    execution_cancellation::ExecutionControl, objective::objective_kind::ObjectiveKind,
};
use clearra_pc_graph::request::{PcCountPolicy, RequestedSearchBackend};
use clearra_problem::{SearchOutputPolicy, SearchProblem, SearchProblemPreset};

use super::{ExactSearchAdvance, SearchProblemRetention, WasmExactSearchSession};
use crate::{
    backend::{wasm_cpu_search_backend::map_error, WasmCpuSearchError, WasmPcFailedQueueAdvance},
    core_execution_result::CoreResultFieldReplacementError,
    pc_failed_queue_evidence::{
        PcFailedQueueEvidence, PcFailedQueueEvidenceError, PcFailedQueueEvidenceProducer,
        PcFailedQueueExecutionAuthority, PcFailedQueueProducerAdmission,
        PcFailedQueueSourceCompleteness,
    },
    resource::{
        admit_budget_bound_search_execution_under_terminal_authority,
        WasmCpuTerminalResourceAuthority,
    },
    service::{field, percent_service::format_probability},
    CoreExecutionResult,
};

/// A whole, one-worker 1–6L CPU execution. Neither a caller's coverage fields
/// nor an ordinary chance result can authorize a failed-queue complement.
pub struct WasmCompactPcFailedQueueSession {
    engine: WasmExactSearchSession,
    authority: PcFailedQueueExecutionAuthority,
    example_limit: usize,
    finished: bool,
    // Field ordering drops the engine's compute child before the parent.
    _terminal_authority: WasmCpuTerminalResourceAuthority,
}

impl WasmCompactPcFailedQueueSession {
    pub fn new(problem: Arc<SearchProblem>) -> Result<Self, WasmCpuSearchError> {
        let example_limit = problem
            .pc_chance_evidence_policy()
            .pc_failed_queue_example_limit()
            .ok_or_else(|| rejected("compact_pc_failed_queue_purpose_required"))?;
        if !matches!(
            problem.preset(),
            SearchProblemPreset::OpeningPc | SearchProblemPreset::ScenarioPc
        ) || problem.initial_board().width() != 10
            || !(1..=6).contains(&problem.visible_height())
            || !(1..=6).contains(&problem.compact_pc_geometry_height())
            || problem.output_policy() != SearchOutputPolicy::CoverageSummary
            || problem.goal().as_str() != "clear-to-empty"
            || !matches!(
                (problem.objective().kind(), problem.count_policy()),
                (ObjectiveKind::All, PcCountPolicy::CountAll)
                    | (ObjectiveKind::Unique, PcCountPolicy::CountUnique)
            )
            || problem.objective().execution_constraints().requested()
            || problem
                .queue_observation_policy()
                .requires_observation_policy()
        {
            return Err(rejected("compact_pc_failed_queue_request_unqualified"));
        }
        let backend = problem.backend_policy();
        if !matches!(
            backend.requested_backend(),
            RequestedSearchBackend::Cpu | RequestedSearchBackend::Auto
        ) || backend.workers() != 1
            || backend.workers_requested().is_some_and(|count| count != 1)
        {
            return Err(WasmCpuSearchError::Unsupported {
                reason: "compact_pc_failed_queue_requires_one_cpu_worker",
            });
        }
        let source = problem.piece_source();
        if !source.complete()
            || !source.materialized_universe().is_some_and(|universe| {
                universe.complete()
                    && universe.pattern_count() > 0
                    && universe.total_possible_pattern_count() == universe.pattern_count() as u128
            })
        {
            return Err(rejected("compact_pc_failed_queue_source_incomplete"));
        }
        WasmExactSearchSession::validate_tiling_session_problem(&problem).map_err(map_error)?;

        // The exact compiled owner is shared by solver and evidence; do not
        // create an unaccounted SearchProblem clone. The solver accounts its
        // inline storage; additionally reserve the full problem and the
        // containing session/Arc headers (including the evidence owner).
        let external = problem
            .checked_pc_family_pointee_retained_bytes()
            .and_then(|bytes| {
                let enclosing_storage = core::mem::size_of::<Self>()
                    .checked_sub(core::mem::size_of::<WasmExactSearchSession>())?
                    .checked_add(6 * core::mem::size_of::<usize>())?;
                bytes.checked_add(enclosing_storage as u128)
            })
            .ok_or_else(memory_projection_unavailable)?;
        let terminal_authority = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity()
            .map_err(WasmCpuSearchError::resource_admission)?;
        let execution_admission = admit_budget_bound_search_execution_under_terminal_authority(
            &problem,
            external,
            &terminal_authority,
            1,
        )
        .map_err(WasmCpuSearchError::resource_admission)?;
        let engine = WasmExactSearchSession::new_with_preacquired_execution_admission(
            Arc::clone(&problem),
            false,
            SearchProblemRetention::ParentAuthorizedSharedInput {
                checked_external_retained_upper_bound_bytes: external,
            },
            execution_admission,
        )
        .map_err(map_error)?;
        Ok(Self {
            engine,
            authority: PcFailedQueueExecutionAuthority::new(problem),
            example_limit,
            finished: false,
            _terminal_authority: terminal_authority,
        })
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<WasmPcFailedQueueAdvance, WasmCpuSearchError> {
        if self.finished {
            return Err(rejected("compact_pc_failed_queue_already_finished"));
        }
        if control.is_cancelled() {
            self.finished = true;
            return Ok(WasmPcFailedQueueAdvance::Cancelled);
        }
        if control.partition().count() != 1 {
            self.finished = true;
            return Err(rejected("compact_pc_failed_queue_partial_execution"));
        }
        match self.engine.advance(work_budget, control) {
            Ok(ExactSearchAdvance::Pending) => Ok(WasmPcFailedQueueAdvance::Pending),
            Ok(ExactSearchAdvance::Cancelled) => {
                self.finished = true;
                Ok(WasmPcFailedQueueAdvance::Cancelled)
            }
            Ok(ExactSearchAdvance::Completed(result)) => {
                self.finished = true;
                if control.is_cancelled() {
                    return Ok(WasmPcFailedQueueAdvance::Cancelled);
                }
                let (result, evidence) = self.finish(result)?;
                if control.is_cancelled() {
                    return Ok(WasmPcFailedQueueAdvance::Cancelled);
                }
                Ok(WasmPcFailedQueueAdvance::Completed(result, evidence))
            }
            Err(error) => {
                self.finished = true;
                Err(map_error(error))
            }
        }
    }

    fn finish(
        &self,
        result: CoreExecutionResult,
    ) -> Result<(CoreExecutionResult, PcFailedQueueEvidence), WasmCpuSearchError> {
        let source = result
            .pc_chance_coverage_evidence()
            .ok_or_else(|| rejected("compact_pc_failed_queue_source_evidence_missing"))?;
        // This is the actual session's private Build coverage capture, not
        // public fields from a caller or a PC-chance command's output.
        if !source
            .problem()
            .matches_search_problem(self.authority.problem())
        {
            return Err(rejected("compact_pc_failed_queue_source_evidence_mismatch"));
        }
        let packing_complete = self.engine.finished && self.engine.truncated_reason.is_none();
        if !packing_complete || !source.complete() {
            return Err(rejected("compact_pc_failed_queue_source_incomplete"));
        }
        let observed_bytes = self
            .engine
            .checked_session_retained_bytes()
            .and_then(|bytes| bytes.checked_add(result.checked_resource_retained_bytes()?))
            .and_then(|bytes| {
                bytes.checked_add(core::mem::size_of::<PcFailedQueueEvidence>() as u128)
            })
            .ok_or_else(memory_projection_unavailable)?;
        let evidence = PcFailedQueueEvidenceProducer::produce(
            self.authority.clone(),
            source.rows(),
            self.example_limit,
            PcFailedQueueSourceCompleteness::new(
                packing_complete,
                self.engine.count_complete,
                source.complete(),
                result.bool_field("objective_complete") == Some(true),
            ),
            PcFailedQueueProducerAdmission::new(
                self.engine._execution_admission.memory_bound(),
                observed_bytes,
            ),
        )
        .map_err(|error| match error {
            PcFailedQueueEvidenceError::MemoryAdmission(report) => {
                WasmCpuSearchError::resource_admission(*report)
            }
            _ => rejected("compact_pc_failed_queue_evidence_invalid"),
        })?;
        let words = result.coverage_pattern_words();
        let success = evidence.success_coverage();
        if success.word_count() != words.len()
            || words
                .iter()
                .enumerate()
                .any(|(index, word)| success.word_at(index) != *word)
        {
            return Err(rejected("compact_pc_failed_queue_coverage_union_mismatch"));
        }
        self.project_finished_result(result.without_pc_chance_transient_evidence(), evidence)
    }

    fn project_finished_result(
        &self,
        result: CoreExecutionResult,
        evidence: PcFailedQueueEvidence,
    ) -> Result<(CoreExecutionResult, PcFailedQueueEvidence), WasmCpuSearchError> {
        let evidence_bytes = evidence
            .memory_report()
            .retained_producer_bytes()
            .checked_add(core::mem::size_of::<PcFailedQueueEvidence>() as u128)
            .ok_or_else(memory_projection_unavailable)?;
        // Before allocating the short legacy strings, include the already
        // retained typed complement. The replacement guard below rechecks
        // their actual capacities and the rebuilt execution report.
        let compatibility_peak = evidence_bytes
            .checked_add(2_048)
            .ok_or_else(memory_projection_unavailable)?;
        self.engine
            .validate_public_result_memory_with_future(&result, compatibility_peak)
            .map_err(map_error)?;
        let probability = format_probability(evidence.success_probability().get());
        let fields = vec![
            field("status", "percent-executed"),
            field("problem_preset", evidence.problem().preset().as_str()),
            field("pattern_count", evidence.pattern_count()),
            // Compatibility field for the App validator; this is the actual
            // number of Rust BuildUp rows, not a C-execution assertion.
            field("c_buildup_coverage_row_count", evidence.source_row_count()),
            field("piece_source_id", evidence.piece_source_id()),
            field("pattern_universe_id", evidence.pattern_universe_id().get()),
            field(
                "pattern_weight_model_id",
                evidence.pattern_weight_model_id().get(),
            ),
            field("coverage_probability", &probability),
            field("probability", &probability),
            field("weighted_probability", &probability),
            field(
                "materialized_probability_mass",
                format_probability(evidence.materialized_probability_mass().get()),
            ),
            field("truncated", false),
        ];
        drop(probability);
        let result = result
            .try_with_replaced_fields_with_memory_guard(fields, |stage, future| {
                self.engine
                    .validate_public_result_memory_with_future(
                        stage,
                        evidence_bytes
                            .checked_add(future)
                            .ok_or_else(memory_projection_unavailable)?,
                    )
                    .map_err(map_error)
            })
            .map_err(|error| match error {
                CoreResultFieldReplacementError::MemoryGuard(error) => error,
                _ => memory_projection_unavailable(),
            })?;
        self.engine
            .validate_public_result_memory_with_future(&result, evidence_bytes)
            .map_err(map_error)?;
        Ok((result, evidence))
    }
}

fn rejected(reason: &'static str) -> WasmCpuSearchError {
    WasmCpuSearchError::InvalidProblem { reason }
}

fn memory_projection_unavailable() -> WasmCpuSearchError {
    rejected("compact_pc_failed_queue_memory_projection_unavailable")
}

#[cfg(test)]
#[path = "compact_pc_failed_queue_tests.rs"]
mod tests;
