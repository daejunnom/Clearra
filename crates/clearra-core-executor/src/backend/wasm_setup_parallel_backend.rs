use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SetupSearchQuery;

use crate::CoreExecutionResult;

use super::{
    wasm_cpu::{
        WasmSetupParallelCoordinator as InnerCoordinator, WasmSetupParallelProduce as InnerProduce,
        WasmSetupParallelWorker as InnerWorker,
    },
    WasmCpuSearchError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WasmSetupParallelProduce {
    Pending,
    Initialization(Vec<u8>),
    Batch(Vec<u8>),
    Completed,
    Cancelled,
}

pub struct WasmSetupParallelCoordinator {
    inner: InnerCoordinator,
}

impl WasmSetupParallelCoordinator {
    pub fn new(query: &SetupSearchQuery, worker_count: usize) -> Result<Self, WasmCpuSearchError> {
        Ok(Self {
            inner: InnerCoordinator::new(query, worker_count).map_err(map_error)?,
        })
    }

    pub fn condition_count(&self) -> usize {
        self.inner.condition_count()
    }

    pub fn task_count(&self) -> usize {
        self.inner.task_count()
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        batch_capacity: usize,
        control: &ExecutionControl,
    ) -> Result<WasmSetupParallelProduce, WasmCpuSearchError> {
        Ok(
            match self
                .inner
                .advance(work_budget, batch_capacity, control)
                .map_err(map_error)?
            {
                InnerProduce::Pending => WasmSetupParallelProduce::Pending,
                InnerProduce::Initialization(bytes) => {
                    WasmSetupParallelProduce::Initialization(bytes)
                }
                InnerProduce::Batch(bytes) => WasmSetupParallelProduce::Batch(bytes),
                InnerProduce::Completed => WasmSetupParallelProduce::Completed,
                InnerProduce::Cancelled => WasmSetupParallelProduce::Cancelled,
            },
        )
    }

    pub fn absorb(&mut self, input: &[u8]) -> Result<(), WasmCpuSearchError> {
        self.inner.absorb(input).map_err(map_error)
    }

    pub fn finish(self, workers_used: usize) -> Result<CoreExecutionResult, WasmCpuSearchError> {
        self.inner.finish(workers_used).map_err(map_error)
    }

    pub fn producer_completed(&self) -> bool {
        self.inner.producer_completed()
    }

    pub fn dispatched_conditions(&self) -> usize {
        self.inner.dispatched_conditions()
    }

    pub fn received_conditions(&self) -> usize {
        self.inner.received_conditions()
    }
}

pub struct WasmSetupParallelWorker {
    inner: InnerWorker,
}

impl WasmSetupParallelWorker {
    pub fn accepts_initialization(input: &[u8]) -> bool {
        InnerWorker::accepts_initialization(input)
    }

    pub fn new(input: &[u8]) -> Result<Self, WasmCpuSearchError> {
        Ok(Self {
            inner: InnerWorker::new(input).map_err(map_error)?,
        })
    }

    pub fn consume(
        &mut self,
        input: &[u8],
        control: &ExecutionControl,
    ) -> Result<(usize, Vec<u8>), WasmCpuSearchError> {
        self.inner.consume(input, control).map_err(map_error)
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
