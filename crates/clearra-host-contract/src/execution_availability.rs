#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub enum ExecutionAvailabilityState {
    Available,
    Unavailable,
    Deferred,
    Exhausted,
    Cancelled,
    Incomplete,
}

impl ExecutionAvailabilityState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
            Self::Deferred => "deferred",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub enum ExecutionAvailabilityReason {
    NotExecuted,
    CapabilityUnavailable,
    PatternCountAddressSpaceExceeded,
    DensePatternRepresentationUnavailable,
    ComputeBudgetExceeded,
    MemoryBudgetExceeded,
    SharedResourceContention,
    CancelledByCaller,
    PartialExecution,
}

impl ExecutionAvailabilityReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotExecuted => "not-executed",
            Self::CapabilityUnavailable => "capability-unavailable",
            Self::PatternCountAddressSpaceExceeded => "pattern-count-address-space-exceeded",
            Self::DensePatternRepresentationUnavailable => {
                "dense-pattern-representation-unavailable"
            }
            Self::ComputeBudgetExceeded => "compute-budget-exceeded",
            Self::MemoryBudgetExceeded => "memory-budget-exceeded",
            Self::SharedResourceContention => "shared-resource-contention",
            Self::CancelledByCaller => "cancelled-by-caller",
            Self::PartialExecution => "partial-execution",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub enum ExecutionCompletenessState {
    NotExecuted,
    Complete,
    Incomplete,
}

impl ExecutionCompletenessState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotExecuted => "not-executed",
            Self::Complete => "complete",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub enum ExecutionSurface {
    Native,
    BrowserWasm32,
    Unknown,
}

impl ExecutionSurface {
    pub const fn current() -> Self {
        if cfg!(target_family = "wasm") {
            Self::BrowserWasm32
        } else {
            Self::Native
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::BrowserWasm32 => "browser-wasm32",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExecutionAvailabilityReport {
    state: ExecutionAvailabilityState,
    reason: Option<ExecutionAvailabilityReason>,
    surface: ExecutionSurface,
    descriptor_pattern_count: Option<String>,
    dense_pattern_count: Option<String>,
    required_dense_bytes: Option<String>,
    required_memory_bytes: Option<String>,
}

impl ExecutionAvailabilityReport {
    pub const fn available(surface: ExecutionSurface) -> Self {
        Self {
            state: ExecutionAvailabilityState::Available,
            reason: None,
            surface,
            descriptor_pattern_count: None,
            dense_pattern_count: None,
            required_dense_bytes: None,
            required_memory_bytes: None,
        }
    }

    /// Allocation-free owned-parts seam for a boundary that has already
    /// authorized the optional decimal evidence strings.
    #[allow(clippy::too_many_arguments)]
    pub fn from_owned_memory_authorized_parts(
        state: ExecutionAvailabilityState,
        reason: Option<ExecutionAvailabilityReason>,
        surface: ExecutionSurface,
        descriptor_pattern_count: Option<String>,
        dense_pattern_count: Option<String>,
        required_dense_bytes: Option<String>,
        required_memory_bytes: Option<String>,
    ) -> Self {
        Self {
            state,
            reason,
            surface,
            descriptor_pattern_count,
            dense_pattern_count,
            required_dense_bytes,
            required_memory_bytes,
        }
    }

    pub const fn unavailable(
        surface: ExecutionSurface,
        reason: ExecutionAvailabilityReason,
    ) -> Self {
        Self::non_available(ExecutionAvailabilityState::Unavailable, surface, reason)
    }

    pub const fn deferred(surface: ExecutionSurface, reason: ExecutionAvailabilityReason) -> Self {
        Self::non_available(ExecutionAvailabilityState::Deferred, surface, reason)
    }

    pub const fn exhausted(surface: ExecutionSurface, reason: ExecutionAvailabilityReason) -> Self {
        Self::non_available(ExecutionAvailabilityState::Exhausted, surface, reason)
    }

    pub const fn cancelled(surface: ExecutionSurface) -> Self {
        Self::non_available(
            ExecutionAvailabilityState::Cancelled,
            surface,
            ExecutionAvailabilityReason::CancelledByCaller,
        )
    }

    pub const fn incomplete(
        surface: ExecutionSurface,
        reason: ExecutionAvailabilityReason,
    ) -> Self {
        Self::non_available(ExecutionAvailabilityState::Incomplete, surface, reason)
    }

    pub const fn not_executed(surface: ExecutionSurface) -> Self {
        Self::unavailable(surface, ExecutionAvailabilityReason::NotExecuted)
    }

    const fn non_available(
        state: ExecutionAvailabilityState,
        surface: ExecutionSurface,
        reason: ExecutionAvailabilityReason,
    ) -> Self {
        Self {
            state,
            reason: Some(reason),
            surface,
            descriptor_pattern_count: None,
            dense_pattern_count: None,
            required_dense_bytes: None,
            required_memory_bytes: None,
        }
    }

    pub fn with_pattern_evidence(
        mut self,
        descriptor_pattern_count: u128,
        dense_pattern_count: u128,
        required_dense_bytes: u128,
    ) -> Self {
        self.descriptor_pattern_count = Some(descriptor_pattern_count.to_string());
        self.dense_pattern_count = Some(dense_pattern_count.to_string());
        self.required_dense_bytes = Some(required_dense_bytes.to_string());
        self
    }

    /// Allocation-free owned-string seam for a caller that has already
    /// created and memory-authorized the canonical decimal evidence.
    pub fn with_owned_pattern_evidence(
        mut self,
        descriptor_pattern_count: String,
        dense_pattern_count: String,
        required_dense_bytes: String,
    ) -> Self {
        self.descriptor_pattern_count = Some(descriptor_pattern_count);
        self.dense_pattern_count = Some(dense_pattern_count);
        self.required_dense_bytes = Some(required_dense_bytes);
        self
    }

    pub fn with_required_memory_bytes(mut self, required_memory_bytes: u128) -> Self {
        self.required_memory_bytes = Some(required_memory_bytes.to_string());
        self
    }

    /// Allocation-free owned-string seam paired with
    /// [`ExecutionAvailabilityReport::with_owned_pattern_evidence`].
    pub fn with_owned_required_memory_bytes(mut self, required_memory_bytes: String) -> Self {
        self.required_memory_bytes = Some(required_memory_bytes);
        self
    }

    pub const fn state(&self) -> ExecutionAvailabilityState {
        self.state
    }

    pub const fn reason(&self) -> Option<ExecutionAvailabilityReason> {
        self.reason
    }

    pub const fn surface(&self) -> ExecutionSurface {
        self.surface
    }

    pub fn descriptor_pattern_count(&self) -> Option<&str> {
        self.descriptor_pattern_count.as_deref()
    }

    pub fn dense_pattern_count(&self) -> Option<&str> {
        self.dense_pattern_count.as_deref()
    }

    pub fn required_dense_bytes(&self) -> Option<&str> {
        self.required_dense_bytes.as_deref()
    }

    pub fn required_memory_bytes(&self) -> Option<&str> {
        self.required_memory_bytes.as_deref()
    }

    /// Returns only the heap payload retained by optional decimal evidence
    /// strings, measured from their actual allocation capacities. Enum fields
    /// and inline owners are excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = 0_u128;
        for value in [
            &self.descriptor_pattern_count,
            &self.dense_pattern_count,
            &self.required_dense_bytes,
            &self.required_memory_bytes,
        ] {
            if let Some(value) = value {
                bytes = bytes.checked_add(value.capacity() as u128)?;
            }
        }
        Some(bytes)
    }
}

impl Default for ExecutionAvailabilityReport {
    fn default() -> Self {
        Self::not_executed(ExecutionSurface::current())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_uses_canonical_surface_state_and_decimal_evidence() {
        let report = ExecutionAvailabilityReport::unavailable(
            ExecutionSurface::BrowserWasm32,
            ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
        )
        .with_pattern_evidence(35_384_428_800, 35_384_428_800, 4_423_053_600)
        .with_required_memory_bytes(4_423_053_600);
        let value = serde_json::to_value(report).expect("serialize report");

        assert_eq!(value["state"], "unavailable");
        assert_eq!(value["reason"], "pattern-count-address-space-exceeded");
        assert_eq!(value["surface"], "browser-wasm32");
        assert_eq!(value["descriptor_pattern_count"], "35384428800");
        assert_eq!(value["required_dense_bytes"], "4423053600");
        assert_eq!(value["required_memory_bytes"], "4423053600");
    }

    #[test]
    fn native_contract_consumes_shared_availability_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/contracts/execution_resource_authority.v1.json"
        ))
        .expect("shared execution resource fixture");
        assert_eq!(
            fixture["schema_version"],
            "clearra.execution-resource-authority.v1"
        );

        let states = fixture["availability_cases"]
            .as_array()
            .expect("availability cases")
            .iter()
            .map(|entry| entry["state"].as_str().expect("state"))
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            vec![
                "available",
                "unavailable",
                "deferred",
                "exhausted",
                "cancelled",
                "incomplete",
            ]
        );

        let dense = fixture["dense_pattern_cases"]
            .as_array()
            .expect("dense cases");
        let native = dense
            .iter()
            .find(|entry| entry["id"] == "six-line-native-dense")
            .expect("native 6L fixture");
        assert_eq!(native["descriptor_pattern_count"], "35384428800");
        assert_eq!(native["required_dense_bytes"], "4423053600");
        assert_eq!(native["state"], "unavailable");
        assert_eq!(native["reason"], "dense-pattern-representation-unavailable");
        assert_eq!(native["result_completeness"], "not-executed");

        let wasm = dense
            .iter()
            .find(|entry| entry["id"] == "six-line-browser-wasm32")
            .expect("browser wasm32 6L fixture");
        assert_eq!(wasm["state"], "unavailable");
        assert_eq!(wasm["reason"], "pattern-count-address-space-exceeded");
        assert_eq!(wasm["result_completeness"], "not-executed");
    }

    #[test]
    fn retained_capacity_counts_all_decimal_evidence_strings() {
        let report = ExecutionAvailabilityReport::exhausted(
            ExecutionSurface::BrowserWasm32,
            ExecutionAvailabilityReason::MemoryBudgetExceeded,
        )
        .with_pattern_evidence(35_384_428_800, 35_384_428_800, 4_423_053_600)
        .with_required_memory_bytes(9_999_999_999);
        let expected = [
            report.descriptor_pattern_count.as_ref(),
            report.dense_pattern_count.as_ref(),
            report.required_dense_bytes.as_ref(),
            report.required_memory_bytes.as_ref(),
        ]
        .into_iter()
        .flatten()
        .try_fold(0_u128, |bytes, value| {
            bytes.checked_add(value.capacity() as u128)
        });

        assert_eq!(report.checked_retained_capacity_bytes(), expected);
    }
}
