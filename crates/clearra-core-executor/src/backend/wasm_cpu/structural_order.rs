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
    /// Subset of analytic work using a cold, fixed-orientation translation
    /// witness. Unlike the O capability, failure is never an impossibility.
    pub translation_interval_attempts: u64,
    pub translation_interval_guards: u64,
    pub translation_interval_row_steps: u64,
    pub movement_clear: MovementClearMetrics,
    pub first_kick_interval: KickIntervalMetrics,
    pub ordered_kick_interval: KickIntervalMetrics,
    pub conditional_order: ConditionalOrderMetrics,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConditionalOrderMetrics {
    pub attempted_candidates: u64,
    pub certified_candidates: u64,
    pub empty_certificates: u64,
    pub unknown_candidates: u64,
    pub budget_candidates: u64,
    pub checked_transitions: u64,
    pub residual_states: u64,
    pub graph_bypasses: u64,
    pub reused_pose_proofs: u64,
}

impl ConditionalOrderMetrics {
    fn add(&mut self, delta: Self) {
        self.attempted_candidates += delta.attempted_candidates;
        self.certified_candidates += delta.certified_candidates;
        self.empty_certificates += delta.empty_certificates;
        self.unknown_candidates += delta.unknown_candidates;
        self.budget_candidates += delta.budget_candidates;
        self.checked_transitions += delta.checked_transitions;
        self.residual_states += delta.residual_states;
        self.graph_bypasses += delta.graph_bypasses;
        self.reused_pose_proofs += delta.reused_pose_proofs;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KickIntervalMetrics {
    pub attempts: u64,
    pub proofs: u64,
    /// Edges selected without any ordinary lock query, excluding interval
    /// successes superseded by a later exact hard-drop alternative.
    pub selected_locks: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MovementClearMetrics {
    pub frame_pose_attempts: u64,
    pub frame_pose_proofs: u64,
    pub reused_positive_locks: u64,
    pub reused_closed_locks: u64,
    pub point_pose_attempts: u64,
    pub point_pose_proofs: u64,
}

impl MovementClearMetrics {
    fn add(&mut self, delta: Self) {
        self.frame_pose_attempts += delta.frame_pose_attempts;
        self.frame_pose_proofs += delta.frame_pose_proofs;
        self.reused_positive_locks += delta.reused_positive_locks;
        self.reused_closed_locks += delta.reused_closed_locks;
        self.point_pose_attempts += delta.point_pose_attempts;
        self.point_pose_proofs += delta.point_pose_proofs;
    }
}

thread_local! {
    static MODE: Cell<Option<bool>> = const { Cell::new(None) };
    static TRANSLATION_MODE: Cell<Option<bool>> = const { Cell::new(None) };
    static MOVEMENT_CLEAR_MODE: Cell<Option<bool>> = const { Cell::new(None) };
    static FIRST_KICK_INTERVAL_MODE: Cell<Option<bool>> = const { Cell::new(None) };
    static ORDERED_KICK_INTERVAL_MODE: Cell<Option<bool>> = const { Cell::new(None) };
    static CONDITIONAL_ORDER_MODE: Cell<Option<bool>> = const { Cell::new(None) };
    static METRICS: Cell<StructuralOrderMetrics> = Cell::new(StructuralOrderMetrics::default());
}

pub(super) fn conditional_order_enabled() -> bool {
    if let Some(mode) = CONDITIONAL_ORDER_MODE.with(Cell::get) {
        return mode;
    }
    #[cfg(feature = "local-search-ab")]
    {
        static MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            std::env::var("CLEARRA_CONDITIONAL_ORDER").is_ok_and(|value| value == "1")
        })
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        false
    }
}

pub(super) fn first_kick_intervals_enabled() -> bool {
    if let Some(mode) = FIRST_KICK_INTERVAL_MODE.with(Cell::get) {
        return mode;
    }
    #[cfg(feature = "local-search-ab")]
    {
        static MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            std::env::var("CLEARRA_FIRST_KICK_INTERVAL").is_ok_and(|value| value == "1")
        })
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        false
    }
}

pub(super) fn ordered_kick_intervals_enabled() -> bool {
    if let Some(mode) = ORDERED_KICK_INTERVAL_MODE.with(Cell::get) {
        return mode;
    }
    #[cfg(feature = "local-search-ab")]
    {
        static MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            std::env::var("CLEARRA_ORDERED_KICK_INTERVAL").is_ok_and(|value| value == "1")
        })
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        false
    }
}

pub(super) fn movement_clear_certificates_enabled() -> bool {
    if let Some(mode) = MOVEMENT_CLEAR_MODE.with(Cell::get) {
        return mode;
    }
    #[cfg(feature = "local-search-ab")]
    {
        static MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            std::env::var("CLEARRA_MOVEMENT_CLEAR_CERTIFICATES").is_ok_and(|value| value == "1")
        })
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        false
    }
}

pub(super) fn translation_intervals_enabled() -> bool {
    if let Some(mode) = TRANSLATION_MODE.with(Cell::get) {
        return mode;
    }
    #[cfg(feature = "local-search-ab")]
    {
        static MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            std::env::var("CLEARRA_TRANSLATION_INTERVAL").is_ok_and(|value| value == "1")
        })
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        false
    }
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

    use super::{
        ConditionalOrderMetrics, KickIntervalMetrics, MovementClearMetrics, StructuralOrderMetrics,
    };

    // Bypass totals count successful certificates. Analytic totals also count
    // failed candidates, once per candidate rather than per row or primitive
    // transition. The finite benchmark performs one search per process.
    static BYPASSES: AtomicU64 = AtomicU64::new(0);
    static RESIDUALS: AtomicU64 = AtomicU64::new(0);
    static TWINS: AtomicU64 = AtomicU64::new(0);
    static INTERVALS: AtomicU64 = AtomicU64::new(0);
    static ALGEBRAIC_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
    static ALGEBRAIC_GUARDS: AtomicU64 = AtomicU64::new(0);
    static ALGEBRAIC_ROWS: AtomicU64 = AtomicU64::new(0);
    static TRANSLATION_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
    static TRANSLATION_GUARDS: AtomicU64 = AtomicU64::new(0);
    static TRANSLATION_ROWS: AtomicU64 = AtomicU64::new(0);
    static FRAME_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
    static FRAME_PROOFS: AtomicU64 = AtomicU64::new(0);
    static REUSED_POSITIVE: AtomicU64 = AtomicU64::new(0);
    static REUSED_CLOSED: AtomicU64 = AtomicU64::new(0);
    static POINT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
    static POINT_PROOFS: AtomicU64 = AtomicU64::new(0);
    static FIRST_KICK_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
    static FIRST_KICK_PROOFS: AtomicU64 = AtomicU64::new(0);
    static FIRST_KICK_SELECTED: AtomicU64 = AtomicU64::new(0);
    static ORDERED_KICK_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
    static ORDERED_KICK_PROOFS: AtomicU64 = AtomicU64::new(0);
    static ORDERED_KICK_SELECTED: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_CERTIFIED: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_EMPTY: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_UNKNOWN: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_BUDGET: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_TRANSITIONS: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_STATES: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_BYPASSES: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_REUSED: AtomicU64 = AtomicU64::new(0);

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
            translation_interval_attempts: TRANSLATION_ATTEMPTS.load(Ordering::Relaxed),
            translation_interval_guards: TRANSLATION_GUARDS.load(Ordering::Relaxed),
            translation_interval_row_steps: TRANSLATION_ROWS.load(Ordering::Relaxed),
            movement_clear: MovementClearMetrics {
                frame_pose_attempts: FRAME_ATTEMPTS.load(Ordering::Relaxed),
                frame_pose_proofs: FRAME_PROOFS.load(Ordering::Relaxed),
                reused_positive_locks: REUSED_POSITIVE.load(Ordering::Relaxed),
                reused_closed_locks: REUSED_CLOSED.load(Ordering::Relaxed),
                point_pose_attempts: POINT_ATTEMPTS.load(Ordering::Relaxed),
                point_pose_proofs: POINT_PROOFS.load(Ordering::Relaxed),
            },
            first_kick_interval: KickIntervalMetrics {
                attempts: FIRST_KICK_ATTEMPTS.load(Ordering::Relaxed),
                proofs: FIRST_KICK_PROOFS.load(Ordering::Relaxed),
                selected_locks: FIRST_KICK_SELECTED.load(Ordering::Relaxed),
            },
            ordered_kick_interval: KickIntervalMetrics {
                attempts: ORDERED_KICK_ATTEMPTS.load(Ordering::Relaxed),
                proofs: ORDERED_KICK_PROOFS.load(Ordering::Relaxed),
                selected_locks: ORDERED_KICK_SELECTED.load(Ordering::Relaxed),
            },
            conditional_order: ConditionalOrderMetrics {
                attempted_candidates: CONDITIONAL_ATTEMPTS.load(Ordering::Relaxed),
                certified_candidates: CONDITIONAL_CERTIFIED.load(Ordering::Relaxed),
                empty_certificates: CONDITIONAL_EMPTY.load(Ordering::Relaxed),
                unknown_candidates: CONDITIONAL_UNKNOWN.load(Ordering::Relaxed),
                budget_candidates: CONDITIONAL_BUDGET.load(Ordering::Relaxed),
                checked_transitions: CONDITIONAL_TRANSITIONS.load(Ordering::Relaxed),
                residual_states: CONDITIONAL_STATES.load(Ordering::Relaxed),
                graph_bypasses: CONDITIONAL_BYPASSES.load(Ordering::Relaxed),
                reused_pose_proofs: CONDITIONAL_REUSED.load(Ordering::Relaxed),
            },
        }
    }

    pub(super) fn record_algebraic(attempts: u64, guards: u64, rows: u64) {
        ALGEBRAIC_ATTEMPTS.fetch_add(attempts, Ordering::Relaxed);
        ALGEBRAIC_GUARDS.fetch_add(guards, Ordering::Relaxed);
        ALGEBRAIC_ROWS.fetch_add(rows, Ordering::Relaxed);
    }

    pub(super) fn record_translation(attempts: u64, guards: u64, rows: u64) {
        TRANSLATION_ATTEMPTS.fetch_add(attempts, Ordering::Relaxed);
        TRANSLATION_GUARDS.fetch_add(guards, Ordering::Relaxed);
        TRANSLATION_ROWS.fetch_add(rows, Ordering::Relaxed);
    }

    pub(super) fn record_movement_clear(delta: MovementClearMetrics) {
        FRAME_ATTEMPTS.fetch_add(delta.frame_pose_attempts, Ordering::Relaxed);
        FRAME_PROOFS.fetch_add(delta.frame_pose_proofs, Ordering::Relaxed);
        REUSED_POSITIVE.fetch_add(delta.reused_positive_locks, Ordering::Relaxed);
        REUSED_CLOSED.fetch_add(delta.reused_closed_locks, Ordering::Relaxed);
        POINT_ATTEMPTS.fetch_add(delta.point_pose_attempts, Ordering::Relaxed);
        POINT_PROOFS.fetch_add(delta.point_pose_proofs, Ordering::Relaxed);
    }

    pub(super) fn record_first_kick_interval(delta: KickIntervalMetrics) {
        FIRST_KICK_ATTEMPTS.fetch_add(delta.attempts, Ordering::Relaxed);
        FIRST_KICK_PROOFS.fetch_add(delta.proofs, Ordering::Relaxed);
        FIRST_KICK_SELECTED.fetch_add(delta.selected_locks, Ordering::Relaxed);
    }

    pub(super) fn record_ordered_kick_interval(delta: KickIntervalMetrics) {
        ORDERED_KICK_ATTEMPTS.fetch_add(delta.attempts, Ordering::Relaxed);
        ORDERED_KICK_PROOFS.fetch_add(delta.proofs, Ordering::Relaxed);
        ORDERED_KICK_SELECTED.fetch_add(delta.selected_locks, Ordering::Relaxed);
    }

    pub(super) fn record_conditional_order(delta: ConditionalOrderMetrics) {
        CONDITIONAL_ATTEMPTS.fetch_add(delta.attempted_candidates, Ordering::Relaxed);
        CONDITIONAL_CERTIFIED.fetch_add(delta.certified_candidates, Ordering::Relaxed);
        CONDITIONAL_EMPTY.fetch_add(delta.empty_certificates, Ordering::Relaxed);
        CONDITIONAL_UNKNOWN.fetch_add(delta.unknown_candidates, Ordering::Relaxed);
        CONDITIONAL_BUDGET.fetch_add(delta.budget_candidates, Ordering::Relaxed);
        CONDITIONAL_TRANSITIONS.fetch_add(delta.checked_transitions, Ordering::Relaxed);
        CONDITIONAL_STATES.fetch_add(delta.residual_states, Ordering::Relaxed);
        CONDITIONAL_BYPASSES.fetch_add(delta.graph_bypasses, Ordering::Relaxed);
        CONDITIONAL_REUSED.fetch_add(delta.reused_pose_proofs, Ordering::Relaxed);
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

pub(super) fn record_translation_intervals(attempts: usize, guards: usize, rows: usize) {
    if attempts == 0 {
        return;
    }
    METRICS.with(|slot| {
        let mut value = slot.get();
        value.translation_interval_attempts += attempts as u64;
        value.translation_interval_guards += guards as u64;
        value.translation_interval_row_steps += rows as u64;
        slot.set(value);
    });
    #[cfg(feature = "local-search-ab")]
    process_totals::record_translation(attempts as u64, guards as u64, rows as u64);
}

pub(super) fn record_movement_clear(delta: MovementClearMetrics) {
    if delta == MovementClearMetrics::default() {
        return;
    }
    METRICS.with(|slot| {
        let mut value = slot.get();
        value.movement_clear.add(delta);
        slot.set(value);
    });
    #[cfg(feature = "local-search-ab")]
    process_totals::record_movement_clear(delta);
}

pub(super) fn record_first_kick_interval(delta: KickIntervalMetrics) {
    if delta.attempts == 0 {
        return;
    }
    METRICS.with(|slot| {
        let mut value = slot.get();
        value.first_kick_interval.attempts += delta.attempts;
        value.first_kick_interval.proofs += delta.proofs;
        value.first_kick_interval.selected_locks += delta.selected_locks;
        slot.set(value);
    });
    #[cfg(feature = "local-search-ab")]
    process_totals::record_first_kick_interval(delta);
}

pub(super) fn record_ordered_kick_interval(delta: KickIntervalMetrics) {
    if delta.attempts == 0 {
        return;
    }
    METRICS.with(|slot| {
        let mut value = slot.get();
        value.ordered_kick_interval.attempts += delta.attempts;
        value.ordered_kick_interval.proofs += delta.proofs;
        value.ordered_kick_interval.selected_locks += delta.selected_locks;
        slot.set(value);
    });
    #[cfg(feature = "local-search-ab")]
    process_totals::record_ordered_kick_interval(delta);
}

pub(super) fn record_conditional_order(delta: ConditionalOrderMetrics) {
    if delta == ConditionalOrderMetrics::default() {
        return;
    }
    METRICS.with(|slot| {
        let mut value = slot.get();
        value.conditional_order.add(delta);
        slot.set(value);
    });
    #[cfg(feature = "local-search-ab")]
    process_totals::record_conditional_order(delta);
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

#[cfg(test)]
pub(super) fn with_translation_interval_mode<T>(enabled: bool, action: impl FnOnce() -> T) -> T {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TRANSLATION_MODE.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(TRANSLATION_MODE.with(|slot| slot.replace(Some(enabled))));
    action()
}

#[cfg(test)]
pub(super) fn with_movement_clear_mode<T>(enabled: bool, action: impl FnOnce() -> T) -> T {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            MOVEMENT_CLEAR_MODE.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(MOVEMENT_CLEAR_MODE.with(|slot| slot.replace(Some(enabled))));
    action()
}

#[cfg(test)]
pub(super) fn with_first_kick_interval_mode<T>(enabled: bool, action: impl FnOnce() -> T) -> T {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FIRST_KICK_INTERVAL_MODE.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(FIRST_KICK_INTERVAL_MODE.with(|slot| slot.replace(Some(enabled))));
    action()
}

#[cfg(test)]
pub(super) fn with_ordered_kick_interval_mode<T>(enabled: bool, action: impl FnOnce() -> T) -> T {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ORDERED_KICK_INTERVAL_MODE.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(ORDERED_KICK_INTERVAL_MODE.with(|slot| slot.replace(Some(enabled))));
    action()
}

#[cfg(test)]
pub(super) fn with_conditional_order_mode<T>(enabled: bool, action: impl FnOnce() -> T) -> T {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CONDITIONAL_ORDER_MODE.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(CONDITIONAL_ORDER_MODE.with(|slot| slot.replace(Some(enabled))));
    action()
}
