//! Narrow OS boundary for optional worker locality. Discovery grants no CPU
//! capacity: callers retain their existing worker-count policy. Applying a
//! placement may only narrow the calling thread's currently permitted CPUs.

#[cfg(target_os = "linux")]
#[path = "raw/native_thread_affinity_linux.rs"]
mod platform;
#[cfg(all(target_os = "windows", target_pointer_width = "64"))]
#[path = "raw/native_thread_affinity_windows.rs"]
mod platform;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum NativeAffinityError {
    UnsupportedPlatform,
    DiscoveryUnavailable,
    InvalidCpuSet,
    EmptyIntersection,
    AmbiguousProcessorGroups,
    RestrictedCpuSets,
    RestrictedMemoryNodes,
    OperatingSystemFailure,
}

/// CPU IDs are local to `processor_group`; Linux has one flat group, zero.
/// Construction validates the sorted unique representation, not OS permission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeThreadAffinity {
    processor_group: u16,
    cpu_ids: Vec<u32>,
}

impl NativeThreadAffinity {
    pub fn from_cpu_ids(
        processor_group: u16,
        cpu_ids: Vec<u32>,
    ) -> Result<Self, NativeAffinityError> {
        if cpu_ids.is_empty() || cpu_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(NativeAffinityError::InvalidCpuSet);
        }
        Ok(Self {
            processor_group,
            cpu_ids,
        })
    }

    pub const fn processor_group(&self) -> u16 {
        self.processor_group
    }

    pub fn cpu_ids(&self) -> &[u32] {
        &self.cpu_ids
    }

    pub fn intersection(&self, permitted: &Self) -> Option<Self> {
        if self.processor_group != permitted.processor_group {
            return None;
        }
        let cpu_ids = self
            .cpu_ids
            .iter()
            .copied()
            .filter(|cpu| permitted.cpu_ids.binary_search(cpu).is_ok())
            .collect::<Vec<_>>();
        Self::from_cpu_ids(self.processor_group, cpu_ids).ok()
    }

    pub fn current() -> Result<Self, NativeAffinityError> {
        #[cfg(any(
            target_os = "linux",
            all(target_os = "windows", target_pointer_width = "64")
        ))]
        {
            platform::current()
        }
        #[cfg(not(any(
            target_os = "linux",
            all(target_os = "windows", target_pointer_width = "64")
        )))]
        {
            Err(NativeAffinityError::UnsupportedPlatform)
        }
    }

    /// Topology is a snapshot for placement only. Incomplete or ambiguous
    /// coverage must fall back to the OS scheduler, never to fewer workers.
    pub fn numa_nodes() -> Result<Vec<NativeNumaNode>, NativeAffinityError> {
        #[cfg(any(
            target_os = "linux",
            all(target_os = "windows", target_pointer_width = "64")
        ))]
        {
            platform::numa_nodes()
        }
        #[cfg(not(any(
            target_os = "linux",
            all(target_os = "windows", target_pointer_width = "64")
        )))]
        {
            Err(NativeAffinityError::UnsupportedPlatform)
        }
    }

    /// Re-reads this worker's inherited affinity immediately before applying
    /// the requested placement. A stale discovery snapshot cannot expand it.
    pub fn restrict_current_thread(&self) -> Result<(), NativeAffinityError> {
        let current = Self::current()?;
        let Some(restricted) = self.intersection(&current) else {
            return Err(NativeAffinityError::EmptyIntersection);
        };
        if restricted == current {
            return Ok(());
        }
        #[cfg(any(
            target_os = "linux",
            all(target_os = "windows", target_pointer_width = "64")
        ))]
        {
            platform::apply(&restricted)
        }
        #[cfg(not(any(
            target_os = "linux",
            all(target_os = "windows", target_pointer_width = "64")
        )))]
        {
            Err(NativeAffinityError::UnsupportedPlatform)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeNumaNode {
    node_id: u32,
    affinity: NativeThreadAffinity,
}

impl NativeNumaNode {
    pub const fn new(node_id: u32, affinity: NativeThreadAffinity) -> Self {
        Self { node_id, affinity }
    }

    pub const fn node_id(&self) -> u32 {
        self.node_id
    }
    pub const fn affinity(&self) -> &NativeThreadAffinity {
        &self.affinity
    }
}

#[cfg(test)]
mod tests {
    use super::NativeAffinityError;
    use super::NativeThreadAffinity;

    #[test]
    fn affinity_intersection_preserves_sparse_cpu_ids_and_processor_groups() {
        let allowed =
            NativeThreadAffinity::from_cpu_ids(1, vec![1, 7, 31, 63]).expect("allowed CPUs");
        let request =
            NativeThreadAffinity::from_cpu_ids(1, vec![0, 1, 31, 62]).expect("requested CPUs");
        assert_eq!(
            request
                .intersection(&allowed)
                .expect("nonempty intersection")
                .cpu_ids(),
            &[1, 31]
        );
        let other_group = NativeThreadAffinity::from_cpu_ids(0, vec![1, 31]).expect("other group");
        assert!(other_group.intersection(&allowed).is_none());
        assert_eq!(
            NativeThreadAffinity::from_cpu_ids(0, vec![1, 1]),
            Err(NativeAffinityError::InvalidCpuSet)
        );
        assert_eq!(
            NativeThreadAffinity::from_cpu_ids(0, vec![4, 1]),
            Err(NativeAffinityError::InvalidCpuSet)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn worker_affinity_never_expands_inherited_permission() {
        let result = std::thread::spawn(|| {
            let inherited = NativeThreadAffinity::current().expect("Linux inherited affinity");
            let Some(&first) = inherited.cpu_ids().first() else {
                panic!("nonempty affinity");
            };
            let narrowed =
                NativeThreadAffinity::from_cpu_ids(0, vec![first]).expect("one allowed CPU");
            // Some sandboxes deny affinity changes. In that case the API's
            // explicit error leaves the scheduler in charge; there is no retry.
            if narrowed.restrict_current_thread().is_err() {
                assert_eq!(
                    NativeThreadAffinity::current().expect("unchanged affinity"),
                    inherited
                );
                return;
            }
            assert_eq!(
                NativeThreadAffinity::current().expect("narrow affinity"),
                narrowed
            );
            inherited
                .restrict_current_thread()
                .expect("intersection remains narrow");
            assert_eq!(
                NativeThreadAffinity::current().expect("still narrow"),
                narrowed
            );
        });
        result.join().expect("isolated affinity test worker");
    }
}
