// Local-only A/B switches: no input, known optimum or candidate identity may
// select a policy. Non-experimental builds have compile-time fixed behavior.
#[cfg(feature = "minimum-hotfix-ab")]
static AB_POLICY: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(185);

#[cfg(feature = "minimum-hotfix-ab")]
pub fn set_local_ab_policy(flags: u16) -> bool {
    AB_POLICY.store(flags, std::sync::atomic::Ordering::Relaxed);
    true
}

/// Total cooperative work before an unissued canonical query is delegated.
/// These budgets include preparation; exhausting one never proves infeasibility.
pub(super) fn canonical_probe_steps(_requested_partitions: usize) -> u64 {
    #[cfg(feature = "minimum-hotfix-ab")]
    {
        let flags = AB_POLICY.load(std::sync::atomic::Ordering::Relaxed);
        if flags & 1024 != 0 {
            128
        } else if flags & 512 != 0 {
            32
        } else if flags & 256 != 0 {
            8
        } else {
            0
        }
    }
    #[cfg(not(feature = "minimum-hotfix-ab"))]
    {
        if combined_canonical_policy_is_admitted(_requested_partitions) {
            32
        } else {
            0
        }
    }
}

#[cfg(not(feature = "minimum-hotfix-ab"))]
fn combined_canonical_policy_is_admitted(requested_partitions: usize) -> bool {
    // The N + bounded-probe combination improved requests of 32/44/48
    // partitions, but regressed 16. Keep the previous low-fanout policy.
    // This threshold concerns scheduler work, not a CPU model or fixture.
    requested_partitions >= 32
}

pub(super) fn distinct_dual_capacity() -> bool {
    #[cfg(feature = "minimum-hotfix-ab")]
    {
        AB_POLICY.load(std::sync::atomic::Ordering::Relaxed) & 32 != 0
    }
    #[cfg(not(feature = "minimum-hotfix-ab"))]
    {
        true
    }
}

pub(super) fn idle_assistance(_requested_partitions: usize) -> bool {
    #[cfg(feature = "minimum-hotfix-ab")]
    {
        AB_POLICY.load(std::sync::atomic::Ordering::Relaxed) & 64 == 0
    }
    #[cfg(not(feature = "minimum-hotfix-ab"))]
    {
        // Retain the positive-only global warm repair in either policy.
        // Only redundant idle assistance changes with the bounded probe.
        !combined_canonical_policy_is_admitted(_requested_partitions)
    }
}

pub(super) fn small_canonical_tail() -> bool {
    #[cfg(feature = "minimum-hotfix-ab")]
    {
        AB_POLICY.load(std::sync::atomic::Ordering::Relaxed) & 128 != 0
    }
    #[cfg(not(feature = "minimum-hotfix-ab"))]
    {
        true
    }
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
