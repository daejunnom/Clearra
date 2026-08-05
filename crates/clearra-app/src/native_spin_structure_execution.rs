//! Native worker-ready driver for the independent spin-structure engine.

use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::Duration,
};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_spin_structure_search::{
    SpinStructureError, SpinStructureQuery, SpinStructureReport, SpinStructureSearcher,
    SpinStructureTask,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NativeSpinStructureError {
    Search(SpinStructureError),
    Cancelled,
    WorkerSpawn,
    WorkerChannelClosed,
    WorkerPanicked,
    MissingTaskResult,
}

impl NativeSpinStructureError {
    pub(crate) const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    pub(crate) const fn reason(&self) -> &'static str {
        match self {
            Self::Search(_) => "spin_structure_invalid_request",
            Self::Cancelled => "spin_structure_cancelled",
            Self::WorkerSpawn => "spin_structure_worker_spawn_failed",
            Self::WorkerChannelClosed => "spin_structure_worker_channel_closed",
            Self::WorkerPanicked => "spin_structure_worker_panicked",
            Self::MissingTaskResult => "spin_structure_missing_task_result",
        }
    }
}

impl From<SpinStructureError> for NativeSpinStructureError {
    fn from(value: SpinStructureError) -> Self {
        Self::Search(value)
    }
}

enum WorkerRequest {
    Search {
        task_id: usize,
        task: SpinStructureTask,
    },
}

enum WorkerEvent {
    Ready {
        worker_index: usize,
    },
    Completed {
        worker_index: usize,
        task_id: usize,
        result: Result<SpinStructureReport, SpinStructureError>,
    },
}

pub(crate) fn run_native_spin_structure_search(
    query: SpinStructureQuery,
    requested_workers: usize,
    control: &ExecutionControl,
) -> Result<SpinStructureReport, NativeSpinStructureError> {
    if control.is_cancelled() {
        return Err(NativeSpinStructureError::Cancelled);
    }
    let requested_workers = requested_workers.max(1);
    if requested_workers == 1 {
        return SpinStructureSearcher::run(query).map_err(Into::into);
    }

    let tasks = SpinStructureSearcher::partition(query.clone())?;
    if tasks.is_empty() {
        return SpinStructureSearcher::run(query).map_err(Into::into);
    }
    // Partitioning is complete before this pool starts. The caller only
    // coordinates completions and therefore is not one of the requested
    // search workers. Spawn the full requested count so an explicit
    // all-logical-processors request does not silently run one search thread
    // short; workers without an initial task simply remain idle until shutdown.
    let worker_threads = requested_workers;
    run_worker_ready(tasks, worker_threads, requested_workers, control)
}

fn run_worker_ready(
    tasks: Vec<SpinStructureTask>,
    worker_threads: usize,
    requested_workers: usize,
    control: &ExecutionControl,
) -> Result<SpinStructureReport, NativeSpinStructureError> {
    let task_count = tasks.len();
    let (event_sender, event_receiver) = mpsc::channel();
    let mut request_senders = Vec::with_capacity(worker_threads);
    let mut handles = Vec::with_capacity(worker_threads);

    for worker_index in 0..worker_threads {
        let (request_sender, request_receiver) = mpsc::sync_channel(1);
        let event_sender = event_sender.clone();
        let handle = match thread::Builder::new()
            .name(format!("clearra-spin-structure-{worker_index}"))
            .spawn(move || worker_main(worker_index, request_receiver, event_sender))
        {
            Ok(handle) => handle,
            Err(_) => {
                drop(request_senders);
                let _ = join_workers(handles);
                return Err(NativeSpinStructureError::WorkerSpawn);
            }
        };
        request_senders.push(request_sender);
        handles.push(handle);
    }
    drop(event_sender);

    let mut pending = tasks.into_iter().enumerate().collect::<VecDeque<_>>();
    let mut results = std::iter::repeat_with(|| None)
        .take(task_count)
        .collect::<Vec<Option<SpinStructureReport>>>();
    let drive_result = drive_workers(
        &request_senders,
        &event_receiver,
        &mut pending,
        &mut results,
        control,
    );
    drop(request_senders);
    let join_result = join_workers(handles);
    drive_result?;
    join_result?;

    let ordered = results
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(NativeSpinStructureError::MissingTaskResult)?;
    SpinStructureSearcher::merge_task_reports(
        ordered,
        u16::try_from(requested_workers).unwrap_or(u16::MAX),
    )
    .map_err(Into::into)
}

fn drive_workers(
    request_senders: &[SyncSender<WorkerRequest>],
    event_receiver: &Receiver<WorkerEvent>,
    pending: &mut VecDeque<(usize, SpinStructureTask)>,
    results: &mut [Option<SpinStructureReport>],
    control: &ExecutionControl,
) -> Result<(), NativeSpinStructureError> {
    let total = results.len() as u64;
    let mut completed = 0_u64;
    while completed != total {
        if control.is_cancelled() {
            return Err(NativeSpinStructureError::Cancelled);
        }
        let event = match event_receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(NativeSpinStructureError::WorkerChannelClosed);
            }
        };
        match event {
            WorkerEvent::Ready { worker_index } => {
                dispatch_next(request_senders, worker_index, pending)?;
            }
            WorkerEvent::Completed {
                worker_index,
                task_id,
                result,
            } => {
                results[task_id] = Some(result?);
                completed += 1;
                control.report_progress("spin-structure", completed, Some(total));
                dispatch_next(request_senders, worker_index, pending)?;
            }
        }
    }
    Ok(())
}

fn dispatch_next(
    request_senders: &[SyncSender<WorkerRequest>],
    worker_index: usize,
    pending: &mut VecDeque<(usize, SpinStructureTask)>,
) -> Result<(), NativeSpinStructureError> {
    let Some((task_id, task)) = pending.pop_front() else {
        return Ok(());
    };
    request_senders[worker_index]
        .send(WorkerRequest::Search { task_id, task })
        .map_err(|_| NativeSpinStructureError::WorkerChannelClosed)
}

fn worker_main(
    worker_index: usize,
    requests: Receiver<WorkerRequest>,
    events: mpsc::Sender<WorkerEvent>,
) {
    if events.send(WorkerEvent::Ready { worker_index }).is_err() {
        return;
    }
    while let Ok(request) = requests.recv() {
        match request {
            WorkerRequest::Search { task_id, task } => {
                let result = SpinStructureSearcher::run_task(task);
                let failed = result.is_err();
                if events
                    .send(WorkerEvent::Completed {
                        worker_index,
                        task_id,
                        result,
                    })
                    .is_err()
                    || failed
                {
                    return;
                }
            }
        }
    }
}

fn join_workers(handles: Vec<JoinHandle<()>>) -> Result<(), NativeSpinStructureError> {
    if handles.into_iter().any(|handle| handle.join().is_err()) {
        Err(NativeSpinStructureError::WorkerPanicked)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use clearra_core_domain::{
        execution_cancellation::{ExecutionCancellationToken, ExecutionProgress, ProgressSink},
        piece::piece_kind::PieceKind,
    };
    use clearra_spin_structure_search::{
        PieceInventory, SpinLineRequirement, SpinStructureMode, StructureBoard,
    };

    use super::*;

    fn fixture_query() -> SpinStructureQuery {
        let inventory = PieceInventory::from_pieces([PieceKind::T]).expect("inventory");
        let mut query = SpinStructureQuery::new(inventory, SpinStructureMode::TSpins);
        query.height = 4;
        query.fill_top = 4;
        query.line_requirement = SpinLineRequirement::Any;
        query.max_placements = Some(1);
        query.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).expect("fixture cell")
            });
        query
    }

    #[derive(Default)]
    struct RecordingProgressSink(Mutex<Vec<ExecutionProgress>>);

    impl ProgressSink for RecordingProgressSink {
        fn report(&self, progress: ExecutionProgress) {
            self.0.lock().expect("progress lock").push(progress);
        }
    }

    #[test]
    fn native_worker_ready_merge_is_exact_and_deterministic() {
        let query = fixture_query();
        let serial =
            run_native_spin_structure_search(query.clone(), 1, &ExecutionControl::default())
                .expect("serial structure search");
        let parallel_a =
            run_native_spin_structure_search(query.clone(), 4, &ExecutionControl::default())
                .expect("parallel structure search");
        let parallel_b = run_native_spin_structure_search(query, 4, &ExecutionControl::default())
            .expect("repeated parallel structure search");

        assert_eq!(serial.regular, parallel_a.regular);
        assert_eq!(serial.mini, parallel_a.mini);
        assert_eq!(serial.minimum_placements, parallel_a.minimum_placements);
        assert_eq!(serial.layers, parallel_a.layers);
        assert_eq!(serial.stages, parallel_a.stages);
        assert_eq!(serial.complete, parallel_a.complete);
        assert_eq!(serial.query, parallel_a.query);
        assert_eq!(parallel_a.regular, parallel_b.regular);
        assert_eq!(parallel_a.mini, parallel_b.mini);
        assert_eq!(parallel_a.minimum_placements, parallel_b.minimum_placements);
        assert_eq!(parallel_a.layers, parallel_b.layers);
        assert_eq!(parallel_a.stages, parallel_b.stages);
        assert_eq!(parallel_a.complete, parallel_b.complete);
        assert_eq!(parallel_a.query, parallel_b.query);
        assert_eq!(serial.workers_used(), 1);
        assert_eq!(parallel_a.workers_used(), 4);
    }

    #[test]
    fn native_worker_ready_path_reports_progress_and_honors_pre_cancellation() {
        let sink = Arc::new(RecordingProgressSink::default());
        let control = ExecutionControl::default().with_progress_sink(sink.clone());
        run_native_spin_structure_search(fixture_query(), 4, &control)
            .expect("parallel structure search");
        assert!(sink
            .0
            .lock()
            .expect("progress lock")
            .iter()
            .any(|progress| progress.stage == "spin-structure"));

        let cancellation = ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        let error = run_native_spin_structure_search(
            fixture_query(),
            4,
            &ExecutionControl::new(cancellation),
        )
        .expect_err("pre-cancelled structure search");
        assert!(error.is_cancelled());
    }
}
