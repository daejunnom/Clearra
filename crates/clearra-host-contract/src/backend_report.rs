#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BackendReport {
    backend_requested: String,
    backend_selected: String,
    fallback_used: bool,
    fallback_reason: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    backend_fallback_reason: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    fallback_backend: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    gpu_failure_class: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    gpu_failure_stage: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    discarded_partial_gpu_result: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    gpu_device_requested: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    gpu_device_selected_index: Option<u8>,
    #[cfg_attr(feature = "serde", serde(default))]
    gpu_device_selected_name: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    gpu_device_selected_type: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    gpu_device_selected_backend: Option<String>,
}

impl BackendReport {
    pub fn new(
        backend_requested: impl Into<String>,
        backend_selected: impl Into<String>,
        fallback_reason: Option<impl Into<String>>,
    ) -> Self {
        let backend_requested = backend_requested.into();
        let backend_selected = backend_selected.into();
        let fallback_reason = fallback_reason.map(Into::into);
        Self {
            backend_requested,
            backend_selected: backend_selected.clone(),
            fallback_used: fallback_reason.is_some(),
            backend_fallback_reason: fallback_reason.clone(),
            fallback_backend: fallback_reason.as_ref().map(|_| backend_selected),
            fallback_reason,
            gpu_failure_class: None,
            gpu_failure_stage: None,
            discarded_partial_gpu_result: false,
            gpu_device_requested: None,
            gpu_device_selected_index: None,
            gpu_device_selected_name: None,
            gpu_device_selected_type: None,
            gpu_device_selected_backend: None,
        }
    }

    pub fn with_gpu_execution_failure(
        mut self,
        failure_class: Option<String>,
        failure_stage: Option<String>,
        fallback_backend: Option<String>,
        discarded_partial_gpu_result: bool,
    ) -> Self {
        self.gpu_failure_class = failure_class;
        self.gpu_failure_stage = failure_stage;
        if fallback_backend.is_some() {
            self.fallback_backend = fallback_backend;
        }
        self.discarded_partial_gpu_result = discarded_partial_gpu_result;
        self
    }

    pub fn with_gpu_device(
        mut self,
        requested: Option<String>,
        selected_index: Option<u8>,
        selected_name: Option<String>,
        selected_type: Option<String>,
        selected_backend: Option<String>,
    ) -> Self {
        self.gpu_device_requested = requested;
        self.gpu_device_selected_index = selected_index;
        self.gpu_device_selected_name = selected_name;
        self.gpu_device_selected_type = selected_type;
        self.gpu_device_selected_backend = selected_backend;
        self
    }
}
impl BackendReport {
    pub fn backend_requested(&self) -> &str {
        &self.backend_requested
    }
}
impl BackendReport {
    pub fn backend_selected(&self) -> &str {
        &self.backend_selected
    }
}
impl BackendReport {
    pub fn fallback_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }
}
impl BackendReport {
    pub fn backend_fallback_reason(&self) -> Option<&str> {
        self.backend_fallback_reason
            .as_deref()
            .or(self.fallback_reason.as_deref())
    }

    pub fn fallback_backend(&self) -> Option<&str> {
        self.fallback_backend.as_deref()
    }

    pub fn gpu_failure_class(&self) -> Option<&str> {
        self.gpu_failure_class.as_deref()
    }

    pub fn gpu_failure_stage(&self) -> Option<&str> {
        self.gpu_failure_stage.as_deref()
    }

    pub const fn discarded_partial_gpu_result(&self) -> bool {
        self.discarded_partial_gpu_result
    }

    pub fn gpu_device_requested(&self) -> Option<&str> {
        self.gpu_device_requested.as_deref()
    }

    pub const fn gpu_device_selected_index(&self) -> Option<u8> {
        self.gpu_device_selected_index
    }

    pub fn gpu_device_selected_name(&self) -> Option<&str> {
        self.gpu_device_selected_name.as_deref()
    }

    pub fn gpu_device_selected_type(&self) -> Option<&str> {
        self.gpu_device_selected_type.as_deref()
    }

    pub fn gpu_device_selected_backend(&self) -> Option<&str> {
        self.gpu_device_selected_backend.as_deref()
    }
}
impl BackendReport {
    pub const fn fallback_used(&self) -> bool {
        self.fallback_used
    }
}

impl Default for BackendReport {
    fn default() -> Self {
        Self::new("auto", "none", None::<String>)
    }
}
