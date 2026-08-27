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
        Self::new_with_observation_workers(query, 1)
    }

    pub fn new_with_observation_workers(
        query: &SetupSearchQuery,
        workers: usize,
    ) -> Result<Self, WasmCpuSearchError> {
        Ok(Self {
            inner: InnerSession::new_with_observation_workers(query, workers).map_err(map_error)?,
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

    fn coarse_progress(&self) -> (&'static str, u64) {
        self.inner.coarse_progress()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WasmSetupSearchBackend;

impl WasmSetupSearchBackend {
    pub fn execute_with_control(
        query: &SetupSearchQuery,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, WasmCpuSearchError> {
        Self::execute_with_observation_workers_and_control(query, 1, control)
    }

    pub fn execute_with_observation_workers_and_control(
        query: &SetupSearchQuery,
        workers: usize,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, WasmCpuSearchError> {
        let mut session = WasmSetupSearchSession::new_with_observation_workers(query, workers)?;
        let mut last_progress = None;
        loop {
            let progress = session.coarse_progress();
            if last_progress != Some(progress) {
                control.report_progress(progress.0, progress.1, Some(4));
                last_progress = Some(progress);
            }
            match session.advance(4096, control)? {
                WasmSetupSearchAdvance::Pending => {}
                WasmSetupSearchAdvance::Completed(result) => {
                    control.report_progress("setup-finalize", 4, Some(4));
                    return Ok(result);
                }
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
        super::wasm_cpu::WasmExactSearchError::ResourceAdmission(resource_report) => {
            WasmCpuSearchError::ResourceAdmission { resource_report }
        }
        super::wasm_cpu::WasmExactSearchError::Cancelled => WasmCpuSearchError::Cancelled,
    }
}
