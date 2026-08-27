use std::sync::{Mutex, MutexGuard, PoisonError};

static EXECUTION_RESOURCE_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Serializes tests that exercise the process-wide execution-resource authority.
pub(crate) fn execution_resource_test_guard() -> MutexGuard<'static, ()> {
    EXECUTION_RESOURCE_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}
