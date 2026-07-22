use clearra_pc_graph::request::WorkerPolicy;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WasmHostCapabilities {
    logical_processor_count: usize,
    webgpu_available: bool,
    cross_origin_isolated: bool,
}

impl WasmHostCapabilities {
    pub fn new(
        logical_processor_count: usize,
        webgpu_available: bool,
        cross_origin_isolated: bool,
    ) -> Self {
        Self {
            logical_processor_count: logical_processor_count.max(1),
            webgpu_available,
            cross_origin_isolated,
        }
    }

    pub const fn logical_processor_count(self) -> usize {
        self.logical_processor_count
    }

    pub const fn webgpu_available(self) -> bool {
        self.webgpu_available
    }

    pub const fn cross_origin_isolated(self) -> bool {
        self.cross_origin_isolated
    }
}

impl Default for WasmHostCapabilities {
    fn default() -> Self {
        Self::new(
            WorkerPolicy::hardware_worker_limit(),
            cfg!(all(feature = "webgpu-search", not(target_arch = "wasm32"))),
            false,
        )
    }
}
