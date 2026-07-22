use super::{NativeCoreContext, NativeMemoryBindingStatus};
use crate::memory::{MemoryBackendKind, NativeMemoryError};

#[test]
fn native_memory_wrapper_is_explicit_skeleton_until_ffi_binding_exists() {
    if cfg!(feature = "native-memory-binding") {
        assert_eq!(
            NativeCoreContext::backend_kind(),
            MemoryBackendKind::NativeBound
        );
        assert_eq!(
            NativeCoreContext::binding_status(),
            NativeMemoryBindingStatus::NativeBound
        );
    } else {
        assert_eq!(
            NativeCoreContext::backend_kind(),
            MemoryBackendKind::NativeSkeleton
        );
        assert_eq!(
            NativeCoreContext::binding_status(),
            NativeMemoryBindingStatus::NativeMemoryBindingUnavailable
        );
        assert_eq!(
            NativeCoreContext::create().expect_err("not bound yet"),
            NativeMemoryError::BindingUnavailable
        );
    }
}

#[test]
fn native_core_context_default_build_returns_binding_unavailable() {
    if !cfg!(feature = "native-memory-binding") {
        assert_eq!(
            NativeCoreContext::create().expect_err("default build is unbound"),
            NativeMemoryError::BindingUnavailable
        );
    }
}

#[test]
fn native_core_context_drop_releases_c_mem_context() {
    if cfg!(feature = "native-memory-binding") {
        let context = NativeCoreContext::create().expect("native context");
        assert_eq!(
            context
                .leak_report()
                .expect("leak report")
                .as_abi()
                .live_scopes,
            0
        );
        drop(context);
    } else {
        assert_eq!(
            NativeCoreContext::create().expect_err("not bound yet"),
            NativeMemoryError::BindingUnavailable
        );
    }
}

#[test]
fn native_core_context_explicit_release_then_drop_does_not_double_free() {
    if cfg!(feature = "native-memory-binding") {
        let mut context = NativeCoreContext::create().expect("native context");
        context.release().expect("explicit release");
        assert_eq!(
            context.leak_report().expect_err("released context"),
            NativeMemoryError::DoubleRelease
        );
        drop(context);
    } else {
        assert_eq!(
            NativeCoreContext::create().expect_err("not bound yet"),
            NativeMemoryError::BindingUnavailable
        );
    }
}
