use clearra_pc_graph::request::{GpuDeviceSelection, PcExecutionPolicy, RequestedSearchBackend};

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
            .with_workers(usize::from(form.workers()))
            .with_deterministic(form.deterministic())
            .with_max_candidates(form.candidate_budget() as usize)
            .with_max_frontier_states(form.candidate_budget() as usize)
            .with_max_patterns(form.pattern_budget() as usize)
            .with_max_memory_mib(Some(u64::from(form.memory_budget_mb())))
            .with_precompute_build_dependencies(form.precompute_build_dependencies())
            .with_allow_backend_fallback(form.allow_fallback());

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
    pub fn validate_form(form: &GuiBackendForm) -> Result<(), RequestBuildError> {
        Self::build_execution_policy(form).map(|_| ())
    }
}
impl BackendRequestBuilder {
    fn validate_budget(form: &GuiBackendForm) -> Result<(), RequestBuildError> {
        if form.workers() == 0 {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::InvalidBudget,
                "GUI backend form requires at least one worker",
            ));
        }
        if form.memory_budget_mb() == 0 {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::InvalidBudget,
                "GUI backend form requires a nonzero memory budget",
            ));
        }
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
