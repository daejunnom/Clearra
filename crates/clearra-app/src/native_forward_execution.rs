//! Native host driver for the forward-search worker wire protocol.
//!
//! The browser owns the same coordinator/worker split in TypeScript. Native hosts keep the
//! coordinator on the calling thread and dedicate the remaining requested workers to persistent
//! Rust threads for the duration of one search.

use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
};

use clearra_core_domain::execution_cancellation::ExecutionControl;
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
            worker_threads: total_workers - 1,
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
}

struct WorkerCompletion {
    worker_index: usize,
    result: Result<(usize, Vec<u8>), ForwardParallelError>,
}

pub(crate) fn run_native_forward_search(
    query: ForwardSearchQuery,
    requested_workers: usize,
    control: &ExecutionControl,
) -> Result<ForwardSearchReport, NativeForwardExecutionError> {
    if control.is_cancelled() {
        return Err(NativeForwardExecutionError::Search(
            ForwardSearchError::Cancelled,
        ));
    }
    match NativeForwardExecutionPlan::for_query(&query, requested_workers) {
        NativeForwardExecutionPlan::Serial => ForwardSearchSession::new(query)?
            .run_to_completion(control)
            .map_err(Into::into),
        NativeForwardExecutionPlan::Parallel {
            total_workers,
            worker_threads,
        } => run_parallel_forward_search(query, total_workers, worker_threads, control),
    }
}

fn run_parallel_forward_search(
    query: ForwardSearchQuery,
    total_workers: usize,
    worker_threads: usize,
    control: &ExecutionControl,
) -> Result<ForwardSearchReport, NativeForwardExecutionError> {
    debug_assert!(worker_threads > 0);
    debug_assert_eq!(total_workers, worker_threads + 1);

    let coordinator = ForwardParallelCoordinator::new(query, total_workers)?;
    let initialization = coordinator.worker_initialization();
    let workers = (0..worker_threads)
        .map(|_| ForwardParallelWorker::new(&initialization))
        .collect::<Result<Vec<_>, _>>()?;
    let (completion_sender, completion_receiver) = mpsc::channel::<WorkerCompletion>();
    let mut request_senders = Vec::with_capacity(worker_threads);
    let mut handles = Vec::with_capacity(worker_threads);

    for (worker_index, worker) in workers.into_iter().enumerate() {
        let (request_sender, request_receiver) = mpsc::sync_channel(1);
        let worker_completion_sender = completion_sender.clone();
        let worker_control = control.clone();
        let handle = match thread::Builder::new()
            .name(format!("clearra-forward-{worker_index}"))
            .spawn(move || {
                forward_worker_main(
                    worker_index,
                    worker,
                    request_receiver,
                    worker_completion_sender,
                    worker_control,
                );
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

    let result = drive_parallel_coordinator(
        coordinator,
        total_workers,
        &request_senders,
        &completion_receiver,
        control,
    );
    drop(request_senders);
    let join_result = join_workers(handles);
    match (result, join_result) {
        (Ok(report), Ok(())) => Ok(report),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn drive_parallel_coordinator(
    mut coordinator: ForwardParallelCoordinator,
    total_workers: usize,
    request_senders: &[SyncSender<WorkerRequest>],
    completion_receiver: &Receiver<WorkerCompletion>,
    control: &ExecutionControl,
) -> Result<ForwardSearchReport, NativeForwardExecutionError> {
    let mut available = (0..request_senders.len()).collect::<VecDeque<_>>();
    let mut in_flight = 0_usize;
    let mut last_progress = ForwardParallelProgress::default();

    loop {
        let mut producer_complete = false;
        while let Some(worker_index) = available.pop_front() {
            let (status, batch) = coordinator.produce(FORWARD_BATCH_CAPACITY, control)?;
            match status {
                ForwardParallelProduce::Batch => {
                    request_senders[worker_index]
                        .send(WorkerRequest::Consume(batch))
                        .map_err(|_| NativeForwardExecutionError::WorkerChannelClosed)?;
                    in_flight = in_flight.saturating_add(1);
                }
                ForwardParallelProduce::Pending => {
                    available.push_front(worker_index);
                    break;
                }
                ForwardParallelProduce::Completed => {
                    available.push_front(worker_index);
                    producer_complete = true;
                    break;
                }
                ForwardParallelProduce::Cancelled => {
                    return Err(NativeForwardExecutionError::Search(
                        ForwardSearchError::Cancelled,
                    ));
                }
            }
        }

        report_progress(control, coordinator.progress(), &mut last_progress);
        if producer_complete {
            debug_assert_eq!(in_flight, 0);
            return coordinator
                .finish_with_control(total_workers, control)
                .map_err(NativeForwardExecutionError::from);
        }
        if control.is_cancelled() {
            return Err(NativeForwardExecutionError::Search(
                ForwardSearchError::Cancelled,
            ));
        }
        if in_flight == 0 {
            return Err(NativeForwardExecutionError::CoordinatorStalled);
        }

        let completion = completion_receiver
            .recv()
            .map_err(|_| NativeForwardExecutionError::WorkerChannelClosed)?;
        in_flight = in_flight.saturating_sub(1);
        let (actual_items, partial) = completion.result?;
        // The coordinator owns task-id ordering. Absorb immediately so this worker can re-enter
        // the ready queue without making fast workers wait behind an older in-flight batch.
        let absorbed_items = coordinator.absorb(&partial, control)?;
        if absorbed_items != actual_items {
            return Err(NativeForwardExecutionError::WorkerResultCountMismatch);
        }
        available.push_back(completion.worker_index);
        report_progress(control, coordinator.progress(), &mut last_progress);
    }
}

fn forward_worker_main(
    worker_index: usize,
    mut worker: ForwardParallelWorker,
    requests: Receiver<WorkerRequest>,
    completions: mpsc::Sender<WorkerCompletion>,
    control: ExecutionControl,
) {
    while let Ok(request) = requests.recv() {
        match request {
            WorkerRequest::Consume(bytes) => {
                let result = worker.consume(&bytes, &control);
                let failed = result.is_err();
                if completions
                    .send(WorkerCompletion {
                        worker_index,
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

    use super::*;
    use crate::{
        AppCommand, AppContext, AppRenderModel, AppRequest, DamageAppCommand, SpinFinderAppCommand,
    };
    use clearra_forward_search::{
        ForwardPathStep, ForwardSearchMode, ForwardSpinCategory, ForwardSpinGroup,
        ForwardSpinTarget,
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
    fn worker_plan_uses_serial_for_one_and_caller_plus_workers_for_two_and_eight() {
        let query = damage_query();
        assert_eq!(
            NativeForwardExecutionPlan::for_query(&query, 1),
            NativeForwardExecutionPlan::Serial
        );
        assert_eq!(
            NativeForwardExecutionPlan::for_query(&query, 2),
            NativeForwardExecutionPlan::Parallel {
                total_workers: 2,
                worker_threads: 1,
            }
        );
        assert_eq!(
            NativeForwardExecutionPlan::for_query(&query, 8),
            NativeForwardExecutionPlan::Parallel {
                total_workers: 8,
                worker_threads: 7,
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
