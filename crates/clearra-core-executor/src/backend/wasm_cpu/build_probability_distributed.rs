// SRP rationale: this module has one change reason: deterministic distributed Build-probability execution on WASM CPU workers.
use std::collections::VecDeque;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, FinesseMetric, FinessePatternKnowledge,
    SearchProblem,
};

use crate::{
    resource::{
        admit_budget_bound_search_execution, ExecutionAdmission, ExecutionAdmissionPlan,
        ExecutionMemoryBound,
    },
    CoreExecutionResult, CorePostProcessSpinCoverage, WasmCpuSearchError,
};

use super::{
    build_probability::{
        attach_finesse_report_with_memory_guard, checked_core_result_vec_retained_bytes,
        checked_finesse_material_vec_retained_bytes, exact_usize_field,
        merge_symmetry_results_with_memory_guard, BuildProbabilityAdvance,
        CompactBuildProbabilitySession, CompactBuildProbabilitySharedCatalog,
        FinesseSearchMaterial,
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
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        if field.is_compact() {
            CompactBuildProbabilitySession::new_with_memory_bound(
                problem,
                field,
                aggregation,
                memory_bound,
            )
            .map(Self::Compact)
        } else {
            ExtendedBuildProbabilitySession::new_with_memory_bound(
                problem,
                field,
                aggregation,
                memory_bound,
            )
            .map(Self::Extended)
        }
    }

    fn new_with_finesse(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_requested: bool,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        if !finesse_requested {
            return Self::new(problem, field, aggregation, memory_bound);
        }
        if field.is_compact() {
            CompactBuildProbabilitySession::new_with_finesse_and_memory_bound(
                problem,
                field,
                aggregation,
                true,
                memory_bound,
            )
            .map(Self::Compact)
        } else {
            ExtendedBuildProbabilitySession::new_with_finesse_and_memory_bound(
                problem,
                field,
                aggregation,
                memory_bound,
            )
            .map(Self::Extended)
        }
    }

    fn new_external_geometry(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_external_geometry_with_coexisting_retained_bytes(
            problem,
            field,
            aggregation,
            memory_bound,
            0,
        )
    }

    fn new_external_geometry_with_coexisting_retained_bytes(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        if field.is_compact() {
            CompactBuildProbabilitySession::new_external_geometry_with_memory_bound_and_coexisting_retained_bytes(
                problem,
                field,
                aggregation,
                memory_bound,
                coexisting_retained_bytes,
            )
            .map(Self::Compact)
        } else {
            ExtendedBuildProbabilitySession::new_external_geometry_with_memory_bound_and_coexisting_retained_bytes(
                problem,
                field,
                aggregation,
                memory_bound,
                coexisting_retained_bytes,
            )
            .map(Self::Extended)
        }
    }

    fn new_with_shared_supply_catalog(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        external_geometry: bool,
        shared_supply_catalog: &CompactBuildProbabilitySharedCatalog,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_shared_supply_catalog_and_coexisting_retained_bytes(
            problem,
            field,
            aggregation,
            external_geometry,
            shared_supply_catalog,
            memory_bound,
            0,
        )
    }

    fn new_with_shared_supply_catalog_and_coexisting_retained_bytes(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        external_geometry: bool,
        shared_supply_catalog: &CompactBuildProbabilitySharedCatalog,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        if field.is_compact() {
            CompactBuildProbabilitySession::new_with_shared_supply_catalog_and_memory_bound_and_coexisting_retained_bytes(
                problem,
                field,
                aggregation,
                external_geometry,
                shared_supply_catalog,
                memory_bound,
                coexisting_retained_bytes,
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

    fn advance_distributed_geometry_with_candidate_memory_guard(
        &mut self,
        pass_index: u8,
        control: &ExecutionControl,
        mut memory_guard: impl FnMut(u128, u128) -> Result<(), WasmExactSearchError>,
    ) -> Result<WasmCandidateProducerAdvance, WasmExactSearchError> {
        match self {
            Self::Compact(session) => session
                .advance_distributed_geometry_with_candidate_memory_guard(
                    pass_index,
                    control,
                    |session, local_retained_bytes, checked_future_bytes| {
                        let observed = session
                            .checked_retained_bytes()
                            .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_build_probability_candidate_row_storage_projection_overflow",
                            ))?;
                        memory_guard(observed, checked_future_bytes)
                    },
                ),
            Self::Extended(session) => session
                .advance_distributed_geometry_with_candidate_memory_guard(
                    pass_index,
                    control,
                    |session, local_retained_bytes, checked_future_bytes| {
                        let observed = session
                            .checked_retained_bytes()
                            .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_extended_candidate_row_storage_projection_overflow",
                            ))?;
                        memory_guard(observed, checked_future_bytes)
                    },
                ),
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

    fn absorb_distributed_result_with_memory_guard(
        &mut self,
        result: &CoreExecutionResult,
        mut memory_guard: impl FnMut(u128, u128) -> Result<(), WasmExactSearchError>,
    ) -> Result<(), WasmExactSearchError> {
        match self {
            Self::Compact(session) => session.absorb_distributed_result_with_memory_guard(
                result,
                |session, local_retained_bytes, checked_future_bytes| {
                    let observed = session
                        .checked_retained_bytes()
                        .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_build_probability_distributed_absorb_projection_overflow",
                        ))?;
                    memory_guard(observed, checked_future_bytes)
                },
            ),
            Self::Extended(session) => session.absorb_distributed_result_with_memory_guard(
                result,
                |session, local_retained_bytes, checked_future_bytes| {
                    let observed = session
                        .checked_retained_bytes()
                        .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_extended_distributed_absorb_projection_overflow",
                        ))?;
                    memory_guard(observed, checked_future_bytes)
                },
            ),
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

    fn annotate_finesse(&mut self, control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
        match self {
            Self::Compact(session) => session.annotate_distributed_finesse(control),
            Self::Extended(session) => session.annotate_distributed_finesse(control),
        }
    }

    fn finesse_search_material(&self) -> Result<FinesseSearchMaterial, WasmExactSearchError> {
        match self {
            Self::Compact(session) => session.finesse_search_material(),
            Self::Extended(session) => session.finesse_search_material(),
        }
    }

    fn checked_finesse_search_material_future_bytes(&self) -> Option<u128> {
        match self {
            Self::Compact(session) => session.checked_finesse_search_material_future_bytes(),
            Self::Extended(session) => session.checked_finesse_search_material_future_bytes(),
        }
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        match self {
            Self::Compact(session) => session.checked_retained_bytes(),
            Self::Extended(session) => session.checked_retained_bytes(),
        }
    }

    fn set_coexisting_retained_bytes(&mut self, bytes: u128) {
        match self {
            Self::Compact(session) => session.set_coexisting_retained_bytes(bytes),
            Self::Extended(session) => session.set_coexisting_retained_bytes(bytes),
        }
    }

    #[cfg(test)]
    fn set_memory_bound_for_test(&mut self, memory_bound: ExecutionMemoryBound) {
        match self {
            Self::Compact(session) => session.set_memory_bound_for_test(memory_bound),
            Self::Extended(session) => session.set_memory_bound_for_test(memory_bound),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuildProbabilityAggregateBudget {
    total_cap_bytes: u128,
    coordinator_reserved_bytes: u128,
    replica_count: u128,
    replica_cap_bytes: u128,
}

impl BuildProbabilityAggregateBudget {
    fn new(
        total_cap_bytes: u128,
        coordinator_reserved_bytes: u128,
        verifier_count: usize,
    ) -> Option<Self> {
        let replica_count = (verifier_count as u128).checked_add(1)?;
        let replica_bytes = total_cap_bytes.checked_sub(coordinator_reserved_bytes)?;
        let replica_cap_bytes = replica_bytes.checked_div(replica_count)?;
        Some(Self {
            total_cap_bytes,
            coordinator_reserved_bytes,
            replica_count,
            replica_cap_bytes,
        })
    }

    fn merger_cap_bytes(self) -> Option<u128> {
        self.total_cap_bytes
            .checked_sub(self.coordinator_reserved_bytes)
    }
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
    finesse_metric: FinesseMetric,
    finesse_pattern_knowledge: FinessePatternKnowledge,
    execution_admission: ExecutionAdmission,
    aggregate_budget: BuildProbabilityAggregateBudget,
    producer_memory_bound: ExecutionMemoryBound,
    finished: bool,
}

impl WasmBuildProbabilityCandidateProducer {
    pub fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, &'static str> {
        Self::new_with_finesse(
            problem,
            field,
            aggregation,
            FinesseMetric::Off,
            FinessePatternKnowledge::Both,
        )
    }

    pub fn new_with_finesse(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_metric: FinesseMetric,
        finesse_pattern_knowledge: FinessePatternKnowledge,
    ) -> Result<Self, &'static str> {
        Self::new_with_finesse_typed(
            problem,
            field,
            aggregation,
            finesse_metric,
            finesse_pattern_knowledge,
        )
        .map_err(WasmCpuSearchError::reason)
    }

    pub fn new_with_finesse_typed(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_metric: FinesseMetric,
        finesse_pattern_knowledge: FinessePatternKnowledge,
    ) -> Result<Self, WasmCpuSearchError> {
        Self::new_with_finesse_and_verifiers_typed(
            problem,
            field,
            aggregation,
            finesse_metric,
            finesse_pattern_knowledge,
            0,
            0,
        )
    }

    pub fn new_with_finesse_and_verifiers_typed(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_metric: FinesseMetric,
        finesse_pattern_knowledge: FinessePatternKnowledge,
        verifier_count: usize,
        coordinator_reserved_bytes: u128,
    ) -> Result<Self, WasmCpuSearchError> {
        let mirror_included = field.includes_applicable_horizontal_mirror();
        let original = field.original_only();
        let mirrored = mirror_included.then(|| original.mirrored_horizontally());
        let mirror_distinct = mirrored.is_some_and(|candidate| candidate != original);
        let replica_count = verifier_count.checked_add(1).ok_or_else(|| {
            WasmCpuSearchError::ResourceAdmission {
                resource_report: clearra_core_domain::resource::ResourceReport::admission_failure(
                    clearra_core_domain::resource::ExecutionAvailability::unavailable(
                        clearra_core_domain::resource::ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
                    ),
                ),
            }
        })?;
        let execution_admission = admit_budget_bound_search_execution(problem, replica_count)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;
        let pass_count = usize::from(mirror_distinct) + 1;
        let aggregate_plan = ExecutionAdmissionPlan::build_probability_with_verifiers(
            problem,
            pass_count,
            verifier_count,
        )
        .ok_or_else(|| WasmCpuSearchError::ResourceAdmission {
            resource_report: execution_admission
                .ensure_memory_bound(u128::MAX, 1)
                .expect_err("checked aggregate plan overflow is unavailable"),
        })?;
        execution_admission
            .ensure_plan(aggregate_plan, coordinator_reserved_bytes)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;
        let aggregate_budget = BuildProbabilityAggregateBudget::new(
            execution_admission.memory_cap_bytes(),
            coordinator_reserved_bytes,
            verifier_count,
        )
        .ok_or_else(|| WasmCpuSearchError::ResourceAdmission {
            resource_report: execution_admission
                .memory_bound()
                .ensure(coordinator_reserved_bytes, 0)
                .expect_err("coordinator reservation exceeds the execution cap"),
        })?;
        let producer_memory_bound = execution_admission
            .memory_bound()
            .with_cap(aggregate_budget.replica_cap_bytes)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;
        let active_session = DistributedBuildProbabilitySession::new_with_finesse(
            problem,
            original,
            aggregation,
            finesse_metric.requested(),
            producer_memory_bound,
        )
        .map_err(map_typed_error)?;
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
        let producer = Self {
            problem: problem.clone(),
            aggregation,
            active,
            pending,
            shared_supply_catalog,
            finalizers: Vec::with_capacity(usize::from(mirror_distinct) + 1),
            summaries: Vec::with_capacity(usize::from(mirror_distinct) + 1),
            mirror_included,
            mirror_distinct,
            finesse_metric,
            finesse_pattern_knowledge,
            execution_admission,
            aggregate_budget,
            producer_memory_bound,
            finished: false,
        };
        producer
            .ensure_memory_bound()
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;
        Ok(producer)
    }

    pub(crate) fn delegate_verifier_admission(
        &self,
    ) -> Result<ExecutionAdmission, WasmCpuSearchError> {
        self.execution_admission
            .try_delegate_compute_only_with_memory_cap(self.aggregate_budget.replica_cap_bytes)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })
    }

    pub fn new_delegated_verifier(
        &self,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<WasmBuildProbabilityDistributedVerifier, WasmCpuSearchError> {
        WasmBuildProbabilityDistributedVerifier::new_with_delegated_admission(
            &self.problem,
            field,
            aggregation,
            self.delegate_verifier_admission()?,
        )
    }

    pub fn advance(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, &'static str> {
        self.advance_with_external_retained(control, 0)
            .map_err(WasmCpuSearchError::reason)
    }

    pub fn advance_with_external_retained(
        &mut self,
        control: &ExecutionControl,
        external_retained_bytes: u128,
    ) -> Result<WasmCandidateProducerAdvance, WasmCpuSearchError> {
        if self.finished {
            return Err(WasmCpuSearchError::InvalidProblem {
                reason: "wasm_build_probability_distributed_geometry_already_finished",
            });
        }
        self.validate_external_result_memory(external_retained_bytes)?;
        self.ensure_memory_bound()
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;
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
                            self.producer_memory_bound,
                        )
                    }
                    None => DistributedBuildProbabilitySession::new_with_finesse(
                        &self.problem,
                        spec.field,
                        self.aggregation,
                        self.finesse_metric.requested(),
                        self.producer_memory_bound,
                    ),
                }
                .map_err(map_typed_error)?;
                self.active = Some(ProducerPass {
                    pass_index: spec.pass_index,
                    session,
                });
            }
            let coexisting_retained_bytes = self
                .checked_active_coexisting_retained_bytes()
                .ok_or_else(|| self.producer_memory_projection_error())?;
            let candidate_coexisting_retained_bytes = coexisting_retained_bytes
                .checked_add(external_retained_bytes)
                .ok_or_else(|| self.producer_memory_projection_error())?;
            let candidate_memory_bound = self.candidate_memory_bound()?;
            let pass = self.active.as_mut().expect("active pass was initialized");
            pass.session
                .set_coexisting_retained_bytes(coexisting_retained_bytes);
            match pass
                .session
                .advance_distributed_geometry_with_candidate_memory_guard(
                    pass.pass_index,
                    control,
                    move |active_observed_bytes, checked_future_bytes| {
                        let future = candidate_coexisting_retained_bytes
                            .checked_add(checked_future_bytes)
                            .ok_or_else(|| {
                                WasmExactSearchError::ResourceAdmission(
                                    candidate_memory_bound.ensure(u128::MAX, 1).expect_err(
                                        "checked candidate packet future overflow is unavailable",
                                    ),
                                )
                            })?;
                        candidate_memory_bound
                            .ensure(active_observed_bytes, future)
                            .map_err(WasmExactSearchError::ResourceAdmission)
                    },
                )
                .map_err(map_typed_error)?
            {
                WasmCandidateProducerAdvance::Pending => {
                    return Ok(WasmCandidateProducerAdvance::Pending);
                }
                WasmCandidateProducerAdvance::Candidate(candidate) => {
                    return Ok(WasmCandidateProducerAdvance::Candidate(candidate));
                }
                WasmCandidateProducerAdvance::Cancelled => {
                    return Ok(WasmCandidateProducerAdvance::Cancelled);
                }
                WasmCandidateProducerAdvance::Completed(summary) => {
                    let mut pass = self.active.take().expect("active pass exists");
                    pass.session.prepare_distributed_finalizer();
                    self.finalizers.push(pass.session);
                    self.summaries.push(summary);
                    self.ensure_memory_bound().map_err(|resource_report| {
                        WasmCpuSearchError::ResourceAdmission { resource_report }
                    })?;
                    self.validate_external_result_memory(external_retained_bytes)?;
                }
            }
        }
    }

    fn candidate_memory_bound(&self) -> Result<ExecutionMemoryBound, WasmCpuSearchError> {
        let aggregate_cap = self
            .aggregate_budget
            .merger_cap_bytes()
            .ok_or_else(|| self.producer_memory_projection_error())?;
        self.execution_admission
            .memory_bound()
            .with_cap(aggregate_cap)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })
    }

    fn producer_memory_projection_error(&self) -> WasmCpuSearchError {
        WasmCpuSearchError::ResourceAdmission {
            resource_report: self
                .execution_admission
                .memory_bound()
                .ensure(u128::MAX, 1)
                .expect_err("checked producer memory projection overflow is unavailable"),
        }
    }

    pub fn into_merger(self) -> Result<WasmBuildProbabilityDistributedResultMerger, &'static str> {
        if !self.finished {
            return Err("wasm_build_probability_distributed_geometry_not_finished");
        }
        self.ensure_memory_bound()
            .map_err(|_| "wasm_build_probability_aggregate_memory_budget_exceeded")?;
        let merger_memory_bound = self
            .execution_admission
            .memory_bound()
            .with_cap(
                self.aggregate_budget
                    .merger_cap_bytes()
                    .ok_or("wasm_build_probability_aggregate_memory_projection_overflow")?,
            )
            .map_err(|_| "wasm_build_probability_aggregate_memory_budget_exceeded")?;
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
            finesse_metric: self.finesse_metric,
            finesse_pattern_knowledge: self.finesse_pattern_knowledge,
            _execution_admission: self.execution_admission,
            memory_bound: merger_memory_bound,
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

    fn ensure_memory_bound(&self) -> Result<(), clearra_core_domain::resource::ResourceReport> {
        let observed = self.checked_retained_bytes().ok_or_else(|| {
            self.producer_memory_bound
                .ensure(u128::MAX, 1)
                .expect_err("checked producer storage overflow is unavailable")
        })?;
        self.producer_memory_bound.ensure(observed, 0)
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        let mut observed =
            (self.finalizers.capacity() as u128)
                .checked_mul(core::mem::size_of::<DistributedBuildProbabilitySession>() as u128)
                .and_then(|bytes| {
                    bytes.checked_add((self.summaries.capacity() as u128).checked_mul(
                        core::mem::size_of::<WasmDistributedGeometrySummary>() as u128,
                    )?)
                })
                .and_then(|bytes| {
                    bytes.checked_add(
                        (self.pending.capacity() as u128)
                            .checked_mul(core::mem::size_of::<ProducerPassSpec>() as u128)?,
                    )
                })?;
        for summary in &self.summaries {
            observed = observed.checked_add(checked_backend_execution_nested_retained_bytes(
                &summary.backend_execution,
            )?)?;
        }
        if let Some(active) = &self.active {
            observed = observed.checked_add(active.session.checked_retained_bytes()?)?;
        }
        for pass in &self.finalizers {
            observed = observed.checked_add(pass.checked_retained_bytes()?)?;
        }
        Some(observed)
    }

    fn checked_active_coexisting_retained_bytes(&self) -> Option<u128> {
        let active = self.active.as_ref()?;
        self.checked_retained_bytes()?
            .checked_sub(active.session.checked_retained_bytes()?)
    }

    pub fn validate_external_result_memory(
        &self,
        external_result_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        let aggregate_cap = self.aggregate_budget.merger_cap_bytes().ok_or_else(|| {
            WasmCpuSearchError::ResourceAdmission {
                resource_report: self
                    .execution_admission
                    .memory_bound()
                    .ensure(u128::MAX, 1)
                    .expect_err("checked aggregate cap overflow is unavailable"),
            }
        })?;
        let aggregate_bound = self
            .execution_admission
            .memory_bound()
            .with_cap(aggregate_cap)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;
        let observed =
            self.checked_retained_bytes()
                .ok_or_else(|| WasmCpuSearchError::ResourceAdmission {
                    resource_report: aggregate_bound
                        .ensure(u128::MAX, 1)
                        .expect_err("checked producer storage overflow is unavailable"),
                })?;
        aggregate_bound
            .ensure(observed, external_result_bytes)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })
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
    _execution_admission: ExecutionAdmission,
    memory_bound: ExecutionMemoryBound,
    finished: bool,
}

impl WasmBuildProbabilityDistributedVerifier {
    pub fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, &'static str> {
        Self::new_typed(problem, field, aggregation).map_err(WasmCpuSearchError::reason)
    }

    pub fn new_typed(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmCpuSearchError> {
        let execution_admission = admit_budget_bound_search_execution(problem, 1)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;
        Self::new_with_delegated_admission(problem, field, aggregation, execution_admission)
    }

    pub(crate) fn new_with_delegated_admission(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        execution_admission: ExecutionAdmission,
    ) -> Result<Self, WasmCpuSearchError> {
        let memory_bound = execution_admission.memory_bound();
        let original = field.original_only();
        let mirrored = field
            .includes_applicable_horizontal_mirror()
            .then(|| original.mirrored_horizontally())
            .filter(|candidate| *candidate != original);
        let pass_count = usize::from(mirrored.is_some()) + 1;
        execution_admission
            .ensure_plan(
                ExecutionAdmissionPlan::build_probability(problem, pass_count),
                0,
            )
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;
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
            completed_results: Vec::with_capacity(pass_count),
            completed_progress: WasmDistributedProgress::default(),
            _execution_admission: execution_admission,
            memory_bound,
            finished: false,
        })
    }

    pub fn consume(
        &mut self,
        candidate: &WasmCandidatePacket,
        control: &ExecutionControl,
    ) -> Result<(), &'static str> {
        self.consume_with_external_retained(candidate, control, 0)
            .map_err(|error| match error {
                WasmCpuSearchError::ResourceAdmission { .. } => {
                    "wasm_build_probability_aggregate_memory_budget_exceeded"
                }
                _ => error.reason(),
            })
    }

    /// Consumes one borrowed worker candidate while every caller-owned raw,
    /// decoded, current-candidate, and sibling-candidate buffer represented by
    /// `external_retained_bytes` remains live.
    pub fn consume_with_external_retained(
        &mut self,
        candidate: &WasmCandidatePacket,
        control: &ExecutionControl,
        external_retained_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        if self.finished {
            return Err(WasmCpuSearchError::InvalidProblem {
                reason: "wasm_build_probability_distributed_verifier_already_finished",
            });
        }
        self.ensure_memory_bound_with_future(external_retained_bytes)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;
        self.activate_pass(candidate.pass_index(), external_retained_bytes)?;
        self.ensure_memory_bound_with_future(external_retained_bytes)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;
        let coexisting = self
            .checked_active_coexisting_retained_bytes()
            .and_then(|bytes| bytes.checked_add(external_retained_bytes))
            .ok_or_else(|| WasmCpuSearchError::ResourceAdmission {
                resource_report: self
                    .memory_bound
                    .ensure(u128::MAX, 1)
                    .expect_err("checked verifier coexisting-byte overflow is unavailable"),
            })?;
        let active = &mut self
            .active_pass
            .as_mut()
            .expect("requested pass was activated")
            .1;
        active.set_coexisting_retained_bytes(coexisting);
        active
            .process_external_candidate(candidate, control)
            .map_err(map_typed_error)?;
        self.ensure_memory_bound_with_future(external_retained_bytes)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })
    }

    pub fn finish(&mut self) -> Result<Vec<CoreExecutionResult>, &'static str> {
        if self.finished {
            return Err("wasm_build_probability_distributed_verifier_already_finished");
        }
        self.finish_active_pass(0)
            .map_err(WasmCpuSearchError::reason)?;
        self.ensure_memory_bound()
            .map_err(|_| "wasm_build_probability_aggregate_memory_budget_exceeded")?;
        self.finished = true;
        Ok(core::mem::take(&mut self.completed_results))
    }

    pub fn validate_external_result_memory(
        &self,
        external_result_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        self.ensure_memory_bound_with_future(external_result_bytes)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })
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

    fn activate_pass(
        &mut self,
        pass_index: u8,
        external_retained_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        let pass_position = usize::from(pass_index);
        let field =
            *self
                .pass_fields
                .get(pass_position)
                .ok_or(WasmCpuSearchError::InvalidProblem {
                    reason: "wasm_build_probability_distributed_pass_invalid",
                })?;
        if let Some((active_index, _)) = &self.active_pass {
            if *active_index == pass_index {
                return Ok(());
            }
            if *active_index > pass_index {
                return Err(WasmCpuSearchError::InvalidProblem {
                    reason: "wasm_build_probability_distributed_pass_out_of_order",
                });
            }
            self.finish_active_pass(external_retained_bytes)?;
        }
        let new_session_coexisting = self
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(external_retained_bytes))
            .ok_or_else(|| self.verifier_memory_projection_error())?;
        let session = match self.shared_supply_catalog.as_ref() {
            Some(shared) => {
                DistributedBuildProbabilitySession::new_with_shared_supply_catalog_and_coexisting_retained_bytes(
                    &self.problem,
                    field,
                    self.aggregation,
                    true,
                    shared,
                    self.memory_bound,
                    new_session_coexisting,
                )
            }
            None => {
                DistributedBuildProbabilitySession::new_external_geometry_with_coexisting_retained_bytes(
                    &self.problem,
                    field,
                    self.aggregation,
                    self.memory_bound,
                    new_session_coexisting,
                )
            }
        }
        .map_err(map_typed_error)?;
        if self.shared_supply_catalog.is_none() {
            self.shared_supply_catalog = session.shared_supply_catalog();
        }
        self.active_pass = Some((pass_index, session));
        Ok(())
    }

    fn finish_active_pass(
        &mut self,
        external_retained_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        if self.active_pass.is_none() {
            return Ok(());
        }
        let coexisting = self
            .checked_active_coexisting_retained_bytes()
            .and_then(|bytes| bytes.checked_add(external_retained_bytes))
            .ok_or_else(|| self.verifier_memory_projection_error())?;
        let Some((pass_index, mut pass)) = self.active_pass.take() else {
            return Ok(());
        };
        pass.set_coexisting_retained_bytes(coexisting);
        self.completed_progress.merge(pass.progress());
        self.completed_progress.pass_index = usize::from(pass_index);
        let result = pass
            .complete_distributed_worker()
            .map_err(map_typed_error)?;
        drop(pass);

        // The completed worker surface is live outside `self` until it is
        // pushed below. Guard both that owner and the pass-index field/report
        // rebuild before allocating either replacement String.
        let mut pass_index_digits = [0_u8; 3];
        let pass_index_value = decimal_u8(pass_index, &mut pass_index_digits);
        let borrowed_fields = [("build_distributed_pass_index", pass_index_value)];
        let borrowed_projection = result
            .checked_borrowed_field_replacement_projection(&borrowed_fields)
            .ok_or(WasmCpuSearchError::InvalidProblem {
                reason: "wasm_build_probability_aggregate_memory_projection_overflow",
            })?;
        let checked_future = super::build_probability::checked_public_result_bytes(&result)
            .and_then(|bytes| bytes.checked_add(borrowed_projection.required_future_bytes))
            .and_then(|bytes| bytes.checked_add(external_retained_bytes))
            .ok_or_else(|| self.verifier_memory_projection_error())?;
        self.ensure_memory_bound_with_future(checked_future)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })?;

        let fields = self.try_build_pass_index_replacement_fields(
            &result,
            pass_index_value,
            external_retained_bytes,
        )?;
        let result = result
            .try_with_replaced_fields_with_memory_guard(fields, |live, future| {
                let checked_future = super::build_probability::checked_public_result_bytes(live)
                    .and_then(|bytes| bytes.checked_add(future))
                    .and_then(|bytes| bytes.checked_add(external_retained_bytes))
                    .ok_or(WasmCpuSearchError::InvalidProblem {
                        reason: "wasm_build_probability_aggregate_memory_projection_overflow",
                    })?;
                self.ensure_memory_bound_with_future(checked_future)
                    .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission {
                        resource_report,
                    })
            })
            .map_err(|error| {
                match error {
                crate::core_execution_result::CoreResultFieldReplacementError::ProjectionOverflow
                | crate::core_execution_result::CoreResultFieldReplacementError::AllocationFailed {
                    ..
                } => WasmCpuSearchError::InvalidProblem {
                    reason: "wasm_build_probability_aggregate_memory_projection_overflow",
                },
                crate::core_execution_result::CoreResultFieldReplacementError::MemoryGuard(
                    error,
                ) => error,
            }
            })?;
        self.completed_results.push(result);
        self.ensure_memory_bound_with_future(external_retained_bytes)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })
    }

    fn try_build_pass_index_replacement_fields(
        &self,
        result: &CoreExecutionResult,
        pass_index_value: &str,
        external_retained_bytes: u128,
    ) -> Result<Vec<(String, String)>, WasmCpuSearchError> {
        const PASS_INDEX_FIELD: &str = "build_distributed_pass_index";

        let result_retained_bytes = super::build_probability::checked_public_result_bytes(result)
            .ok_or_else(|| self.verifier_memory_projection_error())?;
        let requested_field_backing_bytes = core::mem::size_of::<(String, String)>() as u128;
        let requested_key_bytes = PASS_INDEX_FIELD.len() as u128;
        let requested_value_bytes = pass_index_value.len() as u128;
        let authorize = |local_retained_bytes: u128,
                         checked_future_bytes: u128|
         -> Result<(), WasmCpuSearchError> {
            let checked_future = result_retained_bytes
                .checked_add(external_retained_bytes)
                .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                .and_then(|bytes| bytes.checked_add(checked_future_bytes))
                .ok_or_else(|| self.verifier_memory_projection_error())?;
            self.ensure_memory_bound_with_future(checked_future)
                .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission {
                    resource_report,
                })
        };

        let requested_total_bytes = requested_field_backing_bytes
            .checked_add(requested_key_bytes)
            .and_then(|bytes| bytes.checked_add(requested_value_bytes))
            .ok_or_else(|| self.verifier_memory_projection_error())?;
        authorize(0, requested_total_bytes)?;

        let allocation_error = || WasmCpuSearchError::InvalidProblem {
            reason: "wasm_build_probability_pass_field_storage_unavailable",
        };
        let mut fields = Vec::new();
        fields
            .try_reserve_exact(1)
            .map_err(|_| allocation_error())?;
        let actual_field_backing_bytes = (fields.capacity() as u128)
            .checked_mul(core::mem::size_of::<(String, String)>() as u128)
            .ok_or_else(|| self.verifier_memory_projection_error())?;
        let requested_string_bytes = requested_key_bytes
            .checked_add(requested_value_bytes)
            .ok_or_else(|| self.verifier_memory_projection_error())?;
        authorize(actual_field_backing_bytes, requested_string_bytes)?;

        let mut key = String::new();
        key.try_reserve_exact(PASS_INDEX_FIELD.len())
            .map_err(|_| allocation_error())?;
        let actual_key_bytes = key.capacity() as u128;
        let actual_fields_and_key = actual_field_backing_bytes
            .checked_add(actual_key_bytes)
            .ok_or_else(|| self.verifier_memory_projection_error())?;
        authorize(actual_fields_and_key, requested_value_bytes)?;
        key.push_str(PASS_INDEX_FIELD);

        let mut value = String::new();
        value
            .try_reserve_exact(pass_index_value.len())
            .map_err(|_| allocation_error())?;
        let actual_local_bytes = actual_fields_and_key
            .checked_add(value.capacity() as u128)
            .ok_or_else(|| self.verifier_memory_projection_error())?;
        authorize(actual_local_bytes, 0)?;
        value.push_str(pass_index_value);

        fields.push((key, value));
        let final_actual_bytes = (fields.capacity() as u128)
            .checked_mul(core::mem::size_of::<(String, String)>() as u128)
            .and_then(|bytes| bytes.checked_add(fields[0].0.capacity() as u128))
            .and_then(|bytes| bytes.checked_add(fields[0].1.capacity() as u128))
            .ok_or_else(|| self.verifier_memory_projection_error())?;
        authorize(final_actual_bytes, 0)?;
        Ok(fields)
    }

    fn verifier_memory_projection_error(&self) -> WasmCpuSearchError {
        WasmCpuSearchError::ResourceAdmission {
            resource_report: self
                .memory_bound
                .ensure(u128::MAX, 1)
                .expect_err("checked verifier memory projection overflow is unavailable"),
        }
    }

    fn ensure_memory_bound(&self) -> Result<(), clearra_core_domain::resource::ResourceReport> {
        self.ensure_memory_bound_with_future(0)
    }

    fn ensure_memory_bound_with_future(
        &self,
        checked_future_bytes: u128,
    ) -> Result<(), clearra_core_domain::resource::ResourceReport> {
        let observed = self.checked_retained_bytes().ok_or_else(|| {
            self.memory_bound
                .ensure(u128::MAX, 1)
                .expect_err("checked verifier storage overflow is unavailable")
        })?;
        self.memory_bound.ensure(observed, checked_future_bytes)
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        let mut observed = (self.pass_fields.capacity() as u128)
            .checked_mul(core::mem::size_of::<BuildProbabilityField>() as u128)
            .and_then(|bytes| {
                bytes.checked_add(
                    (self.completed_results.capacity() as u128)
                        .checked_mul(core::mem::size_of::<CoreExecutionResult>() as u128)?,
                )
            })?;
        if let Some((_, active)) = &self.active_pass {
            observed = observed.checked_add(active.checked_retained_bytes()?)?;
        }
        for result in &self.completed_results {
            let nested = result.checked_resource_retained_bytes()?;
            let nested_without_inline =
                nested.checked_sub(core::mem::size_of::<CoreExecutionResult>() as u128)?;
            observed = observed.checked_add(nested_without_inline)?;
        }
        Some(observed)
    }

    fn checked_active_coexisting_retained_bytes(&self) -> Option<u128> {
        let mut retained = (self.pass_fields.capacity() as u128)
            .checked_mul(core::mem::size_of::<BuildProbabilityField>() as u128)?
            .checked_add(
                (self.completed_results.capacity() as u128)
                    .checked_mul(core::mem::size_of::<CoreExecutionResult>() as u128)?,
            )?;
        let result_inline = core::mem::size_of::<CoreExecutionResult>() as u128;
        for result in &self.completed_results {
            retained = retained.checked_add(
                result
                    .checked_resource_retained_bytes()?
                    .checked_sub(result_inline)?,
            )?;
        }
        Some(retained)
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
    finesse_metric: FinesseMetric,
    finesse_pattern_knowledge: FinessePatternKnowledge,
    _execution_admission: ExecutionAdmission,
    memory_bound: ExecutionMemoryBound,
}

impl WasmBuildProbabilityDistributedResultMerger {
    pub fn absorb(&mut self, result: &CoreExecutionResult) -> Result<(), &'static str> {
        let external_result_bytes = Self::public_result_retained_bytes(result)
            .ok_or("wasm_build_probability_aggregate_memory_projection_overflow")?;
        self.absorb_with_external_retained(result, external_result_bytes)
            .map_err(WasmCpuSearchError::reason)
    }

    /// Absorbs one borrowed worker result while `external_result_bytes`
    /// accounts for that complete result owner plus any caller-owned raw or
    /// sibling batch storage that remains live.
    pub fn absorb_with_external_retained(
        &mut self,
        result: &CoreExecutionResult,
        external_result_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        let source_result_bytes = result
            .checked_resource_retained_bytes()
            .ok_or_else(|| self.absorb_projection_overflow())?;
        if external_result_bytes < source_result_bytes {
            return Err(WasmCpuSearchError::InvalidProblem {
                reason: "wasm_build_probability_external_result_bytes_below_result",
            });
        }
        let nested_clone_future = Self::checked_spin_clone_nested_future_bytes(result)
            .ok_or_else(|| self.spin_clone_projection_overflow())?;
        let clone_future = self
            .checked_spin_clone_future_bytes(result)
            .ok_or_else(|| self.spin_clone_projection_overflow())?;
        let validation_and_absorb_future =
            checked_worker_validation_and_absorb_future_bytes(result, clone_future).ok_or_else(
                || WasmCpuSearchError::ResourceAdmission {
                    resource_report: self
                        .memory_bound
                        .ensure(u128::MAX, 1)
                        .expect_err("checked worker validation projection overflow is unavailable"),
                },
            )?;
        self.validate_external_result_memory(external_result_bytes, validation_and_absorb_future)?;
        let pass_index = exact_usize_field(
            result,
            "build_distributed_pass_index",
            "wasm_build_probability_distributed_pass_index_invalid",
        )
        .map_err(map_typed_error)?;
        if pass_index >= self.passes.len() {
            return Err(WasmCpuSearchError::InvalidProblem {
                reason: "wasm_build_probability_distributed_pass_invalid",
            });
        }
        let incoming = result.postprocess_spin_coverages();
        let target_len = self
            .spin_coverages
            .len()
            .checked_add(incoming.len())
            .ok_or_else(|| WasmCpuSearchError::ResourceAdmission {
                resource_report: self
                    .memory_bound
                    .ensure(u128::MAX, 1)
                    .expect_err("checked spin clone count overflow is unavailable"),
            })?;
        if self.spin_coverages.capacity() < target_len {
            self.spin_coverages
                .try_reserve_exact(target_len.saturating_sub(self.spin_coverages.len()))
                .map_err(|_| WasmCpuSearchError::ResourceAdmission {
                    resource_report: self
                        .memory_bound
                        .ensure(self.memory_bound.cap_bytes(), 1)
                        .expect_err("spin clone allocation failure is unavailable"),
                })?;
        }
        let post_reserve_future =
            checked_worker_validation_and_absorb_future_bytes(result, nested_clone_future)
                .ok_or_else(|| self.spin_clone_projection_overflow())?;
        self.validate_external_result_memory(external_result_bytes, post_reserve_future)?;
        let active_pass_retained_bytes = self.passes[pass_index]
            .checked_retained_bytes()
            .ok_or_else(|| self.absorb_projection_overflow())?;
        let merger_coexisting_retained_bytes = self
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_sub(active_pass_retained_bytes))
            .ok_or_else(|| self.absorb_projection_overflow())?;
        let absorb_coexisting_retained_bytes = merger_coexisting_retained_bytes
            .checked_add(external_result_bytes)
            .ok_or_else(|| self.absorb_projection_overflow())?;
        let memory_bound = self.memory_bound;
        self.passes[pass_index].set_coexisting_retained_bytes(absorb_coexisting_retained_bytes);
        let absorb_result = self.passes[pass_index].absorb_distributed_result_with_memory_guard(
            result,
            move |active_observed_bytes, checked_future_bytes| {
                let future = absorb_coexisting_retained_bytes
                    .checked_add(checked_future_bytes)
                    .ok_or_else(|| {
                        WasmExactSearchError::ResourceAdmission(
                            memory_bound.ensure(u128::MAX, 1).expect_err(
                                "checked distributed absorb future overflow is unavailable",
                            ),
                        )
                    })?;
                memory_bound
                    .ensure(active_observed_bytes, future)
                    .map_err(WasmExactSearchError::ResourceAdmission)
            },
        );
        // The raw/decoded source owner belongs to this call. Restore the
        // pass's durable coexisting authority even when absorb rejects the
        // payload so later finish work never retains a stale transfer charge.
        self.passes[pass_index].set_coexisting_retained_bytes(merger_coexisting_retained_bytes);
        absorb_result.map_err(map_typed_error)?;
        self.validate_external_result_memory(external_result_bytes, nested_clone_future)?;
        for coverage in incoming {
            let cloned =
                self.try_clone_spin_coverage_with_memory_guard(coverage, external_result_bytes)?;
            self.spin_coverages.push(cloned);
            self.validate_external_result_memory(external_result_bytes, 0)?;
        }
        self.validate_external_result_memory(external_result_bytes, 0)
    }

    fn checked_spin_clone_future_bytes(&self, result: &CoreExecutionResult) -> Option<u128> {
        let incoming = result.postprocess_spin_coverages();
        let target_len = self.spin_coverages.len().checked_add(incoming.len())?;
        let mut future = if self.spin_coverages.capacity() < target_len {
            (target_len as u128)
                .checked_mul(core::mem::size_of::<CorePostProcessSpinCoverage>() as u128)?
        } else {
            0
        };
        for coverage in incoming {
            future = future.checked_add(coverage.checked_clone_nested_bytes()?)?;
        }
        Some(future)
    }

    fn absorb_projection_overflow(&self) -> WasmCpuSearchError {
        WasmCpuSearchError::ResourceAdmission {
            resource_report: self
                .memory_bound
                .ensure(u128::MAX, 1)
                .expect_err("checked distributed absorb projection overflow is unavailable"),
        }
    }

    fn checked_spin_clone_nested_future_bytes(result: &CoreExecutionResult) -> Option<u128> {
        result
            .postprocess_spin_coverages()
            .iter()
            .try_fold(0_u128, |future, coverage| {
                future.checked_add(coverage.checked_clone_nested_bytes()?)
            })
    }

    fn try_clone_spin_coverage_with_memory_guard(
        &self,
        source: &CorePostProcessSpinCoverage,
        external_result_bytes: u128,
    ) -> Result<CorePostProcessSpinCoverage, WasmCpuSearchError> {
        let mut target_id = String::new();
        self.validate_spin_clone_local_memory(
            external_result_bytes,
            0,
            source.target_id().len() as u128,
        )?;
        target_id
            .try_reserve_exact(source.target_id().len())
            .map_err(|_| self.spin_clone_allocation_failure())?;
        let mut local_nested_bytes = target_id.capacity() as u128;
        self.validate_spin_clone_local_memory(external_result_bytes, local_nested_bytes, 0)?;
        target_id.push_str(source.target_id());

        let requested_word_bytes = (source.covered_pattern_words().len() as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)
            .ok_or_else(|| self.spin_clone_projection_overflow())?;
        self.validate_spin_clone_local_memory(
            external_result_bytes,
            local_nested_bytes,
            requested_word_bytes,
        )?;
        let mut covered_pattern_words = Vec::new();
        covered_pattern_words
            .try_reserve_exact(source.covered_pattern_words().len())
            .map_err(|_| self.spin_clone_allocation_failure())?;
        local_nested_bytes = (covered_pattern_words.capacity() as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)
            .and_then(|bytes| bytes.checked_add(target_id.capacity() as u128))
            .ok_or_else(|| self.spin_clone_projection_overflow())?;
        self.validate_spin_clone_local_memory(external_result_bytes, local_nested_bytes, 0)?;
        covered_pattern_words.extend_from_slice(source.covered_pattern_words());

        let requested_candidate_slots = (source.candidate_keys().len() as u128)
            .checked_mul(core::mem::size_of::<String>() as u128)
            .ok_or_else(|| self.spin_clone_projection_overflow())?;
        self.validate_spin_clone_local_memory(
            external_result_bytes,
            local_nested_bytes,
            requested_candidate_slots,
        )?;
        let mut candidate_keys = Vec::new();
        candidate_keys
            .try_reserve_exact(source.candidate_keys().len())
            .map_err(|_| self.spin_clone_allocation_failure())?;
        local_nested_bytes = local_nested_bytes
            .checked_add(
                (candidate_keys.capacity() as u128)
                    .checked_mul(core::mem::size_of::<String>() as u128)
                    .ok_or_else(|| self.spin_clone_projection_overflow())?,
            )
            .ok_or_else(|| self.spin_clone_projection_overflow())?;
        self.validate_spin_clone_local_memory(external_result_bytes, local_nested_bytes, 0)?;

        for source_key in source.candidate_keys() {
            self.validate_spin_clone_local_memory(
                external_result_bytes,
                local_nested_bytes,
                source_key.len() as u128,
            )?;
            let mut candidate_key = String::new();
            candidate_key
                .try_reserve_exact(source_key.len())
                .map_err(|_| self.spin_clone_allocation_failure())?;
            local_nested_bytes = local_nested_bytes
                .checked_add(candidate_key.capacity() as u128)
                .ok_or_else(|| self.spin_clone_projection_overflow())?;
            self.validate_spin_clone_local_memory(external_result_bytes, local_nested_bytes, 0)?;
            candidate_key.push_str(source_key);
            candidate_keys.push(candidate_key);
        }

        Ok(CorePostProcessSpinCoverage::new(
            target_id,
            source.pass_index(),
            source.pattern_count(),
            covered_pattern_words,
            candidate_keys,
            source.witnessed_pattern_count(),
            source.complete(),
        ))
    }

    fn validate_spin_clone_local_memory(
        &self,
        external_result_bytes: u128,
        local_nested_bytes: u128,
        checked_next_allocation_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        let checked_future_bytes = local_nested_bytes
            .checked_add(checked_next_allocation_bytes)
            .ok_or_else(|| self.spin_clone_projection_overflow())?;
        self.validate_external_result_memory(external_result_bytes, checked_future_bytes)
    }

    fn spin_clone_projection_overflow(&self) -> WasmCpuSearchError {
        WasmCpuSearchError::ResourceAdmission {
            resource_report: self
                .memory_bound
                .ensure(u128::MAX, 1)
                .expect_err("checked spin clone projection overflow is unavailable"),
        }
    }

    fn spin_clone_allocation_failure(&self) -> WasmCpuSearchError {
        WasmCpuSearchError::ResourceAdmission {
            resource_report: self
                .memory_bound
                .ensure(self.memory_bound.cap_bytes(), 1)
                .expect_err("spin clone allocation failure is unavailable"),
        }
    }

    pub fn public_result_retained_bytes(result: &CoreExecutionResult) -> Option<u128> {
        super::build_probability::checked_public_result_bytes(result)
    }

    pub fn validate_external_result_memory(
        &self,
        external_result_bytes: u128,
        checked_future_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        let future = external_result_bytes
            .checked_add(checked_future_bytes)
            .ok_or_else(|| WasmCpuSearchError::ResourceAdmission {
                resource_report: self
                    .memory_bound
                    .ensure(u128::MAX, 1)
                    .expect_err("checked external result storage overflow is unavailable"),
            })?;
        self.ensure_memory_bound(future)
            .map_err(|resource_report| WasmCpuSearchError::ResourceAdmission { resource_report })
    }

    pub fn finish(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<CoreExecutionResult, &'static str> {
        self.finish_with_control(summary, workers_used, &ExecutionControl::default())
    }

    pub fn finish_with_control(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, &'static str> {
        self.finish_with_control_and_terminal(summary, workers_used, control, |result, _| result)
    }

    pub fn finish_with_control_and_terminal<R>(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
        control: &ExecutionControl,
        terminal: impl FnOnce(Result<CoreExecutionResult, &'static str>, &Self) -> R,
    ) -> R {
        let result = self.finish_with_control_inner(summary, workers_used, control);
        self.apply_terminal_memory_guard(result, terminal)
    }

    fn apply_terminal_memory_guard<R>(
        &self,
        mut result: Result<CoreExecutionResult, &'static str>,
        terminal: impl FnOnce(Result<CoreExecutionResult, &'static str>, &Self) -> R,
    ) -> R {
        if let Ok(materialized) = result.as_ref() {
            if self.validate_public_result_memory(materialized).is_err() {
                result = Err("wasm_build_probability_aggregate_memory_budget_exceeded");
            }
        }
        terminal(result, self)
    }

    pub fn validate_public_result_memory(
        &self,
        result: &CoreExecutionResult,
    ) -> Result<(), &'static str> {
        self.validate_public_result_memory_with_future(result, 0)
    }

    pub fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        checked_future_bytes: u128,
    ) -> Result<(), &'static str> {
        let retained = super::build_probability::checked_public_result_bytes(result)
            .ok_or("wasm_build_probability_aggregate_memory_projection_overflow")?;
        let future = retained
            .checked_add(checked_future_bytes)
            .ok_or("wasm_build_probability_aggregate_memory_projection_overflow")?;
        self.ensure_memory_bound(future)
            .map_err(|_| "wasm_build_probability_aggregate_memory_budget_exceeded")
    }

    fn finish_with_control_inner(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, &'static str> {
        self.ensure_memory_bound(0)
            .map_err(|_| "wasm_build_probability_aggregate_memory_budget_exceeded")?;
        if self.passes.len() != self.summaries.len() {
            return Err("wasm_build_probability_distributed_summary_mismatch");
        }
        let collect_finesse = self.finesse_metric.requested();
        let result_slot_bytes = (self.passes.len() as u128)
            .checked_mul(core::mem::size_of::<CoreExecutionResult>() as u128)
            .ok_or("wasm_build_probability_aggregate_memory_projection_overflow")?;
        let finesse_slot_bytes = if collect_finesse {
            (self.passes.len() as u128)
                .checked_mul(core::mem::size_of::<FinesseSearchMaterial>() as u128)
                .ok_or("wasm_build_probability_aggregate_memory_projection_overflow")?
        } else {
            0
        };
        self.ensure_memory_bound(
            result_slot_bytes
                .checked_add(finesse_slot_bytes)
                .ok_or("wasm_build_probability_aggregate_memory_projection_overflow")?,
        )
        .map_err(|_| "wasm_build_probability_aggregate_memory_budget_exceeded")?;
        let mut finesse_materials = if collect_finesse {
            Vec::with_capacity(self.passes.len())
        } else {
            Vec::new()
        };
        let mut results = Vec::with_capacity(self.passes.len());
        if collect_finesse {
            // Worker evidence was produced from the pre-finesse build graphs. The
            // coordinator rebuilds exact evidence from the surviving graphs below.
            self.spin_coverages.clear();
            for pass_index in 0..self.passes.len() {
                let coexisting_retained_bytes = self
                    .checked_pass_finalization_coexisting_retained_bytes(
                        pass_index,
                        &results,
                        &finesse_materials,
                    )
                    .ok_or("wasm_build_probability_aggregate_memory_projection_overflow")?;
                self.passes[pass_index].set_coexisting_retained_bytes(coexisting_retained_bytes);
                self.passes[pass_index]
                    .annotate_finesse(control)
                    .map_err(map_error)?;
                let material_future = self.passes[pass_index]
                    .checked_finesse_search_material_future_bytes()
                    .ok_or("wasm_finesse_search_material_projection_overflow")?;
                let local_retained = checked_core_result_vec_retained_bytes(&results)
                    .and_then(|bytes| {
                        bytes.checked_add(checked_finesse_material_vec_retained_bytes(
                            &finesse_materials,
                        )?)
                    })
                    .and_then(|bytes| bytes.checked_add(material_future))
                    .ok_or("wasm_finesse_search_material_projection_overflow")?;
                self.ensure_memory_bound(local_retained)
                    .map_err(|_| "wasm_build_probability_aggregate_memory_budget_exceeded")?;
                finesse_materials.push(
                    self.passes[pass_index]
                        .finesse_search_material()
                        .map_err(map_error)?,
                );
                let local_retained = checked_core_result_vec_retained_bytes(&results)
                    .and_then(|bytes| {
                        bytes.checked_add(checked_finesse_material_vec_retained_bytes(
                            &finesse_materials,
                        )?)
                    })
                    .ok_or("wasm_finesse_search_material_projection_overflow")?;
                self.ensure_memory_bound(local_retained)
                    .map_err(|_| "wasm_build_probability_aggregate_memory_budget_exceeded")?;
            }
        }
        for pass_index in 0..self.passes.len() {
            let coexisting_retained_bytes = self
                .checked_pass_finalization_coexisting_retained_bytes(
                    pass_index,
                    &results,
                    &finesse_materials,
                )
                .ok_or("wasm_build_probability_aggregate_memory_projection_overflow")?;
            self.passes[pass_index].set_coexisting_retained_bytes(coexisting_retained_bytes);
            let summary = &self.summaries[pass_index];
            match self.passes[pass_index]
                .complete_distributed_geometry(summary, workers_used)
                .map_err(map_error)?
            {
                BuildProbabilityAdvance::Completed(result) => results.push(result),
                BuildProbabilityAdvance::Pending => {
                    return Err("wasm_build_probability_distributed_finish_pending");
                }
                BuildProbabilityAdvance::Cancelled => return Err("wasm_cpu_search_cancelled"),
            }
        }
        let mut result = merge_symmetry_results_with_memory_guard(
            results,
            self.mirror_included,
            self.mirror_distinct,
            &self.pattern_weights,
            self.aggregation.requests_spin_coverage() || self.execution_constraints_requested,
            |source_bytes, future_bytes| {
                let finesse_bytes = checked_finesse_material_vec_retained_bytes(&finesse_materials)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_symmetry_memory_projection_overflow",
                    ))?;
                let checked_future = source_bytes
                    .checked_add(future_bytes)
                    .and_then(|bytes| bytes.checked_add(finesse_bytes))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_symmetry_memory_projection_overflow",
                    ))?;
                self.ensure_memory_bound(checked_future)
                    .map_err(WasmExactSearchError::ResourceAdmission)
            },
        )
        .map_err(map_error)?;
        if collect_finesse {
            result = attach_finesse_report_with_memory_guard(
                result,
                finesse_materials,
                self.finesse_metric,
                self.finesse_pattern_knowledge,
                control,
                |live, future| {
                    let live_bytes = super::build_probability::checked_public_result_bytes(live)
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_finesse_report_live_result_memory_projection_unavailable",
                        ))?;
                    let checked_future = live_bytes.checked_add(future).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_finesse_report_live_and_future_memory_projection_overflow",
                        ),
                    )?;
                    self.ensure_memory_bound(checked_future)
                        .map_err(WasmExactSearchError::ResourceAdmission)
                },
            )
            .map_err(map_error)?;
        }
        let result =
            result.with_postprocess_spin_coverages(core::mem::take(&mut self.spin_coverages));
        apply_backend_execution_with_memory_guard(
            result,
            &summary.backend_execution,
            |live, future| {
                let checked_future = super::build_probability::checked_public_result_bytes(live)
                    .and_then(|bytes| bytes.checked_add(future))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_backend_memory_projection_overflow",
                    ))?;
                self.ensure_memory_bound(checked_future)
                    .map_err(WasmExactSearchError::ResourceAdmission)
            },
        )
        .map_err(map_error)
    }

    fn ensure_memory_bound(
        &self,
        checked_future_bytes: u128,
    ) -> Result<(), clearra_core_domain::resource::ResourceReport> {
        let observed = self.checked_retained_bytes().ok_or_else(|| {
            self.memory_bound
                .ensure(u128::MAX, 1)
                .expect_err("checked merger storage overflow is unavailable")
        })?;
        self.memory_bound.ensure(observed, checked_future_bytes)
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        let mut observed =
            (self.passes.capacity() as u128)
                .checked_mul(core::mem::size_of::<DistributedBuildProbabilitySession>() as u128)
                .and_then(|bytes| {
                    bytes.checked_add((self.summaries.capacity() as u128).checked_mul(
                        core::mem::size_of::<WasmDistributedGeometrySummary>() as u128,
                    )?)
                })
                .and_then(|bytes| {
                    bytes.checked_add(
                        (self.spin_coverages.capacity() as u128).checked_mul(
                            core::mem::size_of::<CorePostProcessSpinCoverage>() as u128,
                        )?,
                    )
                })?
                .checked_add(self.pattern_weights.checked_storage_retained_bytes()?)?;
        for summary in &self.summaries {
            observed = observed.checked_add(checked_backend_execution_nested_retained_bytes(
                &summary.backend_execution,
            )?)?;
        }
        for pass in &self.passes {
            observed = observed.checked_add(pass.checked_retained_bytes()?)?;
        }
        for coverage in &self.spin_coverages {
            observed = observed.checked_add(coverage.checked_nested_retained_bytes()?)?;
        }
        Some(observed)
    }

    fn checked_pass_finalization_coexisting_retained_bytes(
        &self,
        active_pass_index: usize,
        results: &Vec<CoreExecutionResult>,
        finesse_materials: &Vec<FinesseSearchMaterial>,
    ) -> Option<u128> {
        let active = self.passes.get(active_pass_index)?;
        self.checked_retained_bytes()?
            .checked_sub(active.checked_retained_bytes()?)?
            .checked_add(checked_core_result_vec_retained_bytes(results)?)?
            .checked_add(checked_finesse_material_vec_retained_bytes(
                finesse_materials,
            )?)
    }
}

fn checked_backend_execution_nested_retained_bytes(
    execution: &super::distributed::WasmDistributedBackendExecution,
) -> Option<u128> {
    use super::distributed::WasmDistributedBackendExecution;

    match execution {
        WasmDistributedBackendExecution::Cpu
        | WasmDistributedBackendExecution::CpuFallback { .. } => Some(0),
        WasmDistributedBackendExecution::WebGpu {
            adapter_name,
            adapter_backend,
            shader_hash,
            ..
        } => (adapter_name.capacity() as u128)
            .checked_add(adapter_backend.capacity() as u128)?
            .checked_add(shader_hash.capacity() as u128),
    }
}

fn checked_worker_validation_and_absorb_future_bytes(
    result: &CoreExecutionResult,
    spin_clone_future_bytes: u128,
) -> Option<u128> {
    // Both compact and extended absorbers copy the public dense words into a
    // `PatternBitSet` and union it into an arbitrary retained representation.
    // Round through the actual word count so malformed declared universes are
    // still bounded by the storage the absorber can allocate before rejecting
    // their shape.
    let projected_pattern_count = result
        .coverage_pattern_words()
        .len()
        .checked_mul(u64::BITS as usize)?;
    let coverage_scratch = PatternBitSet::checked_external_words_materialize_union_future_bytes(
        projected_pattern_count,
    )?;
    // The caller's `external_result_bytes` already owns the complete source
    // result (and may additionally own its raw transfer batch). Only scratch
    // and clone allocations belong in this future projection.
    spin_clone_future_bytes.checked_add(coverage_scratch)
}

fn decimal_u8<'a>(value: u8, storage: &'a mut [u8; 3]) -> &'a str {
    let len = if value >= 100 {
        storage[0] = b'0' + value / 100;
        storage[1] = b'0' + (value / 10) % 10;
        storage[2] = b'0' + value % 10;
        3
    } else if value >= 10 {
        storage[0] = b'0' + value / 10;
        storage[1] = b'0' + value % 10;
        2
    } else {
        storage[0] = b'0' + value;
        1
    };
    core::str::from_utf8(&storage[..len]).expect("u8 decimal digits are valid UTF-8")
}

fn apply_backend_execution_with_memory_guard(
    result: CoreExecutionResult,
    execution: &super::distributed::WasmDistributedBackendExecution,
    mut memory_guard: impl FnMut(&CoreExecutionResult, u128) -> Result<(), WasmExactSearchError>,
) -> Result<CoreExecutionResult, WasmExactSearchError> {
    use super::distributed::WasmDistributedBackendExecution;

    let hybrid_requested = result.field("backend_requested") == Some("hybrid");
    let cpu_backend = if result.field("board_storage") == Some("board256-canonical") {
        "wasm-cpu-build-probability-extended"
    } else {
        "wasm-cpu-build-probability"
    };

    let borrowed_projection = match execution {
        WasmDistributedBackendExecution::Cpu => return Ok(result),
        WasmDistributedBackendExecution::CpuFallback {
            reason,
            failure_class,
            failure_stage,
            discarded_partial_gpu_result: _,
            original_gpu_result_incomplete: _,
        } => result.checked_borrowed_field_replacement_projection(&[
            ("backend_selected", cpu_backend),
            ("actual_backend", cpu_backend),
            ("backend_fallback_used", "false"),
            ("fallback_used", "false"),
            ("backend_fallback_reason", reason),
            ("fallback_backend", cpu_backend),
            ("gpu_available", "false"),
            ("gpu_disabled_reason", reason),
            ("gpu_trust_state", "fallback-used"),
            ("gpu_failure_class", failure_class),
            ("gpu_failure_stage", failure_stage),
            ("discarded_partial_gpu_result", "false"),
            ("gpu_original_result_incomplete", "false"),
        ]),
        WasmDistributedBackendExecution::WebGpu {
            adapter_index: _,
            adapter_name,
            adapter_type,
            adapter_backend,
            peak_gpu_bytes: _,
            shader_hash,
            shader_version,
            warmup_performed: _,
            session_reused: _,
        } => result.checked_borrowed_field_replacement_projection(&[
            ("backend_selected", "webgpu"),
            ("actual_backend", "webgpu"),
            ("backend_fallback_used", "false"),
            ("fallback_used", "false"),
            ("backend_fallback_reason", "none"),
            ("fallback_backend", "none"),
            ("gpu_available", "false"),
            ("gpu_disabled_reason", "none"),
            ("gpu_trust_state", "gpu-computed-cpu-confirmed"),
            (
                "hybrid_status",
                if hybrid_requested {
                    "gpu-ready"
                } else {
                    "not-requested"
                },
            ),
            ("hybrid_disabled_reason", "none"),
            ("gpu_failure_class", "none"),
            ("gpu_failure_stage", "none"),
            ("discarded_partial_gpu_result", "false"),
            ("gpu_original_result_incomplete", "false"),
            // Decimal maxima make this stack-only borrowed projection an
            // upper bound for every concrete u8/u64 backend value.
            ("gpu_device_selected_index", "255"),
            ("gpu_device_selected_name", adapter_name.as_str()),
            ("gpu_device_selected_type", adapter_type),
            ("gpu_device_selected_backend", adapter_backend.as_str()),
            ("resource_peak_gpu_bytes", "18446744073709551615"),
            ("gpu_shader_hash", shader_hash.as_str()),
            ("gpu_shader_version", shader_version),
            ("gpu_warmup_performed", "false"),
            ("gpu_session_reused", "false"),
        ]),
    }
    .ok_or(WasmExactSearchError::InvalidProblem(
        "wasm_build_probability_backend_memory_projection_overflow",
    ))?;
    memory_guard(&result, borrowed_projection.required_future_bytes)?;

    let replacements = match execution {
        WasmDistributedBackendExecution::Cpu => unreachable!("CPU returned before projection"),
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
            field("fallback_used", true),
            field("backend_fallback_reason", reason),
            field("fallback_backend", cpu_backend),
            field("gpu_available", false),
            field("gpu_disabled_reason", reason),
            field("gpu_trust_state", "fallback-used"),
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
            field("fallback_used", false),
            field("backend_fallback_reason", "none"),
            field("fallback_backend", "none"),
            field("gpu_available", true),
            field("gpu_disabled_reason", "none"),
            field("gpu_trust_state", "gpu-computed-cpu-confirmed"),
            field(
                "hybrid_status",
                if hybrid_requested {
                    "gpu-ready"
                } else {
                    "not-requested"
                },
            ),
            field("hybrid_disabled_reason", "none"),
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
    result
        .try_with_replaced_fields_with_memory_guard(replacements, |live, future| {
            memory_guard(live, future)
        })
        .map_err(|error| match error {
            crate::core_execution_result::CoreResultFieldReplacementError::ProjectionOverflow
            | crate::core_execution_result::CoreResultFieldReplacementError::AllocationFailed {
                ..
            } => WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_backend_memory_projection_overflow",
            ),
            crate::core_execution_result::CoreResultFieldReplacementError::MemoryGuard(error) => {
                error
            }
        })
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
    error.reason()
}

fn map_typed_error(error: WasmExactSearchError) -> WasmCpuSearchError {
    match error {
        WasmExactSearchError::InvalidProblem(reason) => {
            WasmCpuSearchError::InvalidProblem { reason }
        }
        WasmExactSearchError::ResourceAdmission(resource_report) => {
            WasmCpuSearchError::ResourceAdmission { resource_report }
        }
        WasmExactSearchError::Cancelled => WasmCpuSearchError::Cancelled,
    }
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::{
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
            NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity,
        },
    };
    use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{
        PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PcSolutionProbabilityPolicy, PieceWindow, SupplyWindowSize,
    };
    use clearra_problem::{
        BuildProbabilityFinesseRequest, FinessePatternKnowledge, ProblemCompiler,
    };
    use clearra_supply::queue::{
        fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression,
    };

    use super::*;
    use crate::backend::wasm_cpu::build_probability::WasmBuildProbabilitySession;
    use crate::{NormalizedSolutionCoverage, SolutionCoverage};

    #[test]
    fn actual_build_probability_producer_packet_stream_matches_transport_kat() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("KAT problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let mut producer =
            WasmBuildProbabilityCandidateProducer::new_with_finesse_and_verifiers_typed(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
                FinesseMetric::Inputs,
                FinessePatternKnowledge::Both,
                1,
                0,
            )
            .expect("actual producer");
        let control = ExecutionControl::default();
        let mut packets = Vec::new();
        loop {
            match producer.advance(&control).expect("producer advance") {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(packet) => packets.push(packet),
                WasmCandidateProducerAdvance::Completed(_) => break,
                WasmCandidateProducerAdvance::Cancelled => panic!("producer cancelled"),
            }
        }
        assert!(
            !packets.is_empty(),
            "the KAT must bind an actual candidate source"
        );
        assert_eq!(
            crate::canonical_wasm_candidate_packet_batch_sha256(&packets),
            "71cc5dd0ab1d2188d562ab1bddd88ca0e94155e765f07b4d7576fe5a90fb3d9f"
        );
    }

    #[test]
    fn aggregate_budget_conserves_coordinator_and_replica_caps_for_one_two_and_four_workers() {
        for verifier_count in [0, 1, 3] {
            let budget = BuildProbabilityAggregateBudget::new(10_000, 1_000, verifier_count)
                .expect("bounded aggregate budget");
            assert_eq!(budget.replica_count, verifier_count as u128 + 1);
            assert_eq!(budget.merger_cap_bytes(), Some(9_000));
            assert!(budget
                .replica_cap_bytes
                .checked_mul(budget.replica_count)
                .and_then(|bytes| bytes.checked_add(budget.coordinator_reserved_bytes))
                .is_some_and(|projected| projected <= budget.total_cap_bytes));
        }
        assert!(BuildProbabilityAggregateBudget::new(8, 9, 1).is_none());
    }

    #[test]
    fn standalone_verifier_full_build_plan_fails_before_allocation() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(10),
        )
        .with_supply_window_size(SupplyWindowSize::new(10))
        .with_allow_hold(false)
        .with_exact_pieces(Some(10))
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_max_memory_mib(Some(1)));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("search problem");
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xffffffffff, 0, 0, 0])
                .expect("four-row target");

        let error = match WasmBuildProbabilityDistributedVerifier::new_typed(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
        ) {
            Ok(_) => panic!("one-MiB budget must fail before verifier allocation"),
            Err(error) => error,
        };
        let WasmCpuSearchError::ResourceAdmission { resource_report } = error else {
            panic!("expected typed resource admission, got {error:?}");
        };
        assert!(!resource_report.execution_started());
        assert!(!resource_report.result_complete());
        assert_eq!(
            resource_report.execution_availability().reason(),
            Some(clearra_core_domain::resource::ExecutionAvailabilityReason::MemoryBudgetExceeded)
        );
        let availability = resource_report.execution_availability();
        assert_eq!(availability.descriptor_pattern_count(), Some(1_058_400));
        assert_eq!(availability.dense_pattern_count(), Some(1_058_400));
        assert_eq!(availability.required_dense_bytes(), Some(132_304));
        assert_eq!(
            availability.required_memory_bytes(),
            Some(132_304 + 1_058_400 * (core::mem::size_of::<usize>() as u128 * 2))
        );
    }

    #[test]
    fn serial_build_probability_full_plan_fails_before_allocation() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(10),
        )
        .with_supply_window_size(SupplyWindowSize::new(10))
        .with_allow_hold(false)
        .with_exact_pieces(Some(10))
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_max_memory_mib(Some(1)));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("search problem");
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xffffffffff, 0, 0, 0])
                .expect("four-row target");

        let error = match WasmBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
        ) {
            Ok(_) => panic!("one-MiB budget must fail before serial allocation"),
            Err(error) => error,
        };
        let WasmExactSearchError::ResourceAdmission(resource_report) = error else {
            panic!("expected typed resource admission, got {error:?}");
        };
        assert!(!resource_report.execution_started());
        assert!(!resource_report.result_complete());
        assert_eq!(
            resource_report.execution_availability().reason(),
            Some(clearra_core_domain::resource::ExecutionAvailabilityReason::MemoryBudgetExceeded)
        );
        let availability = resource_report.execution_availability();
        assert_eq!(availability.descriptor_pattern_count(), Some(1_058_400));
        assert_eq!(availability.dense_pattern_count(), Some(1_058_400));
        assert_eq!(availability.required_dense_bytes(), Some(132_304));
        assert_eq!(
            availability.required_memory_bytes(),
            Some(132_304 + 1_058_400 * (core::mem::size_of::<usize>() as u128 * 2))
        );
    }

    #[test]
    fn delegated_verifier_rechecks_full_build_plan_without_a_new_memory_lease() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(10),
        )
        .with_supply_window_size(SupplyWindowSize::new(10))
        .with_allow_hold(false)
        .with_exact_pieces(Some(10))
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_max_memory_mib(Some(1)));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("search problem");
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xffffffffff, 0, 0, 0])
                .expect("four-row target");
        let parent = admit_budget_bound_search_execution(&problem, 2)
            .expect("dense-only parent admission fits one MiB");
        let parent_token = parent.lease_token();
        let delegated = parent
            .try_delegate_compute_only_with_memory_cap(1024 * 1024)
            .expect("compute-only delegated admission");
        let delegated_token = delegated.lease_token();
        assert_eq!(delegated_token.parent_epoch(), Some(parent_token.epoch()));
        assert_eq!(delegated_token.grant().memory_bytes, 0);

        let error = match WasmBuildProbabilityDistributedVerifier::new_with_delegated_admission(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            delegated,
        ) {
            Ok(_) => panic!("delegated one-MiB cap must fail before verifier allocation"),
            Err(error) => error,
        };
        let WasmCpuSearchError::ResourceAdmission { resource_report } = error else {
            panic!("expected typed resource admission, got {error:?}");
        };
        assert!(!resource_report.execution_started());
        assert!(!resource_report.result_complete());
        assert_eq!(
            resource_report.execution_availability().reason(),
            Some(clearra_core_domain::resource::ExecutionAvailabilityReason::MemoryBudgetExceeded)
        );
        let availability = resource_report.execution_availability();
        assert_eq!(availability.descriptor_pattern_count(), Some(1_058_400));
        assert_eq!(availability.dense_pattern_count(), Some(1_058_400));
        assert_eq!(availability.required_dense_bytes(), Some(132_304));
        assert_eq!(
            availability.required_memory_bytes(),
            Some(132_304 + 1_058_400 * (core::mem::size_of::<usize>() as u128 * 2))
        );
    }

    fn mixed_spawn_problem() -> (SearchProblem, BuildProbabilityField) {
        let queue = QueuePatternExpression::parse("[OI]!", 2).expect("two queue permutations");
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(24, 0),
            PcQueueInput::pattern_expression(queue),
            PieceWindow::new(2),
        )
        .with_allow_hold(true)
        .with_exact_pieces(Some(1))
        .with_objective(
            ObjectivePolicy::unique().with_back_to_back_preservation(SpinProfileSelection::TSpins),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("search problem");
        let field = BuildProbabilityField::from_words_preserving_height(
            24,
            [0, 0, 0, 0x40_0000],
            [0xf, 0, 0, 0],
        )
        .expect("extended field");
        (problem, field)
    }

    #[test]
    fn producer_rejects_external_worker_results_beyond_the_shared_parent_cap() {
        let (problem, field) = mixed_spawn_problem();
        let producer = WasmBuildProbabilityCandidateProducer::new_with_finesse_typed(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            FinesseMetric::Off,
            FinessePatternKnowledge::Both,
        )
        .expect("producer");
        let error = producer
            .validate_external_result_memory(u128::MAX)
            .expect_err("external result overflow must fail closed");
        let WasmCpuSearchError::ResourceAdmission { resource_report } = error else {
            panic!("expected typed resource admission, got {error:?}");
        };
        assert!(!resource_report.execution_started());
        assert!(!resource_report.result_complete());
    }

    #[test]
    fn verifier_completed_results_count_actual_nested_bytes_at_the_exact_cap() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let mut verifier = WasmBuildProbabilityDistributedVerifier::new_typed(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
        )
        .expect("verifier");
        verifier.completed_results.push(
            CoreExecutionResult::default()
                .with_additional_fields(vec![(
                    "resource_peak_cpu_bytes".to_owned(),
                    "0".to_owned(),
                )])
                .with_normalized_solution_keys(vec!["x".repeat(4_096)]),
        );

        let retained = verifier
            .checked_retained_bytes()
            .expect("checked verifier retained bytes");
        let shallow = (verifier.pass_fields.capacity() as u128)
            * core::mem::size_of::<BuildProbabilityField>() as u128
            + (verifier.completed_results.capacity() as u128)
                * core::mem::size_of::<CoreExecutionResult>() as u128;
        assert!(retained >= shallow + 4_096);

        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");
        verifier.memory_bound = unbounded.with_cap(retained).expect("exact cap");
        verifier
            .ensure_memory_bound()
            .expect("the exact actual retained-byte cap is sufficient");
        verifier.memory_bound = unbounded
            .with_cap(retained - 1)
            .expect("one-byte-short cap");
        verifier
            .ensure_memory_bound()
            .expect_err("one byte below actual nested ownership must fail closed");
    }

    #[test]
    fn verifier_external_result_memory_api_has_an_exact_one_byte_boundary_and_checked_overflow() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let mut verifier = WasmBuildProbabilityDistributedVerifier::new_typed(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
        )
        .expect("verifier");
        let retained = verifier
            .checked_retained_bytes()
            .expect("checked verifier retained bytes");
        let external_result_bytes = 4_096_u128;
        let required = retained
            .checked_add(external_result_bytes)
            .expect("checked verifier plus external ownership");
        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");

        verifier.memory_bound = unbounded.with_cap(required).expect("exact cap");
        verifier
            .validate_external_result_memory(external_result_bytes)
            .expect("the exact verifier plus external-result cap is sufficient");
        verifier.memory_bound = unbounded
            .with_cap(required - 1)
            .expect("one-byte-short cap");
        assert!(matches!(
            verifier
                .validate_external_result_memory(external_result_bytes)
                .expect_err("one byte below the external ownership must fail closed"),
            WasmCpuSearchError::ResourceAdmission { .. }
        ));
        assert!(matches!(
            verifier
                .validate_external_result_memory(u128::MAX)
                .expect_err("checked external ownership overflow must fail closed"),
            WasmCpuSearchError::ResourceAdmission { .. }
        ));
    }

    #[test]
    fn verifier_first_pass_session_creation_counts_external_owners_at_exact_peak() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let external_retained_bytes = 4_321_u128;
        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");
        let attempt = |cap_bytes: u128| {
            let mut verifier = WasmBuildProbabilityDistributedVerifier::new_typed(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
            )
            .expect("first-pass verifier");
            verifier.memory_bound = unbounded.with_cap(cap_bytes).expect("candidate cap");
            verifier.activate_pass(0, external_retained_bytes)
        };
        let mut exact_peak = 1_u128;
        loop {
            match attempt(exact_peak) {
                Ok(()) => break,
                Err(WasmCpuSearchError::ResourceAdmission { resource_report }) => {
                    let required = resource_report
                        .execution_availability()
                        .required_memory_bytes()
                        .expect("resource rejection reports the required peak");
                    assert!(required > exact_peak, "the boundary search advances");
                    exact_peak = required;
                }
                Err(error) => panic!("unexpected first-pass activation error: {error:?}"),
            }
        }
        attempt(exact_peak).expect("the discovered exact first-session peak must fit");
        assert!(matches!(
            attempt(exact_peak - 1),
            Err(WasmCpuSearchError::ResourceAdmission { .. })
        ));
    }

    #[test]
    fn verifier_pass_switch_carries_external_owners_through_finish_and_new_session_peak() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field")
            .with_horizontal_mirror_included(true);
        assert!(field.includes_applicable_horizontal_mirror());
        let external_retained_bytes = 7_777_u128;
        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");

        let attempt = |cap_bytes: u128| {
            let mut verifier = WasmBuildProbabilityDistributedVerifier::new_typed(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
            )
            .expect("pass-switch verifier");
            verifier.activate_pass(0, 0).expect("first pass");
            let bound = unbounded.with_cap(cap_bytes).expect("candidate cap");
            verifier.memory_bound = bound;
            verifier
                .active_pass
                .as_mut()
                .expect("active first pass")
                .1
                .set_memory_bound_for_test(bound);
            verifier.activate_pass(1, external_retained_bytes)
        };

        let mut exact_peak = 1_u128;
        loop {
            match attempt(exact_peak) {
                Ok(()) => break,
                Err(WasmCpuSearchError::ResourceAdmission { resource_report }) => {
                    let required = resource_report
                        .execution_availability()
                        .required_memory_bytes()
                        .expect("resource rejection reports the required peak");
                    assert!(required > exact_peak, "the boundary search advances");
                    exact_peak = required;
                }
                Err(error) => panic!("unexpected pass-switch error: {error:?}"),
            }
        }
        attempt(exact_peak).expect("the discovered exact pass-switch peak must fit");
        assert!(matches!(
            attempt(exact_peak - 1),
            Err(WasmCpuSearchError::ResourceAdmission { .. })
        ));
    }

    #[test]
    fn verifier_pass_index_owned_replacement_has_an_exact_actual_boundary() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let result = CoreExecutionResult::default();
        let external_retained_bytes = 6_789_u128;
        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");
        let attempt = |cap_bytes: u128| {
            let mut verifier = WasmBuildProbabilityDistributedVerifier::new_typed(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
            )
            .expect("replacement-field verifier");
            verifier.memory_bound = unbounded.with_cap(cap_bytes).expect("candidate cap");
            verifier.try_build_pass_index_replacement_fields(&result, "1", external_retained_bytes)
        };

        let mut exact_peak = 1_u128;
        loop {
            match attempt(exact_peak) {
                Ok(_) => break,
                Err(WasmCpuSearchError::ResourceAdmission { resource_report }) => {
                    let required = resource_report
                        .execution_availability()
                        .required_memory_bytes()
                        .expect("resource rejection reports the required peak");
                    assert!(required > exact_peak, "the boundary search advances");
                    exact_peak = required;
                }
                Err(error) => panic!("unexpected replacement-field error: {error:?}"),
            }
        }

        let fields = attempt(exact_peak).expect("the exact owned-field peak must fit");
        assert_eq!(
            fields,
            vec![("build_distributed_pass_index".to_owned(), "1".to_owned())]
        );
        let verifier = WasmBuildProbabilityDistributedVerifier::new_typed(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
        )
        .expect("retained-byte verifier");
        let actual_field_bytes = (fields.capacity() as u128)
            .checked_mul(core::mem::size_of::<(String, String)>() as u128)
            .and_then(|bytes| bytes.checked_add(fields[0].0.capacity() as u128))
            .and_then(|bytes| bytes.checked_add(fields[0].1.capacity() as u128))
            .expect("checked actual replacement-field storage");
        let expected_peak = verifier
            .checked_retained_bytes()
            .and_then(|bytes| {
                bytes.checked_add(
                    super::super::build_probability::checked_public_result_bytes(&result)?,
                )
            })
            .and_then(|bytes| bytes.checked_add(external_retained_bytes))
            .and_then(|bytes| bytes.checked_add(actual_field_bytes))
            .expect("checked actual owned-field peak");
        assert_eq!(exact_peak, expected_peak);
        drop(verifier);
        assert!(matches!(
            attempt(exact_peak - 1),
            Err(WasmCpuSearchError::ResourceAdmission { .. })
        ));

        for (value, expected) in [
            (0, "0"),
            (9, "9"),
            (10, "10"),
            (99, "99"),
            (100, "100"),
            (255, "255"),
        ] {
            let mut digits = [0_u8; 3];
            assert_eq!(decimal_u8(value, &mut digits), expected);
        }
    }

    #[test]
    fn verifier_two_candidate_row_id_siblings_reach_the_active_session_exact_memory_boundary() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let mut verifier = WasmBuildProbabilityDistributedVerifier::new_typed(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
        )
        .expect("verifier");
        verifier.activate_pass(0, 0).expect("active compact pass");

        let candidates = vec![
            WasmCandidatePacket::for_pass(0, 0, u32::MAX, vec![u32::MAX; 3]),
            WasmCandidatePacket::for_pass(1, 0, u32::MAX, vec![u32::MAX; 5]),
        ];
        let raw_input_bytes = 257_u128;
        let decoded_outer_bytes = (candidates.capacity() as u128)
            .checked_mul(core::mem::size_of::<WasmCandidatePacket>() as u128)
            .expect("checked decoded candidate outer storage");
        let current_and_sibling_row_id_bytes = candidates
            .iter()
            .try_fold(0_u128, |bytes, candidate| {
                bytes.checked_add(
                    (candidate.row_ids().len() as u128)
                        .checked_mul(core::mem::size_of::<u32>() as u128)?,
                )
            })
            .expect("checked current/sibling row-id storage");
        let external_retained_bytes = raw_input_bytes
            .checked_add(decoded_outer_bytes)
            .and_then(|bytes| bytes.checked_add(current_and_sibling_row_id_bytes))
            .expect("checked raw/decoded/current/sibling ownership");

        let active_retained = verifier
            .active_pass
            .as_ref()
            .expect("active pass")
            .1
            .checked_retained_bytes()
            .expect("checked active-session ownership");
        let base_coexisting = verifier
            .checked_active_coexisting_retained_bytes()
            .expect("checked verifier sibling ownership");
        assert_eq!(
            active_retained.checked_add(base_coexisting),
            verifier.checked_retained_bytes()
        );
        let required = active_retained
            .checked_add(base_coexisting)
            .and_then(|bytes| bytes.checked_add(external_retained_bytes))
            .expect("checked active plus all external owners");
        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");

        verifier.memory_bound = unbounded;
        verifier
            .active_pass
            .as_mut()
            .expect("active pass")
            .1
            .set_memory_bound_for_test(unbounded.with_cap(required).expect("exact active cap"));
        let exact_error = verifier
            .consume_with_external_retained(
                &candidates[0],
                &ExecutionControl::default(),
                external_retained_bytes,
            )
            .expect_err("the intentionally invalid row id must be rejected after the exact guard");
        assert!(matches!(
            exact_error,
            WasmCpuSearchError::InvalidProblem {
                reason: "wasm_build_probability_distributed_candidate_invalid"
            }
        ));

        verifier
            .active_pass
            .as_mut()
            .expect("active pass")
            .1
            .set_memory_bound_for_test(
                unbounded
                    .with_cap(required - 1)
                    .expect("one-byte-short active cap"),
            );
        assert!(matches!(
            verifier
                .consume_with_external_retained(
                    &candidates[0],
                    &ExecutionControl::default(),
                    external_retained_bytes,
                )
                .expect_err(
                    "one byte below active plus raw/decoded/current/sibling ownership must fail"
                ),
            WasmCpuSearchError::ResourceAdmission { .. }
        ));
        assert!(matches!(
            verifier
                .consume_with_external_retained(
                    &candidates[0],
                    &ExecutionControl::default(),
                    u128::MAX,
                )
                .expect_err("external owner overflow must fail closed before candidate processing"),
            WasmCpuSearchError::ResourceAdmission { .. }
        ));
    }

    #[test]
    fn oversized_worker_coverage_scratch_counts_the_external_source_once_at_the_exact_boundary() {
        const WORD_COUNT: usize = 4_096;

        let result =
            CoreExecutionResult::default().with_coverage_pattern_words(vec![u64::MAX; WORD_COUNT]);
        let external = super::super::build_probability::checked_public_result_bytes(&result)
            .expect("checked external worker surface");
        let coverage_scratch =
            PatternBitSet::checked_external_words_materialize_union_future_bytes(
                WORD_COUNT * u64::BITS as usize,
            )
            .expect("checked coverage scratch");
        let future = checked_worker_validation_and_absorb_future_bytes(&result, 0)
            .expect("checked worker validation future");
        assert_eq!(future, coverage_scratch);

        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");
        let required = external
            .checked_add(coverage_scratch)
            .expect("checked exact peak");
        unbounded
            .with_cap(required)
            .expect("exact cap")
            .ensure(external, future)
            .expect("the exact worker scratch peak is sufficient");
        unbounded
            .with_cap(required - 1)
            .expect("one-byte-short cap")
            .ensure(external, future)
            .expect_err("one byte below worker scratch ownership must fail closed");
    }

    #[test]
    fn backend_field_replacement_is_guarded_before_strings_at_the_one_byte_boundary() {
        use super::super::distributed::WasmDistributedBackendExecution;

        let execution = WasmDistributedBackendExecution::WebGpu {
            adapter_index: u8::MAX,
            adapter_name: "adapter-name".repeat(32),
            adapter_type: "discrete-gpu",
            adapter_backend: "vulkan".repeat(32),
            peak_gpu_bytes: u64::MAX,
            shader_hash: "f".repeat(256),
            shader_version: "v-test",
            warmup_performed: true,
            session_reused: true,
        };
        let result = || {
            CoreExecutionResult::default().with_additional_fields(vec![
                ("backend_requested".to_owned(), "hybrid".to_owned()),
                ("board_storage".to_owned(), "board64-canonical".to_owned()),
            ])
        };

        let mut required = 0_u128;
        let materialized =
            apply_backend_execution_with_memory_guard(result(), &execution, |live, future| {
                let peak = super::super::build_probability::checked_public_result_bytes(live)
                    .and_then(|bytes| bytes.checked_add(future))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "test_backend_projection_overflow",
                    ))?;
                required = required.max(peak);
                Ok(())
            })
            .expect("unbounded guarded backend materialization");
        assert_eq!(materialized.field("backend_selected"), Some("webgpu"));

        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");
        let exact = unbounded.with_cap(required).expect("exact cap");
        apply_backend_execution_with_memory_guard(result(), &execution, |live, future| {
            exact
                .ensure(
                    super::super::build_probability::checked_public_result_bytes(live).ok_or(
                        WasmExactSearchError::InvalidProblem("test_backend_projection_overflow"),
                    )?,
                    future,
                )
                .map_err(WasmExactSearchError::ResourceAdmission)
        })
        .expect("the recorded exact pre-allocation peak is sufficient");

        let one_byte_short = unbounded
            .with_cap(required - 1)
            .expect("one-byte-short cap");
        let error =
            apply_backend_execution_with_memory_guard(result(), &execution, |live, future| {
                one_byte_short
                    .ensure(
                        super::super::build_probability::checked_public_result_bytes(live).ok_or(
                            WasmExactSearchError::InvalidProblem(
                                "test_backend_projection_overflow",
                            ),
                        )?,
                        future,
                    )
                    .map_err(WasmExactSearchError::ResourceAdmission)
            })
            .expect_err("one byte below the guarded backend-field peak must fail closed");
        assert!(matches!(error, WasmExactSearchError::ResourceAdmission(_)));
    }

    fn run_serial(problem: &SearchProblem, field: BuildProbabilityField) -> CoreExecutionResult {
        let mut session = WasmBuildProbabilitySession::new(
            problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Search {
                pattern_knowledge: FinessePatternKnowledge::Both,
            },
        )
        .expect("serial session");
        loop {
            match session
                .advance(1_024, &ExecutionControl::default())
                .expect("serial advance")
            {
                BuildProbabilityAdvance::Pending => {}
                BuildProbabilityAdvance::Completed(result) => return result,
                BuildProbabilityAdvance::Cancelled => panic!("serial test was not cancelled"),
            }
        }
    }

    fn run_distributed(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) -> CoreExecutionResult {
        let control = ExecutionControl::default();
        let mut producer =
            WasmBuildProbabilityCandidateProducer::new_with_finesse_and_verifiers_typed(
                problem,
                field,
                BuildProbabilityAggregation::Buildability,
                FinesseMetric::Inputs,
                FinessePatternKnowledge::Both,
                1,
                0,
            )
            .expect("producer");
        let mut verifier = producer
            .new_delegated_verifier(field, BuildProbabilityAggregation::Buildability)
            .expect("delegated verifier");
        let summary = loop {
            match producer.advance(&control).expect("producer advance") {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(candidate) => verifier
                    .consume(&candidate, &control)
                    .expect("candidate verification"),
                WasmCandidateProducerAdvance::Completed(summary) => break summary,
                WasmCandidateProducerAdvance::Cancelled => {
                    panic!("distributed test was not cancelled")
                }
            }
        };
        let partials = verifier.finish().expect("worker results");
        let mut merger = producer.into_merger().expect("result merger");
        for partial in &partials {
            merger.absorb(partial).expect("partial merge");
        }
        merger.finish(&summary, 2).expect("distributed result")
    }

    fn exact_probability_problem(
        height: u8,
        exact_pieces: usize,
        policy: PcSolutionProbabilityPolicy,
    ) -> SearchProblem {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(u16::from(height), 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(exact_pieces),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(exact_pieces))
        .with_solution_probability_policy(policy);
        ProblemCompiler::compile_scenario_pc(&query).expect("probability test problem")
    }

    fn run_serial_without_finesse(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) -> CoreExecutionResult {
        let mut session = WasmBuildProbabilitySession::new(
            problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
        )
        .expect("serial probability session");
        loop {
            match session
                .advance(1_024, &ExecutionControl::default())
                .expect("serial probability advance")
            {
                BuildProbabilityAdvance::Pending => {}
                BuildProbabilityAdvance::Completed(result) => return result,
                BuildProbabilityAdvance::Cancelled => {
                    panic!("serial probability test was not cancelled")
                }
            }
        }
    }

    fn run_distributed_without_finesse(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) -> CoreExecutionResult {
        let control = ExecutionControl::default();
        let mut producer =
            WasmBuildProbabilityCandidateProducer::new_with_finesse_and_verifiers_typed(
                problem,
                field,
                BuildProbabilityAggregation::Buildability,
                FinesseMetric::Off,
                FinessePatternKnowledge::Both,
                1,
                0,
            )
            .expect("probability producer");
        let mut verifier = producer
            .new_delegated_verifier(field, BuildProbabilityAggregation::Buildability)
            .expect("probability verifier");
        let summary = loop {
            match producer
                .advance(&control)
                .expect("probability producer advance")
            {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(candidate) => verifier
                    .consume(&candidate, &control)
                    .expect("probability candidate verification"),
                WasmCandidateProducerAdvance::Completed(summary) => break summary,
                WasmCandidateProducerAdvance::Cancelled => {
                    panic!("distributed probability test was not cancelled")
                }
            }
        };
        let partials = verifier.finish().expect("probability worker results");
        let mut merger = producer.into_merger().expect("probability result merger");
        for partial in &partials {
            merger.absorb(partial).expect("probability partial merge");
        }
        merger
            .finish(&summary, 2)
            .expect("distributed probability result")
    }

    fn assert_solution_probability_contract(
        result: &CoreExecutionResult,
        requested: bool,
        expected_solution_count: usize,
    ) {
        for field in [
            "piece_source_id",
            "pattern_universe_id",
            "pattern_weight_model_id",
            "coverage_aggregation_contract",
            "coverage_aggregation_availability",
            "coverage_aggregation_complete",
            "coverage_aggregation_source_row_count",
            "covered_pattern_count",
            "failed_pattern_count",
            "coverage_probability",
            "failed_coverage_probability",
            "materialized_probability_mass",
            "coverage_probability_denominator",
            "success_conditional_probability_denominator",
            "probability_complete",
            "count_complete",
            "solution_probabilities_requested",
            "solution_probability_count",
            "solution_probability_complete",
            "solution_probability_basis",
            "solution_probability_incomplete_reason",
        ] {
            assert_eq!(
                result.field_occurrence_count(field),
                1,
                "final probability contract field {field} must occur exactly once"
            );
        }
        assert_eq!(
            result.bool_field("solution_probabilities_requested"),
            Some(requested)
        );
        assert_eq!(
            result.field_occurrence_count("solution_probabilities_requested"),
            1
        );
        assert_eq!(
            result.usize_field("unique_solution_count"),
            Some(expected_solution_count)
        );
        assert_eq!(
            result.usize_field("solution_probability_count"),
            Some(if requested {
                expected_solution_count
            } else {
                0
            })
        );
        assert_eq!(
            result.bool_field("solution_probability_complete"),
            Some(true)
        );
        assert_eq!(
            result.field("solution_probability_basis"),
            Some(if requested {
                "normalized-solution-pattern-bitset-or-union"
            } else {
                "not-requested"
            })
        );
        assert_eq!(
            result.field("solution_probability_incomplete_reason"),
            Some("none")
        );
        assert_eq!(result.bool_field("solution_keys_complete"), Some(true));
        assert_eq!(
            result.normalized_solution_keys().len(),
            expected_solution_count
        );
        if requested {
            let weights = crate::solution_probability_pattern_weights(result)
                .expect("included result has typed pattern-weight authority");
            assert_eq!(
                result.normalized_solution_coverages().len(),
                expected_solution_count
            );
            assert_eq!(
                result.solution_probabilities().len(),
                expected_solution_count
            );
            assert_eq!(weights.len(), 1);
            assert_eq!(
                crate::normalized_solution_probability_reports(
                    result.normalized_solution_keys(),
                    result.normalized_solution_coverages(),
                    &weights,
                    true,
                )
                .expect("authoritative result reports"),
                result.solution_probabilities()
            );
            for (key, report) in result
                .normalized_solution_keys()
                .iter()
                .zip(result.solution_probabilities())
            {
                assert_eq!(report.solution_key(), key);
                assert_eq!(report.probability(), "1");
                assert_eq!(report.covered_pattern_count(), 1);
                assert_eq!(report.pattern_count(), 1);
                assert!(report.probability_complete());
            }
        } else {
            assert!(result.solution_probabilities().is_empty());
        }
    }

    fn assert_serial_distributed_probability_parity(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        requested: bool,
        expected_solution_count: usize,
    ) {
        let expected_height = field.height().to_string();
        let serial = run_serial_without_finesse(problem, field);
        let distributed = run_distributed_without_finesse(problem, field);
        assert_solution_probability_contract(&serial, requested, expected_solution_count);
        assert_solution_probability_contract(&distributed, requested, expected_solution_count);
        for result in [&serial, &distributed] {
            assert_eq!(result.field_occurrence_count("board_height"), 1);
            assert_eq!(
                result.unique_field("board_height"),
                Some(expected_height.as_str())
            );
        }
        assert_eq!(
            distributed.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(
            distributed.normalized_solution_coverages(),
            serial.normalized_solution_coverages()
        );
        assert_eq!(
            distributed.solution_probabilities(),
            serial.solution_probabilities()
        );
        for field in [
            "solution_probabilities_requested",
            "solution_probability_count",
            "solution_probability_complete",
            "solution_probability_basis",
            "solution_probability_incomplete_reason",
        ] {
            assert_eq!(distributed.unique_field(field), serial.unique_field(field));
        }
    }

    #[test]
    fn compact_and_extended_include_and_omit_match_serial_and_distributed() {
        let compact =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
                .expect("compact one-I field");
        let extended =
            BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0xf, 0, 0, 0])
                .expect("extended one-I field");
        for policy in [
            PcSolutionProbabilityPolicy::Omit,
            PcSolutionProbabilityPolicy::Include,
        ] {
            let requested = matches!(policy, PcSolutionProbabilityPolicy::Include);
            assert_serial_distributed_probability_parity(
                &exact_probability_problem(4, 1, policy),
                compact,
                requested,
                1,
            );
            assert_serial_distributed_probability_parity(
                &exact_probability_problem(24, 1, policy),
                extended,
                requested,
                1,
            );
        }
    }

    #[test]
    fn compact_and_extended_empty_targets_materialize_one_complete_probability_report() {
        let compact = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
            .expect("compact empty target");
        let extended = BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0; 4])
            .expect("extended empty target");
        let compact_problem = exact_probability_problem(4, 0, PcSolutionProbabilityPolicy::Include);
        let extended_problem =
            exact_probability_problem(24, 0, PcSolutionProbabilityPolicy::Include);

        assert_serial_distributed_probability_parity(&compact_problem, compact, true, 1);
        assert_serial_distributed_probability_parity(&extended_problem, extended, true, 1);

        let compact_result = run_serial_without_finesse(&compact_problem, compact);
        let extended_result = run_serial_without_finesse(&extended_problem, extended);
        assert!(compact_result.normalized_solution_keys()[0].starts_with("ctk1|"));
        assert!(extended_result.normalized_solution_keys()[0].starts_with("ctk2|"));
    }

    fn worker_partials_and_merger(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) -> (
        WasmBuildProbabilityDistributedResultMerger,
        Vec<CoreExecutionResult>,
        WasmDistributedGeometrySummary,
    ) {
        let control = ExecutionControl::default();
        let mut producer =
            WasmBuildProbabilityCandidateProducer::new_with_finesse_and_verifiers_typed(
                problem,
                field,
                BuildProbabilityAggregation::Buildability,
                FinesseMetric::Off,
                FinessePatternKnowledge::Both,
                1,
                0,
            )
            .expect("malformed-partial producer");
        let mut verifier = producer
            .new_delegated_verifier(field, BuildProbabilityAggregation::Buildability)
            .expect("malformed-partial verifier");
        let summary = loop {
            match producer
                .advance(&control)
                .expect("malformed-partial advance")
            {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(candidate) => verifier
                    .consume(&candidate, &control)
                    .expect("valid worker candidate"),
                WasmCandidateProducerAdvance::Completed(summary) => break summary,
                WasmCandidateProducerAdvance::Cancelled => {
                    panic!("malformed-partial setup was not cancelled")
                }
            }
        };
        let partials = verifier.finish().expect("valid worker partial");
        (
            producer.into_merger().expect("malformed-partial merger"),
            partials,
            summary,
        )
    }

    fn one_worker_partial_and_merger(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) -> (
        WasmBuildProbabilityDistributedResultMerger,
        CoreExecutionResult,
        WasmDistributedGeometrySummary,
    ) {
        let (merger, mut partials, summary) = worker_partials_and_merger(problem, field);
        assert_eq!(partials.len(), 1);
        (merger, partials.pop().unwrap(), summary)
    }

    fn assert_actual_absorb_exact_and_peak_minus_one(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) {
        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(problem).expect("test memory authority");
        let attempt = |cap_bytes: u128| {
            let (mut merger, partial, _) = one_worker_partial_and_merger(problem, field);
            let external_result_bytes =
                WasmBuildProbabilityDistributedResultMerger::public_result_retained_bytes(&partial)
                    .expect("checked external worker result");
            merger.memory_bound = unbounded.with_cap(cap_bytes).expect("candidate cap");
            merger.absorb_with_external_retained(&partial, external_result_bytes)
        };

        let mut exact_peak = 1_u128;
        loop {
            match attempt(exact_peak) {
                Ok(()) => break,
                Err(WasmCpuSearchError::ResourceAdmission { resource_report }) => {
                    let required = resource_report
                        .execution_availability()
                        .required_memory_bytes()
                        .expect("resource rejection reports the required peak");
                    assert!(required > exact_peak, "the absorb boundary search advances");
                    exact_peak = required;
                }
                Err(error) => panic!("unexpected distributed absorb error: {error:?}"),
            }
        }
        attempt(exact_peak).expect("the discovered exact absorb peak must fit");
        assert!(matches!(
            attempt(exact_peak - 1),
            Err(WasmCpuSearchError::ResourceAdmission { .. })
        ));
    }

    #[test]
    fn compact_and_extended_actual_absorb_have_exact_stage_guard_boundaries() {
        let compact_problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let compact_field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
                .expect("compact one-I field");
        assert_actual_absorb_exact_and_peak_minus_one(&compact_problem, compact_field);

        let extended_problem = exact_probability_problem(24, 1, PcSolutionProbabilityPolicy::Omit);
        let extended_field =
            BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0xf, 0, 0, 0])
                .expect("extended one-I field");
        assert_actual_absorb_exact_and_peak_minus_one(&extended_problem, extended_field);
    }

    #[test]
    fn absorb_external_owner_must_include_the_complete_borrowed_result() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let (mut merger, partial, _) = one_worker_partial_and_merger(&problem, field);
        let source_result_bytes = partial
            .checked_resource_retained_bytes()
            .expect("checked external worker result");
        assert!(source_result_bytes > 0);
        assert!(matches!(
            merger.absorb_with_external_retained(&partial, source_result_bytes - 1),
            Err(WasmCpuSearchError::InvalidProblem {
                reason: "wasm_build_probability_external_result_bytes_below_result"
            })
        ));
        merger
            .absorb_with_external_retained(&partial, source_result_bytes)
            .expect("the complete exact source owner is accepted");
    }

    #[test]
    fn outer_spin_reserve_actual_checkpoint_rejects_allocator_excess_after_initial_projection() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Include);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let (mut merger, partial, _) = one_worker_partial_and_merger(&problem, field);
        let result =
            partial.with_postprocess_spin_coverages(vec![CorePostProcessSpinCoverage::new(
                "outer-reserve",
                0,
                1,
                vec![1],
                vec!["candidate".to_owned()],
                1,
                true,
            )]);
        let external_result_bytes =
            WasmBuildProbabilityDistributedResultMerger::public_result_retained_bytes(&result)
                .expect("checked external worker result");
        let initial_observed = merger
            .checked_retained_bytes()
            .expect("checked initial merger ownership");
        let initial_clone_future = merger
            .checked_spin_clone_future_bytes(&result)
            .expect("checked initial spin-clone projection");
        let initial_future =
            checked_worker_validation_and_absorb_future_bytes(&result, initial_clone_future)
                .expect("checked initial validation/absorb projection");
        let initial_required = initial_observed
            .checked_add(external_result_bytes)
            .and_then(|bytes| bytes.checked_add(initial_future))
            .expect("checked initial peak");

        let target_len = result.postprocess_spin_coverages().len();
        // `Vec` only promises that its actual capacity is at least the
        // requested capacity. Force an excess-capacity postcondition so this
        // test exercises the post-reserve authorization independently of the
        // platform allocator's current growth choice.
        merger
            .spin_coverages
            .try_reserve_exact(target_len + 64)
            .expect("simulate excess outer capacity at the checkpoint");
        let nested_clone_future =
            WasmBuildProbabilityDistributedResultMerger::checked_spin_clone_nested_future_bytes(
                &result,
            )
            .expect("checked nested clone projection");
        let post_reserve_future =
            checked_worker_validation_and_absorb_future_bytes(&result, nested_clone_future)
                .expect("checked post-reserve validation/absorb projection");
        let post_reserve_required = merger
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(external_result_bytes))
            .and_then(|bytes| bytes.checked_add(post_reserve_future))
            .expect("checked actual post-reserve peak");
        assert!(post_reserve_required > initial_required);

        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");
        let one_byte_short = unbounded
            .with_cap(post_reserve_required - 1)
            .expect("one-byte-short post-reserve cap");
        let initial_checked_future = external_result_bytes
            .checked_add(initial_future)
            .expect("checked initial external-plus-future ownership");
        one_byte_short
            .ensure(initial_observed, initial_checked_future)
            .expect("the initial requested-capacity projection still fits");
        merger.memory_bound = one_byte_short;
        assert!(matches!(
            merger
                .validate_external_result_memory(external_result_bytes, post_reserve_future)
                .expect_err("actual outer capacity must be reauthorized after reserve"),
            WasmCpuSearchError::ResourceAdmission { .. }
        ));

        merger.memory_bound = unbounded
            .with_cap(post_reserve_required)
            .expect("exact post-reserve cap");
        merger
            .validate_external_result_memory(external_result_bytes, post_reserve_future)
            .expect("the exact actual outer-capacity checkpoint is sufficient");
    }

    #[test]
    fn nested_spin_clone_actual_checkpoint_rejects_allocator_excess_before_next_allocation() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Include);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let (mut merger, partial, _) = one_worker_partial_and_merger(&problem, field);
        let external_result_bytes =
            WasmBuildProbabilityDistributedResultMerger::public_result_retained_bytes(&partial)
                .expect("checked external worker result");
        let observed = merger
            .checked_retained_bytes()
            .expect("checked merger ownership");

        let requested_previous_allocation = 1_u128;
        let checked_next_allocation_bytes = core::mem::size_of::<u64>() as u128;
        let mut simulated_local_owner = String::with_capacity(4_096);
        simulated_local_owner.push('x');
        let actual_local_nested_bytes = simulated_local_owner.capacity() as u128;
        assert!(actual_local_nested_bytes > requested_previous_allocation);
        let actual_checkpoint_required = observed
            .checked_add(external_result_bytes)
            .and_then(|bytes| bytes.checked_add(actual_local_nested_bytes))
            .and_then(|bytes| bytes.checked_add(checked_next_allocation_bytes))
            .expect("checked nested-clone checkpoint peak");

        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");
        let one_byte_short = unbounded
            .with_cap(actual_checkpoint_required - 1)
            .expect("one-byte-short nested checkpoint cap");
        let requested_checkpoint_future = external_result_bytes
            .checked_add(requested_previous_allocation)
            .and_then(|bytes| bytes.checked_add(checked_next_allocation_bytes))
            .expect("checked requested nested checkpoint future");
        one_byte_short
            .ensure(observed, requested_checkpoint_future)
            .expect("the requested nested allocation projection still fits");
        merger.memory_bound = one_byte_short;
        assert!(matches!(
            merger
                .validate_spin_clone_local_memory(
                    external_result_bytes,
                    actual_local_nested_bytes,
                    checked_next_allocation_bytes,
                )
                .expect_err("actual nested capacity must be checked before the next allocation"),
            WasmCpuSearchError::ResourceAdmission { .. }
        ));

        merger.memory_bound = unbounded
            .with_cap(actual_checkpoint_required)
            .expect("exact nested checkpoint cap");
        merger
            .validate_spin_clone_local_memory(
                external_result_bytes,
                actual_local_nested_bytes,
                checked_next_allocation_bytes,
            )
            .expect("the exact nested-clone checkpoint is sufficient");

        merger.memory_bound = unbounded;
        let source = CorePostProcessSpinCoverage::new(
            "nested-clone",
            0,
            1,
            vec![1],
            vec!["candidate".to_owned()],
            1,
            true,
        );
        assert_eq!(
            merger
                .try_clone_spin_coverage_with_memory_guard(&source, external_result_bytes)
                .expect("guarded nested clone"),
            source
        );
    }

    #[test]
    fn mirrored_second_pass_counts_the_completed_first_result_at_the_one_byte_boundary() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Include);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field")
            .with_horizontal_mirror_included(true);
        assert!(field.includes_applicable_horizontal_mirror());
        let (mut merger, partials, _) = worker_partials_and_merger(&problem, field);
        assert_eq!(partials.len(), 2);
        for partial in &partials {
            merger.absorb(partial).expect("valid mirrored partial");
        }

        let mut results = Vec::with_capacity(merger.passes.len());
        let finesse_materials = Vec::new();
        let first_coexisting = merger
            .checked_pass_finalization_coexisting_retained_bytes(0, &results, &finesse_materials)
            .expect("first-pass coexisting projection");
        merger.passes[0].set_coexisting_retained_bytes(first_coexisting);
        let first_summary = merger.summaries[0].clone();
        let first = match merger.passes[0]
            .complete_distributed_geometry(&first_summary, 2)
            .expect("first mirrored pass")
        {
            BuildProbabilityAdvance::Completed(result) => result,
            BuildProbabilityAdvance::Pending => panic!("first mirrored pass remained pending"),
            BuildProbabilityAdvance::Cancelled => panic!("first mirrored pass was cancelled"),
        };
        results.push(first);

        let second_coexisting = merger
            .checked_pass_finalization_coexisting_retained_bytes(1, &results, &finesse_materials)
            .expect("second-pass coexisting projection");
        let empty_results = Vec::with_capacity(results.capacity());
        let second_without_completed_result = merger
            .checked_pass_finalization_coexisting_retained_bytes(
                1,
                &empty_results,
                &finesse_materials,
            )
            .expect("second-pass projection without the completed result");
        let result_inline = core::mem::size_of::<CoreExecutionResult>() as u128;
        let first_nested = results[0]
            .checked_resource_retained_bytes()
            .expect("first-result retained bytes")
            .checked_sub(result_inline)
            .expect("first-result nested bytes");
        assert_eq!(
            second_coexisting - second_without_completed_result,
            first_nested,
            "the second pass must count the first result's actual nested owners exactly once"
        );

        let second_live = merger.passes[1]
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(second_coexisting))
            .expect("second-pass live-byte projection");
        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");
        unbounded
            .with_cap(second_live)
            .expect("exact cap")
            .ensure(second_live, 0)
            .expect("the exact pre-materialization live-byte cap is sufficient");
        let one_byte_short = unbounded
            .with_cap(second_live - 1)
            .expect("one-byte-short cap");
        one_byte_short
            .ensure(second_live, 0)
            .expect_err("one byte below the two-pass live ownership must fail closed");
        merger.memory_bound = one_byte_short;
        for pass in &mut merger.passes {
            pass.set_memory_bound_for_test(one_byte_short);
        }
        merger.passes[1].set_coexisting_retained_bytes(second_coexisting);
        let second_summary = merger.summaries[1].clone();
        let error = match merger.passes[1].complete_distributed_geometry(&second_summary, 2) {
            Ok(_) => panic!("the second pass must reject the one-byte-short aggregate cap"),
            Err(error) => error,
        };
        assert!(matches!(error, WasmExactSearchError::ResourceAdmission(_)));
    }

    #[test]
    fn producer_second_pass_coexisting_projection_counts_the_first_finalizer() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Include);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field")
            .with_horizontal_mirror_included(true);
        let control = ExecutionControl::default();
        let mut producer =
            WasmBuildProbabilityCandidateProducer::new_with_finesse_and_verifiers_typed(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
                FinesseMetric::Off,
                FinessePatternKnowledge::Both,
                1,
                0,
            )
            .expect("mirrored producer");

        while producer.finalizers.is_empty() {
            match producer.advance(&control).expect("producer advance") {
                WasmCandidateProducerAdvance::Pending
                | WasmCandidateProducerAdvance::Candidate(_) => {}
                WasmCandidateProducerAdvance::Completed(_) => {
                    panic!("distinct mirrored producer finished before its second pass")
                }
                WasmCandidateProducerAdvance::Cancelled => panic!("producer was not cancelled"),
            }
        }
        while producer
            .active
            .as_ref()
            .is_none_or(|pass| pass.pass_index != 1)
        {
            match producer.advance(&control).expect("second-pass activation") {
                WasmCandidateProducerAdvance::Pending
                | WasmCandidateProducerAdvance::Candidate(_) => {}
                WasmCandidateProducerAdvance::Completed(_) => {
                    panic!("distinct mirrored producer skipped its second pass")
                }
                WasmCandidateProducerAdvance::Cancelled => panic!("producer was not cancelled"),
            }
        }

        let active = producer
            .active
            .as_ref()
            .expect("second pass is active")
            .session
            .checked_retained_bytes()
            .expect("active retained bytes");
        let coexisting = producer
            .checked_active_coexisting_retained_bytes()
            .expect("coexisting retained bytes");
        assert!(coexisting >= producer.finalizers[0].checked_retained_bytes().unwrap());
        assert_eq!(
            active.checked_add(coexisting),
            producer.checked_retained_bytes()
        );
    }

    #[test]
    fn terminal_public_result_validation_counts_actual_nested_bytes_and_identity_cannot_bypass() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Include);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let (mut baseline, partial, summary) = one_worker_partial_and_merger(&problem, field);
        baseline.absorb(&partial).expect("valid worker partial");
        let final_result = baseline.finish(&summary, 2).expect("baseline final result");
        drop(baseline);

        let (mut merger, partial, _) = one_worker_partial_and_merger(&problem, field);
        merger.absorb(&partial).expect("valid worker partial");
        let retained = merger
            .checked_retained_bytes()
            .expect("checked merger retained bytes");
        let final_bytes =
            super::super::build_probability::checked_public_result_bytes(&final_result)
                .expect("checked final public bytes");
        let unbounded =
            ExecutionMemoryBound::unbounded_for_problem(&problem).expect("test memory authority");
        merger.memory_bound = unbounded
            .with_cap(retained + final_bytes)
            .expect("exact terminal cap");
        merger
            .validate_public_result_memory(&final_result)
            .expect("the exact retained-plus-final cap is sufficient");

        let one_byte_short = unbounded
            .with_cap(retained + final_bytes - 1)
            .expect("one-byte-short terminal cap");
        merger.memory_bound = one_byte_short;
        merger
            .validate_public_result_memory(&final_result)
            .expect_err("one byte below actual nested final ownership must fail closed");
        let terminal = merger.apply_terminal_memory_guard(Ok(final_result), |result, _| result);
        assert_eq!(
            terminal.expect_err("identity terminal must preserve the guarded failure"),
            "wasm_build_probability_aggregate_memory_budget_exceeded"
        );
    }

    fn rejected_partial_reason(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        mutate: impl FnOnce(CoreExecutionResult) -> CoreExecutionResult,
    ) -> &'static str {
        let (mut merger, partial, _) = one_worker_partial_and_merger(problem, field);
        let malformed = mutate(partial);
        merger
            .absorb(&malformed)
            .expect_err("malformed worker partial must fail closed")
    }

    fn foreign_u64_field(partial: &CoreExecutionResult, key: &str) -> String {
        partial
            .unique_field(key)
            .expect("worker authority field")
            .parse::<u64>()
            .expect("canonical u64 authority")
            .wrapping_add(1)
            .to_string()
    }

    fn assert_distributed_coverage_authority_rejections(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) {
        assert_eq!(
            rejected_partial_reason(problem, field, |partial| {
                let foreign = foreign_u64_field(&partial, "piece_source_id");
                partial.with_replaced_fields(vec![("piece_source_id".to_owned(), foreign)])
            }),
            "wasm_build_probability_distributed_piece_source_id_mismatch"
        );
        assert_eq!(
            rejected_partial_reason(problem, field, |partial| {
                // The word shape and pattern count remain unchanged. Only the
                // ordered universe identity changes, modeling a foreign or
                // reordered same-cardinality universe.
                let foreign = foreign_u64_field(&partial, "pattern_universe_id");
                partial.with_replaced_fields(vec![("pattern_universe_id".to_owned(), foreign)])
            }),
            "wasm_build_probability_distributed_pattern_universe_id_mismatch"
        );
        assert_eq!(
            rejected_partial_reason(problem, field, |partial| {
                let foreign = foreign_u64_field(&partial, "pattern_weight_model_id");
                partial.with_replaced_fields(vec![("pattern_weight_model_id".to_owned(), foreign)])
            }),
            "wasm_build_probability_distributed_pattern_weight_model_id_mismatch"
        );
        assert_eq!(
            rejected_partial_reason(problem, field, |partial| {
                partial.with_additional_fields(vec![(
                    "coverage_aggregation_contract".to_owned(),
                    "pattern-coverage-aggregation.v1".to_owned(),
                )])
            }),
            "wasm_build_probability_distributed_coverage_contract_invalid"
        );
        assert_eq!(
            rejected_partial_reason(problem, field, |partial| {
                partial.without_field_for_test("coverage_probability_denominator")
            }),
            "wasm_build_probability_distributed_coverage_denominator_invalid"
        );
        assert_eq!(
            rejected_partial_reason(problem, field, |partial| {
                partial.with_replaced_fields(vec![(
                    "coverage_aggregation_complete".to_owned(),
                    "false".to_owned(),
                )])
            }),
            "wasm_build_probability_distributed_coverage_complete_mismatch"
        );
        assert_eq!(
            rejected_partial_reason(problem, field, |partial| {
                partial.with_replaced_fields(vec![(
                    "build_probability_aggregation".to_owned(),
                    "tiling".to_owned(),
                )])
            }),
            "wasm_build_probability_distributed_aggregation_mismatch"
        );
        assert_eq!(
            rejected_partial_reason(problem, field, |partial| {
                partial
                    .with_replaced_fields(vec![("coverage_probability".to_owned(), "0".to_owned())])
            }),
            "wasm_build_probability_distributed_coverage_probability_mismatch"
        );
    }

    #[test]
    fn compact_and_extended_partials_reject_foreign_or_ambiguous_coverage_authority() {
        let compact_problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Include);
        let compact_field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
                .expect("compact one-I field");
        assert_distributed_coverage_authority_rejections(&compact_problem, compact_field);

        let extended_problem =
            exact_probability_problem(24, 1, PcSolutionProbabilityPolicy::Include);
        let extended_field =
            BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0xf, 0, 0, 0])
                .expect("extended one-I field");
        assert_distributed_coverage_authority_rejections(&extended_problem, extended_field);
    }

    #[test]
    fn compact_and_extended_fold_worker_completeness_without_masking_resource_state() {
        {
            let compact_problem =
                exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Include);
            let compact_field =
                BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
                    .expect("compact one-I field");
            let (mut compact_merger, compact_partial, compact_summary) =
                one_worker_partial_and_merger(&compact_problem, compact_field);
            compact_merger
                .absorb(
                    &compact_partial.with_replaced_fields(vec![(
                        "count_complete".to_owned(),
                        "false".to_owned(),
                    )]),
                )
                .expect("well-shaped incomplete compact worker result");
            let compact = compact_merger
                .finish(&compact_summary, 2)
                .expect("incomplete compact aggregate remains a typed result");
            assert_eq!(compact.bool_field("count_complete"), Some(false));
            assert_eq!(compact.bool_field("probability_complete"), Some(true));
            assert_eq!(compact.bool_field("resource_truncated"), Some(false));
            assert_eq!(
                compact.bool_field("solution_probability_complete"),
                Some(false)
            );
            assert_eq!(
                compact.field("solution_probability_incomplete_reason"),
                Some("solution-count-incomplete")
            );
        }

        let extended_problem =
            exact_probability_problem(24, 1, PcSolutionProbabilityPolicy::Include);
        let extended_field =
            BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0xf, 0, 0, 0])
                .expect("extended one-I field");
        let (mut extended_merger, extended_partial, extended_summary) =
            one_worker_partial_and_merger(&extended_problem, extended_field);
        extended_merger
            .absorb(&extended_partial.with_replaced_fields(vec![
                ("probability_complete".to_owned(), "false".to_owned()),
                (
                    "coverage_aggregation_complete".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "coverage_aggregation_availability".to_owned(),
                    "incomplete".to_owned(),
                ),
            ]))
            .expect("well-shaped incomplete extended worker result");
        let extended = extended_merger
            .finish(&extended_summary, 2)
            .expect("incomplete extended aggregate remains a typed result");
        assert_eq!(extended.bool_field("count_complete"), Some(true));
        assert_eq!(extended.bool_field("probability_complete"), Some(false));
        assert_eq!(extended.bool_field("resource_truncated"), Some(false));
        assert_eq!(
            extended.bool_field("solution_probability_complete"),
            Some(false)
        );
        assert_eq!(
            extended.field("solution_probability_incomplete_reason"),
            Some("pattern-specific-coverage-incomplete")
        );
    }

    #[test]
    fn compact_requested_partials_reject_missing_duplicate_foreign_and_out_of_domain_surfaces() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Include);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");

        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial
                    .with_solution_coverages(Vec::new())
                    .with_normalized_solution_coverages(Vec::new())
            }),
            "wasm_build_probability_distributed_board64_solution_coverage_incomplete"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                let identity = partial.normalized_solution_identities()[0];
                let key = partial.normalized_solution_keys()[0].clone();
                let coverage = partial.solution_coverages()[0].clone();
                let normalized = partial.normalized_solution_coverages()[0].clone();
                partial
                    .with_replaced_fields(vec![(
                        "unique_solution_count".to_owned(),
                        "2".to_owned(),
                    )])
                    .with_normalized_solution_identities(vec![identity, identity])
                    .with_normalized_solution_keys(vec![key.clone(), key])
                    .with_solution_coverages(vec![coverage.clone(), coverage])
                    .with_normalized_solution_coverages(vec![normalized.clone(), normalized])
            }),
            "wasm_build_probability_distributed_solution_surface_not_canonical"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial.with_normalized_solution_keys(vec!["foreign-key".to_owned()])
            }),
            "wasm_build_probability_distributed_solution_key_mismatch"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial.with_additional_fields(vec![("board_height".to_owned(), "4".to_owned())])
            }),
            "wasm_build_probability_distributed_board_height_invalid"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial.with_replaced_fields(vec![("board_height".to_owned(), "5".to_owned())])
            }),
            "wasm_build_probability_distributed_board_height_mismatch"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                let patterns = partial.solution_coverages()[0].covered_patterns().clone();
                let identity = StandardBoard64TilingIdentity::from_placements(
                    0,
                    [PiecePlacementMask::new(PieceKind::O, 0xf)],
                )
                .expect("syntactically valid foreign identity");
                let key = NormalizedTilingSolutionKey::from_standard_board64_identity(identity)
                    .as_str()
                    .to_owned();
                partial
                    .with_normalized_solution_identities(vec![identity])
                    .with_normalized_solution_keys(vec![key.clone()])
                    .with_solution_coverages(vec![SolutionCoverage::new(
                        identity,
                        patterns.clone(),
                    )])
                    .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                        key, patterns,
                    )])
            }),
            "wasm_build_probability_distributed_identity_not_in_catalog"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial.with_coverage_pattern_words(vec![0])
            }),
            "wasm_build_probability_distributed_solution_coverage_union_mismatch"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial.with_additional_fields(vec![(
                    "solution_probability_count".to_owned(),
                    "1".to_owned(),
                )])
            }),
            "wasm_build_probability_distributed_final_probability_surface_forbidden"
        );
    }

    #[test]
    fn rejected_worker_partial_does_not_commit_outer_spin_coverage() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Include);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let (mut merger, partial, _) = one_worker_partial_and_merger(&problem, field);
        let malformed = partial
            .with_coverage_pattern_words(vec![0])
            .with_postprocess_spin_coverages(vec![CorePostProcessSpinCoverage::new(
                "malformed-worker",
                0,
                1,
                vec![1],
                vec!["foreign-candidate".to_owned()],
                1,
                true,
            )]);

        assert_eq!(
            merger
                .absorb(&malformed)
                .expect_err("malformed worker partial must fail closed"),
            "wasm_build_probability_distributed_solution_coverage_union_mismatch"
        );
        assert!(merger.spin_coverages.is_empty());
    }

    #[test]
    fn extended_requested_partials_reject_missing_duplicate_foreign_shape_and_domain_surfaces() {
        let problem = exact_probability_problem(24, 1, PcSolutionProbabilityPolicy::Include);
        let field = BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0xf, 0, 0, 0])
            .expect("extended one-I field");

        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial.with_normalized_solution_coverages(Vec::new())
            }),
            "wasm_extended_distributed_solution_coverage_incomplete"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                let key = partial.normalized_solution_keys()[0].clone();
                let coverage = partial.normalized_solution_coverages()[0].clone();
                partial
                    .with_replaced_fields(vec![(
                        "unique_solution_count".to_owned(),
                        "2".to_owned(),
                    )])
                    .with_normalized_solution_keys(vec![key.clone(), key])
                    .with_normalized_solution_coverages(vec![coverage.clone(), coverage])
            }),
            "wasm_extended_distributed_solution_keys_incomplete"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                let patterns = partial.normalized_solution_coverages()[0]
                    .covered_patterns()
                    .clone();
                partial.with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                    "foreign-key",
                    patterns,
                )])
            }),
            "wasm_extended_distributed_solution_coverage_foreign_key"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                let key = partial.normalized_solution_keys()[0].clone();
                partial.with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                    key,
                    PatternBitSet::all(2),
                )])
            }),
            "wasm_extended_distributed_solution_coverage_shape_mismatch"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                let foreign_key = partial.normalized_solution_keys()[0].replacen(
                    "|placements=I:",
                    "|placements=O:",
                    1,
                );
                let patterns = partial.normalized_solution_coverages()[0]
                    .covered_patterns()
                    .clone();
                partial
                    .with_normalized_solution_keys(vec![foreign_key.clone()])
                    .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                        foreign_key,
                        patterns,
                    )])
            }),
            "wasm_extended_finesse_solution_key_placement_not_in_catalog"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial.with_coverage_pattern_words(vec![0])
            }),
            "wasm_extended_distributed_solution_coverage_union_mismatch"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial.with_additional_fields(vec![(
                    "solution_probability_complete".to_owned(),
                    "true".to_owned(),
                )])
            }),
            "wasm_build_probability_distributed_final_probability_surface_forbidden"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                let key = partial.normalized_solution_keys()[0].replacen(
                    "ctk2|height=24",
                    "ctk2|height=024",
                    1,
                );
                let coverage = partial.normalized_solution_coverages()[0]
                    .covered_patterns()
                    .clone();
                partial
                    .with_normalized_solution_keys(vec![key.clone()])
                    .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                        key, coverage,
                    )])
            }),
            "wasm_extended_finesse_solution_key_header_invalid"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                let key = format!("{},", partial.normalized_solution_keys()[0]);
                let coverage = partial.normalized_solution_coverages()[0]
                    .covered_patterns()
                    .clone();
                partial
                    .with_normalized_solution_keys(vec![key.clone()])
                    .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                        key, coverage,
                    )])
            }),
            "wasm_extended_finesse_solution_key_placement_invalid"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                let key = partial.normalized_solution_keys()[0].replacen(
                    "|placements=I:",
                    "|placements=i:",
                    1,
                );
                let coverage = partial.normalized_solution_coverages()[0]
                    .covered_patterns()
                    .clone();
                partial
                    .with_normalized_solution_keys(vec![key.clone()])
                    .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                        key, coverage,
                    )])
            }),
            "wasm_extended_finesse_solution_key_piece_invalid"
        );
    }

    #[test]
    fn omit_partials_forbid_public_coverage_but_b2b_omit_keeps_private_normalized_coverage() {
        let compact_problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Omit);
        let compact_field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
                .expect("compact one-I field");
        assert_eq!(
            rejected_partial_reason(&compact_problem, compact_field, |partial| {
                let identity = partial.normalized_solution_identities()[0];
                let key = partial.normalized_solution_keys()[0].clone();
                let patterns = PatternBitSet::all(1);
                partial
                    .with_solution_coverages(vec![SolutionCoverage::new(
                        identity,
                        patterns.clone(),
                    )])
                    .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                        key, patterns,
                    )])
            }),
            "wasm_build_probability_distributed_unexpected_solution_coverage"
        );
        assert_eq!(
            rejected_partial_reason(&compact_problem, compact_field, |partial| {
                partial.with_coverage_pattern_words(vec![2])
            }),
            "wasm_build_probability_distributed_coverage_invalid"
        );

        let extended_problem = exact_probability_problem(24, 1, PcSolutionProbabilityPolicy::Omit);
        let extended_field =
            BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0xf, 0, 0, 0])
                .expect("extended one-I field");
        assert_eq!(
            rejected_partial_reason(&extended_problem, extended_field, |partial| {
                let key = partial.normalized_solution_keys()[0].clone();
                partial.with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                    key,
                    PatternBitSet::all(1),
                )])
            }),
            "wasm_extended_distributed_unexpected_solution_coverage"
        );
        assert_eq!(
            rejected_partial_reason(&extended_problem, extended_field, |partial| {
                partial.with_coverage_pattern_words(vec![2])
            }),
            "wasm_extended_distributed_coverage_invalid"
        );

        let (b2b_problem, b2b_field) = mixed_spawn_problem();
        let serial = run_serial(&b2b_problem, b2b_field);
        let distributed = run_distributed(&b2b_problem, b2b_field);
        assert_solution_probability_contract(&serial, false, 1);
        assert_solution_probability_contract(&distributed, false, 1);
        assert_eq!(
            distributed.normalized_solution_coverages(),
            serial.normalized_solution_coverages()
        );
        assert_eq!(distributed.normalized_solution_coverages().len(), 1);
    }

    #[test]
    fn distributed_pass_index_requires_one_canonical_integer() {
        let problem = exact_probability_problem(4, 1, PcSolutionProbabilityPolicy::Include);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial.with_additional_fields(vec![(
                    "build_distributed_pass_index".to_owned(),
                    "0".to_owned(),
                )])
            }),
            "wasm_build_probability_distributed_pass_index_invalid"
        );
        assert_eq!(
            rejected_partial_reason(&problem, field, |partial| {
                partial.with_replaced_fields(vec![(
                    "build_distributed_pass_index".to_owned(),
                    "00".to_owned(),
                )])
            }),
            "wasm_build_probability_distributed_pass_index_invalid"
        );
    }

    #[test]
    fn distributed_finesse_rebuilds_mixed_spawn_coverage_and_solution_coverage() {
        let (problem, field) = mixed_spawn_problem();
        let serial = run_serial(&problem, field);
        let distributed = run_distributed(&problem, field);

        assert_eq!(
            distributed.coverage_pattern_words(),
            serial.coverage_pattern_words()
        );
        assert_eq!(
            distributed.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(
            distributed.normalized_solution_coverages(),
            serial.normalized_solution_coverages()
        );
        assert_eq!(distributed.finesse_report(), serial.finesse_report());
        for field in [
            "piece_source_id",
            "pattern_universe_id",
            "pattern_weight_model_id",
            "coverage_aggregation_contract",
            "coverage_aggregation_availability",
            "coverage_aggregation_complete",
            "coverage_aggregation_source_row_count",
            "covered_pattern_count",
            "failed_pattern_count",
            "coverage_probability",
            "failed_coverage_probability",
            "materialized_probability_mass",
            "coverage_probability_denominator",
            "success_conditional_probability_denominator",
            "probability_complete",
            "count_complete",
        ] {
            assert_eq!(
                distributed.unique_field(field),
                serial.unique_field(field),
                "shared coverage aggregation field {field} must be serial/distributed exact"
            );
        }
        assert_eq!(serial.usize_field("coverage_pattern_count"), Some(2));
        assert_eq!(serial.usize_field("covered_pattern_count"), Some(1));
        assert_eq!(serial.usize_field("failed_pattern_count"), Some(1));
        assert_eq!(serial.field("coverage_probability"), Some("0.5"));
        assert_eq!(serial.field("failed_coverage_probability"), Some("0.5"));
        assert_eq!(
            serial.field("coverage_aggregation_contract"),
            Some("pattern-coverage-aggregation.v1")
        );
        assert_eq!(
            serial.field("coverage_aggregation_availability"),
            Some("available")
        );
        assert_eq!(serial.normalized_solution_coverages().len(), 1);
        assert_eq!(
            serial.normalized_solution_coverages()[0]
                .covered_patterns()
                .count_ones(),
            1
        );
    }

    #[test]
    fn distributed_terminal_hold_matches_serial_micro_oracle_set_and_hash() {
        let first_o = 0x0c03u64;
        let second_o = 0x300cu64;
        let queue = QueuePatternExpression::parse("[O]!", 1).expect("single O pattern");
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::pattern_expression(queue),
            PieceWindow::new(2),
        )
        .with_hold_piece(Some(PieceKind::O))
        .with_exact_pieces(Some(2));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        assert!(problem.supply().projects_unplaced_lookahead());
        let field = BuildProbabilityField::from_words_preserving_height(
            4,
            [0; 4],
            [first_o | second_o, 0, 0, 0],
        )
        .expect("two-O build field");
        let oracle = StandardBoard64TilingIdentity::from_placements(
            0,
            [
                PiecePlacementMask::new(PieceKind::O, first_o),
                PiecePlacementMask::new(PieceKind::O, second_o),
            ],
        )
        .expect("independent two-O build oracle");
        let oracle_hash =
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(&[oracle]);

        let serial = run_serial(&problem, field);
        let distributed = run_distributed(&problem, field);

        for result in [&serial, &distributed] {
            assert_eq!(result.normalized_solution_identities(), &[oracle]);
            assert_eq!(
                result.field("normalized_solution_set_hash"),
                Some(oracle_hash.as_str())
            );
        }
        assert_eq!(
            distributed.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(
            distributed.field("normalized_solution_set_hash"),
            serial.field("normalized_solution_set_hash")
        );
    }
}
