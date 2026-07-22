use super::{NativeBatchScope, NativeScopeKind};
use crate::{
    buildup::CPatternBitSet,
    memory::{CClrScopeKind, MemoryBackendKind, NativeCoreContext, NativeMemoryError},
};

#[test]
fn native_scope_kind_maps_to_c_scope_kind() {
    assert_eq!(NativeScopeKind::Search.as_abi(), CClrScopeKind::Search);
    assert_eq!(NativeScopeKind::Batch.as_abi(), CClrScopeKind::Batch);
    assert_eq!(NativeScopeKind::Worker.as_abi(), CClrScopeKind::Worker);
    assert_eq!(
        NativeScopeKind::GpuTransfer.as_abi(),
        CClrScopeKind::GpuTransfer
    );
}

#[test]
fn native_search_scope_drop_releases_c_scope() {
    if cfg!(feature = "native-memory-binding") {
        let context = NativeCoreContext::create().expect("native context");
        {
            let _scope = context.search_scope().expect("search scope");
            assert_eq!(
                context
                    .leak_report()
                    .expect("leak report")
                    .as_abi()
                    .live_scopes,
                1
            );
        }
        assert_eq!(
            context
                .leak_report()
                .expect("leak report")
                .as_abi()
                .live_scopes,
            0
        );
    } else {
        assert_eq!(
            NativeBatchScope::backend_kind(),
            MemoryBackendKind::NativeSkeleton
        );
    }
}

#[test]
fn native_batch_scope_drop_releases_c_scope() {
    if cfg!(feature = "native-memory-binding") {
        let context = NativeCoreContext::create().expect("native context");
        {
            let _scope = context.batch_scope().expect("batch scope");
            assert_eq!(
                context
                    .leak_report()
                    .expect("leak report")
                    .as_abi()
                    .live_scopes,
                1
            );
        }
        assert_eq!(
            context
                .leak_report()
                .expect("leak report")
                .as_abi()
                .live_scopes,
            0
        );
    } else {
        let context = NativeCoreContext::create().expect_err("not bound yet");
        assert_eq!(context, NativeMemoryError::BindingUnavailable);
    }
}

#[test]
fn borrowed_view_cannot_escape_scope() {
    if cfg!(feature = "native-memory-binding") {
        let context = NativeCoreContext::create().expect("native context");
        let scope = context.batch_scope().expect("batch scope");
        let view = scope.borrowed_view(7_u32);
        assert_eq!(*view, 7);
    } else {
        assert_eq!(
            NativeCoreContext::create().expect_err("not bound yet"),
            NativeMemoryError::BindingUnavailable
        );
    }
}

#[test]
fn owned_snapshot_survives_scope_release() {
    let bitset = CPatternBitSet::single_with_identity(7, 9, 128, 65).expect("bitset");
    let snapshot = bitset.owned_snapshot().expect("owned snapshot");

    if cfg!(feature = "native-memory-binding") {
        let context = NativeCoreContext::create().expect("native context");
        let mut scope = context.batch_scope().expect("batch scope");
        scope.release().expect("scope release");
    }

    assert_eq!(snapshot.pattern_universe_id(), 7);
    assert_eq!(snapshot.pattern_weight_model_id(), 9);
    assert_eq!(snapshot.pattern_count(), 128);
    assert_eq!(snapshot.words(), &[0, 2]);
}
