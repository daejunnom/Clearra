use crate::native::CNativePackingCandidateBuffer;

pub(crate) fn new_zeroed_packing_candidate_buffer() -> Box<CNativePackingCandidateBuffer> {
    let mut buffer = Box::<CNativePackingCandidateBuffer>::new_uninit();

    // The C ABI buffer contains only integer fields and integer arrays, so an
    // all-zero representation is valid. Allocate first so the large buffer is
    // initialized directly on the heap instead of through a stack temporary.
    unsafe {
        buffer.as_mut_ptr().write_bytes(0, 1);
        buffer.assume_init()
    }
}
