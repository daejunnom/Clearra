#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type HostInstant = Instant;
#[cfg(target_arch = "wasm32")]
pub(crate) type HostInstant = f64;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now_ms() -> f64;
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn host_now() -> HostInstant {
    Instant::now()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn host_now() -> HostInstant {
    performance_now_ms()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn host_elapsed_ns(started_at: HostInstant) -> u64 {
    u64::try_from(started_at.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn host_elapsed_ns(started_at: HostInstant) -> u64 {
    ((performance_now_ms() - started_at).max(0.0) * 1_000_000.0).min(u64::MAX as f64) as u64
}
