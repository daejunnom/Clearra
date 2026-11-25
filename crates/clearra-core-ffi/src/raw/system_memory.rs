//! Windows memory-capacity query isolated at the crate-private FFI boundary.

use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

pub(crate) fn physical_capacity_bytes() -> Option<u64> {
    // SAFETY: this C record contains only integers, is initialized to zero,
    // and remains uniquely borrowed for the synchronous system call.
    let mut memory: MEMORYSTATUSEX = unsafe { core::mem::zeroed() };
    memory.dwLength = core::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: dwLength identifies the complete writable record above.
    (unsafe { GlobalMemoryStatusEx(&mut memory) } != 0).then_some(memory.ullTotalPhys)
}
