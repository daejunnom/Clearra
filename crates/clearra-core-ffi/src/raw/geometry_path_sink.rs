#![cfg(feature = "native-c-core")]

use std::{
    ffi::c_void,
    panic::{catch_unwind, AssertUnwindSafe},
};

use crate::native::{
    NativeGeometryPathConsumer, NativeGeometryPathSinkError, C_NATIVE_GEOMETRY_PATH_MAX_OPERATIONS,
};

const PACKING_OK: i32 = 0;
const PACKING_INVALID_ARGUMENT: i32 = 1;

#[repr(C)]
pub(crate) struct CNativeGeometryPathView {
    skeleton_row_ids: *const u32,
    operation_count: u8,
    reserved: [u8; 7],
}

type CNativeGeometryPathConsumer =
    unsafe extern "C" fn(context: *mut c_void, path: *const CNativeGeometryPathView) -> i32;

#[repr(C)]
pub(crate) struct CNativeGeometryPathSink {
    context: *mut c_void,
    consume: Option<CNativeGeometryPathConsumer>,
}

struct NativeGeometryPathConsumerContext<'a> {
    consumer: &'a mut dyn NativeGeometryPathConsumer,
}

pub(crate) struct NativeGeometryPathSinkHandle<'a> {
    raw: CNativeGeometryPathSink,
    _context: Box<NativeGeometryPathConsumerContext<'a>>,
}

impl<'a> NativeGeometryPathSinkHandle<'a> {
    pub(crate) fn new(consumer: &'a mut dyn NativeGeometryPathConsumer) -> Self {
        let mut context = Box::new(NativeGeometryPathConsumerContext { consumer });
        Self {
            raw: CNativeGeometryPathSink {
                context: (&mut *context as *mut NativeGeometryPathConsumerContext<'a>)
                    .cast::<c_void>(),
                consume: Some(consume_path),
            },
            _context: context,
        }
    }

    pub(crate) fn as_mut(&mut self) -> &mut CNativeGeometryPathSink {
        &mut self.raw
    }
}

unsafe extern "C" fn consume_path(
    context: *mut c_void,
    path: *const CNativeGeometryPathView,
) -> i32 {
    if context.is_null() || path.is_null() {
        return PACKING_INVALID_ARGUMENT;
    }
    let path = unsafe { &*path };
    let operation_count = usize::from(path.operation_count);
    if operation_count == 0
        || operation_count > C_NATIVE_GEOMETRY_PATH_MAX_OPERATIONS
        || path.skeleton_row_ids.is_null()
    {
        return PACKING_INVALID_ARGUMENT;
    }
    let row_ids = unsafe { std::slice::from_raw_parts(path.skeleton_row_ids, operation_count) };
    let context = unsafe { &mut *context.cast::<NativeGeometryPathConsumerContext<'_>>() };
    match catch_unwind(AssertUnwindSafe(|| context.consumer.consume(row_ids))) {
        Ok(Ok(())) => PACKING_OK,
        Ok(Err(error)) => error.status(),
        Err(_) => PACKING_INVALID_ARGUMENT,
    }
}

impl NativeGeometryPathSinkError {
    const fn status(self) -> i32 {
        match self {
            Self::Invalid => PACKING_INVALID_ARGUMENT,
            Self::CapacityExceeded => 6,
            Self::Cancelled => 7,
        }
    }
}

const _: () = assert!(core::mem::size_of::<CNativeGeometryPathView>() == 16);
const _: () = assert!(core::mem::size_of::<CNativeGeometryPathSink>() == 16);
