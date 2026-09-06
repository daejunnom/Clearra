//! SRP rationale: this module has one change reason: the stable WASM export contract and its
//! single owned ABI state boundary.

use std::{
    cell::RefCell,
    fmt,
    sync::{Arc, Once},
};

use clearra_pc_graph::request::GpuDeviceSelection;
#[cfg(feature = "stage-profiling")]
use clearra_wasm::ExecutorSearchProfileSession;
#[cfg(target_arch = "wasm32")]
use clearra_wasm::prewarm_gpu_search_async;
use clearra_wasm::{
    GovernedWasmJson, GpuSearchWarmupReport, PortfolioPageLoadState, ProductPageSourceOwner,
    ProductPageStore, TilingSolutionPageStore, WasmCommandRuntimeError,
    WasmDistributedCompletionAdvance, WasmDistributedCompletionSession, WasmDistributedCoordinator,
    WasmDistributedFallbackReason, WasmDistributedMode, WasmDistributedPreparation,
    WasmDistributedProducerAdvance, WasmDistributedRequestedBackend,
    WasmDistributedVerifierRuntime, WasmHostCapabilities, WasmMinimumParallelWorker,
    WasmWorkerAdvanceStatus, WasmWorkerJobId, WasmWorkerJobRuntime, install_pc4_compact_tablebase,
    release_pc4_compact_tablebase, serialize_coverage_portfolio_advance_state,
    serialize_coverage_portfolio_load_advance_state, serialize_coverage_portfolio_retained_page,
    serialize_distributed_final_events, serialize_parity_report_exhausted,
    serialize_parity_report_page, serialize_pc_replay_page_advance,
};
#[cfg(test)]
use clearra_wasm::{PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT, WasmWorkerJobStatus};

const ABI_VERSION: u32 = 1;
const MAX_COMMAND_BYTES: usize = 1024 * 1024;
const PRODUCT_PAGE_REQUEST_CONTRACT: &str = "portfolio-page-request.v1";
const PRODUCT_PAGE_REQUEST_CONTRACT_V2: &str = "portfolio-page-request.v2";
const DEFAULT_PRODUCT_PAGE_WORK_STEPS: u64 = 10_000;
// Small continuation/error JSON, control token and scalar serialization
// carriers. The actual public 100-member payload keeps App's separate 16x
// whole-live projection reserve; this is not a per-page or per-worker cap.
const PC_REPLAY_HOST_ENVELOPE_RESERVE: usize = 4096;
const MAX_TRANSFER_BYTES: usize = 512 * 1024 * 1024;
const ABI_OK: i32 = 0;
const ABI_ERROR: i32 = -1;
const ABI_OUTPUT_NOT_RELEASED: i32 = -2;
// The public distributed-mode values 0..=2 are owned by `WasmDistributedMode`.
// Preparation can also complete at the App boundary before a coordinator exists;
// this ABI-only mode tells hosts to consume that terminal owner via
// `clearra_wasm_distributed_finish` instead of replaying the command serially.
const ABI_DISTRIBUTED_READY: i32 = 3;
const HOST_CAPABILITY_WEBGPU: u32 = 1 << 0;
const HOST_CAPABILITY_CROSS_ORIGIN_ISOLATED: u32 = 1 << 1;

#[derive(Default)]
struct WasmAbiState {
    runtime: WasmWorkerJobRuntime,
    input: Vec<u8>,
    transfer_input: Vec<u8>,
    output: Vec<u8>,
    governed_output: Option<GovernedWasmJson>,
    output_outstanding: bool,
    distributed_coordinator: Option<WasmDistributedCoordinator>,
    distributed_ready_result: Option<clearra_wasm::WasmExecutionResult>,
    distributed_completion: Option<(u32, WasmDistributedCompletionSession)>,
    minimum_parallel_worker: Option<WasmMinimumParallelWorker>,
    distributed_verifier: Option<WasmDistributedVerifierRuntime>,
    distributed_verifier_partial_available: bool,
    distributed_verifier_pending_work: bool,
    distributed_verifier_last_candidate_count: Option<AbiI32Count>,
    tiling_solution_page_store: Option<AbiTilingSolutionPageStore>,
    product_page_source_owner: Option<ProductPageSourceOwner>,
    product_page_store: Option<AbiProductPageStore>,
    gpu_warmup: Option<GpuWarmupState>,
    gpu_warmup_generation: u64,
    #[cfg(feature = "stage-profiling")]
    profile: Option<ExecutorSearchProfileSession>,
}

#[derive(Clone, Copy)]
struct GovernedOutputAdmission;

enum GpuWarmupState {
    Pending,
    Ready(GpuSearchWarmupReport),
}

enum AbiTilingSolutionPageStore {
    Legacy(Arc<TilingSolutionPageStore>),
    Governed {
        store: Arc<TilingSolutionPageStore>,
        memory_limit_bytes: u128,
        producer_graph_bytes: u128,
    },
}

/// A product page store that preserves the finite worker's memory authority
/// after the final JSON lease is released. Legacy/distributed producers do not
/// carry a finite memory authority and therefore retain the existing path.
enum AbiProductPageStore {
    Legacy(ProductPageStore),
    Governed {
        store: ProductPageStore,
        memory_limit_bytes: u128,
    },
}

impl AbiProductPageStore {
    fn legacy(store: ProductPageStore) -> Self {
        Self::Legacy(store)
    }

    fn try_governed(
        source: ProductPageSourceOwner,
        memory_limit_bytes: u128,
        expected_source_bytes: u128,
        transition_inline_bytes: u128,
    ) -> Result<Self, ()> {
        let Some(measured_source_bytes) = source.checked_retained_capacity_bytes() else {
            return Err(());
        };
        if measured_source_bytes != expected_source_bytes {
            return Err(());
        }
        let storage_inline_bytes = core::mem::size_of::<Option<AbiProductPageStore>>() as u128;
        let Some(source_transition_peak) =
            measured_source_bytes.checked_add(transition_inline_bytes.max(storage_inline_bytes))
        else {
            return Err(());
        };
        if source_transition_peak > memory_limit_bytes {
            return Err(());
        }

        // ProductPageStore construction moves, rather than clones, the source
        // owner. The completed store is measured before it becomes reachable
        // from ABI state; an allocator that retains more than the admitted
        // limit is dropped fail-closed.
        let store = match ProductPageStore::from_source(source) {
            Ok(store) => store,
            Err(_) => return Err(()),
        };
        let Some(store_bytes) = store.checked_retained_capacity_bytes() else {
            return Err(());
        };
        let Some(transition_peak) =
            store_bytes.checked_add(transition_inline_bytes.max(storage_inline_bytes))
        else {
            return Err(());
        };
        if transition_peak > memory_limit_bytes {
            return Err(());
        }
        Ok(Self::Governed {
            store,
            memory_limit_bytes,
        })
    }

    fn store(&self) -> &ProductPageStore {
        match self {
            Self::Legacy(store) | Self::Governed { store, .. } => store,
        }
    }

    fn store_mut(&mut self) -> &mut ProductPageStore {
        match self {
            Self::Legacy(store) | Self::Governed { store, .. } => store,
        }
    }

    fn is_governed(&self) -> bool {
        matches!(self, Self::Governed { .. })
    }

    fn governed_store_fits(&self) -> bool {
        match self {
            Self::Legacy(_) => true,
            Self::Governed {
                store,
                memory_limit_bytes,
            } => store
                .checked_retained_capacity_bytes()
                .and_then(|bytes| {
                    bytes.checked_add(core::mem::size_of::<Option<AbiProductPageStore>>() as u128)
                })
                .is_some_and(|actual| actual <= *memory_limit_bytes),
        }
    }

    fn governed_output_fits(&self, output_capacity: usize) -> bool {
        match self {
            Self::Legacy(_) => true,
            Self::Governed {
                store,
                memory_limit_bytes,
            } => store
                .checked_retained_capacity_bytes()
                .and_then(|bytes| {
                    bytes.checked_add(core::mem::size_of::<Option<AbiProductPageStore>>() as u128)
                })
                .and_then(|bytes| bytes.checked_add(core::mem::size_of::<String>() as u128))
                .and_then(|bytes| bytes.checked_add(output_capacity as u128))
                .is_some_and(|actual| actual <= *memory_limit_bytes),
        }
    }

    fn governed_request_fits(&self, request_capacity: usize) -> bool {
        match self {
            Self::Legacy(_) => true,
            Self::Governed {
                store,
                memory_limit_bytes,
            } => store
                .checked_retained_capacity_bytes()
                .and_then(|bytes| {
                    bytes.checked_add(core::mem::size_of::<Option<AbiProductPageStore>>() as u128)
                })
                .and_then(|bytes| bytes.checked_add(request_capacity as u128))
                .is_some_and(|actual| actual <= *memory_limit_bytes),
        }
    }

    /// Admits a whole-live App page-store peak under the finite worker's
    /// original authority. The App callback already includes every persistent
    /// and transient owner below `ProductPageStore`; this boundary adds only
    /// the ABI carrier and request allocation that remain live above it.
    fn governed_app_peak_fits(
        memory_limit_bytes: u128,
        app_whole_live_bytes: u128,
        additional_live_capacity: usize,
    ) -> bool {
        app_whole_live_bytes
            .checked_add(core::mem::size_of::<Option<AbiProductPageStore>>() as u128)
            .and_then(|bytes| bytes.checked_add(additional_live_capacity as u128))
            .is_some_and(|actual| actual <= memory_limit_bytes)
    }

    #[cfg(test)]
    fn governed_authority(&self) -> Option<(u128, u128)> {
        match self {
            Self::Legacy(_) => None,
            Self::Governed {
                store,
                memory_limit_bytes,
            } => store
                .checked_retained_capacity_bytes()
                .and_then(|bytes| {
                    bytes.checked_add(core::mem::size_of::<Option<AbiProductPageStore>>() as u128)
                })
                .map(|actual| (*memory_limit_bytes, actual)),
        }
    }
}

impl AbiTilingSolutionPageStore {
    fn legacy(store: Arc<TilingSolutionPageStore>) -> Self {
        Self::Legacy(store)
    }

    fn try_governed(
        store: Arc<TilingSolutionPageStore>,
        memory_limit_bytes: u128,
        expected_graph_bytes: u128,
        transition_inline_bytes: u128,
    ) -> Result<Self, Arc<TilingSolutionPageStore>> {
        let Some(workspace_bytes) =
            TilingSolutionPageStore::checked_retained_capacity_projection_workspace_inline_bytes()
        else {
            return Err(store);
        };
        let storage_inline_bytes =
            core::mem::size_of::<Option<AbiTilingSolutionPageStore>>() as u128;
        let Some(actual_retained_bytes) = expected_graph_bytes.checked_add(storage_inline_bytes)
        else {
            return Err(store);
        };
        let Some(measurement_actual_bytes) = expected_graph_bytes
            .checked_add(transition_inline_bytes.max(storage_inline_bytes))
            .and_then(|actual| actual.checked_add(workspace_bytes))
        else {
            return Err(store);
        };
        if actual_retained_bytes > memory_limit_bytes
            || measurement_actual_bytes > memory_limit_bytes
        {
            return Err(store);
        }
        let Some(measured_graph_bytes) = store.checked_retained_capacity_bytes() else {
            return Err(store);
        };
        if measured_graph_bytes != expected_graph_bytes {
            return Err(store);
        }
        Ok(Self::Governed {
            store,
            memory_limit_bytes,
            producer_graph_bytes: expected_graph_bytes,
        })
    }

    fn store(&self) -> &TilingSolutionPageStore {
        match self {
            Self::Legacy(store) | Self::Governed { store, .. } => store,
        }
    }

    fn is_governed(&self) -> bool {
        match self {
            Self::Legacy(_) => false,
            Self::Governed {
                memory_limit_bytes,
                producer_graph_bytes,
                ..
            } => {
                debug_assert!(
                    producer_graph_bytes
                        .checked_add(
                            core::mem::size_of::<Option<AbiTilingSolutionPageStore>>() as u128
                        )
                        .is_some_and(|actual| actual <= *memory_limit_bytes)
                );
                true
            }
        }
    }

    fn governed_output_fits(&self, output_capacity: usize) -> bool {
        match self {
            Self::Legacy(_) => true,
            Self::Governed {
                memory_limit_bytes,
                producer_graph_bytes,
                ..
            } => producer_graph_bytes
                .checked_add(core::mem::size_of::<Option<AbiTilingSolutionPageStore>>() as u128)
                .and_then(|bytes| bytes.checked_add(core::mem::size_of::<String>() as u128))
                .and_then(|bytes| bytes.checked_add(output_capacity as u128))
                .is_some_and(|actual| actual <= *memory_limit_bytes),
        }
    }

    #[cfg(test)]
    fn governed_authority(&self) -> Option<(u128, u128, u128)> {
        match self {
            Self::Legacy(_) => None,
            Self::Governed {
                memory_limit_bytes,
                producer_graph_bytes,
                ..
            } => producer_graph_bytes
                .checked_add(core::mem::size_of::<Option<AbiTilingSolutionPageStore>>() as u128)
                .map(|actual| (*memory_limit_bytes, *producer_graph_bytes, actual)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AbiU32Count {
    legacy_value: u32,
    exact: bool,
}

impl AbiU32Count {
    fn project(value: usize) -> Self {
        match u32::try_from(value) {
            Ok(value) => Self {
                legacy_value: value,
                exact: true,
            },
            Err(_) => Self {
                legacy_value: u32::MAX,
                exact: false,
            },
        }
    }

    const fn legacy_or(projected: Option<Self>, unavailable_value: u32) -> u32 {
        match projected {
            Some(projected) => projected.legacy_value,
            None => unavailable_value,
        }
    }

    const fn exact_or_false(projected: Option<Self>) -> u32 {
        match projected {
            Some(projected) => projected.exact as u32,
            None => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AbiI32Count {
    legacy_value: i32,
    exact: bool,
}

impl AbiI32Count {
    fn project(value: usize) -> Self {
        match i32::try_from(value) {
            Ok(value) => Self {
                legacy_value: value,
                exact: true,
            },
            Err(_) => Self {
                legacy_value: i32::MAX,
                exact: false,
            },
        }
    }
}

impl WasmAbiState {
    fn require_released_output(&self) -> Result<(), i32> {
        if self.output_outstanding {
            Err(ABI_OUTPUT_NOT_RELEASED)
        } else {
            Ok(())
        }
    }

    fn require_released_output_and_page_store(&self) -> Result<(), i32> {
        self.require_released_output()?;
        if self
            .tiling_solution_page_store
            .as_ref()
            .is_some_and(AbiTilingSolutionPageStore::is_governed)
            || self
                .product_page_store
                .as_ref()
                .is_some_and(AbiProductPageStore::is_governed)
        {
            Err(ABI_ERROR)
        } else {
            Ok(())
        }
    }

    fn require_mutation_admission(&self) -> Result<(), i32> {
        self.require_released_output_and_page_store()?;
        if self.runtime.has_active_finite_job() || self.runtime.has_completed_governed_events() {
            Err(ABI_ERROR)
        } else {
            Ok(())
        }
    }

    fn require_worker_lifecycle_admission(&self) -> Result<(), i32> {
        self.require_released_output_and_page_store()?;
        if self.runtime.has_completed_governed_events() {
            Err(ABI_ERROR)
        } else {
            Ok(())
        }
    }

    fn has_external_worker_owner(&self) -> bool {
        let profile_active = {
            #[cfg(feature = "stage-profiling")]
            {
                self.profile.is_some()
            }
            #[cfg(not(feature = "stage-profiling"))]
            {
                false
            }
        };
        self.distributed_coordinator.is_some()
            || self.distributed_ready_result.is_some()
            || self.distributed_completion.is_some()
            || self.minimum_parallel_worker.is_some()
            || self.distributed_verifier.is_some()
            || self.gpu_warmup.is_some()
            || profile_active
    }

    fn has_worker_job_start_conflict(&self) -> bool {
        self.transfer_input.capacity() != 0 || self.has_external_worker_owner()
    }

    fn has_worker_advance_conflict(&self) -> bool {
        self.input.capacity() != 0 || self.has_worker_job_start_conflict()
    }

    fn require_governed_output_admission(&self) -> Result<GovernedOutputAdmission, i32> {
        self.require_released_output_and_page_store()?;
        if !self.output.is_empty()
            || self.output.capacity() != 0
            || self.governed_output.is_some()
            || !self.input.is_empty()
            || self.input.capacity() != 0
            || !self.transfer_input.is_empty()
            || self.transfer_input.capacity() != 0
            || self.tiling_solution_page_store.is_some()
            || self.product_page_source_owner.is_some()
            || self.product_page_store.is_some()
            || self.has_external_worker_owner()
        {
            Err(ABI_ERROR)
        } else {
            Ok(GovernedOutputAdmission)
        }
    }

    fn output_retained_capacity_bytes(&self) -> u128 {
        if let Some(output) = &self.governed_output {
            return Self::governed_output_storage_actual_bytes(output).unwrap_or(u128::MAX);
        }
        (self.output.capacity() as u128) * (core::mem::size_of::<u8>() as u128)
    }

    fn governed_output_storage_actual_bytes(output: &GovernedWasmJson) -> Option<u128> {
        output
            .actual_retained_bytes()
            .checked_sub(core::mem::size_of::<GovernedWasmJson>() as u128)?
            .checked_add(core::mem::size_of::<Option<GovernedWasmJson>>() as u128)
    }

    fn governed_output_storage_fits(output: &GovernedWasmJson, limit_bytes: u128) -> bool {
        Self::governed_output_storage_actual_bytes(output)
            .is_some_and(|actual_bytes| actual_bytes <= limit_bytes)
    }

    fn output_bytes(&self) -> &[u8] {
        self.governed_output
            .as_ref()
            .map(|output| output.json().as_bytes())
            .unwrap_or(self.output.as_slice())
    }

    fn release_output(&mut self) -> (u128, bool) {
        let released_bytes = self.output_retained_capacity_bytes();
        // `clear()` would preserve the allocation and keep the previous wire
        // payload live across the next producer/verifier/merger operation.
        self.output = Vec::new();
        let governed_output = self.governed_output.take();
        self.output_outstanding = false;
        if let Some(governed_output) = governed_output {
            if governed_output
                .completed_tiling_solution_page_store()
                .is_none()
                && governed_output
                    .completed_product_page_source_owner()
                    .is_none()
            {
                drop(governed_output);
                return (released_bytes, false);
            }
            let memory_limit_bytes = governed_output.memory_limit_bytes();
            let governed_actual_bytes = governed_output.actual_retained_bytes();
            let transition_inline_bytes = core::mem::size_of::<(
                String,
                Option<Arc<TilingSolutionPageStore>>,
                Option<ProductPageSourceOwner>,
                u128,
                u128,
            )>() as u128;
            let transition_actual_bytes = governed_output
                .actual_retained_bytes()
                .checked_sub(core::mem::size_of::<GovernedWasmJson>() as u128)
                .and_then(|heap_bytes| heap_bytes.checked_add(transition_inline_bytes));
            if transition_actual_bytes.is_some_and(|actual| actual <= memory_limit_bytes) {
                let (
                    json,
                    page_store,
                    product_page_source_owner,
                    memory_limit_bytes,
                    actual_retained_bytes,
                ) = governed_output.into_parts();
                let expected_shared_owner_bytes = actual_retained_bytes
                    .checked_sub(core::mem::size_of::<GovernedWasmJson>() as u128)
                    .and_then(|actual| actual.checked_sub(json.capacity() as u128));
                drop(json);
                if actual_retained_bytes == governed_actual_bytes {
                    match (page_store, product_page_source_owner) {
                        (Some(page_store), None) => {
                            if let Some(expected_graph_bytes) = expected_shared_owner_bytes {
                                if let Ok(page_store) = AbiTilingSolutionPageStore::try_governed(
                                    page_store,
                                    memory_limit_bytes,
                                    expected_graph_bytes,
                                    transition_inline_bytes,
                                ) {
                                    self.tiling_solution_page_store = Some(page_store);
                                    return (released_bytes, false);
                                }
                            }
                        }
                        (None, Some(product_page_source_owner)) => {
                            if let Some(expected_source_bytes) = expected_shared_owner_bytes {
                                if self.install_governed_product_page_source(
                                    product_page_source_owner,
                                    memory_limit_bytes,
                                    expected_source_bytes,
                                    transition_inline_bytes,
                                ) {
                                    return (released_bytes, false);
                                }
                            }
                        }
                        // One result cannot transfer two independent mutable
                        // page families through a single finite authority.
                        // Reject any future producer that attempts it until a
                        // combined accounting contract exists.
                        (Some(_), Some(_)) | (None, None) => {}
                    }
                }
            } else {
                drop(governed_output);
            }
            return (released_bytes, true);
        }
        (released_bytes, false)
    }

    fn begin_gpu_warmup(&mut self) -> Option<u64> {
        if self.gpu_warmup.is_some() {
            return None;
        }
        self.gpu_warmup_generation = self.gpu_warmup_generation.wrapping_add(1);
        self.gpu_warmup = Some(GpuWarmupState::Pending);
        Some(self.gpu_warmup_generation)
    }

    fn cancel_gpu_warmup(&mut self) {
        self.gpu_warmup_generation = self.gpu_warmup_generation.wrapping_add(1);
        self.gpu_warmup = None;
    }

    fn accepts_gpu_warmup_completion(&self, generation: u64) -> bool {
        self.require_mutation_admission().is_ok()
            && self.gpu_warmup_generation == generation
            && matches!(self.gpu_warmup, Some(GpuWarmupState::Pending))
    }

    fn complete_gpu_warmup(&mut self, generation: u64, report: GpuSearchWarmupReport) -> bool {
        if !self.accepts_gpu_warmup_completion(generation) {
            drop(report);
            return false;
        }
        self.gpu_warmup = Some(GpuWarmupState::Ready(report));
        true
    }

    fn set_error(&mut self, code: &str, message: impl std::fmt::Display) {
        self.governed_output = None;
        self.output = format!("{code}: {message}").into_bytes();
        self.output_outstanding = true;
    }

    fn set_output(&mut self, output: String) {
        self.governed_output = None;
        self.output = output.into_bytes();
        self.output_outstanding = true;
    }

    fn set_runtime_error(&mut self, error: &WasmCommandRuntimeError) {
        self.set_output(error.structured_output());
    }

    fn set_output_bytes(&mut self, output: Vec<u8>) {
        self.governed_output = None;
        self.output = output;
        self.output_outstanding = true;
    }

    fn store_governed_output(
        &mut self,
        _admission: GovernedOutputAdmission,
        output: GovernedWasmJson,
    ) {
        debug_assert!(self.require_governed_output_admission().is_ok());
        debug_assert!(Self::governed_output_storage_fits(
            &output,
            output.memory_limit_bytes()
        ));
        self.governed_output = Some(output);
        self.output_outstanding = true;
    }

    fn install_governed_product_page_source(
        &mut self,
        source: ProductPageSourceOwner,
        memory_limit_bytes: u128,
        expected_source_bytes: u128,
        transition_inline_bytes: u128,
    ) -> bool {
        match AbiProductPageStore::try_governed(
            source,
            memory_limit_bytes,
            expected_source_bytes,
            transition_inline_bytes,
        ) {
            Ok(store) => {
                self.product_page_store = Some(store);
                true
            }
            Err(()) => false,
        }
    }

    fn reset_distributed_state(&mut self) {
        self.distributed_coordinator = None;
        self.distributed_ready_result = None;
        self.distributed_completion = None;
        self.minimum_parallel_worker = None;
        self.distributed_verifier = None;
        self.distributed_verifier_partial_available = false;
        self.distributed_verifier_pending_work = false;
        self.distributed_verifier_last_candidate_count = None;

        // `clear()` keeps allocations alive. A completed distributed lifecycle
        // must release command and transfer storage back to the WASM allocator
        // so one large request/shard does not become a permanent baseline.
        self.input = Vec::new();
        self.transfer_input = Vec::new();
    }

    fn require_drained_verifier_replacement(&self) -> Result<(), i32> {
        self.require_mutation_admission()?;
        // A Geometry verifier must publish finish before replacement, even
        // when its current candidate cursor is idle: it may retain unapplied
        // results. An exact worker may retain only its already-drained query.
        if self.distributed_coordinator.is_some()
            || self.distributed_ready_result.is_some()
            || self.distributed_completion.is_some()
            || self.distributed_verifier.is_some()
            || self.distributed_verifier_pending_work
            || self
                .minimum_parallel_worker
                .as_ref()
                .is_some_and(WasmMinimumParallelWorker::has_active_shard)
        {
            Err(ABI_ERROR)
        } else {
            Ok(())
        }
    }

    fn record_distributed_verifier_candidate_count(&mut self, value: usize) -> i32 {
        let projected = AbiI32Count::project(value);
        self.distributed_verifier_last_candidate_count = Some(projected);
        projected.legacy_value
    }
}

thread_local! {
    static ABI_STATE: RefCell<WasmAbiState> = RefCell::new(WasmAbiState::default());
    static LAST_PANIC: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn distributed_count(
    read: impl FnOnce(&WasmDistributedCoordinator) -> usize,
) -> Option<AbiU32Count> {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .map(|coordinator| AbiU32Count::project(read(coordinator)))
    })
}

fn verifier_progress_count(
    read: impl FnOnce(&WasmDistributedVerifierRuntime) -> usize,
) -> Option<AbiU32Count> {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_verifier
            .as_ref()
            .map(|verifier| AbiU32Count::project(read(verifier)))
    })
}

fn tiling_solution_count() -> Option<AbiU32Count> {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .tiling_solution_page_store
            .as_ref()
            .map(|store| AbiU32Count::project(store.store().len()))
    })
}

static INSTALL_PANIC_HOOK: Once = Once::new();

fn install_panic_diagnostics() {
    INSTALL_PANIC_HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("Rust panic without a string payload");
            let message = info.location().map_or_else(
                || payload.to_owned(),
                |location| {
                    format!(
                        "{payload} at {}:{}:{}",
                        location.file(),
                        location.line(),
                        location.column()
                    )
                },
            );
            #[cfg(test)]
            eprintln!("clearra-wasm-abi panic: {message}");
            LAST_PANIC.with(|last| *last.borrow_mut() = message.into_bytes());
        }));
    });
}

fn clear_panic_diagnostics() {
    install_panic_diagnostics();
    LAST_PANIC.with(|last| last.borrow_mut().clear());
}

#[no_mangle]
pub extern "C" fn clearra_wasm_abi_version() -> u32 {
    ABI_VERSION
}

#[no_mangle]
pub extern "C" fn clearra_wasm_configure_host(
    logical_processor_count: u32,
    capability_flags: u32,
) -> i32 {
    if let Err(status) = ABI_STATE.with(|state| state.borrow().require_mutation_admission()) {
        return status;
    }
    install_panic_diagnostics();
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        debug_assert!(state.require_mutation_admission().is_ok());
        state
            .runtime
            .set_host_capabilities(WasmHostCapabilities::new(
                logical_processor_count as usize,
                capability_flags & HOST_CAPABILITY_WEBGPU != 0,
                capability_flags & HOST_CAPABILITY_CROSS_ORIGIN_ISOLATED != 0,
            ));
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_configure_product_retention(maximum_bytes: u32) -> i32 {
    if let Err(status) = ABI_STATE.with(|state| state.borrow().require_mutation_admission()) {
        return status;
    }
    let Some(budget) = clearra_wasm::ProductRetentionBudget::new(u64::from(maximum_bytes)) else {
        return ABI_ERROR;
    };
    ABI_STATE.with(|state| {
        state
            .borrow_mut()
            .runtime
            .set_product_retention_budget(Some(budget));
    });
    ABI_OK
}

#[no_mangle]
pub extern "C" fn clearra_wasm_gpu_warmup_start(device_index: i32) -> i32 {
    let warmup = ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.require_mutation_admission()?;
        if state.gpu_warmup.is_some() {
            return Ok(None);
        }
        let device = if device_index < 0 {
            GpuDeviceSelection::Auto
        } else {
            let Ok(index) = u8::try_from(device_index) else {
                state.set_error("E_WASM_GPU_DEVICE", "GPU device index exceeds u8 range");
                return Ok(None);
            };
            GpuDeviceSelection::Index(index)
        };
        Ok(state
            .begin_gpu_warmup()
            .map(|generation| (device, generation)))
    });
    let warmup = match warmup {
        Ok(warmup) => warmup,
        Err(status) => return status,
    };
    if let Some((device, generation)) = warmup {
        start_gpu_warmup(device, generation);
        return ABI_OK;
    }
    ABI_STATE.with(|state| {
        if state.borrow().gpu_warmup.is_some() {
            ABI_OK
        } else {
            ABI_ERROR
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_gpu_warmup_cancel() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        state.cancel_gpu_warmup();
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_gpu_warmup_advance() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let Some(warmup) = state.gpu_warmup.take() else {
            state.set_error("E_WASM_GPU_WARMUP_STATE", "GPU warmup is not active");
            return ABI_ERROR;
        };
        match warmup {
            GpuWarmupState::Pending => {
                state.gpu_warmup = Some(GpuWarmupState::Pending);
                0
            }
            GpuWarmupState::Ready(report) => {
                let status = if report.connected() { 1 } else { 2 };
                state.set_output(format!(
                    "connected={} device_index={} session_reused={} unavailable_reason={}",
                    report.connected(),
                    report
                        .device_index()
                        .map_or_else(|| "none".to_owned(), |index| index.to_string()),
                    report.session_reused(),
                    report.unavailable_reason().unwrap_or("none")
                ));
                status
            }
        }
    })
}

#[cfg(target_arch = "wasm32")]
fn start_gpu_warmup(device: GpuDeviceSelection, generation: u64) {
    wasm_bindgen_futures::spawn_local(async move {
        let report = prewarm_gpu_search_async(device).await;
        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.complete_gpu_warmup(generation, report);
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn start_gpu_warmup(device: GpuDeviceSelection, generation: u64) {
    let report = clearra_wasm::prewarm_gpu_search(device);
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.complete_gpu_warmup(generation, report);
    });
}

#[no_mangle]
pub extern "C" fn clearra_wasm_input_resize(byte_len: u32) -> i32 {
    let byte_len = byte_len as usize;
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if byte_len > MAX_COMMAND_BYTES {
            state.set_error(
                "E_WASM_COMMAND_TOO_LARGE",
                format_args!("command exceeds {MAX_COMMAND_BYTES} bytes"),
            );
            return ABI_ERROR;
        }
        state.input.resize(byte_len, 0);
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_input_ptr() -> u32 {
    ABI_STATE.with(|state| state.borrow().input.as_ptr() as usize as u32)
}

/// Allocates the exact-decimal product-page request buffer without opening the
/// general command mutation gate while a result-bound page owner is live.
#[no_mangle]
pub extern "C" fn clearra_wasm_product_page_request_resize(byte_len: u32) -> i32 {
    let byte_len = byte_len as usize;
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_released_output() {
            return status;
        }
        if state.runtime.has_active_finite_job()
            || state.runtime.has_completed_governed_events()
            || (state.product_page_source_owner.is_none() && state.product_page_store.is_none())
            || byte_len == 0
            || byte_len > MAX_COMMAND_BYTES
        {
            return ABI_ERROR;
        }
        let mut request = Vec::new();
        if request.try_reserve_exact(byte_len).is_err() {
            return ABI_ERROR;
        }
        request.resize(byte_len, 0);
        if state
            .product_page_store
            .as_ref()
            .is_some_and(|store| !store.governed_request_fits(request.capacity()))
        {
            return ABI_ERROR;
        }
        state.input = request;
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_transfer_resize(byte_len: u32) -> i32 {
    let byte_len = byte_len as usize;
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if byte_len > MAX_TRANSFER_BYTES {
            state.set_error(
                "E_WASM_TRANSFER_TOO_LARGE",
                format_args!("distributed transfer exceeds {MAX_TRANSFER_BYTES} bytes"),
            );
            return ABI_ERROR;
        }
        let minimum_active = state.minimum_parallel_worker.is_some()
            || state
                .distributed_completion
                .as_ref()
                .is_some_and(|(_, completion)| completion.has_minimum_control_memory());
        if minimum_active {
            let outer = minimum_coordinator_outer_bytes(&state).unwrap_or(u128::MAX);
            let grows = byte_len > state.transfer_input.capacity();
            let prospective = outer
                .checked_add(if grows { byte_len as u128 } else { 0 })
                .unwrap_or(u128::MAX);
            if let Err(error) = ensure_minimum_transfer_outer(&mut state, prospective) {
                state.set_runtime_error(&error);
                return ABI_ERROR;
            }
            if grows {
                let mut input = Vec::new();
                if input.try_reserve_exact(byte_len).is_err() {
                    state.set_error(
                        "E_WASM_MINIMUM_PARALLEL_MEMORY",
                        "minimum transfer allocation failed",
                    );
                    return ABI_ERROR;
                }
                let actual = outer
                    .checked_add(input.capacity() as u128)
                    .unwrap_or(u128::MAX);
                if let Err(error) = ensure_minimum_transfer_outer(&mut state, actual) {
                    state.set_runtime_error(&error);
                    return ABI_ERROR;
                }
                input.resize(byte_len, 0);
                state.transfer_input = input;
                return ABI_OK;
            }
        }
        state.transfer_input.resize(byte_len, 0);
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_transfer_ptr() -> u32 {
    ABI_STATE.with(|state| state.borrow().transfer_input.as_ptr() as usize as u32)
}

fn ensure_minimum_transfer_outer(
    state: &mut WasmAbiState,
    prospective: u128,
) -> Result<(), WasmCommandRuntimeError> {
    if let Some(worker) = state.minimum_parallel_worker.as_ref() {
        worker.ensure_outer_capacity(prospective)?;
    }
    if let Some((_, completion)) = state.distributed_completion.as_mut() {
        completion.ensure_outer_capacity(prospective)?;
    }
    Ok(())
}

#[no_mangle]
pub extern "C" fn clearra_wasm_tablebase_install() -> i32 {
    if let Err(status) = ABI_STATE.with(|state| state.borrow().require_mutation_admission()) {
        return status;
    }
    clear_panic_diagnostics();
    let input = match ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        debug_assert!(state.require_mutation_admission().is_ok());
        Ok::<_, i32>(std::mem::take(&mut state.transfer_input))
    }) {
        Ok(input) => input,
        Err(status) => return status,
    };
    let result = install_pc4_compact_tablebase(&input);
    drop(input);
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        match result {
            Ok(tablebase) => {
                state.set_output(format!(
                    "{{\"schema_version\":12,\"tier\":\"compact-exact\",\"artifact_bytes\":{},\"certified_states\":{},\"certified_targets\":{},\"payload_sha256\":\"{}\"}}",
                    tablebase.artifact_bytes(),
                    tablebase.certified_state_count(),
                    tablebase.certified_target_count(),
                    tablebase.payload_sha256_hex()
                ));
                ABI_OK
            }
            Err(error) => {
                state.set_error("E_WASM_TABLEBASE_INSTALL", error);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_tablebase_release() -> i32 {
    if let Err(status) = ABI_STATE.with(|state| state.borrow().require_mutation_admission()) {
        return status;
    }
    clear_panic_diagnostics();
    let released = release_pc4_compact_tablebase();
    i32::from(released)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_prepare() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        state.tiling_solution_page_store = None;
        state.product_page_source_owner = None;
        state.product_page_store = None;
        let command_text = match String::from_utf8(std::mem::take(&mut state.input)) {
            Ok(command_text) => command_text,
            Err(error) => {
                let utf8_error = error.utf8_error();
                drop(error.into_bytes());
                state.set_error("E_WASM_COMMAND_UTF8", utf8_error);
                return ABI_ERROR;
            }
        };
        state.distributed_coordinator = None;
        state.distributed_ready_result = None;
        state.distributed_completion = None;
        let preparation = WasmDistributedCoordinator::prepare(
            state.runtime.command_runtime(),
            command_text.as_str(),
        );
        drop(command_text);
        match preparation {
            Ok(WasmDistributedPreparation::Serial) => WasmDistributedMode::Serial as i32,
            Ok(WasmDistributedPreparation::Ready(result)) => {
                state.distributed_ready_result = Some(result);
                ABI_DISTRIBUTED_READY
            }
            Ok(WasmDistributedPreparation::Coordinator(coordinator)) => {
                let mode = coordinator.mode() as i32;
                state.distributed_coordinator = Some(coordinator);
                mode
            }
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_worker_count() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .and_then(|coordinator| u32::try_from(coordinator.worker_count()).ok())
            .unwrap_or(1)
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_worker_count_available() -> u32 {
    // The legacy contract defines a serial/default worker count of one even when there is no
    // coordinator, so this count is always available.
    1
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_worker_count_exact() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .is_none_or(|coordinator| u32::try_from(coordinator.worker_count()).is_ok())
            .into()
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verification_required() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .is_none_or(WasmDistributedCoordinator::verification_required)
            .into()
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_tiling_geometry_parallel() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .is_some_and(WasmDistributedCoordinator::tiling_geometry_parallel)
            .into()
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_requested_backend() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .map(|coordinator| coordinator.requested_backend() as u32)
            .unwrap_or(WasmDistributedRequestedBackend::Auto as u32)
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_preparation_fallback_reason() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .map(|coordinator| coordinator.preparation_fallback_reason() as u32)
            .unwrap_or(WasmDistributedFallbackReason::None as u32)
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_worker_initialization() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let output = state
            .distributed_coordinator
            .as_ref()
            .and_then(WasmDistributedCoordinator::worker_initialization)
            .unwrap_or_default();
        state.set_output_bytes(output);
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_worker_initialization_deferred() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .is_some_and(WasmDistributedCoordinator::worker_initialization_deferred)
            .into()
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_geometry_nodes() -> u32 {
    AbiU32Count::legacy_or(
        distributed_count(|value| value.progress().geometry_nodes),
        0,
    )
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_available() -> u32 {
    ABI_STATE.with(|state| state.borrow().distributed_coordinator.is_some().into())
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_geometry_nodes_exact() -> u32 {
    AbiU32Count::exact_or_false(distributed_count(|value| value.progress().geometry_nodes))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_candidate_count() -> u32 {
    AbiU32Count::legacy_or(distributed_count(|value| value.progress().candidates), 0)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_candidate_count_exact() -> u32 {
    AbiU32Count::exact_or_false(distributed_count(|value| value.progress().candidates))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_build_nodes() -> u32 {
    AbiU32Count::legacy_or(distributed_count(|value| value.progress().build_nodes), 0)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_build_nodes_exact() -> u32 {
    AbiU32Count::exact_or_false(distributed_count(|value| value.progress().build_nodes))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_coverage_checks() -> u32 {
    AbiU32Count::legacy_or(
        distributed_count(|value| value.progress().coverage_checks),
        0,
    )
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_coverage_checks_exact() -> u32 {
    AbiU32Count::exact_or_false(distributed_count(|value| value.progress().coverage_checks))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_candidate_family_count_available() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .is_some_and(|coordinator| coordinator.progress().candidate_family_count.is_some())
            .into()
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_candidate_family_count_exact() -> u32 {
    // All four u32 words are exported, so every available u128 value is represented exactly.
    clearra_wasm_distributed_progress_candidate_family_count_available()
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_candidate_family_count_word(
    word_index: u32,
) -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .and_then(|coordinator| coordinator.progress().candidate_family_count)
            .map_or(0, |count| u128_word(count, word_index))
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_pass_index() -> u32 {
    AbiU32Count::legacy_or(distributed_count(|value| value.progress().pass_index), 0)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_pass_index_exact() -> u32 {
    AbiU32Count::exact_or_false(distributed_count(|value| value.progress().pass_index))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_pass_count() -> u32 {
    AbiU32Count::legacy_or(
        distributed_count(|value| value.progress().pass_count.max(1)),
        1,
    )
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_pass_count_exact() -> u32 {
    AbiU32Count::exact_or_false(distributed_count(|value| {
        value.progress().pass_count.max(1)
    }))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_layer_index() -> u32 {
    AbiU32Count::legacy_or(distributed_count(|value| value.progress().layer_index), 0)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_layer_index_exact() -> u32 {
    AbiU32Count::exact_or_false(distributed_count(|value| value.progress().layer_index))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_layer_count() -> u32 {
    AbiU32Count::legacy_or(distributed_count(|value| value.progress().layer_count), 0)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_layer_count_exact() -> u32 {
    AbiU32Count::exact_or_false(distributed_count(|value| value.progress().layer_count))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_layer_done() -> u32 {
    AbiU32Count::legacy_or(distributed_count(|value| value.progress().layer_done), 0)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_layer_done_exact() -> u32 {
    AbiU32Count::exact_or_false(distributed_count(|value| value.progress().layer_done))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_layer_total() -> u32 {
    AbiU32Count::legacy_or(distributed_count(|value| value.progress().layer_total), 0)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_layer_total_exact() -> u32 {
    AbiU32Count::exact_or_false(distributed_count(|value| value.progress().layer_total))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_produce(work_budget: u32, batch_capacity: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let Some(coordinator) = state.distributed_coordinator.as_mut() else {
            state.set_error(
                "E_WASM_DISTRIBUTED_STATE",
                "distributed coordinator is not active",
            );
            return ABI_ERROR;
        };
        match coordinator.advance_producer(work_budget as usize, batch_capacity as usize) {
            Ok(WasmDistributedProducerAdvance::Pending) => 0,
            Ok(WasmDistributedProducerAdvance::Initialization(output)) => {
                state.set_output_bytes(output);
                4
            }
            Ok(WasmDistributedProducerAdvance::Batch(output)) => {
                state.set_output_bytes(output);
                1
            }
            Ok(WasmDistributedProducerAdvance::Completed) => 2,
            Ok(WasmDistributedProducerAdvance::Cancelled) => 3,
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_merge_partial() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let input = std::mem::take(&mut state.transfer_input);
        let input_retained_bytes = input.capacity() as u128;
        let outcome = state
            .distributed_coordinator
            .as_mut()
            .ok_or((
                "E_WASM_DISTRIBUTED_STATE",
                "distributed coordinator is not active".to_owned(),
            ))
            .and_then(|coordinator| {
                coordinator
                    .absorb_partial_with_external_retained(&input, input_retained_bytes)
                    .map_err(|error| (error.code(), error.message().to_owned()))
            });
        drop(input);
        match outcome {
            Ok(()) => ABI_OK,
            Err((code, message)) => {
                state.set_error(code, message);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish(job_id: u32, workers_used: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let result = if state.distributed_ready_result.is_some() {
            if workers_used != 0 {
                state.set_error(
                    "E_WASM_DISTRIBUTED_STATE",
                    "prepared terminal result requires zero distributed workers",
                );
                return ABI_ERROR;
            }
            state
                .distributed_ready_result
                .take()
                .expect("ready result presence checked above")
        } else {
            let Some(coordinator) = state.distributed_coordinator.take() else {
                state.set_error(
                    "E_WASM_DISTRIBUTED_STATE",
                    "distributed coordinator is not active",
                );
                return ABI_ERROR;
            };
            match coordinator.finish(workers_used as usize) {
                Ok(result) => result,
                Err(error) => {
                    state.set_runtime_error(&error);
                    return ABI_ERROR;
                }
            }
        };
        publish_distributed_result(&mut state, job_id, result)
    })
}

fn publish_distributed_result(
    state: &mut WasmAbiState,
    job_id: u32,
    result: clearra_wasm::WasmExecutionResult,
) -> i32 {
    let output = match serialize_distributed_final_events(job_id.into(), &result) {
        Ok(output) => output,
        Err(error) => {
            state.set_error(error.code(), error.message());
            return ABI_ERROR;
        }
    };
    state.tiling_solution_page_store = result
        .tiling_solution_page_store()
        .cloned()
        .map(AbiTilingSolutionPageStore::legacy);
    state.product_page_source_owner = result.product_page_source_owner().cloned();
    state.product_page_store = None;
    state.set_output(output);
    ABI_OK
}

/// Start an owned completion without running an exact minimum proof inside
/// the source-merge ABI call. Status 1 has no output lease; 0 has final events.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_start(job_id: u32, workers_used: u32) -> i32 {
    let cooperative = ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .is_some_and(WasmDistributedCoordinator::requires_cooperative_completion)
    });
    if !cooperative {
        return clearra_wasm_distributed_finish(job_id, workers_used);
    }
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if state.distributed_completion.is_some() {
            state.set_error(
                "E_WASM_DISTRIBUTED_STATE",
                "distributed completion already active",
            );
            return ABI_ERROR;
        }
        let coordinator = state
            .distributed_coordinator
            .take()
            .expect("coordinator kind checked");
        match coordinator.into_cooperative_completion(workers_used as usize) {
            Ok(completion) => {
                state.distributed_completion = Some((job_id, completion));
                1
            }
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

/// Resume the same source-bound proof/selection cursor. The host must yield
/// between pending calls so cancellation and lifecycle events can be handled.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_advance(job_id: u32, maximum_work: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let outer = minimum_coordinator_outer_bytes(&state).unwrap_or(u128::MAX);
        #[cfg(target_arch = "wasm32")]
        let physical = (core::arch::wasm32::memory_size(0) as u128) * 65_536;
        #[cfg(not(target_arch = "wasm32"))]
        let physical = 0;
        let Some((active_job_id, completion)) = state.distributed_completion.as_mut() else {
            state.set_error(
                "E_WASM_DISTRIBUTED_STATE",
                "distributed completion is not active",
            );
            return ABI_ERROR;
        };
        if *active_job_id != job_id {
            state.set_error(
                "E_WASM_DISTRIBUTED_STATE",
                "distributed completion job identity mismatch",
            );
            return ABI_ERROR;
        }
        match completion.advance_guarded(maximum_work as usize, outer, physical) {
            Ok(WasmDistributedCompletionAdvance::Pending) => 1,
            Ok(WasmDistributedCompletionAdvance::Completed(result)) => {
                state.distributed_completion = None;
                publish_distributed_result(&mut state, job_id, result)
            }
            Ok(WasmDistributedCompletionAdvance::Cancelled) => {
                state.distributed_completion = None;
                state.set_error(
                    "E_WASM_DISTRIBUTED_CANCELLED",
                    "distributed completion cancelled",
                );
                ABI_ERROR
            }
            Err(error) => {
                state.distributed_completion = None;
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

fn with_minimum_parallel_output(
    job_id: u32,
    action: impl FnOnce(
        &mut WasmDistributedCompletionSession,
        u128,
    ) -> Result<Option<Vec<u8>>, WasmCommandRuntimeError>,
) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let outer = minimum_coordinator_outer_bytes(&state).unwrap_or(u128::MAX);
        let Some((active_job_id, completion)) = state.distributed_completion.as_mut() else {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum completion is not active",
            );
            return ABI_ERROR;
        };
        if *active_job_id != job_id {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum completion job identity mismatch",
            );
            return ABI_ERROR;
        }
        match action(completion, outer) {
            Ok(Some(bytes)) => {
                state.set_output_bytes(bytes);
                1
            }
            Ok(None) => ABI_OK,
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

/// Read-only positive decision signal. An empty task queue is not a SAT proof.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_found(job_id: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        match state.distributed_completion.as_ref() {
            Some((active, completion)) if *active == job_id => {
                i32::from(completion.parallel_query_satisfied())
            }
            _ => {
                state.set_error(
                    "E_WASM_MINIMUM_PARALLEL_STATE",
                    "minimum completion job identity mismatch",
                );
                ABI_ERROR
            }
        }
    })
}

/// Return this active task's core-minted Cancelled receipt, never ProvedNone.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_worker_cancel() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let outer = minimum_coordinator_outer_bytes(&state).unwrap_or(u128::MAX);
        let Some(worker) = state.minimum_parallel_worker.as_mut() else {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum worker query is not initialized",
            );
            return ABI_ERROR;
        };
        match worker.cancel_guarded(outer) {
            Ok(bytes) => {
                state.set_output_bytes(bytes);
                ABI_OK
            }
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

/// Enable shared-core parallel exact proof. Status 1 owns a new query packet;
/// status 0 means no newly available query (including an already published one).
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_prepare(
    job_id: u32,
    target_partitions: u32,
) -> i32 {
    with_minimum_parallel_output(job_id, |completion, outer| {
        completion.prepare_parallel_guarded(target_partitions as usize, outer)
    })
}

/// Drain the core-issued frontier, never host-generated row subsets.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_task(job_id: u32) -> i32 {
    with_minimum_parallel_output(
        job_id,
        WasmDistributedCompletionSession::take_parallel_task_guarded,
    )
}

/// Validate one opaque receipt against the still-live source/query frontier.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_merge(job_id: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if !state
            .distributed_completion
            .as_ref()
            .is_some_and(|(active, _)| *active == job_id)
        {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum completion job identity mismatch",
            );
            return ABI_ERROR;
        }
        let outer_bytes = minimum_coordinator_outer_bytes(&state).unwrap_or(u128::MAX);
        let bytes = core::mem::take(&mut state.transfer_input);
        let completion = &mut state
            .distributed_completion
            .as_mut()
            .expect("job checked")
            .1;
        match completion.merge_parallel_receipt(&bytes, outer_bytes) {
            Ok(()) => ABI_OK,
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

fn minimum_coordinator_outer_bytes(state: &WasmAbiState) -> Option<u128> {
    let bytes = (core::mem::size_of::<WasmAbiState>() as u128)
        .checked_add(state.input.capacity() as u128)?
        .checked_add(state.transfer_input.capacity() as u128)?
        .checked_add(state.output.capacity() as u128)?;
    #[cfg(target_arch = "wasm32")]
    {
        Some(bytes)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        bytes.checked_add(state.runtime.minimum_coordinator_idle_retained_bytes()?)
    }
}

fn with_minimum_coordinator_shard(
    job_id: u32,
    action: impl FnOnce(
        &mut WasmDistributedCompletionSession,
        u128,
        u128,
    ) -> Result<bool, WasmCommandRuntimeError>,
) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let outer_bytes = minimum_coordinator_outer_bytes(&state);
        #[cfg(target_arch = "wasm32")]
        let physical_floor = (core::arch::wasm32::memory_size(0) as u128).checked_mul(65_536);
        #[cfg(not(target_arch = "wasm32"))]
        let physical_floor = Some(0_u128);
        let Some((active_job_id, completion)) = state.distributed_completion.as_mut() else {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum completion is not active",
            );
            return ABI_ERROR;
        };
        if *active_job_id != job_id {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum completion job identity mismatch",
            );
            return ABI_ERROR;
        }
        // Missing native owner projection or checked overflow declines local
        // admission; it is never turned into zero retained bytes.
        match action(
            completion,
            outer_bytes.unwrap_or(u128::MAX),
            physical_floor.unwrap_or(u128::MAX),
        ) {
            Ok(value) => i32::from(value),
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

/// Add bounded assistance only for an idle executor; no proof is minted here.
/// Version 1 binds both the control owner and every remote replica to the
/// actual outer lease. Absence means legacy shared topology only.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_guard_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_configure(
    job_id: u32,
    host_compute: u32,
    memory_low: u32,
    memory_high: u32,
) -> i32 {
    with_minimum_coordinator_shard(job_id, |completion, _, _| {
        let memory = (memory_high as u128) << 32 | memory_low as u128;
        completion
            .configure_parallel_control(host_compute as usize, memory)
            .map(|()| true)
    })
}

/// Decline (0) is pre-task only and leaves the original control lease intact.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_admit(
    job_id: u32,
    remote_count: u32,
    control_only: u32,
    host_compute: u32,
    memory_low: u32,
    memory_high: u32,
) -> i32 {
    if control_only > 1 {
        return ABI_ERROR;
    }
    with_minimum_coordinator_shard(job_id, |completion, outer, physical| {
        let memory = (memory_high as u128) << 32 | memory_low as u128;
        completion.admit_parallel_control(
            remote_count as usize,
            control_only != 0,
            host_compute as usize,
            memory,
            outer,
            physical,
        )
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_guarded_query(job_id: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let outer = minimum_coordinator_outer_bytes(&state).unwrap_or(u128::MAX);
        let Some((active, completion)) = state.distributed_completion.as_mut() else {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum completion is unavailable",
            );
            return ABI_ERROR;
        };
        if *active != job_id {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum completion job identity mismatch",
            );
            return ABI_ERROR;
        }
        match completion.guarded_parallel_query(outer) {
            Ok(bytes) => {
                state.set_output_bytes(bytes);
                ABI_OK
            }
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_assist(
    job_id: u32,
    maximum_children: u32,
) -> i32 {
    with_minimum_coordinator_shard(job_id, |completion, outer, physical| {
        completion.prepare_idle_assist(maximum_children as usize, outer, physical)
    })
}

/// Opaque routing metadata for the most recently issued original/assist task.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_last_task_key(job_id: u32) -> i32 {
    with_minimum_parallel_output(job_id, |completion, _| {
        completion.last_parallel_task_key().map(Some)
    })
}

/// Read-only core decision for selectively cancelling a redundant task owner.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_redundant(job_id: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if !state
            .distributed_completion
            .as_ref()
            .is_some_and(|(active, _)| *active == job_id)
        {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum completion job identity mismatch",
            );
            return ABI_ERROR;
        }
        let key = core::mem::take(&mut state.transfer_input);
        match state
            .distributed_completion
            .as_ref()
            .expect("job checked")
            .1
            .parallel_task_redundant(&key)
        {
            Ok(redundant) => i32::from(redundant),
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

/// Take one core-issued shard on the coordinator without an external worker.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_local_start(job_id: u32) -> i32 {
    with_minimum_coordinator_shard(
        job_id,
        WasmDistributedCompletionSession::start_coordinator_shard,
    )
}

/// 0 is pending, 1 means the local owner drained (receipt or exact remote retry).
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_local_advance(
    job_id: u32,
    maximum_work: u32,
) -> i32 {
    with_minimum_coordinator_shard(job_id, |completion, outer_bytes, _| {
        completion.advance_coordinator_shard(maximum_work as usize, outer_bytes)
    })
}

/// Replace a drained Geometry verifier or previous AtMost query on this
/// durable worker. Input is read once per query, not once per partition.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_worker_init() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_drained_verifier_replacement() {
            return status;
        }
        // Include the detached packet and prior output capacity in the outer
        // owner before resetting the previous, fully drained query.
        let outer = minimum_coordinator_outer_bytes(&state).unwrap_or(u128::MAX);
        #[cfg(target_arch = "wasm32")]
        let physical = (core::arch::wasm32::memory_size(0) as u128) * 65_536;
        #[cfg(not(target_arch = "wasm32"))]
        let physical = 0;
        let bytes = core::mem::take(&mut state.transfer_input);
        state.reset_distributed_state();
        match WasmMinimumParallelWorker::initialize_guarded(&bytes, outer, physical) {
            Ok(worker) => {
                state.minimum_parallel_worker = Some(worker);
                ABI_OK
            }
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_worker_start() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let outer = minimum_coordinator_outer_bytes(&state).unwrap_or(u128::MAX);
        let bytes = core::mem::take(&mut state.transfer_input);
        let Some(worker) = state.minimum_parallel_worker.as_mut() else {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum worker query is not initialized",
            );
            return ABI_ERROR;
        };
        match worker.start_guarded(&bytes, outer) {
            Ok(()) => ABI_OK,
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

/// Status 1 is cooperative pending without an output lease; status 0 owns the
/// exact core receipt. Cancellation/reset drops the cursor without a negative.
#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_finish_parallel_worker_advance(
    maximum_work: u32,
) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let outer = minimum_coordinator_outer_bytes(&state).unwrap_or(u128::MAX);
        let Some(worker) = state.minimum_parallel_worker.as_mut() else {
            state.set_error(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum worker query is not initialized",
            );
            return ABI_ERROR;
        };
        match worker.advance_guarded(maximum_work as usize, outer) {
            Ok(Some(bytes)) => {
                state.set_output_bytes(bytes);
                ABI_OK
            }
            Ok(None) => 1,
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_tiling_solution_count() -> u32 {
    AbiU32Count::legacy_or(tiling_solution_count(), u32::MAX)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_tiling_solution_count_available() -> u32 {
    ABI_STATE.with(|state| state.borrow().tiling_solution_page_store.is_some().into())
}

#[no_mangle]
pub extern "C" fn clearra_wasm_tiling_solution_count_exact() -> u32 {
    AbiU32Count::exact_or_false(tiling_solution_count())
}

struct CheckedJsonLength {
    len: Option<usize>,
}

impl CheckedJsonLength {
    fn new() -> Self {
        Self { len: Some(0) }
    }

    fn finish(self) -> Result<usize, &'static str> {
        self.len.ok_or("wasm_tiling_solution_page_size_overflow")
    }
}

impl fmt::Write for CheckedJsonLength {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.len = self.len.and_then(|len| len.checked_add(value.len()));
        self.len.map(|_| ()).ok_or(fmt::Error)
    }
}

fn write_tiling_solution_page_json(
    store: &TilingSolutionPageStore,
    offset: usize,
    limit: usize,
    output: &mut impl fmt::Write,
) -> Result<(), &'static str> {
    output
        .write_char('[')
        .map_err(|_| "wasm_tiling_solution_page_serialize_failed")?;
    let mut first = true;
    let mut write_failed = false;
    store.for_each_page_identity(offset, limit, |identity| {
        if write_failed {
            return;
        }
        let result = (|| {
            if !first {
                output.write_char(',')?;
            }
            first = false;
            output.write_char('"')?;
            identity.write_canonical(output)?;
            output.write_char('"')
        })();
        write_failed = result.is_err();
    })?;
    if write_failed {
        return Err("wasm_tiling_solution_page_serialize_failed");
    }
    output
        .write_char(']')
        .map_err(|_| "wasm_tiling_solution_page_serialize_failed")
}

fn checked_tiling_solution_page_json_len(
    store: &TilingSolutionPageStore,
    offset: usize,
    limit: usize,
) -> Result<usize, &'static str> {
    let mut length = CheckedJsonLength::new();
    write_tiling_solution_page_json(store, offset, limit, &mut length)?;
    length.finish()
}

fn serialize_tiling_solution_page_json(
    store: &TilingSolutionPageStore,
    offset: usize,
    limit: usize,
    exact_len: usize,
) -> Result<String, &'static str> {
    let mut output = String::new();
    output
        .try_reserve_exact(exact_len)
        .map_err(|_| "wasm_tiling_solution_page_storage_unavailable")?;
    write_tiling_solution_page_json(store, offset, limit, &mut output)?;
    if output.len() != exact_len {
        return Err("wasm_tiling_solution_page_size_mismatch");
    }
    Ok(output)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_tiling_solution_page(offset: u32, limit: u32) -> i32 {
    const MAX_PAGE_SIZE: usize = 1_000;

    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.runtime.has_active_finite_job() || state.runtime.has_completed_governed_events() {
            return ABI_ERROR;
        }
        if let Err(status) = state.require_released_output() {
            return status;
        }
        let Some(store) = state.tiling_solution_page_store.as_ref() else {
            state.set_error(
                "E_WASM_TILING_PAGE_STATE",
                "tiling solution page store is not available",
            );
            return ABI_ERROR;
        };
        let governed = store.is_governed();
        let offset = offset as usize;
        let limit = (limit as usize).min(MAX_PAGE_SIZE);
        let exact_len = match checked_tiling_solution_page_json_len(store.store(), offset, limit) {
            Ok(exact_len) => exact_len,
            Err(reason) => {
                if !governed {
                    state.set_error("E_WASM_TILING_PAGE", reason);
                }
                return ABI_ERROR;
            }
        };
        if !store.governed_output_fits(exact_len) {
            return ABI_ERROR;
        }
        let output =
            match serialize_tiling_solution_page_json(store.store(), offset, limit, exact_len) {
                Ok(output) => output,
                Err(reason) => {
                    if !governed {
                        state.set_error("E_WASM_TILING_PAGE", reason);
                    }
                    return ABI_ERROR;
                }
            };
        // `try_reserve_exact` may retain more than requested. Recheck the
        // allocator-observed capacity before moving the page into ABI state.
        if !store.governed_output_fits(output.capacity()) {
            return ABI_ERROR;
        }
        state.set_output(output);
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_tiling_solution_release() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_released_output() {
            return status;
        }
        if state.runtime.has_active_finite_job() || state.runtime.has_completed_governed_events() {
            return ABI_ERROR;
        }
        state.tiling_solution_page_store = None;
        ABI_OK
    })
}

fn ensure_product_page_store(state: &mut WasmAbiState) -> Result<(), &'static str> {
    if state.product_page_store.is_some() {
        return Ok(());
    }
    let source = state
        .product_page_source_owner
        .take()
        .ok_or("product page source is not available")?;
    match ProductPageStore::from_source(source) {
        Ok(store) => {
            state.product_page_store = Some(AbiProductPageStore::legacy(store));
            Ok(())
        }
        Err(error) => Err(error.as_str()),
    }
}

#[no_mangle]
pub extern "C" fn clearra_wasm_product_page_available() -> u32 {
    ABI_STATE.with(|state| {
        let state = state.borrow();
        (state.product_page_source_owner.is_some() || state.product_page_store.is_some()).into()
    })
}

/// Advances one exact outer alternative under a bounded combination budget.
/// A work-budget response is retryable and never claims enumeration is sealed.
#[no_mangle]
pub extern "C" fn clearra_wasm_product_page_next(maximum_work_steps: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_released_output() {
            return status;
        }
        if state.runtime.has_active_finite_job() || state.runtime.has_completed_governed_events() {
            return ABI_ERROR;
        }
        if let Err(reason) = ensure_product_page_store(&mut state) {
            state.set_error("E_WASM_PRODUCT_PAGE_STATE", reason);
            return ABI_ERROR;
        }
        let governed = state
            .product_page_store
            .as_ref()
            .is_some_and(AbiProductPageStore::is_governed);
        let coverage_portfolio = state
            .product_page_store
            .as_ref()
            .is_some_and(|store| store.store().coverage_portfolio().is_some());
        let parity_report = state
            .product_page_store
            .as_ref()
            .is_some_and(|store| store.store().parity_report().is_some());
        let output = if coverage_portfolio {
            let (advance, retained_slot) = {
                let advance_result = match state.product_page_store.as_mut() {
                    Some(AbiProductPageStore::Legacy(store)) => {
                        let Some(store) = store.coverage_portfolio_mut() else {
                            return ABI_ERROR;
                        };
                        store.next_page(maximum_work_steps.max(1) as u64, &mut || false)
                    }
                    Some(AbiProductPageStore::Governed {
                        store,
                        memory_limit_bytes,
                    }) => {
                        let memory_limit_bytes = *memory_limit_bytes;
                        let Some(store) = store.coverage_portfolio_mut() else {
                            return ABI_ERROR;
                        };
                        store.next_page_with_memory_guard(
                            maximum_work_steps.max(1) as u64,
                            &mut |app_whole_live| {
                                AbiProductPageStore::governed_app_peak_fits(
                                    memory_limit_bytes,
                                    app_whole_live,
                                    0,
                                )
                            },
                            &mut || false,
                        )
                    }
                    None => return ABI_ERROR,
                };
                let advance = match advance_result {
                    Ok(advance) => advance,
                    Err(error) => {
                        if governed {
                            state.product_page_store = None;
                        } else {
                            state.set_error("E_WASM_PRODUCT_PAGE", error.as_str());
                        }
                        return ABI_ERROR;
                    }
                };
                let retained_slot = advance.page().and_then(|page| {
                    state
                        .product_page_store
                        .as_ref()
                        .and_then(|store| store.store().coverage_portfolio())
                        .and_then(|store| {
                            store.retained_page_slot(page.alternative_index_decimal())
                        })
                });
                (advance, retained_slot)
            };
            if state
                .product_page_store
                .as_ref()
                .is_some_and(AbiProductPageStore::governed_store_fits)
            {
                let Some(store) = state
                    .product_page_store
                    .as_ref()
                    .and_then(|store| store.store().coverage_portfolio())
                else {
                    return ABI_ERROR;
                };
                if advance.page().is_some() {
                    let Some(retained_slot) = retained_slot else {
                        return ABI_ERROR;
                    };
                    serialize_coverage_portfolio_retained_page(store, retained_slot, 1)
                } else {
                    serialize_coverage_portfolio_advance_state(&advance)
                }
            } else {
                state.product_page_store = None;
                return ABI_ERROR;
            }
        } else if parity_report {
            let page = {
                let Some(store) = state
                    .product_page_store
                    .as_mut()
                    .and_then(|store| store.store_mut().parity_report_mut())
                else {
                    return ABI_ERROR;
                };
                match store.next_page() {
                    Ok(page) => page,
                    Err(error) => {
                        if governed {
                            state.product_page_store = None;
                        } else {
                            state.set_error("E_WASM_PRODUCT_PAGE", error.as_str());
                        }
                        return ABI_ERROR;
                    }
                }
            };
            if state
                .product_page_store
                .as_ref()
                .is_some_and(AbiProductPageStore::governed_store_fits)
            {
                match page {
                    Some(page) => serialize_parity_report_page(&page),
                    None => serialize_parity_report_exhausted(),
                }
            } else {
                state.product_page_store = None;
                return ABI_ERROR;
            }
        } else {
            if !governed {
                state.set_error(
                    "E_WASM_PRODUCT_PAGE_KIND",
                    "product page kind is unsupported",
                );
            }
            return ABI_ERROR;
        };
        match output {
            Ok(output) => {
                if state
                    .product_page_store
                    .as_ref()
                    .is_some_and(|store| !store.governed_output_fits(output.capacity()))
                {
                    state.product_page_store = None;
                    return ABI_ERROR;
                }
                state.set_output(output);
                ABI_OK
            }
            Err(error) => {
                if governed {
                    state.product_page_store = None;
                } else {
                    state.set_runtime_error(&error);
                }
                ABI_ERROR
            }
        }
    })
}

/// Legacy numeric compatibility export. Browser/Desktop product code uses the
/// exact-decimal request export below.
#[no_mangle]
pub extern "C" fn clearra_wasm_product_page_get(
    outer_page_number: u32,
    member_page_number: u32,
) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let alternative_index_decimal = outer_page_number.to_string();
        let additional_live_capacity = alternative_index_decimal.capacity();
        product_page_get_from_state(
            &mut state,
            &alternative_index_decimal,
            member_page_number as usize,
            DEFAULT_PRODUCT_PAGE_WORK_STEPS,
            additional_live_capacity,
        )
    })
}

/// Loads a product/member page from canonical positive decimal identities in
/// either the compatibility v1 request or the bounded v2 request:
/// `portfolio-page-request.v2\n<outer>\n<member>\n<maximum-work-steps>`.
#[no_mangle]
pub extern "C" fn clearra_wasm_product_page_get_exact() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_released_output() {
            return status;
        }
        if state.runtime.has_active_finite_job() || state.runtime.has_completed_governed_events() {
            return ABI_ERROR;
        }
        let request = match String::from_utf8(std::mem::take(&mut state.input)) {
            Ok(request) => request,
            Err(_) => return ABI_ERROR,
        };
        let (alternative_index_decimal, member_page_number, maximum_work_steps) =
            match parse_product_page_request(&request) {
                Ok(request) => request,
                Err(_) => return ABI_ERROR,
            };
        product_page_get_from_state(
            &mut state,
            alternative_index_decimal,
            member_page_number,
            maximum_work_steps,
            request.capacity(),
        )
    })
}

fn product_page_get_from_state(
    state: &mut WasmAbiState,
    alternative_index_decimal: &str,
    member_page_number: usize,
    maximum_work_steps: u64,
    additional_live_capacity: usize,
) -> i32 {
    if let Err(status) = state.require_released_output() {
        return status;
    }
    if state.runtime.has_active_finite_job() || state.runtime.has_completed_governed_events() {
        return ABI_ERROR;
    }
    if let Err(reason) = ensure_product_page_store(state) {
        state.set_error("E_WASM_PRODUCT_PAGE_STATE", reason);
        return ABI_ERROR;
    }
    let governed = state
        .product_page_store
        .as_ref()
        .is_some_and(AbiProductPageStore::is_governed);
    if state
        .product_page_store
        .as_ref()
        .is_some_and(|store| !store.governed_request_fits(additional_live_capacity))
    {
        state.product_page_store = None;
        return ABI_ERROR;
    }
    let coverage_portfolio = state
        .product_page_store
        .as_ref()
        .is_some_and(|store| store.store().coverage_portfolio().is_some());
    let output = if coverage_portfolio {
        let load_advance_result = match state.product_page_store.as_mut() {
            Some(AbiProductPageStore::Legacy(store)) => {
                let Some(store) = store.coverage_portfolio_mut() else {
                    return ABI_ERROR;
                };
                store.load_page_by_alternative_index_slice(
                    alternative_index_decimal,
                    maximum_work_steps,
                    &mut || false,
                )
            }
            Some(AbiProductPageStore::Governed {
                store,
                memory_limit_bytes,
            }) => {
                let memory_limit_bytes = *memory_limit_bytes;
                let Some(store) = store.coverage_portfolio_mut() else {
                    return ABI_ERROR;
                };
                store.load_page_by_alternative_index_slice_with_memory_guard(
                    alternative_index_decimal,
                    maximum_work_steps,
                    &mut |app_whole_live| {
                        AbiProductPageStore::governed_app_peak_fits(
                            memory_limit_bytes,
                            app_whole_live,
                            additional_live_capacity,
                        )
                    },
                    &mut || false,
                )
            }
            None => return ABI_ERROR,
        };
        let load_advance = match load_advance_result {
            Ok(load_advance) => load_advance,
            Err(error) => {
                if governed {
                    state.product_page_store = None;
                } else {
                    state.set_error("E_WASM_PRODUCT_PAGE", error.as_str());
                }
                return ABI_ERROR;
            }
        };
        if state.product_page_store.as_ref().is_some_and(|store| {
            !store.governed_store_fits() || !store.governed_request_fits(additional_live_capacity)
        }) {
            state.product_page_store = None;
            return ABI_ERROR;
        }
        let Some(store) = state
            .product_page_store
            .as_ref()
            .and_then(|store| store.store().coverage_portfolio())
        else {
            return ABI_ERROR;
        };
        match load_advance.state() {
            PortfolioPageLoadState::Page => {
                let Some(retained_slot) = load_advance.retained_slot() else {
                    return ABI_ERROR;
                };
                serialize_coverage_portfolio_retained_page(store, retained_slot, member_page_number)
            }
            PortfolioPageLoadState::WorkBudgetExhausted | PortfolioPageLoadState::Cancelled => {
                serialize_coverage_portfolio_load_advance_state(store, load_advance)
            }
        }
    } else if state
        .product_page_store
        .as_ref()
        .is_some_and(|store| store.store().pc_replay().is_some())
    {
        let Some(geometry_page_number) = parse_canonical_positive_usize(alternative_index_decimal)
        else {
            state.set_error("E_WASM_PRODUCT_PAGE", "invalid-geometry-page");
            return ABI_ERROR;
        };
        let Some(request_and_envelope_bytes) =
            additional_live_capacity.checked_add(PC_REPLAY_HOST_ENVELOPE_RESERVE)
        else {
            return ABI_ERROR;
        };
        let Some(page_store) = state.product_page_store.as_ref() else {
            return ABI_ERROR;
        };
        let Some(replay) = page_store.store().pc_replay() else {
            return ABI_ERROR;
        };
        let source_limit = replay.source().maximum_memory_bytes();
        let memory_limit_bytes = match page_store {
            AbiProductPageStore::Legacy(_) => source_limit,
            AbiProductPageStore::Governed {
                memory_limit_bytes, ..
            } => source_limit.min(*memory_limit_bytes),
        };
        let entry_required = replay
            .checked_host_entry_bytes()
            .and_then(|n| {
                n.checked_add(core::mem::size_of::<Option<AbiProductPageStore>>() as u128)
            })
            .and_then(|n| n.checked_add(request_and_envelope_bytes as u128));
        if !entry_required.is_some_and(|required| required <= memory_limit_bytes) {
            state.product_page_store = None;
            return publish_pc_replay_error(
                state,
                "complete_replay_host_memory_limit_exceeded",
                entry_required,
                Some(memory_limit_bytes),
                memory_limit_bytes,
                additional_live_capacity,
            );
        }
        let control = clearra_wasm::ExecutionControl::default();
        // Match App's primitive ceiling; App also yields at its monotonic 8ms
        // quantum. Do not force every cache miss into 64-step host round trips.
        // Clamp before narrowing a caller-provided u64 on wasm32.
        let work = maximum_work_steps.clamp(1, 8192) as usize;
        let mut rejected_host_peak = None;
        let advanced = match state.product_page_store.as_mut() {
            Some(AbiProductPageStore::Legacy(store))
            | Some(AbiProductPageStore::Governed { store, .. }) => {
                let Some(store) = store.pc_replay_mut() else {
                    return ABI_ERROR;
                };
                store.advance_page_with_memory_guard(
                    geometry_page_number,
                    member_page_number,
                    work,
                    &control,
                    &mut |app_whole_live| {
                        let required =
                            app_whole_live
                                .checked_add(
                                    core::mem::size_of::<Option<AbiProductPageStore>>() as u128
                                )
                                .and_then(|n| n.checked_add(request_and_envelope_bytes as u128));
                        let admitted =
                            required.is_some_and(|required| required <= memory_limit_bytes);
                        if !admitted {
                            rejected_host_peak = required;
                        }
                        admitted
                    },
                )
            }
            None => return ABI_ERROR,
        };
        match advanced {
            Ok(advance) => {
                let Some(store) = state.product_page_store.as_ref() else {
                    return ABI_ERROR;
                };
                if !store.governed_store_fits()
                    || !store.governed_request_fits(additional_live_capacity)
                {
                    state.product_page_store = None;
                    return ABI_ERROR;
                }
                let Some(replay) = store.store().pc_replay() else {
                    return ABI_ERROR;
                };
                serialize_pc_replay_page_advance(
                    &advance,
                    replay.source().identity_sha256(),
                    geometry_page_number,
                    member_page_number,
                )
            }
            Err(error) => {
                // A governed page owner is released before the bounded error
                // carrier is allocated. The 4 KiB entry reservation admitted
                // these exact scalar diagnostics; it never admits a partial page.
                if governed || error.required_memory_bytes().is_some() {
                    state.product_page_store = None;
                }
                let required = rejected_host_peak.or(error.required_memory_bytes());
                let maximum = if rejected_host_peak.is_some() {
                    Some(memory_limit_bytes)
                } else {
                    error.max_memory_bytes()
                };
                return publish_pc_replay_error(
                    state,
                    error.code(),
                    required,
                    maximum,
                    memory_limit_bytes,
                    additional_live_capacity,
                );
            }
        }
    } else if let Some(store) = state
        .product_page_store
        .as_ref()
        .and_then(|store| store.store().parity_report())
    {
        match (
            member_page_number,
            parse_canonical_positive_usize(alternative_index_decimal),
        ) {
            (1, Some(outer_page_number)) => match store.page(outer_page_number) {
                Ok(page) => serialize_parity_report_page(&page),
                Err(error) => Err(WasmCommandRuntimeError::new(
                    "E_WASM_PRODUCT_PAGE",
                    error.as_str(),
                )),
            },
            _ => Err(WasmCommandRuntimeError::new(
                "E_WASM_PRODUCT_PAGE",
                "invalid-member-page",
            )),
        }
    } else {
        if !governed {
            state.set_error(
                "E_WASM_PRODUCT_PAGE_KIND",
                "product page kind is unsupported",
            );
        }
        return ABI_ERROR;
    };
    match output {
        Ok(output) => {
            let Some(governed_capacity) = output.capacity().checked_add(additional_live_capacity)
            else {
                state.product_page_store = None;
                return ABI_ERROR;
            };
            if state
                .product_page_store
                .as_ref()
                .is_some_and(|store| !store.governed_output_fits(governed_capacity))
            {
                state.product_page_store = None;
                return ABI_ERROR;
            }
            state.set_output(output);
            ABI_OK
        }
        Err(error) => {
            if governed {
                state.product_page_store = None;
            } else {
                state.set_runtime_error(&error);
            }
            ABI_ERROR
        }
    }
}

fn publish_pc_replay_error(
    state: &mut WasmAbiState,
    code: &str,
    required: Option<u128>,
    maximum: Option<u128>,
    memory_limit_bytes: u128,
    request_capacity: usize,
) -> i32 {
    // Static code + two 39-digit u128 fields + JSON escaping remain far below
    // the admitted reserve. Unknown/oversized diagnostic text is not copied.
    if code.len() > 256
        || !code.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
        || request_capacity
            .checked_add(PC_REPLAY_HOST_ENVELOPE_RESERVE)
            .is_none_or(|n| n as u128 > memory_limit_bytes)
    {
        return ABI_ERROR;
    }
    use core::fmt::Write;
    let mut message = String::with_capacity(512);
    let _ = write!(message, "{code}");
    if let Some(required) = required {
        let _ = write!(message, ": required_memory_bytes={required}");
    }
    if let Some(maximum) = maximum {
        let _ = write!(message, ", max_memory_bytes={maximum}");
    }
    let message_capacity = message.capacity();
    let error = WasmCommandRuntimeError::new("E_WASM_PRODUCT_PAGE", message);
    let output = error.structured_output();
    if output
        .capacity()
        .checked_add(message_capacity)
        .and_then(|n| n.checked_add(core::mem::size_of::<WasmCommandRuntimeError>()))
        .and_then(|n| n.checked_add(core::mem::size_of::<String>()))
        .is_none_or(|n| n > PC_REPLAY_HOST_ENVELOPE_RESERVE)
    {
        return ABI_ERROR;
    }
    state.set_output(output);
    ABI_ERROR
}

fn parse_product_page_request(request: &str) -> Result<(&str, usize, u64), &'static str> {
    let mut fields = request.split('\n');
    let contract = fields.next().ok_or("missing-contract")?;
    let alternative_index_decimal = fields.next().ok_or("missing-alternative-index")?;
    let member_page_decimal = fields.next().ok_or("missing-member-page")?;
    if !is_canonical_positive_decimal(alternative_index_decimal) {
        return Err("invalid-product-page-request");
    }
    let member_page_number =
        parse_canonical_positive_usize(member_page_decimal).ok_or("invalid-member-page")?;
    let maximum_work_steps = match contract {
        PRODUCT_PAGE_REQUEST_CONTRACT => {
            if fields.next().is_some() {
                return Err("invalid-product-page-request");
            }
            DEFAULT_PRODUCT_PAGE_WORK_STEPS
        }
        PRODUCT_PAGE_REQUEST_CONTRACT_V2 => {
            let work_steps_decimal = fields.next().ok_or("missing-work-steps")?;
            if fields.next().is_some() || !is_canonical_positive_decimal(work_steps_decimal) {
                return Err("invalid-product-page-request");
            }
            work_steps_decimal
                .parse::<u64>()
                .ok()
                .filter(|value| *value != 0)
                .ok_or("invalid-work-steps")?
        }
        _ => return Err("invalid-product-page-request"),
    };
    Ok((
        alternative_index_decimal,
        member_page_number,
        maximum_work_steps,
    ))
}

fn parse_canonical_positive_usize(value: &str) -> Option<usize> {
    is_canonical_positive_decimal(value)
        .then(|| value.parse::<usize>().ok())
        .flatten()
}

fn is_canonical_positive_decimal(value: &str) -> bool {
    !value.is_empty()
        && value != "0"
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

#[no_mangle]
pub extern "C" fn clearra_wasm_product_page_release() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_released_output() {
            return status;
        }
        if state.runtime.has_active_finite_job() || state.runtime.has_completed_governed_events() {
            return ABI_ERROR;
        }
        state.product_page_source_owner = None;
        state.product_page_store = None;
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_cancel() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        state.distributed_ready_result = None;
        if let Some(coordinator) = state.distributed_coordinator.as_ref() {
            coordinator.cancel();
        }
        if let Some((_, completion)) = state.distributed_completion.take() {
            completion.cancel();
        }
        state.minimum_parallel_worker = None;
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_reset() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_released_output_and_page_store() {
            return status;
        }
        if state.runtime.has_active_finite_job() {
            return ABI_ERROR;
        }
        state.reset_distributed_state();
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_start() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_drained_verifier_replacement() {
            return status;
        }
        // Preserve this request while releasing a drained exact predecessor's
        // immutable query and real memory lease. Reusing the warm WASM module
        // must not reuse the previous minimum wave's execution authority.
        let input = core::mem::take(&mut state.input);
        state.reset_distributed_state();
        let command_text = match String::from_utf8(input) {
            Ok(command_text) => command_text,
            Err(error) => {
                let utf8_error = error.utf8_error();
                drop(error.into_bytes());
                state.set_error("E_WASM_COMMAND_UTF8", utf8_error);
                return ABI_ERROR;
            }
        };
        let preparation = WasmDistributedVerifierRuntime::prepare(
            state.runtime.command_runtime(),
            command_text.as_str(),
        );
        drop(command_text);
        match preparation {
            Ok(verifier) => {
                state.distributed_verifier = Some(verifier);
                ABI_OK
            }
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_forward_verifier_start() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_drained_verifier_replacement() {
            return status;
        }
        let input = std::mem::take(&mut state.transfer_input);
        state.reset_distributed_state();
        let outcome = WasmDistributedVerifierRuntime::prepare_forward(
            state.runtime.command_runtime(),
            &input,
        );
        drop(input);
        match outcome {
            Ok(verifier) => {
                state.distributed_verifier = Some(verifier);
                ABI_OK
            }
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_consume() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        state.distributed_verifier_last_candidate_count = None;
        let input = std::mem::take(&mut state.transfer_input);
        let input_retained_bytes = input.capacity() as u128;
        let outcome = state
            .distributed_verifier
            .as_mut()
            .ok_or((
                "E_WASM_DISTRIBUTED_STATE",
                "distributed verifier is not active".to_owned(),
            ))
            .and_then(|verifier| {
                verifier
                    .consume_with_external_retained(&input, input_retained_bytes)
                    .map_err(|error| (error.code(), error.message().to_owned()))
            });
        drop(input);
        match outcome {
            Ok(consumed) => {
                state.distributed_verifier_partial_available = consumed.partial.is_some();
                state.distributed_verifier_pending_work = consumed.has_pending_work;
                if let Some(partial) = consumed.partial {
                    state.set_output_bytes(partial);
                }
                state.record_distributed_verifier_candidate_count(consumed.candidate_count)
            }
            Err((code, message)) => {
                state.set_error(code, message);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_partial_available() -> u32 {
    ABI_STATE.with(|state| state.borrow().distributed_verifier_partial_available.into())
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_pending_work() -> u32 {
    ABI_STATE.with(|state| state.borrow().distributed_verifier_pending_work.into())
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_last_candidate_count_available() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_verifier_last_candidate_count
            .is_some()
            .into()
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_last_candidate_count_exact() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_verifier_last_candidate_count
            .is_some_and(|count| count.exact)
            .into()
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_continue() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        state.distributed_verifier_last_candidate_count = None;
        let outcome = state
            .distributed_verifier
            .as_mut()
            .ok_or((
                "E_WASM_DISTRIBUTED_STATE",
                "distributed verifier is not active".to_owned(),
            ))
            .and_then(|verifier| {
                verifier
                    .continue_work()
                    .map_err(|error| (error.code(), error.message().to_owned()))
            });
        match outcome {
            Ok(consumed) => {
                state.distributed_verifier_partial_available = consumed.partial.is_some();
                state.distributed_verifier_pending_work = consumed.has_pending_work;
                if let Some(partial) = consumed.partial {
                    state.set_output_bytes(partial);
                }
                state.record_distributed_verifier_candidate_count(consumed.candidate_count)
            }
            Err((code, message)) => {
                state.set_error(code, message);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_progress_candidate_count() -> u32 {
    AbiU32Count::legacy_or(
        verifier_progress_count(|value| value.progress().candidates),
        0,
    )
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_progress_available() -> u32 {
    ABI_STATE.with(|state| state.borrow().distributed_verifier.is_some().into())
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_progress_candidate_count_exact() -> u32 {
    AbiU32Count::exact_or_false(verifier_progress_count(|value| value.progress().candidates))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_progress_build_nodes() -> u32 {
    AbiU32Count::legacy_or(
        verifier_progress_count(|value| value.progress().build_nodes),
        0,
    )
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_progress_build_nodes_exact() -> u32 {
    AbiU32Count::exact_or_false(verifier_progress_count(|value| {
        value.progress().build_nodes
    }))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_progress_coverage_checks() -> u32 {
    AbiU32Count::legacy_or(
        verifier_progress_count(|value| value.progress().coverage_checks),
        0,
    )
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_progress_coverage_checks_exact() -> u32 {
    AbiU32Count::exact_or_false(verifier_progress_count(|value| {
        value.progress().coverage_checks
    }))
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_finish() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let Some(verifier) = state.distributed_verifier.as_mut() else {
            state.set_error(
                "E_WASM_DISTRIBUTED_STATE",
                "distributed verifier is not active",
            );
            return ABI_ERROR;
        };
        match verifier.finish() {
            Ok(output) => {
                state.set_output_bytes(output);
                state.distributed_verifier = None;
                state.distributed_verifier_partial_available = false;
                state.distributed_verifier_pending_work = false;
                state.distributed_verifier_last_candidate_count = None;
                ABI_OK
            }
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

fn u128_word(value: u128, word_index: u32) -> u32 {
    if word_index >= 4 {
        return 0;
    }
    (value >> (word_index * 32)) as u32
}

#[no_mangle]
pub extern "C" fn clearra_wasm_start_job() -> u32 {
    if ABI_STATE.with(|state| {
        let state = state.borrow();
        state.require_mutation_admission().is_err() || state.has_worker_job_start_conflict()
    }) {
        return 0;
    }
    clear_panic_diagnostics();
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        debug_assert!(state.require_mutation_admission().is_ok());
        debug_assert!(!state.has_worker_job_start_conflict());
        // Legacy page stores are not governed and retain the historical
        // replacement behavior. A governed page-store lease was rejected by
        // the admission check above and can only be explicitly released.
        state.tiling_solution_page_store = None;
        state.product_page_source_owner = None;
        state.product_page_store = None;
        let command_text = match String::from_utf8(std::mem::take(&mut state.input)) {
            Ok(command_text) => command_text,
            Err(error) => {
                let utf8_error = error.utf8_error();
                drop(error.into_bytes());
                state.set_error("E_WASM_COMMAND_UTF8", utf8_error);
                return 0;
            }
        };
        let started = state.runtime.start_job(&command_text);
        drop(command_text);
        match started {
            Ok(job_id) => match u32::try_from(job_id.get()) {
                Ok(job_id) => job_id,
                Err(error) => {
                    state.set_error("E_WASM_JOB_ID_EXHAUSTED", error);
                    0
                }
            },
            Err(error) => {
                state.set_error(error.code(), error.message());
                0
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_advance_job(job_id: u32, work_budget: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_worker_lifecycle_admission() {
            return status;
        }
        if state.has_worker_advance_conflict() {
            return ABI_ERROR;
        }
        if state.runtime.has_completed_governed_events() {
            return ABI_ERROR;
        }
        let status = match state
            .runtime
            .advance_job(WasmWorkerJobId::new(job_id.into()), work_budget.max(1))
        {
            Ok(status) => status,
            Err(error) => {
                if !state.runtime.has_completed_governed_events() {
                    state.set_error(error.code(), error.message());
                }
                return ABI_ERROR;
            }
        };
        if status == WasmWorkerAdvanceStatus::Completed {
            let legacy_page_store = state.runtime.take_completed_tiling_solution_page_store();
            let product_page_source_owner =
                state.runtime.take_completed_product_page_source_owner();
            if state.runtime.has_completed_governed_events() {
                debug_assert!(legacy_page_store.is_none());
                debug_assert!(product_page_source_owner.is_none());
                drop(legacy_page_store);
                drop(product_page_source_owner);
                state.tiling_solution_page_store = None;
                state.product_page_source_owner = None;
                state.product_page_store = None;
            } else {
                state.tiling_solution_page_store =
                    legacy_page_store.map(AbiTilingSolutionPageStore::legacy);
                state.product_page_source_owner = product_page_source_owner;
                state.product_page_store = None;
            }
        }
        match status {
            WasmWorkerAdvanceStatus::Pending => 0,
            WasmWorkerAdvanceStatus::Completed => 1,
            WasmWorkerAdvanceStatus::Cancelled => 2,
            WasmWorkerAdvanceStatus::Failed => 3,
            WasmWorkerAdvanceStatus::Progress => 4,
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_cancel_job(job_id: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_worker_lifecycle_admission() {
            return status;
        }
        if state.runtime.has_completed_governed_events() {
            return ABI_ERROR;
        }
        match state
            .runtime
            .cancel_job(WasmWorkerJobId::new(job_id.into()))
        {
            Ok(()) => {
                state.product_page_source_owner = None;
                state.product_page_store = None;
                ABI_OK
            }
            Err(error) => {
                state.set_error(error.code(), error.message());
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_drain_job_events(job_id: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_released_output_and_page_store() {
            return status;
        }
        // A finite job that has not produced its terminal governed batch may
        // only be advanced or cancelled. Never reinterpret its unavailable
        // governed drain as the allocation-heavy legacy event path.
        if state.runtime.has_active_finite_job() {
            return ABI_ERROR;
        }
        let job_id = WasmWorkerJobId::new(job_id.into());
        if state.runtime.has_completed_governed_events() {
            // Prove every dynamic ABI storage precondition before the runtime
            // consumes the batch. The serializer's finite authority includes
            // the target `Option<GovernedWasmJson>` carrier, so this same
            // mutable borrow can commit the certified output without a second
            // fallible boundary.
            let admission = match state.require_governed_output_admission() {
                Ok(admission) => admission,
                Err(status) => return status,
            };
            return match state.runtime.drain_governed_events_json(job_id) {
                Ok(output) => {
                    state.store_governed_output(admission, output);
                    ABI_OK
                }
                Err(_) => ABI_ERROR,
            };
        }
        match state.runtime.drain_events_json(job_id) {
            Ok(output) => {
                state.set_output(output);
                ABI_OK
            }
            Err(error) => {
                if !state.runtime.has_completed_governed_events() {
                    state.set_error(error.code(), error.message());
                }
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_output_ptr() -> u32 {
    ABI_STATE.with(|state| state.borrow().output_bytes().as_ptr() as usize as u32)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_output_len() -> u32 {
    ABI_STATE.with(|state| state.borrow().output_bytes().len().min(u32::MAX as usize) as u32)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_output_len_exact() -> u32 {
    ABI_STATE.with(|state| {
        u32::try_from(state.borrow().output_bytes().len())
            .is_ok()
            .into()
    })
}

/// Releases the ABI-owned output allocation after the host has copied it.
///
/// `clearra_wasm_output_ptr` is invalid after this call. Hosts must release an
/// output before starting the next operation so a candidate/partial wire does
/// not coexist with the next producer, verifier, or merger authority. Until
/// release, `i32`-returning mutating ABI entrypoints fail closed with status
/// `-2` and leave the existing pointer, length, capacity, and owner unchanged.
/// The legacy `u32`-returning `clearra_wasm_start_job` export instead returns
/// its reserved failure value `0` while preserving the same lease invariants.
/// A governed output moves its tiling or product page owner and finite memory
/// authority into the ABI when the JSON lease is released. Until the matching
/// page release export is called, unrelated mutation fails. Product pages are
/// serialized only when the live store plus page output remain within that
/// authority. If a transfer or later growth cannot be validated, the output
/// pointer is still invalidated and the page owner is dropped fail-closed.
#[no_mangle]
pub extern "C" fn clearra_wasm_output_release() -> i32 {
    ABI_STATE.with(|state| {
        let (_, page_store_transfer_failed) = state.borrow_mut().release_output();
        if page_store_transfer_failed {
            ABI_ERROR
        } else {
            ABI_OK
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_last_panic_ptr() -> u32 {
    LAST_PANIC.with(|last| last.borrow().as_ptr() as usize as u32)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_last_panic_len() -> u32 {
    LAST_PANIC.with(|last| last.borrow().len().min(u32::MAX as usize) as u32)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_last_panic_len_exact() -> u32 {
    LAST_PANIC.with(|last| u32::try_from(last.borrow().len()).is_ok().into())
}

#[cfg(feature = "stage-profiling")]
#[no_mangle]
pub extern "C" fn clearra_wasm_profile_start() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if state.profile.is_some() {
            state.set_error(
                "E_WASM_PROFILE_ALREADY_ACTIVE",
                "a search profile is already active",
            );
            return ABI_ERROR;
        }
        match ExecutorSearchProfileSession::start() {
            Ok(profile) => {
                state.profile = Some(profile);
                ABI_OK
            }
            Err(_) => {
                state.set_error(
                    "E_WASM_PROFILE_ALREADY_ACTIVE",
                    "a search profile is already active",
                );
                ABI_ERROR
            }
        }
    })
}

#[cfg(feature = "stage-profiling")]
#[no_mangle]
pub extern "C" fn clearra_wasm_profile_finish() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let Some(profile) = state.profile.take() else {
            state.set_error("E_WASM_PROFILE_NOT_ACTIVE", "no search profile is active");
            return ABI_ERROR;
        };
        let stages = profile
            .finish()
            .into_iter()
            .map(|stage| {
                serde_json::json!({
                    "name": stage.name,
                    "duration_ns": stage.duration_ns,
                    "invocation_count": stage.invocation_count,
                    "work_item_count": stage.work_item_count,
                })
            })
            .collect::<Vec<_>>();
        match serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "stages": stages,
        })) {
            Ok(output) => {
                state.set_output(output);
                ABI_OK
            }
            Err(error) => {
                state.set_error("E_WASM_PROFILE_SERIALIZE_FAILED", error);
                ABI_ERROR
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;

    const FINITE_BUILD_COMMAND: &str = "clearra build-probability --base-mask 0x0 \
        --target-mask 0xf --height 4 --queue I --no-hold --no-mirror \
        --workers 1 --max-memory-mib 64";
    const FINITE_GENERIC_TILING_COMMAND: &str = "clearra build-probability --base-mask 0x0 \
        --target-mask 0xf --height 4 --queue I --no-hold --no-mirror \
        --tiling-only --workers 1 --max-memory-mib 64";
    const TYPED_PC_TILING_COMMAND: &str = "clearra pc tiling --board-mask 0xf83e0f83e0 \
        --height 4 --pieces 5 --lines 4 --patterns P5 --no-hold --backend cpu --workers 1";
    const TYPED_PC_MINIMALS_COMMAND: &str = "clearra pc minimals --lines 1 \
        --board-mask 0x3f --height 1 --pieces 1 --queue I --hold empty --rule srs-plus \
        --backend cpu --workers 1";
    const MULTI_ALTERNATIVE_PC_MINIMALS_COMMAND: &str = "clearra pc minimals --lines 2 \
        --queue IIOOO --backend cpu --workers 1";
    const TYPED_PARITY_COMMAND: &str = "clearra utility parity --format ctk3 \
        --document ctk3_w0kCERPPgGduYXRpdmWycg";

    fn typed_pc_tiling_page_store_fixture() -> Arc<TilingSolutionPageStore> {
        static STORE: OnceLock<Arc<TilingSolutionPageStore>> = OnceLock::new();
        Arc::clone(STORE.get_or_init(|| {
            let execution = clearra_wasm::WasmCommandRuntime::default()
                .run_command_text(TYPED_PC_TILING_COMMAND)
                .expect("pageable typed pc tiling fixture");
            Arc::clone(
                execution
                    .tiling_solution_page_store()
                    .expect("typed family has a public continuation page"),
            )
        }))
    }

    fn typed_pc_minimals_product_source_fixture() -> ProductPageSourceOwner {
        static SOURCE: OnceLock<ProductPageSourceOwner> = OnceLock::new();
        SOURCE
            .get_or_init(|| {
                let execution = clearra_wasm::WasmCommandRuntime::default()
                    .run_command_text(TYPED_PC_MINIMALS_COMMAND)
                    .expect("pageable typed pc minimals fixture");
                execution
                    .product_page_source_owner()
                    .cloned()
                    .unwrap_or_else(|| {
                        panic!(
                            "typed pc minimals has no public product page source: {:?}",
                            execution.app_response()
                        )
                    })
            })
            .clone()
    }

    fn multi_alternative_pc_minimals_product_source_fixture() -> ProductPageSourceOwner {
        static SOURCE: OnceLock<ProductPageSourceOwner> = OnceLock::new();
        SOURCE
            .get_or_init(|| {
                let execution = clearra_wasm::WasmCommandRuntime::default()
                    .run_command_text(MULTI_ALTERNATIVE_PC_MINIMALS_COMMAND)
                    .expect("multi-alternative typed pc minimals fixture");
                execution
                    .product_page_source_owner()
                    .cloned()
                    .expect("multi-alternative pc minimals has a public product page source")
            })
            .clone()
    }

    fn reset_abi_state_for_test() {
        ABI_STATE.with(|state| *state.borrow_mut() = WasmAbiState::default());
    }

    #[test]
    fn product_retention_configuration_is_bounded_and_cannot_mutate_leased_output() {
        reset_abi_state_for_test();
        assert_eq!(clearra_wasm_configure_product_retention(0), ABI_ERROR);
        assert_eq!(
            clearra_wasm_configure_product_retention(u32::MAX),
            ABI_ERROR
        );
        assert_eq!(
            clearra_wasm_configure_product_retention(64 * 1024 * 1024),
            ABI_OK
        );
        ABI_STATE.with(|state| state.borrow_mut().set_output_bytes(vec![1]));
        assert_eq!(
            clearra_wasm_configure_product_retention(128 * 1024 * 1024),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        reset_abi_state_for_test();
    }

    #[test]
    fn typed_pc_minimals_runtime_completes_on_two_mib_stack() {
        std::thread::Builder::new()
            .name("typed-pc-minimals-two-mib-stack".to_owned())
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let execution = clearra_wasm::WasmCommandRuntime::default()
                    .run_command_text(TYPED_PC_MINIMALS_COMMAND)
                    .expect("typed pc minimals completes within the explicit stack boundary");
                assert!(execution.product_page_source_owner().is_some());
            })
            .expect("two-MiB stack test thread starts")
            .join()
            .expect("typed pc minimals does not overflow or panic");
    }

    #[test]
    fn distributed_ready_preparation_is_finished_once_without_serial_reexecution() {
        const COMMAND: &str = "clearra pc --lines 4 --board-mask 0 --height 4 \
            --pieces 10 --queue IOTSZJL --hold empty --backend cpu --workers 2";
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().input = COMMAND.as_bytes().to_vec();
        });

        assert_eq!(
            clearra_wasm_distributed_prepare(),
            3,
            "an App-terminal preparation needs a distinct ABI mode"
        );
        ABI_STATE.with(|state| {
            state.borrow_mut().input = COMMAND.as_bytes().to_vec();
        });
        assert_eq!(
            clearra_wasm_start_job(),
            0,
            "the retained App result must block a serial replay owner"
        );
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.distributed_ready_result.is_some());
            assert!(!state.input.is_empty(), "rejected replay input stays owned");
        });
        assert_eq!(clearra_wasm_distributed_finish(71, 0), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let output = std::str::from_utf8(state.output_bytes()).expect("terminal UTF-8");
            assert!(output.contains("\"event\":\"final_response\""), "{output}");
            assert!(output.contains("\"job_id\":71"), "{output}");
            assert!(output.contains("piece window cannot exceed"), "{output}");
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);

        assert_eq!(
            clearra_wasm_distributed_finish(72, 0),
            ABI_ERROR,
            "the prepared App response must have one terminal owner"
        );
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    fn completed_minimum_source_for_test() -> WasmDistributedCoordinator {
        const COMMAND: &str = "clearra pc minimals --lines 4 --board-mask 0xfc3f --height 4 \
            --pieces 7 --patterns IOOOOOO;OOOOOOO --no-hold --backend cpu --workers 2";
        completed_source_for_test(COMMAND)
    }

    fn completed_source_for_test(command: &str) -> WasmDistributedCoordinator {
        let runtime = clearra_wasm::WasmCommandRuntime::default()
            .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
        let mut coordinator = match WasmDistributedCoordinator::prepare(&runtime, command).unwrap()
        {
            WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
            _ => panic!("fixture must create a distributed coordinator"),
        };
        let mut verifier = coordinator
            .prepare_in_process_verifier(&runtime, command)
            .unwrap();
        loop {
            match coordinator.advance_producer(16_384, 16).unwrap() {
                WasmDistributedProducerAdvance::Pending
                | WasmDistributedProducerAdvance::Initialization(_) => {}
                WasmDistributedProducerAdvance::Batch(batch) => {
                    let mut consumed = verifier.consume(&batch).unwrap();
                    loop {
                        if let Some(partial) = consumed.partial.take() {
                            coordinator.absorb_partial(&partial).unwrap();
                        }
                        if !consumed.has_pending_work {
                            break;
                        }
                        consumed = verifier.continue_work().unwrap();
                    }
                }
                WasmDistributedProducerAdvance::Completed => break,
                WasmDistributedProducerAdvance::Cancelled => {
                    panic!("unexpected source cancellation")
                }
            }
        }
        let partial = verifier.finish().unwrap();
        if !partial.is_empty() {
            coordinator.absorb_partial(&partial).unwrap();
        }
        drop(verifier);
        coordinator
    }

    fn completed_replay_source_for_test() -> ProductPageSourceOwner {
        let coordinator = completed_source_for_test(
            "clearra pc path --lines 2 --board-mask 0 --height 2 --pieces 5 \
             --queue IIOOO --no-hold --backend cpu --workers 2",
        );
        let mut completion = coordinator.into_cooperative_completion(2).unwrap();
        for _ in 0..10_000 {
            match completion.advance(64).unwrap() {
                WasmDistributedCompletionAdvance::Pending => {}
                WasmDistributedCompletionAdvance::Completed(result) => {
                    return result
                        .product_page_source_owner()
                        .cloned()
                        .expect("exact replay source");
                }
                WasmDistributedCompletionAdvance::Cancelled => panic!("uncancelled replay fixture"),
            }
        }
        panic!("tiny replay source exceeded its test-only work bound");
    }

    fn minimum_query_and_task_for_replacement_test() -> (Vec<u8>, Vec<u8>) {
        let mut completion = completed_minimum_source_for_test()
            .into_cooperative_completion(2)
            .unwrap();
        for _ in 0..1_000 {
            if let Some(query) = completion.prepare_parallel(4).unwrap() {
                let task = completion
                    .take_parallel_task()
                    .unwrap()
                    .expect("source-issued exact task");
                // Separate WASM instances have independent authorities. Do
                // not retain this native coordinator beside the remote lease.
                drop(completion);
                return (query, task);
            }
            assert!(matches!(
                completion.advance(8).unwrap(),
                WasmDistributedCompletionAdvance::Pending
            ));
        }
        panic!("tiny exact source did not publish a query");
    }

    fn replacement_test_output() -> String {
        ABI_STATE.with(|state| String::from_utf8_lossy(state.borrow().output_bytes()).into_owned())
    }

    #[test]
    fn geometry_replacement_missing_initial_field_reports_command_error_and_allows_corrected_retry()
    {
        reset_abi_state_for_test();
        assert_eq!(clearra_wasm_configure_host(4, 0), ABI_OK);
        // --pieces selects the scenario grammar: the empty field and its
        // height must still be explicit. Parsing must fail before a verifier
        // or its execution authority is acquired.
        const INVALID: &str = "clearra pc --lines 2 --pieces 5 --queue IIOOO \
            --no-hold --backend cpu --workers 2";
        ABI_STATE.with(|state| state.borrow_mut().input = INVALID.as_bytes().to_vec());
        assert_eq!(clearra_wasm_distributed_verifier_start(), ABI_ERROR);
        let error_output = replacement_test_output();
        assert!(
            error_output.contains("E_WASM_COMMAND_MISSING_VALUE"),
            "{error_output}"
        );
        assert!(
            error_output.contains("scenario PC requires --board-mask"),
            "{error_output}"
        );
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.distributed_verifier.is_none());
            assert!(state.minimum_parallel_worker.is_none());
            assert!(state.output_outstanding);
        });
        const CORRECTED: &str = "clearra pc --lines 2 --board-mask 0 --height 2 \
            --pieces 5 --queue IIOOO --no-hold --backend cpu --workers 2";
        ABI_STATE.with(|state| state.borrow_mut().input = CORRECTED.as_bytes().to_vec());
        assert_eq!(
            clearra_wasm_distributed_verifier_start(),
            ABI_OUTPUT_NOT_RELEASED
        );
        ABI_STATE.with(|state| assert_eq!(state.borrow().input, CORRECTED.as_bytes()));
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(
            clearra_wasm_distributed_verifier_start(),
            ABI_OK,
            "{}",
            replacement_test_output()
        );
        assert_eq!(clearra_wasm_distributed_verifier_finish(), ABI_OK);
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        reset_abi_state_for_test();
    }

    #[test]
    fn warm_minimum_to_geometry_releases_authority_for_same_and_larger_worker_policies() {
        reset_abi_state_for_test();
        let (query, task) = minimum_query_and_task_for_replacement_test();
        for (workers, all_threads) in [(11, false), (12, true)] {
            reset_abi_state_for_test();
            assert_eq!(clearra_wasm_configure_host(12, 0), ABI_OK);
            let initial = "clearra pc --lines 2 --board-mask 0 --height 2 \
                --pieces 5 --queue IIOOO --no-hold --backend cpu --workers 11";
            ABI_STATE.with(|state| state.borrow_mut().input = initial.as_bytes().to_vec());
            assert_eq!(
                clearra_wasm_distributed_verifier_start(),
                ABI_OK,
                "{}",
                replacement_test_output()
            );
            assert_eq!(clearra_wasm_distributed_verifier_finish(), ABI_OK);
            assert_eq!(clearra_wasm_output_release(), ABI_OK);

            ABI_STATE.with(|state| state.borrow_mut().transfer_input = query.clone());
            assert_eq!(
                clearra_wasm_distributed_finish_parallel_worker_init(),
                ABI_OK
            );
            ABI_STATE.with(|state| state.borrow_mut().transfer_input = task.clone());
            assert_eq!(
                clearra_wasm_distributed_finish_parallel_worker_start(),
                ABI_OK
            );
            let mut drained = false;
            for _ in 0..1_000 {
                match clearra_wasm_distributed_finish_parallel_worker_advance(64) {
                    1 => {}
                    ABI_OK => {
                        drained = true;
                        break;
                    }
                    status => panic!("tiny exact task failed: {status}"),
                }
            }
            assert!(drained);
            let next = format!(
                "clearra pc minimals --lines 2 --board-mask 0 --height 2 \
                 --pieces 5 --queue IIOOO --no-hold --backend cpu --workers {workers} {}",
                if all_threads {
                    "--use-all-cpu-threads"
                } else {
                    ""
                }
            );
            ABI_STATE.with(|state| state.borrow_mut().input = next.as_bytes().to_vec());
            assert_eq!(
                clearra_wasm_distributed_verifier_start(),
                ABI_OUTPUT_NOT_RELEASED
            );
            assert_eq!(clearra_wasm_output_release(), ABI_OK);
            assert_eq!(
                clearra_wasm_distributed_verifier_start(),
                ABI_OK,
                "{}",
                replacement_test_output()
            );
            ABI_STATE.with(|state| {
                let state = state.borrow();
                assert!(state.minimum_parallel_worker.is_none());
                assert!(state.distributed_verifier.is_some());
                assert!(state.input.is_empty());
            });
            assert_eq!(clearra_wasm_distributed_verifier_finish(), ABI_OK);
            assert_eq!(clearra_wasm_output_release(), ABI_OK);
        }
        reset_abi_state_for_test();
    }

    #[test]
    fn geometry_replacement_rejects_unfinished_verifier_and_exact_shard_without_losing_inputs() {
        reset_abi_state_for_test();
        assert_eq!(clearra_wasm_configure_host(4, 0), ABI_OK);
        let (query, task) = minimum_query_and_task_for_replacement_test();
        ABI_STATE.with(|state| state.borrow_mut().transfer_input = query);
        assert_eq!(
            clearra_wasm_distributed_finish_parallel_worker_init(),
            ABI_OK
        );
        ABI_STATE.with(|state| state.borrow_mut().transfer_input = task);
        assert_eq!(
            clearra_wasm_distributed_finish_parallel_worker_start(),
            ABI_OK
        );
        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.input = b"must not be parsed or discarded".to_vec();
            state.transfer_input = vec![0xff];
        });
        assert_eq!(clearra_wasm_distributed_verifier_start(), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_forward_verifier_start(), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.input, b"must not be parsed or discarded");
            assert_eq!(state.transfer_input, [0xff]);
            assert!(
                state
                    .minimum_parallel_worker
                    .as_ref()
                    .unwrap()
                    .has_active_shard()
            );
            assert!(!state.output_outstanding);
        });
        assert_eq!(
            clearra_wasm_distributed_finish_parallel_worker_cancel(),
            ABI_OK
        );
        assert_eq!(
            clearra_wasm_distributed_forward_verifier_start(),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        // Invalid next input still releases the authorized drained predecessor;
        // it must not leave a hidden exact lease after decode fails.
        assert_eq!(clearra_wasm_distributed_forward_verifier_start(), ABI_ERROR);
        ABI_STATE.with(|state| assert!(state.borrow().minimum_parallel_worker.is_none()));
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        ABI_STATE.with(|state| {
            state.borrow_mut().input = b"clearra pc --lines 2 --board-mask 0 --height 2 \
                --pieces 5 --queue IIOOO --no-hold --backend cpu --workers 2"
                .to_vec();
        });
        assert_eq!(
            clearra_wasm_distributed_verifier_start(),
            ABI_OK,
            "{}",
            replacement_test_output()
        );
        ABI_STATE.with(|state| state.borrow_mut().input = b"replacement".to_vec());
        assert_eq!(clearra_wasm_distributed_verifier_start(), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.distributed_verifier.is_some());
            assert_eq!(state.input, b"replacement");
        });
        assert_eq!(clearra_wasm_distributed_verifier_finish(), ABI_OK);
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
        reset_abi_state_for_test();
    }

    #[test]
    fn exact_replacement_requires_geometry_finish_and_drained_output() {
        reset_abi_state_for_test();
        assert_eq!(clearra_wasm_configure_host(4, 0), ABI_OK);
        let (query, task) = minimum_query_and_task_for_replacement_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().input = b"clearra pc --lines 2 --board-mask 0 --height 2 \
                --pieces 5 --queue IIOOO --no-hold --backend cpu --workers 2"
                .to_vec();
        });
        assert_eq!(
            clearra_wasm_distributed_verifier_start(),
            ABI_OK,
            "{}",
            replacement_test_output()
        );
        ABI_STATE.with(|state| state.borrow_mut().transfer_input = query.clone());
        assert_eq!(
            clearra_wasm_distributed_finish_parallel_worker_init(),
            ABI_ERROR
        );
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.distributed_verifier.is_some());
            assert!(state.minimum_parallel_worker.is_none());
            assert_eq!(state.transfer_input, query);
            assert!(!state.output_outstanding);
        });
        assert_eq!(clearra_wasm_distributed_verifier_finish(), ABI_OK);
        assert_eq!(
            clearra_wasm_distributed_finish_parallel_worker_init(),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(
            clearra_wasm_distributed_finish_parallel_worker_init(),
            ABI_OK
        );
        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            assert!(state.distributed_verifier.is_none());
            assert!(state.minimum_parallel_worker.is_some());
            state.transfer_input = task;
        });
        assert_eq!(
            clearra_wasm_distributed_finish_parallel_worker_start(),
            ABI_OK
        );
        assert_eq!(
            clearra_wasm_distributed_finish_parallel_worker_cancel(),
            ABI_OK
        );
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
        reset_abi_state_for_test();
    }

    #[test]
    fn pc_replay_page_abi_yields_and_preserves_request_fencing_output_lease_and_release() {
        reset_abi_state_for_test();
        let source = completed_replay_source_for_test();
        let ProductPageSourceOwner::PcReplay(replay) = &source else {
            panic!("replay source");
        };
        assert!(
            replay.geometry_count() >= 2,
            "fixture exercises a non-prefetched geometry"
        );
        ABI_STATE.with(|state| {
            state.borrow_mut().product_page_store = Some(AbiProductPageStore::legacy(
                ProductPageStore::from_source(source).unwrap(),
            ));
        });
        let mut pending = false;
        let mut complete = false;
        for _ in 0..10_000 {
            let status = ABI_STATE
                .with(|state| product_page_get_from_state(&mut state.borrow_mut(), "2", 1, 1, 0));
            assert_eq!(status, ABI_OK);
            let output = ABI_STATE
                .with(|state| String::from_utf8(state.borrow().output_bytes().to_vec()).unwrap());
            assert_eq!(clearra_wasm_product_page_get(2, 1), ABI_OUTPUT_NOT_RELEASED);
            assert_eq!(clearra_wasm_output_release(), ABI_OK);
            if output.contains("\"state\":\"pending\"") {
                pending = true;
                assert!(!output.contains("\"witnesses\""));
                assert!(output.contains("\"page_source_identity_sha256\""));
                assert!(output.contains("\"geometry_page_number\":\"2\""));
                assert!(output.contains("\"member_page_number\":\"1\""));
                assert_eq!(
                    clearra_wasm_product_page_get(1, 1),
                    ABI_ERROR,
                    "a pending request cannot change its geometry"
                );
                assert_eq!(clearra_wasm_output_release(), ABI_OK);
            } else {
                assert!(output.contains("\"state\":\"page\""), "{output}");
                assert!(output.contains("\"witnesses\""));
                complete = true;
                break;
            }
        }
        assert!(
            pending && complete,
            "cache-miss traversal yields before exact publication"
        );
        assert_eq!(clearra_wasm_product_page_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_get(1, 1), ABI_ERROR);
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        reset_abi_state_for_test();
    }

    #[test]
    fn pc_replay_error_diagnostics_require_a_reserved_bounded_carrier_and_preserve_u128_bytes() {
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            assert_eq!(
                publish_pc_replay_error(
                    &mut state,
                    "complete_replay_host_memory_limit_exceeded",
                    Some(u128::MAX),
                    Some(64 * 1024 * 1024),
                    (PC_REPLAY_HOST_ENVELOPE_RESERVE - 1) as u128,
                    0,
                ),
                ABI_ERROR
            );
            assert!(!state.output_outstanding);
            assert!(state.output.is_empty());
            assert_eq!(
                publish_pc_replay_error(
                    &mut state,
                    "complete_replay_host_memory_limit_exceeded",
                    Some(u128::MAX),
                    Some(64 * 1024 * 1024),
                    PC_REPLAY_HOST_ENVELOPE_RESERVE as u128,
                    0,
                ),
                ABI_ERROR
            );
            let output = std::str::from_utf8(state.output_bytes()).unwrap();
            assert!(output.contains(&format!("required_memory_bytes={}", u128::MAX)));
            assert!(output.contains("max_memory_bytes=67108864"));
            assert!(output.contains("E_WASM_PRODUCT_PAGE"));
            assert!(!output.contains("\"page\""));
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        reset_abi_state_for_test();
    }

    #[test]
    fn staged_distributed_minimum_binds_job_and_yields_without_an_output_lease() {
        reset_abi_state_for_test();
        let coordinator = completed_minimum_source_for_test();
        ABI_STATE.with(|state| state.borrow_mut().distributed_coordinator = Some(coordinator));
        assert_eq!(clearra_wasm_distributed_finish_start(31, 2), 1);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(!state.output_outstanding);
            assert!(state.has_worker_job_start_conflict());
            assert!(state.product_page_source_owner.is_none());
        });
        assert_eq!(clearra_wasm_distributed_finish_advance(31, 0), 1);
        assert_eq!(clearra_wasm_distributed_finish_advance(32, 1), ABI_ERROR);
        ABI_STATE.with(|state| assert!(state.borrow().distributed_completion.is_some()));
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_finish_advance(31, 1), 1);
        let mut completed = false;
        for _ in 0..1_000 {
            match clearra_wasm_distributed_finish_advance(31, 64) {
                1 => {}
                ABI_OK => {
                    completed = true;
                    break;
                }
                status => panic!("staged completion failed: {status}"),
            }
        }
        assert!(completed);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let output = std::str::from_utf8(state.output_bytes()).unwrap();
            assert!(output.contains("\"event\":\"final_response\""));
            assert!(output.contains("\"job_id\":31"));
            assert!(state.distributed_completion.is_none());
            assert!(state.product_page_source_owner.is_some());
        });
        assert_eq!(
            clearra_wasm_distributed_finish_advance(31, 1),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn coordinator_minimum_shards_share_live_completion_and_keep_job_fencing() {
        reset_abi_state_for_test();
        let coordinator = completed_minimum_source_for_test();
        ABI_STATE.with(|state| state.borrow_mut().distributed_coordinator = Some(coordinator));
        assert_eq!(clearra_wasm_distributed_finish_start(31, 2), 1);
        assert_eq!(
            clearra_wasm_distributed_finish_parallel_local_start(32),
            ABI_ERROR
        );
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        let mut local_tasks = 0;
        let mut completed = false;
        for _ in 0..1_000 {
            match clearra_wasm_distributed_finish_parallel_prepare(31, 8) {
                0 => {}
                1 => {
                    assert_eq!(clearra_wasm_output_release(), ABI_OK);
                    while clearra_wasm_distributed_finish_parallel_local_start(31) == 1 {
                        local_tasks += 1;
                        assert_eq!(
                            clearra_wasm_distributed_finish_parallel_local_advance(31, 0),
                            0
                        );
                        assert_eq!(
                            clearra_wasm_distributed_finish_advance(31, 8),
                            1,
                            "normal continuation cannot discard an issued coordinator shard"
                        );
                        ABI_STATE.with(|state| {
                            let state = state.borrow();
                            assert!(state.distributed_completion.is_some());
                            assert!(
                                state.minimum_parallel_worker.is_none(),
                                "local work must not replace completion with a verifier owner"
                            );
                            assert!(!state.output_outstanding);
                        });
                        let mut settled = false;
                        for _ in 0..1_000 {
                            match clearra_wasm_distributed_finish_parallel_local_advance(31, 8) {
                                0 => {}
                                1 => {
                                    settled = true;
                                    break;
                                }
                                status => panic!("coordinator shard failed: {status}"),
                            }
                        }
                        assert!(settled);
                    }
                }
                status => panic!("parallel query failed: {status}"),
            }
            match clearra_wasm_distributed_finish_advance(31, 8) {
                1 => {}
                0 => {
                    completed = true;
                    break;
                }
                status => panic!("coordinator completion failed: {status}"),
            }
        }
        assert!(completed && local_tasks > 0);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let output = std::str::from_utf8(state.output_bytes()).unwrap();
            assert!(output.contains("\"event\":\"final_response\""));
            assert!(state.distributed_completion.is_none());
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn guarded_minimum_topology_is_source_bound_and_full_cpu_keeps_shared_shape() {
        for (host_compute, dedicated) in [(3, true), (2, false)] {
            reset_abi_state_for_test();
            let coordinator = completed_minimum_source_for_test();
            ABI_STATE.with(|state| state.borrow_mut().distributed_coordinator = Some(coordinator));
            assert_eq!(clearra_wasm_distributed_finish_start(31, 2), 1);
            assert_eq!(clearra_wasm_distributed_finish_parallel_guard_version(), 1);
            assert_eq!(
                clearra_wasm_distributed_finish_parallel_configure(32, host_compute, 1 << 30, 0),
                ABI_ERROR
            );
            assert_eq!(clearra_wasm_output_release(), ABI_OK);
            assert_eq!(
                clearra_wasm_distributed_finish_parallel_configure(31, host_compute, 1 << 30, 0),
                1
            );
            let mut query_ready = false;
            for _ in 0..1_000 {
                match clearra_wasm_distributed_finish_parallel_prepare(31, 8) {
                    0 => assert_eq!(clearra_wasm_distributed_finish_advance(31, 8), 1),
                    1 => {
                        assert_eq!(clearra_wasm_output_release(), ABI_OK);
                        query_ready = true;
                        break;
                    }
                    status => panic!("guarded query preparation failed: {status}"),
                }
            }
            assert!(
                query_ready,
                "initial deferred frontier remains protected and reachable"
            );
            if !dedicated {
                assert_eq!(
                    clearra_wasm_distributed_finish_parallel_admit(
                        31,
                        2,
                        1,
                        host_compute,
                        1 << 30,
                        0
                    ),
                    0,
                    "full compute cannot add a control-only instance"
                );
            }
            assert_eq!(
                clearra_wasm_distributed_finish_parallel_admit(
                    31,
                    if dedicated { 2 } else { 1 },
                    u32::from(dedicated),
                    host_compute,
                    1 << 30,
                    0
                ),
                1
            );
            assert_eq!(
                clearra_wasm_distributed_finish_parallel_guarded_query(31),
                ABI_OK
            );
            let guarded_query = ABI_STATE.with(|state| state.borrow().output_bytes().to_vec());
            assert_eq!(
                u32::from_le_bytes(guarded_query[8..12].try_into().unwrap()),
                4,
                "source identity and exact remote slice share a guarded packet"
            );
            assert_eq!(clearra_wasm_output_release(), ABI_OK);
            assert!(
                WasmMinimumParallelWorker::initialize(&guarded_query).is_err(),
                "native process shares a real authority: manager lease remains held without a local cursor"
            );
            assert_eq!(
                clearra_wasm_distributed_finish_parallel_local_start(31),
                i32::from(!dedicated)
            );
            assert_eq!(
                clearra_wasm_distributed_finish_parallel_admit(31, 1, 0, host_compute, 1 << 30, 0),
                ABI_ERROR,
                "fixed slices cannot be changed after admission or task issuance"
            );
            assert_eq!(clearra_wasm_output_release(), ABI_OK);
            assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
        }
        reset_abi_state_for_test();
    }

    #[test]
    fn coordinator_admission_decline_returns_issued_task_to_exact_remote_frontier() {
        reset_abi_state_for_test();
        let coordinator = completed_minimum_source_for_test();
        ABI_STATE.with(|state| state.borrow_mut().distributed_coordinator = Some(coordinator));
        assert_eq!(clearra_wasm_distributed_finish_start(31, 2), 1);
        let mut query = None;
        for _ in 0..1_000 {
            match clearra_wasm_distributed_finish_parallel_prepare(31, 8) {
                0 => assert_eq!(clearra_wasm_distributed_finish_advance(31, 8), 1),
                1 => {
                    query = Some(ABI_STATE.with(|state| state.borrow().output_bytes().to_vec()));
                    assert_eq!(clearra_wasm_output_release(), ABI_OK);
                    break;
                }
                status => panic!("parallel query failed: {status}"),
            }
        }
        let query = query.expect("tiny fixture reaches an exact parallel query");
        assert_eq!(clearra_wasm_distributed_finish_parallel_local_start(31), 1);
        // A checked, unrepresentable ABI-owner growth must drop only local
        // scratch, retaining the exact already-issued task for remote retry.
        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            let (_, completion) = state.distributed_completion.as_mut().unwrap();
            assert!(completion.advance_coordinator_shard(8, u128::MAX).unwrap());
        });
        assert_eq!(clearra_wasm_distributed_finish_parallel_local_start(31), 0);
        assert_eq!(clearra_wasm_distributed_finish_advance(31, 8), 1);
        assert_eq!(clearra_wasm_distributed_finish_parallel_task(31), 1);
        let task = ABI_STATE.with(|state| state.borrow().output_bytes().to_vec());
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        let mut remote = WasmMinimumParallelWorker::initialize(&query).unwrap();
        remote.start(&task).unwrap();
        let mut receipt = None;
        for _ in 0..1_000 {
            if let Some(completed) = remote.advance(8).unwrap() {
                receipt = Some(completed);
                break;
            }
        }
        let receipt = receipt.expect("retried exact task terminates");
        drop(remote); // Native instances share one real authority; transport is now detached.
        ABI_STATE.with(|state| state.borrow_mut().transfer_input = receipt.clone());
        assert_eq!(clearra_wasm_distributed_finish_parallel_merge(31), ABI_OK);
        // Receipt identity and original issuance are still valid; the retry
        // cannot be accepted a second time as another negative certificate.
        ABI_STATE.with(|state| state.borrow_mut().transfer_input = receipt);
        assert_eq!(
            clearra_wasm_distributed_finish_parallel_merge(31),
            ABI_ERROR
        );
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn parallel_minimum_abi_preserves_exact_product_and_rejects_stale_job_receipts() {
        fn output() -> Vec<u8> {
            let output = ABI_STATE.with(|state| state.borrow().output_bytes().to_vec());
            assert_eq!(clearra_wasm_output_release(), ABI_OK);
            output
        }
        reset_abi_state_for_test();
        let coordinator = completed_minimum_source_for_test();
        ABI_STATE.with(|state| state.borrow_mut().distributed_coordinator = Some(coordinator));
        assert_eq!(clearra_wasm_distributed_finish_start(31, 2), 1);
        let mut query_count = 0;
        let mut completed = false;
        for _ in 0..1_000 {
            match clearra_wasm_distributed_finish_parallel_prepare(31, 4) {
                0 => {}
                1 => {
                    query_count += 1;
                    assert_eq!(
                        clearra_wasm_distributed_finish_parallel_task(31),
                        ABI_OUTPUT_NOT_RELEASED
                    );
                    let query = output();
                    assert_eq!(
                        clearra_wasm_distributed_finish_parallel_prepare(31, 4),
                        0,
                        "same query is published once"
                    );
                    let mut tasks = Vec::new();
                    loop {
                        match clearra_wasm_distributed_finish_parallel_task(31) {
                            0 => break,
                            1 => {
                                tasks.push(output());
                                assert_eq!(
                                    clearra_wasm_distributed_finish_parallel_last_task_key(31),
                                    1
                                );
                                let key = output();
                                assert_eq!(
                                    key.len(),
                                    56,
                                    "routing includes matrix, generation, query and partition"
                                );
                                ABI_STATE
                                    .with(|state| state.borrow_mut().transfer_input = key.clone());
                                assert_eq!(
                                    clearra_wasm_distributed_finish_parallel_redundant(31),
                                    0
                                );
                                let mut stale = key;
                                stale[0] ^= 1;
                                ABI_STATE.with(|state| state.borrow_mut().transfer_input = stale);
                                assert_eq!(
                                    clearra_wasm_distributed_finish_parallel_redundant(31),
                                    ABI_ERROR
                                );
                                let _ = output();
                            }
                            status => panic!("task export failed: {status}"),
                        }
                    }
                    assert!(!tasks.is_empty());
                    // Model separate workers with separate ABI owners, not a
                    // host-created proof or a second source-search execution.
                    let coordinator_state =
                        ABI_STATE.with(|state| core::mem::take(&mut *state.borrow_mut()));
                    ABI_STATE.with(|state| state.borrow_mut().transfer_input = query);
                    assert_eq!(
                        clearra_wasm_distributed_finish_parallel_worker_init(),
                        ABI_OK
                    );
                    ABI_STATE.with(|state| assert!(state.borrow().has_external_worker_owner()));
                    let mut receipts = Vec::new();
                    for task in tasks.into_iter().rev() {
                        ABI_STATE.with(|state| state.borrow_mut().transfer_input = task.clone());
                        assert_eq!(
                            clearra_wasm_distributed_finish_parallel_worker_start(),
                            ABI_OK
                        );
                        assert_eq!(
                            clearra_wasm_distributed_finish_parallel_worker_init(),
                            ABI_ERROR,
                            "an active shard cannot silently disappear on query replacement"
                        );
                        let _ = output();
                        assert_eq!(
                            clearra_wasm_distributed_finish_parallel_worker_cancel(),
                            ABI_OK
                        );
                        assert!(
                            !output().is_empty(),
                            "cancellation must return an identity-bound receipt"
                        );
                        ABI_STATE.with(|state| state.borrow_mut().transfer_input = task);
                        assert_eq!(
                            clearra_wasm_distributed_finish_parallel_worker_start(),
                            ABI_OK
                        );
                        assert_eq!(
                            clearra_wasm_distributed_finish_parallel_worker_advance(0),
                            1
                        );
                        let mut terminal = false;
                        for _ in 0..1_000 {
                            match clearra_wasm_distributed_finish_parallel_worker_advance(8) {
                                1 => {}
                                ABI_OK => {
                                    receipts.push(output());
                                    terminal = true;
                                    break;
                                }
                                status => panic!("worker exact shard failed: {status}"),
                            }
                        }
                        assert!(terminal, "tiny worker must terminate");
                    }
                    assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
                    ABI_STATE.with(|state| *state.borrow_mut() = coordinator_state);
                    for receipt in receipts {
                        ABI_STATE.with(|state| state.borrow_mut().transfer_input = receipt.clone());
                        assert_eq!(
                            clearra_wasm_distributed_finish_parallel_merge(32),
                            ABI_ERROR
                        );
                        let _ = output();
                        ABI_STATE.with(|state| assert_eq!(state.borrow().transfer_input, receipt));
                        assert_eq!(clearra_wasm_distributed_finish_parallel_merge(31), ABI_OK);
                    }
                }
                status => panic!("parallel query export failed: {status}"),
            }
            match clearra_wasm_distributed_finish_advance(31, 8) {
                1 => {}
                ABI_OK => {
                    completed = true;
                    break;
                }
                status => panic!("parallel completion failed: {status}"),
            }
        }
        assert!(completed);
        assert!(query_count > 0, "test must use the parallel proof contract");
        let final_output = String::from_utf8(output()).unwrap();
        assert!(final_output.contains("\"event\":\"final_response\""));
        assert!(final_output.contains("\"job_id\":31"));
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.distributed_completion.is_none());
            assert!(state.product_page_source_owner.is_some());
        });
        assert_eq!(clearra_wasm_product_page_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    fn install_exact_product_page_request(alternative_index: &str, member_page: &str) {
        let request =
            format!("{PRODUCT_PAGE_REQUEST_CONTRACT}\n{alternative_index}\n{member_page}");
        assert_eq!(
            clearra_wasm_product_page_request_resize(request.len() as u32),
            ABI_OK
        );
        ABI_STATE.with(|state| {
            state.borrow_mut().input.copy_from_slice(request.as_bytes());
        });
    }

    fn reject_unaccounted_finite_worker_job(command: &str) -> String {
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().input = command.as_bytes().to_vec();
        });
        assert_eq!(
            clearra_wasm_start_job(),
            0,
            "the raw worker must reject finite memory without a retained authority"
        );
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(!state.runtime.has_active_finite_job());
            assert!(!state.runtime.has_completed_governed_events());
            assert!(state.input.is_empty());
            assert_eq!(state.input.capacity(), 0);
            assert!(state.transfer_input.is_empty());
            assert_eq!(state.transfer_input.capacity(), 0);
            assert!(state.tiling_solution_page_store.is_none());
            assert!(state.product_page_source_owner.is_none());
            assert!(state.product_page_store.is_none());
            assert!(state.governed_output.is_none());
            assert!(state.output_outstanding);
            String::from_utf8_lossy(state.output_bytes()).into_owned()
        })
    }

    fn complete_legacy_job(command: &str) -> u32 {
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().input = command.as_bytes().to_vec();
        });
        let job_id = clearra_wasm_start_job();
        assert_ne!(job_id, 0, "legacy worker job must be admitted");
        for _ in 0..10_000 {
            match clearra_wasm_advance_job(job_id, 100_000) {
                0 | 4 => {}
                1 => {
                    ABI_STATE.with(|state| {
                        let state = state.borrow();
                        assert!(!state.runtime.has_completed_governed_events());
                    });
                    return job_id;
                }
                status => {
                    let output = ABI_STATE.with(|state| {
                        String::from_utf8_lossy(state.borrow().output_bytes()).into_owned()
                    });
                    panic!("legacy worker job failed with status {status}: {output}");
                }
            }
        }
        panic!("legacy worker job did not complete within the bounded advance loop");
    }

    #[test]
    fn cancelled_gpu_warmup_cannot_complete_into_a_new_generation() {
        let mut state = WasmAbiState::default();
        let cancelled_generation = state.begin_gpu_warmup().expect("first warmup starts");

        state.cancel_gpu_warmup();
        assert!(!state.accepts_gpu_warmup_completion(cancelled_generation));

        let active_generation = state.begin_gpu_warmup().expect("replacement warmup starts");
        assert_ne!(cancelled_generation, active_generation);
        assert!(!state.accepts_gpu_warmup_completion(cancelled_generation));
        assert!(state.accepts_gpu_warmup_completion(active_generation));

        let mut output = Vec::with_capacity(193);
        output.extend_from_slice(b"preserved-output-owner");
        let output_pointer = output.as_ptr();
        let output_capacity = output.capacity();
        state.set_output_bytes(output);
        assert!(!state.accepts_gpu_warmup_completion(active_generation));
        assert_eq!(state.output.as_ptr(), output_pointer);
        assert_eq!(state.output.capacity(), output_capacity);
        assert!(matches!(state.gpu_warmup, Some(GpuWarmupState::Pending)));
    }

    #[test]
    fn pending_gpu_warmup_rejects_job_start_without_moving_the_command_owner() {
        reset_abi_state_for_test();
        let mut command = Vec::with_capacity(FINITE_BUILD_COMMAND.len() + 73);
        command.extend_from_slice(FINITE_BUILD_COMMAND.as_bytes());
        let command_pointer = command.as_ptr();
        let command_capacity = command.capacity();
        let generation = ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.input = command;
            state.begin_gpu_warmup().expect("pending warmup starts")
        });

        assert_eq!(clearra_wasm_start_job(), 0);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.input.as_ptr(), command_pointer);
            assert_eq!(state.input.capacity(), command_capacity);
            assert_eq!(state.gpu_warmup_generation, generation);
            assert!(matches!(state.gpu_warmup, Some(GpuWarmupState::Pending)));
            assert!(state.output.is_empty());
            assert_eq!(state.output.capacity(), 0);
            assert!(state.governed_output.is_none());
            assert!(!state.output_outstanding);
        });
        assert_eq!(clearra_wasm_gpu_warmup_cancel(), ABI_OK);

        let mut transfer = Vec::with_capacity(311);
        transfer.extend_from_slice(b"preserved-transfer-owner");
        let transfer_pointer = transfer.as_ptr();
        let transfer_capacity = transfer.capacity();
        ABI_STATE.with(|state| state.borrow_mut().transfer_input = transfer);
        assert_eq!(clearra_wasm_start_job(), 0);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.input.as_ptr(), command_pointer);
            assert_eq!(state.input.capacity(), command_capacity);
            assert_eq!(state.transfer_input.as_ptr(), transfer_pointer);
            assert_eq!(state.transfer_input.capacity(), transfer_capacity);
            assert!(!state.output_outstanding);
        });
        reset_abi_state_for_test();
    }

    #[test]
    fn finite_memory_worker_start_is_fail_closed_without_retained_authority() {
        let output = reject_unaccounted_finite_worker_job(FINITE_BUILD_COMMAND);
        assert_eq!(output, "E_WASM_FINITE_AUTHORITY_UNAVAILABLE: ");

        assert_eq!(clearra_wasm_input_resize(1), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn optional_u32_count_distinguishes_absence_exact_maximum_and_overflow() {
        let unavailable = None;
        assert_eq!(AbiU32Count::legacy_or(unavailable, u32::MAX), u32::MAX);
        assert_eq!(AbiU32Count::exact_or_false(unavailable), 0);

        for value in [0, u32::MAX - 1, u32::MAX] {
            let exact = Some(AbiU32Count::project(value as usize));
            assert_eq!(AbiU32Count::legacy_or(exact, u32::MAX), value);
            assert_eq!(AbiU32Count::exact_or_false(exact), 1);
        }

        #[cfg(target_pointer_width = "64")]
        {
            let overflow = Some(AbiU32Count::project(u32::MAX as usize + 1));
            assert_eq!(AbiU32Count::legacy_or(overflow, 0), u32::MAX);
            assert_eq!(AbiU32Count::exact_or_false(overflow), 0);
        }
    }

    #[test]
    fn verifier_i32_count_distinguishes_exact_maximum_and_saturation() {
        for value in [0, i32::MAX - 1, i32::MAX] {
            let exact = AbiI32Count::project(value as usize);
            assert_eq!(exact.legacy_value, value);
            assert!(exact.exact);
        }

        let overflow = AbiI32Count::project(i32::MAX as usize + 1);
        assert_eq!(overflow.legacy_value, i32::MAX);
        assert!(!overflow.exact);
    }

    #[test]
    fn distributed_reset_requires_output_release_then_releases_transfer_capacity() {
        reset_abi_state_for_test();
        for byte_len in [2 * 1024 * 1024, 257, 4 * 1024 * 1024] {
            ABI_STATE.with(|state| {
                let mut state = state.borrow_mut();
                state.input = Vec::with_capacity(byte_len / 2 + 17);
                state.input.extend([b'x'; 19]);
                state.transfer_input = Vec::with_capacity(byte_len);
                state.transfer_input.resize(byte_len, 0x5a);
                let mut output = Vec::with_capacity(byte_len + 31);
                output.extend([0x4f; 11]);
                state.set_output_bytes(output);
                state.distributed_verifier_partial_available = true;
                state.distributed_verifier_pending_work = true;
                state.record_distributed_verifier_candidate_count(7);
                assert!(state.input.capacity() >= byte_len / 2 + 17);
                assert!(state.transfer_input.capacity() >= byte_len);
                assert!(state.output_retained_capacity_bytes() >= (byte_len + 31) as u128);
            });

            assert_eq!(clearra_wasm_distributed_reset(), ABI_OUTPUT_NOT_RELEASED);
            ABI_STATE.with(|state| {
                let state = state.borrow();
                assert!(state.input.capacity() >= byte_len / 2 + 17);
                assert!(state.transfer_input.capacity() >= byte_len);
                assert!(state.output_retained_capacity_bytes() >= (byte_len + 31) as u128);
                assert!(state.output_outstanding);
                assert!(state.distributed_verifier_partial_available);
                assert!(state.distributed_verifier_pending_work);
                assert!(state.distributed_verifier_last_candidate_count.is_some());
            });

            assert_eq!(clearra_wasm_output_release(), ABI_OK);
            assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
            ABI_STATE.with(|state| {
                let state = state.borrow();
                assert!(state.input.is_empty());
                assert_eq!(state.input.capacity(), 0);
                assert!(state.transfer_input.is_empty());
                assert_eq!(state.transfer_input.capacity(), 0);
                assert!(state.output.is_empty());
                assert_eq!(state.output_retained_capacity_bytes(), 0);
                assert!(!state.output_outstanding);
                assert!(!state.distributed_verifier_partial_available);
                assert!(!state.distributed_verifier_pending_work);
                assert!(state.distributed_verifier_last_candidate_count.is_none());
            });

            assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
            ABI_STATE.with(|state| {
                let state = state.borrow();
                assert_eq!(state.input.capacity(), 0);
                assert_eq!(state.transfer_input.capacity(), 0);
            });
        }
    }

    #[test]
    fn output_release_drops_actual_capacity_at_the_exact_owner_boundary() {
        reset_abi_state_for_test();
        let mut output = Vec::with_capacity(8 * 1024 + 37);
        output.extend([0x5a; 17]);
        let expected_actual_bytes = output.capacity() as u128;
        assert!(expected_actual_bytes > output.len() as u128);

        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.set_output_bytes(output);
            let actual_bytes = state.output_retained_capacity_bytes();
            assert!(state.output_outstanding);
            assert_eq!(actual_bytes, expected_actual_bytes);
            assert!(
                actual_bytes <= expected_actual_bytes,
                "the exact boundary must admit"
            );
            assert!(
                actual_bytes > expected_actual_bytes - 1,
                "one byte below the actual-capacity peak must reject"
            );
        });

        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.output.is_empty());
            assert_eq!(state.output_retained_capacity_bytes(), 0);
            assert!(!state.output_outstanding);
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn finite_memory_worker_rejection_never_creates_a_governed_event_lease() {
        let output = reject_unaccounted_finite_worker_job(FINITE_BUILD_COMMAND);
        assert_eq!(output, "E_WASM_FINITE_AUTHORITY_UNAVAILABLE: ");
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.governed_output.is_none());
            assert!(!state.runtime.has_completed_governed_events());
            assert!(state.tiling_solution_page_store.is_none());
            assert!(state.product_page_source_owner.is_none());
            assert!(state.product_page_store.is_none());
        });
        assert_eq!(clearra_wasm_drain_job_events(1), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_drain_job_events(1), ABI_ERROR);
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn governed_page_store_bounds_admission_and_continuation_output_peaks() {
        let store = typed_pc_tiling_page_store_fixture();
        let graph_bytes = store
            .checked_retained_capacity_bytes()
            .expect("fixture graph retained bytes remain representable");
        let workspace_bytes =
            TilingSolutionPageStore::checked_retained_capacity_projection_workspace_inline_bytes()
                .expect("projection workspace remains representable");
        let transition_inline_bytes =
            core::mem::size_of::<(String, Option<Arc<TilingSolutionPageStore>>, u128, u128)>()
                as u128;
        let storage_inline_bytes =
            core::mem::size_of::<Option<AbiTilingSolutionPageStore>>() as u128;
        let actual_peak = graph_bytes
            .checked_add(transition_inline_bytes.max(storage_inline_bytes))
            .and_then(|actual| actual.checked_add(workspace_bytes))
            .expect("fixture peak remains representable");
        assert!(actual_peak > 0);

        let store = match AbiTilingSolutionPageStore::try_governed(
            store,
            actual_peak - 1,
            graph_bytes,
            transition_inline_bytes,
        ) {
            Ok(_) => panic!("one byte below the measured peak must reject"),
            Err(store) => store,
        };
        let governed = AbiTilingSolutionPageStore::try_governed(
            store,
            actual_peak,
            graph_bytes,
            transition_inline_bytes,
        )
        .unwrap_or_else(|_| panic!("the exact measured peak must admit"));
        let (limit, governed_graph, stored_actual) = governed
            .governed_authority()
            .expect("exact admission creates a governed page-store owner");
        assert_eq!(limit, actual_peak);
        assert_eq!(governed_graph, graph_bytes);
        assert_eq!(stored_actual, graph_bytes + storage_inline_bytes);
        assert!(stored_actual <= actual_peak);

        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().tiling_solution_page_store = Some(governed);
        });
        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            let generation = state.begin_gpu_warmup().expect("late callback fixture");
            assert!(!state.accepts_gpu_warmup_completion(generation));
            state.cancel_gpu_warmup();
        });

        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_tiling_solution_count_available(), 1);
        assert_ne!(clearra_wasm_tiling_solution_count(), u32::MAX);
        assert_eq!(clearra_wasm_output_len(), 0);
        assert_eq!(clearra_wasm_input_resize(1), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_ERROR);

        assert_eq!(clearra_wasm_tiling_solution_release(), ABI_OK);
        ABI_STATE.with(|state| {
            assert!(state.borrow().tiling_solution_page_store.is_none());
        });
        assert_eq!(clearra_wasm_input_resize(1), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);

        let store = typed_pc_tiling_page_store_fixture();
        assert!(store.len() > 100);
        let expected = store
            .page_keys(100, 100)
            .expect("canonical continuation-page fixture");
        assert!(!expected.is_empty());
        let exact_len = checked_tiling_solution_page_json_len(&store, 100, 100)
            .expect("governed continuation-page JSON length");
        let expected_output = serialize_tiling_solution_page_json(&store, 100, 100, exact_len)
            .expect("governed continuation-page JSON fixture");
        assert_eq!(expected_output, format!("[\"{}\"]", expected.join("\",\"")));
        let output_peak = graph_bytes
            .checked_add(storage_inline_bytes)
            .and_then(|actual| actual.checked_add(core::mem::size_of::<String>() as u128))
            .and_then(|actual| actual.checked_add(expected_output.capacity() as u128))
            .expect("continuation output peak remains representable");
        assert!(
            output_peak <= actual_peak,
            "the admitted projection workspace must also cover one bounded continuation page"
        );

        let denied_output_store = AbiTilingSolutionPageStore::Governed {
            store: Arc::clone(&store),
            memory_limit_bytes: output_peak - 1,
            producer_graph_bytes: graph_bytes,
        };
        assert!(!denied_output_store.governed_output_fits(expected_output.capacity()));
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().tiling_solution_page_store = Some(denied_output_store);
        });
        assert_eq!(clearra_wasm_tiling_solution_page(100, 100), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.output.is_empty());
            assert!(!state.output_outstanding);
            assert!(
                state
                    .tiling_solution_page_store
                    .as_ref()
                    .is_some_and(AbiTilingSolutionPageStore::is_governed)
            );
        });
        assert_eq!(clearra_wasm_tiling_solution_count_available(), 1);
        assert_eq!(clearra_wasm_tiling_solution_release(), ABI_OK);

        let exact_peak_store = AbiTilingSolutionPageStore::try_governed(
            store,
            actual_peak,
            graph_bytes,
            transition_inline_bytes,
        )
        .unwrap_or_else(|_| panic!("the exact measured peak must admit a continuation owner"));
        assert!(exact_peak_store.governed_output_fits(expected_output.capacity()));
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().tiling_solution_page_store = Some(exact_peak_store);
        });
        assert_eq!(clearra_wasm_tiling_solution_page(100, 100), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let page = String::from_utf8(state.output_bytes().to_vec())
                .expect("continuation page remains UTF-8");
            assert_eq!(page, expected_output);
            assert!(
                state
                    .tiling_solution_page_store
                    .as_ref()
                    .is_some_and(AbiTilingSolutionPageStore::is_governed)
            );
        });
        assert_eq!(
            clearra_wasm_tiling_solution_page(200, 100),
            ABI_OUTPUT_NOT_RELEASED
        );

        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_tiling_solution_release(), ABI_OK);
        assert_eq!(clearra_wasm_tiling_solution_count_available(), 0);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn governed_product_owner_transfer_bounds_every_page_output_until_release() {
        reset_abi_state_for_test();
        let source = typed_pc_minimals_product_source_fixture();
        let source_bytes = source
            .checked_retained_capacity_bytes()
            .expect("product source retained bytes remain representable");
        let transition_inline_bytes = core::mem::size_of::<(
            String,
            Option<Arc<TilingSolutionPageStore>>,
            Option<ProductPageSourceOwner>,
            u128,
            u128,
        )>() as u128;
        let memory_limit_bytes = 64_u128 * 1024 * 1024;
        ABI_STATE.with(|state| {
            assert!(state.borrow_mut().install_governed_product_page_source(
                source,
                memory_limit_bytes,
                source_bytes,
                transition_inline_bytes,
            ));
        });
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let store = state
                .product_page_store
                .as_ref()
                .expect("admitted transition transfers the product page store");
            let (limit, actual) = store
                .governed_authority()
                .expect("finite page store retains its memory authority");
            assert!(actual <= limit);
            assert!(state.product_page_source_owner.is_none());
            assert!(!state.output_outstanding);
        });
        assert_eq!(clearra_wasm_product_page_available(), 1);
        assert_eq!(clearra_wasm_input_resize(1), ABI_ERROR);

        assert_eq!(clearra_wasm_product_page_get(1, 1), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let wire = String::from_utf8_lossy(state.output_bytes());
            assert!(wire.contains("\"product_page_kind\":\"coverage-portfolio\""));
            assert!(wire.contains("\"candidate_id\":\"1\""));
            let store = state
                .product_page_store
                .as_ref()
                .expect("page owner retained");
            assert!(store.governed_output_fits(state.output.capacity()));
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);

        install_exact_product_page_request("1", "1");
        assert_eq!(clearra_wasm_product_page_get_exact(), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let wire = String::from_utf8_lossy(state.output_bytes());
            assert!(wire.contains("\"alternative_index\":\"1\""));
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);

        for _ in 0..32 {
            assert_eq!(clearra_wasm_product_page_next(10_000), ABI_OK);
            let sealed = ABI_STATE.with(|state| {
                let state = state.borrow();
                assert!(state.output_outstanding);
                let store = state
                    .product_page_store
                    .as_ref()
                    .expect("page owner retained");
                assert!(store.governed_store_fits());
                assert!(store.governed_output_fits(state.output.capacity()));
                assert!(store.store().coverage_portfolio().is_none_or(|coverage| {
                    coverage.loaded_page_count() <= PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT
                }));
                String::from_utf8_lossy(state.output_bytes()).contains("\"state\":\"sealed\"")
            });
            assert_eq!(clearra_wasm_output_release(), ABI_OK);
            if sealed {
                break;
            }
        }

        install_exact_product_page_request("1", "1");
        assert_eq!(clearra_wasm_product_page_get_exact(), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let wire = String::from_utf8_lossy(state.output_bytes());
            assert!(wire.contains("\"alternative_index\":\"1\""));
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_available(), 0);
        assert_eq!(clearra_wasm_input_resize(1), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn governed_product_advance_denial_is_an_abi_error_without_a_false_page_state() {
        let source = typed_pc_minimals_product_source_fixture();
        let mut probe = ProductPageStore::from_source(source.clone()).expect("probe page store");
        let mut trace = Vec::new();
        probe
            .coverage_portfolio_mut()
            .expect("coverage portfolio")
            .next_page_with_memory_guard(
                10_000,
                &mut |whole_live| {
                    trace.push(whole_live);
                    true
                },
                &mut || false,
            )
            .expect("measure one guarded advance");
        let abi_inline = core::mem::size_of::<Option<AbiProductPageStore>>() as u128;
        let denied_limit = trace
            .iter()
            .copied()
            .max()
            .expect("advance reports a whole-live peak")
            .checked_add(abi_inline)
            .and_then(|peak| peak.checked_sub(1))
            .expect("peak-minus-one limit");

        let denied_store = ProductPageStore::from_source(source).expect("denied page store");
        let retained_before = denied_store
            .checked_retained_capacity_bytes()
            .expect("retained bytes");
        assert!(retained_before + abi_inline <= denied_limit);
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().product_page_store = Some(AbiProductPageStore::Governed {
                store: denied_store,
                memory_limit_bytes: denied_limit,
            });
        });

        assert_eq!(clearra_wasm_product_page_next(10_000), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.product_page_store.is_none());
            assert!(state.output.is_empty());
            assert!(!state.output_outstanding);
        });
    }

    #[test]
    fn governed_product_cache_replay_denial_is_fail_closed_at_the_abi_boundary() {
        fn fully_advanced(source: ProductPageSourceOwner) -> ProductPageStore {
            let mut store = ProductPageStore::from_source(source).expect("page store");
            let coverage = store.coverage_portfolio_mut().expect("coverage portfolio");
            loop {
                let advance = coverage
                    .next_page(u64::MAX, &mut || false)
                    .expect("advance all alternatives");
                if advance.checkpoint().enumeration_complete() {
                    break;
                }
            }
            assert!(coverage.retained_page_slot("1").is_none());
            store
        }

        let source = multi_alternative_pc_minimals_product_source_fixture();
        let mut probe = fully_advanced(source.clone());
        let mut trace = Vec::new();
        probe
            .coverage_portfolio_mut()
            .expect("coverage portfolio")
            .load_page_by_alternative_index_slice_with_memory_guard(
                "1",
                DEFAULT_PRODUCT_PAGE_WORK_STEPS,
                &mut |whole_live| {
                    trace.push(whole_live);
                    true
                },
                &mut || false,
            )
            .expect("measure guarded cache replay");
        let abi_inline = core::mem::size_of::<Option<AbiProductPageStore>>() as u128;
        let requested_alternative_index = "1".to_owned();
        let request_capacity = requested_alternative_index.capacity();
        let denied_limit = trace
            .iter()
            .copied()
            .max()
            .expect("replay reports a whole-live peak")
            .checked_add(abi_inline)
            .and_then(|peak| peak.checked_add(request_capacity as u128))
            .and_then(|peak| peak.checked_sub(1))
            .expect("peak-minus-one limit");
        let denied_store = fully_advanced(source);
        let retained_before = denied_store
            .checked_retained_capacity_bytes()
            .expect("retained bytes");
        assert!(retained_before + abi_inline + request_capacity as u128 <= denied_limit);
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().product_page_store = Some(AbiProductPageStore::Governed {
                store: denied_store,
                memory_limit_bytes: denied_limit,
            });
        });

        let status = ABI_STATE.with(|state| {
            product_page_get_from_state(
                &mut state.borrow_mut(),
                &requested_alternative_index,
                1,
                DEFAULT_PRODUCT_PAGE_WORK_STEPS,
                request_capacity,
            )
        });
        assert_eq!(status, ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.product_page_store.is_none());
            assert!(state.output.is_empty());
            assert!(!state.output_outstanding);
        });
    }

    #[test]
    fn exact_product_page_request_preserves_indices_beyond_js_and_u32_ranges() {
        let request = format!("{PRODUCT_PAGE_REQUEST_CONTRACT}\n184467440737095516160\n1");
        let (alternative_index, member_page, maximum_work_steps) =
            parse_product_page_request(&request).expect("exact decimal request");
        assert_eq!(alternative_index, "184467440737095516160");
        assert_eq!(member_page, 1);
        assert_eq!(maximum_work_steps, DEFAULT_PRODUCT_PAGE_WORK_STEPS);
        let bounded = format!("{PRODUCT_PAGE_REQUEST_CONTRACT_V2}\n184467440737095516160\n1\n17");
        let (_, _, maximum_work_steps) =
            parse_product_page_request(&bounded).expect("bounded exact decimal request");
        assert_eq!(maximum_work_steps, 17);
        assert!(
            parse_product_page_request("portfolio-page-request.v1\n9007199254740992\n01").is_err()
        );
        assert!(
            parse_product_page_request("portfolio-page-request.v1\n4294967296\n1\ntrailing")
                .is_err()
        );
    }

    #[test]
    fn actual_pc_minimals_worker_transfers_and_releases_the_public_page_owner() {
        let job_id = complete_legacy_job(TYPED_PC_MINIMALS_COMMAND);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.product_page_source_owner.is_some());
            assert!(state.product_page_store.is_none());
            assert!(state.tiling_solution_page_store.is_none());
            assert!(!state.output_outstanding);
        });
        assert_eq!(clearra_wasm_product_page_available(), 1);

        assert_eq!(clearra_wasm_drain_job_events(job_id), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let wire = String::from_utf8_lossy(state.output_bytes());
            assert!(wire.contains("\"product_result_payload\""));
            assert!(wire.contains("\"payload_kind\":\"coverage-portfolio\""));
            assert!(wire.contains("\"page_handle_available\":true"));
            assert!(state.product_page_source_owner.is_some());
            assert!(state.output_outstanding);
        });
        assert_eq!(clearra_wasm_product_page_get(1, 1), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_output_release(), ABI_OK);

        assert_eq!(clearra_wasm_product_page_get(1, 1), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let wire = String::from_utf8_lossy(state.output_bytes());
            assert!(wire.contains("\"product_page_kind\":\"coverage-portfolio\""));
            assert!(wire.contains("\"candidate_id\":\"1\""));
            assert!(state.product_page_source_owner.is_none());
            assert!(state.product_page_store.is_some());
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);

        assert_eq!(clearra_wasm_product_page_next(10_000), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.product_page_store.is_some());
            assert!(state.output_outstanding);
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_available(), 0);
        assert_eq!(clearra_wasm_input_resize(1), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn actual_parity_worker_transfers_browses_and_releases_the_public_page_owner() {
        let job_id = complete_legacy_job(TYPED_PARITY_COMMAND);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(matches!(
                state.product_page_source_owner,
                Some(ProductPageSourceOwner::ParityReport(_))
            ));
            assert!(state.product_page_store.is_none());
        });
        assert_eq!(clearra_wasm_product_page_available(), 1);

        assert_eq!(clearra_wasm_drain_job_events(job_id), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let wire = String::from_utf8_lossy(state.output_bytes());
            assert!(wire.contains("\"payload_kind\":\"parity-report-page\""));
            assert!(wire.contains("\"feasibility_claim\":false"));
            assert!(wire.contains("\"pruning_authority\":\"none\""));
            assert!(wire.contains("\"page_handle_available\":true"));
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);

        assert_eq!(clearra_wasm_product_page_get(1, 1), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let wire = String::from_utf8_lossy(state.output_bytes());
            assert!(wire.contains("\"product_page_kind\":\"parity-report\""));
            assert!(wire.contains("\"state\":\"page\""));
            assert!(wire.contains("\"page_number\":1"));
            assert!(state.product_page_source_owner.is_none());
            assert!(state.product_page_store.is_some());
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);

        assert_eq!(clearra_wasm_product_page_next(1), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let wire = String::from_utf8_lossy(state.output_bytes());
            assert!(wire.contains("\"product_page_kind\":\"parity-report\""));
            assert!(wire.contains("\"state\":\"exhausted\""));
            assert!(state.product_page_store.is_some());
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_available(), 0);
        assert_eq!(clearra_wasm_input_resize(1), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn rejected_finite_generic_tiling_does_not_forge_pc_tiling_page_authority() {
        let output = reject_unaccounted_finite_worker_job(FINITE_GENERIC_TILING_COMMAND);
        assert_eq!(output, "E_WASM_FINITE_AUTHORITY_UNAVAILABLE: ");
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.tiling_solution_page_store.is_none());
            assert!(state.product_page_source_owner.is_none());
            assert!(state.product_page_store.is_none());
            assert!(state.governed_output.is_none());
            assert!(state.output_outstanding);
        });

        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.tiling_solution_page_store.is_none());
            assert!(!state.output_outstanding);
            assert!(state.governed_output.is_none());
        });
        assert_eq!(clearra_wasm_tiling_solution_count_available(), 0);
        assert_eq!(clearra_wasm_tiling_solution_count(), u32::MAX);
        assert_eq!(clearra_wasm_tiling_solution_count_exact(), 0);
        assert_eq!(clearra_wasm_tiling_solution_page(0, 1), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.tiling_solution_page_store.is_none());
            assert!(!state.output.is_empty());
            assert!(state.output_outstanding);
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_input_resize(1), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn outstanding_output_rejects_every_mutating_export_without_changing_the_lease() {
        reset_abi_state_for_test();
        let mut output = Vec::with_capacity(4 * 1024 + 19);
        output.extend([0x43; 23]);
        let expected_output = output.clone();
        let expected_output_ptr = output.as_ptr();
        let expected_output_capacity = output.capacity();
        let mut transfer = Vec::with_capacity(2 * 1024 + 11);
        transfer.extend([0x7a; 31]);
        let expected_transfer = transfer.clone();
        let expected_transfer_capacity = transfer.capacity();
        let mut command = Vec::with_capacity(1024 + 7);
        command.extend([0xff; 13]);
        let expected_command = command.clone();
        let expected_command_capacity = command.capacity();
        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.set_output_bytes(output);
            state.transfer_input = transfer;
            state.input = command;
            state.record_distributed_verifier_candidate_count(29);
        });

        assert_eq!(
            clearra_wasm_configure_host(u32::MAX, u32::MAX),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_gpu_warmup_start(i32::MAX),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(clearra_wasm_gpu_warmup_cancel(), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_gpu_warmup_advance(), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_input_resize(u32::MAX), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(
            clearra_wasm_transfer_resize(u32::MAX),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(clearra_wasm_tablebase_install(), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_tablebase_release(), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_distributed_prepare(), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(
            clearra_wasm_distributed_worker_initialization(),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_distributed_produce(usize::MAX as u32, usize::MAX as u32),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_distributed_merge_partial(),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_distributed_finish(u32::MAX, u32::MAX),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_distributed_verifier_start(),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_distributed_forward_verifier_start(),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_distributed_verifier_consume(),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_distributed_verifier_continue(),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_distributed_verifier_finish(),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_tiling_solution_page(u32::MAX, u32::MAX),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(
            clearra_wasm_tiling_solution_release(),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(clearra_wasm_distributed_cancel(), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_start_job(), 0);
        assert_eq!(
            clearra_wasm_advance_job(u32::MAX, u32::MAX),
            ABI_OUTPUT_NOT_RELEASED
        );
        assert_eq!(clearra_wasm_cancel_job(u32::MAX), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(
            clearra_wasm_drain_job_events(u32::MAX),
            ABI_OUTPUT_NOT_RELEASED
        );
        #[cfg(feature = "stage-profiling")]
        {
            assert_eq!(clearra_wasm_profile_start(), ABI_OUTPUT_NOT_RELEASED);
            assert_eq!(clearra_wasm_profile_finish(), ABI_OUTPUT_NOT_RELEASED);
        }
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.output, expected_output);
            assert_eq!(state.output.capacity(), expected_output_capacity);
            assert_eq!(state.output.as_ptr(), expected_output_ptr);
            assert_eq!(state.input, expected_command);
            assert_eq!(state.input.capacity(), expected_command_capacity);
            assert_eq!(state.transfer_input, expected_transfer);
            assert_eq!(state.transfer_input.capacity(), expected_transfer_capacity);
            assert_eq!(
                state
                    .distributed_verifier_last_candidate_count
                    .expect("candidate metadata remains untouched")
                    .legacy_value,
                29
            );
            let actual_peak = state.output_retained_capacity_bytes();
            assert_eq!(actual_peak, expected_output_capacity as u128);
            assert!(actual_peak <= expected_output_capacity as u128);
            assert!(actual_peak > expected_output_capacity as u128 - 1);
        });

        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_merge_partial(), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.transfer_input.is_empty());
            assert_eq!(state.transfer_input.capacity(), 0);
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn verifier_candidate_count_metadata_is_cleared_by_reset() {
        let mut state = WasmAbiState::default();

        assert_eq!(
            state.record_distributed_verifier_candidate_count(i32::MAX as usize),
            i32::MAX
        );
        assert!(
            state
                .distributed_verifier_last_candidate_count
                .is_some_and(|count| count.exact)
        );

        assert_eq!(
            state.record_distributed_verifier_candidate_count(i32::MAX as usize + 1),
            i32::MAX
        );
        assert!(
            state
                .distributed_verifier_last_candidate_count
                .is_some_and(|count| !count.exact)
        );

        state.reset_distributed_state();
        assert!(state.distributed_verifier_last_candidate_count.is_none());
    }

    #[test]
    fn starting_any_next_job_releases_the_prior_typed_tiling_store_before_admission() {
        reset_abi_state_for_test();
        let command = TYPED_PC_TILING_COMMAND;
        let store = typed_pc_tiling_page_store_fixture();
        let baseline_strong_count = Arc::strong_count(&store);

        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.tiling_solution_page_store =
                Some(AbiTilingSolutionPageStore::legacy(Arc::clone(&store)));
            state.input = vec![0xff];
        });
        assert_eq!(Arc::strong_count(&store), baseline_strong_count + 1);
        assert_eq!(clearra_wasm_start_job(), 0);
        assert_eq!(Arc::strong_count(&store), baseline_strong_count);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.tiling_solution_page_store.is_none());
            assert!(state.input.is_empty());
            assert_eq!(state.input.capacity(), 0);
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);

        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.tiling_solution_page_store =
                Some(AbiTilingSolutionPageStore::legacy(Arc::clone(&store)));
            state.input = command.as_bytes().to_vec();
        });
        assert_eq!(Arc::strong_count(&store), baseline_strong_count + 1);
        let replacement_job = clearra_wasm_start_job();
        assert_ne!(replacement_job, 0);
        assert_eq!(Arc::strong_count(&store), baseline_strong_count);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.tiling_solution_page_store.is_none());
            assert!(state.input.is_empty());
            assert_eq!(state.input.capacity(), 0);
        });
        assert_eq!(clearra_wasm_cancel_job(replacement_job), ABI_OK);
        ABI_STATE.with(|state| {
            assert_eq!(
                state
                    .borrow()
                    .runtime
                    .status(WasmWorkerJobId::new(replacement_job.into())),
                Some(WasmWorkerJobStatus::Cancelled)
            );
        });
    }

    #[test]
    fn exported_count_metadata_distinguishes_unavailable_legacy_defaults() {
        reset_abi_state_for_test();

        assert_eq!(clearra_wasm_distributed_worker_count(), 1);
        assert_eq!(clearra_wasm_distributed_worker_count_available(), 1);
        assert_eq!(clearra_wasm_distributed_worker_count_exact(), 1);

        assert_eq!(clearra_wasm_distributed_progress_available(), 0);
        assert_eq!(clearra_wasm_distributed_progress_candidate_count(), 0);
        assert_eq!(clearra_wasm_distributed_progress_candidate_count_exact(), 0);
        assert_eq!(clearra_wasm_distributed_progress_pass_count(), 1);
        assert_eq!(clearra_wasm_distributed_progress_pass_count_exact(), 0);

        assert_eq!(clearra_wasm_tiling_solution_count(), u32::MAX);
        assert_eq!(clearra_wasm_tiling_solution_count_available(), 0);
        assert_eq!(clearra_wasm_tiling_solution_count_exact(), 0);

        assert_eq!(
            clearra_wasm_distributed_verifier_last_candidate_count_available(),
            0
        );
        assert_eq!(
            clearra_wasm_distributed_verifier_last_candidate_count_exact(),
            0
        );
        assert_eq!(clearra_wasm_distributed_verifier_progress_available(), 0);
        assert_eq!(
            clearra_wasm_distributed_verifier_progress_candidate_count(),
            0
        );
        assert_eq!(
            clearra_wasm_distributed_verifier_progress_candidate_count_exact(),
            0
        );
    }

    #[test]
    fn standalone_build_verifier_rejects_unaccounted_finite_authority() {
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().input = b"clearra build-probability --base-mask 0x0 \
                --target-mask 0xffffffffff --height 4 \
                --no-hold --no-mirror --workers 2 --max-memory-mib 1"
                .to_vec();
        });

        assert_eq!(clearra_wasm_distributed_verifier_start(), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.distributed_verifier.is_none());
            assert!(state.input.is_empty());
            assert_eq!(state.input.capacity(), 0);
            let output = std::str::from_utf8(&state.output).expect("typed UTF-8 error");
            assert!(output.starts_with('{'), "output={output}");
            assert!(
                output.contains("\"code\":\"E_WASM_FINITE_AUTHORITY_UNAVAILABLE\""),
                "output={output}"
            );
            assert!(
                output.contains("\"resource_report\":null"),
                "output={output}"
            );
            assert!(state.output_outstanding);
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }
}
