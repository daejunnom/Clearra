//! Opt-in native benchmark telemetry. RSS is sampled by a separate thread;
//! logical capacities are published cooperatively and need not be simultaneous.
use std::{
    cell::RefCell,
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, PoisonError,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde_json::{json, Value};

const INTERVAL: Duration = Duration::from_millis(25);
pub(super) const WORKER_SAMPLE_MASK: usize = 0xfff;

thread_local! {
    static ACTIVE: RefCell<Option<Arc<MemoryProbe>>> = const { RefCell::new(None) };
}

pub(super) fn current() -> Option<Arc<MemoryProbe>> {
    ACTIVE.with(|active| active.borrow().clone())
}

#[derive(Clone, Copy, Default)]
pub(super) struct WorkerStorage {
    pub standard_bag_bytes: usize,
    // Disjoint subcomponents of StandardBag, not additional allocations.
    pub bag_product_memo_bytes: usize,
    pub bag_union_memo_bytes: usize,
    pub piece_language_bytes: usize,
    pub reachability_bytes: usize,
    pub graph_projection_bytes: usize,
    pub other_buildup_bytes: usize,
    pub evaluator_bytes: usize,
    pub solution_identity_bytes: usize,
    pub branch_and_digest_bytes: usize,
    pub observation_producer_bytes: usize,
}

impl WorkerStorage {
    fn add(&mut self, other: Self) {
        self.standard_bag_bytes += other.standard_bag_bytes;
        self.bag_product_memo_bytes += other.bag_product_memo_bytes;
        self.bag_union_memo_bytes += other.bag_union_memo_bytes;
        self.piece_language_bytes += other.piece_language_bytes;
        self.reachability_bytes += other.reachability_bytes;
        self.graph_projection_bytes += other.graph_projection_bytes;
        self.other_buildup_bytes += other.other_buildup_bytes;
        self.evaluator_bytes += other.evaluator_bytes;
        self.solution_identity_bytes += other.solution_identity_bytes;
        self.branch_and_digest_bytes += other.branch_and_digest_bytes;
        self.observation_producer_bytes += other.observation_producer_bytes;
    }

    fn json(self) -> Value {
        json!({
            "private_standard_bag_bytes": self.standard_bag_bytes,
            "private_standard_bag_product_memo_payload_subset_bytes": self.bag_product_memo_bytes,
            "private_standard_bag_union_memo_payload_subset_bytes": self.bag_union_memo_bytes,
            "private_piece_language_bytes": self.piece_language_bytes,
            "private_reachability_bytes": self.reachability_bytes,
            "private_graph_projection_bytes": self.graph_projection_bytes,
            "private_other_buildup_bytes": self.other_buildup_bytes,
            "private_evaluator_bytes": self.evaluator_bytes,
            "returned_or_pending_solution_identities_bytes": self.solution_identity_bytes,
            "returned_or_pending_branch_slots_and_digest_bytes": self.branch_and_digest_bytes,
            "returned_or_pending_observation_producer_bytes": self.observation_producer_bytes,
        })
    }
}

#[derive(Clone, Copy)]
struct PublishedWorker {
    at: Instant,
    storage: WorkerStorage,
    returned: bool,
}

#[derive(Clone, Copy)]
pub(super) enum CoordinatorStorage {
    BeforeDispatch,
    Geometry {
        accounted_bytes: usize,
        target_preparation_upper_bound: bool,
        residual_memo_bytes: usize,
        solution_family_bytes: usize,
    },
    Verifying {
        target_index_bytes: usize,
        solution_family_bytes: usize,
        global_coverage_words_bytes: usize,
    },
    Merging,
    Finalizing,
}

impl CoordinatorStorage {
    fn json(self) -> Value {
        match self {
            Self::BeforeDispatch => json!({ "phase": "pre-dispatch" }),
            Self::Geometry {
                accounted_bytes,
                target_preparation_upper_bound,
                residual_memo_bytes,
                solution_family_bytes,
            } => json!({
                "phase": "geometry-preparation",
                "accounted_geometry_bytes": accounted_bytes,
                "geometry_bytes_include_target_preparation_upper_bound": target_preparation_upper_bound,
                "geometry_residual_memo_subset_bytes": residual_memo_bytes,
                "geometry_solution_family_subset_bytes": solution_family_bytes,
            }),
            Self::Verifying {
                target_index_bytes,
                solution_family_bytes,
                global_coverage_words_bytes,
            } => json!({
                "phase": "parallel-verification",
                "dispatch_shared_target_index_bytes": target_index_bytes,
                "dispatch_shared_solution_family_bytes": solution_family_bytes,
                "global_coverage_words_bytes": global_coverage_words_bytes,
            }),
            Self::Merging => json!({ "phase": "merge" }),
            Self::Finalizing => json!({ "phase": "result-finalization" }),
        }
    }
}

struct LiveOwners {
    coordinator: CoordinatorStorage,
    coordinator_published_at: Instant,
    catalog_at_dispatch_bytes: Option<usize>,
    workers: Vec<Option<PublishedWorker>>,
}

pub(super) struct MemoryProbe {
    started: Instant,
    stop: AtomicBool,
    owners: Mutex<LiveOwners>,
}

impl MemoryProbe {
    fn owners(&self) -> MutexGuard<'_, LiveOwners> {
        self.owners.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn coordinator(&self, storage: CoordinatorStorage) {
        let mut owners = self.owners();
        if matches!(
            storage,
            CoordinatorStorage::Merging | CoordinatorStorage::Finalizing
        ) {
            // Worker payload ownership has changed; do not attribute missing
            // merge/finalizer transients to the worker publications.
            owners.workers.clear();
        }
        owners.coordinator = storage;
        owners.coordinator_published_at = Instant::now();
    }

    pub fn begin_verify(
        &self,
        worker_count: usize,
        catalog_at_dispatch_bytes: usize,
        storage: CoordinatorStorage,
    ) {
        let mut owners = self.owners();
        owners.workers.resize(worker_count, None);
        owners.catalog_at_dispatch_bytes = Some(catalog_at_dispatch_bytes);
        owners.coordinator = storage;
        owners.coordinator_published_at = Instant::now();
    }

    pub fn worker(&self, index: usize, storage: WorkerStorage, returned: bool) {
        let mut owners = self.owners();
        if let Some(worker) = owners.workers.get_mut(index) {
            *worker = Some(PublishedWorker {
                at: Instant::now(),
                storage,
                returned,
            });
        }
    }
}

struct Peak {
    rss_bytes: u64,
    preceding_kernel_hwm_bytes: u64,
    at: Instant,
    coordinator: CoordinatorStorage,
    coordinator_published_at: Instant,
    catalog_at_dispatch_bytes: Option<usize>,
    workers: Vec<Option<PublishedWorker>>,
}

impl Peak {
    fn json(self, started: Instant) -> Value {
        let mut sum = WorkerStorage::default();
        let mut oldest = Duration::ZERO;
        let mut worker_reports = Vec::with_capacity(self.workers.len());
        for (index, entry) in self.workers.iter().enumerate() {
            if let Some(worker) = entry {
                sum.add(worker.storage);
                let age = self.at.saturating_duration_since(worker.at);
                oldest = oldest.max(age);
                worker_reports.push(json!({
                    "index": index,
                    "last_observation_age_ms": age.as_millis(),
                    "returned": worker.returned,
                    "storage": worker.storage.json(),
                }));
            }
        }
        json!({
            "rss_bytes": self.rss_bytes,
            "kernel_hwm_bytes_reported_in_same_proc_read": self.preceding_kernel_hwm_bytes,
            "elapsed_ms": self.at.duration_since(started).as_millis(),
            "coordinator": self.coordinator.json(),
            "coordinator_last_publication_age_ms": self.at.saturating_duration_since(self.coordinator_published_at).as_millis(),
            "catalog_capacity_at_parallel_dispatch_bytes": self.catalog_at_dispatch_bytes,
            "expected_worker_slots_in_phase": self.workers.len(),
            "oldest_worker_observation_ms": (!worker_reports.is_empty()).then(|| oldest.as_millis()),
            "worker_storage_last_publications_sum": sum.json(),
            "worker_storage_last_publications": worker_reports,
        })
    }
}

fn status_bytes(status: &str, field: &str) -> Option<u64> {
    let mut parts = status
        .lines()
        .find_map(|line| line.strip_prefix(field))?
        .split_whitespace();
    let bytes = parts.next()?.parse::<u64>().ok()?.checked_mul(1024)?;
    (parts.next() == Some("kB") && parts.next().is_none()).then_some(bytes)
}

fn observe(probe: &MemoryProbe) -> io::Result<Value> {
    let mut peak: Option<Peak> = None;
    let mut max_hwm = 0;
    let mut observations = 0usize;
    loop {
        // Keep phase/publication changes out of this short proc read so all
        // last publications in a peak frame predate that RSS observation.
        let owners = probe.owners();
        let status = std::fs::read_to_string("/proc/self/status")?;
        let rss = status_bytes(&status, "VmRSS:");
        let hwm = status_bytes(&status, "VmHWM:");
        let (Some(rss_bytes), Some(kernel_hwm)) = (rss, hwm) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "missing Linux RSS fields",
            ));
        };
        let at = Instant::now();
        max_hwm = max_hwm.max(kernel_hwm);
        observations += 1;
        if peak
            .as_ref()
            .is_none_or(|previous| rss_bytes > previous.rss_bytes)
        {
            peak = Some(Peak {
                rss_bytes,
                preceding_kernel_hwm_bytes: kernel_hwm,
                at,
                coordinator: owners.coordinator,
                coordinator_published_at: owners.coordinator_published_at,
                catalog_at_dispatch_bytes: owners.catalog_at_dispatch_bytes,
                workers: owners.workers.clone(),
            });
        }
        drop(owners);
        if probe.stop.load(Ordering::Acquire) {
            break;
        }
        thread::park_timeout(INTERVAL);
    }
    Ok(json!({
        "schema": "clearra.native-pc-memory-observation.v1",
        "interpretation": "logical capacities from last owner publications, not allocator bytes; kernel HWM may have occurred between polls; unsampled workspace temporaries, allocators, and handoff transients are not attributed",
        "sampling_interval_ms": INTERVAL.as_millis(),
        "worker_candidate_sample_interval": WORKER_SAMPLE_MASK + 1,
        "rss_sample_count": observations,
        "kernel_hwm_bytes_seen": max_hwm,
        "peak_rss_observation": peak.map(|snapshot| snapshot.json(probe.started)),
    }))
}

struct Scope {
    probe: Arc<MemoryProbe>,
    sampler: Option<JoinHandle<io::Result<Value>>>,
}

impl Scope {
    fn finish(&mut self) -> io::Result<Value> {
        let sampler = self.sampler.take().expect("owned native PC memory sampler");
        self.probe.stop.store(true, Ordering::Release);
        sampler.thread().unpark();
        sampler
            .join()
            .map_err(|_| io::Error::other("Linux RSS sampler panicked"))?
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if self.sampler.is_some() {
            // Join on unwinding too; this diagnostic never outlives its probe.
            let _ = self.finish();
        }
        ACTIVE.with(|active| {
            active.replace(None);
        });
    }
}

/// Run one explicitly requested local native PC memory diagnostic. Do not
/// compare this instrumented run's time to uninstrumented ABBA samples.
pub fn observe_native_pc_memory<T>(run: impl FnOnce() -> T) -> io::Result<(T, Value)> {
    if current().is_some() {
        return Err(io::Error::other("nested native PC memory diagnostic"));
    }
    let started = Instant::now();
    let probe = Arc::new(MemoryProbe {
        started,
        stop: AtomicBool::new(false),
        owners: Mutex::new(LiveOwners {
            coordinator: CoordinatorStorage::BeforeDispatch,
            coordinator_published_at: started,
            catalog_at_dispatch_bytes: None,
            workers: Vec::new(),
        }),
    });
    let observed = Arc::clone(&probe);
    let sampler = thread::Builder::new()
        .name("clearra-pc-rss".to_owned())
        .spawn(move || observe(&observed))?;
    ACTIVE.with(|active| active.replace(Some(Arc::clone(&probe))));
    let mut scope = Scope {
        probe,
        sampler: Some(sampler),
    };
    let value = run();
    Ok((value, scope.finish()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_checked_kibibytes_in_proc_fields() {
        assert_eq!(
            status_bytes("VmHWM:\t10 kB\nVmRSS: 3 kB", "VmRSS:"),
            Some(3072)
        );
        assert_eq!(status_bytes("VmRSS: 3 MB", "VmRSS:"), None);
        assert_eq!(
            status_bytes("VmRSS: 18446744073709551615 kB", "VmRSS:"),
            None
        );
    }

    #[test]
    fn joins_sampler_and_reports_bounded_ownership_after_run() {
        let (value, evidence) = observe_native_pc_memory(|| {
            let probe = current().expect("one scoped observer");
            probe.begin_verify(1, 23, CoordinatorStorage::BeforeDispatch);
            probe.worker(
                0,
                WorkerStorage {
                    solution_identity_bytes: 42,
                    ..Default::default()
                },
                false,
            );
            7
        })
        .expect("running Linux proc observer");
        assert_eq!(value, 7);
        assert!(current().is_none());
        assert!(evidence["rss_sample_count"]
            .as_u64()
            .is_some_and(|count| count > 0));
        assert!(evidence["peak_rss_observation"]["rss_bytes"]
            .as_u64()
            .is_some());
    }
}
