use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::{BuildProbabilityAggregation, BuildProbabilityField, SearchProblem};

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
}

impl WasmBuildProbabilitySession {
    pub fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmCpuSearchError> {
        Ok(Self {
            inner: InnerSession::new(problem, field, aggregation).map_err(map_error)?,
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
            BuildProbabilityAdvance::Completed(result) => {
                Ok(WasmBuildProbabilityAdvance::Completed(result))
            }
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
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, WasmCpuSearchError> {
        let mut session = WasmBuildProbabilitySession::new(problem, field, aggregation)?;
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

fn map_error(error: super::wasm_cpu::WasmExactSearchError) -> WasmCpuSearchError {
    match error {
        super::wasm_cpu::WasmExactSearchError::InvalidProblem(reason) => {
            WasmCpuSearchError::InvalidProblem { reason }
        }
        super::wasm_cpu::WasmExactSearchError::Cancelled => WasmCpuSearchError::Cancelled,
    }
}
