use std::{cell::RefCell, sync::Once};

use clearra_pc_graph::request::GpuDeviceSelection;
#[cfg(target_arch = "wasm32")]
use clearra_wasm::prewarm_gpu_search_async;
#[cfg(feature = "stage-profiling")]
use clearra_wasm::ExecutorSearchProfileSession;
use clearra_wasm::{
    serialize_distributed_final_events, GpuSearchWarmupReport, WasmDistributedCoordinator,
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
    gpu_warmup: Option<GpuWarmupState>,
    #[cfg(feature = "stage-profiling")]
    profile: Option<ExecutorSearchProfileSession>,
}

enum GpuWarmupState {
    Pending,
    Ready(GpuSearchWarmupReport),
}

impl WasmAbiState {
    fn set_error(&mut self, code: &str, message: impl std::fmt::Display) {
        self.output = format!("{code}: {message}").into_bytes();
    }

    fn set_output(&mut self, output: String) {
        self.output = output.into_bytes();
    }

    fn set_output_bytes(&mut self, output: Vec<u8>) {
        self.output = output;
    }
}

thread_local! {
    static ABI_STATE: RefCell<WasmAbiState> = RefCell::new(WasmAbiState::default());
    static LAST_PANIC: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
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
    let device = ABI_STATE.with(|state| {
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
        state.gpu_warmup = Some(GpuWarmupState::Pending);
        Some(device)
    });
    if let Some(device) = device {
        start_gpu_warmup(device);
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
fn start_gpu_warmup(device: GpuDeviceSelection) {
    wasm_bindgen_futures::spawn_local(async move {
        let report = prewarm_gpu_search_async(device).await;
        ABI_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if matches!(state.gpu_warmup, Some(GpuWarmupState::Pending)) {
                state.gpu_warmup = Some(GpuWarmupState::Ready(report));
            }
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn start_gpu_warmup(device: GpuDeviceSelection) {
    let report = clearra_wasm::prewarm_gpu_search(device);
    ABI_STATE.with(|state| {
        state.borrow_mut().gpu_warmup = Some(GpuWarmupState::Ready(report));
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
pub extern "C" fn clearra_wasm_distributed_prepare() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
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
pub extern "C" fn clearra_wasm_distributed_progress_geometry_nodes() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .map_or(0, |coordinator| {
                progress_u32(coordinator.progress().geometry_nodes)
            })
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_candidate_count() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .map_or(0, |coordinator| {
                progress_u32(coordinator.progress().candidates)
            })
    })
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
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .map_or(0, |coordinator| {
                progress_u32(coordinator.progress().pass_index)
            })
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_progress_pass_count() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_coordinator
            .as_ref()
            .map_or(1, |coordinator| {
                progress_u32(coordinator.progress().pass_count.max(1))
            })
    })
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
        match coordinator
            .finish(workers_used as usize)
            .and_then(|result| serialize_distributed_final_events(job_id.into(), &result))
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
        let mut state = state.borrow_mut();
        state.distributed_coordinator = None;
        state.distributed_verifier = None;
        state.distributed_verifier_partial_available = false;
        state.transfer_input.clear();
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_start() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.output.clear();
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
        let input = std::mem::take(&mut state.transfer_input);
        let outcome = WasmDistributedVerifierRuntime::prepare_forward(
            state.runtime.command_runtime(),
            &input,
        );
        state.transfer_input = input;
        match outcome {
            Ok(verifier) => {
                state.distributed_verifier = Some(verifier);
                state.distributed_verifier_partial_available = false;
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
                if let Some(partial) = consumed.partial {
                    state.set_output_bytes(partial);
                }
                i32::try_from(consumed.candidate_count).unwrap_or(i32::MAX)
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
pub extern "C" fn clearra_wasm_distributed_verifier_progress_candidate_count() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_verifier
            .as_ref()
            .map_or(0, |verifier| progress_u32(verifier.progress().candidates))
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_progress_build_nodes() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_verifier
            .as_ref()
            .map_or(0, |verifier| progress_u32(verifier.progress().build_nodes))
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_distributed_verifier_progress_coverage_checks() -> u32 {
    ABI_STATE.with(|state| {
        state
            .borrow()
            .distributed_verifier
            .as_ref()
            .map_or(0, |verifier| {
                progress_u32(verifier.progress().coverage_checks)
            })
    })
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
                ABI_OK
            }
            Err(error) => {
                state.set_error(error.code(), error.message());
                ABI_ERROR
            }
        }
    })
}

fn progress_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
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
        match state
            .runtime
            .advance_job(WasmWorkerJobId::new(job_id.into()), work_budget.max(1))
        {
            Ok(WasmWorkerAdvanceStatus::Pending) => 0,
            Ok(WasmWorkerAdvanceStatus::Completed) => 1,
            Ok(WasmWorkerAdvanceStatus::Cancelled) => 2,
            Ok(WasmWorkerAdvanceStatus::Failed) => 3,
            Err(error) => {
                state.set_error(error.code(), error.message());
                ABI_ERROR
            }
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
pub extern "C" fn clearra_wasm_last_panic_ptr() -> u32 {
    LAST_PANIC.with(|last| last.borrow().as_ptr() as usize as u32)
}

#[no_mangle]
pub extern "C" fn clearra_wasm_last_panic_len() -> u32 {
    LAST_PANIC.with(|last| last.borrow().len().min(u32::MAX as usize) as u32)
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
