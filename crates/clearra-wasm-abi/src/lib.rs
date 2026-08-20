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
use clearra_wasm::{
    install_pc4_compact_tablebase, release_pc4_compact_tablebase,
    serialize_distributed_final_events, GpuSearchWarmupReport, TilingSolutionPageStore,
    WasmDistributedCoordinator, WasmDistributedFallbackReason, WasmDistributedMode,
    WasmDistributedPreparation, WasmDistributedProducerAdvance, WasmDistributedRequestedBackend,
    WasmDistributedVerifierRuntime, WasmHostCapabilities, WasmWorkerAdvanceStatus, WasmWorkerJobId,
    WasmWorkerJobRuntime,
};

const ABI_VERSION: u32 = 1;
const MAX_COMMAND_BYTES: usize = 1024 * 1024;
const MAX_TRANSFER_BYTES: usize = 512 * 1024 * 1024;
const ABI_OK: i32 = 0;
const ABI_ERROR: i32 = -1;
const HOST_CAPABILITY_WEBGPU: u32 = 1 << 0;
const HOST_CAPABILITY_CROSS_ORIGIN_ISOLATED: u32 = 1 << 1;

#[derive(Default)]
struct WasmAbiState {
    runtime: WasmWorkerJobRuntime,
    input: Vec<u8>,
    transfer_input: Vec<u8>,
    output: Vec<u8>,
    distributed_coordinator: Option<WasmDistributedCoordinator>,
    distributed_verifier: Option<WasmDistributedVerifierRuntime>,
    distributed_verifier_partial_available: bool,
    distributed_verifier_pending_work: bool,
    distributed_verifier_last_candidate_count: Option<AbiI32Count>,
    tiling_solution_page_store: Option<Arc<TilingSolutionPageStore>>,
    gpu_warmup: Option<GpuWarmupState>,
    gpu_warmup_generation: u64,
    #[cfg(feature = "stage-profiling")]
    profile: Option<ExecutorSearchProfileSession>,
}

enum GpuWarmupState {
    Pending,
    Ready(GpuSearchWarmupReport),
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
        self.gpu_warmup_generation == generation
            && matches!(self.gpu_warmup, Some(GpuWarmupState::Pending))
    }

    fn set_error(&mut self, code: &str, message: impl std::fmt::Display) {
        self.output = format!("{code}: {message}").into_bytes();
    }

    fn set_output(&mut self, output: String) {
        self.output = output.into_bytes();
    }

    fn set_output_bytes(&mut self, output: Vec<u8>) {
        self.output = output;
    }

    fn reset_distributed_state(&mut self) {
        self.distributed_coordinator = None;
        self.distributed_verifier = None;
        self.distributed_verifier_partial_available = false;
        self.distributed_verifier_pending_work = false;
        self.distributed_verifier_last_candidate_count = None;

        // `clear()` keeps the allocation alive. A completed distributed lifecycle must release
        // the transfer allocation back to the WASM allocator so one large shard does not become
        // the permanent retained baseline for later jobs.
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
            .map(|store| AbiU32Count::project(store.len()))
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
    install_panic_diagnostics();
    ABI_STATE.with(|state| {
        state
            .borrow_mut()
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
        if state.gpu_warmup.is_some() {
            return None;
        }
        let device = if device_index < 0 {
            GpuDeviceSelection::Auto
        } else {
            let Ok(index) = u8::try_from(device_index) else {
                state.set_error("E_WASM_GPU_DEVICE", "GPU device index exceeds u8 range");
                return None;
            };
            GpuDeviceSelection::Index(index)
        };
        state
            .begin_gpu_warmup()
            .map(|generation| (device, generation))
    });
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
        state.borrow_mut().cancel_gpu_warmup();
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_gpu_warmup_advance() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
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
            if state.accepts_gpu_warmup_completion(generation) {
                state.gpu_warmup = Some(GpuWarmupState::Ready(report));
            }
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn start_gpu_warmup(device: GpuDeviceSelection, generation: u64) {
    let report = clearra_wasm::prewarm_gpu_search(device);
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.accepts_gpu_warmup_completion(generation) {
            state.gpu_warmup = Some(GpuWarmupState::Ready(report));
        }
    });
}

#[no_mangle]
pub extern "C" fn clearra_wasm_input_resize(byte_len: u32) -> i32 {
    let byte_len = byte_len as usize;
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
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
    clear_panic_diagnostics();
    let input = ABI_STATE.with(|state| std::mem::take(&mut state.borrow_mut().transfer_input));
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
    clear_panic_diagnostics();
    let released = release_pc4_compact_tablebase();
    ABI_STATE.with(|state| state.borrow_mut().output.clear());
    i32::from(released)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_prepare() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.tiling_solution_page_store = None;
        let command_text = match std::str::from_utf8(&state.input) {
            Ok(command_text) => command_text.to_owned(),
            Err(error) => {
                state.set_error("E_WASM_COMMAND_UTF8", error);
                return ABI_ERROR;
            }
        };
        state.distributed_coordinator = None;
        match WasmDistributedCoordinator::prepare(
            state.runtime.command_runtime(),
            command_text.as_str(),
        ) {
            Ok(WasmDistributedPreparation::Serial) | Ok(WasmDistributedPreparation::Ready(_)) => {
                WasmDistributedMode::Serial as i32
            }
            Ok(WasmDistributedPreparation::Coordinator(coordinator)) => {
                let mode = coordinator.mode() as i32;
                state.distributed_coordinator = Some(coordinator);
                mode
            }
            Err(error) => {
                state.set_error(error.code(), error.message());
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
                state.set_error(error.code(), error.message());
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_merge_partial() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let input = std::mem::take(&mut state.transfer_input);
        let outcome = state
            .distributed_coordinator
            .as_mut()
            .ok_or((
                "E_WASM_DISTRIBUTED_STATE",
                "distributed coordinator is not active".to_owned(),
            ))
            .and_then(|coordinator| {
                coordinator
                    .absorb_partial(&input)
                    .map_err(|error| (error.code(), error.message().to_owned()))
            });
        state.transfer_input = input;
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
                state.set_error(error.code(), error.message());
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
        state.tiling_solution_page_store = result.tiling_solution_page_store().cloned();
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
        let Some(store) = state.tiling_solution_page_store.as_ref() else {
            state.set_error(
                "E_WASM_TILING_PAGE_STATE",
                "tiling solution page store is not available",
            );
            return ABI_ERROR;
        };
        let keys = match store.page_keys(offset as usize, (limit as usize).min(MAX_PAGE_SIZE)) {
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
        state.borrow_mut().tiling_solution_page_store = None;
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_cancel() -> i32 {
    ABI_STATE.with(|state| {
        if let Some(coordinator) = state.borrow().distributed_coordinator.as_ref() {
            coordinator.cancel();
        }
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_reset() -> i32 {
    ABI_STATE.with(|state| {
        state.borrow_mut().reset_distributed_state();
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_start() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.output.clear();
        state.begin_distributed_verifier();
        let command_text = match std::str::from_utf8(&state.input) {
            Ok(command_text) => command_text.to_owned(),
            Err(error) => {
                state.set_error("E_WASM_COMMAND_UTF8", error);
                return ABI_ERROR;
            }
        };
        match WasmDistributedVerifierRuntime::prepare(
            state.runtime.command_runtime(),
            command_text.as_str(),
        ) {
            Ok(verifier) => {
                state.distributed_verifier = Some(verifier);
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
pub extern "C" fn clearra_wasm_distributed_forward_verifier_start() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.output.clear();
        state.begin_distributed_verifier();
        let input = std::mem::take(&mut state.transfer_input);
        let outcome = WasmDistributedVerifierRuntime::prepare_forward(
            state.runtime.command_runtime(),
            &input,
        );
        state.transfer_input = input;
        match outcome {
            Ok(verifier) => {
                state.distributed_verifier = Some(verifier);
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
pub extern "C" fn clearra_wasm_distributed_verifier_consume() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.distributed_verifier_last_candidate_count = None;
        let input = std::mem::take(&mut state.transfer_input);
        let outcome = state
            .distributed_verifier
            .as_mut()
            .ok_or((
                "E_WASM_DISTRIBUTED_STATE",
                "distributed verifier is not active".to_owned(),
            ))
            .and_then(|verifier| {
                verifier
                    .consume(&input)
                    .map_err(|error| (error.code(), error.message().to_owned()))
            });
        state.transfer_input = input;
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
                state.set_error(error.code(), error.message());
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
    clear_panic_diagnostics();
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let command_text = match std::str::from_utf8(&state.input) {
            Ok(command_text) => command_text.to_owned(),
            Err(error) => {
                state.set_error("E_WASM_COMMAND_UTF8", error);
                return 0;
            }
        };
        match state.runtime.start_job(&command_text) {
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
        let status = match state
            .runtime
            .advance_job(WasmWorkerJobId::new(job_id.into()), work_budget.max(1))
        {
            Ok(status) => status,
            Err(error) => {
                state.set_error(error.code(), error.message());
                return ABI_ERROR;
            }
        };
        if status == WasmWorkerAdvanceStatus::Completed {
            state.tiling_solution_page_store =
                state.runtime.take_completed_tiling_solution_page_store();
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
        match state
            .runtime
            .cancel_job(WasmWorkerJobId::new(job_id.into()))
        {
            Ok(()) => ABI_OK,
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
        match state
            .runtime
            .drain_events_json(WasmWorkerJobId::new(job_id.into()))
        {
            Ok(output) => {
                state.set_output(output);
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
pub extern "C" fn clearra_wasm_output_ptr() -> u32 {
    ABI_STATE.with(|state| state.borrow().output.as_ptr() as usize as u32)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_output_len() -> u32 {
    ABI_STATE.with(|state| state.borrow().output.len().min(u32::MAX as usize) as u32)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_output_len_exact() -> u32 {
    ABI_STATE.with(|state| u32::try_from(state.borrow().output.len()).is_ok().into())
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
    use super::*;

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
    fn distributed_reset_releases_transfer_capacity_on_every_lifecycle() {
        for byte_len in [2 * 1024 * 1024, 257, 4 * 1024 * 1024] {
            ABI_STATE.with(|state| {
                let mut state = state.borrow_mut();
                state.transfer_input = Vec::with_capacity(byte_len);
                state.transfer_input.resize(byte_len, 0x5a);
                state.distributed_verifier_partial_available = true;
                state.distributed_verifier_pending_work = true;
                state.record_distributed_verifier_candidate_count(7);
                assert!(state.transfer_input.capacity() >= byte_len);
            });

            assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
            ABI_STATE.with(|state| {
                let state = state.borrow();
                assert!(state.transfer_input.is_empty());
                assert_eq!(state.transfer_input.capacity(), 0);
                assert!(!state.distributed_verifier_partial_available);
                assert!(!state.distributed_verifier_pending_work);
                assert!(state.distributed_verifier_last_candidate_count.is_none());
            });

            assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);
            ABI_STATE.with(|state| assert_eq!(state.borrow().transfer_input.capacity(), 0));
        }
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
    fn exported_count_metadata_distinguishes_unavailable_legacy_defaults() {
        assert_eq!(clearra_wasm_distributed_reset(), ABI_OK);

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
}
