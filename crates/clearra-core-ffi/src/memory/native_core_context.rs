use super::{
    memory_backend_kind::MemoryBackendKind,
    native_leak_report::NativeLeakReport,
    native_memory_bindings::{self, NativeMemContextHandle},
    native_memory_error::NativeMemoryError,
    native_scope::{NativeBatchScope, NativeSearchScope},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMemoryBindingStatus {
    NativeMemoryBindingUnavailable,
    NativeMemoryBindingFeatureEnabled,
    NativeBound,
}

/// Safe RAII facade over the C `clr_mem_context_*` API.
///
/// Raw C pointers are private to `native_memory_bindings.rs`. Product code only
/// receives `NativeCoreContext`, `NativeSearchScope`, `NativeBatchScope`, and
/// owned `NativeLeakReport` values. Without the `native-memory-binding` feature,
/// this remains an explicit skeleton and returns `BindingUnavailable`.
#[derive(Debug)]
pub struct NativeCoreContext {
    handle: Option<NativeMemContextHandle>,
}

impl NativeCoreContext {
    pub fn backend_kind() -> MemoryBackendKind {
        if cfg!(feature = "native-memory-binding") {
            MemoryBackendKind::NativeBound
        } else {
            MemoryBackendKind::NativeSkeleton
        }
    }
}
impl NativeCoreContext {
    pub fn binding_status() -> NativeMemoryBindingStatus {
        if cfg!(feature = "native-memory-binding") {
            NativeMemoryBindingStatus::NativeBound
        } else {
            NativeMemoryBindingStatus::NativeMemoryBindingUnavailable
        }
    }
}
impl NativeCoreContext {
    pub fn create() -> Result<Self, NativeMemoryError> {
        Self::try_create()
    }
}
impl NativeCoreContext {
    pub fn try_create() -> Result<Self, NativeMemoryError> {
        native_memory_bindings::context_create().map(|handle| Self {
            handle: Some(handle),
        })
    }
}
impl NativeCoreContext {
    pub fn leak_report(&self) -> Result<NativeLeakReport, NativeMemoryError> {
        let handle = self
            .handle
            .as_ref()
            .ok_or(NativeMemoryError::DoubleRelease)?;
        native_memory_bindings::leak_report(handle).map(NativeLeakReport::from_abi)
    }
}
impl NativeCoreContext {
    pub fn release(&mut self) -> Result<(), NativeMemoryError> {
        let handle = self.handle.take().ok_or(NativeMemoryError::DoubleRelease)?;
        native_memory_bindings::context_release(handle)
    }
}
impl NativeCoreContext {
    pub fn search_scope(&self) -> Result<NativeSearchScope<'_>, NativeMemoryError> {
        NativeSearchScope::create(self)
    }
}
impl NativeCoreContext {
    pub fn batch_scope(&self) -> Result<NativeBatchScope<'_>, NativeMemoryError> {
        NativeBatchScope::create(self)
    }
}
impl NativeCoreContext {
    pub(super) fn handle(&self) -> Result<&NativeMemContextHandle, NativeMemoryError> {
        self.handle.as_ref().ok_or(NativeMemoryError::DoubleRelease)
    }
}

impl Drop for NativeCoreContext {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = native_memory_bindings::context_release(handle);
        }
    }
}

#[cfg(test)]
#[path = "native_core_context_tests.rs"]
mod tests;
