use std::fs;
use std::mem;

use super::NativeAffinityError;
use super::NativeNumaNode;
use super::NativeThreadAffinity;

pub(super) fn current() -> Result<NativeThreadAffinity, NativeAffinityError> {
    // cpu_set_t is an integer bitset; all-zero is a valid empty value. Its
    // fixed ABI size is checked by the kernel. Larger kernels return EINVAL,
    // which deliberately disables locality without reducing CPU capacity.
    let mut mask: libc::cpu_set_t = unsafe { mem::zeroed() };
    let status = unsafe { libc::sched_getaffinity(0, mem::size_of_val(&mask), &mut mask) };
    if status != 0 {
        return Err(NativeAffinityError::OperatingSystemFailure);
    }
    let cpus = (0..libc::CPU_SETSIZE as usize)
        .filter(|&cpu| unsafe { libc::CPU_ISSET(cpu, &mask) })
        .map(|cpu| cpu as u32)
        .collect();
    NativeThreadAffinity::from_cpu_ids(0, cpus)
}

pub(super) fn apply(affinity: &NativeThreadAffinity) -> Result<(), NativeAffinityError> {
    if affinity.processor_group() != 0
        || affinity
            .cpu_ids()
            .iter()
            .any(|&cpu| cpu as usize >= libc::CPU_SETSIZE as usize)
    {
        return Err(NativeAffinityError::InvalidCpuSet);
    }
    // Every index has been bounds checked; libc's CPU_SET only writes this
    // stack-owned cpu_set_t. PID zero changes the calling worker alone.
    let mut mask: libc::cpu_set_t = unsafe { mem::zeroed() };
    for &cpu in affinity.cpu_ids() {
        unsafe {
            libc::CPU_SET(cpu as usize, &mut mask);
        }
    }
    let status = unsafe { libc::sched_setaffinity(0, mem::size_of_val(&mask), &mask) };
    if status == 0 {
        Ok(())
    } else {
        Err(NativeAffinityError::OperatingSystemFailure)
    }
}

pub(super) fn numa_nodes() -> Result<Vec<NativeNumaNode>, NativeAffinityError> {
    let online = fs::read_to_string("/sys/devices/system/node/online")
        .map_err(|_| NativeAffinityError::DiscoveryUnavailable)?;
    let node_ids = parse_id_list(&online)?;
    let status = fs::read_to_string("/proc/thread-self/status")
        .map_err(|_| NativeAffinityError::DiscoveryUnavailable)?;
    let memory_nodes = status
        .lines()
        .find_map(|line| line.strip_prefix("Mems_allowed_list:"))
        .ok_or(NativeAffinityError::DiscoveryUnavailable)
        .and_then(parse_id_list)?;
    let allowed = current()?;
    let mut nodes = Vec::new();
    for node_id in node_ids {
        let path = format!("/sys/devices/system/node/node{node_id}/cpulist");
        let text =
            fs::read_to_string(path).map_err(|_| NativeAffinityError::DiscoveryUnavailable)?;
        if text.trim().is_empty() {
            continue;
        } // memory-only NUMA node
        let affinity = NativeThreadAffinity::from_cpu_ids(0, parse_id_list(&text)?)?;
        if affinity.intersection(&allowed).is_none() {
            continue;
        }
        if memory_nodes.binary_search(&node_id).is_err() {
            // Pinning onto a memory-disallowed node would not provide local
            // first touch. Keep every permitted CPU available to the scheduler.
            return Err(NativeAffinityError::RestrictedMemoryNodes);
        }
        nodes.push(NativeNumaNode::new(node_id, affinity));
    }
    Ok(nodes)
}

fn parse_id_list(text: &str) -> Result<Vec<u32>, NativeAffinityError> {
    const MAX_DISCOVERY_IDS: usize = 65_536;
    let mut ids = Vec::new();
    for item in text.trim().split(',') {
        let parse = |value: &str| {
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(NativeAffinityError::InvalidCpuSet);
            }
            value
                .parse::<u32>()
                .map_err(|_| NativeAffinityError::InvalidCpuSet)
        };
        let (start, end) = match item.trim().split_once('-') {
            Some((start, end)) => (parse(start)?, parse(end)?),
            None => {
                let id = parse(item.trim())?;
                (id, id)
            }
        };
        if end < start || ids.last().is_some_and(|previous| *previous >= start) {
            return Err(NativeAffinityError::InvalidCpuSet);
        }
        let count = u64::from(end) - u64::from(start) + 1;
        if count > (MAX_DISCOVERY_IDS - ids.len()) as u64 {
            return Err(NativeAffinityError::DiscoveryUnavailable);
        }
        ids.extend(start..=end);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::parse_id_list;

    #[test]
    fn linux_topology_lists_reject_overlap_and_unbounded_discovery() {
        assert_eq!(
            parse_id_list("0-2,8,10-11").expect("sparse list"),
            vec![0, 1, 2, 8, 10, 11]
        );
        for invalid in ["", "1-0", "0-3,3-4", "3,1", "-1", "0-4294967295"] {
            assert!(parse_id_list(invalid).is_err(), "{invalid}");
        }
    }
}
