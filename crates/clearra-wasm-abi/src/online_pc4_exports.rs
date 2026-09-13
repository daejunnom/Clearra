//! Scalar ABI for the host-driven PC4 Range handshake. Existing job state,
//! cancellation, result events and output leases remain authoritative.
use super::*;

#[no_mangle]
pub extern "C" fn clearra_wasm_online_pc4_configure() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if state.has_worker_job_start_conflict() {
            return ABI_ERROR;
        }
        let json = match String::from_utf8(std::mem::take(&mut state.input)) {
            Ok(json) => json,
            Err(_) => {
                state.set_error("pc4_online_configuration_invalid", "invalid UTF-8");
                return ABI_ERROR;
            }
        };
        match state.runtime.configure_online_pc4(&json) {
            Ok(()) => ABI_OK,
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_online_pc4_pending(job_id: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_worker_lifecycle_admission() {
            return status;
        }
        let json = state
            .runtime
            .online_pc4_pending_json(WasmWorkerJobId::new(job_id.into()));
        state.output = json.into_bytes();
        state.output_outstanding = true;
        ABI_OK
    })
}

#[no_mangle]
pub extern "C" fn clearra_wasm_online_pc4_admit(job_id: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_worker_lifecycle_admission() {
            return status;
        }
        let json = match String::from_utf8(std::mem::take(&mut state.input)) {
            Ok(json) => json,
            Err(_) => {
                state.set_error("pc4_online_response_invalid", "invalid UTF-8");
                return ABI_ERROR;
            }
        };
        match state
            .runtime
            .online_pc4_admit_json(WasmWorkerJobId::new(job_id.into()), &json)
        {
            Ok(()) => ABI_OK,
            Err(error) => {
                state.set_runtime_error(&error);
                ABI_ERROR
            }
        }
    })
}
