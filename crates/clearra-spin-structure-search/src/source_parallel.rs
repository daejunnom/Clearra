//! Spin structure jobs use the existing host producer/worker/result lifecycle.
//! The first compute worker prepares the canonical target manifest. The
//! management coordinator never builds a geometry catalog or explores a target.
use crate::{
    SpinStructureError, SpinStructureQuery, SpinStructureReport, SpinStructureSearcher,
    SpinStructureSourceCompletion, SpinStructureSourceMergeCursor, SpinStructureSourceMerger,
    SpinStructureSourcePlan, SpinStructureSourcePreparation, SpinStructureSourceWorker,
    StructureOperation,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use std::collections::{BTreeSet, VecDeque};

#[path = "source_parallel_wire.rs"]
mod wire;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpinStructureParallelError {
    Search(SpinStructureError),
    InvalidWire(&'static str),
    InvalidState(&'static str),
}
type Error = SpinStructureParallelError;
impl From<SpinStructureError> for Error {
    fn from(error: SpinStructureError) -> Self {
        Self::Search(error)
    }
}
impl Error {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Search(SpinStructureError::Cancelled) => "spin_structure_cancelled",
            Self::Search(SpinStructureError::ParallelExecutionFailed(reason))
            | Self::InvalidWire(reason)
            | Self::InvalidState(reason) => reason,
            Self::Search(_) => "spin_structure_search_invalid",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpinStructureParallelProduce {
    Pending,
    Batch,
    Completed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpinStructureParallelProgress {
    pub tasks_dispatched: usize,
    pub tasks_completed: usize,
    pub task_count: usize,
    pub preparing: bool,
    pub finalizing: bool,
}

#[derive(Clone, Copy)]
enum Task {
    Bootstrap,
    Target {
        ordinal: usize,
        target: StructureOperation,
    },
}

pub struct SpinStructureParallelCoordinator {
    query: SpinStructureQuery,
    workers: usize,
    bootstrap_issued: bool,
    merger: Option<SpinStructureSourceMerger>,
    next: usize,
    pending: BTreeSet<usize>,
    incoming: VecDeque<wire::Decoder>,
    finalizer: Option<SpinStructureSourceMergeCursor>,
    result: Option<SpinStructureReport>,
    progress: SpinStructureParallelProgress,
}

impl SpinStructureParallelCoordinator {
    pub fn new(query: SpinStructureQuery, workers: usize) -> Result<Self, Error> {
        let query = SpinStructureSearcher::normalize_query(query)?;
        Ok(Self {
            query,
            workers: workers.max(1),
            bootstrap_issued: false,
            merger: None,
            next: 0,
            pending: BTreeSet::new(),
            incoming: VecDeque::new(),
            finalizer: None,
            result: None,
            progress: SpinStructureParallelProgress {
                preparing: true,
                ..SpinStructureParallelProgress::default()
            },
        })
    }
    pub fn worker_initialization(&self) -> Vec<u8> {
        wire::initialization(&self.query)
    }
    pub fn waiting_for_results(&self) -> bool {
        self.incoming.is_empty()
            && self.finalizer.is_none()
            && self.result.is_none()
            && ((self.bootstrap_issued && self.merger.is_none())
                || self.pending.len() >= self.workers.saturating_mul(4)
                || self.merger.as_ref().is_some_and(|merger| {
                    self.next == merger.target_count() && !self.pending.is_empty()
                }))
    }
    pub fn progress(&self) -> SpinStructureParallelProgress {
        self.progress
    }
    pub fn produce(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<(SpinStructureParallelProduce, Vec<u8>), Error> {
        if control.is_cancelled() {
            return Ok((SpinStructureParallelProduce::Cancelled, Vec::new()));
        }
        for _ in 0..budget.max(1) {
            if control.is_cancelled() {
                return Ok((SpinStructureParallelProduce::Cancelled, Vec::new()));
            }
            let Some(decoder) = self.incoming.front_mut() else {
                break;
            };
            if decoder.step(&self.query)? {
                match self.incoming.pop_front().unwrap().finish() {
                    wire::Decoded::Manifest(targets) => {
                        if !self.bootstrap_issued || self.merger.is_some() {
                            return Err(Error::InvalidState("spin_manifest_not_pending"));
                        }
                        self.progress.task_count = targets.len();
                        self.progress.preparing = false;
                        self.merger = Some(SpinStructureSourceMerger::from_manifest(
                            self.query.clone(),
                            targets,
                            self.workers,
                        ));
                    }
                    wire::Decoded::Completion(completion) => {
                        let ordinal = completion.ordinal();
                        if !self.pending.contains(&ordinal) {
                            return Err(Error::InvalidState("spin_source_task_not_pending"));
                        }
                        self.merger
                            .as_mut()
                            .ok_or(Error::InvalidState("spin_source_manifest_missing"))?
                            .absorb(completion)?;
                        self.pending.remove(&ordinal);
                        self.progress.tasks_completed += 1;
                    }
                }
            }
        }
        if self.result.is_some() {
            return Ok((SpinStructureParallelProduce::Completed, Vec::new()));
        }
        if let Some(finalizer) = self.finalizer.as_mut() {
            if finalizer.advance(budget, control)? {
                self.result = Some(self.finalizer.take().unwrap().finish()?);
                self.progress.finalizing = false;
                return Ok((SpinStructureParallelProduce::Completed, Vec::new()));
            }
            return Ok((SpinStructureParallelProduce::Pending, Vec::new()));
        }
        if !self.bootstrap_issued {
            self.bootstrap_issued = true;
            return Ok((
                SpinStructureParallelProduce::Batch,
                wire::task(&self.query, Task::Bootstrap),
            ));
        }
        let Some(merger) = self.merger.as_ref() else {
            return Ok((SpinStructureParallelProduce::Pending, Vec::new()));
        };
        if self.next < merger.target_count() && self.pending.len() < self.workers.saturating_mul(4)
        {
            let ordinal = self.next;
            let target = merger.target(ordinal).unwrap();
            self.next += 1;
            self.pending.insert(ordinal);
            self.progress.tasks_dispatched += 1;
            return Ok((
                SpinStructureParallelProduce::Batch,
                wire::task(&self.query, Task::Target { ordinal, target }),
            ));
        }
        if self.next == merger.target_count() && self.pending.is_empty() && self.incoming.is_empty()
        {
            self.finalizer = Some(self.merger.take().unwrap().begin_finish()?);
            self.progress.finalizing = true;
        }
        Ok((SpinStructureParallelProduce::Pending, Vec::new()))
    }
    pub fn absorb(&mut self, bytes: &[u8], control: &ExecutionControl) -> Result<(), Error> {
        if control.is_cancelled() {
            return Err(SpinStructureError::Cancelled.into());
        }
        if self.incoming.len() > self.workers.saturating_mul(4) {
            return Err(Error::InvalidState("spin_result_window_exceeded"));
        }
        self.incoming
            .push_back(wire::Decoder::new(bytes, &self.query)?);
        Ok(())
    }
    pub fn finish(self) -> Result<SpinStructureReport, Error> {
        self.result
            .ok_or(Error::InvalidState("spin_source_not_complete"))
    }
}

enum Encoder {
    Manifest(wire::ManifestEncoder),
    Completion(wire::CompletionEncoder),
}

pub struct SpinStructureParallelWorker {
    query: SpinStructureQuery,
    preparation: Option<SpinStructureSourcePreparation>,
    plan: Option<SpinStructureSourcePlan>,
    task: Option<Task>,
    source: Option<SpinStructureSourceWorker>,
    encoder: Option<Encoder>,
}

impl SpinStructureParallelWorker {
    pub fn is_initialization(bytes: &[u8]) -> bool {
        wire::is_init(bytes)
    }
    pub fn new(bytes: &[u8]) -> Result<Self, Error> {
        let query = wire::read_initialization(bytes)?;
        let preparation = SpinStructureSourcePreparation::new(query.clone())?;
        Ok(Self {
            query,
            preparation: Some(preparation),
            plan: None,
            task: None,
            source: None,
            encoder: None,
        })
    }
    pub fn has_pending_work(&self) -> bool {
        self.task.is_some() || self.source.is_some() || self.encoder.is_some()
    }
    pub fn consume(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, Error> {
        if self.has_pending_work() {
            return Err(Error::InvalidState("spin_worker_task_already_pending"));
        }
        self.task = Some(wire::read_task(bytes, &self.query)?);
        self.advance(256, control)
    }
    pub fn advance(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, Error> {
        if control.is_cancelled() {
            return Err(SpinStructureError::Cancelled.into());
        }
        if self.preparation.is_some() && self.task.is_some() {
            if !self
                .preparation
                .as_mut()
                .unwrap()
                .advance(budget, control)?
            {
                return Ok(None);
            }
            self.plan = Some(self.preparation.take().unwrap().finish()?);
            return Ok(None);
        }
        if let Some(encoder) = self.encoder.as_mut() {
            for _ in 0..budget.max(1) {
                if control.is_cancelled() {
                    return Err(SpinStructureError::Cancelled.into());
                }
                let done = match encoder {
                    Encoder::Manifest(encoder) => encoder.step(self.plan.as_ref().unwrap()),
                    Encoder::Completion(encoder) => encoder.step(),
                };
                if done {
                    return Ok(Some(match self.encoder.take().unwrap() {
                        Encoder::Manifest(encoder) => encoder.finish(),
                        Encoder::Completion(encoder) => encoder.finish(),
                    }));
                }
            }
            return Ok(None);
        }
        if let Some(source) = self.source.as_mut() {
            if source.advance(budget, control)? {
                self.encoder = Some(Encoder::Completion(wire::CompletionEncoder::new(
                    self.source.take().unwrap().finish()?,
                )));
            }
            return Ok(None);
        }
        if let Some(task) = self.task.take() {
            let plan = self
                .plan
                .as_ref()
                .ok_or(Error::InvalidState("spin_worker_catalog_missing"))?;
            match task {
                Task::Bootstrap => {
                    self.encoder = Some(Encoder::Manifest(wire::ManifestEncoder::new(plan)))
                }
                Task::Target { ordinal, target } => {
                    if plan.targets().get(ordinal) != Some(&target) {
                        return Err(Error::InvalidWire("spin_worker_target_source_mismatch"));
                    }
                    self.source = Some(plan.worker(ordinal, control)?);
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PieceInventory, SpinLineRequirement, SpinStructureMode, StructureBoard};
    use clearra_core_domain::piece::piece_kind::PieceKind;
    fn query() -> SpinStructureQuery {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::T, PieceKind::I, PieceKind::O]).unwrap(),
            SpinStructureMode::TSpins,
        );
        query.height = 4;
        query.fill_top = 4;
        query.max_placements = Some(3);
        query.line_requirement = SpinLineRequirement::Any;
        query.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).unwrap()
            });
        query
    }
    #[test]
    fn wire_bootstrap_and_reordered_target_workers_preserve_the_complete_source() {
        let query = query();
        let control = ExecutionControl::default();
        let mut coordinator = SpinStructureParallelCoordinator::new(query.clone(), 4).unwrap();
        assert!(coordinator.merger.is_none());
        assert!(coordinator.progress().preparing);
        let init = coordinator.worker_initialization();
        let mut workers = (0..4)
            .map(|_| SpinStructureParallelWorker::new(&init).unwrap())
            .collect::<Vec<_>>();
        let mut pending = Vec::new();
        let mut ticks = 0;
        loop {
            let (status, bytes) = coordinator.produce(31, &control).unwrap();
            if status == SpinStructureParallelProduce::Completed {
                break;
            }
            if status == SpinStructureParallelProduce::Batch {
                let index = (coordinator.progress.tasks_dispatched + 3) % workers.len();
                let worker = &mut workers[index];
                assert!(worker.consume(&bytes, &control).unwrap().is_none());
                let output = loop {
                    if let Some(output) = worker.advance(41, &control).unwrap() {
                        break output;
                    }
                };
                pending.push(output);
            }
            if pending.len() >= 3 || status == SpinStructureParallelProduce::Pending {
                while let Some(output) = pending.pop() {
                    coordinator.absorb(&output, &control).unwrap();
                }
            }
            ticks += 1;
            assert!(ticks < 1_000_000);
        }
        let actual = coordinator.finish().unwrap();
        let mut expected = SpinStructureSearcher::run(query)
            .unwrap()
            .with_workers_used(4);
        expected.timings = Default::default();
        assert_eq!(actual, expected);
    }
    #[test]
    fn worker_refuses_a_foreign_query_before_doing_catalog_work() {
        let query = query();
        let mut other = query.clone();
        other.fill_top = 3;
        let mut worker = SpinStructureParallelWorker::new(&wire::initialization(&query)).unwrap();
        assert!(worker
            .consume(
                &wire::task(&other, Task::Bootstrap),
                &ExecutionControl::default()
            )
            .is_err());
        assert!(worker.plan.is_none());
    }
}
