#[cfg(all(feature = "stage-profiling", not(target_arch = "wasm32")))]
use std::time::Instant;
#[cfg(all(feature = "stage-profiling", target_arch = "wasm32"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(feature = "stage-profiling", target_arch = "wasm32"))]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now_ms() -> f64;
}

#[cfg(all(feature = "stage-profiling", target_arch = "wasm32"))]
type WebGpuInstant = f64;
#[cfg(all(feature = "stage-profiling", not(target_arch = "wasm32")))]
type WebGpuInstant = Instant;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WebGpuGeometryExactCoverTimings {
    host_prepare_submit_ns: u64,
    dispatch_counter_wait_ns: u64,
    payload_readback_ns: u64,
    exact_host_reduce_ns: u64,
    trace_enumeration_ns: u64,
    layer_dispatch_count: u64,
    generated_record_count: u64,
}

impl WebGpuGeometryExactCoverTimings {
    pub const fn host_prepare_submit_ns(self) -> u64 {
        self.host_prepare_submit_ns
    }

    pub const fn dispatch_counter_wait_ns(self) -> u64 {
        self.dispatch_counter_wait_ns
    }

    pub const fn payload_readback_ns(self) -> u64 {
        self.payload_readback_ns
    }

    pub const fn exact_host_reduce_ns(self) -> u64 {
        self.exact_host_reduce_ns
    }

    pub const fn trace_enumeration_ns(self) -> u64 {
        self.trace_enumeration_ns
    }

    pub const fn layer_dispatch_count(self) -> u64 {
        self.layer_dispatch_count
    }

    pub const fn generated_record_count(self) -> u64 {
        self.generated_record_count
    }

    pub(crate) fn add_layer(&mut self, layer: WebGpuLayerTiming) {
        self.host_prepare_submit_ns = self
            .host_prepare_submit_ns
            .saturating_add(layer.host_prepare_submit_ns);
        self.dispatch_counter_wait_ns = self
            .dispatch_counter_wait_ns
            .saturating_add(layer.dispatch_counter_wait_ns);
        self.payload_readback_ns = self
            .payload_readback_ns
            .saturating_add(layer.payload_readback_ns);
        self.layer_dispatch_count = self.layer_dispatch_count.saturating_add(1);
        self.generated_record_count = self
            .generated_record_count
            .saturating_add(u64::from(layer.generated_record_count));
    }

    pub(crate) fn add_exact_host_reduce(&mut self, elapsed_ns: u64) {
        self.exact_host_reduce_ns = self.exact_host_reduce_ns.saturating_add(elapsed_ns);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct WebGpuLayerTiming {
    pub(crate) host_prepare_submit_ns: u64,
    pub(crate) dispatch_counter_wait_ns: u64,
    pub(crate) payload_readback_ns: u64,
    pub(crate) generated_record_count: u32,
}

pub(crate) struct WebGpuStageTimer {
    #[cfg(feature = "stage-profiling")]
    started: WebGpuInstant,
}

impl WebGpuStageTimer {
    #[inline]
    pub(crate) fn begin() -> Self {
        Self {
            #[cfg(feature = "stage-profiling")]
            started: webgpu_now(),
        }
    }

    #[inline]
    pub(crate) fn finish_ns(self) -> u64 {
        #[cfg(feature = "stage-profiling")]
        {
            webgpu_elapsed_ns(self.started)
        }
        #[cfg(not(feature = "stage-profiling"))]
        {
            0
        }
    }
}

#[cfg(all(feature = "stage-profiling", target_arch = "wasm32"))]
#[inline]
fn webgpu_now() -> WebGpuInstant {
    performance_now_ms()
}

#[cfg(all(feature = "stage-profiling", not(target_arch = "wasm32")))]
#[inline]
fn webgpu_now() -> WebGpuInstant {
    Instant::now()
}

#[cfg(all(feature = "stage-profiling", target_arch = "wasm32"))]
#[inline]
fn webgpu_elapsed_ns(started: WebGpuInstant) -> u64 {
    ((performance_now_ms() - started).max(0.0) * 1_000_000.0).min(u64::MAX as f64) as u64
}

#[cfg(all(feature = "stage-profiling", not(target_arch = "wasm32")))]
#[inline]
fn webgpu_elapsed_ns(started: WebGpuInstant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
