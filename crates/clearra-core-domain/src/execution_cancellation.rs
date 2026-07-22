use std::{
    fmt,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
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
}

impl ExecutionControl {
    pub fn new(cancellation: CancellationToken) -> Self {
        Self {
            cancellation,
            progress_sink: None,
            partition: ExecutionPartition::whole(),
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
            .finish()
    }
}
