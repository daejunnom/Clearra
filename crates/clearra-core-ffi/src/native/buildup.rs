use crate::{
    buildup::{CBuildUpTraceStep, CKickEvidenceView},
    problem::CBuildUpProblem,
};
use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;

pub use crate::raw::buildup_types::{CNativeBuildVariantBuffer, CNativeBuildVariantView};

use super::{
    NativeCoreError, C_NATIVE_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT,
    C_NATIVE_BUILDUP_MAX_OPERATIONS,
};

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
    native_workspace: Option<crate::raw::buildup_workspace::RawBuildUpWorkspace>,
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
    pub fn new() -> Self {
        Self {
            buffer: crate::raw::buildup_buffer::zeroed_build_variant_buffer(),
            #[cfg(feature = "native-c-core")]
            native_workspace: crate::raw::buildup_workspace::RawBuildUpWorkspace::create(),
        }
    }

    pub fn retained_bytes(&self) -> usize {
        let buffer_bytes = self.host_buffer_bytes();
        #[cfg(feature = "native-c-core")]
        {
            return buffer_bytes.saturating_add(self.native_workspace.as_ref().map_or(
                0,
                crate::raw::buildup_workspace::RawBuildUpWorkspace::retained_bytes,
            ));
        }
        #[cfg(not(feature = "native-c-core"))]
        buffer_bytes
    }

    pub const fn host_buffer_bytes(&self) -> usize {
        std::mem::size_of::<CNativeBuildVariantBuffer>()
    }

    #[cfg(feature = "native-c-core")]
    pub(crate) fn raw_pointer(&mut self) -> Result<*mut core::ffi::c_void, NativeCoreError> {
        self.native_workspace
            .as_mut()
            .map(crate::raw::buildup_workspace::RawBuildUpWorkspace::as_mut_ptr)
            .ok_or(NativeCoreError::Unavailable)
    }

    #[cfg(feature = "native-c-core")]
    pub fn buildup_exists_with_cancellation(
        &mut self,
        problem: &CBuildUpProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<i32, NativeCoreError> {
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

impl Default for NativeBuildUpWorkspace {
    fn default() -> Self {
        Self::new()
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
