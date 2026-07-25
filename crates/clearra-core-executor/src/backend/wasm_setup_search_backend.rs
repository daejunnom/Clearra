use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SetupSearchQuery;

use crate::CoreExecutionResult;

use super::{
    wasm_cpu::{WasmSetupSearchAdvance as InnerAdvance, WasmSetupSearchSession as InnerSession},
    WasmCpuSearchError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WasmSetupSearchAdvance {
    Pending,
    Completed(CoreExecutionResult),
    Cancelled,
}

pub struct WasmSetupSearchSession {
    inner: InnerSession,
}

impl WasmSetupSearchSession {
    pub fn new(query: &SetupSearchQuery) -> Result<Self, WasmCpuSearchError> {
        Ok(Self {
            inner: InnerSession::new(query).map_err(map_error)?,
        })
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<WasmSetupSearchAdvance, WasmCpuSearchError> {
        match self
            .inner
            .advance(work_budget, control)
            .map_err(map_error)?
        {
            InnerAdvance::Pending => Ok(WasmSetupSearchAdvance::Pending),
            InnerAdvance::Completed(result) => Ok(WasmSetupSearchAdvance::Completed(result)),
            InnerAdvance::Cancelled => Ok(WasmSetupSearchAdvance::Cancelled),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WasmSetupSearchBackend;

impl WasmSetupSearchBackend {
    pub fn execute_with_control(
        query: &SetupSearchQuery,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, WasmCpuSearchError> {
        let mut session = WasmSetupSearchSession::new(query)?;
        loop {
            match session.advance(4096, control)? {
                WasmSetupSearchAdvance::Pending => {}
                WasmSetupSearchAdvance::Completed(result) => return Ok(result),
                WasmSetupSearchAdvance::Cancelled => return Err(WasmCpuSearchError::Cancelled),
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
