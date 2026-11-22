#[cfg(not(target_family = "wasm"))]
#[path = "cpu_worker_topology.rs"]
mod topology;

#[cfg(not(target_family = "wasm"))]
mod native {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Condvar, Mutex, OnceLock};
    use std::thread::JoinHandle;

    use super::topology::CpuWorkerTopology;
    use super::topology::TopologyFallback;

    type WorkerJob = Box<dyn FnOnce() + Send + 'static>;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) struct CpuWarmupReport {
        total_workers: usize,
    }

    impl CpuWarmupReport {
        pub(crate) const fn total_workers(self) -> usize {
            self.total_workers
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum CpuWorkerPoolError {
        ThreadSpawnFailed,
    }

    struct JobQueue {
        state: Mutex<JobQueueState>,
        ready: Condvar,
    }

    struct JobQueueState {
        jobs: VecDeque<WorkerJob>,
        // Local test pools close and join their workers. The persistent product
        // pool lives for the process, so this adds no product dispatch state.
        #[cfg(test)]
        closed: bool,
    }

    impl JobQueue {
        fn new() -> Self {
            Self {
                state: Mutex::new(JobQueueState {
                    jobs: VecDeque::new(),
                    #[cfg(test)]
                    closed: false,
                }),
                ready: Condvar::new(),
            }
        }

        fn push(&self, job: WorkerJob) {
            self.state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .jobs
                .push_back(job);
            self.ready.notify_one();
        }

        fn pop(&self) -> Option<WorkerJob> {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                if let Some(job) = state.jobs.pop_front() {
                    return Some(job);
                }
                #[cfg(test)]
                if state.closed {
                    return None;
                }
                state = self
                    .ready
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
    }

    struct CpuWorkerPool {
        queue: Arc<JobQueue>,
        threads: Mutex<Vec<JoinHandle<()>>>,
        spawned_workers: AtomicUsize,
        growth: Mutex<PoolGrowth>,
        // One immutable, tiny topology owner shared across worker startup.
        // Search catalogs and workspaces are never cloned into this metadata.
        locality: Option<Arc<CpuWorkerTopology>>,
        #[cfg(test)]
        growth_hook: Option<Arc<dyn Fn(usize) -> Result<(), CpuWorkerPoolError> + Send + Sync>>,
        #[cfg(test)]
        affinity_attempts: Arc<AtomicUsize>,
        #[cfg(test)]
        growth_coordinators: AtomicUsize,
    }

    #[derive(Default)]
    struct PoolGrowth {
        requested_background_workers: usize,
        coordinator_running: bool,
    }

    impl CpuWorkerPool {
        fn new() -> Self {
            Self::with_topology(CpuWorkerTopology::detect())
        }

        fn with_topology(topology: Result<CpuWorkerTopology, TopologyFallback>) -> Self {
            Self {
                queue: Arc::new(JobQueue::new()),
                threads: Mutex::new(Vec::new()),
                spawned_workers: AtomicUsize::new(0),
                growth: Mutex::new(PoolGrowth::default()),
                locality: topology.ok().map(Arc::new),
                #[cfg(test)]
                growth_hook: None,
                #[cfg(test)]
                affinity_attempts: Arc::new(AtomicUsize::new(0)),
                #[cfg(test)]
                growth_coordinators: AtomicUsize::new(0),
            }
        }

        fn ensure_background_workers(
            &self,
            background_workers: usize,
        ) -> Result<(), CpuWorkerPoolError> {
            // A growth coordinator can hold the handle mutex while starting
            // later workers. An already usable worker must not wait for it.
            if self.spawned_workers.load(Ordering::Acquire) >= background_workers {
                return Ok(());
            }
            let mut threads = self
                .threads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while threads.len() < background_workers {
                let worker_index = threads.len();
                #[cfg(test)]
                if let Some(hook) = &self.growth_hook {
                    hook(worker_index)?;
                }
                let queue = Arc::clone(&self.queue);
                let locality = self.locality.as_ref().map(Arc::clone);
                #[cfg(test)]
                let affinity_attempts = Arc::clone(&self.affinity_attempts);
                let handle = std::thread::Builder::new()
                    .name(format!("clearra-cpu-worker-{worker_index}"))
                    .spawn(move || {
                        if let Some(topology) = locality {
                            #[cfg(test)]
                            affinity_attempts.fetch_add(1, Ordering::Relaxed);
                            // Bind before a job creates its private workspace,
                            // preserving first touch without copying shared data.
                            // Unsupported or stale permission leaves OS scheduling
                            // in charge and cannot discard any logical job.
                            let _ = topology
                                .affinity_for_worker(worker_index)
                                .restrict_current_thread();
                        }
                        while let Some(job) = queue.pop() {
                            job();
                        }
                    })
                    .map_err(|_| CpuWorkerPoolError::ThreadSpawnFailed)?;
                threads.push(handle);
                self.spawned_workers.store(threads.len(), Ordering::Release);
            }
            Ok(())
        }

        fn ensure_background_workers_progressively(
            self: &Arc<Self>,
            background_workers: usize,
        ) -> Result<(), CpuWorkerPoolError> {
            if background_workers == 0 {
                return Ok(());
            }

            // Guarantee one usable background worker, then let the caller begin
            // useful work while the remaining workers join the shared queue.
            self.ensure_background_workers(1)?;
            let current_workers = self.spawned_workers.load(Ordering::Acquire);
            if current_workers >= background_workers {
                return Ok(());
            }

            // Concurrent callers raise one target instead of spawning a stack
            // and coordinator for every request while later workers are slow.
            // This mutex is never held across worker creation or useful work.
            {
                let mut growth = self
                    .growth
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if self.spawned_workers.load(Ordering::Acquire) >= background_workers {
                    return Ok(());
                }
                growth.requested_background_workers =
                    growth.requested_background_workers.max(background_workers);
                if growth.coordinator_running {
                    return Ok(());
                }
                growth.coordinator_running = true;
            }
            #[cfg(test)]
            self.growth_coordinators.fetch_add(1, Ordering::Relaxed);

            // The short-lived coordinator shares only pool ownership; private
            // request workspaces remain with their original logical jobs.
            let pool = Arc::clone(self);
            if std::thread::Builder::new()
                .name("clearra-cpu-worker-growth".to_owned())
                .spawn(move || {
                    // A failed late spawn only reduces parallelism. Logical jobs
                    // remain queued and are still completed by ready workers.
                    let _ = pool.complete_background_growth();
                })
                .is_err()
            {
                // If even the lightweight growth coordinator cannot start, keep
                // the previous exact behavior as the fail-operational fallback.
                self.complete_background_growth()?;
            }
            Ok(())
        }

        fn complete_background_growth(&self) -> Result<(), CpuWorkerPoolError> {
            loop {
                let requested = self
                    .growth
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .requested_background_workers;
                let result = self.ensure_background_workers(requested);
                let mut growth = self
                    .growth
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let spawned = self.spawned_workers.load(Ordering::Acquire);
                if result.is_err() || spawned >= growth.requested_background_workers {
                    // The same lock admits subsequent requests and retires this
                    // coordinator, so a target raised at handoff cannot be lost.
                    growth.coordinator_running = false;
                    growth.requested_background_workers = spawned;
                    return result;
                }
            }
        }

        fn submit(&self, job: WorkerJob) {
            self.queue.push(job);
        }

        fn ensure_workers(
            self: &Arc<Self>,
            total_workers: usize,
        ) -> Result<CpuWarmupReport, CpuWorkerPoolError> {
            let total_workers = total_workers.max(1);
            self.ensure_background_workers_progressively(total_workers.saturating_sub(1))?;
            Ok(CpuWarmupReport { total_workers })
        }
    }

    #[cfg(test)]
    impl Drop for CpuWorkerPool {
        fn drop(&mut self) {
            {
                let mut state = self
                    .queue
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.closed = true;
            }
            self.queue.ready.notify_all();
            for thread in self
                .threads
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .drain(..)
            {
                let _ = thread.join();
            }
        }
    }

    static POOL: OnceLock<Arc<CpuWorkerPool>> = OnceLock::new();

    fn pool() -> &'static Arc<CpuWorkerPool> {
        POOL.get_or_init(|| Arc::new(CpuWorkerPool::new()))
    }

    /// Optional startup placement for a newly spawned scoped solver thread.
    /// Never initializes the pool or discovers topology. Do not call this on
    /// the request's caller thread: its affinity remains the host's authority.
    pub(crate) fn apply_worker_locality(worker_index: usize) {
        let Some(pool) = POOL.get() else {
            return;
        };
        let Some(topology) = &pool.locality else {
            return;
        };
        let _ = topology
            .affinity_for_worker(worker_index)
            .restrict_current_thread();
    }

    pub(crate) fn prewarm_cpu_workers(
        total_workers: usize,
    ) -> Result<CpuWarmupReport, CpuWorkerPoolError> {
        let total_workers = total_workers.max(1);
        if total_workers == 1 {
            return Ok(CpuWarmupReport { total_workers });
        }
        let background_workers = total_workers.saturating_sub(1);
        let pool = pool();
        pool.ensure_background_workers(background_workers)?;

        if background_workers != 0 {
            let barrier = Arc::new(Barrier::new(background_workers + 1));
            for _ in 0..background_workers {
                let barrier = Arc::clone(&barrier);
                pool.submit(Box::new(move || {
                    barrier.wait();
                }));
            }
            barrier.wait();
        }

        Ok(CpuWarmupReport { total_workers })
    }

    pub(crate) fn ensure_cpu_workers(
        total_workers: usize,
    ) -> Result<CpuWarmupReport, CpuWorkerPoolError> {
        if total_workers <= 1 {
            return Ok(CpuWarmupReport { total_workers: 1 });
        }
        pool().ensure_workers(total_workers)
    }

    pub(crate) fn submit_cpu_job(
        job: impl FnOnce() + Send + 'static,
    ) -> Result<(), CpuWorkerPoolError> {
        pool().submit(Box::new(job));
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use std::sync::atomic::Ordering;
        use std::sync::{mpsc, Arc, Mutex};
        use std::time::Duration;

        use clearra_core_ffi::native_thread_affinity::NativeAffinityError;
        use clearra_core_ffi::native_thread_affinity::NativeNumaNode;
        use clearra_core_ffi::native_thread_affinity::NativeThreadAffinity;

        use super::CpuWorkerPool;
        use super::CpuWorkerPoolError;
        use super::CpuWorkerTopology;
        use super::TopologyFallback;

        #[test]
        fn ready_worker_and_caller_do_not_wait_for_blocked_pool_growth() {
            let (growth_started, growth_receiver) = mpsc::channel();
            let (release_growth, release_receiver) = mpsc::channel();
            let release_receiver = Mutex::new(release_receiver);
            let mut pool = CpuWorkerPool::with_topology(Err(TopologyFallback::SingleNode));
            pool.growth_hook = Some(Arc::new(move |worker| {
                if worker == 1 {
                    let _ = growth_started.send(());
                    release_receiver
                        .lock()
                        .expect("growth gate")
                        .recv()
                        .map_err(|_| CpuWorkerPoolError::ThreadSpawnFailed)?;
                }
                Ok(())
            }));
            let pool = Arc::new(pool);
            assert_eq!(
                pool.ensure_workers(4)
                    .expect("first worker ready")
                    .total_workers(),
                4
            );
            growth_receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("later growth is blocked");

            let (job_sender, job_receiver) = mpsc::channel();
            pool.submit(Box::new(move || {
                let _ = job_sender.send(41);
            }));
            let (caller_sender, caller_receiver) = mpsc::channel();
            let second_request = Arc::clone(&pool);
            let caller = std::thread::spawn(move || {
                let _ = caller_sender.send(second_request.ensure_workers(4));
            });
            // Capture outcomes before releasing growth. Releasing even on
            // failure makes the regression bounded and joins every test thread.
            let job = job_receiver.recv_timeout(Duration::from_secs(2));
            let report = caller_receiver.recv_timeout(Duration::from_secs(2));
            let _ = release_growth.send(());
            caller.join().expect("second request caller");
            pool.ensure_background_workers(3)
                .expect("remaining growth finishes");
            assert_eq!(job.expect("ready worker ran during blocked growth"), 41);
            assert_eq!(
                report
                    .expect("caller did not wait for growth")
                    .expect("progressive admission")
                    .total_workers(),
                4
            );
            assert_eq!(pool.affinity_attempts.load(Ordering::Relaxed), 0);
        }

        #[test]
        fn overlapping_growth_requests_share_one_coordinator_and_keep_the_largest_target() {
            let (growth_started, growth_receiver) = mpsc::channel();
            let (largest_started, largest_receiver) = mpsc::channel();
            let (release_growth, release_receiver) = mpsc::channel();
            let release_receiver = Mutex::new(release_receiver);
            let mut pool = CpuWorkerPool::with_topology(Err(TopologyFallback::SingleNode));
            pool.growth_hook = Some(Arc::new(move |worker| {
                if worker == 1 {
                    let _ = growth_started.send(());
                    release_receiver
                        .lock()
                        .expect("growth release gate")
                        .recv()
                        .map_err(|_| CpuWorkerPoolError::ThreadSpawnFailed)?;
                }
                if worker == 6 {
                    let _ = largest_started.send(());
                }
                Ok(())
            }));
            let pool = Arc::new(pool);
            pool.ensure_workers(4).expect("first ready worker");
            growth_receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("later worker creation is blocked");

            let request_pool = Arc::clone(&pool);
            let (reports_sender, reports_receiver) = mpsc::channel();
            let requests = std::thread::spawn(move || {
                let reports =
                    [3, 8, 5, 7, 4, 8].map(|workers| request_pool.ensure_workers(workers));
                let _ = reports_sender.send(reports);
            });
            let reports = reports_receiver.recv_timeout(Duration::from_secs(2));
            let coordinators_before_release = pool.growth_coordinators.load(Ordering::Relaxed);
            let (job_sender, job_receiver) = mpsc::channel();
            pool.submit(Box::new(move || {
                let _ = job_sender.send(19);
            }));
            let ready_job = job_receiver.recv_timeout(Duration::from_secs(2));
            let _ = release_growth.send(());
            requests.join().expect("overlapping callers joined");
            let largest_target = largest_receiver.recv_timeout(Duration::from_secs(2));
            pool.ensure_background_workers(7)
                .expect("largest requested pool is available");

            for (report, requested) in reports
                .expect("no caller waited for the remaining workers")
                .into_iter()
                .zip([3, 8, 5, 7, 4, 8])
            {
                assert_eq!(
                    report.expect("progressive admission").total_workers(),
                    requested
                );
            }
            assert_eq!(ready_job.expect("ready worker ran during growth"), 19);
            largest_target.expect("the existing coordinator observed the raised target");
            assert_eq!(coordinators_before_release, 1);
            assert_eq!(pool.spawned_workers.load(Ordering::Acquire), 7);
            assert_eq!(pool.affinity_attempts.load(Ordering::Relaxed), 0);
        }

        #[test]
        fn unknown_or_single_numa_and_late_spawn_failure_keep_every_logical_job() {
            for fallback in [
                TopologyFallback::SingleNode,
                TopologyFallback::Discovery(NativeAffinityError::DiscoveryUnavailable),
                TopologyFallback::Discovery(NativeAffinityError::AmbiguousProcessorGroups),
                TopologyFallback::Discovery(NativeAffinityError::RestrictedCpuSets),
            ] {
                let (failed_sender, failed_receiver) = mpsc::channel();
                let mut pool = CpuWorkerPool::with_topology(Err(fallback));
                pool.growth_hook = Some(Arc::new(move |worker| {
                    if worker != 0 {
                        let _ = failed_sender.send(());
                        return Err(CpuWorkerPoolError::ThreadSpawnFailed);
                    }
                    Ok(())
                }));
                let pool = Arc::new(pool);
                // Greater than a Windows processor group: locality fallback
                // cannot silently report only 64 logical workers.
                let report = pool
                    .ensure_workers(97)
                    .expect("one usable background worker");
                failed_receiver
                    .recv_timeout(Duration::from_secs(2))
                    .expect("late spawn failure");
                assert_eq!(report.total_workers(), 97);
                assert!(pool.locality.is_none());
                let (sender, receiver) = mpsc::channel();
                for logical_worker in 1..report.total_workers() {
                    let sender = sender.clone();
                    pool.submit(Box::new(move || {
                        let _ = sender.send(logical_worker);
                    }));
                }
                drop(sender);
                let mut completed = Vec::new();
                for _ in 1..report.total_workers() {
                    completed.push(
                        receiver
                            .recv_timeout(Duration::from_secs(2))
                            .expect("queued logical worker"),
                    );
                }
                completed.sort_unstable();
                assert_eq!(completed, (1..97).collect::<Vec<_>>());
                assert_eq!(pool.spawned_workers.load(Ordering::Acquire), 1);
                assert_eq!(pool.affinity_attempts.load(Ordering::Relaxed), 0);
            }
        }

        #[test]
        fn rejected_worker_affinity_preserves_inherited_mask_and_job_execution() {
            let allowed =
                NativeThreadAffinity::from_cpu_ids(u16::MAX, vec![0, 1]).expect("synthetic group");
            let nodes = vec![
                NativeNumaNode::new(
                    0,
                    NativeThreadAffinity::from_cpu_ids(u16::MAX, vec![0]).expect("node zero"),
                ),
                NativeNumaNode::new(
                    1,
                    NativeThreadAffinity::from_cpu_ids(u16::MAX, vec![1]).expect("node one"),
                ),
            ];
            let topology =
                CpuWorkerTopology::from_observations(&allowed, nodes).expect("synthetic topology");
            let inherited = NativeThreadAffinity::current();
            let pool = Arc::new(CpuWorkerPool::with_topology(Ok(topology)));
            assert_eq!(
                pool.ensure_workers(2)
                    .expect("worker creation")
                    .total_workers(),
                2
            );
            let (sender, receiver) = mpsc::channel();
            pool.submit(Box::new(move || {
                let _ = sender.send(NativeThreadAffinity::current());
            }));
            assert_eq!(
                receiver
                    .recv_timeout(Duration::from_secs(2))
                    .expect("job after failed affinity"),
                inherited
            );
            assert_eq!(pool.affinity_attempts.load(Ordering::Relaxed), 1);
        }
    }
}

#[cfg(target_family = "wasm")]
mod wasm {
    pub(crate) fn apply_worker_locality(_worker_index: usize) {}

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) struct CpuWarmupReport;

    impl CpuWarmupReport {
        pub(crate) const fn total_workers(self) -> usize {
            1
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum CpuWorkerPoolError {
        ThreadsUnsupported,
    }

    pub(crate) fn prewarm_cpu_workers(
        total_workers: usize,
    ) -> Result<CpuWarmupReport, CpuWorkerPoolError> {
        if total_workers <= 1 {
            Ok(CpuWarmupReport)
        } else {
            Err(CpuWorkerPoolError::ThreadsUnsupported)
        }
    }

    pub(crate) fn ensure_cpu_workers(
        total_workers: usize,
    ) -> Result<CpuWarmupReport, CpuWorkerPoolError> {
        prewarm_cpu_workers(total_workers)
    }

    pub(crate) fn submit_cpu_job(
        _job: impl FnOnce() + Send + 'static,
    ) -> Result<(), CpuWorkerPoolError> {
        Err(CpuWorkerPoolError::ThreadsUnsupported)
    }
}

#[cfg(not(target_family = "wasm"))]
pub(crate) use native::*;
#[cfg(target_family = "wasm")]
pub(crate) use wasm::*;

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use super::*;

    #[test]
    fn progressive_pool_completes_every_submitted_logical_worker() {
        let workers = ensure_cpu_workers(4).expect("progressive worker pool");
        let background_jobs = workers.total_workers().saturating_sub(1);
        let (sender, receiver) = mpsc::channel();
        for worker_index in 0..background_jobs {
            let sender = sender.clone();
            submit_cpu_job(move || {
                sender.send(worker_index).expect("worker result receiver");
            })
            .expect("submit logical worker");
        }
        drop(sender);

        let mut completed = Vec::new();
        for _ in 0..background_jobs {
            completed.push(
                receiver
                    .recv_timeout(Duration::from_secs(2))
                    .expect("logical worker completion"),
            );
        }
        completed.sort_unstable();
        assert_eq!(completed, (0..background_jobs).collect::<Vec<_>>());
    }
}
