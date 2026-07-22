use std::alloc::{alloc_zeroed, handle_alloc_error, Layout};

use super::buildup_types::CNativeBuildVariantBuffer;

pub(crate) fn zeroed_build_variant_buffer() -> Box<CNativeBuildVariantBuffer> {
    let layout = Layout::new::<CNativeBuildVariantBuffer>();
    // The C ABI defines the all-zero representation as an empty output buffer.
    // Allocate directly on the heap so the large fixed storage never lands on
    // the Rust stack.
    let pointer = unsafe { alloc_zeroed(layout) as *mut CNativeBuildVariantBuffer };
    if pointer.is_null() {
        handle_alloc_error(layout);
    }
    unsafe { Box::from_raw(pointer) }
}
