//! Native host driver for the forward-search worker wire protocol.
//!
//! Native hosts dedicate all requested compute workers to persistent Rust
//! threads. Frontier production and ordered reduction move with their owner
//! through that same pool; the management thread only routes messages.

use std::{
    sync::{
        mpsc::{self, Receiver, SyncSender},
        Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    parallel_work::{
        try_for_each_chunk, with_compute_coordinator, with_compute_worker, ParallelWorkError,
    },
};
use clearra_forward_search::{
    ForwardParallelCoordinator, ForwardParallelError, ForwardParallelProduce,
    ForwardParallelProgress, ForwardParallelWorker, ForwardSearchError, ForwardSearchQuery,
    ForwardSearchReport, ForwardSearchSession,
};

const FORWARD_BATCH_CAPACITY: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeForwardExecutionPlan {
    Serial,
    Parallel {
        total_workers: usize,
        worker_threads: usize,
    },
}

impl NativeForwardExecutionPlan {
    fn for_query(query: &ForwardSearchQuery, requested_workers: usize) -> Self {
        let total_workers = requested_workers.max(1);
        if !ForwardParallelCoordinator::is_worthwhile(query, total_workers) {
            return Self::Serial;
        }
        Self::Parallel {
            total_workers,
            worker_threads: total_workers,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeForwardExecutionError {
    Search(ForwardSearchError),
    Parallel(ForwardParallelError),
    WorkerSpawn,
    WorkerChannelClosed,
    WorkerPanicked,
    WorkerResultCountMismatch,
    CoordinatorStalled,
}

impl NativeForwardExecutionError {
    pub(crate) const fn is_cancelled(self) -> bool {
        matches!(
            self,
            Self::Search(ForwardSearchError::Cancelled)
                | Self::Parallel(ForwardParallelError::Search(ForwardSearchError::Cancelled))
        )
    }

    pub(crate) const fn is_request_error(self) -> bool {
        self.request_error().is_some()
    }

    pub(crate) const fn request_error(self) -> Option<ForwardSearchError> {
        match self {
            Self::Search(ForwardSearchError::Cancelled)
            | Self::Parallel(ForwardParallelError::Search(ForwardSearchError::Cancelled)) => None,
            Self::Search(error) | Self::Parallel(ForwardParallelError::Search(error)) => {
                Some(error)
            }
            Self::Parallel(ForwardParallelError::InvalidWire(_))
            | Self::Parallel(ForwardParallelError::InvalidState(_))
            | Self::WorkerSpawn
            | Self::WorkerChannelClosed
            | Self::WorkerPanicked
            | Self::WorkerResultCountMismatch
            | Self::CoordinatorStalled => None,
        }
    }

    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Search(ForwardSearchError::EmptyQueue) => "forward_search_empty_queue",
            Self::Search(ForwardSearchError::QueueTooLong) => "forward_search_queue_too_long",
            Self::Search(ForwardSearchError::InvalidHeight) => "forward_search_invalid_height",
            Self::Search(ForwardSearchError::BoardOutsideField) => {
                "forward_search_board_outside_field"
            }
            Self::Search(ForwardSearchError::PatternRequiresSpinFinder) => {
                "forward_search_pattern_requires_spin_finder"
            }
            Self::Search(ForwardSearchError::RenRequiresFixedQueue) => {
                "forward_ren_requires_fixed_queue"
            }
            Self::Search(ForwardSearchError::RenQueueTooLong) => "forward_ren_queue_too_long",
            Self::Search(ForwardSearchError::RenInitialComboUnsupported) => {
                "forward_ren_initial_combo_unsupported"
            }
            Self::Search(ForwardSearchError::RenInitialBackToBackUnsupported) => {
                "forward_ren_initial_back_to_back_unsupported"
            }
            Self::Search(ForwardSearchError::RenLineClearPolicyUnsupported) => {
                "forward_ren_line_clear_policy_unsupported"
            }
            Self::Search(ForwardSearchError::RenSpinProfileMustBeDisabled) => {
                "forward_ren_spin_profile_must_be_disabled"
            }
            Self::Search(ForwardSearchError::SpinProfileDisabled) => {
                "forward_search_spin_profile_disabled"
            }
            Self::Search(ForwardSearchError::UnsupportedRuleProfile(reason)) => reason,
            Self::Search(ForwardSearchError::Cancelled) => "forward_search_cancelled",
            Self::Parallel(error) => error.reason(),
            Self::WorkerSpawn => "forward_native_worker_spawn_failed",
            Self::WorkerChannelClosed => "forward_native_worker_channel_closed",
            Self::WorkerPanicked => "forward_native_worker_panicked",
            Self::WorkerResultCountMismatch => "forward_native_worker_result_count_mismatch",
            Self::CoordinatorStalled => "forward_native_coordinator_stalled",
        }
    }
}

impl From<ForwardSearchError> for NativeForwardExecutionError {
    fn from(value: ForwardSearchError) -> Self {
        Self::Search(value)
    }
}

impl From<ForwardParallelError> for NativeForwardExecutionError {
    fn from(value: ForwardParallelError) -> Self {
        Self::Parallel(value)
    }
}

enum WorkerRequest {
    Consume(Vec<u8>),
    Produce {
        coordinator: Box<ForwardParallelCoordinator>,
        completed: Vec<(usize, Vec<u8>)>,
    },
}

enum WorkerCompletion {
    Consumed {
        worker_index: usize,
        result: Result<(usize, Vec<u8>), NativeForwardExecutionError>,
    },
    Produced {
        worker_index: usize,
        result: Result<ProducedForwardWork, NativeForwardExecutionError>,
    },
}

struct ProducedForwardWork {
    coordinator: Box<ForwardParallelCoordinator>,
    status: ForwardParallelProduce,
    batch: Vec<u8>,
    progress: ForwardParallelProgress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkerActivity {
    Idle,
    Consuming,
    Producing,
}

fn on_forward_compute_worker<T: Send>(
    control: &ExecutionControl,
    operation: impl FnOnce() -> Result<T, NativeForwardExecutionError> + Send,
) -> Result<T, NativeForwardExecutionError> {
    let operation = Mutex::new(Some(operation));
    let result = Mutex::new(None);
    try_for_each_chunk(
        control,
        1,
        1,
        || Ok::<_, NativeForwardExecutionError>(()),
        |_, _| {
            let operation = operation
                .lock()
                .map_err(|_| NativeForwardExecutionError::WorkerPanicked)?
                .take()
                .ok_or(NativeForwardExecutionError::WorkerResultCountMismatch)?;
            let completed = operation()?;
            *result
                .lock()
                .map_err(|_| NativeForwardExecutionError::WorkerPanicked)? = Some(completed);
            Ok(())
        },
    )
    .map_err(|error| match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => ForwardSearchError::Cancelled.into(),
        ParallelWorkError::ThreadSpawnFailed => NativeForwardExecutionError::WorkerSpawn,
        ParallelWorkError::WorkerPanicked => NativeForwardExecutionError::WorkerPanicked,
    })?;
    result
        .into_inner()
        .map_err(|_| NativeForwardExecutionError::WorkerPanicked)?
        .ok_or(NativeForwardExecutionError::WorkerResultCountMismatch)
}

pub(crate) fn run_native_forward_search(
    query: ForwardSearchQuery,
    requested_workers: usize,
    control: &ExecutionControl,
) -> Result<ForwardSearchReport, NativeForwardExecutionError> {
    let control = control
        .clone()
        .with_parallelism_if_unbound(requested_workers.max(1));
    if control.is_cancelled() {
        return Err(NativeForwardExecutionError::Search(
            ForwardSearchError::Cancelled,
        ));
    }
    // This owner covers initialization and terminal projection too, including
    // direct callers and the one-worker path. Pool management suspends it only
    // while the N persistent workers own the request's compute permits.
    on_forward_compute_worker(&control, || {
        match NativeForwardExecutionPlan::for_query(&query, control.parallelism()) {
            NativeForwardExecutionPlan::Serial => ForwardSearchSession::new(query)?
                .run_to_completion(&control)
                .map_err(NativeForwardExecutionError::from),
            NativeForwardExecutionPlan::Parallel {
                total_workers,
                worker_threads,
            } => run_parallel_forward_search(query, total_workers, worker_threads, &control),
        }
    })
}

fn run_parallel_forward_search(
    query: ForwardSearchQuery,
    total_workers: usize,
    worker_threads: usize,
    control: &ExecutionControl,
) -> Result<ForwardSearchReport, NativeForwardExecutionError> {
    debug_assert!(worker_threads > 0);
    debug_assert_eq!(total_workers, worker_threads);

    let coordinator = ForwardParallelCoordinator::new(query, total_workers)?;
    let initialization = std::sync::Arc::new(coordinator.worker_initialization());
    let coordinator = with_compute_coordinator(control, || {
        let (completion_sender, completion_receiver) = mpsc::channel::<WorkerCompletion>();
        let mut request_senders = Vec::with_capacity(worker_threads);
        let mut handles = Vec::with_capacity(worker_threads);

        for worker_index in 0..worker_threads {
            let (request_sender, request_receiver) = mpsc::sync_channel(1);
            let worker_completion_sender = completion_sender.clone();
            let worker_control = control.clone();
            let initialization = initialization.clone();
            let Some(permit) = control
                .compute_budget()
                .acquire_worker(&control.cancellation)
            else {
                drop(request_senders);
                drop(completion_sender);
                let _ = join_workers(handles);
                return Err(NativeForwardExecutionError::Search(
                    ForwardSearchError::Cancelled,
                ));
            };
            let handle = match thread::Builder::new()
                .name(format!("clearra-forward-{worker_index}"))
                .spawn(move || {
                    with_compute_worker(permit, || {
                        let error_sender = worker_completion_sender.clone();
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            match ForwardParallelWorker::new(&initialization) {
                                Ok(worker) => forward_worker_main(
                                    worker_index,
                                    worker,
                                    request_receiver,
                                    worker_completion_sender,
                                    worker_control,
                                ),
                                Err(error) => {
                                    let _ = error_sender.send(WorkerCompletion::Consumed {
                                        worker_index,
                                        result: Err(error.into()),
                                    });
                                }
                            }
                        }));
                        if result.is_err() {
                            let _ = error_sender.send(WorkerCompletion::Consumed {
                                worker_index,
                                result: Err(ForwardParallelError::InvalidState(
                                    "forward_worker_panicked",
                                )
                                .into()),
                            });
                        }
                    });
                }) {
                Ok(handle) => handle,
                Err(_) => {
                    drop(request_senders);
                    drop(completion_sender);
                    let _ = join_workers(handles);
                    return Err(NativeForwardExecutionError::WorkerSpawn);
                }
            };
            request_senders.push(request_sender);
            handles.push(handle);
        }
        drop(completion_sender);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            drive_parallel_coordinator(coordinator, &request_senders, &completion_receiver, control)
        }))
        .unwrap_or(Err(NativeForwardExecutionError::WorkerPanicked));
        if result.is_err() {
            control.cancellation.handle().cancel();
        }
        drop(request_senders);
        let join_result = join_workers(handles);
        match (result, join_result) {
            (Ok(coordinator), Ok(())) => Ok(coordinator),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    })?;
    coordinator
        .finish_with_control(total_workers, control)
        .map_err(NativeForwardExecutionError::from)
}

fn drive_parallel_coordinator(
    coordinator: ForwardParallelCoordinator,
    request_senders: &[SyncSender<WorkerRequest>],
    completion_receiver: &Receiver<WorkerCompletion>,
    control: &ExecutionControl,
) -> Result<Box<ForwardParallelCoordinator>, NativeForwardExecutionError> {
    let mut coordinator = Some(Box::new(coordinator));
    let mut activity = vec![WorkerActivity::Idle; request_senders.len()];
    let mut completed = Vec::with_capacity(request_senders.len());
    let mut waiting_for_results = false;
    let mut next_completion = None;
    let mut last_progress = ForwardParallelProgress::default();

    loop {
        if control.is_cancelled() {
            return Err(NativeForwardExecutionError::Search(
                ForwardSearchError::Cancelled,
            ));
        }
        // Drain ready completions before scheduling another production step.
        // One owner carries the frontier through the pool, and each worker has
        // at most one outstanding request. Pending result packets stay <= N.
        let completion = match next_completion.take() {
            Some(completion) => Some(completion),
            None => match completion_receiver.try_recv() {
                Ok(completion) => Some(completion),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) if control.is_cancelled() => {
                    return Err(ForwardSearchError::Cancelled.into());
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(NativeForwardExecutionError::WorkerChannelClosed);
                }
            },
        };
        if let Some(completion) = completion {
            match completion {
                WorkerCompletion::Consumed {
                    worker_index,
                    result,
                } => {
                    let packet = result?;
                    if activity.get(worker_index) != Some(&WorkerActivity::Consuming)
                        || completed.len() >= request_senders.len()
                    {
                        return Err(NativeForwardExecutionError::WorkerResultCountMismatch);
                    }
                    completed.push(packet);
                    activity[worker_index] = WorkerActivity::Idle;
                }
                WorkerCompletion::Produced {
                    worker_index,
                    result,
                } => {
                    let produced = result?;
                    if activity.get(worker_index) != Some(&WorkerActivity::Producing)
                        || coordinator.is_some()
                    {
                        return Err(NativeForwardExecutionError::WorkerResultCountMismatch);
                    }
                    activity[worker_index] = WorkerActivity::Idle;
                    report_progress(control, produced.progress, &mut last_progress);
                    waiting_for_results =
                        matches!(produced.status, ForwardParallelProduce::Pending);
                    match produced.status {
                        ForwardParallelProduce::Batch => {
                            request_senders[worker_index]
                                .send(WorkerRequest::Consume(produced.batch))
                                .map_err(|_| NativeForwardExecutionError::WorkerChannelClosed)?;
                            activity[worker_index] = WorkerActivity::Consuming;
                        }
                        ForwardParallelProduce::Completed => {
                            if !completed.is_empty()
                                || activity.iter().any(|state| *state != WorkerActivity::Idle)
                            {
                                return Err(NativeForwardExecutionError::WorkerResultCountMismatch);
                            }
                            return Ok(produced.coordinator);
                        }
                        ForwardParallelProduce::Cancelled => {
                            return Err(ForwardSearchError::Cancelled.into());
                        }
                        ForwardParallelProduce::Pending => {}
                    }
                    coordinator = Some(produced.coordinator);
                }
            }
            continue;
        }

        if coordinator.is_some() && (!waiting_for_results || !completed.is_empty()) {
            if let Some(worker_index) = activity
                .iter()
                .position(|state| *state == WorkerActivity::Idle)
            {
                request_senders[worker_index]
                    .send(WorkerRequest::Produce {
                        coordinator: coordinator.take().expect("checked coordinator owner"),
                        completed: std::mem::take(&mut completed),
                    })
                    .map_err(|_| NativeForwardExecutionError::WorkerChannelClosed)?;
                activity[worker_index] = WorkerActivity::Producing;
            }
        }
        if activity.iter().all(|state| *state == WorkerActivity::Idle) {
            return Err(NativeForwardExecutionError::CoordinatorStalled);
        }
        // Keep cancellation observable even if a worker has not returned its
        // packet. Pool shutdown still drops requests and joins every owner.
        next_completion = match completion_receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(completion) => Some(completion),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) if control.is_cancelled() => {
                return Err(ForwardSearchError::Cancelled.into());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(NativeForwardExecutionError::WorkerChannelClosed);
            }
        };
    }
}

fn produce_forward_work(
    mut coordinator: Box<ForwardParallelCoordinator>,
    completed: Vec<(usize, Vec<u8>)>,
    control: &ExecutionControl,
) -> Result<ProducedForwardWork, NativeForwardExecutionError> {
    for (actual_items, partial) in completed {
        let absorbed_items = coordinator.absorb(&partial, control)?;
        if absorbed_items != actual_items {
            return Err(NativeForwardExecutionError::WorkerResultCountMismatch);
        }
    }
    // Both ordered frontier reduction and layered pattern report construction
    // happen on the worker that owns this production request's compute permit.
    let (status, batch) = coordinator.produce(FORWARD_BATCH_CAPACITY, control)?;
    let progress = coordinator.progress();
    Ok(ProducedForwardWork {
        coordinator,
        status,
        batch,
        progress,
    })
}

fn forward_worker_main(
    worker_index: usize,
    mut worker: ForwardParallelWorker,
    requests: Receiver<WorkerRequest>,
    completions: mpsc::Sender<WorkerCompletion>,
    control: ExecutionControl,
) {
    while let Ok(request) = requests.recv() {
        let (completion, failed) = match request {
            WorkerRequest::Consume(bytes) => {
                let result = worker
                    .consume(&bytes, &control)
                    .map_err(NativeForwardExecutionError::from);
                let failed = result.is_err();
                (
                    WorkerCompletion::Consumed {
                        worker_index,
                        result,
                    },
                    failed,
                )
            }
            WorkerRequest::Produce {
                coordinator,
                completed,
            } => {
                let result = produce_forward_work(coordinator, completed, &control);
                let failed = result.is_err();
                (
                    WorkerCompletion::Produced {
                        worker_index,
                        result,
                    },
                    failed,
                )
            }
        };
        if completions.send(completion).is_err() || failed {
            return;
        }
    }
}

fn join_workers(handles: Vec<JoinHandle<()>>) -> Result<(), NativeForwardExecutionError> {
    let mut panicked = false;
    for handle in handles {
        panicked |= handle.join().is_err();
    }
    if panicked {
        Err(NativeForwardExecutionError::WorkerPanicked)
    } else {
        Ok(())
    }
}

fn report_progress(
    control: &ExecutionControl,
    progress: ForwardParallelProgress,
    previous: &mut ForwardParallelProgress,
) {
    if progress.visited_states != previous.visited_states {
        control.report_progress("forward-search", progress.visited_states, None);
    }
    if progress.patterns_completed != previous.patterns_completed {
        control.report_progress(
            "forward-search-patterns",
            progress.patterns_completed as u64,
            Some(progress.pattern_count as u64),
        );
    }
    *previous = progress;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use clearra_core_domain::{
        board::standard_pc_board::Board256Mask,
        execution_cancellation::{ExecutionCancellationToken, ExecutionProgress, ProgressSink},
        piece::piece_kind::PieceKind,
    };
    use clearra_host_contract::ResourceBudget;
    use clearra_rules::profile::rule_profile::RuleProfileId;
    use clearra_scoring::profile::SpinProfileId;
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

    use super::*;
    use crate::{
        AppCommand, AppContext, AppRenderModel, AppRequest, DamageAppCommand, SpinFinderAppCommand,
    };
    use clearra_forward_search::{
        ForwardPathStep, ForwardPieceSource, ForwardSearchMode, ForwardSpinCategory,
        ForwardSpinGroup, ForwardSpinTarget,
    };

    fn damage_query() -> ForwardSearchQuery {
        let row_without_left_cell = ((1_u64 << 10) - 1) & !1_u64;
        let board = row_without_left_cell | (row_without_left_cell << 10);
        ForwardSearchQuery::new(
            Board256Mask::from_words([board, 0, 0, 0]),
            4,
            vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::J],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::AllMiniPlus,
            None,
            None,
            ForwardSearchMode::DamageAtLeast(1),
        )
    }

    fn spin_query() -> ForwardSearchQuery {
        ForwardSearchQuery::new(
            Board256Mask::EMPTY,
            4,
            vec![PieceKind::T, PieceKind::I, PieceKind::O, PieceKind::S],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpinsPlus,
            None,
            None,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::new(None, ForwardSpinCategory::T)),
        )
    }

    fn control() -> ExecutionControl {
        ExecutionControl::new(ExecutionCancellationToken::new())
    }

    #[test]
    fn preparation_and_finish_owner_preserves_all_slots_without_using_caller() {
        let caller = thread::current().id();
        for workers in [1, 2, 8] {
            let control = control().with_parallelism(workers);
            on_forward_compute_worker(&control, || {
                assert_ne!(thread::current().id(), caller);
                let mut spare = Vec::new();
                while let Some(permit) = control.compute_budget().try_acquire_worker() {
                    spare.push(permit);
                }
                assert_eq!(spare.len(), workers - 1, "preparation owns one slot");
                drop(spare);

                with_compute_coordinator(&control, || {
                    let mut pool = Vec::new();
                    while let Some(permit) = control.compute_budget().try_acquire_worker() {
                        pool.push(permit);
                    }
                    assert_eq!(pool.len(), workers, "management reserves no compute slot");
                });

                let mut spare = Vec::new();
                while let Some(permit) = control.compute_budget().try_acquire_worker() {
                    spare.push(permit);
                }
                assert_eq!(spare.len(), workers - 1, "finish reacquires its slot");
                Ok(())
            })
            .unwrap();
            let mut returned = Vec::new();
            while let Some(permit) = control.compute_budget().try_acquire_worker() {
                returned.push(permit);
            }
            assert_eq!(returned.len(), workers, "all owners drained");
        }
    }

    #[test]
    fn bound_compute_owner_reuses_budget_and_drains_nested_pool() {
        let control = control().with_parallelism(2);
        let caller = thread::current().id();
        on_forward_compute_worker(&control, || {
            let owner = thread::current().id();
            assert_ne!(owner, caller);
            let report = run_native_forward_search(damage_query(), 8, &control)?;
            assert_eq!(
                report.workers_used(),
                2,
                "bound parent owns worker authority"
            );
            assert_eq!(thread::current().id(), owner);
            let spare = control.compute_budget().try_acquire_worker().unwrap();
            assert!(control.compute_budget().try_acquire_worker().is_none());
            drop(spare);
            Ok(())
        })
        .unwrap();
        let first = control.compute_budget().try_acquire_worker().unwrap();
        let second = control.compute_budget().try_acquire_worker().unwrap();
        assert!(control.compute_budget().try_acquire_worker().is_none());
        drop((first, second));
    }

    #[test]
    fn manager_delegates_unadvanced_frontier_and_preserves_producer_failure() {
        let coordinator = ForwardParallelCoordinator::new(damage_query(), 2).unwrap();
        let (requests, receiver) = mpsc::sync_channel(1);
        let (completions, completed) = mpsc::channel();
        thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let WorkerRequest::Produce {
                    coordinator,
                    completed,
                } = receiver.recv().unwrap()
                else {
                    panic!("manager must delegate production before any search batch");
                };
                assert_eq!(coordinator.progress().tasks_dispatched, 0);
                assert!(completed.is_empty());
                assert!(completions
                    .send(WorkerCompletion::Produced {
                        worker_index: 0,
                        result: Err(NativeForwardExecutionError::WorkerResultCountMismatch),
                    })
                    .is_ok());
            });
            assert_eq!(
                drive_parallel_coordinator(coordinator, &[requests], &completed, &control()).err(),
                Some(NativeForwardExecutionError::WorkerResultCountMismatch)
            );
            worker.join().unwrap();
        });
    }

    #[test]
    fn manager_observes_cancellation_while_production_is_in_flight() {
        let coordinator = ForwardParallelCoordinator::new(damage_query(), 2).unwrap();
        let control = control();
        let cancellation = control.cancellation.handle();
        let (requests, receiver) = mpsc::sync_channel(1);
        let (completions, completed) = mpsc::channel();
        let (release, released) = mpsc::sync_channel(1);
        thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let request = receiver.recv().unwrap();
                assert!(matches!(request, WorkerRequest::Produce { .. }));
                cancellation.cancel();
                // Keep the owner and completion channel live until management
                // returns; cancellation must not depend on a result arriving.
                released.recv_timeout(Duration::from_secs(5)).unwrap();
                drop(request);
                drop(completions);
            });
            let error = drive_parallel_coordinator(coordinator, &[requests], &completed, &control)
                .err()
                .expect("in-flight cancellation");
            assert!(error.is_cancelled());
            release.send(()).unwrap();
            worker.join().unwrap();
        });
    }

    #[test]
    fn manager_fails_if_production_owner_loses_completion_channel() {
        let coordinator = ForwardParallelCoordinator::new(damage_query(), 2).unwrap();
        let (requests, receiver) = mpsc::sync_channel(1);
        let (completions, completed) = mpsc::channel();
        thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let request = receiver.recv().unwrap();
                assert!(matches!(request, WorkerRequest::Produce { .. }));
                drop(request);
                drop(completions);
            });
            assert_eq!(
                drive_parallel_coordinator(coordinator, &[requests], &completed, &control()).err(),
                Some(NativeForwardExecutionError::WorkerChannelClosed)
            );
            worker.join().unwrap();
        });
    }

    #[test]
    fn production_rejects_consumed_item_count_mismatch() {
        let control = control();
        let coordinator = ForwardParallelCoordinator::new(damage_query(), 2).unwrap();
        let mut worker = ForwardParallelWorker::new(&coordinator.worker_initialization()).unwrap();
        let produced = produce_forward_work(Box::new(coordinator), Vec::new(), &control).unwrap();
        assert_eq!(produced.status, ForwardParallelProduce::Batch);
        let (items, partial) = worker.consume(&produced.batch, &control).unwrap();
        assert_eq!(
            produce_forward_work(produced.coordinator, vec![(items + 1, partial)], &control).err(),
            Some(NativeForwardExecutionError::WorkerResultCountMismatch)
        );
    }

    fn assert_exact_search_semantics(serial: &ForwardSearchReport, parallel: &ForwardSearchReport) {
        assert_eq!(serial.complete(), parallel.complete());
        assert_eq!(serial.initial_board(), parallel.initial_board());
        assert_eq!(serial.visited_states(), parallel.visited_states());
        assert_eq!(serial.generated_locks(), parallel.generated_locks());
        assert_eq!(serial.peak_frontier(), parallel.peak_frontier());
        assert_eq!(serial.maximum_damage(), parallel.maximum_damage());
        assert_eq!(canonical_outcomes(serial), canonical_outcomes(parallel));
    }

    #[allow(clippy::type_complexity)]
    fn canonical_outcomes(
        report: &ForwardSearchReport,
    ) -> Vec<(
        u32,
        Vec<PieceKind>,
        Option<ForwardSpinGroup>,
        [u64; 4],
        Option<PieceKind>,
        bool,
        u8,
        u32,
        String,
        bool,
        Vec<ForwardPathStep>,
    )> {
        let mut outcomes = report
            .outcomes()
            .iter()
            .map(|outcome| {
                (
                    outcome.source_pattern_index(),
                    outcome.source_queue().to_vec(),
                    outcome.group(),
                    outcome.final_board(),
                    outcome.spin_piece(),
                    outcome.spin_mini(),
                    outcome.spin_lines(),
                    outcome.total_damage(),
                    outcome.evidence_path_count().to_owned(),
                    outcome.evidence_complete(),
                    outcome.path().to_vec(),
                )
            })
            .collect::<Vec<_>>();
        outcomes.sort_unstable();
        outcomes
    }

    #[test]
    fn worker_plan_keeps_coordinator_outside_all_requested_compute_workers() {
        let query = damage_query();
        assert_eq!(
            NativeForwardExecutionPlan::for_query(&query, 1),
            NativeForwardExecutionPlan::Serial
        );
        assert_eq!(
            NativeForwardExecutionPlan::for_query(&query, 2),
            NativeForwardExecutionPlan::Parallel {
                total_workers: 2,
                worker_threads: 2,
            }
        );
        assert_eq!(
            NativeForwardExecutionPlan::for_query(&query, 8),
            NativeForwardExecutionPlan::Parallel {
                total_workers: 8,
                worker_threads: 8,
            }
        );

        let short_query = ForwardSearchQuery::new(
            Board256Mask::EMPTY,
            4,
            vec![PieceKind::I, PieceKind::O, PieceKind::T],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        );
        assert_eq!(
            NativeForwardExecutionPlan::for_query(&short_query, 8),
            NativeForwardExecutionPlan::Serial
        );
    }

    #[test]
    fn two_worker_damage_is_exactly_equivalent_to_serial() {
        let query = damage_query();
        let serial = run_native_forward_search(query.clone(), 1, &control()).expect("serial");
        let parallel = run_native_forward_search(query, 2, &control()).expect("two workers");

        assert_eq!(serial.workers_used(), 1);
        assert_eq!(parallel.workers_used(), 2);
        assert_exact_search_semantics(&serial, &parallel);
    }

    #[test]
    fn eight_worker_spin_is_exactly_equivalent_and_deterministic() {
        let query = spin_query();
        let serial = run_native_forward_search(query.clone(), 1, &control()).expect("serial");
        let parallel_a =
            run_native_forward_search(query.clone(), 8, &control()).expect("eight workers");
        let parallel_b = run_native_forward_search(query, 8, &control()).expect("repeat");

        assert_eq!(parallel_a.workers_used(), 8);
        assert_eq!(parallel_a, parallel_b);
        assert_exact_search_semantics(&serial, &parallel_a);
    }

    #[test]
    fn layered_pattern_reduction_and_report_match_serial() {
        let query = ForwardSearchQuery::new_with_source(
            Board256Mask::EMPTY,
            1,
            ForwardPieceSource::pattern(QueuePatternExpression::parse("IIIIII", 8).unwrap()),
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::AllMiniPlus,
            None,
            None,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::new(None, ForwardSpinCategory::Other)),
        );
        let serial = run_native_forward_search(query.clone(), 1, &control()).unwrap();
        let parallel = run_native_forward_search(query, 8, &control()).unwrap();
        assert!(serial.visited_states() > 0);
        assert!(serial.generated_locks() > 0);
        assert_eq!(parallel.workers_used(), 8);
        assert_exact_search_semantics(&serial, &parallel);
    }

    #[test]
    fn app_context_routes_forward_resource_budget_to_native_workers() {
        let response = AppContext::default().run(
            AppRequest::new(AppCommand::Damage(DamageAppCommand::new(damage_query())))
                .with_resource_budget(ResourceBudget::new(2, None, None)),
        );

        let Some(AppRenderModel::Damage(report)) = response.render_model() else {
            panic!("damage render model");
        };
        assert_eq!(report.workers_used(), 2);
    }

    #[derive(Default)]
    struct RecordingProgressSink(Mutex<Vec<ExecutionProgress>>);

    impl ProgressSink for RecordingProgressSink {
        fn report(&self, progress: ExecutionProgress) {
            self.0.lock().expect("progress lock").push(progress);
        }
    }

    struct CancellingProgressSink {
        cancellation: ExecutionCancellationToken,
    }

    impl ProgressSink for CancellingProgressSink {
        fn report(&self, progress: ExecutionProgress) {
            if progress.stage == "forward-search" {
                self.cancellation.handle().cancel();
            }
        }
    }

    #[test]
    fn cancellation_during_parallel_progress_drains_every_compute_permit() {
        let cancellation = ExecutionCancellationToken::new();
        let control = ExecutionControl::new(cancellation.clone())
            .with_parallelism(2)
            .with_progress_sink(Arc::new(CancellingProgressSink { cancellation }));
        let error = run_native_forward_search(damage_query(), 2, &control)
            .expect_err("cancelled after real parallel progress");
        assert!(error.is_cancelled());
        let mut returned = Vec::new();
        while let Some(permit) = control.compute_budget().try_acquire_worker() {
            returned.push(permit);
        }
        assert_eq!(
            returned.len(),
            2,
            "all worker and suspended owner permits returned"
        );
    }

    struct PanickingProgressSink;

    impl ProgressSink for PanickingProgressSink {
        fn report(&self, progress: ExecutionProgress) {
            if progress.stage == "forward-search" {
                panic!("injected management progress failure");
            }
        }
    }

    #[test]
    fn management_callback_panic_joins_pool_before_returning() {
        let control = control()
            .with_parallelism(2)
            .with_progress_sink(Arc::new(PanickingProgressSink));
        assert_eq!(
            run_native_forward_search(damage_query(), 2, &control).err(),
            Some(NativeForwardExecutionError::WorkerPanicked)
        );
        let mut returned = Vec::new();
        while let Some(permit) = control.compute_budget().try_acquire_worker() {
            returned.push(permit);
        }
        assert_eq!(
            returned.len(),
            2,
            "panic drained both workers before return"
        );
    }

    #[test]
    fn native_parallel_preserves_progress_and_cancellation_contracts() {
        let sink = Arc::new(RecordingProgressSink::default());
        let progress_control = control().with_progress_sink(sink.clone());
        run_native_forward_search(damage_query(), 2, &progress_control).expect("parallel search");
        assert!(sink
            .0
            .lock()
            .expect("progress lock")
            .iter()
            .any(|progress| progress.stage == "forward-search"));

        let cancellation = ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        let error =
            run_native_forward_search(spin_query(), 8, &ExecutionControl::new(cancellation))
                .expect_err("pre-cancelled search");
        assert!(error.is_cancelled());
    }

    #[test]
    fn spin_app_context_also_uses_the_native_forward_driver() {
        let response = AppContext::default().run(
            AppRequest::new(AppCommand::SpinFinder(SpinFinderAppCommand::new(
                spin_query(),
            )))
            .with_resource_budget(ResourceBudget::new(2, None, None)),
        );

        let Some(AppRenderModel::SpinFinder(report)) = response.render_model() else {
            panic!("spin render model");
        };
        assert_eq!(report.workers_used(), 2);
    }
}
