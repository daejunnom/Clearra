//! Windows locality deliberately requires complete, single-group permission.
//! Primary-group APIs do not describe all hard masks on modern multi-group
//! systems, and CPU Sets are soft assignments. Ambiguity leaves the OS's
//! existing all-group scheduling intact, without imposing a 64-CPU ceiling.

use std::ffi::c_void;
use std::mem;
use std::ptr;

use super::NativeAffinityError;
use super::NativeNumaNode;
use super::NativeThreadAffinity;

#[repr(C)]
#[derive(Default)]
struct GroupAffinity {
    mask: usize,
    group: u16,
    reserved: [u16; 3],
}

// This OS ABI stays in the existing unsafe FFI crate. It does not require a
// new dependency or a C-core feature. All handles below are pseudo-handles.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn GetCurrentThread() -> *mut c_void;
    fn GetActiveProcessorGroupCount() -> u16;
    fn GetProcessAffinityMask(
        process: *mut c_void,
        process_mask: *mut usize,
        system_mask: *mut usize,
    ) -> i32;
    fn GetThreadGroupAffinity(thread: *mut c_void, affinity: *mut GroupAffinity) -> i32;
    fn SetThreadGroupAffinity(
        thread: *mut c_void,
        affinity: *const GroupAffinity,
        previous: *mut GroupAffinity,
    ) -> i32;
    fn GetNumaHighestNodeNumber(highest: *mut u32) -> i32;
    fn GetNumaNodeProcessorMaskEx(node: u16, affinity: *mut GroupAffinity) -> i32;
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}

type CpuSetQuery = unsafe extern "system" fn(*mut c_void, *mut u32, u32, *mut u32) -> i32;

fn check_permission_scope() -> Result<(), NativeAffinityError> {
    // A count of one proves that GROUP_AFFINITY describes the whole system.
    // Do not infer permission in other groups from a primary-group mask.
    match unsafe { GetActiveProcessorGroupCount() } {
        0 => return Err(NativeAffinityError::DiscoveryUnavailable),
        1 => {}
        _ => return Err(NativeAffinityError::AmbiguousProcessorGroups),
    }
    const KERNEL32: [u16; 13] = [107, 101, 114, 110, 101, 108, 51, 50, 46, 100, 108, 108, 0];
    // The terminated name is valid for the call. GetModuleHandle borrows an
    // already loaded module; no LoadLibrary, ownership transfer, or unload.
    let module = unsafe { GetModuleHandleW(KERNEL32.as_ptr()) };
    if module.is_null() {
        return Err(NativeAffinityError::DiscoveryUnavailable);
    }
    for (name, handle) in [
        (b"GetProcessDefaultCpuSets\0".as_slice(), unsafe {
            GetCurrentProcess()
        }),
        (b"GetThreadSelectedCpuSets\0".as_slice(), unsafe {
            GetCurrentThread()
        }),
    ] {
        let address = unsafe { GetProcAddress(module, name.as_ptr()) };
        if address.is_null() {
            // Older hosts lack CPU Set APIs. Do not introduce a static import
            // that could prevent application startup on such a host.
            return Err(NativeAffinityError::DiscoveryUnavailable);
        }
        // Both named functions have exactly this documented Win32 signature;
        // the checked address belongs to the process-lifetime system module.
        let query: CpuSetQuery = unsafe { mem::transmute(address) };
        let mut required = 0;
        let status = unsafe { query(handle, ptr::null_mut(), 0, &mut required) };
        if required != 0 {
            return Err(NativeAffinityError::RestrictedCpuSets);
        }
        if status == 0 {
            return Err(NativeAffinityError::DiscoveryUnavailable);
        }
    }
    Ok(())
}

fn from_mask(group: u16, mask: usize) -> Result<NativeThreadAffinity, NativeAffinityError> {
    let cpus = (0..usize::BITS)
        .filter(|bit| mask & (1_usize << bit) != 0)
        .collect();
    NativeThreadAffinity::from_cpu_ids(group, cpus)
}

pub(super) fn current() -> Result<NativeThreadAffinity, NativeAffinityError> {
    check_permission_scope()?;
    let mut process_mask = 0;
    let mut system_mask = 0;
    let mut thread_affinity = GroupAffinity::default();
    // Writable outputs have the precise Win32 ABI layout and live through
    // these synchronous calls; no foreign pointer is retained.
    let process_ok =
        unsafe { GetProcessAffinityMask(GetCurrentProcess(), &mut process_mask, &mut system_mask) };
    let thread_ok = unsafe { GetThreadGroupAffinity(GetCurrentThread(), &mut thread_affinity) };
    if process_ok == 0 || thread_ok == 0 {
        return Err(NativeAffinityError::OperatingSystemFailure);
    }
    from_mask(
        thread_affinity.group,
        process_mask & system_mask & thread_affinity.mask,
    )
}

pub(super) fn numa_nodes() -> Result<Vec<NativeNumaNode>, NativeAffinityError> {
    check_permission_scope()?;
    let mut highest = 0;
    if unsafe { GetNumaHighestNodeNumber(&mut highest) } == 0 || highest > 4095 {
        return Err(NativeAffinityError::DiscoveryUnavailable);
    }
    let mut nodes = Vec::new();
    for node_id in 0..=highest {
        let mut affinity = GroupAffinity::default();
        // Node IDs can have holes and memory-only nodes can have an empty mask.
        // The planner subsequently requires exact coverage of permitted CPUs.
        if unsafe { GetNumaNodeProcessorMaskEx(node_id as u16, &mut affinity) } == 0
            || affinity.mask == 0
        {
            continue;
        }
        nodes.push(NativeNumaNode::new(
            node_id,
            from_mask(affinity.group, affinity.mask)?,
        ));
    }
    Ok(nodes)
}

pub(super) fn apply(affinity: &NativeThreadAffinity) -> Result<(), NativeAffinityError> {
    if affinity.cpu_ids().iter().any(|&cpu| cpu >= usize::BITS) {
        return Err(NativeAffinityError::InvalidCpuSet);
    }
    let mask = affinity
        .cpu_ids()
        .iter()
        .fold(0, |mask, cpu| mask | (1_usize << cpu));
    let request = GroupAffinity {
        mask,
        group: affinity.processor_group(),
        reserved: [0; 3],
    };
    // The safe caller intersected with a fresh complete permission snapshot.
    // This affects only the current persistent worker, not process/job policy.
    let status = unsafe { SetThreadGroupAffinity(GetCurrentThread(), &request, ptr::null_mut()) };
    if status == 0 {
        Err(NativeAffinityError::OperatingSystemFailure)
    } else {
        Ok(())
    }
}
