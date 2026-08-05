const EXPECTED_VCPUS_ENV: &str = "CLEARRA_EXPECTED_VCPUS";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuCapacity {
    recommended_parallelism: usize,
    affinity_limit: Option<usize>,
    expected_vcpus: Option<usize>,
    hard_limit: usize,
}

impl CpuCapacity {
    pub fn current() -> Self {
        // Affinity and cgroup limits may change while a long-lived process is
        // running. Re-probe at each worker-pool boundary so the hard ceiling
        // never relies on a stale startup snapshot.
        Self::detect()
    }

    pub fn detect() -> Self {
        let recommended_parallelism = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .max(1);
        let affinity_limit = process_affinity_limit();
        let expected_vcpus = std::env::var(EXPECTED_VCPUS_ENV)
            .ok()
            .and_then(|value| parse_positive_usize(&value));
        Self::from_observations(recommended_parallelism, affinity_limit, expected_vcpus)
    }

    pub const fn recommended_parallelism(self) -> usize {
        self.recommended_parallelism
    }

    pub const fn affinity_limit(self) -> Option<usize> {
        self.affinity_limit
    }

    pub const fn expected_vcpus(self) -> Option<usize> {
        self.expected_vcpus
    }

    pub const fn hard_limit(self) -> usize {
        self.hard_limit
    }

    fn from_observations(
        recommended_parallelism: usize,
        affinity_limit: Option<usize>,
        expected_vcpus: Option<usize>,
    ) -> Self {
        let recommended_parallelism = recommended_parallelism.max(1);
        let affinity_limit = affinity_limit.filter(|limit| *limit > 0);
        let expected_vcpus = expected_vcpus.filter(|limit| *limit > 0);
        let hard_limit = match (affinity_limit, expected_vcpus) {
            (Some(affinity), Some(expected)) if expected <= affinity => expected,
            // Non-Linux hosts do not expose /proc affinity. They may accept only
            // an expected value already bounded by the standard runtime probe.
            (None, Some(expected)) if expected <= recommended_parallelism => expected,
            _ => recommended_parallelism,
        }
        .max(1);
        Self {
            recommended_parallelism,
            affinity_limit,
            expected_vcpus,
            hard_limit,
        }
    }
}

#[cfg(target_os = "linux")]
fn process_affinity_limit() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))?;
    parse_cpu_list(value)
}

#[cfg(not(target_os = "linux"))]
fn process_affinity_limit() -> Option<usize> {
    None
}

fn parse_positive_usize(value: &str) -> Option<usize> {
    value
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
}

#[cfg(any(target_os = "linux", test))]
fn parse_cpu_list(value: &str) -> Option<usize> {
    let mut count = 0usize;
    let mut previous_end = None;
    for item in value.trim().split(',') {
        let item = item.trim();
        if item.is_empty() {
            return None;
        }
        let (start, end) = match item.split_once('-') {
            Some((start, end)) => (parse_positive_or_zero(start)?, parse_positive_or_zero(end)?),
            None => {
                let cpu = parse_positive_or_zero(item)?;
                (cpu, cpu)
            }
        };
        if end < start || previous_end.is_some_and(|previous| start <= previous) {
            return None;
        }
        count = count.checked_add(end.checked_sub(start)?.checked_add(1)?)?;
        previous_end = Some(end);
    }
    (count > 0).then_some(count)
}

#[cfg(any(target_os = "linux", test))]
fn parse_positive_or_zero(value: &str) -> Option<usize> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::{parse_cpu_list, CpuCapacity};

    #[test]
    fn parses_linux_affinity_lists_without_overcounting_ranges() {
        assert_eq!(parse_cpu_list("0-5"), Some(6));
        assert_eq!(parse_cpu_list("0-3,8,10-12"), Some(8));
        assert_eq!(parse_cpu_list("3"), Some(1));
        assert_eq!(parse_cpu_list("0-3,3-4"), None);
        assert_eq!(parse_cpu_list("4-2"), None);
    }

    #[test]
    fn expected_vcpus_raise_the_hard_limit_only_after_affinity_validation() {
        let cloud_run = CpuCapacity::from_observations(6, Some(8), Some(8));
        assert_eq!(cloud_run.recommended_parallelism(), 6);
        assert_eq!(cloud_run.affinity_limit(), Some(8));
        assert_eq!(cloud_run.expected_vcpus(), Some(8));
        assert_eq!(cloud_run.hard_limit(), 8);

        assert_eq!(
            CpuCapacity::from_observations(6, Some(6), Some(8)).hard_limit(),
            6
        );
        assert_eq!(
            CpuCapacity::from_observations(6, None, Some(8)).hard_limit(),
            6
        );
        assert_eq!(
            CpuCapacity::from_observations(8, None, Some(8)).hard_limit(),
            8
        );
    }
}
