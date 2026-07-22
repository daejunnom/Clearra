use clearra_core_ffi::CoreContext;

use super::ScopeGuard;

#[test]
fn search_scope_guard_releases_on_drop() {
    let context = CoreContext::create().expect("context");
    let signal = context.release_signal();

    {
        let _guard = ScopeGuard::search(&context).expect("guard");
        assert_eq!(signal.search_scope_releases(), 0);
    }

    assert_eq!(signal.search_scope_releases(), 1);
    assert!(context.leak_report().is_zero());
}

#[test]
fn batch_scope_guard_releases_on_drop() {
    let context = CoreContext::create().expect("context");
    let signal = context.release_signal();

    {
        let _guard = ScopeGuard::batch(&context).expect("guard");
        assert_eq!(signal.batch_scope_releases(), 0);
    }

    assert_eq!(signal.batch_scope_releases(), 1);
    assert!(context.leak_report().is_zero());
}
