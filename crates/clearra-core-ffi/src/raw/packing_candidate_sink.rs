#![cfg(feature = "native-c-core")]

use std::{
    ffi::c_void,
    panic::{catch_unwind, AssertUnwindSafe},
};

use crate::{
    native::{
        NativePackingCandidateConsumer, NativePackingCandidateContext, C_NATIVE_PACKING_MAX_PIECES,
    },
    packing_problem::{CPackingCandidate, CPackingOperation, C_PACKING_MAX_OPERATIONS},
};

const PACKING_OK: i32 = 0;
const PACKING_INVALID_ARGUMENT: i32 = 1;
const TRUNCATION_NONE: u16 = 0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CNativePackingCandidateView {
    pub candidate_id: u64,
    pub canonical_operation_set_id: u64,
    pub final_board: u64,
    pub shape_mask: u64,
    pub shape_key: u64,
    pub tiling_key: u64,
    pub operation_set_key: u64,
    pub placed_count: u8,
    pub cleared_lines: u8,
    pub geometry_variant_domains: u16,
    pub pieces: [u8; C_NATIVE_PACKING_MAX_PIECES],
    pub rotations: [u8; C_NATIVE_PACKING_MAX_PIECES],
    pub xs: [i8; C_NATIVE_PACKING_MAX_PIECES],
    pub ys: [i8; C_NATIVE_PACKING_MAX_PIECES],
    pub operation_ids: [u16; C_NATIVE_PACKING_MAX_PIECES],
    pub operation_deleted_row_masks: [u16; C_NATIVE_PACKING_MAX_PIECES],
    pub operation_masks: [u64; C_NATIVE_PACKING_MAX_PIECES],
}

impl CNativePackingCandidateView {
    pub(crate) fn to_candidate(self) -> CPackingCandidate {
        let operation_count = usize::from(self.placed_count).min(C_PACKING_MAX_OPERATIONS);
        let mut candidate = CPackingCandidate {
            candidate_id: self.candidate_id,
            canonical_operation_set_id: self.canonical_operation_set_id,
            final_board: self.final_board,
            shape_mask: self.shape_mask,
            shape_key: self.shape_key,
            tiling_key: self.tiling_key,
            operation_set_key: self.operation_set_key,
            operation_count: operation_count as u16,
            geometry_variant_domains: self.geometry_variant_domains,
            cleared_lines: self.cleared_lines,
            ..Default::default()
        };
        for index in 0..operation_count {
            candidate.operations[index] = CPackingOperation {
                piece: self.pieces[index],
                rotation: self.rotations[index],
                x: self.xs[index],
                y: self.ys[index],
                operation_id: self.operation_ids[index],
                required_deleted_row_mask: self.operation_deleted_row_masks[index],
                mask: self.operation_masks[index],
            };
        }
        candidate
    }
}

type CNativePackingCandidateConsumer = unsafe extern "C" fn(
    context: *mut c_void,
    candidate: *const CNativePackingCandidateView,
    accepted_candidate_count: usize,
    engine_resident_bytes: usize,
    max_candidate_rows: usize,
    max_total_bytes: usize,
    out_inserted: *mut u8,
    out_truncation_reason: *mut u16,
    out_host_resident_bytes: *mut usize,
) -> i32;

#[repr(C)]
pub(crate) struct CNativePackingCandidateSink {
    context: *mut c_void,
    consume: Option<CNativePackingCandidateConsumer>,
}

struct NativeCandidateConsumerContext<'a> {
    consumer: &'a mut dyn NativePackingCandidateConsumer,
}

pub(crate) struct NativeCandidateSinkHandle<'a> {
    raw: CNativePackingCandidateSink,
    _context: Box<NativeCandidateConsumerContext<'a>>,
}

impl<'a> NativeCandidateSinkHandle<'a> {
    pub(crate) fn new(consumer: &'a mut dyn NativePackingCandidateConsumer) -> Self {
        let mut context = Box::new(NativeCandidateConsumerContext { consumer });
        Self {
            raw: CNativePackingCandidateSink {
                context: (&mut *context as *mut NativeCandidateConsumerContext<'a>)
                    .cast::<c_void>(),
                consume: Some(consume_candidate),
            },
            _context: context,
        }
    }

    pub(crate) fn as_mut(&mut self) -> &mut CNativePackingCandidateSink {
        &mut self.raw
    }
}

unsafe extern "C" fn consume_candidate(
    context: *mut c_void,
    candidate: *const CNativePackingCandidateView,
    accepted_candidate_count: usize,
    engine_resident_bytes: usize,
    max_candidate_rows: usize,
    max_total_bytes: usize,
    out_inserted: *mut u8,
    out_truncation_reason: *mut u16,
    out_host_resident_bytes: *mut usize,
) -> i32 {
    if context.is_null()
        || candidate.is_null()
        || out_inserted.is_null()
        || out_truncation_reason.is_null()
        || out_host_resident_bytes.is_null()
        || max_candidate_rows == 0
    {
        return PACKING_INVALID_ARGUMENT;
    }

    let context = unsafe { &mut *context.cast::<NativeCandidateConsumerContext<'_>>() };
    let candidate = unsafe { *candidate }.to_candidate();
    let result = catch_unwind(AssertUnwindSafe(|| {
        context.consumer.consume(
            candidate,
            NativePackingCandidateContext {
                accepted_candidate_count,
                engine_resident_bytes,
                max_candidate_rows,
                max_total_bytes,
            },
        )
    }));
    let (status, inserted, reason) = match result {
        Ok(Ok(inserted)) => (PACKING_OK, inserted, TRUNCATION_NONE),
        Ok(Err(limit)) => (limit.status(), false, limit.truncation_reason()),
        Err(_) => (PACKING_INVALID_ARGUMENT, false, TRUNCATION_NONE),
    };
    unsafe {
        *out_inserted = u8::from(inserted);
        *out_truncation_reason = reason;
        *out_host_resident_bytes = context.consumer.resident_bytes();
    }
    status
}
