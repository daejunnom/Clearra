use std::marker::PhantomData;

use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;

use crate::native::NativeCoreError;

const CANCELLATION_CHECK_INTERVAL: u32 = 256;

#[repr(C)]
pub(crate) struct CNativeExecutionControl {
    cancelled: *const u32,
    check_interval: u32,
}

pub(crate) struct NativeExecutionControlGuard<'a> {
    _token: PhantomData<&'a ExecutionCancellationToken>,
}

impl<'a> NativeExecutionControlGuard<'a> {
    pub(crate) fn install(token: &'a ExecutionCancellationToken) -> Result<Self, NativeCoreError> {
        let control = CNativeExecutionControl {
            cancelled: token.atomic_flag().as_ptr().cast_const(),
            check_interval: CANCELLATION_CHECK_INTERVAL,
        };
        let status = super::bindings::install_execution_control(&control);
        if status != 0 {
            return Err(NativeCoreError::ExecutionControlStatus(status));
        }
        Ok(Self {
            _token: PhantomData,
        })
    }
}

impl Drop for NativeExecutionControlGuard<'_> {
    fn drop(&mut self) {
        super::bindings::clear_execution_control();
    }
}
