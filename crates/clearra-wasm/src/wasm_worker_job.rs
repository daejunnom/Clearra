// SRP rationale: this module has one change reason: lifecycle and validation of one WASM worker job.
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use clearra_app::{
    CancellationHandle, CancellationToken, ExecutionControl, ExecutionPartition, ExecutionProgress,
    ProductPageSourceOwner, ProgressSink,
};
use clearra_host_contract::{AppResponse, AppStatus, Diagnostic, DiagnosticReport};

use crate::{
    json_event_envelope::{
        serialize_governed_worker_events_for_abi_preserving_owner, serialize_worker_events,
    },
    wasm_command_runtime::{
        GovernedWasmExecutionResult, PreparedWasmAdvance, PreparedWasmExecution,
    },
    TilingSolutionPageStore, WasmCommandRuntime, WasmCommandRuntimeError, WasmSearchReport,
    WebGpuBackendReport,
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
    Progress,
    Completed,
    Cancelled,
    Failed,
}

impl WasmWorkerAdvanceStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Progress => "progress",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending | Self::Progress)
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
        response: Option<JobFinalResponse>,
    },
    Cancelled {
        job_id: WasmWorkerJobId,
        scope_released: bool,
    },
}

impl BudgetStatus {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(self.state.capacity() as u128)
    }
}

impl BackendStatus {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let bytes = (self.backend_requested.capacity() as u128)
            .checked_add(self.backend_selected.capacity() as u128)?;
        bytes.checked_add(
            self.fallback_reason
                .as_ref()
                .map_or(0, |reason| reason.capacity() as u128),
        )
    }
}

impl MemoryStatus {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(self.state.capacity() as u128)
    }
}

impl JobProgress {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.label.capacity() as u128)
            .checked_add(self.budget_status.checked_retained_capacity_bytes()?)?
            .checked_add(self.backend_status.checked_retained_capacity_bytes()?)?
            .checked_add(self.memory_status.checked_retained_capacity_bytes()?)
    }
}

impl WasmWorkerJobEvent {
    /// Returns every heap allocation retained by this event. For terminal
    /// responses the App/WebGPU/search graphs are included fieldwise.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::Started { .. } | Self::Cancelled { .. } => Some(0),
            Self::Progress { progress, .. } => progress.checked_retained_capacity_bytes(),
            Self::Diagnostic { diagnostic, .. } => diagnostic.checked_retained_capacity_bytes(),
            Self::PartialResult { label, .. } => Some(label.capacity() as u128),
            Self::FinalResponse {
                response,
                webgpu_backend,
                search_report,
                ..
            } => {
                let mut bytes = response.checked_retained_capacity_bytes()?;
                bytes = bytes.checked_add(webgpu_backend.checked_retained_capacity_bytes()?)?;
                if let Some(search_report) = search_report {
                    bytes = bytes.checked_add(search_report.checked_retained_capacity_bytes()?)?;
                }
                Some(bytes)
            }
            Self::Failed {
                diagnostics,
                response,
                ..
            } => {
                let mut bytes = diagnostics.checked_retained_capacity_bytes()?;
                if let Some(response) = response {
                    bytes = bytes.checked_add(response.checked_retained_capacity_bytes()?)?;
                }
                Some(bytes)
            }
        }
    }
}

pub(crate) fn checked_worker_events_retained_capacity_bytes(
    events: &Vec<WasmWorkerJobEvent>,
) -> Option<u128> {
    let mut bytes = (events.capacity() as u128)
        .checked_mul(core::mem::size_of::<WasmWorkerJobEvent>() as u128)?;
    for event in events {
        bytes = bytes.checked_add(event.checked_retained_capacity_bytes()?)?;
    }
    Some(bytes)
}

fn checked_worker_event_queue_retained_capacity_bytes(
    events: &VecDeque<WasmWorkerJobEvent>,
) -> Option<u128> {
    let mut bytes = (events.capacity() as u128)
        .checked_mul(core::mem::size_of::<WasmWorkerJobEvent>() as u128)?;
    for event in events {
        bytes = bytes.checked_add(event.checked_retained_capacity_bytes()?)?;
    }
    Some(bytes)
}

#[derive(Clone, Copy)]
enum GovernedEventRoute {
    PublicResult,
    WorkerStored,
}

impl GovernedEventRoute {
    fn source_carrier_inline_bytes(self) -> usize {
        let wrapper = core::mem::size_of::<GovernedWasmExecutionResult>();
        let parts = core::mem::size_of::<(
            crate::WasmExecutionResult,
            crate::WasmExecutionMemoryAuthority,
        )>();
        let result =
            core::mem::size_of::<Result<GovernedWasmExecutionResult, WasmCommandRuntimeError>>();
        let direct = wrapper.max(parts).max(result);
        match self {
            Self::PublicResult => direct,
            Self::WorkerStored => direct.max(core::mem::size_of::<PreparedWasmAdvance>()),
        }
    }

    fn returned_carrier_inline_bytes(self) -> Option<u128> {
        let wrapper = core::mem::size_of::<GovernedWasmWorkerEvents>();
        let result =
            core::mem::size_of::<Result<GovernedWasmWorkerEvents, WasmCommandRuntimeError>>();
        let parts = core::mem::size_of::<(
            Vec<WasmWorkerJobEvent>,
            Option<Arc<TilingSolutionPageStore>>,
            u128,
            u128,
        )>();
        let direct = wrapper.max(result).max(parts) as u128;
        match self {
            Self::PublicResult => Some(direct),
            Self::WorkerStored => {
                let stored = (core::mem::size_of::<
                    Option<(WasmWorkerJobId, GovernedWasmWorkerEvents)>,
                >() as u128)
                    .checked_add(worker_runtime_return_carrier_inline_bytes())?;
                Some(direct.max(stored))
            }
        }
    }
}

fn worker_runtime_return_carrier_inline_bytes() -> u128 {
    core::mem::size_of::<Result<WasmWorkerJobId, WasmCommandRuntimeError>>()
        .max(core::mem::size_of::<
            Result<WasmWorkerAdvanceStatus, WasmCommandRuntimeError>,
        >())
        .max(core::mem::size_of::<Result<(), WasmCommandRuntimeError>>())
        .max(core::mem::size_of::<Result<String, WasmCommandRuntimeError>>())
        .max(core::mem::size_of::<
            Result<GovernedWasmWorkerEvents, WasmCommandRuntimeError>,
        >())
        .max(core::mem::size_of::<
            Result<crate::GovernedWasmJson, WasmCommandRuntimeError>,
        >()) as u128
}

fn governed_result_transition_metadata_bytes(route: GovernedEventRoute) -> Option<u128> {
    let result_inline = core::mem::size_of::<crate::WasmExecutionResult>() as u128;
    (route.source_carrier_inline_bytes() as u128).checked_sub(result_inline)
}

fn prefix_source_carrier_inline_bytes() -> u128 {
    core::mem::size_of::<VecDeque<WasmWorkerJobEvent>>()
        .max(core::mem::size_of::<Option<VecDeque<WasmWorkerJobEvent>>>()) as u128
}

/// Non-cloneable terminal event batch that keeps the finite authority and an
/// optional shared page-store owner alive until the batch is consumed by the
/// finite serializer.
#[derive(Debug)]
pub struct GovernedWasmWorkerEvents {
    events: Vec<WasmWorkerJobEvent>,
    completed_tiling_solution_page_store: Option<Arc<TilingSolutionPageStore>>,
    completed_product_page_source_owner: Option<ProductPageSourceOwner>,
    memory_limit_bytes: u128,
    actual_retained_bytes: u128,
}

struct EventTransitionLedger {
    source_actual_bytes: u128,
    transition_inline_bytes: u128,
    target_heap_bytes: u128,
    memory_limit_bytes: u128,
}

impl EventTransitionLedger {
    fn new(
        source_actual_bytes: u128,
        memory_limit_bytes: u128,
    ) -> Result<Self, WasmCommandRuntimeError> {
        let transition_inline_bytes = (core::mem::size_of::<Vec<WasmWorkerJobEvent>>() as u128)
            .checked_add(core::mem::size_of::<JobProgress>() as u128)
            .ok_or_else(event_projection_error)?;
        let ledger = Self {
            source_actual_bytes,
            transition_inline_bytes,
            target_heap_bytes: 0,
            memory_limit_bytes,
        };
        ledger.authorize_requested(0)?;
        Ok(ledger)
    }

    fn authorize_requested(&self, requested_bytes: u128) -> Result<(), WasmCommandRuntimeError> {
        let required = self
            .source_actual_bytes
            .checked_add(self.transition_inline_bytes)
            .and_then(|bytes| bytes.checked_add(self.target_heap_bytes))
            .and_then(|bytes| bytes.checked_add(requested_bytes))
            .ok_or_else(event_projection_error)?;
        if required > self.memory_limit_bytes {
            return Err(event_limit_error());
        }
        Ok(())
    }

    fn retain_actual(&mut self, actual_bytes: u128) -> Result<(), WasmCommandRuntimeError> {
        self.target_heap_bytes = self
            .target_heap_bytes
            .checked_add(actual_bytes)
            .ok_or_else(event_projection_error)?;
        self.authorize_requested(0)
    }
}

fn try_event_string(
    value: &str,
    ledger: &mut EventTransitionLedger,
) -> Result<String, WasmCommandRuntimeError> {
    ledger.authorize_requested(value.len() as u128)?;
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(|_| event_allocation_error())?;
    let actual_capacity = output.capacity();
    ledger.retain_actual(actual_capacity as u128)?;
    if actual_capacity < value.len() {
        return Err(event_projection_error());
    }
    output.push_str(value);
    if output.capacity() != actual_capacity {
        return Err(event_projection_error());
    }
    Ok(output)
}

fn try_terminal_progress(
    result: &crate::WasmExecutionResult,
    ledger: &mut EventTransitionLedger,
) -> Result<JobProgress, WasmCommandRuntimeError> {
    let response = result.app_response();
    let webgpu = result.webgpu_backend();
    let app_backend = response.backend_report();
    let fallback_reason = if webgpu.fallback_used {
        webgpu.webgpu_unavailable_reason.as_deref()
    } else {
        app_backend.fallback_reason()
    };
    Ok(JobProgress {
        done: 2,
        total: 2,
        label: try_event_string("AppResponse completed", ledger)?,
        budget_status: BudgetStatus {
            state: try_event_string("within-budget", ledger)?,
            used: 0,
            limit: None,
        },
        backend_status: BackendStatus {
            backend_requested: try_event_string(app_backend.backend_requested(), ledger)?,
            backend_selected: try_event_string(app_backend.backend_selected(), ledger)?,
            fallback_used: webgpu.fallback_used || app_backend.fallback_used(),
            fallback_reason: fallback_reason
                .map(|reason| try_event_string(reason, ledger))
                .transpose()?,
        },
        memory_status: MemoryStatus {
            state: try_event_string("wasm-computation-scope-active", ledger)?,
            raw_pointer_exposed: false,
        },
    })
}

impl GovernedWasmWorkerEvents {
    pub fn try_from_final_result(
        job_id: WasmWorkerJobId,
        governed: GovernedWasmExecutionResult,
    ) -> Result<Self, WasmCommandRuntimeError> {
        Self::try_from_final_result_for_route(
            job_id,
            governed,
            VecDeque::new(),
            GovernedEventRoute::PublicResult,
        )
    }

    pub(crate) fn try_from_final_result_with_prefix(
        job_id: WasmWorkerJobId,
        governed: GovernedWasmExecutionResult,
        prefix: VecDeque<WasmWorkerJobEvent>,
    ) -> Result<Self, WasmCommandRuntimeError> {
        Self::try_from_final_result_for_route(
            job_id,
            governed,
            prefix,
            GovernedEventRoute::PublicResult,
        )
    }

    pub(crate) fn try_from_final_result_for_worker_storage(
        job_id: WasmWorkerJobId,
        governed: GovernedWasmExecutionResult,
        prefix: VecDeque<WasmWorkerJobEvent>,
    ) -> Result<Self, WasmCommandRuntimeError> {
        Self::try_from_final_result_for_route(
            job_id,
            governed,
            prefix,
            GovernedEventRoute::WorkerStored,
        )
    }

    fn try_from_final_result_for_route(
        job_id: WasmWorkerJobId,
        governed: GovernedWasmExecutionResult,
        mut prefix: VecDeque<WasmWorkerJobEvent>,
        route: GovernedEventRoute,
    ) -> Result<Self, WasmCommandRuntimeError> {
        if prefix.iter().any(|event| {
            matches!(
                event,
                WasmWorkerJobEvent::FinalResponse { .. }
                    | WasmWorkerJobEvent::Failed { .. }
                    | WasmWorkerJobEvent::Cancelled { .. }
            )
        }) {
            return Err(event_projection_error());
        }
        let (result, authority) = governed.into_parts();
        let memory_limit_bytes = authority.memory_limit_bytes();
        let old_actual = authority.actual_retained_bytes();
        let transport_heap = result
            .checked_transport_retained_capacity_bytes()
            .ok_or_else(event_projection_error)?;
        let shared_page_store_bytes = old_actual
            .checked_sub(core::mem::size_of_val(&result) as u128)
            .and_then(|bytes| bytes.checked_sub(transport_heap))
            .ok_or_else(event_projection_error)?;
        let prefix_heap = checked_worker_event_queue_retained_capacity_bytes(&prefix)
            .ok_or_else(event_projection_error)?;
        let prefix_outer = (prefix.capacity() as u128)
            .checked_mul(core::mem::size_of::<WasmWorkerJobEvent>() as u128)
            .ok_or_else(event_projection_error)?;
        let prefix_nested = prefix_heap
            .checked_sub(prefix_outer)
            .ok_or_else(event_projection_error)?;
        let source_with_prefix = old_actual
            .checked_add(
                governed_result_transition_metadata_bytes(route)
                    .ok_or_else(event_projection_error)?,
            )
            .and_then(|bytes| bytes.checked_add(prefix_source_carrier_inline_bytes()))
            .and_then(|bytes| bytes.checked_add(prefix_heap))
            .ok_or_else(event_projection_error)?;
        let mut ledger = EventTransitionLedger::new(source_with_prefix, memory_limit_bytes)?;
        let target_len = prefix
            .len()
            .checked_add(2)
            .ok_or_else(event_projection_error)?;
        let requested_outer = (target_len as u128)
            .checked_mul(core::mem::size_of::<WasmWorkerJobEvent>() as u128)
            .ok_or_else(event_projection_error)?;
        ledger.authorize_requested(requested_outer)?;
        let mut events = Vec::new();
        events
            .try_reserve_exact(target_len)
            .map_err(|_| event_allocation_error())?;
        let actual_outer = (events.capacity() as u128)
            .checked_mul(core::mem::size_of::<WasmWorkerJobEvent>() as u128)
            .ok_or_else(event_projection_error)?;
        ledger.retain_actual(actual_outer)?;
        let progress = try_terminal_progress(&result, &mut ledger)?;
        let progress_heap = progress
            .checked_retained_capacity_bytes()
            .ok_or_else(event_projection_error)?;
        if ledger.target_heap_bytes
            != actual_outer
                .checked_add(progress_heap)
                .ok_or_else(event_projection_error)?
        {
            return Err(event_projection_error());
        }
        events.extend(prefix.drain(..));
        drop(prefix);
        events.push(WasmWorkerJobEvent::Progress { job_id, progress });
        let (
            response,
            webgpu_backend,
            search_report,
            completed_tiling_solution_page_store,
            completed_product_page_source_owner,
        ) = result.into_parts();
        events.push(WasmWorkerJobEvent::FinalResponse {
            job_id,
            response,
            webgpu_backend,
            search_report,
        });
        let final_outer = (events.capacity() as u128)
            .checked_mul(core::mem::size_of::<WasmWorkerJobEvent>() as u128)
            .ok_or_else(event_projection_error)?;
        if final_outer != actual_outer || events.capacity() < events.len() {
            return Err(event_projection_error());
        }
        let event_heap = checked_worker_events_retained_capacity_bytes(&events)
            .ok_or_else(event_projection_error)?;
        let expected_event_heap = transport_heap
            .checked_add(actual_outer)
            .and_then(|bytes| bytes.checked_add(prefix_nested))
            .and_then(|bytes| bytes.checked_add(progress_heap))
            .ok_or_else(event_projection_error)?;
        if event_heap != expected_event_heap {
            return Err(event_projection_error());
        }
        let actual_retained_bytes = (core::mem::size_of::<Self>() as u128)
            .checked_add(event_heap)
            .and_then(|bytes| bytes.checked_add(shared_page_store_bytes))
            .ok_or_else(event_projection_error)?;
        if actual_retained_bytes > memory_limit_bytes {
            return Err(event_limit_error());
        }
        let final_payload_heap = actual_retained_bytes
            .checked_sub(core::mem::size_of::<Self>() as u128)
            .ok_or_else(event_projection_error)?;
        let final_peak = final_payload_heap
            .checked_add(
                route
                    .returned_carrier_inline_bytes()
                    .ok_or_else(event_projection_error)?,
            )
            .ok_or_else(event_projection_error)?;
        if final_peak > memory_limit_bytes {
            return Err(event_limit_error());
        }
        Ok(Self {
            events,
            completed_tiling_solution_page_store,
            completed_product_page_source_owner,
            memory_limit_bytes,
            actual_retained_bytes,
        })
    }

    pub fn events(&self) -> &[WasmWorkerJobEvent] {
        &self.events
    }

    pub const fn memory_limit_bytes(&self) -> u128 {
        self.memory_limit_bytes
    }

    pub const fn actual_retained_bytes(&self) -> u128 {
        self.actual_retained_bytes
    }

    pub fn completed_tiling_solution_page_store(&self) -> Option<&Arc<TilingSolutionPageStore>> {
        self.completed_tiling_solution_page_store.as_ref()
    }

    pub fn completed_product_page_source_owner(&self) -> Option<&ProductPageSourceOwner> {
        self.completed_product_page_source_owner.as_ref()
    }

    pub(crate) fn checked_event_heap_bytes(&self) -> Option<u128> {
        checked_worker_events_retained_capacity_bytes(&self.events)
    }

    pub(crate) fn checked_page_store_retained_bytes(&self) -> Option<u128> {
        let tiling_bytes = self
            .completed_tiling_solution_page_store
            .as_ref()
            .map_or(Some(0), |store| store.checked_retained_capacity_bytes())?;
        tiling_bytes.checked_add(self.completed_product_page_source_owner.as_ref().map_or(
            Some(0),
            ProductPageSourceOwner::checked_retained_capacity_bytes,
        )?)
    }

    pub(crate) fn into_serialization_parts(
        self,
    ) -> (
        Vec<WasmWorkerJobEvent>,
        Option<Arc<TilingSolutionPageStore>>,
        Option<ProductPageSourceOwner>,
        u128,
        u128,
    ) {
        (
            self.events,
            self.completed_tiling_solution_page_store,
            self.completed_product_page_source_owner,
            self.memory_limit_bytes,
            self.actual_retained_bytes,
        )
    }

    pub(crate) fn checked_worker_storage_peak_bytes(&self) -> Option<u128> {
        self.actual_retained_bytes
            .checked_sub(core::mem::size_of::<Self>() as u128)?
            .checked_add(GovernedEventRoute::WorkerStored.returned_carrier_inline_bytes()?)
    }

    #[cfg(test)]
    pub(crate) fn from_events_for_test(
        events: Vec<WasmWorkerJobEvent>,
        memory_limit_bytes: u128,
    ) -> Self {
        let event_heap = checked_worker_events_retained_capacity_bytes(&events)
            .expect("test event capacity fits u128");
        let actual_retained_bytes = (core::mem::size_of::<Self>() as u128)
            .checked_add(event_heap)
            .expect("test governed event capacity fits u128");
        Self {
            events,
            completed_tiling_solution_page_store: None,
            completed_product_page_source_owner: None,
            memory_limit_bytes,
            actual_retained_bytes,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_memory_limit_for_test(mut self, memory_limit_bytes: u128) -> Self {
        self.memory_limit_bytes = memory_limit_bytes;
        self
    }
}

fn event_limit_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_EVENT_MEMORY_LIMIT", String::new())
}

fn event_allocation_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_EVENT_ALLOCATION", String::new())
}

fn event_projection_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_EVENT_MEMORY_PROJECTION", String::new())
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
    progress: Option<Arc<WasmProgressBuffer>>,
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
            progress: Some(progress),
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

/// Finite jobs must not retain the compatibility progress buffer.  This
/// scope keeps cancellation and partition control only; its constructor does
/// not create a progress sink or a growable queue.  It is intentionally not
/// used until `PreparedWasmExecution` exposes the direct finite authority that
/// can account for its own retained owner graph.
#[derive(Debug)]
struct WasmFiniteComputationScope {
    cancellation: WasmCancellationToken,
    control: ExecutionControl,
    released: bool,
}

impl WasmFiniteComputationScope {
    fn new(partition: ExecutionPartition) -> Self {
        let cancellation = WasmCancellationToken::new();
        let control = ExecutionControl::new(cancellation.token.clone()).with_partition(partition);
        Self {
            cancellation,
            control,
            released: false,
        }
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(core::mem::size_of::<std::sync::atomic::AtomicU32>() as u128)
    }

    fn cancellation_handles_share_one_atomic(&self) -> bool {
        let token_flag = self.cancellation.token.atomic_flag();
        std::ptr::eq(token_flag, self.cancellation.handle.atomic_flag())
            && std::ptr::eq(token_flag, self.control.cancellation.atomic_flag())
    }

    fn release(&mut self) {
        self.released = true;
    }
}

impl Drop for WasmFiniteComputationScope {
    fn drop(&mut self) {
        self.release();
    }
}

/// A fixed single-owner slot for finite worker state.  Unlike the unbounded
/// compatibility maps, this slot has no bucket capacity or historical
/// capacity to account for.  Public finite activation remains gated until the
/// direct prepared-execution authority is available.
#[derive(Debug, Default)]
struct FiniteOwnerSlot<T> {
    owner: Option<T>,
}

impl<T> FiniteOwnerSlot<T> {
    fn is_occupied(&self) -> bool {
        self.owner.is_some()
    }

    fn insert(&mut self, owner: T) -> Result<(), T> {
        if self.owner.is_some() {
            return Err(owner);
        }
        self.owner = Some(owner);
        Ok(())
    }

    fn as_ref(&self) -> Option<&T> {
        self.owner.as_ref()
    }

    fn take(&mut self) -> Option<T> {
        self.owner.take()
    }
}

struct ActiveJob {
    command_text: String,
    execution: Option<PreparedWasmExecution>,
    scope: WasmComputationScope,
}

fn take_command_text_for_prepare(command_text: &mut String) -> String {
    std::mem::take(command_text)
}

fn after_dropping_prepare_owner<Owner, Output>(
    owner: Owner,
    next: impl FnOnce() -> Output,
) -> Output {
    drop(owner);
    next()
}

pub struct WasmWorkerJobRuntime {
    runtime: WasmCommandRuntime,
    next_id: u64,
    statuses: HashMap<WasmWorkerJobId, WasmWorkerJobStatus>,
    events: HashMap<WasmWorkerJobId, VecDeque<WasmWorkerJobEvent>>,
    active_jobs: HashMap<WasmWorkerJobId, ActiveJob>,
    completed_tiling_solution_page_store: Option<Arc<TilingSolutionPageStore>>,
    completed_product_page_source_owner: Option<ProductPageSourceOwner>,
    completed_governed_events: Option<(WasmWorkerJobId, GovernedWasmWorkerEvents)>,
    finite_job: Option<WasmWorkerJobId>,
}

impl WasmWorkerJobRuntime {
    pub fn new(runtime: WasmCommandRuntime) -> Self {
        Self {
            runtime,
            next_id: 1,
            statuses: HashMap::new(),
            events: HashMap::new(),
            active_jobs: HashMap::new(),
            completed_tiling_solution_page_store: None,
            completed_product_page_source_owner: None,
            completed_governed_events: None,
            finite_job: None,
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
        if raw_worker_requests_finite_memory(command_text) {
            return Err(finite_authority_unavailable_error());
        }
        if self.completed_governed_events.is_some() {
            return Err(governed_events_outstanding_error());
        }
        if self.finite_job.is_some() {
            return Err(finite_single_flight_error());
        }
        self.completed_tiling_solution_page_store = None;
        self.completed_product_page_source_owner = None;
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
        if self.completed_governed_events.is_some() {
            return Err(governed_events_outstanding_error());
        }
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
                .get_mut(&job_id)
                .map(|job| take_command_text_for_prepare(&mut job.command_text))
                .ok_or_else(|| missing_job_error(job_id))?;
            self.set_status(job_id, WasmWorkerJobStatus::Running);
            match self.runtime.prepare_command_text(command_text.as_str()) {
                Ok(prepared) => {
                    let finite_build = prepared.has_finite_build_memory_authority();
                    if finite_build && (self.finite_job.is_some() || self.active_jobs.len() != 1) {
                        drop(prepared);
                        drop(command_text);
                        self.release_scope(job_id);
                        self.set_status(job_id, WasmWorkerJobStatus::Failed);
                        return Err(finite_single_flight_error());
                    }
                    if finite_build {
                        // `PreparedWasmExecution` currently exposes no direct
                        // finite caller-memory authority. Do not construct a
                        // public finite worker owner graph through the
                        // compatibility App path; reject before the finite
                        // marker, execution, or terminal storage is created.
                        drop(prepared);
                        drop(command_text);
                        self.release_scope(job_id);
                        self.set_status(job_id, WasmWorkerJobStatus::Failed);
                        return Err(finite_authority_unavailable_error());
                    }
                    // The prepared request now owns every parsed value needed
                    // by execution. Destroy the only command buffer before
                    // constructing the cooperative execution so their heap
                    // capacities never overlap.
                    let execution = after_dropping_prepare_owner(command_text, || {
                        self.runtime.start_prepared_execution(prepared)
                    });
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
                    drop(command_text);
                    self.release_scope(job_id);
                    self.set_status(job_id, WasmWorkerJobStatus::Failed);
                    self.push_event(WasmWorkerJobEvent::Failed {
                        job_id,
                        diagnostics: error.diagnostic_report(),
                        response: None,
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
        match advance {
            PreparedWasmAdvance::Pending => {
                self.emit_buffered_progress(job_id);
                Ok(WasmWorkerAdvanceStatus::Pending)
            }
            PreparedWasmAdvance::Progress => {
                self.emit_buffered_progress(job_id);
                Ok(WasmWorkerAdvanceStatus::Progress)
            }
            PreparedWasmAdvance::Cancelled => {
                self.emit_buffered_progress(job_id);
                self.finish_cancelled(job_id)?;
                Ok(WasmWorkerAdvanceStatus::Cancelled)
            }
            PreparedWasmAdvance::Failed(error) => {
                self.release_scope(job_id);
                self.set_status(job_id, WasmWorkerJobStatus::Failed);
                Err(error)
            }
            PreparedWasmAdvance::CompletedGoverned(result) => {
                self.release_scope(job_id);
                let prefix = self.events.remove(&job_id).unwrap_or_default();
                let governed_events =
                    match GovernedWasmWorkerEvents::try_from_final_result_for_worker_storage(
                        job_id, result, prefix,
                    ) {
                        Ok(events) => events,
                        Err(error) => {
                            self.set_status(job_id, WasmWorkerJobStatus::Failed);
                            return Err(error);
                        }
                    };
                self.finite_job = Some(job_id);
                if let Err(error) = self.store_completed_governed_events(job_id, governed_events) {
                    self.finite_job = None;
                    self.set_status(job_id, WasmWorkerJobStatus::Failed);
                    return Err(error);
                }
                self.set_status(job_id, WasmWorkerJobStatus::Completed);
                Ok(WasmWorkerAdvanceStatus::Completed)
            }
            PreparedWasmAdvance::Completed(result) => {
                self.emit_buffered_progress(job_id);
                if result.app_response().status() != AppStatus::Success {
                    let response = result.app_response().clone();
                    let diagnostics = DiagnosticReport::new(response.diagnostics().to_vec());
                    self.release_scope(job_id);
                    self.set_status(job_id, WasmWorkerJobStatus::Failed);
                    self.push_event(WasmWorkerJobEvent::Failed {
                        job_id,
                        diagnostics,
                        response: Some(response),
                    });
                    return Ok(WasmWorkerAdvanceStatus::Failed);
                }
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
                let (
                    response,
                    webgpu_backend,
                    search_report,
                    tiling_solution_page_store,
                    product_page_source_owner,
                ) = result.into_parts();
                self.completed_tiling_solution_page_store = tiling_solution_page_store;
                self.completed_product_page_source_owner = product_page_source_owner;
                self.push_event(WasmWorkerJobEvent::FinalResponse {
                    job_id,
                    response,
                    webgpu_backend,
                    search_report,
                });
                Ok(WasmWorkerAdvanceStatus::Completed)
            }
        }
    }

    pub fn cancel_job(&mut self, job_id: WasmWorkerJobId) -> Result<(), WasmCommandRuntimeError> {
        if self.completed_governed_events.is_some() {
            return Err(governed_events_outstanding_error());
        }
        if let Some(job) = self.active_jobs.get_mut(&job_id) {
            job.scope.cancellation.cancel();
        } else {
            return Err(missing_job_error(job_id));
        }
        self.release_scope(job_id);
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
        if self.completed_governed_events.is_some() {
            return Err(governed_events_outstanding_error());
        }
        if !self.statuses.contains_key(&job_id) {
            return Err(missing_job_error(job_id));
        }
        serialize_worker_events(&self.drain_events(job_id))
    }

    pub fn drain_governed_events(
        &mut self,
        job_id: WasmWorkerJobId,
    ) -> Result<GovernedWasmWorkerEvents, WasmCommandRuntimeError> {
        match self.completed_governed_events.take() {
            Some((completed_job_id, events)) if completed_job_id == job_id => {
                self.finite_job = None;
                Ok(events)
            }
            Some(other) => {
                self.completed_governed_events = Some(other);
                Err(governed_events_job_mismatch_error())
            }
            None => Err(WasmCommandRuntimeError::new(
                "E_WASM_GOVERNED_EVENTS_UNAVAILABLE",
                String::new(),
            )),
        }
    }

    pub fn drain_governed_events_json(
        &mut self,
        job_id: WasmWorkerJobId,
    ) -> Result<crate::GovernedWasmJson, WasmCommandRuntimeError> {
        match self.completed_governed_events.as_ref() {
            Some((completed_job_id, _)) if *completed_job_id == job_id => {}
            Some(_) => return Err(governed_events_job_mismatch_error()),
            None => {
                return Err(WasmCommandRuntimeError::new(
                    "E_WASM_GOVERNED_EVENTS_UNAVAILABLE",
                    String::new(),
                ));
            }
        }
        let (completed_job_id, governed) = self
            .completed_governed_events
            .take()
            .expect("the preflighted governed batch remains stored");
        match serialize_governed_worker_events_for_abi_preserving_owner(governed) {
            Ok(output) => {
                self.finite_job = None;
                Ok(output)
            }
            Err((error, governed)) => {
                debug_assert!(self.completed_governed_events.is_none());
                self.completed_governed_events = Some((completed_job_id, governed));
                Err(error)
            }
        }
    }

    pub fn status(&self, job_id: WasmWorkerJobId) -> Option<WasmWorkerJobStatus> {
        self.statuses.get(&job_id).copied()
    }

    pub fn has_completed_governed_events(&self) -> bool {
        self.completed_governed_events.is_some()
    }

    /// Reports a finite job that is still queued/running, excluding the
    /// completed governed batch lease that must remain drainable.
    pub fn has_active_finite_job(&self) -> bool {
        self.finite_job.is_some() && self.completed_governed_events.is_none()
    }

    pub fn take_completed_tiling_solution_page_store(
        &mut self,
    ) -> Option<Arc<TilingSolutionPageStore>> {
        self.completed_tiling_solution_page_store.take()
    }

    pub fn take_completed_product_page_source_owner(&mut self) -> Option<ProductPageSourceOwner> {
        self.completed_product_page_source_owner.take()
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
        if self.finite_job == Some(job_id) {
            self.finite_job = None;
        }
    }

    fn store_completed_governed_events(
        &mut self,
        job_id: WasmWorkerJobId,
        events: GovernedWasmWorkerEvents,
    ) -> Result<(), WasmCommandRuntimeError> {
        if self.completed_governed_events.is_some() {
            return Err(governed_events_outstanding_error());
        }
        if self.finite_job != Some(job_id) {
            return Err(event_projection_error());
        }
        let storage_peak = events
            .checked_worker_storage_peak_bytes()
            .ok_or_else(event_projection_error)?;
        if storage_peak > events.memory_limit_bytes() {
            return Err(event_limit_error());
        }
        self.completed_governed_events = Some((job_id, events));
        Ok(())
    }

    fn emit_buffered_progress(&mut self, job_id: WasmWorkerJobId) {
        let progress = self
            .active_jobs
            .get(&job_id)
            .and_then(|job| job.scope.progress.as_ref())
            .map(|progress| progress.drain())
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

fn raw_worker_requests_finite_memory(command_text: &str) -> bool {
    command_text
        .split_whitespace()
        .any(|token| token == "--max-memory-mib")
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

fn governed_events_outstanding_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_GOVERNED_EVENTS_OUTSTANDING", String::new())
}

fn governed_events_job_mismatch_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_GOVERNED_EVENTS_JOB_MISMATCH", String::new())
}

fn finite_single_flight_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_FINITE_SINGLE_FLIGHT", String::new())
}

fn finite_authority_unavailable_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_FINITE_AUTHORITY_UNAVAILABLE", String::new())
}

#[cfg(test)]
mod ownership_tests {
    use std::{cell::Cell, rc::Rc};

    use super::{
        after_dropping_prepare_owner, finite_authority_unavailable_error,
        raw_worker_requests_finite_memory, take_command_text_for_prepare, FiniteOwnerSlot,
        GovernedWasmWorkerEvents, WasmCommandRuntime, WasmFiniteComputationScope,
        WasmWorkerJobEvent, WasmWorkerJobId, WasmWorkerJobRuntime,
    };

    fn governed_events_fixture(job_id: WasmWorkerJobId) -> GovernedWasmWorkerEvents {
        GovernedWasmWorkerEvents::from_events_for_test(
            vec![WasmWorkerJobEvent::Started { job_id }],
            u128::MAX,
        )
    }

    #[test]
    fn prepare_takes_the_original_command_buffer_without_cloning() {
        let mut command = String::with_capacity(257);
        command.push_str("clearra build-probability");
        let pointer = command.as_ptr();
        let capacity = command.capacity();

        let moved = take_command_text_for_prepare(&mut command);

        assert_eq!(moved.as_ptr(), pointer);
        assert_eq!(moved.capacity(), capacity);
        assert_eq!(command.capacity(), 0);
        assert!(command.is_empty());
    }

    #[test]
    fn command_owner_drops_before_prepared_execution_is_constructed() {
        struct DropProbe(Rc<Cell<bool>>);

        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let dropped = Rc::new(Cell::new(false));
        after_dropping_prepare_owner(DropProbe(Rc::clone(&dropped)), || {
            assert!(dropped.get());
        });
        assert!(dropped.get());
    }

    #[test]
    fn finite_scope_has_no_progress_owner_or_growable_progress_queue() {
        let scope = WasmFiniteComputationScope::new(clearra_app::ExecutionPartition::whole());
        assert!(scope.control.progress_sink.is_none());
        assert_eq!(
            scope.checked_retained_bytes(),
            Some(
                core::mem::size_of::<WasmFiniteComputationScope>() as u128
                    + core::mem::size_of::<std::sync::atomic::AtomicU32>() as u128
            )
        );
        assert!(scope.cancellation_handles_share_one_atomic());
        assert!(!scope.cancellation.is_cancelled());
    }

    #[test]
    fn finite_owner_slot_rejects_duplicate_without_replacing_or_allocating() {
        let mut slot = FiniteOwnerSlot::default();
        let mut first = String::with_capacity(257);
        first.push_str("finite-owner");
        let first_pointer = first.as_ptr();
        let first_capacity = first.capacity();
        slot.insert(first)
            .expect("first finite owner slot insertion");

        let mut duplicate = String::with_capacity(263);
        duplicate.push_str("duplicate-owner");
        let duplicate_pointer = duplicate.as_ptr();
        let duplicate_capacity = duplicate.capacity();
        let duplicate = slot
            .insert(duplicate)
            .expect_err("duplicate finite owner must be rejected");
        assert_eq!(duplicate.as_ptr(), duplicate_pointer);
        assert_eq!(duplicate.capacity(), duplicate_capacity);

        let retained = slot.as_ref().expect("first finite owner remains stored");
        assert_eq!(retained.as_ptr(), first_pointer);
        assert_eq!(retained.capacity(), first_capacity);
        assert!(slot.is_occupied());
    }

    #[test]
    fn finite_authority_rejection_is_static_and_allocation_free() {
        let error = finite_authority_unavailable_error();
        assert_eq!(error.code(), "E_WASM_FINITE_AUTHORITY_UNAVAILABLE");
        assert!(error.message().is_empty());
        assert_eq!(error.message_capacity_for_test(), 0);
    }

    #[test]
    fn raw_finite_memory_precheck_is_exact_and_leaves_runtime_unchanged_on_replay() {
        assert!(raw_worker_requests_finite_memory(
            "clearra build-probability --max-memory-mib 64"
        ));
        assert!(raw_worker_requests_finite_memory(
            "clearra\u{2003}build-probability\u{2003}--max-memory-mib\u{2003}64"
        ));
        assert!(!raw_worker_requests_finite_memory(
            "clearra build-probability --max-memory-mib=64"
        ));
        assert!(!raw_worker_requests_finite_memory(
            "clearra build-probability --max-memory-mib-extra 64"
        ));

        let mut runtime = WasmWorkerJobRuntime::new(WasmCommandRuntime::default());
        let compatibility_job = runtime
            .start_job("$verify")
            .expect("compatibility job establishes nonempty worker state");
        let snapshot = |runtime: &WasmWorkerJobRuntime| {
            (
                (
                    runtime.next_id,
                    runtime.statuses.len(),
                    runtime.statuses.capacity(),
                    runtime.events.len(),
                    runtime.events.capacity(),
                    runtime.active_jobs.len(),
                    runtime.active_jobs.capacity(),
                ),
                (
                    runtime.completed_tiling_solution_page_store.is_some(),
                    runtime.completed_governed_events.is_some(),
                    runtime.finite_job,
                    runtime.status(compatibility_job),
                    runtime.active_jobs.get(&compatibility_job).map(|job| {
                        (
                            job.command_text.as_ptr() as usize,
                            job.command_text.len(),
                            job.command_text.capacity(),
                            job.execution.is_some(),
                            job.scope.released,
                        )
                    }),
                    runtime
                        .events
                        .get(&compatibility_job)
                        .map(|events| (events.len(), events.capacity())),
                ),
            )
        };
        let before = snapshot(&runtime);

        for command in [
            "clearra build-probability --max-memory-mib 64",
            "clearra\u{2003}build-probability\u{2003}--max-memory-mib\u{2003}64",
            "clearra\u{2003}build-probability\u{2003}--max-memory-mib\u{2003}64",
        ] {
            let error = runtime
                .start_job(command)
                .expect_err("raw finite worker entry remains inactive");
            assert_eq!(error.code(), "E_WASM_FINITE_AUTHORITY_UNAVAILABLE");
            assert!(error.message().is_empty());
            assert_eq!(error.message_capacity_for_test(), 0);
            assert_eq!(snapshot(&runtime), before);
        }
    }

    #[test]
    fn completed_batch_a_cannot_be_overwritten_by_batch_b() {
        let mut runtime = WasmWorkerJobRuntime::new(WasmCommandRuntime::default());
        let job_a = WasmWorkerJobId::new(41);
        let job_b = WasmWorkerJobId::new(42);
        let batch_a = governed_events_fixture(job_a);
        let batch_a_pointer = batch_a.events().as_ptr();
        runtime.finite_job = Some(job_a);
        runtime
            .store_completed_governed_events(job_a, batch_a)
            .expect("batch A storage");

        let error = runtime
            .store_completed_governed_events(job_b, governed_events_fixture(job_b))
            .expect_err("batch B cannot replace undrained batch A");

        assert_eq!(error.code(), "E_WASM_GOVERNED_EVENTS_OUTSTANDING");
        assert_eq!(error.message_capacity_for_test(), 0);
        let (stored_job_id, stored_batch) = runtime
            .completed_governed_events
            .as_ref()
            .expect("batch A remains stored");
        assert_eq!(*stored_job_id, job_a);
        assert_eq!(stored_batch.events().as_ptr(), batch_a_pointer);
    }

    #[test]
    fn wrong_id_drain_is_allocation_free_and_preserves_the_batch() {
        let mut runtime = WasmWorkerJobRuntime::new(WasmCommandRuntime::default());
        let job_a = WasmWorkerJobId::new(51);
        let batch_a = governed_events_fixture(job_a);
        let batch_a_pointer = batch_a.events().as_ptr();
        runtime.finite_job = Some(job_a);
        runtime
            .store_completed_governed_events(job_a, batch_a)
            .expect("batch A storage");

        let error = runtime
            .drain_governed_events(WasmWorkerJobId::new(52))
            .expect_err("wrong id fails closed");

        assert_eq!(error.code(), "E_WASM_GOVERNED_EVENTS_JOB_MISMATCH");
        assert!(error.message().is_empty());
        assert_eq!(error.message_capacity_for_test(), 0);
        let (stored_job_id, stored_batch) = runtime
            .completed_governed_events
            .as_ref()
            .expect("batch A remains stored");
        assert_eq!(*stored_job_id, job_a);
        assert_eq!(stored_batch.events().as_ptr(), batch_a_pointer);
        assert!(runtime.has_completed_governed_events());
    }

    #[test]
    fn active_finite_query_excludes_the_completed_batch_lease() {
        let mut runtime = WasmWorkerJobRuntime::new(WasmCommandRuntime::default());
        let job_id = WasmWorkerJobId::new(53);

        runtime.finite_job = Some(job_id);
        assert!(runtime.has_active_finite_job());
        runtime
            .store_completed_governed_events(job_id, governed_events_fixture(job_id))
            .expect("completed batch storage");

        assert!(!runtime.has_active_finite_job());
        assert!(runtime.has_completed_governed_events());
    }

    #[test]
    fn governed_json_failure_restores_the_exact_completed_batch_owner() {
        let mut runtime = WasmWorkerJobRuntime::new(WasmCommandRuntime::default());
        let job_id = WasmWorkerJobId::new(54);
        let batch = governed_events_fixture(job_id);
        let events_pointer = batch.events().as_ptr();
        runtime.finite_job = Some(job_id);
        runtime
            .store_completed_governed_events(job_id, batch)
            .expect("completed batch storage");
        runtime
            .completed_governed_events
            .as_mut()
            .expect("stored batch")
            .1
            .memory_limit_bytes = 0;

        let error = runtime
            .drain_governed_events_json(job_id)
            .expect_err("JSON admission failure preserves the source batch");

        assert_eq!(error.code(), "E_WASM_JSON_MEMORY_LIMIT");
        assert_eq!(error.message_capacity_for_test(), 0);
        let (stored_job_id, stored_batch) = runtime
            .completed_governed_events
            .as_ref()
            .expect("the failed transaction restores the batch");
        assert_eq!(*stored_job_id, job_id);
        assert_eq!(stored_batch.events().as_ptr(), events_pointer);
        assert_eq!(runtime.finite_job, Some(job_id));
        assert!(!runtime.has_active_finite_job());
    }

    #[test]
    fn worker_storage_carrier_accepts_exact_peak_and_rejects_peak_minus_one() {
        let job_id = WasmWorkerJobId::new(61);
        let measured = governed_events_fixture(job_id);
        let exact_peak = measured
            .checked_worker_storage_peak_bytes()
            .expect("worker storage peak fits");
        drop(measured);

        let mut exact_runtime = WasmWorkerJobRuntime::new(WasmCommandRuntime::default());
        exact_runtime.finite_job = Some(job_id);
        exact_runtime
            .store_completed_governed_events(
                job_id,
                governed_events_fixture(job_id).with_memory_limit_for_test(exact_peak),
            )
            .expect("exact worker storage carrier is admitted");

        let mut below_runtime = WasmWorkerJobRuntime::new(WasmCommandRuntime::default());
        below_runtime.finite_job = Some(job_id);
        let error = below_runtime
            .store_completed_governed_events(
                job_id,
                governed_events_fixture(job_id).with_memory_limit_for_test(exact_peak - 1),
            )
            .expect_err("worker storage carrier peak minus one is rejected");
        assert_eq!(error.code(), "E_WASM_EVENT_MEMORY_LIMIT");
        assert!(!below_runtime.has_completed_governed_events());
    }

    #[test]
    fn finite_single_flight_rejects_a_second_job_without_mutating_the_owner() {
        let mut runtime = WasmWorkerJobRuntime::new(WasmCommandRuntime::default());
        let finite_job = WasmWorkerJobId::new(71);
        runtime.finite_job = Some(finite_job);

        let error = runtime
            .start_job("$verify")
            .expect_err("a live finite job owns the worker runtime");

        assert_eq!(error.code(), "E_WASM_FINITE_SINGLE_FLIGHT");
        assert_eq!(runtime.finite_job, Some(finite_job));
        assert!(runtime.active_jobs.is_empty());
    }

    #[test]
    fn json_drain_rejects_an_unknown_job_instead_of_serializing_an_empty_batch() {
        let mut runtime = WasmWorkerJobRuntime::new(WasmCommandRuntime::default());
        let missing_job = WasmWorkerJobId::new(79);

        let error = runtime
            .drain_events_json(missing_job)
            .expect_err("an unknown job has no event authority");

        assert_eq!(error.code(), "E_WASM_WORKER_JOB_MISSING");
        assert!(runtime.events.is_empty());
        assert!(runtime.statuses.is_empty());
    }
}
