// Local-only A/B switches: no input, known optimum or candidate identity may
// select a policy. Non-experimental builds have compile-time fixed behavior.
#[cfg(feature = "minimum-hotfix-ab")]
static AB_POLICY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(25);

#[cfg(feature = "minimum-hotfix-ab")]
pub fn set_local_ab_policy(flags: u8) -> bool {
    if flags > 31 {
        return false;
    }
    AB_POLICY.store(flags, std::sync::atomic::Ordering::Relaxed);
    true
}

pub(super) fn residual_partition_budget(requested: usize, candidate_rows: usize) -> usize {
    #[cfg(feature = "minimum-hotfix-ab")]
    let enabled = AB_POLICY.load(std::sync::atomic::Ordering::Relaxed) & 16 != 0;
    #[cfg(not(feature = "minimum-hotfix-ab"))]
    let enabled = true;
    if enabled {
        // A measured scheduling policy, never a bound on admissible solutions:
        // avoid repeating full shard preparation for fewer than four remaining
        // candidate rows per queued task. prepare() still emits every child
        // of any pivot it splits, even when that exceeds this target.
        requested.max(1).min(candidate_rows.div_ceil(4).max(1))
    } else {
        requested
    }
}

pub(super) fn impact_branching() -> bool {
    #[cfg(feature = "minimum-hotfix-ab")]
    {
        AB_POLICY.load(std::sync::atomic::Ordering::Relaxed) & 8 != 0
    }
    #[cfg(not(feature = "minimum-hotfix-ab"))]
    {
        true
    }
}

pub(super) fn canonical_interval_bisection() -> bool {
    #[cfg(feature = "minimum-hotfix-ab")]
    {
        AB_POLICY.load(std::sync::atomic::Ordering::Relaxed) & 4 != 0
    }
    #[cfg(not(feature = "minimum-hotfix-ab"))]
    {
        false
    }
}

pub(super) fn parallel_first_dispatch() -> bool {
    #[cfg(feature = "minimum-hotfix-ab")]
    {
        AB_POLICY.load(std::sync::atomic::Ordering::Relaxed) & 1 != 0
    }
    #[cfg(not(feature = "minimum-hotfix-ab"))]
    {
        true
    }
}

pub(super) fn rounded_components() -> bool {
    #[cfg(feature = "minimum-hotfix-ab")]
    {
        AB_POLICY.load(std::sync::atomic::Ordering::Relaxed) & 2 != 0
    }
    // Experimental until same-binary A/B shows a useful improvement.
    #[cfg(not(feature = "minimum-hotfix-ab"))]
    {
        false
    }
}
