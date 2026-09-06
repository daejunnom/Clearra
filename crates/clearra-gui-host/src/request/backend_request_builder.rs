use clearra_pc_graph::request::{
    GpuDeviceSelection, PcExecutionPolicy, RequestedSearchBackend, WorkerPolicy,
};

use clearra_app::PC_SCORE_MAX_PATTERNS;

use crate::{
    model::GuiBackendForm,
    request::{RequestBuildError, RequestBuildErrorCode},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BackendRequestBuilder;

impl BackendRequestBuilder {
    pub fn build_execution_policy(
        form: &GuiBackendForm,
    ) -> Result<PcExecutionPolicy, RequestBuildError> {
        Self::validate_budget(form)?;
        let backend = RequestedSearchBackend::parse(form.backend_id()).ok_or_else(|| {
            RequestBuildError::new(
                RequestBuildErrorCode::UnknownBackend,
                format!(
                    "unknown GUI backend option '{}'; expected auto, cpu, gpu, or hybrid",
                    form.backend_id()
                ),
            )
        })?;
        if !matches!(
            backend,
            RequestedSearchBackend::Auto
                | RequestedSearchBackend::Cpu
                | RequestedSearchBackend::Gpu
                | RequestedSearchBackend::Hybrid
        ) {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::UnknownBackend,
                format!(
                    "GUI backend option '{}' is not exposed by clearra-ui-schema",
                    form.backend_id()
                ),
            ));
        }

        let mut policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(backend)
            .with_worker_hardware_limit(WorkerPolicy::hardware_worker_limit())
            .with_use_all_logical_processors(form.use_all_logical_processors())
            .with_deterministic(form.deterministic())
            .with_max_candidates(form.candidate_budget() as usize)
            .with_max_frontier_states(form.candidate_budget() as usize)
            .with_max_patterns(form.pattern_budget() as usize)
            .with_precompute_build_dependencies(form.precompute_build_dependencies())
            .with_tablebase_requested(form.tablebase_requested())
            .with_allow_backend_fallback(form.allow_fallback());

        if let Some(workers) = form.workers_requested() {
            policy = policy.with_workers(usize::from(workers));
        }

        // The desktop contract uses zero for an unbounded policy.
        if form.memory_budget_mb() != 0 {
            policy = policy.with_max_memory_mib(Some(u64::from(form.memory_budget_mb())));
        }

        if let Some(device) = form.gpu_device() {
            let gpu_device = GpuDeviceSelection::parse(device).ok_or_else(|| {
                RequestBuildError::new(
                    RequestBuildErrorCode::InvalidGpuDevice,
                    format!("invalid GUI GPU device selector '{device}'"),
                )
            })?;
            policy = policy.with_gpu_device(gpu_device);
        }

        Ok(policy)
    }
}
impl BackendRequestBuilder {
    /// Builds the product-owned execution policy for canonical `pc.score`
    /// requests. The GUI may reach this path from its neutral `auto` default or
    /// from the explicit CPU projection emitted by the Desktop bridge;
    /// other execution selections are active overrides and fail closed.
    pub fn build_pc_score_execution_policy(
        form: &GuiBackendForm,
    ) -> Result<PcExecutionPolicy, RequestBuildError> {
        Self::validate_budget(form)?;
        if !matches!(
            form.backend(),
            crate::GuiBackendChoice::Auto | crate::GuiBackendChoice::Cpu
        ) || form.precompute_build_dependencies()
            || form.tablebase_requested()
            || form.memory_budget_mb() != 0
            || form
                .gpu_device()
                .is_some_and(|device| !device.eq_ignore_ascii_case("auto"))
        {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "canonical GUI pc score request contains an execution override outside its product-owned CPU policy",
            ));
        }

        let mut policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_worker_hardware_limit(WorkerPolicy::hardware_worker_limit())
            .with_use_all_logical_processors(form.use_all_logical_processors())
            .with_deterministic(form.deterministic())
            .with_allow_backend_fallback(false)
            .with_max_patterns(PC_SCORE_MAX_PATTERNS);
        if let Some(workers) = form.workers_requested() {
            policy = policy.with_workers(usize::from(workers));
        }
        Ok(policy)
    }
}
impl BackendRequestBuilder {
    pub fn validate_form(form: &GuiBackendForm) -> Result<(), RequestBuildError> {
        Self::build_execution_policy(form).map(|_| ())
    }
}
impl BackendRequestBuilder {
    fn validate_budget(form: &GuiBackendForm) -> Result<(), RequestBuildError> {
        if form.candidate_budget() == 0 {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::InvalidBudget,
                "GUI backend form requires a nonzero candidate budget",
            ));
        }
        if form.pattern_budget() == 0 {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::InvalidBudget,
                "GUI backend form requires a nonzero pattern budget",
            ));
        }
        Ok(())
    }
}
