use core::ptr::NonNull;

pub(crate) fn copy_native_slice<T: Copy>(pointer: *const T, count: usize) -> Option<Vec<T>> {
    if count == 0 {
        return Some(Vec::new());
    }

    let pointer = NonNull::new(pointer.cast_mut())?;
    // Pointer validity comes from the native output-buffer ownership contract.
    // Callers must bound `count` against the ABI capacity before entering this
    // raw boundary. The data is copied immediately and cannot escape borrowed.
    Some(unsafe { core::slice::from_raw_parts(pointer.as_ptr(), count).to_vec() })
}
