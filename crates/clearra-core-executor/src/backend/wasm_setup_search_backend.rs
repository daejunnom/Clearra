use std::sync::Arc;

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_problem::SetupSearchQuery;

use crate::CoreExecutionResult;

use super::{
    wasm_cpu::{WasmSetupSearchAdvance as InnerAdvance, WasmSetupSearchSession as InnerSession},
    WasmCpuSearchError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
// Completed setup results move directly across the public backend boundary.
#[allow(clippy::large_enum_variant)]
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

    pub fn new_with_complete_precomputed_pc_candidates(
        query: &SetupSearchQuery,
        candidates: Arc<[StandardBoard64TilingIdentity]>,
        workers: usize,
    ) -> Result<Self, WasmCpuSearchError> {
        Ok(Self {
            inner: InnerSession::new_with_complete_precomputed_pc_candidates(
                query, candidates, workers,
            )
            .map_err(map_error)?,
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
        execute_setup_session(&mut session, control)
    }

    /// Evaluates one already sealed, complete PC candidate universe through
    /// the ordinary Setup graph, BuildUp, shape, score and coverage owners.
    /// Core validates the candidate geometry but does not mint provider or
    /// SetupSearch completeness authority; callers must pass the App admission
    /// boundary before selecting this path.
    pub fn execute_with_complete_precomputed_pc_candidates_and_control(
        query: &SetupSearchQuery,
        candidates: Arc<[StandardBoard64TilingIdentity]>,
        workers: usize,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, WasmCpuSearchError> {
        let mut session = WasmSetupSearchSession::new_with_complete_precomputed_pc_candidates(
            query, candidates, workers,
        )?;
        execute_setup_session(&mut session, control)
    }
}

fn execute_setup_session(
    session: &mut WasmSetupSearchSession,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, WasmCpuSearchError> {
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

fn map_error(error: super::wasm_cpu::WasmExactSearchError) -> WasmCpuSearchError {
    match error {
        super::wasm_cpu::WasmExactSearchError::InvalidProblem(reason) => {
            WasmCpuSearchError::InvalidProblem { reason }
        }
        super::wasm_cpu::WasmExactSearchError::ResourceAdmission(resource_report) => {
            WasmCpuSearchError::resource_admission(*resource_report)
        }
        super::wasm_cpu::WasmExactSearchError::Cancelled => WasmCpuSearchError::Cancelled,
    }
}
