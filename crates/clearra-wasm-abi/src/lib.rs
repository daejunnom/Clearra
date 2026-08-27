//! SRP rationale: this module has one change reason: the stable WASM export contract and its
//! single owned ABI state boundary.

use std::{
    cell::RefCell,
    sync::{Arc, Once},
};

use clearra_pc_graph::request::GpuDeviceSelection;
#[cfg(target_arch = "wasm32")]
use clearra_wasm::prewarm_gpu_search_async;
#[cfg(feature = "stage-profiling")]
use clearra_wasm::ExecutorSearchProfileSession;
#[cfg(test)]
use clearra_wasm::WasmWorkerJobStatus;
use clearra_wasm::{
    install_pc4_compact_tablebase, release_pc4_compact_tablebase,
    serialize_coverage_portfolio_advance_state, serialize_coverage_portfolio_page,
    serialize_distributed_final_events, serialize_parity_report_exhausted,
    serialize_parity_report_page, GovernedWasmJson, GpuSearchWarmupReport, ProductPageSourceOwner,
    ProductPageStore, TilingSolutionPageStore, WasmCommandRuntimeError, WasmDistributedCoordinator,
    WasmDistributedFallbackReason, WasmDistributedMode, WasmDistributedPreparation,
    WasmDistributedProducerAdvance, WasmDistributedRequestedBackend,
    WasmDistributedVerifierRuntime, WasmHostCapabilities, WasmWorkerAdvanceStatus, WasmWorkerJobId,
    WasmWorkerJobRuntime,
};

const ABI_VERSION: u32 = 1;
const MAX_COMMAND_BYTES: usize = 1024 * 1024;
const MAX_TRANSFER_BYTES: usize = 512 * 1024 * 1024;
const ABI_OK: i32 = 0;
const ABI_ERROR: i32 = -1;
const ABI_OUTPUT_NOT_RELEASED: i32 = -2;
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
                debug_assert!(producer_graph_bytes
                    .checked_add(core::mem::size_of::<Option<AbiTilingSolutionPageStore>>() as u128)
                    .is_some_and(|actual| actual <= *memory_limit_bytes));
                true
            }
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

    fn begin_distributed_verifier(&mut self) {
        self.distributed_verifier_partial_available = false;
        self.distributed_verifier_pending_work = false;
        self.distributed_verifier_last_candidate_count = None;
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
pub extern "C" fn clearra_wasm_gpu_warmup_start(device_index: i32) -> i32 {
    let warmup = ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return Err(status);
        }
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
        state.transfer_input.resize(byte_len, 0);
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_transfer_ptr() -> u32 {
    ABI_STATE.with(|state| state.borrow().transfer_input.as_ptr() as usize as u32)
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
        let preparation = WasmDistributedCoordinator::prepare(
            state.runtime.command_runtime(),
            command_text.as_str(),
        );
        drop(command_text);
        match preparation {
            Ok(WasmDistributedPreparation::Serial) | Ok(WasmDistributedPreparation::Ready(_)) => {
                WasmDistributedMode::Serial as i32
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
        let Some(coordinator) = state.distributed_coordinator.take() else {
            state.set_error(
                "E_WASM_DISTRIBUTED_STATE",
                "distributed coordinator is not active",
            );
            return ABI_ERROR;
        };
        let result = match coordinator.finish(workers_used as usize) {
            Ok(result) => result,
            Err(error) => {
                state.set_runtime_error(&error);
                return ABI_ERROR;
            }
        };
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
        // The legacy page API allocates a Vec and owned key Strings without an
        // exact retained-memory admission seam. Keep governed page ownership
        // intact and fail without creating a competing error/output buffer.
        if store.is_governed() {
            return ABI_ERROR;
        }
        let keys = match store
            .store()
            .page_keys(offset as usize, (limit as usize).min(MAX_PAGE_SIZE))
        {
            Ok(keys) => keys,
            Err(reason) => {
                state.set_error("E_WASM_TILING_PAGE", reason);
                return ABI_ERROR;
            }
        };
        let mut output = String::from("[");
        for (index, key) in keys.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            output.push('"');
            output.push_str(key);
            output.push('"');
        }
        output.push(']');
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
            let (advance, loaded_page_number) = {
                let Some(store) = state
                    .product_page_store
                    .as_mut()
                    .and_then(|store| store.store_mut().coverage_portfolio_mut())
                else {
                    return ABI_ERROR;
                };
                let advance = match store.next_page(maximum_work_steps.max(1) as u64, &mut || false)
                {
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
                (advance, store.loaded_page_count())
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
                    serialize_coverage_portfolio_page(store, loaded_page_number, 1)
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
                if !governed {
                    state.set_runtime_error(&error);
                }
                ABI_ERROR
            }
        }
    })
}

/// Loads a member page for any retained outer alternative. The App store
/// enforces the fixed member-page size of exactly 100.
#[no_mangle]
pub extern "C" fn clearra_wasm_product_page_get(
    outer_page_number: u32,
    member_page_number: u32,
) -> i32 {
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
        let output = if let Some(store) = state
            .product_page_store
            .as_ref()
            .and_then(|store| store.store().coverage_portfolio())
        {
            serialize_coverage_portfolio_page(
                store,
                outer_page_number as usize,
                member_page_number as usize,
            )
        } else if let Some(store) = state
            .product_page_store
            .as_ref()
            .and_then(|store| store.store().parity_report())
        {
            if member_page_number != 1 {
                Err(WasmCommandRuntimeError::new(
                    "E_WASM_PRODUCT_PAGE",
                    "invalid-member-page",
                ))
            } else {
                store
                    .page(outer_page_number as usize)
                    .map_err(|error| {
                        WasmCommandRuntimeError::new("E_WASM_PRODUCT_PAGE", error.as_str())
                    })
                    .and_then(|page| serialize_parity_report_page(&page))
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
                if !governed {
                    state.set_runtime_error(&error);
                }
                ABI_ERROR
            }
        }
    })
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
        let state = state.borrow();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if let Some(coordinator) = state.distributed_coordinator.as_ref() {
            coordinator.cancel();
        }
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
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        state.begin_distributed_verifier();
        let command_text = match String::from_utf8(std::mem::take(&mut state.input)) {
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
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        state.begin_distributed_verifier();
        let input = std::mem::take(&mut state.transfer_input);
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
    const TYPED_PC_TILING_COMMAND: &str =
        "clearra pc tiling --lines 4 --queue OTSZJLIOTI --no-hold --backend cpu --workers 1";
    const TYPED_PC_MINIMALS_COMMAND: &str = "clearra pc minimals --lines 1 \
        --board-mask 0x3f --height 1 --pieces 1 --queue I --hold empty --rule srs-plus \
        --backend cpu --workers 1";
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

    fn reset_abi_state_for_test() {
        ABI_STATE.with(|state| *state.borrow_mut() = WasmAbiState::default());
    }

    fn complete_finite_job(command: &str) -> u32 {
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().input = command.as_bytes().to_vec();
        });
        let job_id = clearra_wasm_start_job();
        assert_ne!(job_id, 0, "finite Build job must be admitted");
        for _ in 0..10_000 {
            match clearra_wasm_advance_job(job_id, 100_000) {
                0 | 4 => {}
                1 => {
                    ABI_STATE.with(|state| {
                        let state = state.borrow();
                        assert!(state.runtime.has_completed_governed_events());
                        assert!(state.tiling_solution_page_store.is_none());
                    });
                    return job_id;
                }
                status => {
                    let output = ABI_STATE.with(|state| {
                        String::from_utf8_lossy(state.borrow().output_bytes()).into_owned()
                    });
                    panic!("finite Build job failed with status {status}: {output}");
                }
            }
        }
        panic!("finite Build job did not complete within the bounded advance loop");
    }

    fn complete_finite_build_job() -> u32 {
        complete_finite_job(FINITE_BUILD_COMMAND)
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

    fn start_active_finite_job() -> u32 {
        reset_abi_state_for_test();
        ABI_STATE.with(|state| {
            state.borrow_mut().input = FINITE_BUILD_COMMAND.as_bytes().to_vec();
        });
        let job_id = clearra_wasm_start_job();
        assert_ne!(job_id, 0, "finite Build job must be admitted");
        assert_eq!(clearra_wasm_advance_job(job_id, 1), 0);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.runtime.has_active_finite_job());
            assert!(!state.runtime.has_completed_governed_events());
        });
        job_id
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
    fn active_finite_job_rejects_preterminal_drain_and_unrelated_mutation_without_output() {
        let job_id = start_active_finite_job();
        let mut command = Vec::with_capacity(257);
        command.extend_from_slice(b"preserve-command-owner");
        let command_pointer = command.as_ptr();
        let command_capacity = command.capacity();
        let mut transfer = Vec::with_capacity(521);
        transfer.extend_from_slice(b"preserve-transfer-owner");
        let transfer_pointer = transfer.as_ptr();
        let transfer_capacity = transfer.capacity();
        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.input = command;
            state.transfer_input = transfer;
            let generation = state.begin_gpu_warmup().expect("late callback fixture");
            assert!(!state.accepts_gpu_warmup_completion(generation));
            state.cancel_gpu_warmup();
        });

        assert_eq!(clearra_wasm_drain_job_events(job_id), ABI_ERROR);
        assert_eq!(clearra_wasm_configure_host(u32::MAX, u32::MAX), ABI_ERROR);
        assert_eq!(clearra_wasm_gpu_warmup_start(i32::MAX), ABI_ERROR);
        assert_eq!(clearra_wasm_gpu_warmup_cancel(), ABI_ERROR);
        assert_eq!(clearra_wasm_gpu_warmup_advance(), ABI_ERROR);
        assert_eq!(clearra_wasm_input_resize(u32::MAX), ABI_ERROR);
        assert_eq!(clearra_wasm_transfer_resize(u32::MAX), ABI_ERROR);
        assert_eq!(clearra_wasm_tablebase_install(), ABI_ERROR);
        assert_eq!(clearra_wasm_tablebase_release(), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_prepare(), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_worker_initialization(), ABI_ERROR);
        assert_eq!(
            clearra_wasm_distributed_produce(u32::MAX, u32::MAX),
            ABI_ERROR
        );
        assert_eq!(clearra_wasm_distributed_merge_partial(), ABI_ERROR);
        assert_eq!(
            clearra_wasm_distributed_finish(u32::MAX, u32::MAX),
            ABI_ERROR
        );
        assert_eq!(clearra_wasm_distributed_verifier_start(), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_forward_verifier_start(), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_verifier_consume(), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_verifier_continue(), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_verifier_finish(), ABI_ERROR);
        assert_eq!(
            clearra_wasm_tiling_solution_page(u32::MAX, u32::MAX),
            ABI_ERROR
        );
        assert_eq!(clearra_wasm_tiling_solution_release(), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_cancel(), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_ERROR);
        assert_eq!(clearra_wasm_start_job(), 0);
        #[cfg(feature = "stage-profiling")]
        {
            assert_eq!(clearra_wasm_profile_start(), ABI_ERROR);
            assert_eq!(clearra_wasm_profile_finish(), ABI_ERROR);
        }

        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.runtime.has_active_finite_job());
            assert!(!state.runtime.has_completed_governed_events());
            assert_eq!(state.input.as_ptr(), command_pointer);
            assert_eq!(state.input.capacity(), command_capacity);
            assert_eq!(state.transfer_input.as_ptr(), transfer_pointer);
            assert_eq!(state.transfer_input.capacity(), transfer_capacity);
            assert!(state.output.is_empty());
            assert_eq!(state.output.capacity(), 0);
            assert!(state.governed_output.is_none());
            assert!(!state.output_outstanding);
        });

        assert_eq!(clearra_wasm_cancel_job(job_id), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(!state.runtime.has_active_finite_job());
            assert_eq!(
                state.runtime.status(WasmWorkerJobId::new(job_id.into())),
                Some(WasmWorkerJobStatus::Cancelled)
            );
        });
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
    fn governed_drain_preserves_wrong_job_owner_then_leases_exact_abi_output_until_release() {
        let job_id = complete_finite_build_job();
        let wrong_job_id = if job_id == u32::MAX {
            job_id - 1
        } else {
            job_id + 1
        };

        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            let generation = state.begin_gpu_warmup().expect("late callback fixture");
            assert!(!state.accepts_gpu_warmup_completion(generation));
        });
        assert_eq!(clearra_wasm_drain_job_events(job_id), ABI_ERROR);
        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            assert!(state.runtime.has_completed_governed_events());
            assert!(matches!(state.gpu_warmup, Some(GpuWarmupState::Pending)));
            assert!(state.output.is_empty());
            assert_eq!(state.output.capacity(), 0);
            assert!(!state.output_outstanding);
            state.cancel_gpu_warmup();
        });
        assert_eq!(clearra_wasm_tiling_solution_page(0, 1), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.runtime.has_completed_governed_events());
            assert!(state.output.is_empty());
            assert_eq!(state.output.capacity(), 0);
            assert!(state.governed_output.is_none());
            assert!(!state.output_outstanding);
        });

        assert_eq!(clearra_wasm_drain_job_events(wrong_job_id), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.runtime.has_completed_governed_events());
            assert!(state.output.is_empty());
            assert_eq!(state.output.capacity(), 0);
            assert!(state.governed_output.is_none());
            assert!(!state.output_outstanding);
        });

        assert_eq!(clearra_wasm_input_resize(1), ABI_ERROR);
        assert_eq!(clearra_wasm_transfer_resize(1), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_prepare(), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.runtime.has_completed_governed_events());
            assert_eq!(state.input.capacity(), 0);
            assert_eq!(state.transfer_input.capacity(), 0);
            assert!(state.output.is_empty());
            assert_eq!(state.output.capacity(), 0);
            assert!(!state.output_outstanding);
        });

        let mut next_command = Vec::with_capacity(FINITE_BUILD_COMMAND.len() + 37);
        next_command.extend_from_slice(FINITE_BUILD_COMMAND.as_bytes());
        let next_command_ptr = next_command.as_ptr();
        let next_command_capacity = next_command.capacity();
        ABI_STATE.with(|state| state.borrow_mut().input = next_command);
        assert_eq!(clearra_wasm_start_job(), 0);
        assert_eq!(clearra_wasm_advance_job(wrong_job_id, 1), ABI_ERROR);
        assert_eq!(clearra_wasm_cancel_job(wrong_job_id), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.runtime.has_completed_governed_events());
            assert_eq!(state.input.as_ptr(), next_command_ptr);
            assert_eq!(state.input.capacity(), next_command_capacity);
            assert!(state.output.is_empty());
            assert_eq!(state.output.capacity(), 0);
            assert!(!state.output_outstanding);
        });

        assert_eq!(clearra_wasm_drain_job_events(job_id), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.runtime.has_completed_governed_events());
            assert_eq!(state.input.as_ptr(), next_command_ptr);
            assert_eq!(state.input.capacity(), next_command_capacity);
            assert!(state.governed_output.is_none());
        });
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.runtime.has_completed_governed_events());
            assert_eq!(state.input.capacity(), 0);
            assert_eq!(state.transfer_input.capacity(), 0);
        });

        assert_eq!(clearra_wasm_drain_job_events(job_id), ABI_OK);
        let (output_ptr, output_len, output_actual, output_limit) = ABI_STATE.with(|state| {
            let state = state.borrow();
            let output = state
                .governed_output
                .as_ref()
                .expect("correct job drains the governed JSON owner into ABI state");
            let output_actual = WasmAbiState::governed_output_storage_actual_bytes(output)
                .expect("governed ABI storage bytes remain representable");
            assert!(WasmAbiState::governed_output_storage_fits(
                output,
                output_actual
            ));
            assert!(output_actual > 0);
            assert!(!WasmAbiState::governed_output_storage_fits(
                output,
                output_actual - 1
            ));
            assert!(output_actual <= output.memory_limit_bytes());
            assert_eq!(state.output_retained_capacity_bytes(), output_actual);
            assert!(state.output.is_empty());
            assert_eq!(state.output.capacity(), 0);
            assert!(state.output_outstanding);
            (
                output.json().as_ptr(),
                output.json().len(),
                output_actual,
                output.memory_limit_bytes(),
            )
        });
        assert_eq!(clearra_wasm_output_ptr(), output_ptr as usize as u32);
        assert_eq!(clearra_wasm_output_len(), output_len as u32);
        assert_eq!(clearra_wasm_output_len_exact(), 1);
        assert!(output_actual <= output_limit);

        assert_eq!(clearra_wasm_input_resize(1), ABI_OUTPUT_NOT_RELEASED);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OUTPUT_NOT_RELEASED);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let output = state
                .governed_output
                .as_ref()
                .expect("the next operation cannot consume the output owner");
            assert_eq!(output.json().as_ptr(), output_ptr);
            assert_eq!(output.json().len(), output_len);
            assert_eq!(state.output_retained_capacity_bytes(), output_actual);
            assert!(state.output_outstanding);
        });

        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.governed_output.is_none());
            assert!(state.output.is_empty());
            assert_eq!(state.output.capacity(), 0);
            assert_eq!(state.output_retained_capacity_bytes(), 0);
            assert!(!state.output_outstanding);
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_drain_job_events(wrong_job_id), ABI_ERROR);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(!state.runtime.has_completed_governed_events());
            assert!(!state.output.is_empty());
            assert!(state.output_outstanding);
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_input_resize(1), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }

    #[test]
    fn governed_page_store_admits_workspace_at_exact_peak_and_rejects_peak_minus_one() {
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
        assert_eq!(clearra_wasm_tiling_solution_page(0, 1), ABI_ERROR);
        assert_eq!(clearra_wasm_output_len(), 0);
        assert_eq!(clearra_wasm_input_resize(1), ABI_ERROR);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_ERROR);

        assert_eq!(clearra_wasm_tiling_solution_release(), ABI_OK);
        ABI_STATE.with(|state| {
            assert!(state.borrow().tiling_solution_page_store.is_none());
        });
        assert_eq!(clearra_wasm_input_resize(1), ABI_OK);
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

        assert_eq!(clearra_wasm_product_page_next(10_000), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.output_outstanding);
            let store = state
                .product_page_store
                .as_ref()
                .expect("page owner retained");
            assert!(store.governed_store_fits());
            assert!(store.governed_output_fits(state.output.capacity()));
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_release(), ABI_OK);
        assert_eq!(clearra_wasm_product_page_available(), 0);
        assert_eq!(clearra_wasm_input_resize(1), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
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
    fn governed_generic_tiling_does_not_forge_pc_tiling_page_authority() {
        let job_id = complete_finite_job(FINITE_GENERIC_TILING_COMMAND);
        assert_eq!(clearra_wasm_drain_job_events(job_id), ABI_OK);
        ABI_STATE.with(|state| {
            let state = state.borrow();
            let output = state
                .governed_output
                .as_ref()
                .expect("finite Build drains through the governed JSON route");
            assert!(output.completed_tiling_solution_page_store().is_none());
            assert!(state.tiling_solution_page_store.is_none());
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
        assert!(state
            .distributed_verifier_last_candidate_count
            .is_some_and(|count| count.exact));

        assert_eq!(
            state.record_distributed_verifier_candidate_count(i32::MAX as usize + 1),
            i32::MAX
        );
        assert!(state
            .distributed_verifier_last_candidate_count
            .is_some_and(|count| !count.exact));

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
    fn standalone_build_verifier_admission_exports_typed_not_executed_report() {
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
            assert!(output.contains("\"code\":\"E_WASM_DISTRIBUTED_VERIFIER_START\""));
            assert!(output.contains("\"solver_executed\":false"));
            assert!(output.contains("\"state\":\"exhausted\""));
            assert!(output.contains("\"reason\":\"memory-budget-exceeded\""));
            assert!(output.contains("\"descriptor_pattern_count\":\"1058400\""));
            assert!(output.contains("\"dense_pattern_count\":\"1058400\""));
            assert!(output.contains("\"required_dense_bytes\":\"132304\""));
            assert!(output.contains("\"required_memory_bytes\":\"17066704\""));
            assert!(output.contains("\"result_completeness\":\"not-executed\""));
        });
        assert_eq!(clearra_wasm_output_release(), ABI_OK);
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
    }
}
