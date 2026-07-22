use super::{contract_core_context::CoreLeakReport, memory_abi::CClrMemLeakReport};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NativeLeakReport {
    inner: CClrMemLeakReport,
}

impl NativeLeakReport {
    pub fn from_abi(inner: CClrMemLeakReport) -> Self {
        Self { inner }
    }
}
impl NativeLeakReport {
    pub fn as_abi(self) -> CClrMemLeakReport {
        self.inner
    }
}
impl NativeLeakReport {
    pub fn to_core_leak_report(self) -> CoreLeakReport {
        CoreLeakReport {
            live_search_scopes: saturating_usize(self.inner.live_scopes),
            live_batch_scopes: 0,
        }
    }
}
impl NativeLeakReport {
    pub fn to_diagnostic_material(self) -> NativeMemoryDiagnosticMaterial {
        NativeMemoryDiagnosticMaterial {
            live_scopes: self.inner.live_scopes,
            live_allocations: self.inner.live_allocations,
            live_gpu_buffers: self.inner.live_gpu_buffers,
            pending_release_queue: self.inner.pending_release_queue,
            pending_gpu_buffer_releases: self.inner.pending_gpu_buffer_releases,
            double_releases: self.inner.double_releases,
            canary_failures: self.inner.canary_failures,
            poison_detections: self.inner.poison_detections,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NativeMemoryDiagnosticMaterial {
    pub live_scopes: u64,
    pub live_allocations: u64,
    pub live_gpu_buffers: u64,
    pub pending_release_queue: u64,
    pub pending_gpu_buffer_releases: u64,
    pub double_releases: u64,
    pub canary_failures: u64,
    pub poison_detections: u64,
}

fn saturating_usize(value: u64) -> usize {
    value.min(usize::MAX as u64) as usize
}

#[cfg(test)]
#[path = "native_leak_report_tests.rs"]
mod tests;
