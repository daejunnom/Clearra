//! Read-only operating-system capacity through the native FFI boundary.
//! Capacity is stable physical/container memory, never a free-memory snapshot.

pub fn native_memory_capacity_bytes() -> Option<u64> {
    platform_capacity()
        .map(|bytes| bytes.min(isize::MAX as u64))
        .filter(|bytes| *bytes > 0)
}

#[cfg(target_os = "linux")]
fn platform_capacity() -> Option<u64> {
    use std::{
        fs,
        path::{Component, Path, PathBuf},
    };
    let contents = fs::read_to_string("/proc/meminfo").ok()?;
    let mut capacity = parse_linux_physical_bytes(&contents)?;
    let membership = fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    for (root, name, relative) in [
        (
            "/sys/fs/cgroup",
            "memory.max",
            membership.lines().find_map(|line| line.strip_prefix("0::")),
        ),
        (
            "/sys/fs/cgroup/memory",
            "memory.limit_in_bytes",
            membership.lines().find_map(|line| {
                let mut fields = line.splitn(3, ':');
                fields.next()?;
                let controllers = fields.next()?;
                controllers
                    .split(',')
                    .any(|value| value == "memory")
                    .then(|| fields.next())
                    .flatten()
            }),
        ),
    ] {
        let root = Path::new(root);
        // The namespace root is an applicable ancestor even when /proc shows
        // a host-relative membership path that is not mounted in this process.
        if let Some(limit) = fs::read_to_string(root.join(name))
            .ok()
            .and_then(|value| parse_finite_limit(&value))
        {
            capacity = capacity.min(limit);
        }
        let Some(relative) = relative else { continue };
        let relative = Path::new(relative.strip_prefix('/').unwrap_or(relative));
        if !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            continue;
        }
        let mut path: PathBuf = root.join(relative);
        while path.starts_with(root) && path != root {
            if let Some(limit) = fs::read_to_string(path.join(name))
                .ok()
                .and_then(|value| parse_finite_limit(&value))
            {
                capacity = capacity.min(limit);
            }
            if !path.pop() {
                break;
            }
        }
    }
    Some(capacity)
}

#[cfg(windows)]
fn platform_capacity() -> Option<u64> {
    crate::raw::system_memory::physical_capacity_bytes()
}

#[cfg(not(any(target_os = "linux", windows)))]
fn platform_capacity() -> Option<u64> {
    None
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_physical_bytes(contents: &str) -> Option<u64> {
    let mut matches = contents
        .lines()
        .filter_map(|line| line.strip_prefix("MemTotal:"));
    let mut fields = matches.next()?.split_whitespace();
    let kib = fields.next()?.parse::<u64>().ok()?;
    if fields.next()? != "kB" || fields.next().is_some() || matches.next().is_some() {
        return None;
    }
    kib.checked_mul(1024).filter(|bytes| *bytes > 0)
}

#[cfg(any(target_os = "linux", test))]
fn parse_finite_limit(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok().filter(|limit| *limit > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_uses_total_physical_memory_and_finite_container_limits() {
        let physical =
            parse_linux_physical_bytes("MemTotal: 33554432 kB\nMemAvailable: 1 kB\n").unwrap();
        assert_eq!(physical, 32 * 1024 * 1024 * 1024);
        assert_eq!(
            physical.min(parse_finite_limit("17179869184\n").unwrap()),
            16 * 1024 * 1024 * 1024
        );
        for invalid in ["max", "0", "-1", "18446744073709551616", ""] {
            assert_eq!(parse_finite_limit(invalid), None);
        }
        assert_eq!(
            parse_linux_physical_bytes("MemTotal: 1 kB\nMemTotal: 2 kB\n"),
            None
        );
        assert_eq!(
            parse_linux_physical_bytes("MemTotal: 18446744073709551615 kB\n"),
            None
        );
    }
}
