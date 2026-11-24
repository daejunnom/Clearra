use std::{
    fmt,
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
};

const CANCELLED: u32 = 1;

#[derive(Clone, Debug, Default)]
pub struct ExecutionCancellationToken {
    cancelled: Arc<AtomicU32>,
}

impl ExecutionCancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle(&self) -> ExecutionCancellationHandle {
        ExecutionCancellationHandle {
            cancelled: Arc::clone(&self.cancelled),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) == CANCELLED
    }

    /// Gives the FFI owner a stable atomic flag without exposing a native pointer.
    pub fn atomic_flag(&self) -> &AtomicU32 {
        self.cancelled.as_ref()
    }
}

pub type CancellationToken = ExecutionCancellationToken;

#[derive(Clone, Debug)]
pub struct ExecutionCancellationHandle {
    cancelled: Arc<AtomicU32>,
}

impl ExecutionCancellationHandle {
    pub fn cancel(&self) {
        self.cancelled.store(CANCELLED, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) == CANCELLED
    }

    /// Exposes the shared atomic owner by reference for fieldwise ownership
    /// accounting and pointer-identity checks without exposing a raw pointer.
    pub fn atomic_flag(&self) -> &AtomicU32 {
        self.cancelled.as_ref()
    }
}

pub type CancellationHandle = ExecutionCancellationHandle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionProgress {
    pub stage: &'static str,
    pub completed: u64,
    pub total: Option<u64>,
}

impl ExecutionProgress {
    pub const fn new(stage: &'static str, completed: u64, total: Option<u64>) -> Self {
        Self {
            stage,
            completed,
            total,
        }
    }
}

pub trait ProgressSink: Send + Sync {
    fn report(&self, progress: ExecutionProgress);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionPartition {
    index: usize,
    count: usize,
}

impl ExecutionPartition {
    pub fn new(index: usize, count: usize) -> Option<Self> {
        (count > 0 && index < count).then_some(Self { index, count })
    }

    pub const fn whole() -> Self {
        Self { index: 0, count: 1 }
    }

    pub const fn index(self) -> usize {
        self.index
    }

    pub const fn count(self) -> usize {
        self.count
    }

    pub fn owns_candidate(self, candidate_id: u64) -> bool {
        candidate_id % self.count as u64 == self.index as u64
    }
}

#[derive(Clone)]
pub struct ExecutionControl {
    pub cancellation: CancellationToken,
    pub progress_sink: Option<Arc<dyn ProgressSink>>,
    partition: ExecutionPartition,
    compute_budget: Arc<ComputeBudget>,
}

/// One request's compute-worker width. The request coordinator never consumes
/// a compute slot. Descendants share this owner and reuse released permits.
#[derive(Debug)]
pub struct ComputeBudget {
    parallelism: usize,
    available: AtomicUsize,
    waiters: Mutex<()>,
    ready: Condvar,
}

impl ComputeBudget {
    fn new(parallelism: usize) -> Self {
        let parallelism = parallelism.max(1);
        Self {
            parallelism,
            available: AtomicUsize::new(parallelism),
            waiters: Mutex::new(()),
            ready: Condvar::new(),
        }
    }

    pub const fn parallelism(&self) -> usize {
        self.parallelism
    }

    pub fn try_acquire_worker(self: &Arc<Self>) -> Option<ComputePermit> {
        self.available
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |available| {
                available.checked_sub(1)
            })
            .ok()
            .map(|_| ComputePermit { budget: Arc::clone(self) })
    }

    pub fn acquire_worker(
        self: &Arc<Self>,
        cancellation: &CancellationToken,
    ) -> Option<ComputePermit> {
        let mut waiting = self.waiters.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if cancellation.is_cancelled() {
                return None;
            }
            if let Some(permit) = self.try_acquire_worker() {
                return Some(permit);
            }
            waiting = self.ready.wait_timeout(waiting, std::time::Duration::from_millis(25))
                .unwrap_or_else(std::sync::PoisonError::into_inner).0;
        }
    }
}

#[derive(Debug)]
pub struct ComputePermit {
    budget: Arc<ComputeBudget>,
}

impl ComputePermit {
    pub(crate) fn budget_identity(&self) -> usize {
        Arc::as_ptr(&self.budget) as usize
    }
}

impl Drop for ComputePermit {
    fn drop(&mut self) {
        self.budget.available.fetch_add(1, Ordering::Release);
        self.budget.ready.notify_one();
    }
}

impl ExecutionControl {
    pub fn new(cancellation: CancellationToken) -> Self {
        Self {
            cancellation,
            progress_sink: None,
            partition: ExecutionPartition::whole(),
            compute_budget: Arc::new(ComputeBudget::new(1)),
        }
    }

    pub fn with_progress_sink(mut self, progress_sink: Arc<dyn ProgressSink>) -> Self {
        self.progress_sink = Some(progress_sink);
        self
    }

    pub fn with_partition(mut self, partition: ExecutionPartition) -> Self {
        self.partition = partition;
        self
    }

    /// Set the admitted request width at the product boundary. Passing the
    /// same width preserves the shared owner through composed child stages.
    pub fn with_parallelism(mut self, parallelism: usize) -> Self {
        let parallelism = parallelism.max(1);
        if self.parallelism() != parallelism {
            self.compute_budget = Arc::new(ComputeBudget::new(parallelism));
        }
        self
    }

    pub fn parallelism(&self) -> usize {
        self.compute_budget.parallelism()
    }

    pub fn compute_budget(&self) -> &Arc<ComputeBudget> {
        &self.compute_budget
    }

    pub const fn partition(&self) -> ExecutionPartition {
        self.partition
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn report_progress(&self, stage: &'static str, completed: u64, total: Option<u64>) {
        if let Some(sink) = &self.progress_sink {
            sink.report(ExecutionProgress::new(stage, completed, total));
        }
    }
}

impl Default for ExecutionControl {
    fn default() -> Self {
        Self::new(CancellationToken::new())
    }
}

impl fmt::Debug for ExecutionControl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionControl")
            .field("cancelled", &self.is_cancelled())
            .field("has_progress_sink", &self.progress_sink.is_some())
            .field("partition", &self.partition)
            .field("parallelism", &self.parallelism())
            .finish()
    }
}
