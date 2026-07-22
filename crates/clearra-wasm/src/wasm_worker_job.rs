use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use clearra_app::{
    CancellationHandle, CancellationToken, ExecutionControl, ExecutionPartition, ExecutionProgress,
    ProgressSink,
};
use clearra_host_contract::{AppResponse, Diagnostic, DiagnosticReport};

use crate::{
    json_event_envelope::serialize_worker_events,
    wasm_command_runtime::{PreparedWasmAdvance, PreparedWasmExecution},
    WasmCommandRuntime, WasmCommandRuntimeError, WasmSearchReport, WebGpuBackendReport,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WasmWorkerJobId(u64);

impl WasmWorkerJobId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WasmWorkerJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WasmWorkerAdvanceStatus {
    Pending,
    Completed,
    Cancelled,
    Failed,
}

impl WasmWorkerAdvanceStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }
}

pub type JobId = WasmWorkerJobId;
pub type JobStatus = WasmWorkerJobStatus;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetStatus {
    pub state: String,
    pub used: u64,
    pub limit: Option<u64>,
}

impl BudgetStatus {
    pub fn within_budget() -> Self {
        Self {
            state: "within-budget".to_owned(),
            used: 0,
            limit: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendStatus {
    pub backend_requested: String,
    pub backend_selected: String,
    pub fallback_used: bool,
    pub fallback_reason: Option<String>,
}

impl BackendStatus {
    pub fn pending() -> Self {
        Self {
            backend_requested: "auto".to_owned(),
            backend_selected: "pending".to_owned(),
            fallback_used: false,
            fallback_reason: None,
        }
    }

    pub fn from_execution(response: &AppResponse, webgpu: &WebGpuBackendReport) -> Self {
        let app_backend = response.backend_report();
        let fallback_reason = if webgpu.fallback_used {
            webgpu.webgpu_unavailable_reason.clone()
        } else {
            app_backend.fallback_reason().map(ToOwned::to_owned)
        };
        Self {
            backend_requested: app_backend.backend_requested().to_owned(),
            backend_selected: app_backend.backend_selected().to_owned(),
            fallback_used: webgpu.fallback_used || app_backend.fallback_used(),
            fallback_reason,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryStatus {
    pub state: String,
    pub raw_pointer_exposed: bool,
}

impl MemoryStatus {
    pub fn active() -> Self {
        Self {
            state: "wasm-computation-scope-active".to_owned(),
            raw_pointer_exposed: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobProgress {
    pub done: u32,
    pub total: u32,
    pub label: String,
    pub budget_status: BudgetStatus,
    pub backend_status: BackendStatus,
    pub memory_status: MemoryStatus,
}

impl JobProgress {
    pub fn new(done: u32, total: u32, label: impl Into<String>) -> Self {
        Self {
            done,
            total,
            label: label.into(),
            budget_status: BudgetStatus::within_budget(),
            backend_status: BackendStatus::pending(),
            memory_status: MemoryStatus::active(),
        }
    }

    pub fn with_backend_status(mut self, backend_status: BackendStatus) -> Self {
        self.backend_status = backend_status;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDiagnosticEvent {
    pub job_id: JobId,
    pub diagnostic: Diagnostic,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobPartialResult {
    pub job_id: JobId,
    pub partial: bool,
    pub label: String,
    pub final_result: bool,
}

pub type JobFinalResponse = AppResponse;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancelRequest {
    pub job_id: JobId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WasmWorkerJobEvent {
    Started {
        job_id: WasmWorkerJobId,
    },
    Progress {
        job_id: WasmWorkerJobId,
        progress: JobProgress,
    },
    Diagnostic {
        job_id: WasmWorkerJobId,
        diagnostic: Diagnostic,
    },
    PartialResult {
        job_id: WasmWorkerJobId,
        partial: bool,
        label: String,
        final_result: bool,
    },
    FinalResponse {
        job_id: WasmWorkerJobId,
        response: JobFinalResponse,
        webgpu_backend: WebGpuBackendReport,
        search_report: Option<WasmSearchReport>,
    },
    Failed {
        job_id: WasmWorkerJobId,
        diagnostics: DiagnosticReport,
    },
    Cancelled {
        job_id: WasmWorkerJobId,
        scope_released: bool,
    },
}

#[derive(Clone, Debug)]
pub struct WasmCancellationToken {
    token: CancellationToken,
    handle: CancellationHandle,
}

impl WasmCancellationToken {
    fn new() -> Self {
        let token = CancellationToken::new();
        Self {
            handle: token.handle(),
            token,
        }
    }

    pub fn cancel(&self) {
        self.handle.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

#[derive(Debug, Default)]
struct WasmProgressBuffer {
    pending: Mutex<Vec<ExecutionProgress>>,
}

impl WasmProgressBuffer {
    fn drain(&self) -> Vec<ExecutionProgress> {
        std::mem::take(
            &mut *self
                .pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }
}

impl ProgressSink for WasmProgressBuffer {
    fn report(&self, progress: ExecutionProgress) {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(progress);
    }
}

#[derive(Debug)]
struct WasmComputationScope {
    cancellation: WasmCancellationToken,
    progress: Arc<WasmProgressBuffer>,
    control: ExecutionControl,
    released: bool,
}

impl WasmComputationScope {
    fn new(partition: ExecutionPartition) -> Self {
        let cancellation = WasmCancellationToken::new();
        let progress = Arc::new(WasmProgressBuffer::default());
        let control = ExecutionControl::new(cancellation.token.clone())
            .with_progress_sink(progress.clone())
            .with_partition(partition);
        Self {
            cancellation,
            progress,
            control,
            released: false,
        }
    }

    fn release(&mut self) {
        self.released = true;
    }

    fn execution_control(&self) -> &ExecutionControl {
        &self.control
    }
}

impl Drop for WasmComputationScope {
    fn drop(&mut self) {
        self.release();
    }
}

struct ActiveJob {
    command_text: String,
    execution: Option<PreparedWasmExecution>,
    scope: WasmComputationScope,
}

pub struct WasmWorkerJobRuntime {
    runtime: WasmCommandRuntime,
    next_id: u64,
    statuses: HashMap<WasmWorkerJobId, WasmWorkerJobStatus>,
    events: HashMap<WasmWorkerJobId, VecDeque<WasmWorkerJobEvent>>,
    active_jobs: HashMap<WasmWorkerJobId, ActiveJob>,
}

impl WasmWorkerJobRuntime {
    pub fn new(runtime: WasmCommandRuntime) -> Self {
        Self {
            runtime,
            next_id: 1,
            statuses: HashMap::new(),
            events: HashMap::new(),
            active_jobs: HashMap::new(),
        }
    }

    pub fn set_host_capabilities(&mut self, capabilities: crate::WasmHostCapabilities) {
        self.runtime.set_host_capabilities(capabilities);
    }

    pub fn command_runtime(&self) -> &WasmCommandRuntime {
        &self.runtime
    }

    pub fn start_job(
        &mut self,
        command_text: &str,
    ) -> Result<WasmWorkerJobId, WasmCommandRuntimeError> {
        self.start_whole_job(command_text)
    }

    fn start_whole_job(
        &mut self,
        command_text: &str,
    ) -> Result<WasmWorkerJobId, WasmCommandRuntimeError> {
        let job_id = self.allocate_job()?;
        self.active_jobs.insert(
            job_id,
            ActiveJob {
                command_text: command_text.to_owned(),
                execution: None,
                scope: WasmComputationScope::new(ExecutionPartition::whole()),
            },
        );
        self.set_status(job_id, WasmWorkerJobStatus::Queued);
        self.push_event(WasmWorkerJobEvent::Started { job_id });
        Ok(job_id)
    }

    pub fn advance_job(
        &mut self,
        job_id: WasmWorkerJobId,
        work_budget: u32,
    ) -> Result<WasmWorkerAdvanceStatus, WasmCommandRuntimeError> {
        let cancelled = self
            .active_jobs
            .get(&job_id)
            .map(|job| job.scope.cancellation.is_cancelled())
            .ok_or_else(|| missing_job_error(job_id))?;
        if cancelled {
            self.finish_cancelled(job_id)?;
            return Ok(WasmWorkerAdvanceStatus::Cancelled);
        }

        let needs_prepare = self
            .active_jobs
            .get(&job_id)
            .is_some_and(|job| job.execution.is_none());
        if needs_prepare {
            let command_text = self
                .active_jobs
                .get(&job_id)
                .map(|job| job.command_text.clone())
                .ok_or_else(|| missing_job_error(job_id))?;
            self.set_status(job_id, WasmWorkerJobStatus::Running);
            match self.runtime.prepare_command_text(command_text.as_str()) {
                Ok(prepared) => {
                    let execution = self.runtime.start_prepared_execution(prepared);
                    if let Some(job) = self.active_jobs.get_mut(&job_id) {
                        job.execution = Some(execution);
                    }
                    self.push_event(WasmWorkerJobEvent::Progress {
                        job_id,
                        progress: JobProgress::new(1, 2, "AppRequest parsed and validated"),
                    });
                    return Ok(WasmWorkerAdvanceStatus::Pending);
                }
                Err(error) => {
                    self.release_scope(job_id);
                    self.set_status(job_id, WasmWorkerJobStatus::Failed);
                    self.push_event(WasmWorkerJobEvent::Failed {
                        job_id,
                        diagnostics: error.diagnostic_report(),
                    });
                    return Ok(WasmWorkerAdvanceStatus::Failed);
                }
            }
        }

        if self
            .active_jobs
            .get(&job_id)
            .is_some_and(|job| job.scope.cancellation.is_cancelled())
        {
            self.finish_cancelled(job_id)?;
            return Ok(WasmWorkerAdvanceStatus::Cancelled);
        }
        let advance = {
            let job = self
                .active_jobs
                .get_mut(&job_id)
                .ok_or_else(|| missing_job_error(job_id))?;
            let control = job.scope.execution_control();
            job.execution
                .as_mut()
                .ok_or_else(|| missing_job_error(job_id))?
                .advance(work_budget.max(1) as usize, control)
        };
        self.emit_buffered_progress(job_id);
        match advance {
            PreparedWasmAdvance::Pending => Ok(WasmWorkerAdvanceStatus::Pending),
            PreparedWasmAdvance::Cancelled => {
                self.finish_cancelled(job_id)?;
                Ok(WasmWorkerAdvanceStatus::Cancelled)
            }
            PreparedWasmAdvance::Completed(result) => {
                self.release_scope(job_id);
                self.push_event(WasmWorkerJobEvent::Progress {
                    job_id,
                    progress: JobProgress::new(2, 2, "AppResponse completed").with_backend_status(
                        BackendStatus::from_execution(
                            result.app_response(),
                            result.webgpu_backend(),
                        ),
                    ),
                });
                self.set_status(job_id, WasmWorkerJobStatus::Completed);
                self.push_event(WasmWorkerJobEvent::FinalResponse {
                    job_id,
                    response: result.app_response().clone(),
                    webgpu_backend: result.webgpu_backend().clone(),
                    search_report: result.search_report().cloned(),
                });
                Ok(WasmWorkerAdvanceStatus::Completed)
            }
        }
    }

    pub fn cancel_job(&mut self, job_id: WasmWorkerJobId) -> Result<(), WasmCommandRuntimeError> {
        if let Some(job) = self.active_jobs.get_mut(&job_id) {
            job.scope.cancellation.cancel();
            job.scope.release();
            self.active_jobs.remove(&job_id);
        } else {
            return Err(missing_job_error(job_id));
        }
        self.set_status(job_id, WasmWorkerJobStatus::Cancelled);
        self.push_event(WasmWorkerJobEvent::Cancelled {
            job_id,
            scope_released: true,
        });
        Ok(())
    }

    pub fn cancellation_token(&self, job_id: WasmWorkerJobId) -> Option<WasmCancellationToken> {
        self.active_jobs
            .get(&job_id)
            .map(|job| job.scope.cancellation.clone())
    }

    pub fn drain_events(&mut self, job_id: WasmWorkerJobId) -> Vec<WasmWorkerJobEvent> {
        self.events
            .remove(&job_id)
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    pub fn drain_events_json(
        &mut self,
        job_id: WasmWorkerJobId,
    ) -> Result<String, WasmCommandRuntimeError> {
        serialize_worker_events(&self.drain_events(job_id))
    }

    pub fn status(&self, job_id: WasmWorkerJobId) -> Option<WasmWorkerJobStatus> {
        self.statuses.get(&job_id).copied()
    }

    fn finish_cancelled(&mut self, job_id: WasmWorkerJobId) -> Result<(), WasmCommandRuntimeError> {
        self.release_scope(job_id);
        self.set_status(job_id, WasmWorkerJobStatus::Cancelled);
        self.push_event(WasmWorkerJobEvent::Cancelled {
            job_id,
            scope_released: true,
        });
        Ok(())
    }

    fn release_scope(&mut self, job_id: WasmWorkerJobId) {
        if let Some(mut job) = self.active_jobs.remove(&job_id) {
            job.scope.release();
        }
    }

    fn emit_buffered_progress(&mut self, job_id: WasmWorkerJobId) {
        let progress = self
            .active_jobs
            .get(&job_id)
            .map(|job| job.scope.progress.drain())
            .unwrap_or_default();
        for item in progress {
            let total = item.total.unwrap_or(item.completed.max(1));
            self.push_event(WasmWorkerJobEvent::Progress {
                job_id,
                progress: JobProgress::new(
                    u32::try_from(item.completed).unwrap_or(u32::MAX),
                    u32::try_from(total).unwrap_or(u32::MAX),
                    item.stage,
                ),
            });
        }
    }

    fn allocate_job(&mut self) -> Result<WasmWorkerJobId, WasmCommandRuntimeError> {
        let job_id = WasmWorkerJobId(self.next_id);
        self.next_id = self.next_id.checked_add(1).ok_or_else(|| {
            WasmCommandRuntimeError::new("E_WASM_JOB_ID_EXHAUSTED", "job id space exhausted")
        })?;
        self.events.insert(job_id, VecDeque::new());
        Ok(job_id)
    }

    fn set_status(&mut self, job_id: WasmWorkerJobId, status: WasmWorkerJobStatus) {
        self.statuses.insert(job_id, status);
    }

    fn push_event(&mut self, event: WasmWorkerJobEvent) {
        let job_id = match &event {
            WasmWorkerJobEvent::Started { job_id }
            | WasmWorkerJobEvent::Progress { job_id, .. }
            | WasmWorkerJobEvent::FinalResponse { job_id, .. }
            | WasmWorkerJobEvent::Failed { job_id, .. }
            | WasmWorkerJobEvent::Cancelled { job_id, .. } => *job_id,
            WasmWorkerJobEvent::Diagnostic { job_id, .. }
            | WasmWorkerJobEvent::PartialResult { job_id, .. } => *job_id,
        };
        self.events.entry(job_id).or_default().push_back(event);
    }
}

impl Default for WasmWorkerJobRuntime {
    fn default() -> Self {
        Self::new(WasmCommandRuntime::default())
    }
}

fn missing_job_error(job_id: WasmWorkerJobId) -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new(
        "E_WASM_WORKER_JOB_MISSING",
        format!("unknown or inactive Web Worker job id {}", job_id.get()),
    )
}
