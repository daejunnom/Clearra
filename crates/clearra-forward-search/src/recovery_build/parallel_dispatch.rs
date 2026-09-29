//! One host protocol, with an unchanged paired fast path and a symbolic chain.
use super::{
    chain,
    parallel::{self, PairCoordinator, PairWorker},
    RecoveryBuildParallelError as Error, RecoveryBuildParallelProduce as Produce,
    RecoveryBuildParallelProgress as Progress, RecoveryBuildPopulation, RecoveryBuildQuery,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
enum Coordinator {
    Pair(PairCoordinator),
    Chain(chain::Coordinator),
}
pub struct RecoveryBuildParallelCoordinator {
    inner: Coordinator,
}
impl RecoveryBuildParallelCoordinator {
    pub fn new(q: RecoveryBuildQuery, workers: usize) -> Result<Self, Error> {
        if workers == 0 {
            return Err(Error::InvalidState("zero recovery workers"));
        }
        q.validate()?;
        Ok(Self {
            inner: if q.stages.is_empty() {
                Coordinator::Pair(PairCoordinator::new(q, workers)?)
            } else {
                Coordinator::Chain(chain::Coordinator::new(q, workers)?)
            },
        })
    }
    pub fn has_pending_preparation(&self) -> bool {
        match &self.inner {
            Coordinator::Pair(c) => c.has_pending_preparation(),
            Coordinator::Chain(c) => c.has_pending_preparation(),
        }
    }
    pub fn worker_initialization(&self) -> Vec<u8> {
        match &self.inner {
            Coordinator::Pair(c) => c.worker_initialization(),
            Coordinator::Chain(c) => c.worker_initialization(),
        }
    }
    pub fn progress(&self) -> Progress {
        match &self.inner {
            Coordinator::Pair(c) => c.progress(),
            Coordinator::Chain(c) => c.progress(),
        }
    }
    pub fn produce(
        &mut self,
        max: usize,
        control: &ExecutionControl,
    ) -> Result<(Produce, Vec<u8>), Error> {
        match &mut self.inner {
            Coordinator::Pair(c) => c.produce(max, control),
            Coordinator::Chain(c) => c.produce(max, control),
        }
    }
    pub fn absorb(&mut self, bytes: &[u8], control: &ExecutionControl) -> Result<(), Error> {
        match &mut self.inner {
            Coordinator::Pair(c) => c.absorb(bytes, control),
            Coordinator::Chain(c) => c.absorb(bytes, control),
        }
    }
    pub fn finish(self, control: &ExecutionControl) -> Result<RecoveryBuildPopulation, Error> {
        match self.inner {
            Coordinator::Pair(c) => c.finish(control),
            Coordinator::Chain(c) => c.finish(control),
        }
    }
}
enum Worker {
    Pair(PairWorker),
    Chain(chain::Worker),
}
pub struct RecoveryBuildParallelWorker {
    inner: Worker,
}
impl RecoveryBuildParallelWorker {
    pub fn is_initialization(bytes: &[u8]) -> bool {
        PairWorker::is_initialization(bytes)
    }
    pub fn new(bytes: &[u8]) -> Result<Self, Error> {
        let q = parallel::wire::decode_initialization(bytes)?;
        Ok(Self {
            inner: if q.stages.is_empty() {
                Worker::Pair(PairWorker::new(bytes)?)
            } else {
                Worker::Chain(chain::Worker::new(bytes)?)
            },
        })
    }
    pub fn has_pending_work(&self) -> bool {
        match &self.inner {
            Worker::Pair(w) => w.has_pending_work(),
            Worker::Chain(w) => w.has_pending_work(),
        }
    }
    pub fn progress(&self) -> Progress {
        match &self.inner {
            Worker::Pair(w) => w.progress(),
            Worker::Chain(w) => w.progress(),
        }
    }
    pub fn consume(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, Error> {
        match &mut self.inner {
            Worker::Pair(w) => w.consume(bytes, control),
            Worker::Chain(w) => w.consume(bytes, control),
        }
    }
    pub fn advance(&mut self, control: &ExecutionControl) -> Result<Option<Vec<u8>>, Error> {
        match &mut self.inner {
            Worker::Pair(w) => w.advance(control),
            Worker::Chain(w) => w.advance(control),
        }
    }
}
