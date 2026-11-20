//! Opt-in experiment for exact placement-poset certificates.
//!
//! A failed certificate or an exhausted symbolic compilation budget is Unknown
//! and returns to the existing exact producer. This switch never changes the
//! probability universe, count policy, or worker policy.

use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StructuralOrderMetrics {
    /// Completed candidates whose exact queue coverage used a certified poset.
    pub skipped_build_graphs: u64,
    /// Count-vector residuals used by direct piece-language compilation.
    pub language_residual_states: u64,
    /// Concrete operations removed by proven same-kind twin equivalence.
    pub exchangeable_operations_removed: u64,
    /// Successful candidate/frame guards using one cached positive movement
    /// certificate for the entire occupancy interval instead of hard drop.
    pub cached_interval_guards: u64,
    /// Analytic interval work includes candidates whose later certificate
    /// fails. These are not old-cache observations or primitive BFS states.
    pub algebraic_interval_attempts: u64,
    pub algebraic_interval_guards: u64,
    pub algebraic_interval_row_steps: u64,
}

thread_local! {
    static MODE: Cell<Option<bool>> = const { Cell::new(None) };
    static METRICS: Cell<StructuralOrderMetrics> = Cell::new(StructuralOrderMetrics::default());
}

pub(super) fn enabled() -> bool {
    if let Some(mode) = MODE.with(Cell::get) {
        return mode;
    }
    #[cfg(feature = "local-search-ab")]
    {
        static MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            std::env::var("CLEARRA_STRUCTURAL_ORDER").is_ok_and(|value| value == "1")
        })
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        false
    }
}

#[cfg(feature = "local-search-ab")]
mod process_totals {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::StructuralOrderMetrics;

    // Updated only on a successful graph bypass, not for every geometric
    // candidate or primitive transition. These are process-lifetime diagnostic
    // totals; the finite benchmark performs exactly one search per process.
    static BYPASSES: AtomicU64 = AtomicU64::new(0);
    static RESIDUALS: AtomicU64 = AtomicU64::new(0);
    static TWINS: AtomicU64 = AtomicU64::new(0);
    static INTERVALS: AtomicU64 = AtomicU64::new(0);
    static ALGEBRAIC_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
    static ALGEBRAIC_GUARDS: AtomicU64 = AtomicU64::new(0);
    static ALGEBRAIC_ROWS: AtomicU64 = AtomicU64::new(0);

    pub(super) fn record(delta: StructuralOrderMetrics) {
        BYPASSES.fetch_add(delta.skipped_build_graphs, Ordering::Relaxed);
        RESIDUALS.fetch_add(delta.language_residual_states, Ordering::Relaxed);
        TWINS.fetch_add(delta.exchangeable_operations_removed, Ordering::Relaxed);
        INTERVALS.fetch_add(delta.cached_interval_guards, Ordering::Relaxed);
    }

    pub(super) fn snapshot() -> StructuralOrderMetrics {
        StructuralOrderMetrics {
            skipped_build_graphs: BYPASSES.load(Ordering::Relaxed),
            language_residual_states: RESIDUALS.load(Ordering::Relaxed),
            exchangeable_operations_removed: TWINS.load(Ordering::Relaxed),
            cached_interval_guards: INTERVALS.load(Ordering::Relaxed),
            algebraic_interval_attempts: ALGEBRAIC_ATTEMPTS.load(Ordering::Relaxed),
            algebraic_interval_guards: ALGEBRAIC_GUARDS.load(Ordering::Relaxed),
            algebraic_interval_row_steps: ALGEBRAIC_ROWS.load(Ordering::Relaxed),
        }
    }

    pub(super) fn record_algebraic(attempts: u64, guards: u64, rows: u64) {
        ALGEBRAIC_ATTEMPTS.fetch_add(attempts, Ordering::Relaxed);
        ALGEBRAIC_GUARDS.fetch_add(guards, Ordering::Relaxed);
        ALGEBRAIC_ROWS.fetch_add(rows, Ordering::Relaxed);
    }
}

pub(super) fn record_bypass(
    residual_states: usize,
    removed_operations: usize,
    interval_guards: usize,
) {
    let delta = StructuralOrderMetrics {
        skipped_build_graphs: 1,
        language_residual_states: residual_states as u64,
        exchangeable_operations_removed: removed_operations as u64,
        cached_interval_guards: interval_guards as u64,
        ..StructuralOrderMetrics::default()
    };
    METRICS.with(|slot| {
        let mut value = slot.get();
        value.skipped_build_graphs += delta.skipped_build_graphs;
        value.language_residual_states += delta.language_residual_states;
        value.exchangeable_operations_removed += delta.exchangeable_operations_removed;
        value.cached_interval_guards += delta.cached_interval_guards;
        slot.set(value);
    });
    #[cfg(feature = "local-search-ab")]
    process_totals::record(delta);
}

pub(super) fn record_algebraic_intervals(attempts: usize, guards: usize, rows: usize) {
    if attempts == 0 {
        return;
    }
    METRICS.with(|slot| {
        let mut value = slot.get();
        value.algebraic_interval_attempts += attempts as u64;
        value.algebraic_interval_guards += guards as u64;
        value.algebraic_interval_row_steps += rows as u64;
        slot.set(value);
    });
    #[cfg(feature = "local-search-ab")]
    process_totals::record_algebraic(attempts as u64, guards as u64, rows as u64);
}

/// Diagnostic totals since process start. Read after all experiment workers
/// have joined; this is not a multi-search session statistics interface.
#[cfg(feature = "local-search-ab")]
pub fn structural_order_experiment_totals() -> StructuralOrderMetrics {
    process_totals::snapshot()
}

#[cfg(test)]
pub(super) fn with_mode<T>(
    enabled: bool,
    action: impl FnOnce() -> T,
) -> (T, StructuralOrderMetrics) {
    struct Restore {
        mode: Option<bool>,
        metrics: StructuralOrderMetrics,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            MODE.with(|slot| slot.set(self.mode));
            METRICS.with(|slot| slot.set(self.metrics));
        }
    }
    let _restore = Restore {
        mode: MODE.with(|slot| slot.replace(Some(enabled))),
        metrics: METRICS.with(|slot| slot.replace(StructuralOrderMetrics::default())),
    };
    let output = action();
    (output, METRICS.with(Cell::get))
}
