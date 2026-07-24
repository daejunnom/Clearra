use std::collections::VecDeque;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_coverage::pattern::weighted_pattern_set::WeightedPatternSet;
use clearra_problem::{BuildProbabilityAggregation, BuildProbabilityField, SearchProblem};

use crate::{CoreExecutionResult, CorePostProcessSpinCoverage};

use super::{
    build_probability::{
        merge_symmetry_results, BuildProbabilityAdvance, CompactBuildProbabilitySession,
        CompactBuildProbabilitySharedCatalog,
    },
    distributed::{
        WasmCandidatePacket, WasmCandidateProducerAdvance, WasmDistributedGeometrySummary,
        WasmDistributedProgress,
    },
    extended_build_probability::ExtendedBuildProbabilitySession,
    WasmExactSearchError,
};

enum DistributedBuildProbabilitySession {
    Compact(CompactBuildProbabilitySession),
    Extended(ExtendedBuildProbabilitySession),
}

impl DistributedBuildProbabilitySession {
    fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        if field.is_compact() {
            CompactBuildProbabilitySession::new(problem, field, aggregation).map(Self::Compact)
        } else {
            ExtendedBuildProbabilitySession::new(problem, field, aggregation).map(Self::Extended)
        }
    }

    fn new_external_geometry(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        if field.is_compact() {
            CompactBuildProbabilitySession::new_external_geometry(problem, field, aggregation)
                .map(Self::Compact)
        } else {
            ExtendedBuildProbabilitySession::new_external_geometry(problem, field, aggregation)
                .map(Self::Extended)
        }
    }

    fn new_with_shared_supply_catalog(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        external_geometry: bool,
        shared_supply_catalog: &CompactBuildProbabilitySharedCatalog,
    ) -> Result<Self, WasmExactSearchError> {
        if field.is_compact() {
            CompactBuildProbabilitySession::new_with_shared_supply_catalog(
                problem,
                field,
                aggregation,
                external_geometry,
                shared_supply_catalog,
            )
            .map(Self::Compact)
        } else {
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_shared_supply_catalog_not_compact",
            ))
        }
    }

    fn shared_supply_catalog(&self) -> Option<CompactBuildProbabilitySharedCatalog> {
        match self {
            Self::Compact(session) => Some(session.shared_supply_catalog()),
            Self::Extended(_) => None,
        }
    }

    fn advance_distributed_geometry(
        &mut self,
        pass_index: u8,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, WasmExactSearchError> {
        match self {
            Self::Compact(session) => session.advance_distributed_geometry(pass_index, control),
            Self::Extended(session) => session.advance_distributed_geometry(pass_index, control),
        }
    }

    fn prepare_distributed_finalizer(&mut self) {
        match self {
            Self::Compact(session) => session.prepare_distributed_finalizer(),
            Self::Extended(session) => session.prepare_distributed_finalizer(),
        }
    }

    fn process_external_candidate(
        &mut self,
        candidate: &WasmCandidatePacket,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        match self {
            Self::Compact(session) => session.process_external_candidate(candidate, control),
            Self::Extended(session) => session.process_external_candidate(candidate, control),
        }
    }

    fn complete_distributed_worker(&mut self) -> Result<CoreExecutionResult, WasmExactSearchError> {
        match self {
            Self::Compact(session) => session.complete_distributed_worker(),
            Self::Extended(session) => session.complete_distributed_worker(),
        }
    }

    fn absorb_distributed_result(
        &mut self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmExactSearchError> {
        match self {
            Self::Compact(session) => session.absorb_distributed_result(result),
            Self::Extended(session) => session.absorb_distributed_result(result),
        }
    }

    fn complete_distributed_geometry(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        match self {
            Self::Compact(session) => session.complete_distributed_geometry(summary, workers_used),
            Self::Extended(session) => session.complete_distributed_geometry(summary, workers_used),
        }
    }

    fn progress(&self) -> WasmDistributedProgress {
        match self {
            Self::Compact(session) => session.distributed_progress(),
            Self::Extended(session) => session.distributed_progress(),
        }
    }
}

struct ProducerPass {
    pass_index: u8,
    session: DistributedBuildProbabilitySession,
}

struct ProducerPassSpec {
    pass_index: u8,
    field: BuildProbabilityField,
}

pub struct WasmBuildProbabilityCandidateProducer {
    problem: SearchProblem,
    aggregation: BuildProbabilityAggregation,
    active: Option<ProducerPass>,
    pending: VecDeque<ProducerPassSpec>,
    shared_supply_catalog: Option<CompactBuildProbabilitySharedCatalog>,
    finalizers: Vec<DistributedBuildProbabilitySession>,
    summaries: Vec<WasmDistributedGeometrySummary>,
    mirror_included: bool,
    mirror_distinct: bool,
    finished: bool,
}

impl WasmBuildProbabilityCandidateProducer {
    pub fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, &'static str> {
        let mirror_included = field.includes_applicable_horizontal_mirror();
        let original = field.original_only();
        let mirrored = mirror_included.then(|| original.mirrored_horizontally());
        let mirror_distinct = mirrored.is_some_and(|candidate| candidate != original);
        let active_session =
            DistributedBuildProbabilitySession::new(problem, original, aggregation)
                .map_err(map_error)?;
        let shared_supply_catalog = active_session.shared_supply_catalog();
        let active = Some(ProducerPass {
            pass_index: 0,
            session: active_session,
        });
        let mut pending = VecDeque::with_capacity(usize::from(mirror_distinct));
        if let Some(mirrored) = mirrored.filter(|candidate| *candidate != original) {
            pending.push_back(ProducerPassSpec {
                pass_index: 1,
                field: mirrored,
            });
        }
        Ok(Self {
            problem: problem.clone(),
            aggregation,
            active,
            pending,
            shared_supply_catalog,
            finalizers: Vec::with_capacity(usize::from(mirror_distinct) + 1),
            summaries: Vec::with_capacity(usize::from(mirror_distinct) + 1),
            mirror_included,
            mirror_distinct,
            finished: false,
        })
    }

    pub fn advance(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, &'static str> {
        if self.finished {
            return Err("wasm_build_probability_distributed_geometry_already_finished");
        }
        loop {
            if self.active.is_none() {
                let Some(spec) = self.pending.pop_front() else {
                    self.finished = true;
                    return Ok(WasmCandidateProducerAdvance::Completed(combined_summary(
                        &self.summaries,
                    )));
                };
                let session = match self.shared_supply_catalog.as_ref() {
                    Some(shared) => {
                        DistributedBuildProbabilitySession::new_with_shared_supply_catalog(
                            &self.problem,
                            spec.field,
                            self.aggregation,
                            false,
                            shared,
                        )
                    }
                    None => DistributedBuildProbabilitySession::new(
                        &self.problem,
                        spec.field,
                        self.aggregation,
                    ),
                }
                .map_err(map_error)?;
                self.active = Some(ProducerPass {
                    pass_index: spec.pass_index,
                    session,
                });
            }
            let pass = self.active.as_mut().expect("active pass was initialized");
            match pass
                .session
                .advance_distributed_geometry(pass.pass_index, control)
                .map_err(map_error)?
            {
                WasmCandidateProducerAdvance::Pending => {
                    return Ok(WasmCandidateProducerAdvance::Pending)
                }
                WasmCandidateProducerAdvance::Candidate(candidate) => {
                    return Ok(WasmCandidateProducerAdvance::Candidate(candidate))
                }
                WasmCandidateProducerAdvance::Cancelled => {
                    return Ok(WasmCandidateProducerAdvance::Cancelled)
                }
                WasmCandidateProducerAdvance::Completed(summary) => {
                    let mut pass = self.active.take().expect("active pass exists");
                    pass.session.prepare_distributed_finalizer();
                    self.finalizers.push(pass.session);
                    self.summaries.push(summary);
                }
            }
        }
    }

    pub fn into_merger(self) -> Result<WasmBuildProbabilityDistributedResultMerger, &'static str> {
        if !self.finished {
            return Err("wasm_build_probability_distributed_geometry_not_finished");
        }
        Ok(WasmBuildProbabilityDistributedResultMerger {
            passes: self.finalizers,
            summaries: self.summaries,
            pattern_weights: self
                .problem
                .piece_source()
                .materialized_pattern_weights()
                .ok_or("wasm_piece_source_not_materialized")?
                .clone(),
            aggregation: self.aggregation,
            execution_constraints_requested: self
                .problem
                .objective()
                .execution_constraints()
                .requested(),
            mirror_included: self.mirror_included,
            mirror_distinct: self.mirror_distinct,
            spin_coverages: Vec::new(),
        })
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        let pass_count = usize::from(self.mirror_distinct) + 1;
        let mut progress = WasmDistributedProgress {
            candidate_family_count: Some(0),
            pass_index: self
                .active
                .as_ref()
                .map_or(pass_count.saturating_sub(1), |pass| {
                    usize::from(pass.pass_index)
                }),
            pass_count,
            ..WasmDistributedProgress::default()
        };
        for summary in &self.summaries {
            progress.geometry_nodes = progress
                .geometry_nodes
                .saturating_add(summary.expanded_nodes);
            progress.candidates = progress.candidates.saturating_add(summary.candidate_count);
            progress.candidate_family_count = match (
                progress.candidate_family_count,
                summary.candidate_family_count,
            ) {
                (Some(total), Some(count)) => total.checked_add(count),
                _ => None,
            };
        }
        if let Some(pass) = &self.active {
            let pass_progress = pass.session.progress();
            progress.geometry_nodes = progress
                .geometry_nodes
                .saturating_add(pass_progress.geometry_nodes);
            progress.candidates = progress.candidates.saturating_add(pass_progress.candidates);
            progress.candidate_family_count = match (
                progress.candidate_family_count,
                pass_progress.candidate_family_count,
            ) {
                (Some(total), Some(count)) => total.checked_add(count),
                _ => None,
            };
        }
        progress
    }
}

pub struct WasmBuildProbabilityDistributedVerifier {
    problem: SearchProblem,
    aggregation: BuildProbabilityAggregation,
    pass_fields: Vec<BuildProbabilityField>,
    shared_supply_catalog: Option<CompactBuildProbabilitySharedCatalog>,
    active_pass: Option<(u8, DistributedBuildProbabilitySession)>,
    completed_results: Vec<CoreExecutionResult>,
    completed_progress: WasmDistributedProgress,
    finished: bool,
}

impl WasmBuildProbabilityDistributedVerifier {
    pub fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, &'static str> {
        let original = field.original_only();
        let mirrored = field
            .includes_applicable_horizontal_mirror()
            .then(|| original.mirrored_horizontally())
            .filter(|candidate| *candidate != original);
        let mut pass_fields = Vec::with_capacity(usize::from(mirrored.is_some()) + 1);
        pass_fields.push(original);
        if let Some(mirrored) = mirrored {
            pass_fields.push(mirrored);
        }
        Ok(Self {
            problem: problem.clone(),
            aggregation,
            pass_fields,
            shared_supply_catalog: None,
            active_pass: None,
            completed_results: Vec::new(),
            completed_progress: WasmDistributedProgress::default(),
            finished: false,
        })
    }

    pub fn consume(
        &mut self,
        candidate: &WasmCandidatePacket,
        control: &ExecutionControl,
    ) -> Result<(), &'static str> {
        if self.finished {
            return Err("wasm_build_probability_distributed_verifier_already_finished");
        }
        self.activate_pass(candidate.pass_index())?;
        self.active_pass
            .as_mut()
            .expect("requested pass was activated")
            .1
            .process_external_candidate(candidate, control)
            .map_err(map_error)
    }

    pub fn finish(&mut self) -> Result<Vec<CoreExecutionResult>, &'static str> {
        if self.finished {
            return Err("wasm_build_probability_distributed_verifier_already_finished");
        }
        self.finish_active_pass()?;
        self.finished = true;
        Ok(core::mem::take(&mut self.completed_results))
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        let mut progress = self.completed_progress;
        progress.pass_count = self.pass_fields.len().max(1);
        if let Some((pass_index, pass)) = &self.active_pass {
            progress.pass_index = usize::from(*pass_index);
            progress.merge(pass.progress());
        }
        progress
    }

    fn activate_pass(&mut self, pass_index: u8) -> Result<(), &'static str> {
        let pass_position = usize::from(pass_index);
        let field = *self
            .pass_fields
            .get(pass_position)
            .ok_or("wasm_build_probability_distributed_pass_invalid")?;
        if let Some((active_index, _)) = &self.active_pass {
            if *active_index == pass_index {
                return Ok(());
            }
            if *active_index > pass_index {
                return Err("wasm_build_probability_distributed_pass_out_of_order");
            }
            self.finish_active_pass()?;
        }
        let session = match self.shared_supply_catalog.as_ref() {
            Some(shared) => DistributedBuildProbabilitySession::new_with_shared_supply_catalog(
                &self.problem,
                field,
                self.aggregation,
                true,
                shared,
            ),
            None => DistributedBuildProbabilitySession::new_external_geometry(
                &self.problem,
                field,
                self.aggregation,
            ),
        }
        .map_err(map_error)?;
        if self.shared_supply_catalog.is_none() {
            self.shared_supply_catalog = session.shared_supply_catalog();
        }
        self.active_pass = Some((pass_index, session));
        Ok(())
    }

    fn finish_active_pass(&mut self) -> Result<(), &'static str> {
        let Some((pass_index, mut pass)) = self.active_pass.take() else {
            return Ok(());
        };
        self.completed_progress.merge(pass.progress());
        self.completed_progress.pass_index = usize::from(pass_index);
        let result = pass
            .complete_distributed_worker()
            .map_err(map_error)?
            .with_additional_fields(vec![(
                "build_distributed_pass_index".to_owned(),
                pass_index.to_string(),
            )]);
        self.completed_results.push(result);
        Ok(())
    }
}

pub struct WasmBuildProbabilityDistributedResultMerger {
    passes: Vec<DistributedBuildProbabilitySession>,
    summaries: Vec<WasmDistributedGeometrySummary>,
    pattern_weights: WeightedPatternSet,
    aggregation: BuildProbabilityAggregation,
    execution_constraints_requested: bool,
    mirror_included: bool,
    mirror_distinct: bool,
    spin_coverages: Vec<CorePostProcessSpinCoverage>,
}

impl WasmBuildProbabilityDistributedResultMerger {
    pub fn absorb(&mut self, result: &CoreExecutionResult) -> Result<(), &'static str> {
        let pass_index = result
            .usize_field("build_distributed_pass_index")
            .ok_or("wasm_build_probability_distributed_pass_missing")?;
        self.spin_coverages
            .extend(result.postprocess_spin_coverages().iter().cloned());
        self.passes
            .get_mut(pass_index)
            .ok_or("wasm_build_probability_distributed_pass_invalid")?
            .absorb_distributed_result(result)
            .map_err(map_error)
    }

    pub fn finish(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<CoreExecutionResult, &'static str> {
        if self.passes.len() != self.summaries.len() {
            return Err("wasm_build_probability_distributed_summary_mismatch");
        }
        let mut results = Vec::with_capacity(self.passes.len());
        for (pass, summary) in self.passes.iter_mut().zip(&self.summaries) {
            match pass
                .complete_distributed_geometry(summary, workers_used)
                .map_err(map_error)?
            {
                BuildProbabilityAdvance::Completed(result) => results.push(result),
                BuildProbabilityAdvance::Pending => {
                    return Err("wasm_build_probability_distributed_finish_pending")
                }
                BuildProbabilityAdvance::Cancelled => return Err("wasm_cpu_search_cancelled"),
            }
        }
        let result = merge_symmetry_results(
            results,
            self.mirror_included,
            self.mirror_distinct,
            &self.pattern_weights,
            self.aggregation.requests_spin_coverage() || self.execution_constraints_requested,
        )
        .map_err(map_error)?;
        Ok(apply_backend_execution(
            result.with_postprocess_spin_coverages(core::mem::take(&mut self.spin_coverages)),
            &summary.backend_execution,
        ))
    }
}

fn apply_backend_execution(
    result: CoreExecutionResult,
    execution: &super::distributed::WasmDistributedBackendExecution,
) -> CoreExecutionResult {
    use super::distributed::WasmDistributedBackendExecution;

    let cpu_backend = if result.field("board_storage") == Some("board256-canonical") {
        "wasm-cpu-build-probability-extended"
    } else {
        "wasm-cpu-build-probability"
    };

    let replacements = match execution {
        WasmDistributedBackendExecution::Cpu => return result,
        WasmDistributedBackendExecution::CpuFallback {
            reason,
            failure_class,
            failure_stage,
            discarded_partial_gpu_result,
            original_gpu_result_incomplete,
        } => vec![
            field("backend_selected", cpu_backend),
            field("actual_backend", cpu_backend),
            field("backend_fallback_used", true),
            field("backend_fallback_reason", reason),
            field("fallback_backend", cpu_backend),
            field("gpu_failure_class", failure_class),
            field("gpu_failure_stage", failure_stage),
            field("discarded_partial_gpu_result", discarded_partial_gpu_result),
            field(
                "gpu_original_result_incomplete",
                original_gpu_result_incomplete,
            ),
        ],
        WasmDistributedBackendExecution::WebGpu {
            adapter_index,
            adapter_name,
            adapter_type,
            adapter_backend,
            peak_gpu_bytes,
            shader_hash,
            shader_version,
            warmup_performed,
            session_reused,
        } => vec![
            field("backend_selected", "webgpu"),
            field("actual_backend", "webgpu"),
            field("backend_fallback_used", false),
            field("backend_fallback_reason", "none"),
            field("fallback_backend", "none"),
            field("gpu_failure_class", "none"),
            field("gpu_failure_stage", "none"),
            field("discarded_partial_gpu_result", false),
            field("gpu_original_result_incomplete", false),
            field("gpu_device_selected_index", adapter_index),
            field("gpu_device_selected_name", adapter_name),
            field("gpu_device_selected_type", adapter_type),
            field("gpu_device_selected_backend", adapter_backend),
            field("resource_peak_gpu_bytes", peak_gpu_bytes),
            field("gpu_shader_hash", shader_hash),
            field("gpu_shader_version", shader_version),
            field("gpu_warmup_performed", warmup_performed),
            field("gpu_session_reused", session_reused),
        ],
    };
    result.with_replaced_fields(replacements)
}

fn combined_summary(
    summaries: &[WasmDistributedGeometrySummary],
) -> WasmDistributedGeometrySummary {
    WasmDistributedGeometrySummary {
        candidate_count: summaries
            .iter()
            .map(|summary| summary.candidate_count)
            .sum(),
        candidate_digest: summaries.iter().fold(0, |digest, summary| {
            super::mix_digest(digest, summary.candidate_digest)
        }),
        candidate_family_count: summaries.iter().try_fold(0_u128, |total, summary| {
            summary
                .candidate_family_count
                .and_then(|count| total.checked_add(count))
        }),
        expanded_nodes: summaries.iter().map(|summary| summary.expanded_nodes).sum(),
        peak_frontier: summaries
            .iter()
            .map(|summary| summary.peak_frontier)
            .max()
            .unwrap_or(0),
        domain_pruned_states: summaries
            .iter()
            .map(|summary| summary.domain_pruned_states)
            .sum(),
        hall_pruned_states: summaries
            .iter()
            .map(|summary| summary.hall_pruned_states)
            .sum(),
        column_pruned_states: summaries
            .iter()
            .map(|summary| summary.column_pruned_states)
            .sum(),
        component_compositions: summaries
            .iter()
            .map(|summary| summary.component_compositions)
            .sum(),
        truncated_reason: summaries
            .iter()
            .find_map(|summary| summary.truncated_reason),
        backend_execution: super::distributed::WasmDistributedBackendExecution::Cpu,
    }
}

fn map_error(error: WasmExactSearchError) -> &'static str {
    match error {
        WasmExactSearchError::InvalidProblem(reason) => reason,
        WasmExactSearchError::Cancelled => "wasm_cpu_search_cancelled",
    }
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}
