//! Request-local A/B controls for the placement-condition prototype.
//!
//! This module is compiled only for tests or `local-search-ab`. Ordinary
//! product builds retain the existing exact search. No flag grants a global
//! pruning proof, changes supply branches, or changes counting policy.

use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct PlacementDependencyMetrics {
    pub queries: u64,
    pub generated_relations: u64,
    pub reverse_relations: u64,
    pub full_relations: u64,
    pub reused_relations: u64,
    pub cross_board_reuses: u64,
    pub independent_placement_edges: u64,
    pub frame_misses: u64,
    pub static_masks_compiled: u64,
    pub static_independent_cells: u64,
    pub static_reuses: u64,
    pub catalogs_compiled: u64,
    pub row_conditions_specialized: u64,
    pub shared_clear_domains: u64,
    pub catalog_placements: u64,
    pub unique_clear_domains: u64,
    pub row_condition_groups: u64,
    pub completed_row_events: u64,
    pub atomic_row_groups: u64,
    pub row_condition_peak_bytes: u64,
    // States counted by dependency observers, not all production reachability
    // work: observers that return before recording a result are excluded.
    pub primitive_states: u64,
    // Actual fused canonicalization calls and their input graph sizes.
    pub fused_language_graphs: u64,
    pub fused_language_nodes: u64,
    pub fused_language_edges: u64,
    // Fused-path graphs with no accepting terminal need no language pass.
    pub skipped_dead_language_graphs: u64,
    pub skipped_dead_language_nodes: u64,
}

thread_local! {
    static MODE: Cell<Option<bool>> = const { Cell::new(None) };
    static METRICS: Cell<PlacementDependencyMetrics> = Cell::new(PlacementDependencyMetrics::default());
}

#[cfg(test)]
pub(super) fn has_explicit_scope() -> bool {
    MODE.get().is_some()
}

pub(super) fn enabled() -> bool {
    if let Some(value) = MODE.with(Cell::get) {
        return value;
    }
    #[cfg(feature = "local-search-ab")]
    {
        // The experiment is selected at process start. A/B runs use separate
        // processes; do not allocate/read the environment in every ILC/lock
        // query. Scoped unit-test overrides above remain independent.
        static PROCESS_MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *PROCESS_MODE.get_or_init(|| {
            std::env::var("CLEARRA_PLACEMENT_DEPENDENCY").is_ok_and(|value| value == "1")
        })
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        false
    }
}

pub(super) fn record(update: impl FnOnce(&mut PlacementDependencyMetrics)) {
    METRICS.with(|slot| {
        let mut value = slot.get();
        update(&mut value);
        slot.set(value);
    });
}

/// Isolated same-thread experiment. The previous mode and counters are
/// restored on both normal return and unwind; concurrent tests cannot change
/// each other's solver behavior. Worker-count policy is not modified.
#[cfg(test)]
pub(super) fn with_mode<T>(
    enabled: bool,
    action: impl FnOnce() -> T,
) -> (T, PlacementDependencyMetrics) {
    struct Restore {
        mode: Option<bool>,
        metrics: PlacementDependencyMetrics,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            MODE.with(|slot| slot.set(self.mode));
            METRICS.with(|slot| slot.set(self.metrics));
        }
    }
    let _restore = Restore {
        mode: MODE.with(|slot| slot.replace(Some(enabled))),
        metrics: METRICS.with(|slot| slot.replace(PlacementDependencyMetrics::default())),
    };
    let output = action();
    (output, METRICS.with(Cell::get))
}
