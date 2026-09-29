//! Bounded FIRST-source shards with symbolic second-stage continuations.
//! Coordinator metrics count original pairs; neither worker enumerates pairs.
use super::{
    population::{PopulationAccumulator, PreparedPopulation},
    staged::{Block, BlockResult, Geometry},
    RecoveryBuildError, RecoveryBuildExample, RecoveryBuildFixedReport, RecoveryBuildPopulation,
    RecoveryBuildQuery, RecoveryBuildStatus,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use std::collections::BTreeMap;
pub(super) mod wire;
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
            Self::Search(_) => "recovery-build stage search failed",
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
struct ResultBatch {
    task: Task,
    block: BlockResult,
}

pub struct RecoveryBuildParallelCoordinator {
    pub(super) catalog: Option<super::catalog::Coordinator>,
    source: PreparedPopulation,
    initialization: Vec<u8>,
    accumulator: PopulationAccumulator,
    issued: BTreeMap<u128, Task>,
    completed: BTreeMap<u128, ResultBatch>,
    next: u128,
    merged: u128,
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
        // Fixed partitioning makes all worker counts use identical arithmetic
        // and search quotients. Parallelism does not change a probability space.
        let batch_size = if source.query.allow_piece_exchange || source.first.pattern_count() <= 64
        {
            1
        } else {
            MAX_BATCH
        };
        let accumulator = PopulationAccumulator::new(source.possible);
        let catalog = if source.query.all_solutions {
            Some(super::catalog::Coordinator::new(
                source.query.clone(),
                initialization.clone(),
                workers,
            )?)
        } else {
            None
        };
        Ok(Self {
            catalog,
            source,
            initialization,
            accumulator,
            issued: BTreeMap::new(),
            completed: BTreeMap::new(),
            next: 0,
            merged: 0,
            finished_pairs: 0,
            finished_states: 0,
            capacity: workers.saturating_mul(4).max(1),
            batch_size,
        })
    }
    pub fn has_pending_preparation(&self) -> bool {
        self.catalog.as_ref().is_some_and(|c| !c.enumeration_done())
    }
    pub fn worker_initialization(&self) -> Vec<u8> {
        self.initialization.clone()
    }
    pub fn progress(&self) -> RecoveryBuildParallelProgress {
        if let Some(catalog) = &self.catalog {
            return catalog.progress();
        }
        RecoveryBuildParallelProgress {
            possible: self.source.possible,
            issued: self.next * self.source.second.pattern_count() as u128,
            completed: self.finished_pairs,
            states: self.finished_states,
        }
    }
    pub fn produce(
        &mut self,
        maximum_rows: usize,
        control: &ExecutionControl,
    ) -> Result<(RecoveryBuildParallelProduce, Vec<u8>), RecoveryBuildParallelError> {
        use RecoveryBuildParallelProduce::*;
        if let Some(catalog) = &mut self.catalog {
            return catalog.produce(control);
        }
        if control.is_cancelled() {
            return Ok((Cancelled, Vec::new()));
        }
        if self.next == self.source.first.pattern_count() as u128 {
            return Ok((
                if self.issued.is_empty() && self.completed.is_empty() {
                    Completed
                } else {
                    Pending
                },
                Vec::new(),
            ));
        }
        // Only active work occupies dispatch slots. Completed scalar summaries
        // wait in rank order without starving idle workers behind a slow shard.
        if self.issued.len() >= self.capacity {
            return Ok((Pending, Vec::new()));
        }
        let count = self
            .batch_size
            .min(maximum_rows.max(1))
            .min((self.source.first.pattern_count() as u128 - self.next) as usize);
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
        if let Some(catalog) = &mut self.catalog {
            return catalog.absorb(bytes, control);
        }
        if control.is_cancelled() {
            return Err(RecoveryBuildError::Cancelled.into());
        }
        let mut batch = wire::decode_result(bytes, &self.initialization)?;
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
        let represented = (batch.task.count as u128) * self.source.second.pattern_count() as u128;
        let total = batch
            .block
            .counts
            .iter()
            .try_fold(0_u128, |sum, n| sum.checked_add(*n));
        if total != Some(represented) {
            return Err(RecoveryBuildParallelError::InvalidWire(
                "stage counts do not partition the issued universe",
            ));
        }
        let start =
            usize::try_from(batch.task.start).map_err(|_| RecoveryBuildError::CounterOverflow)?;
        let mass: f64 = (start..start + batch.task.count)
            .map(|i| self.source.first.weight_at(i).get())
            .sum();
        if batch
            .block
            .probabilities
            .iter()
            .any(|p| !p.is_finite() || *p < 0.0 || *p > 1.0)
            || (batch.block.probabilities.iter().sum::<f64>() - mass).abs() > 1e-10
        {
            return Err(RecoveryBuildParallelError::InvalidWire(
                "invalid stage probability measure",
            ));
        }
        let variants = super::mirror::orientations(&self.source.query.fields)?;
        for (category, slot) in [(0, &mut batch.block.normal), (1, &mut batch.block.recovery)] {
            let needed =
                expected.examples & (1 << category) != 0 && batch.block.counts[category] > 0;
            if needed != slot.is_some() {
                return Err(RecoveryBuildParallelError::InvalidWire(
                    "missing or redundant stage example",
                ));
            }
            if let Some(example) = slot {
                let status = if category == 0 {
                    RecoveryBuildStatus::Normal
                } else {
                    RecoveryBuildStatus::Recovery
                };
                if example.first_pattern < start
                    || example.first_pattern >= start + batch.task.count
                    || example.second_pattern >= self.source.second.pattern_count()
                    || example.path.status != status
                    || !variants.iter().any(|fields| {
                        example.path.middle_target == fields.middle.words()
                            && example.path.result_target == fields.result.words()
                    })
                    || example.path.steps.is_empty()
                {
                    return Err(RecoveryBuildParallelError::InvalidWire(
                        "stage example outside task",
                    ));
                }
                example.first_queue = self
                    .source
                    .first
                    .sequence_at(example.first_pattern)
                    .to_vec();
                example.second_queue = self
                    .source
                    .second
                    .sequence_at(example.second_pattern)
                    .to_vec();
                let n = example.first_queue.len() + example.second_queue.len();
                if example.path.steps.iter().any(|s| s.source_index >= n) {
                    return Err(RecoveryBuildParallelError::InvalidWire(
                        "stage example source outside supplies",
                    ));
                }
            }
        }
        self.finished_pairs = self
            .finished_pairs
            .checked_add(represented)
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        self.finished_states = self
            .finished_states
            .checked_add(batch.block.states)
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        // Retain only the lexicographically earliest witness, even while a
        // lower-ranked shard is outstanding. Do not buffer every replay path.
        for (incoming, retained) in [
            (
                &mut batch.block.normal,
                &mut self.accumulator.report.normal_example,
            ),
            (
                &mut batch.block.recovery,
                &mut self.accumulator.report.recovery_example,
            ),
        ] {
            if let Some(example) = incoming.take() {
                if retained.as_ref().is_none_or(|old| {
                    (example.first_pattern, example.second_pattern)
                        < (old.first_pattern, old.second_pattern)
                }) {
                    *retained = Some(example);
                }
            }
        }
        // Membership was validated above. Removing this active lease now is
        // essential: ordered reduction must not impose ordered scheduling.
        self.issued.remove(&batch.task.start);
        self.completed.insert(batch.task.start, batch);
        while let Some(batch) = self.completed.remove(&self.merged) {
            self.accumulator.record_block(
                &self.source,
                self.merged as usize,
                batch.task.count,
                batch.block,
            )?;
            self.merged += batch.task.count as u128;
        }
        self.source.progress(self.finished_pairs, control);
        Ok(())
    }
    pub fn finish(
        self,
        control: &ExecutionControl,
    ) -> Result<RecoveryBuildPopulation, RecoveryBuildParallelError> {
        if let Some(catalog) = self.catalog {
            return catalog.finish(control);
        }
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
    block: Block,
}
pub struct RecoveryBuildParallelWorker {
    catalog: Option<super::catalog::Worker>,
    source: PreparedPopulation,
    initialization: Vec<u8>,
    geometry: Option<Geometry>,
    pending: Option<PendingBatch>,
    progress: RecoveryBuildParallelProgress,
}
impl RecoveryBuildParallelWorker {
    pub fn is_initialization(bytes: &[u8]) -> bool {
        bytes.starts_with(wire::INIT)
    }
    pub fn new(bytes: &[u8]) -> Result<Self, RecoveryBuildParallelError> {
        let source = PreparedPopulation::new(wire::decode_initialization(bytes)?)?;
        let possible = source.possible;
        let catalog = if source.query.all_solutions {
            Some(super::catalog::Worker::new(
                source.query.clone(),
                bytes.to_vec(),
            )?)
        } else {
            None
        };
        Ok(Self {
            catalog,
            source,
            initialization: bytes.to_vec(),
            geometry: None,
            pending: None,
            progress: RecoveryBuildParallelProgress {
                possible,
                ..Default::default()
            },
        })
    }
    pub fn has_pending_work(&self) -> bool {
        if let Some(catalog) = &self.catalog {
            return catalog.has_pending_work();
        }
        self.pending.is_some()
    }
    pub fn progress(&self) -> RecoveryBuildParallelProgress {
        if let Some(catalog) = &self.catalog {
            return catalog.progress();
        }
        let mut result = self.progress;
        if let Some(pending) = &self.pending {
            result.states = result.states.saturating_add(pending.block.states());
        }
        result
    }
    pub fn consume(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, RecoveryBuildParallelError> {
        if let Some(catalog) = &mut self.catalog {
            return catalog.consume(bytes, control);
        }
        if control.is_cancelled() {
            return Err(RecoveryBuildError::Cancelled.into());
        }
        if self.pending.is_some() {
            return Err(RecoveryBuildParallelError::InvalidState(
                "recovery worker busy",
            ));
        }
        let task = wire::decode_task(bytes, &self.initialization)?;
        if task
            .start
            .checked_add(task.count as u128)
            .is_none_or(|end| end > self.source.first.pattern_count() as u128)
        {
            return Err(RecoveryBuildParallelError::InvalidWire(
                "recovery shard outside first source",
            ));
        }
        let geometry = match self.geometry.take() {
            Some(value) => value,
            None => Geometry::new(&self.source.query, control)?,
        };
        let block = Block::new(
            &self.source,
            geometry,
            task.start as usize,
            task.count,
            task.examples,
            control,
        )?;
        self.progress.issued += task.count as u128 * self.source.second.pattern_count() as u128;
        self.pending = Some(PendingBatch { task, block });
        // Return to the host even for a one-row shard, before solver expansion.
        Ok(None)
    }
    /// Bounded diagram/state-machine work per host quantum, including while a
    /// single first queue has a very large second-source continuation language.
    pub fn advance(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, RecoveryBuildParallelError> {
        if let Some(catalog) = &mut self.catalog {
            return catalog.advance(control);
        }
        if control.is_cancelled() {
            self.pending = None;
            return Err(RecoveryBuildError::Cancelled.into());
        }
        let Some(pending) = &mut self.pending else {
            return Ok(None);
        };
        if !pending.block.advance(control)? {
            return Ok(None);
        }
        let PendingBatch { task, block } =
            self.pending
                .take()
                .ok_or(RecoveryBuildParallelError::InvalidState(
                    "missing stage shard",
                ))?;
        let (block, mut geometry) = block.finish(&self.source, control)?;
        geometry.retire_shard_cache()?;
        self.geometry = Some(geometry);
        self.progress.completed += task.count as u128 * self.source.second.pattern_count() as u128;
        self.progress.states = self
            .progress
            .states
            .checked_add(block.states)
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        Ok(Some(wire::encode_result(
            &self.initialization,
            &ResultBatch { task, block },
        )))
    }
}
pub(super) fn search_serial(
    query: RecoveryBuildQuery,
    control: &ExecutionControl,
) -> Result<RecoveryBuildPopulation, RecoveryBuildParallelError> {
    let mut c = RecoveryBuildParallelCoordinator::new(query, 1)?;
    let mut w = RecoveryBuildParallelWorker::new(&c.worker_initialization())?;
    loop {
        let (status, bytes) = c.produce(MAX_BATCH, control)?;
        match status {
            RecoveryBuildParallelProduce::Completed => return c.finish(control),
            RecoveryBuildParallelProduce::Cancelled => {
                return Err(RecoveryBuildError::Cancelled.into())
            }
            RecoveryBuildParallelProduce::Pending => {
                if c.has_pending_preparation() {
                    continue;
                }
                return Err(RecoveryBuildParallelError::InvalidState(
                    "serial stage without work",
                ));
            }
            RecoveryBuildParallelProduce::Batch => {
                let mut result = w.consume(&bytes, control)?;
                while result.is_none() {
                    result = w.advance(control)?;
                }
                c.absorb(
                    &result.ok_or(RecoveryBuildParallelError::InvalidState(
                        "stage did not return",
                    ))?,
                    control,
                )?;
            }
        }
    }
}
