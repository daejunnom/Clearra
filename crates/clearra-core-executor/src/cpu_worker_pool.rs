#[cfg(not(target_family = "wasm"))]
mod native {
    use std::{
        collections::VecDeque,
        sync::{Arc, Barrier, Condvar, Mutex, OnceLock},
        thread::JoinHandle,
    };

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
        jobs: Mutex<VecDeque<WorkerJob>>,
        ready: Condvar,
    }

    impl JobQueue {
        fn new() -> Self {
            Self {
                jobs: Mutex::new(VecDeque::new()),
                ready: Condvar::new(),
            }
        }

        fn push(&self, job: WorkerJob) {
            self.jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push_back(job);
            self.ready.notify_one();
        }

        fn pop(&self) -> WorkerJob {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                if let Some(job) = jobs.pop_front() {
                    return job;
                }
                jobs = self
                    .ready
                    .wait(jobs)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
    }

    struct CpuWorkerPool {
        queue: Arc<JobQueue>,
        threads: Mutex<Vec<JoinHandle<()>>>,
    }

    impl CpuWorkerPool {
        fn new() -> Self {
            Self {
                queue: Arc::new(JobQueue::new()),
                threads: Mutex::new(Vec::new()),
            }
        }

        fn ensure_background_workers(
            &self,
            background_workers: usize,
        ) -> Result<(), CpuWorkerPoolError> {
            let mut threads = self
                .threads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while threads.len() < background_workers {
                let worker_index = threads.len();
                let queue = Arc::clone(&self.queue);
                let handle = std::thread::Builder::new()
                    .name(format!("clearra-cpu-worker-{worker_index}"))
                    .spawn(move || loop {
                        queue.pop()();
                    })
                    .map_err(|_| CpuWorkerPoolError::ThreadSpawnFailed)?;
                threads.push(handle);
            }
            Ok(())
        }

        fn submit(&self, job: WorkerJob) {
            self.queue.push(job);
        }
    }

    fn pool() -> &'static CpuWorkerPool {
        static POOL: OnceLock<CpuWorkerPool> = OnceLock::new();
        POOL.get_or_init(CpuWorkerPool::new)
    }

    pub(crate) fn prewarm_cpu_workers(
        total_workers: usize,
    ) -> Result<CpuWarmupReport, CpuWorkerPoolError> {
        let total_workers = total_workers.max(1);
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
        let total_workers = total_workers.max(1);
        let background_workers = total_workers.saturating_sub(1);
        pool().ensure_background_workers(background_workers)?;
        Ok(CpuWarmupReport { total_workers })
    }

    pub(crate) fn submit_cpu_job(
        job: impl FnOnce() + Send + 'static,
    ) -> Result<(), CpuWorkerPoolError> {
        pool().submit(Box::new(job));
        Ok(())
    }
}

#[cfg(target_family = "wasm")]
mod wasm {
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
