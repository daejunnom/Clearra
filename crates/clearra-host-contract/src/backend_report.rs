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

    /// Allocation-free owned-parts seam for a boundary that has already
    /// created and memory-authorized every retained backend string. The
    /// ordinary constructor remains the compatibility API that derives and
    /// clones its fallback fields.
    #[allow(clippy::too_many_arguments)]
    pub fn from_owned_memory_authorized_parts(
        backend_requested: String,
        backend_selected: String,
        fallback_reason: Option<String>,
        backend_fallback_reason: Option<String>,
        fallback_backend: Option<String>,
        gpu_failure_class: Option<String>,
        gpu_failure_stage: Option<String>,
        discarded_partial_gpu_result: bool,
        gpu_device_requested: Option<String>,
        gpu_device_selected_index: Option<u8>,
        gpu_device_selected_name: Option<String>,
        gpu_device_selected_type: Option<String>,
        gpu_device_selected_backend: Option<String>,
    ) -> Self {
        let fallback_used = fallback_reason.is_some();
        Self::from_owned_memory_authorized_parts_strict(
            backend_requested,
            backend_selected,
            fallback_used,
            fallback_reason,
            backend_fallback_reason,
            fallback_backend,
            gpu_failure_class,
            gpu_failure_stage,
            discarded_partial_gpu_result,
            gpu_device_requested,
            gpu_device_selected_index,
            gpu_device_selected_name,
            gpu_device_selected_type,
            gpu_device_selected_backend,
        )
    }

    /// Allocation-free owned-parts seam for a boundary that must preserve an
    /// independently authoritative `fallback_used` bit. Unlike the
    /// compatibility constructor above, this does not infer that bit from the
    /// presence of a reason.
    #[allow(clippy::too_many_arguments)]
    pub fn from_owned_memory_authorized_parts_strict(
        backend_requested: String,
        backend_selected: String,
        fallback_used: bool,
        fallback_reason: Option<String>,
        backend_fallback_reason: Option<String>,
        fallback_backend: Option<String>,
        gpu_failure_class: Option<String>,
        gpu_failure_stage: Option<String>,
        discarded_partial_gpu_result: bool,
        gpu_device_requested: Option<String>,
        gpu_device_selected_index: Option<u8>,
        gpu_device_selected_name: Option<String>,
        gpu_device_selected_type: Option<String>,
        gpu_device_selected_backend: Option<String>,
    ) -> Self {
        Self {
            backend_requested,
            backend_selected,
            fallback_used,
            fallback_reason,
            backend_fallback_reason,
            fallback_backend,
            gpu_failure_class,
            gpu_failure_stage,
            discarded_partial_gpu_result,
            gpu_device_requested,
            gpu_device_selected_index,
            gpu_device_selected_name,
            gpu_device_selected_type,
            gpu_device_selected_backend,
        }
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

    /// Returns only heap payload transitively retained by this report.
    ///
    /// Every owned backend/device string is measured from its actual
    /// allocation capacity. Inline flags, indices, `String` owners, and
    /// `Option` discriminants are excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.backend_requested.capacity() as u128)
            .checked_add(self.backend_selected.capacity() as u128)?;
        for value in [
            &self.fallback_reason,
            &self.backend_fallback_reason,
            &self.fallback_backend,
            &self.gpu_failure_class,
            &self.gpu_failure_stage,
            &self.gpu_device_requested,
            &self.gpu_device_selected_name,
            &self.gpu_device_selected_type,
            &self.gpu_device_selected_backend,
        ]
        .into_iter()
        .flatten()
        {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        Some(bytes)
    }
}

impl Default for BackendReport {
    fn default() -> Self {
        Self::new("auto", "none", None::<String>)
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::BackendReport;

    fn allocated_text(capacity: usize, value: &str) -> String {
        let mut text = String::with_capacity(capacity);
        text.push_str(value);
        text
    }

    #[test]
    fn retained_capacity_counts_every_owned_backend_string() {
        let requested = allocated_text(32, "gpu");
        let selected = allocated_text(48, "cpu");
        let reason = allocated_text(64, "adapter-unavailable");
        let failure_class = allocated_text(80, "adapter-request");
        let failure_stage = allocated_text(96, "device-selection");
        let fallback_backend = allocated_text(112, "wasm-cpu");
        let device_requested = allocated_text(128, "discrete");
        let device_name = allocated_text(144, "fixture adapter");
        let device_type = allocated_text(160, "discrete-gpu");
        let device_backend = allocated_text(176, "vulkan");
        let report = BackendReport::new(requested, selected, Some(reason))
            .with_gpu_execution_failure(
                Some(failure_class),
                Some(failure_stage),
                Some(fallback_backend),
                true,
            )
            .with_gpu_device(
                Some(device_requested),
                Some(3),
                Some(device_name),
                Some(device_type),
                Some(device_backend),
            );
        let expected = [
            Some(&report.backend_requested),
            Some(&report.backend_selected),
            report.fallback_reason.as_ref(),
            report.backend_fallback_reason.as_ref(),
            report.fallback_backend.as_ref(),
            report.gpu_failure_class.as_ref(),
            report.gpu_failure_stage.as_ref(),
            report.gpu_device_requested.as_ref(),
            report.gpu_device_selected_name.as_ref(),
            report.gpu_device_selected_type.as_ref(),
            report.gpu_device_selected_backend.as_ref(),
        ]
        .into_iter()
        .flatten()
        .try_fold(0_u128, |bytes, value| {
            bytes.checked_add(value.capacity() as u128)
        });

        assert_eq!(report.checked_retained_capacity_bytes(), expected);
    }

    #[test]
    fn strict_owned_parts_preserve_fallback_bit_without_a_reason() {
        let report = BackendReport::from_owned_memory_authorized_parts_strict(
            "gpu".to_owned(),
            "cpu".to_owned(),
            true,
            None,
            None,
            Some("cpu".to_owned()),
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(report.fallback_used());
        assert_eq!(report.fallback_reason(), None);
        assert_eq!(report.fallback_backend(), Some("cpu"));
    }
}
