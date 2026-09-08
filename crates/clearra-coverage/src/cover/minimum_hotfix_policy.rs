// Local-only A/B switches: no input, known optimum or candidate identity may
// select a policy. Non-experimental builds have compile-time fixed behavior.
#[cfg(feature = "minimum-hotfix-ab")]
static AB_POLICY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

#[cfg(feature = "minimum-hotfix-ab")]
pub fn set_local_ab_policy(flags: u8) -> bool {
    if flags > 7 {
        return false;
    }
    AB_POLICY.store(flags, std::sync::atomic::Ordering::Relaxed);
    true
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
