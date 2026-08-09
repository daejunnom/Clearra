use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_pc_graph::request::RequestedSearchBackend;
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    SearchProblem,
};

use crate::CoreExecutionResult;

use super::{
    wasm_cpu::{BuildProbabilityAdvance, WasmBuildProbabilitySession as InnerSession},
    WasmCpuSearchError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WasmBuildProbabilityAdvance {
    Pending,
    Completed(CoreExecutionResult),
    Cancelled,
}

pub struct WasmBuildProbabilitySession {
    inner: InnerSession,
    cpu_fallback_reason: Option<&'static str>,
}

impl WasmBuildProbabilitySession {
    pub fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
    ) -> Result<Self, WasmCpuSearchError> {
        let explicit_gpu =
            problem.backend_policy().requested_backend() == RequestedSearchBackend::Gpu;
        if explicit_gpu && !problem.backend_policy().allow_backend_fallback() {
            return Err(WasmCpuSearchError::Unsupported {
                reason: "webgpu_backend_unavailable",
            });
        }
        Ok(Self {
            inner: InnerSession::new(problem, field, aggregation, finesse).map_err(map_error)?,
            cpu_fallback_reason: explicit_gpu.then_some("gpu_kernel_unavailable"),
        })
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<WasmBuildProbabilityAdvance, WasmCpuSearchError> {
        match self
            .inner
            .advance(work_budget, control)
            .map_err(map_error)?
        {
            BuildProbabilityAdvance::Pending => Ok(WasmBuildProbabilityAdvance::Pending),
            BuildProbabilityAdvance::Completed(result) => Ok(
                WasmBuildProbabilityAdvance::Completed(match self.cpu_fallback_reason {
                    Some(reason) => mark_cpu_fallback(result, reason),
                    None => result,
                }),
            ),
            BuildProbabilityAdvance::Cancelled => Ok(WasmBuildProbabilityAdvance::Cancelled),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WasmBuildProbabilityBackend;

impl WasmBuildProbabilityBackend {
    pub fn execute_with_control(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, WasmCpuSearchError> {
        let mut session = WasmBuildProbabilitySession::new(problem, field, aggregation, finesse)?;
        loop {
            match session.advance(4096, control)? {
                WasmBuildProbabilityAdvance::Pending => {}
                WasmBuildProbabilityAdvance::Completed(result) => return Ok(result),
                WasmBuildProbabilityAdvance::Cancelled => {
                    return Err(WasmCpuSearchError::Cancelled)
                }
            }
        }
    }
}

fn mark_cpu_fallback(result: CoreExecutionResult, reason: &'static str) -> CoreExecutionResult {
    let fallback_backend = result
        .field("backend_selected")
        .unwrap_or("wasm-cpu-build-probability")
        .to_owned();
    result.with_replaced_fields(vec![
        field("backend_fallback_used", true),
        field("fallback_used", true),
        field("backend_fallback_reason", reason),
        field("fallback_backend", fallback_backend),
        field("gpu_available", false),
        field("gpu_disabled_reason", reason),
        field("gpu_trust_state", "fallback-used"),
        field("gpu_failure_class", "unavailable"),
        field("gpu_failure_stage", "capability-query"),
        field("discarded_partial_gpu_result", false),
        field("gpu_original_result_incomplete", false),
    ])
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

fn map_error(error: super::wasm_cpu::WasmExactSearchError) -> WasmCpuSearchError {
    match error {
        super::wasm_cpu::WasmExactSearchError::InvalidProblem(reason) => {
            WasmCpuSearchError::InvalidProblem { reason }
        }
        super::wasm_cpu::WasmExactSearchError::Cancelled => WasmCpuSearchError::Cancelled,
    }
}
