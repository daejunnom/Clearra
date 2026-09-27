//! Exact, bounded pair partitioning shared by native threads and WASM workers.
//! The coordinator alone owns probability aggregation and example selection.
use super::{
    population::{PopulationAccumulator, PreparedPopulation},
    RecoveryBuildError, RecoveryBuildFixedReport, RecoveryBuildPopulation, RecoveryBuildQuery,
    RecoveryBuildStatus,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use std::collections::BTreeMap;
mod wire;

const MAX_BATCH: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryBuildParallelError {
    Search(RecoveryBuildError),
    InvalidWire(&'static str),
    InvalidState(&'static str),
}
impl RecoveryBuildParallelError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Search(RecoveryBuildError::Cancelled) => "recovery-build cancelled",
            Self::Search(_) => "recovery-build pair search failed",
            Self::InvalidWire(reason) | Self::InvalidState(reason) => reason,
        }
    }
}
impl From<RecoveryBuildError> for RecoveryBuildParallelError {
    fn from(error: RecoveryBuildError) -> Self {
        Self::Search(error)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryBuildParallelProduce {
    Pending,
    Batch,
    Completed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryBuildParallelProgress {
    pub possible: u128,
    pub issued: u128,
    pub completed: u128,
    pub states: u128,
}
#[derive(Clone, Copy)]
struct Task {
    start: u128,
    count: usize,
    examples: u8,
}
struct Record {
    status: RecoveryBuildStatus,
    states: usize,
    path: Option<RecoveryBuildFixedReport>,
}
struct ResultBatch {
    task: Task,
    records: Vec<Record>,
}

pub struct RecoveryBuildParallelCoordinator {
    source: PreparedPopulation,
    initialization: Vec<u8>,
    accumulator: PopulationAccumulator,
    issued: BTreeMap<u128, Task>,
    completed: BTreeMap<u128, ResultBatch>,
    next: u128,
    finished_pairs: u128,
    finished_states: u128,
    capacity: usize,
    batch_size: usize,
}
impl RecoveryBuildParallelCoordinator {
    pub fn new(
        query: RecoveryBuildQuery,
        workers: usize,
    ) -> Result<Self, RecoveryBuildParallelError> {
        if workers == 0 {
            return Err(RecoveryBuildParallelError::InvalidState(
                "zero recovery workers",
            ));
        }
        let source = PreparedPopulation::new(query)?;
        let initialization = wire::encode_initialization(&source.query);
        let desired = (workers as u128).saturating_mul(8).max(1);
        let batch_size = source
            .possible
            .div_ceil(desired)
            .clamp(1, MAX_BATCH as u128) as usize;
        let accumulator = PopulationAccumulator::new(source.possible);
        Ok(Self {
            source,
            initialization,
            accumulator,
            issued: BTreeMap::new(),
            completed: BTreeMap::new(),
            next: 0,
            finished_pairs: 0,
            finished_states: 0,
            capacity: workers.saturating_mul(4).max(1),
            batch_size,
        })
    }
    pub fn worker_initialization(&self) -> Vec<u8> {
        self.initialization.clone()
    }
    pub fn progress(&self) -> RecoveryBuildParallelProgress {
        RecoveryBuildParallelProgress {
            possible: self.source.possible,
            issued: self.next,
            completed: self.finished_pairs,
            states: self.finished_states,
        }
    }
    pub fn produce(
        &mut self,
        maximum_pairs: usize,
        control: &ExecutionControl,
    ) -> Result<(RecoveryBuildParallelProduce, Vec<u8>), RecoveryBuildParallelError> {
        use RecoveryBuildParallelProduce::*;
        if control.is_cancelled() {
            return Ok((Cancelled, Vec::new()));
        }
        if self.next == self.source.possible {
            return Ok((
                if self.issued.is_empty() {
                    Completed
                } else {
                    Pending
                },
                Vec::new(),
            ));
        }
        // Completed out-of-order batches count against the same window. A slow
        // first batch cannot grow a product-sized reorder buffer.
        if self.issued.len() >= self.capacity {
            return Ok((Pending, Vec::new()));
        }
        let count = self
            .batch_size
            .min(maximum_pairs.max(1))
            .min((self.source.possible - self.next).min(MAX_BATCH as u128) as usize);
        let examples = u8::from(self.accumulator.report.normal_example.is_none())
            | (u8::from(self.accumulator.report.recovery_example.is_none()) << 1);
        let task = Task {
            start: self.next,
            count,
            examples,
        };
        self.next += count as u128;
        self.issued.insert(task.start, task);
        Ok((Batch, wire::encode_task(&self.initialization, task)))
    }
    pub fn absorb(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<(), RecoveryBuildParallelError> {
        if control.is_cancelled() {
            return Err(RecoveryBuildError::Cancelled.into());
        }
        let batch = wire::decode_result(bytes, &self.initialization)?;
        let expected =
            self.issued
                .get(&batch.task.start)
                .ok_or(RecoveryBuildParallelError::InvalidState(
                    "unknown or already merged recovery task",
                ))?;
        if expected.count != batch.task.count
            || expected.examples != batch.task.examples
            || self.completed.contains_key(&batch.task.start)
        {
            return Err(RecoveryBuildParallelError::InvalidState(
                "duplicate or mismatched recovery result",
            ));
        }
        let mut needed = expected.examples;
        let mut states = 0_u128;
        for record in &batch.records {
            let bit = match record.status {
                RecoveryBuildStatus::Normal => 1,
                RecoveryBuildStatus::Recovery => 2,
                RecoveryBuildStatus::NoPath => 0,
            };
            let requires_path = needed & bit != 0;
            if requires_path != record.path.is_some() {
                return Err(RecoveryBuildParallelError::InvalidWire(
                    "missing or redundant recovery example",
                ));
            }
            needed &= !bit;
            if let Some(path) = &record.path {
                if path.status != record.status || path.states != record.states {
                    return Err(RecoveryBuildParallelError::InvalidWire(
                        "recovery example status differs",
                    ));
                }
            }
            states = states
                .checked_add(record.states as u128)
                .ok_or(RecoveryBuildError::CounterOverflow)?;
        }
        self.finished_pairs += batch.task.count as u128;
        self.finished_states = self
            .finished_states
            .checked_add(states)
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        self.completed.insert(batch.task.start, batch);
        while let Some(batch) = self.completed.remove(&self.accumulator.report.evaluated) {
            for (offset, record) in batch.records.into_iter().enumerate() {
                self.accumulator.record(
                    &self.source,
                    batch.task.start + offset as u128,
                    record.status,
                    record.states,
                    record.path,
                )?;
            }
            self.issued.remove(&batch.task.start);
        }
        self.source.progress(self.finished_pairs, control);
        Ok(())
    }
    pub fn finish(
        self,
        control: &ExecutionControl,
    ) -> Result<RecoveryBuildPopulation, RecoveryBuildParallelError> {
        if control.is_cancelled() {
            return Err(RecoveryBuildError::Cancelled.into());
        }
        if !self.issued.is_empty()
            || !self.completed.is_empty()
            || self.accumulator.report.evaluated != self.source.possible
        {
            return Err(RecoveryBuildParallelError::InvalidState(
                "recovery product is incomplete",
            ));
        }
        Ok(self.accumulator.finish())
    }
}

struct PendingBatch {
    task: Task,
    records: Vec<Record>,
    wanted: u8,
}
pub struct RecoveryBuildParallelWorker {
    source: PreparedPopulation,
    initialization: Vec<u8>,
    pending: Option<PendingBatch>,
    progress: RecoveryBuildParallelProgress,
}
impl RecoveryBuildParallelWorker {
    pub fn is_initialization(bytes: &[u8]) -> bool {
        bytes.starts_with(wire::INIT)
    }
    pub fn new(bytes: &[u8]) -> Result<Self, RecoveryBuildParallelError> {
        let query = wire::decode_initialization(bytes)?;
        let source = PreparedPopulation::new(query)?;
        let possible = source.possible;
        Ok(Self {
            source,
            initialization: bytes.to_vec(),
            pending: None,
            progress: RecoveryBuildParallelProgress {
                possible,
                ..Default::default()
            },
        })
    }
    pub fn has_pending_work(&self) -> bool {
        self.pending.is_some()
    }
    pub fn progress(&self) -> RecoveryBuildParallelProgress {
        self.progress
    }
    pub fn consume(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, RecoveryBuildParallelError> {
        if self.pending.is_some() {
            return Err(RecoveryBuildParallelError::InvalidState(
                "recovery worker busy",
            ));
        }
        let task = wire::decode_task(bytes, &self.initialization)?;
        if task
            .start
            .checked_add(task.count as u128)
            .is_none_or(|end| end > self.source.possible)
        {
            return Err(RecoveryBuildParallelError::InvalidWire(
                "recovery task outside product",
            ));
        }
        self.pending = Some(PendingBatch {
            task,
            records: Vec::with_capacity(task.count),
            wanted: task.examples,
        });
        self.advance(control)
    }
    /// One exact fixed-pair search per host quantum. Between pairs the existing
    /// worker runtime can process cancellation and return its admitted memory.
    pub fn advance(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, RecoveryBuildParallelError> {
        if control.is_cancelled() {
            self.pending = None;
            return Err(RecoveryBuildError::Cancelled.into());
        }
        let Some(pending) = &mut self.pending else {
            return Ok(None);
        };
        let index = pending.task.start + pending.records.len() as u128;
        let path = self.source.evaluate(index, control)?;
        self.progress.completed += 1;
        self.progress.states = self
            .progress
            .states
            .checked_add(path.states as u128)
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        let bit = match path.status {
            RecoveryBuildStatus::Normal => 1,
            RecoveryBuildStatus::Recovery => 2,
            RecoveryBuildStatus::NoPath => 0,
        };
        let keep = pending.wanted & bit != 0;
        pending.wanted &= !bit;
        pending.records.push(Record {
            status: path.status,
            states: path.states,
            path: keep.then_some(path),
        });
        if pending.records.len() != pending.task.count {
            return Ok(None);
        }
        let pending = self.pending.take().expect("completed batch retained");
        Ok(Some(wire::encode_result(
            &self.initialization,
            &ResultBatch {
                task: pending.task,
                records: pending.records,
            },
        )))
    }
}
