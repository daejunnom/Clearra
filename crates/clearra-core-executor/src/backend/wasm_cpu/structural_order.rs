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
    pub ordered_kick_guard_reuse: KickGuardReuseMetrics,
    pub conditional_order: ConditionalOrderMetrics,
    pub conditional_ordered_negative: ConditionalOrderedNegativeMetrics,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConditionalOrderedNegativeMetrics {
    pub attempts: u64,
    pub proofs: u64,
    /// Poses checked even when the ordered closure returns Unknown.
    pub checked_poses: u64,
    pub reused: u64,
}

impl ConditionalOrderedNegativeMetrics {
    pub(super) fn attempted(&mut self, proven: bool, checked_poses: usize) {
        self.attempts += 1;
        self.proofs += u64::from(proven);
        self.checked_poses += checked_poses as u64;
    }
}

/// A same-thread diagnostic override, never selected by environment or product policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionalOrderedWorkMode {
    Off,
    Point,
    ClosedWithin,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KickGuardReuseMetrics {
    pub constructed: u64,
    pub stored: u64,
    pub reused_selected: u64,
    pub conflicts: u64,
    pub exact_retries: u64,
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
    static KICK_GUARD_REUSE_MODE: Cell<Option<bool>> = const { Cell::new(None) };
    static CONDITIONAL_ORDER_MODE: Cell<Option<bool>> = const { Cell::new(None) };
    static METRICS: Cell<StructuralOrderMetrics> = Cell::new(StructuralOrderMetrics::default());
}

thread_local! {
    static CONDITIONAL_ORDERED_NEGATIVE: Cell<bool> = const { Cell::new(false) };
    static CONDITIONAL_ORDERED_NEGATIVE_REUSE: Cell<bool> = const { Cell::new(false) };
}

/// Keep an explicitly selected diagnostic producer independent of automatic
/// product certificates, including a scope selecting the reference arm.
#[cfg(test)]
pub(super) fn has_explicit_scope() -> bool {
    MODE.get().is_some()
        || TRANSLATION_MODE.get().is_some()
        || MOVEMENT_CLEAR_MODE.get().is_some()
        || FIRST_KICK_INTERVAL_MODE.get().is_some()
        || ORDERED_KICK_INTERVAL_MODE.get().is_some()
        || KICK_GUARD_REUSE_MODE.get().is_some()
        || CONDITIONAL_ORDER_MODE.get().is_some()
        || CONDITIONAL_ORDERED_NEGATIVE.get()
        || CONDITIONAL_ORDERED_NEGATIVE_REUSE.get()
        || CONDITIONAL_FALLBACK.get().is_some()
}

/// Isolated fallback probe; neither the normal 256-transition budget nor the
/// local-feature comparator opts into carrying these negatives to BuildUp.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ConditionalFallbackWork {
    pub offered: usize,
    pub filtered_poses: usize,
    pub root_filtered_poses: usize,
    pub conflicting_poses: usize,
}

#[cfg(test)]
thread_local! {
    static CONDITIONAL_FALLBACK: Cell<Option<(usize, bool, ConditionalFallbackWork)>> = const { Cell::new(None) };
}

#[cfg(test)]
pub(super) fn with_conditional_fallback_work<T>(
    cap: usize,
    carry: bool,
    run: impl FnOnce() -> T,
) -> (T, ConditionalFallbackWork) {
    assert!((1..=256).contains(&cap));
    struct Restore(Option<(usize, bool, ConditionalFallbackWork)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CONDITIONAL_FALLBACK.set(self.0);
        }
    }
    let _restore = Restore(CONDITIONAL_FALLBACK.replace(Some((cap, carry, Default::default()))));
    let result = run();
    (result, CONDITIONAL_FALLBACK.get().unwrap().2)
}

#[cfg(test)]
pub(super) fn conditional_fallback_cap() -> usize {
    CONDITIONAL_FALLBACK.get().map_or(256, |(cap, _, _)| cap)
}

#[cfg(test)]
pub(super) fn record_conditional_fallback(
    update: impl FnOnce(&mut ConditionalFallbackWork),
) -> bool {
    if let Some((cap, carry, mut work)) = CONDITIONAL_FALLBACK.get() {
        update(&mut work);
        CONDITIONAL_FALLBACK.set(Some((cap, carry, work)));
        return carry;
    }
    false
}

pub(super) fn conditional_ordered_work_mode() -> ConditionalOrderedWorkMode {
    if !CONDITIONAL_ORDERED_NEGATIVE.get() {
        ConditionalOrderedWorkMode::Off
    } else if CONDITIONAL_ORDERED_NEGATIVE_REUSE.get() {
        ConditionalOrderedWorkMode::ClosedWithin
    } else {
        ConditionalOrderedWorkMode::Point
    }
}

pub(super) fn record_conditional_ordered_negative(delta: ConditionalOrderedNegativeMetrics) {
    if delta == ConditionalOrderedNegativeMetrics::default() {
        return;
    }
    METRICS.with(|slot| {
        let mut metrics = slot.get();
        let ordered = &mut metrics.conditional_ordered_negative;
        ordered.attempts += delta.attempts;
        ordered.proofs += delta.proofs;
        ordered.checked_poses += delta.checked_poses;
        ordered.reused += delta.reused;
        slot.set(metrics);
    });
    #[cfg(feature = "local-search-ab")]
    process_totals::record_ordered_negative(delta);
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

pub(super) fn ordered_kick_guard_reuse_enabled() -> bool {
    if let Some(mode) = KICK_GUARD_REUSE_MODE.with(Cell::get) {
        return mode;
    }
    #[cfg(feature = "local-search-ab")]
    {
        static MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            std::env::var("CLEARRA_ORDERED_KICK_GUARD_REUSE").is_ok_and(|value| value == "1")
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
        ConditionalOrderMetrics, ConditionalOrderedNegativeMetrics, KickGuardReuseMetrics,
        KickIntervalMetrics, MovementClearMetrics, StructuralOrderMetrics,
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
    static KICK_GUARDS_CONSTRUCTED: AtomicU64 = AtomicU64::new(0);
    static KICK_GUARDS_STORED: AtomicU64 = AtomicU64::new(0);
    static KICK_GUARDS_REUSED: AtomicU64 = AtomicU64::new(0);
    static KICK_GUARD_CONFLICTS: AtomicU64 = AtomicU64::new(0);
    static KICK_GUARD_EXACT_RETRIES: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_CERTIFIED: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_EMPTY: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_UNKNOWN: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_BUDGET: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_TRANSITIONS: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_STATES: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_BYPASSES: AtomicU64 = AtomicU64::new(0);
    static CONDITIONAL_REUSED: AtomicU64 = AtomicU64::new(0);
    static NEGATIVE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
    static NEGATIVE_PROOFS: AtomicU64 = AtomicU64::new(0);
    static NEGATIVE_POSES: AtomicU64 = AtomicU64::new(0);
    static NEGATIVE_REUSES: AtomicU64 = AtomicU64::new(0);

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
            ordered_kick_guard_reuse: KickGuardReuseMetrics {
                constructed: KICK_GUARDS_CONSTRUCTED.load(Ordering::Relaxed),
                stored: KICK_GUARDS_STORED.load(Ordering::Relaxed),
                reused_selected: KICK_GUARDS_REUSED.load(Ordering::Relaxed),
                conflicts: KICK_GUARD_CONFLICTS.load(Ordering::Relaxed),
                exact_retries: KICK_GUARD_EXACT_RETRIES.load(Ordering::Relaxed),
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
            conditional_ordered_negative: ConditionalOrderedNegativeMetrics {
                attempts: NEGATIVE_ATTEMPTS.load(Ordering::Relaxed),
                proofs: NEGATIVE_PROOFS.load(Ordering::Relaxed),
                checked_poses: NEGATIVE_POSES.load(Ordering::Relaxed),
                reused: NEGATIVE_REUSES.load(Ordering::Relaxed),
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

    pub(super) fn record_ordered_kick_guard_reuse(delta: KickGuardReuseMetrics) {
        KICK_GUARDS_CONSTRUCTED.fetch_add(delta.constructed, Ordering::Relaxed);
        KICK_GUARDS_STORED.fetch_add(delta.stored, Ordering::Relaxed);
        KICK_GUARDS_REUSED.fetch_add(delta.reused_selected, Ordering::Relaxed);
        KICK_GUARD_CONFLICTS.fetch_add(delta.conflicts, Ordering::Relaxed);
        KICK_GUARD_EXACT_RETRIES.fetch_add(delta.exact_retries, Ordering::Relaxed);
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

    pub(super) fn record_ordered_negative(delta: ConditionalOrderedNegativeMetrics) {
        NEGATIVE_ATTEMPTS.fetch_add(delta.attempts, Ordering::Relaxed);
        NEGATIVE_PROOFS.fetch_add(delta.proofs, Ordering::Relaxed);
        NEGATIVE_POSES.fetch_add(delta.checked_poses, Ordering::Relaxed);
        NEGATIVE_REUSES.fetch_add(delta.reused, Ordering::Relaxed);
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

pub(super) fn record_ordered_kick_guard_reuse(delta: KickGuardReuseMetrics) {
    if delta == KickGuardReuseMetrics::default() {
        return;
    }
    METRICS.with(|slot| {
        let mut value = slot.get();
        let metrics = &mut value.ordered_kick_guard_reuse;
        metrics.constructed += delta.constructed;
        metrics.stored += delta.stored;
        metrics.reused_selected += delta.reused_selected;
        metrics.conflicts += delta.conflicts;
        metrics.exact_retries += delta.exact_retries;
        slot.set(value);
    });
    #[cfg(feature = "local-search-ab")]
    process_totals::record_ordered_kick_guard_reuse(delta);
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

/// Execute the existing conditional language on this thread and return its
/// actual counters for this call, not the process totals. A fixed single
/// worker is required; never change an existing request's worker policy.
#[cfg(feature = "local-search-ab")]
pub fn run_conditional_ordered_work(
    problem: &clearra_problem::SearchProblem,
    mode: ConditionalOrderedWorkMode,
) -> Result<(crate::CoreExecutionResult, StructuralOrderMetrics), String> {
    use clearra_core_domain::execution_cancellation::ExecutionControl;
    use clearra_pc_graph::request::WorkerPolicy;

    if problem.backend_policy().worker_policy() != WorkerPolicy::Fixed(1)
        || problem.backend_policy().workers() != 1
    {
        return Err("conditional ordered work requires an explicitly fixed single worker".into());
    }
    if super::placement_dependency::enabled()
        || translation_intervals_enabled()
        || movement_clear_certificates_enabled()
        || first_kick_intervals_enabled()
        || ordered_kick_intervals_enabled()
        || ordered_kick_guard_reuse_enabled()
    {
        return Err("conditional ordered work requires the other movement proofs off".into());
    }
    let (result, work) = with_conditional_ordered_negative_reuse(
        mode == ConditionalOrderedWorkMode::ClosedWithin,
        || {
            with_conditional_ordered_negative(mode != ConditionalOrderedWorkMode::Off, || {
                with_conditional_order_mode(true, || {
                    with_mode(false, || {
                        crate::WasmCpuSearchBackend::execute_with_control(
                            problem,
                            &ExecutionControl::default(),
                        )
                    })
                })
            })
        },
    );
    let result = result.map_err(|error| format!("conditional ordered work: {error:?}"))?;
    if result.usize_field("workers_used") != Some(1) {
        return Err("conditional ordered work was not executed by one worker".into());
    }
    Ok((result, work))
}

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
pub(super) fn with_ordered_kick_guard_reuse_mode<T>(
    enabled: bool,
    action: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            KICK_GUARD_REUSE_MODE.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(KICK_GUARD_REUSE_MODE.with(|slot| slot.replace(Some(enabled))));
    action()
}

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

pub(super) fn with_conditional_ordered_negative<T>(enabled: bool, action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            CONDITIONAL_ORDERED_NEGATIVE.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(CONDITIONAL_ORDERED_NEGATIVE.with(|slot| slot.replace(enabled)));
    action()
}

pub(super) fn with_conditional_ordered_negative_reuse<T>(
    enabled: bool,
    action: impl FnOnce() -> T,
) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            CONDITIONAL_ORDERED_NEGATIVE_REUSE.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(CONDITIONAL_ORDERED_NEGATIVE_REUSE.with(|slot| slot.replace(enabled)));
    action()
}
