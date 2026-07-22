use core::{marker::PhantomData, ops::Deref};

use super::{
    memory_abi::CClrScopeKind,
    memory_backend_kind::MemoryBackendKind,
    native_core_context::{NativeCoreContext, NativeMemoryBindingStatus},
    native_memory_bindings::{self, NativeScopeHandle},
    native_memory_error::NativeMemoryError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeScopeKind {
    Search,
    Batch,
    Worker,
    GpuTransfer,
}

impl NativeScopeKind {
    pub fn as_abi(self) -> CClrScopeKind {
        match self {
            Self::Search => CClrScopeKind::Search,
            Self::Batch => CClrScopeKind::Batch,
            Self::Worker => CClrScopeKind::Worker,
            Self::GpuTransfer => CClrScopeKind::GpuTransfer,
        }
    }
}

#[derive(Debug)]
pub struct BorrowedNativeView<'scope, T> {
    value: T,
    _scope: PhantomData<&'scope ()>,
}

impl<T> Deref for BorrowedNativeView<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[derive(Debug)]
pub struct NativeSearchScope<'ctx> {
    handle: Option<NativeScopeHandle>,
    _context: PhantomData<&'ctx NativeCoreContext>,
}

impl<'ctx> NativeSearchScope<'ctx> {
    pub fn backend_kind() -> MemoryBackendKind {
        if cfg!(feature = "native-memory-binding") {
            MemoryBackendKind::NativeBound
        } else {
            MemoryBackendKind::NativeSkeleton
        }
    }
}
impl<'ctx> NativeSearchScope<'ctx> {
    pub fn binding_status() -> NativeMemoryBindingStatus {
        NativeCoreContext::binding_status()
    }
}
impl<'ctx> NativeSearchScope<'ctx> {
    pub fn create(context: &'ctx NativeCoreContext) -> Result<Self, NativeMemoryError> {
        let handle = native_memory_bindings::scope_create(
            context.handle()?,
            NativeScopeKind::Search.as_abi(),
        )?;
        Ok(Self {
            handle: Some(handle),
            _context: PhantomData,
        })
    }
}
impl<'ctx> NativeSearchScope<'ctx> {
    pub fn release(&mut self) -> Result<(), NativeMemoryError> {
        let handle = self.handle.take().ok_or(NativeMemoryError::DoubleRelease)?;
        native_memory_bindings::scope_release(handle)
    }
}
impl<'ctx> NativeSearchScope<'ctx> {
    pub fn borrowed_view<T>(&self, value: T) -> BorrowedNativeView<'_, T> {
        BorrowedNativeView {
            value,
            _scope: PhantomData,
        }
    }
}

impl Drop for NativeSearchScope<'_> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = native_memory_bindings::scope_release(handle);
        }
    }
}

#[derive(Debug)]
pub struct NativeBatchScope<'ctx> {
    handle: Option<NativeScopeHandle>,
    _context: PhantomData<&'ctx NativeCoreContext>,
}

impl<'ctx> NativeBatchScope<'ctx> {
    pub fn backend_kind() -> MemoryBackendKind {
        if cfg!(feature = "native-memory-binding") {
            MemoryBackendKind::NativeBound
        } else {
            MemoryBackendKind::NativeSkeleton
        }
    }
}
impl<'ctx> NativeBatchScope<'ctx> {
    pub fn binding_status() -> NativeMemoryBindingStatus {
        NativeCoreContext::binding_status()
    }
}
impl<'ctx> NativeBatchScope<'ctx> {
    pub fn create(context: &'ctx NativeCoreContext) -> Result<Self, NativeMemoryError> {
        let handle = native_memory_bindings::scope_create(
            context.handle()?,
            NativeScopeKind::Batch.as_abi(),
        )?;
        Ok(Self {
            handle: Some(handle),
            _context: PhantomData,
        })
    }
}
impl<'ctx> NativeBatchScope<'ctx> {
    pub fn release(&mut self) -> Result<(), NativeMemoryError> {
        let handle = self.handle.take().ok_or(NativeMemoryError::DoubleRelease)?;
        native_memory_bindings::scope_release(handle)
    }
}
impl<'ctx> NativeBatchScope<'ctx> {
    pub fn borrowed_view<T>(&self, value: T) -> BorrowedNativeView<'_, T> {
        BorrowedNativeView {
            value,
            _scope: PhantomData,
        }
    }
}

impl Drop for NativeBatchScope<'_> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = native_memory_bindings::scope_release(handle);
        }
    }
}

#[cfg(test)]
#[path = "native_scope_tests.rs"]
mod tests;
