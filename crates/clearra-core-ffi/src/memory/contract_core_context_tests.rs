use super::{ContractCoreContext, CoreMemoryError};

#[test]
fn context_create_release_records_context_release() {
    let context = ContractCoreContext::create().expect("context");
    let signal = context.release_signal();

    drop(context);

    assert_eq!(signal.context_releases(), 1);
}

#[test]
fn contract_core_context_drop_records_release_signal() {
    let context = ContractCoreContext::create().expect("context");
    let signal = context.release_signal();

    assert_eq!(context.backend_kind().as_str(), "contract");
    drop(context);

    assert_eq!(signal.context_releases(), 1);
}

#[test]
fn search_scope_raii_drop_calls_release() {
    let context = ContractCoreContext::create().expect("context");
    let signal = context.release_signal();

    {
        let _scope = context.search_scope().expect("search scope");
        assert_eq!(signal.search_scope_releases(), 0);
        assert_eq!(context.leak_report().live_scopes(), 1);
    }

    assert_eq!(signal.search_scope_releases(), 1);
    assert!(context.leak_report().is_zero());
}

#[test]
fn context_release_waits_until_scopes_drop() {
    let context = ContractCoreContext::create().expect("context");
    let signal = context.release_signal();
    let scope = context.search_scope().expect("search scope");

    drop(context);

    assert_eq!(signal.context_releases(), 0);
    assert_eq!(signal.search_scope_releases(), 0);

    drop(scope);

    assert_eq!(signal.search_scope_releases(), 1);
    assert_eq!(signal.context_releases(), 1);
}

#[test]
fn batch_scope_explicit_release_calls_release_once() {
    let context = ContractCoreContext::create().expect("context");
    let signal = context.release_signal();
    let mut scope = context.batch_scope().expect("batch scope");

    scope.release().expect("release");

    assert_eq!(signal.batch_scope_releases(), 1);
    assert_eq!(scope.release(), Err(CoreMemoryError::DoubleRelease));
    assert_eq!(signal.batch_scope_releases(), 1);
    assert!(context.leak_report().is_zero());
}
