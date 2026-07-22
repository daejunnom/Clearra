use std::{fmt, ptr::NonNull};

use super::{
    memory_abi::{CClrMemContext, CClrMemLeakReport, CClrMemStatus, CClrScope, CClrScopeKind},
    native_memory_error::NativeMemoryError,
};

#[cfg_attr(not(feature = "native-memory-binding"), allow(dead_code))]
pub(super) struct NativeMemContextHandle {
    raw: NonNull<CClrMemContext>,
}

impl fmt::Debug for NativeMemContextHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeMemContextHandle")
            .finish_non_exhaustive()
    }
}

impl NativeMemContextHandle {
    #[cfg(feature = "native-memory-binding")]
    fn as_ptr(&self) -> *mut CClrMemContext {
        self.raw.as_ptr()
    }
}

#[cfg_attr(not(feature = "native-memory-binding"), allow(dead_code))]
pub(super) struct NativeScopeHandle {
    raw: NonNull<CClrScope>,
}

impl fmt::Debug for NativeScopeHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeScopeHandle").finish_non_exhaustive()
    }
}

impl NativeScopeHandle {
    #[cfg(feature = "native-memory-binding")]
    fn as_ptr(&self) -> *mut CClrScope {
        self.raw.as_ptr()
    }
}

#[cfg_attr(not(feature = "native-memory-binding"), allow(dead_code))]
fn status_to_result(status: CClrMemStatus) -> Result<(), NativeMemoryError> {
    NativeMemoryError::from_status(status).map_or(Ok(()), Err)
}

#[cfg(feature = "native-memory-binding")]
#[link(name = "clearra_core", kind = "static")]
unsafe extern "C" {
    fn clr_mem_context_create(out_context: *mut *mut CClrMemContext) -> CClrMemStatus;
    fn clr_mem_context_release(context: *mut *mut CClrMemContext) -> CClrMemStatus;
    fn clr_mem_context_leak_report(
        context: *const CClrMemContext,
        out_report: *mut CClrMemLeakReport,
    ) -> CClrMemStatus;
    fn clr_scope_create(
        context: *mut CClrMemContext,
        kind: CClrScopeKind,
        out_scope: *mut *mut CClrScope,
    ) -> CClrMemStatus;
    fn clr_scope_release(scope: *mut CClrScope) -> CClrMemStatus;
}

#[cfg(feature = "native-memory-binding")]
pub(super) fn context_create() -> Result<NativeMemContextHandle, NativeMemoryError> {
    let mut raw = core::ptr::null_mut();
    // SAFETY: `raw` is a valid out pointer and ownership is wrapped in
    // `NativeMemContextHandle` immediately after a successful status.
    status_to_result(unsafe { clr_mem_context_create(&mut raw) })?;
    NonNull::new(raw)
        .map(|raw| NativeMemContextHandle { raw })
        .ok_or(NativeMemoryError::InvalidArgument)
}

#[cfg(not(feature = "native-memory-binding"))]
pub(super) fn context_create() -> Result<NativeMemContextHandle, NativeMemoryError> {
    Err(NativeMemoryError::BindingUnavailable)
}

#[cfg(feature = "native-memory-binding")]
pub(super) fn context_release(handle: NativeMemContextHandle) -> Result<(), NativeMemoryError> {
    let mut raw = handle.raw.as_ptr();
    // SAFETY: the handle owns a live `ClrMemContext` pointer and is consumed.
    status_to_result(unsafe { clr_mem_context_release(&mut raw) })
}

#[cfg(not(feature = "native-memory-binding"))]
pub(super) fn context_release(_handle: NativeMemContextHandle) -> Result<(), NativeMemoryError> {
    Err(NativeMemoryError::BindingUnavailable)
}

#[cfg(feature = "native-memory-binding")]
pub(super) fn leak_report(
    handle: &NativeMemContextHandle,
) -> Result<CClrMemLeakReport, NativeMemoryError> {
    let mut report = CClrMemLeakReport::default();
    // SAFETY: `handle` owns a valid context and `report` is a valid out pointer.
    status_to_result(unsafe { clr_mem_context_leak_report(handle.as_ptr(), &mut report) })?;
    Ok(report)
}

#[cfg(not(feature = "native-memory-binding"))]
pub(super) fn leak_report(
    _handle: &NativeMemContextHandle,
) -> Result<CClrMemLeakReport, NativeMemoryError> {
    Err(NativeMemoryError::BindingUnavailable)
}

#[cfg(feature = "native-memory-binding")]
pub(super) fn scope_create(
    context: &NativeMemContextHandle,
    kind: CClrScopeKind,
) -> Result<NativeScopeHandle, NativeMemoryError> {
    let mut raw = core::ptr::null_mut();
    // SAFETY: `context` owns a valid native context and `raw` is a valid out pointer.
    status_to_result(unsafe { clr_scope_create(context.as_ptr(), kind, &mut raw) })?;
    NonNull::new(raw)
        .map(|raw| NativeScopeHandle { raw })
        .ok_or(NativeMemoryError::InvalidArgument)
}

#[cfg(not(feature = "native-memory-binding"))]
pub(super) fn scope_create(
    _context: &NativeMemContextHandle,
    _kind: CClrScopeKind,
) -> Result<NativeScopeHandle, NativeMemoryError> {
    Err(NativeMemoryError::BindingUnavailable)
}

#[cfg(feature = "native-memory-binding")]
pub(super) fn scope_release(handle: NativeScopeHandle) -> Result<(), NativeMemoryError> {
    // SAFETY: the handle owns a live `ClrScope` pointer and is consumed.
    status_to_result(unsafe { clr_scope_release(handle.as_ptr()) })
}

#[cfg(not(feature = "native-memory-binding"))]
pub(super) fn scope_release(_handle: NativeScopeHandle) -> Result<(), NativeMemoryError> {
    Err(NativeMemoryError::BindingUnavailable)
}

#[cfg(test)]
#[path = "native_memory_bindings_tests.rs"]
mod tests;
