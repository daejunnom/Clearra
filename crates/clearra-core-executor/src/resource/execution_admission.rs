// SRP rationale: this module has one change reason: admission and lease ownership for one bounded execution.
use clearra_core_domain::resource::{
    ExecutionAvailability, ExecutionAvailabilityReason, ExecutionAvailabilityState, ResourceLease,
    ResourceLeaseRequest, ResourceLeaseToken, ResourceReport,
};
use clearra_core_ffi::{CPackingCandidate, CPackingProblem, NativeGeometrySolutionTask};
use clearra_problem::SearchProblem;

use super::{
    acquire_shared_execution_resources, next_execution_resource_owner,
    preflight_dense_pattern_execution, shared_execution_resource_capacity,
    DensePatternExecutionSurface, DensePatternPreflight, WasmCpuTerminalResourceAuthority,
};

/// A checked upper bound for count-proportional allocations owned by one
/// execution. This is deliberately separate from the one-bitset dense
/// representation evidence carried by [`DensePatternPreflight`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionMemoryProjection {
    pub dense_bitset_bytes: u128,
    pub dense_bitset_copies: u128,
    pub per_pattern_bytes: u128,
    pub fixed_bytes: u128,
    pub required_memory_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionAdmissionPlan {
    dense_bitset_copies: u128,
    per_pattern_bytes: u128,
    fixed_bytes: u128,
    compute_units: Option<u128>,
}

impl ExecutionAdmissionPlan {
    pub(crate) const fn new(
        dense_bitset_copies: u128,
        per_pattern_bytes: u128,
        fixed_bytes: u128,
    ) -> Self {
        Self {
            dense_bitset_copies,
            per_pattern_bytes,
            fixed_bytes,
            compute_units: None,
        }
    }

    const fn with_compute_units(mut self, compute_units: u128) -> Self {
        self.compute_units = Some(compute_units);
        self
    }

    pub(crate) const fn exact_search() -> Self {
        Self::new(1, 0, 0)
    }

    pub(crate) fn build_probability(problem: &SearchProblem, pass_count: usize) -> Self {
        let retained_coverage = if problem.solution_probability_policy().requested()
            || problem.objective().execution_constraints().requested()
        {
            problem.budget().max_results().max(1) as u128
        } else {
            0
        };
        let passes = pass_count.max(1) as u128;
        Self::new(
            passes.saturating_mul(retained_coverage.saturating_add(1)),
            (core::mem::size_of::<usize>() * 2) as u128,
            0,
        )
        .with_compute_units(1)
    }

    pub(crate) fn build_probability_with_verifiers(
        problem: &SearchProblem,
        pass_count: usize,
        verifier_count: usize,
    ) -> Option<Self> {
        let replica = Self::build_probability(problem, pass_count);
        let replicas = u128::try_from(verifier_count).ok()?.checked_add(1)?;
        Some(
            Self::new(
                replica.dense_bitset_copies.checked_mul(replicas)?,
                replica.per_pattern_bytes.checked_mul(replicas)?,
                replica.fixed_bytes.checked_mul(replicas)?,
            )
            .with_compute_units(replicas),
        )
    }

    pub(crate) fn native_packing(problem: &CPackingProblem, worker_count: usize) -> Option<Self> {
        const TASKS_PER_WORKER: u128 = 4;
        const RETAINED_CANDIDATE_CAPACITY: u128 = 8_192;

        let workers = u128::try_from(worker_count.max(1)).ok()?;
        let width = u128::from(problem.board.width.max(1));
        let height = u128::from(
            if problem.board.search_height == 0 {
                problem.board.visible_height
            } else {
                problem.board.search_height
            }
            .max(1),
        );
        let board_cells = width.checked_mul(height)?;
        let catalog_rows = board_cells.checked_mul(7)?.checked_mul(4)?;
        let catalog_bytes = catalog_rows.checked_mul(
            (core::mem::size_of::<clearra_core_ffi::CPackingOperation>()
                + core::mem::size_of::<usize>() * 3) as u128,
        )?;
        let graph_bytes = catalog_rows.checked_mul(
            (core::mem::size_of::<u64>() * 8 + core::mem::size_of::<usize>() * 4) as u128,
        )?;
        let task_bytes = workers
            .checked_mul(TASKS_PER_WORKER)?
            .checked_mul(core::mem::size_of::<NativeGeometrySolutionTask>() as u128)?;
        let worker_scratch_bytes = catalog_rows
            .checked_mul((core::mem::size_of::<u64>() * 4) as u128)?
            .checked_mul(workers)?;
        let retained_candidates = if problem.budget.max_results == 0 {
            RETAINED_CANDIDATE_CAPACITY
        } else {
            u128::from(problem.budget.max_results)
        };
        let retained_candidate_bytes = retained_candidates.checked_mul(
            (core::mem::size_of::<CPackingCandidate>() + core::mem::size_of::<u32>() * 3) as u128,
        )?;
        let fixed_bytes = catalog_bytes
            .checked_add(graph_bytes)?
            .checked_add(task_bytes)?
            .checked_add(worker_scratch_bytes)?
            .checked_add(retained_candidate_bytes)?;
        Some(
            Self::new(
                u128::from(problem.piece_multiset_family.count.max(1)),
                0,
                fixed_bytes,
            )
            .with_compute_units(workers),
        )
    }

    pub(crate) fn finesse_score(problem: &SearchProblem) -> Self {
        let retained_coverage = if problem.solution_probability_policy().requested() {
            problem.budget().max_results().max(1)
        } else {
            0
        };
        // Queue class materialization owns at least a sequence identity, score,
        // and rank per pattern. Use a stable conservative bound independent of
        // the target ABI's Vec bookkeeping.
        Self::new((1 + retained_coverage) as u128, 32, 0)
    }
}

pub(crate) struct ExecutionAdmission {
    pub dense_preflight: DensePatternPreflight,
    projection: ExecutionMemoryProjection,
    memory_enforcement_cap_bytes: u128,
    lease: ResourceLease,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionMemoryBound {
    descriptor_pattern_count: u128,
    dense_pattern_count: u128,
    required_dense_bytes: u128,
    cap_bytes: u128,
}

impl ExecutionMemoryBound {
    pub(crate) fn unbounded_for_problem(problem: &SearchProblem) -> Result<Self, ResourceReport> {
        let universe = problem
            .piece_source()
            .materialized_universe()
            .ok_or_else(|| {
                ResourceReport::admission_failure(ExecutionAvailability::unavailable(
                    ExecutionAvailabilityReason::CapabilityUnavailable,
                ))
            })?;
        let preflight = preflight_dense_pattern_execution(
            universe.total_possible_pattern_count(),
            universe.pattern_count() as u128,
            DensePatternExecutionSurface::current(),
            None,
        );
        if preflight.availability.state() != ExecutionAvailabilityState::Available {
            return Err(ResourceReport::admission_failure(preflight.availability));
        }
        Ok(Self {
            descriptor_pattern_count: preflight.descriptor_pattern_count,
            dense_pattern_count: preflight.dense_pattern_count,
            required_dense_bytes: preflight.required_dense_bytes,
            cap_bytes: u128::MAX,
        })
    }

    pub(crate) const fn cap_bytes(self) -> u128 {
        self.cap_bytes
    }

    pub(crate) fn with_cap(self, cap_bytes: u128) -> Result<Self, ResourceReport> {
        if cap_bytes <= self.cap_bytes {
            return Ok(Self { cap_bytes, ..self });
        }
        Err(admission_failure(
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded),
            self.descriptor_pattern_count,
            self.dense_pattern_count,
            self.required_dense_bytes,
            cap_bytes,
        ))
    }

    pub(crate) fn ensure(
        self,
        observed_retained_bytes: u128,
        checked_future_bytes: u128,
    ) -> Result<(), ResourceReport> {
        let required_memory_bytes = observed_retained_bytes
            .checked_add(checked_future_bytes)
            .ok_or_else(|| {
                admission_failure(
                    ExecutionAvailability::unavailable(
                        ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
                    ),
                    self.descriptor_pattern_count,
                    self.dense_pattern_count,
                    self.required_dense_bytes,
                    u128::MAX,
                )
            })?;
        if required_memory_bytes <= self.cap_bytes {
            return Ok(());
        }
        Err(admission_failure(
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded),
            self.descriptor_pattern_count,
            self.dense_pattern_count,
            self.required_dense_bytes,
            required_memory_bytes,
        ))
    }
}

impl ExecutionAdmission {
    pub(crate) fn lease_token(&self) -> ResourceLeaseToken {
        self.lease.token()
    }

    pub(crate) const fn memory_cap_bytes(&self) -> u128 {
        self.memory_enforcement_cap_bytes
    }

    pub(crate) const fn memory_bound(&self) -> ExecutionMemoryBound {
        ExecutionMemoryBound {
            descriptor_pattern_count: self.dense_preflight.descriptor_pattern_count,
            dense_pattern_count: self.dense_preflight.dense_pattern_count,
            required_dense_bytes: self.dense_preflight.required_dense_bytes,
            cap_bytes: self.memory_enforcement_cap_bytes,
        }
    }

    pub(crate) fn ensure_memory_bound(
        &self,
        observed_retained_bytes: u128,
        checked_future_bytes: u128,
    ) -> Result<(), ResourceReport> {
        self.memory_bound()
            .ensure(observed_retained_bytes, checked_future_bytes)
    }

    /// Verifies a count-proportional allocation plan against this admission's
    /// configured memory cap without acquiring another lease. Callers use this
    /// before constructing catalogs, bitsets, or queue-class storage; runtime
    /// retained/transient accounting continues to use the full configured cap.
    pub(crate) fn ensure_plan(
        &self,
        plan: ExecutionAdmissionPlan,
        additional_fixed_bytes: u128,
    ) -> Result<(), ResourceReport> {
        let projection = project_memory(
            self.dense_preflight.dense_pattern_count,
            self.dense_preflight.required_dense_bytes,
            plan,
        )
        .ok_or_else(|| {
            admission_failure(
                ExecutionAvailability::unavailable(
                    ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
                ),
                self.dense_preflight.descriptor_pattern_count,
                self.dense_preflight.dense_pattern_count,
                self.dense_preflight.required_dense_bytes,
                u128::MAX,
            )
        })?;
        self.memory_bound()
            .ensure(projection.required_memory_bytes, additional_fixed_bytes)
    }

    pub(crate) fn try_delegate_compute_only_with_memory_cap(
        &self,
        memory_enforcement_cap_bytes: u128,
    ) -> Result<Self, ResourceReport> {
        if memory_enforcement_cap_bytes > self.memory_cap_bytes() {
            return Err(admission_failure(
                ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded),
                self.dense_preflight.descriptor_pattern_count,
                self.dense_preflight.dense_pattern_count,
                self.dense_preflight.required_dense_bytes,
                memory_enforcement_cap_bytes,
            ));
        }
        let request = ResourceLeaseRequest::new(1, 0).expect("one compute unit is a valid request");
        let lease = self
            .lease
            .try_child(next_execution_resource_owner(), request)
            .map_err(|error| {
                admission_failure(
                    error.availability(),
                    self.dense_preflight.descriptor_pattern_count,
                    self.dense_preflight.dense_pattern_count,
                    self.dense_preflight.required_dense_bytes,
                    self.memory_cap_bytes(),
                )
            })?;
        Ok(Self {
            dense_preflight: self.dense_preflight,
            projection: ExecutionMemoryProjection {
                dense_bitset_bytes: self.projection.dense_bitset_bytes,
                dense_bitset_copies: 0,
                per_pattern_bytes: 0,
                fixed_bytes: 0,
                required_memory_bytes: 0,
            },
            memory_enforcement_cap_bytes,
            lease,
        })
    }

    pub(crate) fn try_delegate(
        &self,
        plan: ExecutionAdmissionPlan,
    ) -> Result<Self, ResourceReport> {
        let projection = project_memory(
            self.dense_preflight.dense_pattern_count,
            self.dense_preflight.required_dense_bytes,
            plan,
        )
        .ok_or_else(|| {
            admission_failure(
                ExecutionAvailability::unavailable(
                    ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
                ),
                self.dense_preflight.descriptor_pattern_count,
                self.dense_preflight.dense_pattern_count,
                self.dense_preflight.required_dense_bytes,
                u128::MAX,
            )
        })?;
        let compute_units = u32::try_from(plan.compute_units.unwrap_or(1)).map_err(|_| {
            admission_failure(
                ExecutionAvailability::exhausted(
                    ExecutionAvailabilityReason::ComputeBudgetExceeded,
                ),
                self.dense_preflight.descriptor_pattern_count,
                self.dense_preflight.dense_pattern_count,
                self.dense_preflight.required_dense_bytes,
                projection.required_memory_bytes,
            )
        })?;
        let memory_bytes = u64::try_from(projection.required_memory_bytes).map_err(|_| {
            admission_failure(
                ExecutionAvailability::unavailable(
                    ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
                ),
                self.dense_preflight.descriptor_pattern_count,
                self.dense_preflight.dense_pattern_count,
                self.dense_preflight.required_dense_bytes,
                projection.required_memory_bytes,
            )
        })?;
        let request = ResourceLeaseRequest::new(compute_units, memory_bytes).ok_or_else(|| {
            admission_failure(
                ExecutionAvailability::unavailable(
                    ExecutionAvailabilityReason::CapabilityUnavailable,
                ),
                self.dense_preflight.descriptor_pattern_count,
                self.dense_preflight.dense_pattern_count,
                self.dense_preflight.required_dense_bytes,
                projection.required_memory_bytes,
            )
        })?;
        let lease = self
            .lease
            .try_child(next_execution_resource_owner(), request)
            .map_err(|error| {
                admission_failure(
                    error.availability(),
                    self.dense_preflight.descriptor_pattern_count,
                    self.dense_preflight.dense_pattern_count,
                    self.dense_preflight.required_dense_bytes,
                    projection.required_memory_bytes,
                )
            })?;
        let mut dense_preflight = self.dense_preflight;
        dense_preflight.availability = dense_preflight
            .availability
            .with_required_memory_bytes(projection.required_memory_bytes);
        Ok(Self {
            dense_preflight,
            projection,
            memory_enforcement_cap_bytes: projection.required_memory_bytes,
            lease,
        })
    }
}

fn budget_bound_execution_preflight(
    problem: &SearchProblem,
) -> Result<(DensePatternPreflight, u128, u128, u128), ResourceReport> {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or_else(|| {
            ResourceReport::admission_failure(ExecutionAvailability::unavailable(
                ExecutionAvailabilityReason::CapabilityUnavailable,
            ))
        })?;
    let descriptor_pattern_count = universe.total_possible_pattern_count();
    let dense_pattern_count = universe.pattern_count() as u128;
    let host_capacity = shared_execution_resource_capacity();
    let configured_memory_bytes = problem
        .backend_request()
        .max_memory_mib()
        .map(|mib| u128::from(mib).checked_mul(1024 * 1024))
        .transpose_option()
        .ok_or_else(|| {
            admission_failure(
                ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded),
                descriptor_pattern_count,
                dense_pattern_count,
                u128::MAX,
                u128::MAX,
            )
        })?
        .unwrap_or(u128::from(host_capacity.memory_bytes));
    let mut dense_preflight = preflight_dense_pattern_execution(
        descriptor_pattern_count,
        dense_pattern_count,
        DensePatternExecutionSurface::current(),
        Some(configured_memory_bytes),
    );
    if dense_preflight.availability.state() != ExecutionAvailabilityState::Available {
        return Err(admission_failure(
            dense_preflight.availability,
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            dense_preflight.required_dense_bytes,
        ));
    }
    dense_preflight.availability = dense_preflight
        .availability
        .with_required_memory_bytes(configured_memory_bytes);
    Ok((
        dense_preflight,
        descriptor_pattern_count,
        dense_pattern_count,
        configured_memory_bytes,
    ))
}

/// Acquires the configured request cap, or the complete host cap when the
/// request is unbounded. Count-proportional engines using this admission must
/// enforce this same cap against their actual retained and transient storage;
/// the lease alone is not allocation evidence.
pub(crate) fn admit_budget_bound_search_execution(
    problem: &SearchProblem,
    requested_compute_units: usize,
) -> Result<ExecutionAdmission, ResourceReport> {
    let (dense_preflight, descriptor_pattern_count, dense_pattern_count, configured_memory_bytes) =
        budget_bound_execution_preflight(problem)?;
    let compute_units = u32::try_from(requested_compute_units.max(1)).map_err(|_| {
        admission_failure(
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::ComputeBudgetExceeded),
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            configured_memory_bytes,
        )
    })?;
    let memory_bytes = u64::try_from(configured_memory_bytes).map_err(|_| {
        admission_failure(
            ExecutionAvailability::unavailable(
                ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
            ),
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            configured_memory_bytes,
        )
    })?;
    let request = ResourceLeaseRequest::new(compute_units, memory_bytes).ok_or_else(|| {
        admission_failure(
            ExecutionAvailability::unavailable(ExecutionAvailabilityReason::CapabilityUnavailable),
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            configured_memory_bytes,
        )
    })?;
    let lease = acquire_shared_execution_resources(next_execution_resource_owner(), request)
        .map_err(|error| {
            admission_failure(
                error.availability(),
                descriptor_pattern_count,
                dense_pattern_count,
                dense_preflight.required_dense_bytes,
                configured_memory_bytes,
            )
        })?;
    Ok(ExecutionAdmission {
        dense_preflight,
        projection: ExecutionMemoryProjection {
            dense_bitset_bytes: dense_preflight.required_dense_bytes,
            dense_bitset_copies: 0,
            per_pattern_bytes: 0,
            fixed_bytes: configured_memory_bytes,
            required_memory_bytes: configured_memory_bytes,
        },
        memory_enforcement_cap_bytes: configured_memory_bytes,
        lease,
    })
}

/// Creates a serial exact-search admission under a request-level parent that
/// already owns the complete physical memory surface. The child consumes only
/// the parent's compute slot; the configured request cap and conservative
/// external retained bound remain logical checks over the same parent memory.
pub(crate) fn admit_budget_bound_search_execution_under_terminal_authority(
    problem: &SearchProblem,
    checked_external_retained_upper_bound_bytes: u128,
    authority: &WasmCpuTerminalResourceAuthority,
) -> Result<ExecutionAdmission, ResourceReport> {
    let (dense_preflight, descriptor_pattern_count, dense_pattern_count, configured_memory_bytes) =
        budget_bound_execution_preflight(problem)?;
    if configured_memory_bytes > authority.memory_capacity_bytes() {
        return Err(admission_failure(
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded),
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            configured_memory_bytes,
        ));
    }
    ExecutionMemoryBound {
        descriptor_pattern_count,
        dense_pattern_count,
        required_dense_bytes: dense_preflight.required_dense_bytes,
        cap_bytes: configured_memory_bytes,
    }
    .ensure(checked_external_retained_upper_bound_bytes, 0)?;
    let lease = authority.try_acquire_compute_child().map_err(|error| {
        admission_failure(
            error.availability(),
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            configured_memory_bytes,
        )
    })?;
    Ok(ExecutionAdmission {
        dense_preflight,
        projection: ExecutionMemoryProjection {
            dense_bitset_bytes: dense_preflight.required_dense_bytes,
            dense_bitset_copies: 0,
            per_pattern_bytes: 0,
            fixed_bytes: configured_memory_bytes,
            required_memory_bytes: configured_memory_bytes,
        },
        memory_enforcement_cap_bytes: configured_memory_bytes,
        lease,
    })
}

pub(crate) fn admit_search_execution(
    problem: &SearchProblem,
    plan: ExecutionAdmissionPlan,
) -> Result<ExecutionAdmission, ResourceReport> {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or_else(|| {
            ResourceReport::admission_failure(ExecutionAvailability::unavailable(
                ExecutionAvailabilityReason::CapabilityUnavailable,
            ))
        })?;
    let descriptor_pattern_count = universe.total_possible_pattern_count();
    let dense_pattern_count = universe.pattern_count() as u128;
    let request_memory_budget_bytes = problem
        .backend_request()
        .max_memory_mib()
        .map(|mib| u128::from(mib).checked_mul(1024 * 1024))
        .transpose_option()
        .ok_or_else(|| {
            admission_failure(
                ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded),
                descriptor_pattern_count,
                dense_pattern_count,
                u128::MAX,
                u128::MAX,
            )
        })?;

    let mut dense_preflight = preflight_dense_pattern_execution(
        descriptor_pattern_count,
        dense_pattern_count,
        DensePatternExecutionSurface::current(),
        request_memory_budget_bytes,
    );
    let projection = project_memory(
        dense_pattern_count,
        dense_preflight.required_dense_bytes,
        plan,
    )
    .ok_or_else(|| {
        admission_failure(
            ExecutionAvailability::unavailable(
                ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
            ),
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            u128::MAX,
        )
    })?;
    dense_preflight.availability = dense_preflight
        .availability
        .with_required_memory_bytes(projection.required_memory_bytes);
    if dense_preflight.availability.state() != ExecutionAvailabilityState::Available {
        return Err(ResourceReport::admission_failure(
            dense_preflight.availability,
        ));
    }
    if request_memory_budget_bytes.is_some_and(|budget| projection.required_memory_bytes > budget) {
        return Err(admission_failure(
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded),
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            projection.required_memory_bytes,
        ));
    }

    let requested_compute_units = plan.compute_units.unwrap_or_else(|| {
        if cfg!(target_family = "wasm") {
            1
        } else {
            problem
                .backend_policy()
                .workers()
                .min(problem.backend_policy().worker_hardware_limit())
                .max(1) as u128
        }
    });
    let compute_units = u32::try_from(requested_compute_units).map_err(|_| {
        admission_failure(
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::ComputeBudgetExceeded),
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            projection.required_memory_bytes,
        )
    })?;
    let memory_bytes = u64::try_from(projection.required_memory_bytes).map_err(|_| {
        admission_failure(
            ExecutionAvailability::unavailable(
                ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
            ),
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            projection.required_memory_bytes,
        )
    })?;
    let request = ResourceLeaseRequest::new(compute_units, memory_bytes).ok_or_else(|| {
        admission_failure(
            ExecutionAvailability::unavailable(ExecutionAvailabilityReason::CapabilityUnavailable),
            descriptor_pattern_count,
            dense_pattern_count,
            dense_preflight.required_dense_bytes,
            projection.required_memory_bytes,
        )
    })?;
    let lease = acquire_shared_execution_resources(next_execution_resource_owner(), request)
        .map_err(|error| {
            admission_failure(
                error.availability(),
                descriptor_pattern_count,
                dense_pattern_count,
                dense_preflight.required_dense_bytes,
                projection.required_memory_bytes,
            )
        })?;

    Ok(ExecutionAdmission {
        dense_preflight,
        projection,
        memory_enforcement_cap_bytes: projection.required_memory_bytes,
        lease,
    })
}

fn project_memory(
    dense_pattern_count: u128,
    dense_bitset_bytes: u128,
    plan: ExecutionAdmissionPlan,
) -> Option<ExecutionMemoryProjection> {
    let bitsets = dense_bitset_bytes.checked_mul(plan.dense_bitset_copies)?;
    let linear = dense_pattern_count.checked_mul(plan.per_pattern_bytes)?;
    let required_memory_bytes = bitsets.checked_add(linear)?.checked_add(plan.fixed_bytes)?;
    Some(ExecutionMemoryProjection {
        dense_bitset_bytes,
        dense_bitset_copies: plan.dense_bitset_copies,
        per_pattern_bytes: plan.per_pattern_bytes,
        fixed_bytes: plan.fixed_bytes,
        required_memory_bytes,
    })
}

fn admission_failure(
    availability: ExecutionAvailability,
    descriptor_pattern_count: u128,
    dense_pattern_count: u128,
    required_dense_bytes: u128,
    required_memory_bytes: u128,
) -> ResourceReport {
    ResourceReport::admission_failure(
        availability
            .with_pattern_evidence(
                descriptor_pattern_count,
                dense_pattern_count,
                required_dense_bytes,
            )
            .with_required_memory_bytes(required_memory_bytes),
    )
}

trait OptionTranspose<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}

impl<T> OptionTranspose<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            Some(Some(value)) => Some(Some(value)),
            Some(None) => None,
            None => Some(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{
        PcQueueInput, PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy, PieceWindow,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;

    fn one_piece_scenario_problem() -> SearchProblem {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        ProblemCompiler::compile_scenario_pc(&query).expect("one-piece scenario problem")
    }

    #[test]
    fn projection_is_checked_and_includes_every_declared_component() {
        let projection = project_memory(4_096, 512, ExecutionAdmissionPlan::new(3, 16, 7))
            .expect("bounded projection");
        assert_eq!(projection.required_memory_bytes, 512 * 3 + 4_096 * 16 + 7);
        assert!(
            projection.required_memory_bytes
                >= projection.dense_bitset_bytes * projection.dense_bitset_copies
        );
    }

    #[test]
    fn projection_overflow_fails_closed() {
        assert!(
            project_memory(u128::MAX, u128::MAX, ExecutionAdmissionPlan::new(2, 2, 1),).is_none()
        );
    }

    #[test]
    fn failed_projection_evidence_does_not_claim_execution_or_completeness() {
        let report = admission_failure(
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded),
            99,
            77,
            16,
            55,
        );
        assert!(!report.execution_started());
        assert!(!report.result_complete());
        assert_eq!(
            report.execution_availability().descriptor_pattern_count(),
            Some(99)
        );
        assert_eq!(
            report.execution_availability().dense_pattern_count(),
            Some(77)
        );
        assert_eq!(
            report.execution_availability().required_dense_bytes(),
            Some(16)
        );
        assert_eq!(
            report.execution_availability().required_memory_bytes(),
            Some(55)
        );
    }

    #[test]
    fn native_packing_projection_scales_workers_and_preserves_checked_large_counts() {
        let mut problem = CPackingProblem::default();
        problem.board.width = 10;
        problem.board.visible_height = 4;
        problem.budget.max_results = 128;
        problem.piece_multiset_family.count = 3;
        let one = ExecutionAdmissionPlan::native_packing(&problem, 1).expect("one worker");
        let four = ExecutionAdmissionPlan::native_packing(&problem, 4).expect("four workers");
        assert_eq!(one.compute_units, Some(1));
        assert_eq!(four.compute_units, Some(4));
        assert!(four.fixed_bytes > one.fixed_bytes);

        problem.board.width = u16::MAX;
        problem.board.search_height = u16::MAX;
        let oversized = ExecutionAdmissionPlan::native_packing(&problem, usize::MAX)
            .expect("u128 projection remains representable");
        assert!(oversized.compute_units.expect("compute projection") > u128::from(u32::MAX));
    }

    #[test]
    fn build_probability_projection_distinguishes_native_totals_one_two_and_four() {
        let problem = one_piece_scenario_problem();
        let one = ExecutionAdmissionPlan::build_probability(&problem, 1);
        let two = ExecutionAdmissionPlan::build_probability_with_verifiers(&problem, 1, 1)
            .expect("producer plus one verifier");
        let four = ExecutionAdmissionPlan::build_probability_with_verifiers(&problem, 1, 3)
            .expect("producer plus three verifiers");
        assert_eq!(one.compute_units, Some(1));
        assert_eq!(two.compute_units, Some(2));
        assert_eq!(four.compute_units, Some(4));
        assert_eq!(two.dense_bitset_copies, one.dense_bitset_copies * 2);
        assert_eq!(four.dense_bitset_copies, one.dense_bitset_copies * 4);
        assert_eq!(two.per_pattern_bytes, one.per_pattern_bytes * 2);
        assert_eq!(four.per_pattern_bytes, one.per_pattern_bytes * 4);
    }

    #[test]
    fn build_probability_omit_with_b2b_constraint_still_reserves_private_coverage_per_pass() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
        .with_objective(
            ObjectivePolicy::unique().with_back_to_back_preservation(SpinProfileSelection::TSpins),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("B2B problem");
        assert_eq!(
            problem.solution_probability_policy(),
            PcSolutionProbabilityPolicy::Omit
        );
        assert!(problem.objective().execution_constraints().requested());

        let retained_coverage = problem.budget().max_results().max(1) as u128;
        let one = ExecutionAdmissionPlan::build_probability(&problem, 1);
        let two = ExecutionAdmissionPlan::build_probability(&problem, 2);
        assert_eq!(one.dense_bitset_copies, retained_coverage + 1);
        assert_eq!(two.dense_bitset_copies, (retained_coverage + 1) * 2);
    }

    #[test]
    fn delegated_verifiers_preserve_parent_accounting_and_explicit_identity() {
        let authority = clearra_core_domain::resource::SharedResourceLeaseAuthority::new(
            clearra_core_domain::resource::ResourceLeaseCapacity::new(4, 32)
                .expect("test capacity"),
        );
        let parent_owner =
            clearra_core_domain::resource::ResourceLeaseOwnerId::new(700).expect("parent owner");
        let parent_lease = authority
            .try_acquire(
                parent_owner,
                clearra_core_domain::resource::ResourceLeaseRequest::new(4, 32)
                    .expect("parent request"),
            )
            .expect("parent grant");
        let parent_token = parent_lease.token();
        let parent = ExecutionAdmission {
            dense_preflight: DensePatternPreflight {
                descriptor_pattern_count: 64,
                dense_pattern_count: 64,
                dense_word_count: 1,
                required_dense_bytes: 8,
                availability: ExecutionAvailability::available().with_pattern_evidence(64, 64, 8),
            },
            projection: ExecutionMemoryProjection {
                dense_bitset_bytes: 8,
                dense_bitset_copies: 4,
                per_pattern_bytes: 0,
                fixed_bytes: 0,
                required_memory_bytes: 32,
            },
            memory_enforcement_cap_bytes: 32,
            lease: parent_lease,
        };

        let mut children = Vec::new();
        for _ in 0..3 {
            let child = parent
                .try_delegate(ExecutionAdmissionPlan::new(1, 0, 0).with_compute_units(1))
                .expect("delegated verifier grant");
            let token = child.lease.token();
            assert_eq!(token.parent_epoch(), Some(parent_token.epoch()));
            assert_ne!(token.owner(), parent_token.owner());
            assert_eq!(token.grant().compute_units, 1);
            assert_eq!(token.grant().memory_bytes, 8);
            children.push(child);
        }
        assert_eq!(authority.available().compute_units, 0);
        assert_eq!(authority.available().memory_bytes, 0);

        drop(parent);
        drop(children);
        assert_eq!(authority.available().compute_units, 4);
        assert_eq!(authority.available().memory_bytes, 32);
    }

    #[test]
    fn compute_only_verifiers_share_parent_memory_cap_without_duplicate_accounting() {
        let authority = clearra_core_domain::resource::SharedResourceLeaseAuthority::new(
            clearra_core_domain::resource::ResourceLeaseCapacity::new(4, 64)
                .expect("test capacity"),
        );
        let parent_lease = authority
            .try_acquire(
                clearra_core_domain::resource::ResourceLeaseOwnerId::new(800)
                    .expect("parent owner"),
                clearra_core_domain::resource::ResourceLeaseRequest::new(4, 64)
                    .expect("parent request"),
            )
            .expect("parent grant");
        let parent_token = parent_lease.token();
        let parent = ExecutionAdmission {
            dense_preflight: DensePatternPreflight {
                descriptor_pattern_count: 64,
                dense_pattern_count: 64,
                dense_word_count: 1,
                required_dense_bytes: 8,
                availability: ExecutionAvailability::available().with_pattern_evidence(64, 64, 8),
            },
            projection: ExecutionMemoryProjection {
                dense_bitset_bytes: 8,
                dense_bitset_copies: 1,
                per_pattern_bytes: 0,
                fixed_bytes: 56,
                required_memory_bytes: 64,
            },
            memory_enforcement_cap_bytes: 64,
            lease: parent_lease,
        };

        let children = (0..3)
            .map(|_| {
                parent
                    .try_delegate_compute_only_with_memory_cap(16)
                    .expect("compute-only child")
            })
            .collect::<Vec<_>>();
        for child in &children {
            let token = child.lease_token();
            assert_eq!(token.parent_epoch(), Some(parent_token.epoch()));
            assert_ne!(token.owner(), parent_token.owner());
            assert_eq!(token.grant().compute_units, 1);
            assert_eq!(token.grant().memory_bytes, 0);
            assert_eq!(child.memory_cap_bytes(), 16);
        }
        assert_eq!(authority.available().compute_units, 0);
        assert_eq!(authority.available().memory_bytes, 0);

        drop(parent);
        drop(children);
        assert_eq!(
            authority.available(),
            clearra_core_domain::resource::ResourceLeaseCapacity::new(4, 64).unwrap()
        );
    }
}
