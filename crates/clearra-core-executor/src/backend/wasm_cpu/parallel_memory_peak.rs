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
const PHASE_NAMES: [&str; 5] = [
    "pre-dispatch",
    "geometry-preparation",
    "parallel-verification",
    "merge",
    "result-finalization",
];

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
    fn phase_index(self) -> usize {
        match self {
            Self::BeforeDispatch => 0,
            Self::Geometry { .. } => 1,
            Self::Verifying { .. } => 2,
            Self::Merging => 3,
            Self::Finalizing => 4,
        }
    }

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
    fn json(&self, started: Instant) -> Value {
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

#[derive(Clone, Copy)]
struct RssObservation {
    rss_bytes: u64,
    at: Instant,
}

impl RssObservation {
    fn json(self, started: Instant) -> Value {
        json!({
            "rss_bytes": self.rss_bytes,
            "elapsed_ms": self.at.saturating_duration_since(started).as_millis(),
        })
    }
}

#[derive(Default)]
struct PhaseObservations {
    samples: usize,
    first: Option<RssObservation>,
    last: Option<RssObservation>,
    peak: Option<Arc<Peak>>,
}

impl PhaseObservations {
    fn json(self, index: usize, started: Instant) -> Value {
        json!({
            "phase": PHASE_NAMES[index],
            "observed": self.samples != 0,
            "rss_sample_count": self.samples,
            "first_rss_observation": self.first.map(|sample| sample.json(started)),
            "last_rss_observation": self.last.map(|sample| sample.json(started)),
            // A phase may be revisited. This is an observation range, not its
            // active execution time or a claim to have sampled its boundaries.
            "first_to_last_observation_span_ms": self.first.zip(self.last).map(|(first, last)| last.at.saturating_duration_since(first.at).as_millis()),
            "peak_rss_observation": self.peak.as_ref().map(|peak| peak.json(started)),
        })
    }
}

#[derive(Default)]
struct MemoryObservations {
    samples: usize,
    max_hwm: u64,
    peak: Option<Arc<Peak>>,
    // Fixed phase slots retain at most one owner snapshot each. A global
    // peak shares its allocation with the corresponding phase peak.
    phases: [PhaseObservations; PHASE_NAMES.len()],
}

impl MemoryObservations {
    fn record(&mut self, rss_bytes: u64, kernel_hwm: u64, at: Instant, owners: &LiveOwners) {
        self.samples += 1;
        self.max_hwm = self.max_hwm.max(kernel_hwm);
        let phase = &mut self.phases[owners.coordinator.phase_index()];
        let sample = RssObservation { rss_bytes, at };
        phase.samples += 1;
        phase.first.get_or_insert(sample);
        phase.last = Some(sample);
        let global_peak = self
            .peak
            .as_ref()
            .is_none_or(|previous| rss_bytes > previous.rss_bytes);
        let phase_peak = phase
            .peak
            .as_ref()
            .is_none_or(|previous| rss_bytes > previous.rss_bytes);
        if global_peak || phase_peak {
            // Ordinary polling changes only scalar observations. Clone the
            // published worker slots once when either peak actually changes.
            let snapshot = Arc::new(Peak {
                rss_bytes,
                preceding_kernel_hwm_bytes: kernel_hwm,
                at,
                coordinator: owners.coordinator,
                coordinator_published_at: owners.coordinator_published_at,
                catalog_at_dispatch_bytes: owners.catalog_at_dispatch_bytes,
                workers: owners.workers.clone(),
            });
            if global_peak {
                self.peak = Some(Arc::clone(&snapshot));
            }
            if phase_peak {
                phase.peak = Some(snapshot);
            }
        }
    }

    fn json(self, started: Instant) -> Value {
        json!({
            "schema": "clearra.native-pc-memory-observation.v1",
            "interpretation": "logical capacities from last owner publications, not allocator bytes; kernel HWM may have occurred between polls; unsampled workspace temporaries, allocators, and handoff transients are not attributed",
            "sampling_interval_ms": INTERVAL.as_millis(),
            "worker_candidate_sample_interval": WORKER_SAMPLE_MASK + 1,
            "rss_sample_count": self.samples,
            "kernel_hwm_bytes_seen": self.max_hwm,
            "peak_rss_observation": self.peak.as_ref().map(|snapshot| snapshot.json(started)),
            "phase_observation_interpretation": "phase names come from the latest coordinator publication; first/last samples are not phase boundaries; revisited phases share one observation range; unobserved phases have zero samples and null observations, not zero memory; phase peaks include only the last published owners, with their ages",
            "phase_observations": self.phases.into_iter().enumerate().map(|(index, phase)| phase.json(index, started)).collect::<Vec<_>>(),
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
    let mut observations = MemoryObservations::default();
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
        observations.record(rss_bytes, kernel_hwm, at, &owners);
        drop(owners);
        if probe.stop.load(Ordering::Acquire) {
            break;
        }
        thread::park_timeout(INTERVAL);
    }
    Ok(observations.json(probe.started))
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
    fn phase_samples_preserve_owner_age_and_distinguish_unobserved_phases() {
        let started = Instant::now();
        let at = |millis| started + Duration::from_millis(millis);
        let geometry = CoordinatorStorage::Geometry {
            accounted_bytes: 77,
            target_preparation_upper_bound: true,
            residual_memo_bytes: 11,
            solution_family_bytes: 22,
        };
        let mut owners = LiveOwners {
            coordinator: CoordinatorStorage::BeforeDispatch,
            coordinator_published_at: started,
            catalog_at_dispatch_bytes: None,
            workers: Vec::new(),
        };
        let mut observations = MemoryObservations::default();
        observations.record(100, 110, at(10), &owners);
        observations.record(90, 120, at(15), &owners);
        owners.coordinator = geometry;
        owners.coordinator_published_at = at(16);
        observations.record(80, 120, at(17), &owners);
        owners.coordinator = CoordinatorStorage::Verifying {
            target_index_bytes: 11,
            solution_family_bytes: 22,
            global_coverage_words_bytes: 33,
        };
        owners.coordinator_published_at = at(18);
        owners.catalog_at_dispatch_bytes = Some(23);
        owners.workers = vec![
            Some(PublishedWorker {
                at: at(19),
                storage: WorkerStorage {
                    solution_identity_bytes: 42,
                    ..Default::default()
                },
                returned: false,
            }),
            None,
        ];
        observations.record(130, 150, at(20), &owners);
        let peak = Arc::clone(observations.peak.as_ref().unwrap());
        assert!(Arc::ptr_eq(
            &peak,
            observations.phases[2].peak.as_ref().unwrap()
        ));
        owners.workers[0]
            .as_mut()
            .unwrap()
            .storage
            .solution_identity_bytes = 99;
        owners.workers[0].as_mut().unwrap().at = at(21);
        observations.record(120, 150, at(23), &owners);
        assert!(Arc::ptr_eq(
            &peak,
            observations.phases[2].peak.as_ref().unwrap()
        ));
        assert_eq!(
            peak.workers[0].unwrap().storage.solution_identity_bytes,
            42,
            "a later publication cannot rewrite a retained peak"
        );
        // A revisited phase changes its last observation and local peak even
        // while the global peak stays in verification. Merge is unsampled.
        owners.workers.clear();
        owners.coordinator = geometry;
        owners.coordinator_published_at = at(26);
        observations.record(95, 150, at(27), &owners);
        owners.coordinator = CoordinatorStorage::Finalizing;
        owners.coordinator_published_at = at(28);
        observations.record(100, 150, at(30), &owners);
        let report = observations.json(started);
        assert_eq!(report["rss_sample_count"], 7);
        assert_eq!(report["kernel_hwm_bytes_seen"], 150);
        let global = &report["peak_rss_observation"];
        assert_eq!(global["rss_bytes"], 130);
        assert_eq!(global["elapsed_ms"], 20);
        assert_eq!(global["coordinator_last_publication_age_ms"], 2);
        assert_eq!(global["oldest_worker_observation_ms"], 1);
        assert_eq!(global["expected_worker_slots_in_phase"], 2);
        assert_eq!(
            global["worker_storage_last_publications"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            global["worker_storage_last_publications_sum"]
                ["returned_or_pending_solution_identities_bytes"],
            42
        );
        let phases = report["phase_observations"].as_array().unwrap();
        assert_eq!(phases.len(), PHASE_NAMES.len());
        assert_eq!(phases[1]["rss_sample_count"], 2);
        assert_eq!(phases[1]["first_rss_observation"]["rss_bytes"], 80);
        assert_eq!(phases[1]["last_rss_observation"]["rss_bytes"], 95);
        assert_eq!(phases[1]["first_to_last_observation_span_ms"], 10);
        assert_eq!(phases[1]["peak_rss_observation"]["rss_bytes"], 95);
        assert_eq!(phases[2]["peak_rss_observation"], *global);
        assert_eq!(phases[3]["observed"], false);
        assert_eq!(phases[3]["rss_sample_count"], 0);
        for field in [
            "first_rss_observation",
            "last_rss_observation",
            "first_to_last_observation_span_ms",
            "peak_rss_observation",
        ] {
            assert!(phases[3][field].is_null(), "unobserved {field}");
        }
        assert_eq!(
            phases[4]["peak_rss_observation"]["expected_worker_slots_in_phase"],
            0
        );
        assert!(phases[4]["peak_rss_observation"]["oldest_worker_observation_ms"].is_null());
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
        let phases = evidence["phase_observations"].as_array().unwrap();
        assert_eq!(phases.len(), PHASE_NAMES.len());
        assert_eq!(
            phases
                .iter()
                .map(|phase| phase["rss_sample_count"].as_u64().unwrap())
                .sum::<u64>(),
            evidence["rss_sample_count"].as_u64().unwrap()
        );
    }
}
