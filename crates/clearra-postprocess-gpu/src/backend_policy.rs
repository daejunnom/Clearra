#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchBackendRequest {
    #[default]
    Auto,
    Cpu,
    Gpu,
    Hybrid,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PostBackendRequest {
    #[default]
    Auto,
    Cpu,
    Gpu,
    Hybrid,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackendFallbackPolicy {
    Disabled,
    #[default]
    AllowWithDiagnostic,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GpuDeviceSelection {
    Auto,
    DeviceId(String),
}

impl Default for GpuDeviceSelection {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendPolicy {
    pub search_backend: SearchBackendRequest,
    pub post_backend: PostBackendRequest,
    pub fallback_policy: BackendFallbackPolicy,
    pub gpu_device: GpuDeviceSelection,
    pub deterministic: bool,
}

impl BackendPolicy {
    pub fn new(
        search_backend: SearchBackendRequest,
        post_backend: PostBackendRequest,
        fallback_policy: BackendFallbackPolicy,
        gpu_device: GpuDeviceSelection,
        deterministic: bool,
    ) -> Self {
        Self {
            search_backend,
            post_backend,
            fallback_policy,
            gpu_device,
            deterministic,
        }
    }
}
impl BackendPolicy {
    pub fn search_backend(&self) -> SearchBackendRequest {
        self.search_backend
    }
}
impl BackendPolicy {
    pub fn post_backend(&self) -> PostBackendRequest {
        self.post_backend
    }
}
impl BackendPolicy {
    pub fn with_post_backend(mut self, post_backend: PostBackendRequest) -> Self {
        self.post_backend = post_backend;
        self
    }
}
impl BackendPolicy {
    pub fn search_backend_and_post_backend_are_separate(&self) -> bool {
        let replacement = if self.post_backend == PostBackendRequest::Cpu {
            PostBackendRequest::Gpu
        } else {
            PostBackendRequest::Cpu
        };
        let changed = self.clone().with_post_backend(replacement);

        changed.search_backend == self.search_backend && changed.post_backend != self.post_backend
    }
}

impl Default for BackendPolicy {
    fn default() -> Self {
        Self::new(
            SearchBackendRequest::Auto,
            PostBackendRequest::Auto,
            BackendFallbackPolicy::AllowWithDiagnostic,
            GpuDeviceSelection::Auto,
            true,
        )
    }
}
