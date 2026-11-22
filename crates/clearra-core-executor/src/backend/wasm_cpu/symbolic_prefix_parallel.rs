//! One native mapping experiment and its completed-worker evidence.

use std::{
    cell::RefCell,
    collections::HashSet,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
    thread::{self, ThreadId},
};

use clearra_core_domain::execution_cancellation::ExecutionControl;

use super::{
    catalog::GeometryCatalog,
    symbolic_prefix_orders::{CatalogStaticGuards, SymbolicPrefixRefusal, SymbolicPrefixWork},
    WasmExactSearchError,
};

const REPORT_UNAVAILABLE: &str = "wasm_strict_parallel_prefix_report_unavailable";
const ALREADY_CLAIMED: &str = "wasm_strict_parallel_prefix_requires_one_search";
const WORKER_MISMATCH: &str = "wasm_strict_parallel_prefix_worker_evidence_mismatch";
const DUPLICATE_WORKER: &str = "wasm_strict_parallel_prefix_duplicate_worker";
const REPORT_MISMATCH: &str = "wasm_strict_parallel_prefix_report_mismatch";
const REPORT_OVERFLOW: &str = "wasm_strict_parallel_prefix_report_overflow";

/// Work counts from completed native workers, not timings, RSS, or proof of
/// a matching coverage stream. A caller must compare that stream separately.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StrictParallelPrefixReport {
    pub workers: usize,
    pub executing_workers: usize,
    pub candidates: usize,
    pub reverse_relations: u64,
    pub unresolved_lanes: u64,
    pub pruned_lanes: u64,
    pub subsumed_lanes: u64,
    pub frontier_compilations: u64,
    pub pose_work: u64,
    pub worklist_pops: u64,
    pub pose_domains: u64,
    pub ordered_kick_terms: u64,
    /// Largest immutable template reduction observed by any completed worker.
    pub global_kick_pruned_edges_peak: u64,
    pub catalog_static_guard_hits: u64,
    /// Shared table payload once, not per-worker bytes or RSS.
    pub catalog_static_guard_bytes: usize,
    /// Largest logical capacity of a worker's sampled relation, not RSS.
    pub peak_relation_logical_bytes: usize,
    pub sealed: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Unclaimed,
    Claimed,
    Sealed,
    Closed,
}

struct RequestState {
    owner: ThreadId,
    phase: Phase,
    published: HashSet<ThreadId>,
    report: StrictParallelPrefixReport,
}

/// A worker can use only the explicit token supplied by the request's
/// coordinator. No worker tries to recover its request from caller TLS.
#[derive(Clone)]
pub(super) struct StrictParallelPrefixRequest {
    state: Arc<Mutex<RequestState>>,
    legacy_upper: bool,
    catalog_static_guards: Option<Arc<CatalogStaticGuardSlot>>,
}

type CatalogStaticGuardSlot = OnceLock<Result<Arc<CatalogStaticGuards>, SymbolicPrefixRefusal>>;

thread_local! {
    static ACTIVE_REQUEST: RefCell<Option<StrictParallelPrefixRequest>> =
        const { RefCell::new(None) };
}

/// Give one synchronous native parallel search its own evidence owner.
/// An unsealed returned report must never be used as completion evidence.
pub fn with_strict_parallel_prefix_experiment<T>(
    run: impl FnOnce() -> T,
) -> (T, StrictParallelPrefixReport) {
    with_strict_parallel_prefix_experiment_mode(false, false, run)
}

/// Compare the original per-word upper calculation in a native diagnostic.
pub fn with_legacy_strict_parallel_prefix_experiment<T>(
    run: impl FnOnce() -> T,
) -> (T, StrictParallelPrefixReport) {
    with_strict_parallel_prefix_experiment_mode(true, false, run)
}

/// Compare request-owned catalog guard reuse without changing the verifier,
/// solution language, or the default strict prefix experiment.
pub fn with_catalog_guard_strict_parallel_prefix_experiment<T>(
    run: impl FnOnce() -> T,
) -> (T, StrictParallelPrefixReport) {
    with_strict_parallel_prefix_experiment_mode(false, true, run)
}

fn with_strict_parallel_prefix_experiment_mode<T>(
    legacy_upper: bool,
    catalog_static_guards: bool,
    run: impl FnOnce() -> T,
) -> (T, StrictParallelPrefixReport) {
    struct Restore {
        previous: Option<StrictParallelPrefixRequest>,
        current: StrictParallelPrefixRequest,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Ok(mut state) = self.current.state() {
                state.phase = Phase::Closed;
            }
            ACTIVE_REQUEST.with(|slot| slot.replace(self.previous.take()));
        }
    }

    let request = StrictParallelPrefixRequest {
        state: Arc::new(Mutex::new(RequestState {
            owner: thread::current().id(),
            phase: Phase::Unclaimed,
            published: HashSet::new(),
            report: StrictParallelPrefixReport::default(),
        })),
        legacy_upper,
        catalog_static_guards: catalog_static_guards.then(|| Arc::new(OnceLock::new())),
    };
    let previous = ACTIVE_REQUEST.with(|slot| slot.replace(Some(request.clone())));
    let _restore = Restore {
        previous,
        current: request.clone(),
    };
    let result = run();
    let report = request
        .state()
        .map(|state| state.report)
        .unwrap_or_default();
    (result, report)
}

pub(super) fn strict_parallel_prefix_requested() -> bool {
    ACTIVE_REQUEST.with(|slot| slot.borrow().is_some())
}

pub(super) fn claim_strict_parallel_prefix(
) -> Result<Option<StrictParallelPrefixRequest>, WasmExactSearchError> {
    let Some(request) = ACTIVE_REQUEST.with(|slot| slot.borrow().clone()) else {
        return Ok(None);
    };
    {
        let mut state = request.state()?;
        if state.phase != Phase::Unclaimed {
            return Err(WasmExactSearchError::InvalidProblem(ALREADY_CLAIMED));
        }
        state.phase = Phase::Claimed;
    }
    Ok(Some(request))
}

impl StrictParallelPrefixRequest {
    pub(super) fn legacy_upper(&self) -> bool {
        self.legacy_upper
    }

    pub(super) fn catalog_static_guards(
        &self,
        catalog: &Arc<GeometryCatalog>,
        control: &ExecutionControl,
    ) -> Result<Option<Arc<CatalogStaticGuards>>, WasmExactSearchError> {
        let Some(slot) = &self.catalog_static_guards else {
            return Ok(None);
        };
        slot.get_or_init(|| {
            CatalogStaticGuards::compile(Arc::clone(catalog), control).map(Arc::new)
        })
        .as_ref()
        .map(|table| Some(Arc::clone(table)))
        .map_err(|reason| match reason {
            SymbolicPrefixRefusal::Cancelled => WasmExactSearchError::Cancelled,
            _ => WasmExactSearchError::InvalidProblem(
                "wasm_strict_parallel_prefix_catalog_static_guard_table_unavailable",
            ),
        })
    }

    fn state(&self) -> Result<MutexGuard<'_, RequestState>, WasmExactSearchError> {
        self.state
            .lock()
            .map_err(|_| WasmExactSearchError::InvalidProblem(REPORT_UNAVAILABLE))
    }

    pub(super) fn publish_worker(
        &self,
        candidate_count: usize,
        work: SymbolicPrefixWork,
    ) -> Result<(), WasmExactSearchError> {
        let mut state = self.state()?;
        if state.phase != Phase::Claimed
            || work.candidates_attempted != candidate_count
            || work.candidates_certified != candidate_count
            || work.frame_kind_groups != 0
            || work.pose.attempted_relations != work.pose.completed_relations
            || u64::try_from(work.target_operations).ok() != Some(work.pose.completed_relations)
            || work.pose.lane_boards != work.upper_remaining_unknown_edges
        {
            return Err(WasmExactSearchError::InvalidProblem(WORKER_MISMATCH));
        }
        let worker = thread::current().id();
        if state.published.contains(&worker) {
            return Err(WasmExactSearchError::InvalidProblem(DUPLICATE_WORKER));
        }

        // Do not modify the aggregate on a refused or overflowing receipt.
        let mut report = state.report;
        macro_rules! add {
            ($field:ident, $value:expr) => {
                report.$field = report
                    .$field
                    .checked_add($value)
                    .ok_or(WasmExactSearchError::InvalidProblem(REPORT_OVERFLOW))?;
            };
        }
        add!(workers, 1);
        add!(executing_workers, usize::from(candidate_count != 0));
        add!(candidates, candidate_count);
        add!(reverse_relations, work.pose.completed_relations);
        add!(unresolved_lanes, work.upper_remaining_unknown_edges);
        add!(pruned_lanes, work.upper_pruned_unknown_edges);
        add!(subsumed_lanes, work.subsumed_lanes);
        add!(frontier_compilations, work.frontier_compilations);
        add!(pose_work, work.pose.reached_pose_lanes);
        add!(worklist_pops, work.pose.worklist_pops);
        add!(pose_domains, work.pose.pose_domains);
        add!(ordered_kick_terms, work.pose.ordered_kick_terms);
        report.global_kick_pruned_edges_peak = report
            .global_kick_pruned_edges_peak
            .max(work.pose.global_kick_pruned_edges_peak);
        add!(catalog_static_guard_hits, work.catalog_static_guard_hits);
        report.catalog_static_guard_bytes = report
            .catalog_static_guard_bytes
            .max(work.catalog_static_guard_bytes);
        report.peak_relation_logical_bytes = report
            .peak_relation_logical_bytes
            .max(work.pose.peak_logical_bytes);
        state
            .published
            .try_reserve(1)
            .map_err(|_| WasmExactSearchError::InvalidProblem(REPORT_UNAVAILABLE))?;
        state.published.insert(worker);
        state.report = report;
        Ok(())
    }

    pub(super) fn seal(
        &self,
        actual_workers: usize,
        candidate_count: usize,
    ) -> Result<(), WasmExactSearchError> {
        let mut state = self.state()?;
        if state.phase != Phase::Claimed
            || state.owner != thread::current().id()
            || state.published.len() != actual_workers
            || state.report.workers != actual_workers
            || state.report.executing_workers < 2
            || state.report.candidates != candidate_count
        {
            return Err(WasmExactSearchError::InvalidProblem(REPORT_MISMATCH));
        }
        state.phase = Phase::Sealed;
        state.report.sealed = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(candidates: usize, lanes: u64, peak: usize) -> SymbolicPrefixWork {
        let mut work = SymbolicPrefixWork {
            candidates_attempted: candidates,
            candidates_certified: candidates,
            target_operations: 1,
            upper_remaining_unknown_edges: lanes,
            upper_pruned_unknown_edges: 3,
            ..SymbolicPrefixWork::default()
        };
        work.pose.attempted_relations = 1;
        work.pose.completed_relations = 1;
        work.pose.lane_boards = lanes;
        work.pose.reached_pose_lanes = lanes * 3;
        work.pose.peak_logical_bytes = peak;
        work
    }

    #[test]
    fn strict_parallel_prefix_scope_aggregates_once_and_closes() {
        assert!(!strict_parallel_prefix_requested());
        assert!(claim_strict_parallel_prefix().unwrap().is_none());
        let (request, report) = with_strict_parallel_prefix_experiment(|| {
            assert!(strict_parallel_prefix_requested());
            let request = claim_strict_parallel_prefix().unwrap().unwrap();
            assert_eq!(
                claim_strict_parallel_prefix().err().unwrap().reason(),
                ALREADY_CLAIMED
            );
            request.publish_worker(4, sample(4, 2, 10)).unwrap();
            assert_eq!(
                request
                    .publish_worker(4, sample(4, 2, 10))
                    .err()
                    .unwrap()
                    .reason(),
                DUPLICATE_WORKER
            );
            thread::scope(|scope| {
                scope.spawn(|| {
                    assert!(!strict_parallel_prefix_requested());
                    assert_eq!(request.seal(1, 4).err().unwrap().reason(), REPORT_MISMATCH);
                    request.publish_worker(2, sample(2, 1, 4)).unwrap();
                });
            });
            assert_eq!(request.seal(2, 7).err().unwrap().reason(), REPORT_MISMATCH);
            request.seal(2, 6).unwrap();
            request
        });
        assert!(!strict_parallel_prefix_requested());
        assert!(report.sealed);
        assert_eq!(
            (report.workers, report.executing_workers, report.candidates),
            (2, 2, 6)
        );
        assert_eq!(
            (
                report.reverse_relations,
                report.unresolved_lanes,
                report.pruned_lanes
            ),
            (2, 3, 6)
        );
        assert_eq!(
            (report.pose_work, report.peak_relation_logical_bytes),
            (9, 10)
        );
        assert_eq!(
            request
                .publish_worker(1, sample(1, 2, 12))
                .err()
                .unwrap()
                .reason(),
            WORKER_MISMATCH
        );
    }

    #[test]
    fn strict_parallel_prefix_scope_refuses_partial_or_idle_evidence() {
        let (_, report) = with_strict_parallel_prefix_experiment(|| {
            let request = claim_strict_parallel_prefix().unwrap().unwrap();
            assert_eq!(
                request
                    .publish_worker(4, sample(3, 2, 10))
                    .err()
                    .unwrap()
                    .reason(),
                WORKER_MISMATCH
            );
            request.publish_worker(4, sample(4, 2, 10)).unwrap();
            thread::scope(|scope| {
                scope.spawn(|| request.publish_worker(0, Default::default()).unwrap());
            });
            assert_eq!(request.seal(2, 4).err().unwrap().reason(), REPORT_MISMATCH);
        });
        assert!(!report.sealed);
        assert_eq!(
            (report.workers, report.executing_workers, report.candidates),
            (2, 1, 4)
        );
    }
}
