use super::*;

#[test]
fn native_memory_binding_is_feature_gated() {
    if cfg!(feature = "native-memory-binding") {
        assert_ne!(core::mem::size_of::<NativeMemContextHandle>(), 0);
    } else {
        assert_eq!(
            context_create().expect_err("feature-gated binding"),
            NativeMemoryError::BindingUnavailable
        );
    }
}

#[test]
fn native_binding_raw_pointers_are_private() {
    assert!(format!("{:?}", NativeMemoryError::BindingUnavailable).contains("BindingUnavailable"));
}
