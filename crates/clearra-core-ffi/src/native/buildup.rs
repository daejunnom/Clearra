use crate::{
    buildup::{CBuildUpTraceStep, CKickEvidenceView},
    problem::CBuildUpProblem,
};
use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken,
    resource::{ExecutionAvailability, ExecutionAvailabilityReason, ResourceReport},
};

pub use crate::raw::buildup_types::{CNativeBuildVariantBuffer, CNativeBuildVariantView};

use super::{
    NativeCoreError, C_NATIVE_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT,
    C_NATIVE_BUILDUP_MAX_OPERATIONS,
};

#[cfg(feature = "native-c-core")]
use super::CoreCNative;
#[cfg(all(test, feature = "native-c-core"))]
use super::CLEARRA_CORE_ABI_VERSION_EXPECTED;

#[cfg(all(test, feature = "native-c-core"))]
use std::cell::Cell;

pub const C_BUILDUP_STATUS_OK: i32 = 0;
pub const C_BUILDUP_STATUS_INVALID_ARGUMENT: i32 = 1;
pub const C_BUILDUP_STATUS_INVALID_PROBLEM: i32 = 2;
pub const C_BUILDUP_STATUS_INVALID_ORDER: i32 = 3;
pub const C_BUILDUP_STATUS_LOGICAL_REJECT_MIN: i32 = 4;
pub const C_BUILDUP_STATUS_HOLD_DISABLED_IMPOSSIBLE: i32 = 9;
pub const C_BUILDUP_STATUS_LOGICAL_REJECT_MAX: i32 = 13;
pub const C_BUILDUP_STATUS_CAPACITY_EXCEEDED: i32 = 14;
pub const C_BUILDUP_STATUS_KICK_EVIDENCE_BUFFER_EXHAUSTED: i32 = 15;
pub const C_BUILDUP_STATUS_ENUMERATION_TRUNCATED: i32 = 16;
pub const C_BUILDUP_STATUS_UNSUPPORTED_RUNTIME_SCOPE: i32 = 17;
pub const C_BUILDUP_STATUS_CANCELLED: i32 = 18;
pub const CLR_BUILDUP_CAPACITY_EXCEEDED: i32 = C_BUILDUP_STATUS_CAPACITY_EXCEEDED;
pub const CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED: i32 = C_BUILDUP_STATUS_KICK_EVIDENCE_BUFFER_EXHAUSTED;
pub const CLR_BUILDUP_ENUMERATION_TRUNCATED: i32 = C_BUILDUP_STATUS_ENUMERATION_TRUNCATED;
pub const CLR_BUILDUP_MODE_VERIFY_FIRST: u32 = 1;
pub const CLR_BUILDUP_MODE_ENUMERATE_VARIANTS: u32 = 2;
pub const CLR_BUILDUP_MODE_COUNT_VARIANTS: u32 = 3;
pub const CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CNativeBuildUpSearchMetrics {
    pub expanded_state_count: u64,
    pub memo_probes: u64,
    pub memo_hits: u64,
    pub memo_insertions: u64,
    pub memo_saturation_skips: u64,
    pub memo_capacity: u32,
    pub memo_max_probe_length: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CNativeBuildUpEnumerationLimits {
    pub max_variants: u32,
    pub preserve_hold_branches: u8,
    pub prefer_highest_t_spin_trace: u8,
    pub reserved: [u8; 6],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CNativeBuildUpCountLimits {
    pub max_variants: u32,
    pub preserve_hold_branches: u8,
    pub retain_traces: u8,
    pub reserved: [u8; 6],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CNativeBuildUpCountReport {
    pub total_variant_count: u64,
    pub search_complete: u8,
    pub solution_exists: u8,
    pub count_complete: u8,
    pub trace_retained: u8,
    pub retained_variant_count: u16,
    pub reserved: u16,
    pub no_variant_reason: u32,
    pub truncation_reason: u32,
    pub search_metrics: CNativeBuildUpSearchMetrics,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CNativeBuildUpVerification {
    pub accepted: u8,
    pub reserved: u8,
    pub rejected_step: u16,
    pub reject_reason: u32,
    pub variant: CNativeBuildVariantView,
    pub kick_evidence_storage: [CKickEvidenceView; C_NATIVE_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT],
    pub operation_order_storage: [u16; C_NATIVE_BUILDUP_MAX_OPERATIONS],
    pub trace_step_storage: [CBuildUpTraceStep; C_NATIVE_BUILDUP_MAX_OPERATIONS],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeBuildUpOutcome {
    pub status: i32,
    pub verification: CNativeBuildUpVerification,
    pub buffer: Box<CNativeBuildVariantBuffer>,
}

impl NativeBuildUpOutcome {
    pub fn accepted(&self) -> bool {
        self.status == C_BUILDUP_STATUS_OK
            && (self.verification.accepted != 0 || self.buffer.count > 0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeBuildUpCountOutcome {
    pub status: i32,
    pub report: CNativeBuildUpCountReport,
}

pub struct NativeBuildUpWorkspace {
    buffer: Box<CNativeBuildVariantBuffer>,
    #[cfg(feature = "native-c-core")]
    abi_status: Result<(), NativeCoreError>,
    #[cfg(feature = "native-c-core")]
    native_workspace: Option<crate::raw::buildup_workspace::RawBuildUpWorkspace>,
}

#[cfg(all(test, feature = "native-c-core"))]
thread_local! {
    static TEST_WORKSPACE_ABI_OVERRIDE: Cell<Option<i32>> = const { Cell::new(None) };
    static TEST_WORKSPACE_RAW_C_ENTRY_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(feature = "native-c-core")]
fn ensure_workspace_abi() -> Result<(), NativeCoreError> {
    #[cfg(test)]
    if let Some(actual) = TEST_WORKSPACE_ABI_OVERRIDE.with(Cell::get) {
        return if actual == CLEARRA_CORE_ABI_VERSION_EXPECTED {
            Ok(())
        } else {
            Err(NativeCoreError::AbiMismatch {
                expected: CLEARRA_CORE_ABI_VERSION_EXPECTED,
                actual,
            })
        };
    }

    CoreCNative::ensure_abi()
}

#[cfg(feature = "native-c-core")]
fn record_workspace_raw_c_entry() {
    #[cfg(test)]
    TEST_WORKSPACE_RAW_C_ENTRY_COUNT.with(|count| count.set(count.get().saturating_add(1)));
}

#[cfg(all(test, feature = "native-c-core"))]
struct TestWorkspaceAbiOverrideReset {
    previous_abi: Option<i32>,
    previous_entry_count: usize,
}

#[cfg(all(test, feature = "native-c-core"))]
impl Drop for TestWorkspaceAbiOverrideReset {
    fn drop(&mut self) {
        TEST_WORKSPACE_ABI_OVERRIDE.with(|value| value.set(self.previous_abi));
        TEST_WORKSPACE_RAW_C_ENTRY_COUNT.with(|count| count.set(self.previous_entry_count));
    }
}

#[cfg(all(test, feature = "native-c-core"))]
fn with_test_workspace_abi_override(test_abi: i32, test: impl FnOnce()) -> usize {
    let reset = TestWorkspaceAbiOverrideReset {
        previous_abi: TEST_WORKSPACE_ABI_OVERRIDE.with(|value| value.replace(Some(test_abi))),
        previous_entry_count: TEST_WORKSPACE_RAW_C_ENTRY_COUNT.with(|count| count.replace(0)),
    };
    test();
    let entry_count = TEST_WORKSPACE_RAW_C_ENTRY_COUNT.with(Cell::get);
    drop(reset);
    entry_count
}

pub struct NativeBuildUpWorkspaceOutcome<'a> {
    pub status: i32,
    pub buffer: &'a CNativeBuildVariantBuffer,
}

impl NativeBuildUpWorkspaceOutcome<'_> {
    pub fn accepted(&self) -> bool {
        self.status == C_BUILDUP_STATUS_OK && self.buffer.count > 0
    }
}

impl NativeBuildUpWorkspace {
    pub const fn host_buffer_allocation_bytes() -> u128 {
        std::mem::size_of::<CNativeBuildVariantBuffer>() as u128
    }

    /// Constructs the workspace only after the known Rust host buffer fits,
    /// then measures the opaque native owner immediately. An over-cap native
    /// workspace is dropped before it can reach a worker or a cache.
    pub fn try_new_with_memory_limit(
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<Self, NativeCoreError> {
        let projected_minimum = already_retained_bytes
            .checked_add(Self::host_buffer_allocation_bytes())
            .ok_or_else(|| workspace_memory_error(u128::MAX))?;
        ensure_workspace_memory_limit(projected_minimum, max_memory_bytes)?;

        let workspace = Self::new();
        #[cfg(feature = "native-c-core")]
        {
            workspace.abi_status.clone()?;
            if workspace.native_workspace.is_none() {
                return Err(workspace_memory_error(projected_minimum.saturating_add(1)));
            }
        }
        let retained_bytes = workspace
            .checked_retained_bytes()
            .ok_or_else(|| workspace_memory_error(u128::MAX))?;
        let observed_total = already_retained_bytes
            .checked_add(retained_bytes)
            .ok_or_else(|| workspace_memory_error(u128::MAX))?;
        ensure_workspace_memory_limit(observed_total, max_memory_bytes)?;
        Ok(workspace)
    }

    pub fn new() -> Self {
        #[cfg(feature = "native-c-core")]
        let abi_status = ensure_workspace_abi();
        #[cfg(feature = "native-c-core")]
        let native_workspace = if abi_status.is_ok() {
            record_workspace_raw_c_entry();
            crate::raw::buildup_workspace::RawBuildUpWorkspace::create()
        } else {
            None
        };
        Self {
            buffer: crate::raw::buildup_buffer::zeroed_build_variant_buffer(),
            #[cfg(feature = "native-c-core")]
            abi_status,
            #[cfg(feature = "native-c-core")]
            native_workspace,
        }
    }

    pub fn retained_bytes(&self) -> usize {
        let buffer_bytes = self.host_buffer_bytes();
        #[cfg(feature = "native-c-core")]
        {
            if self.abi_status.is_err() {
                return buffer_bytes;
            }
            let native_bytes = self.native_workspace.as_ref().map_or(0, |workspace| {
                record_workspace_raw_c_entry();
                workspace.retained_bytes()
            });
            buffer_bytes.saturating_add(native_bytes)
        }
        #[cfg(not(feature = "native-c-core"))]
        buffer_bytes
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        let buffer_bytes = Self::host_buffer_allocation_bytes();
        #[cfg(feature = "native-c-core")]
        {
            if self.abi_status.is_err() {
                return Some(buffer_bytes);
            }
            let native_bytes = self.native_workspace.as_ref().map_or(0, |workspace| {
                record_workspace_raw_c_entry();
                workspace.retained_bytes()
            });
            buffer_bytes.checked_add(native_bytes as u128)
        }
        #[cfg(not(feature = "native-c-core"))]
        Some(buffer_bytes)
    }

    pub const fn host_buffer_bytes(&self) -> usize {
        std::mem::size_of::<CNativeBuildVariantBuffer>()
    }

    #[cfg(feature = "native-c-core")]
    pub(crate) fn raw_handle(
        &mut self,
    ) -> Result<crate::raw::buildup_workspace::RawBuildUpWorkspaceHandle<'_>, NativeCoreError> {
        self.ensure_abi()?;
        self.native_workspace
            .as_mut()
            .map(crate::raw::buildup_workspace::RawBuildUpWorkspace::handle)
            .ok_or(NativeCoreError::Unavailable)
    }

    #[cfg(feature = "native-c-core")]
    fn ensure_abi(&self) -> Result<(), NativeCoreError> {
        self.abi_status.clone()
    }

    #[cfg(feature = "native-c-core")]
    pub fn buildup_exists_with_cancellation(
        &mut self,
        problem: &CBuildUpProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<i32, NativeCoreError> {
        self.ensure_abi()?;
        record_workspace_raw_c_entry();
        let _execution_control =
            crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
        let status = self
            .native_workspace
            .as_mut()
            .ok_or(NativeCoreError::Unavailable)?
            .exists(problem);
        if status == C_BUILDUP_STATUS_CANCELLED || cancellation.is_cancelled() {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        Ok(status)
    }

    #[cfg(not(feature = "native-c-core"))]
    pub fn buildup_exists_with_cancellation(
        &mut self,
        _problem: &CBuildUpProblem,
        _cancellation: &ExecutionCancellationToken,
    ) -> Result<i32, NativeCoreError> {
        Err(NativeCoreError::Unavailable)
    }

    #[cfg(feature = "native-c-core")]
    pub fn verify_first_buildup_problem_with_cancellation(
        &mut self,
        problem: &CBuildUpProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<NativeBuildUpWorkspaceOutcome<'_>, NativeCoreError> {
        self.ensure_abi()?;
        record_workspace_raw_c_entry();
        let _execution_control =
            crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
        let status = match self.native_workspace.as_mut() {
            Some(workspace) => workspace.verify_first(problem, &mut self.buffer),
            None => crate::raw::bindings::verify_first_buildup_problem(problem, &mut self.buffer),
        };
        if status == C_BUILDUP_STATUS_CANCELLED || cancellation.is_cancelled() {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        Ok(NativeBuildUpWorkspaceOutcome {
            status,
            buffer: &self.buffer,
        })
    }

    #[cfg(not(feature = "native-c-core"))]
    pub fn verify_first_buildup_problem_with_cancellation(
        &mut self,
        _problem: &CBuildUpProblem,
        _cancellation: &ExecutionCancellationToken,
    ) -> Result<NativeBuildUpWorkspaceOutcome<'_>, NativeCoreError> {
        let _ = &self.buffer;
        Err(NativeCoreError::Unavailable)
    }

    #[cfg(feature = "native-c-core")]
    pub fn enumerate_buildup_variants_with_cancellation(
        &mut self,
        problem: &CBuildUpProblem,
        limits: &CNativeBuildUpEnumerationLimits,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<NativeBuildUpWorkspaceOutcome<'_>, NativeCoreError> {
        self.ensure_abi()?;
        record_workspace_raw_c_entry();
        let _execution_control =
            crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
        let status = match self.native_workspace.as_mut() {
            Some(workspace) => workspace.enumerate(problem, limits, &mut self.buffer),
            None => {
                crate::raw::bindings::enumerate_buildup_variants(problem, limits, &mut self.buffer)
            }
        };
        if status == C_BUILDUP_STATUS_CANCELLED || cancellation.is_cancelled() {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        Ok(NativeBuildUpWorkspaceOutcome {
            status,
            buffer: &self.buffer,
        })
    }

    #[cfg(not(feature = "native-c-core"))]
    pub fn enumerate_buildup_variants_with_cancellation(
        &mut self,
        _problem: &CBuildUpProblem,
        _limits: &CNativeBuildUpEnumerationLimits,
        _cancellation: &ExecutionCancellationToken,
    ) -> Result<NativeBuildUpWorkspaceOutcome<'_>, NativeCoreError> {
        let _ = &self.buffer;
        Err(NativeCoreError::Unavailable)
    }

    #[cfg(feature = "native-c-core")]
    pub fn export_geometry_language_with_cancellation(
        &mut self,
        problem: &CBuildUpProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<super::BuildUpGeometryLanguage, NativeCoreError> {
        self.ensure_abi()?;
        record_workspace_raw_c_entry();
        let _execution_control =
            crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
        let workspace = self
            .native_workspace
            .as_mut()
            .ok_or(NativeCoreError::Unavailable)?;
        let language = super::buildup_geometry_language::export_with_workspace(workspace, problem)
            .map_err(NativeCoreError::BuildUpStatus)?;
        if cancellation.is_cancelled() {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        Ok(language)
    }

    #[cfg(feature = "native-c-core")]
    pub fn export_geometry_language_v2_with_cancellation(
        &mut self,
        problem: &CBuildUpProblem,
        transition_mode: super::BuildUpGeometryTransitionMode,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<super::BuildUpGeometryLanguageV2, NativeCoreError> {
        self.ensure_abi()?;
        record_workspace_raw_c_entry();
        let _execution_control =
            crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
        let workspace = self
            .native_workspace
            .as_mut()
            .ok_or(NativeCoreError::Unavailable)?;
        let language = super::buildup_geometry_language::export_v2_with_workspace(
            workspace,
            problem,
            transition_mode,
        )
        .map_err(NativeCoreError::BuildUpStatus)?;
        if cancellation.is_cancelled() {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        Ok(language)
    }

    #[cfg(not(feature = "native-c-core"))]
    pub fn export_geometry_language_with_cancellation(
        &mut self,
        _problem: &CBuildUpProblem,
        _cancellation: &ExecutionCancellationToken,
    ) -> Result<super::BuildUpGeometryLanguage, NativeCoreError> {
        Err(NativeCoreError::Unavailable)
    }

    #[cfg(not(feature = "native-c-core"))]
    pub fn export_geometry_language_v2_with_cancellation(
        &mut self,
        _problem: &CBuildUpProblem,
        _transition_mode: super::BuildUpGeometryTransitionMode,
        _cancellation: &ExecutionCancellationToken,
    ) -> Result<super::BuildUpGeometryLanguageV2, NativeCoreError> {
        Err(NativeCoreError::Unavailable)
    }
}

fn workspace_memory_error(required_memory_bytes: u128) -> NativeCoreError {
    NativeCoreError::packing_incomplete(
        C_BUILDUP_STATUS_CAPACITY_EXCEEDED,
        ResourceReport::admission_failure(
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded)
                .with_required_memory_bytes(required_memory_bytes),
        ),
    )
}

fn ensure_workspace_memory_limit(
    required_memory_bytes: u128,
    max_memory_bytes: u128,
) -> Result<(), NativeCoreError> {
    if required_memory_bytes > max_memory_bytes {
        return Err(workspace_memory_error(required_memory_bytes));
    }
    Ok(())
}

impl Default for NativeBuildUpWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod memory_contract_tests {
    use super::*;

    #[test]
    fn workspace_host_buffer_is_rejected_before_native_construction() {
        let required = NativeBuildUpWorkspace::host_buffer_allocation_bytes();
        let error = ensure_workspace_memory_limit(required, required - 1)
            .expect_err("one byte below the host buffer must fail");
        let NativeCoreError::PackingIncomplete {
            resource_report, ..
        } = error
        else {
            panic!("expected typed resource failure");
        };
        assert_eq!(
            resource_report
                .execution_availability()
                .required_memory_bytes(),
            Some(required)
        );
        assert!(!resource_report.execution_started());
        assert!(!resource_report.result_complete());
    }

    #[test]
    fn workspace_projection_adds_existing_retained_bytes_once() {
        let host = NativeBuildUpWorkspace::host_buffer_allocation_bytes();
        assert!(ensure_workspace_memory_limit(host + 7, host + 7).is_ok());
        assert!(ensure_workspace_memory_limit(host + 7, host + 6).is_err());
    }
}

#[cfg(feature = "native-c-core")]
mod linked {
    use super::*;

    pub(crate) fn verify_buildup_problem(
        problem: &CBuildUpProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<NativeBuildUpOutcome, NativeCoreError> {
        let _execution_control =
            crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
        let mut buffer = crate::raw::buildup_buffer::zeroed_build_variant_buffer();
        let mut verification = CNativeBuildUpVerification::default();
        let status =
            crate::raw::bindings::verify_buildup_problem(problem, &mut buffer, &mut verification);
        if status == C_BUILDUP_STATUS_CANCELLED || cancellation.is_cancelled() {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        Ok(NativeBuildUpOutcome {
            status,
            verification,
            buffer,
        })
    }

    pub(crate) fn verify_first_buildup_problem(
        problem: &CBuildUpProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<NativeBuildUpOutcome, NativeCoreError> {
        let _execution_control =
            crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
        let mut buffer = crate::raw::buildup_buffer::zeroed_build_variant_buffer();
        let status = crate::raw::bindings::verify_first_buildup_problem(problem, &mut buffer);
        if status == C_BUILDUP_STATUS_CANCELLED || cancellation.is_cancelled() {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        Ok(NativeBuildUpOutcome {
            status,
            verification: CNativeBuildUpVerification::default(),
            buffer,
        })
    }

    pub(crate) fn enumerate_buildup_variants(
        problem: &CBuildUpProblem,
        limits: &CNativeBuildUpEnumerationLimits,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<NativeBuildUpOutcome, NativeCoreError> {
        let _execution_control =
            crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
        let mut buffer = crate::raw::buildup_buffer::zeroed_build_variant_buffer();
        let status = crate::raw::bindings::enumerate_buildup_variants(problem, limits, &mut buffer);
        if status == C_BUILDUP_STATUS_CANCELLED || cancellation.is_cancelled() {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        Ok(NativeBuildUpOutcome {
            status,
            verification: CNativeBuildUpVerification::default(),
            buffer,
        })
    }

    pub(crate) fn count_buildup_variants(
        problem: &CBuildUpProblem,
        limits: &CNativeBuildUpCountLimits,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<NativeBuildUpCountOutcome, NativeCoreError> {
        let _execution_control =
            crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
        let mut report = CNativeBuildUpCountReport::default();
        let status = crate::raw::bindings::count_buildup_variants(problem, limits, &mut report);
        if status == C_BUILDUP_STATUS_CANCELLED || cancellation.is_cancelled() {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        Ok(NativeBuildUpCountOutcome { status, report })
    }
}

#[cfg(feature = "native-c-core")]
pub(crate) use linked::{
    count_buildup_variants, enumerate_buildup_variants, verify_buildup_problem,
    verify_first_buildup_problem,
};

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn verify_buildup_problem(
    _problem: &CBuildUpProblem,
    _cancellation: &ExecutionCancellationToken,
) -> Result<NativeBuildUpOutcome, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn verify_first_buildup_problem(
    _problem: &CBuildUpProblem,
    _cancellation: &ExecutionCancellationToken,
) -> Result<NativeBuildUpOutcome, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn enumerate_buildup_variants(
    _problem: &CBuildUpProblem,
    _limits: &CNativeBuildUpEnumerationLimits,
    _cancellation: &ExecutionCancellationToken,
) -> Result<NativeBuildUpOutcome, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn count_buildup_variants(
    _problem: &CBuildUpProblem,
    _limits: &CNativeBuildUpCountLimits,
    _cancellation: &ExecutionCancellationToken,
) -> Result<NativeBuildUpCountOutcome, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(test)]
#[path = "buildup_tests.rs"]
mod tests;
