#[repr(C)]
pub struct CClrMemContext {
    _private: [u8; 0],
}

#[repr(C)]
pub struct CClrScope {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CClrMemStatus {
    Ok = 0,
    InvalidArgument = 1,
    OutOfMemory = 2,
    DoubleRelease = 3,
    Aborted = 4,
    CanaryCorrupted = 5,
    DebugPoisoned = 6,
    NotFound = 7,
    InvalidState = 8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CClrScopeKind {
    Search = 1,
    Batch = 2,
    Worker = 3,
    GpuTransfer = 4,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CClrMemLeakReport {
    pub live_scopes: u64,
    pub live_allocations: u64,
    pub live_gpu_buffers: u64,
    pub pending_release_queue: u64,
    pub pending_gpu_buffer_releases: u64,
    pub released_scopes: u64,
    pub aborted_scopes: u64,
    pub double_releases: u64,
    pub canary_failures: u64,
    pub poison_detections: u64,
}
