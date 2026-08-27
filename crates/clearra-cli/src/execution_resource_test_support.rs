use std::sync::{Mutex, MutexGuard, PoisonError};

static EXECUTION_RESOURCE_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Serializes CLI tests that enter the process-wide App execution-resource
/// authority. The production authority correctly rejects overlapping owners;
/// unrelated test cases must therefore not manufacture contention by running
/// those complete product executions in parallel.
pub(crate) fn execution_resource_test_guard() -> MutexGuard<'static, ()> {
    EXECUTION_RESOURCE_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}
