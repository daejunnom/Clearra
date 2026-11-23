//! Piece-local movement work over one immutable rank prefix.
//!
//! Workers start immediately and publish through a rendezvous channel. Each
//! worker still owns its private arena while blocked on publication; the
//! channel is backpressure, not a process memory bound. Common prefix tables
//! are moved into an Arc once, while cross-piece private interning is not
//! shared. Explicit Boolean representation bounds use the sequential path.

use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;

use clearra_core_domain::{
    execution_cancellation::{
        CancellationToken, ExecutionControl, ExecutionProgress, ProgressSink,
    },
    piece::piece_kind::PieceKind,
};

use super::{
    reachability::{
        symbolic_family::{SymbolicBoard, SymbolicReachabilityWork},
        SharedReachabilityTemplates,
    },
    symbolic_boolean::{BooleanArena, BooleanId, FrozenBooleanPrefix},
    symbolic_family::SymbolicFamilyError,
    symbolic_movement_progress::{self, MovementMetric, MovementPhase},
};

/// All roots belong to this predicate arena and are valid only during consume.
pub(super) struct PieceMovement {
    pub piece: PieceKind,
    pub predicates: BooleanArena,
    /// Includes false roots so the caller can count every actual legal query.
    pub legal_rows: Vec<(u32, BooleanId)>,
    pub work: SymbolicReachabilityWork,
    /// Construction-only payload released before this result is published.
    pub released_builder_bytes_estimate: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ParallelMovementReport {
    pub requested_workers: usize,
    pub eligible_pieces: usize,
    pub spawned_workers: usize,
    /// Distinct workers that actually claimed a piece task.
    pub active_workers: usize,
    pub peak_concurrent_movement_tasks: usize,
    pub completed_pieces: usize,
    pub eligible: bool,
    pub fallback_reason: Option<&'static str>,
    pub movement_work: SymbolicReachabilityWork,
    pub shared_prefix_nodes: usize,
    pub shared_prefix_retained_bytes_estimate: usize,
    pub predicate_work_steps: u64,
    pub predicate_apply_cache_hits: u64,
    /// Sum of per-piece private peaks, NOT an observed simultaneous peak.
    pub private_node_peak_sum_upper_bound: usize,
    /// Sum at publication; omits transient stacks and is NOT peak RSS.
    pub private_retained_bytes_sum_estimate: usize,
    pub released_builder_bytes_sum_estimate: usize,
}

pub(super) fn parallel_movement_fallback_reason(
    predicates: &BooleanArena,
    requested_workers: usize,
    eligible_pieces: usize,
) -> Option<&'static str> {
    if !predicates.has_unlimited_representation_bounds() {
        Some("explicit-predicate-work-or-memo-bounds")
    } else if requested_workers <= 1 {
        Some("single-worker-policy")
    } else if eligible_pieces <= 1 {
        Some("insufficient-piece-tasks")
    } else {
        None
    }
}

/// `requested_workers` is already resolved by the caller's worker policy.
/// The only task-count clamp is the number of distinct eligible piece kinds.
/// The coordinator alone mutates the joint ZDD; it can read each completed
/// worker arena directly through FamilyBridge, without importing its nodes.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_piece_movements(
    board: &SymbolicBoard<'_>,
    predicates: &mut BooleanArena,
    templates: &SharedReachabilityTemplates,
    pieces: &[PieceKind],
    requested_workers: usize,
    control: &ExecutionControl,
    mut consume: impl FnMut(&PieceMovement) -> Result<(), SymbolicFamilyError>,
) -> Result<ParallelMovementReport, SymbolicFamilyError> {
    if control.is_cancelled() {
        return Err(SymbolicFamilyError::Cancelled);
    }
    if let Some(reason) =
        parallel_movement_fallback_reason(predicates, requested_workers, pieces.len())
    {
        return Err(SymbolicFamilyError::Unsupported(reason));
    }
    for (index, piece) in pieces.iter().enumerate() {
        if !PieceKind::STANDARD_TETROMINOES.contains(piece) || pieces[..index].contains(piece) {
            return Err(SymbolicFamilyError::InvalidProblem(
                "symbolic_parallel_piece_tasks",
            ));
        }
    }
    let worker_count = requested_workers.min(pieces.len());
    let prefix = predicates.freeze_prefix()?;
    let mut report = ParallelMovementReport {
        requested_workers,
        eligible_pieces: pieces.len(),
        eligible: true,
        shared_prefix_nodes: prefix.node_count(),
        shared_prefix_retained_bytes_estimate: prefix.retained_bytes(),
        ..ParallelMovementReport::default()
    };
    control.report_progress(
        "symbolic_storage_boolean_shared_nodes",
        prefix.node_count() as u64,
        None,
    );
    control.report_progress(
        "symbolic_storage_boolean_shared_bytes",
        prefix.retained_bytes() as u64,
        None,
    );
    control.report_progress("symbolic_storage_boolean_private_nodes", 0, None);
    control.report_progress("symbolic_storage_boolean_private_bytes", 0, None);
    let stop = CancellationToken::new();
    let first_error = Arc::new(Mutex::new(None));
    let next_piece = AtomicUsize::new(0);
    let running = AtomicUsize::new(0);
    let peak_running = AtomicUsize::new(0);
    let progress = control.progress_sink.as_ref().map(|sink| {
        Arc::new(SharedProgress {
            completed: Mutex::new(predicates.work_steps()),
            sink: Arc::clone(sink),
            stop: stop.clone(),
            first_error: Arc::clone(&first_error),
        })
    });
    let result = thread::scope(|scope| {
        let (sender, receiver) = mpsc::sync_channel::<PieceMovement>(0);
        let mut handles = Vec::new();
        handles
            .try_reserve_exact(worker_count)
            .map_err(allocation)?;
        for worker in 0..worker_count {
            let sender = sender.clone();
            let prefix = Arc::clone(&prefix);
            let progress = progress.clone();
            let stop = &stop;
            let first_error = &first_error;
            let next_piece = &next_piece;
            let running = &running;
            let peak_running = &peak_running;
            let handle = thread::Builder::new()
                .name(format!("symbolic-piece-{worker}"))
                .spawn_scoped(scope, move || {
                    let mut active = false;
                    loop {
                        if stop.is_cancelled() {
                            break;
                        }
                        if control.is_cancelled() {
                            fail(first_error, stop, SymbolicFamilyError::Cancelled);
                            break;
                        }
                        let index = next_piece.fetch_add(1, Ordering::Relaxed);
                        let Some(&piece) = pieces.get(index) else {
                            break;
                        };
                        active = true;
                        let task = ActiveTask::new(running, peak_running);
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            compile_piece(
                                board,
                                Arc::clone(&prefix),
                                templates,
                                piece,
                                control,
                                stop,
                                progress.as_ref(),
                            )
                        }))
                        .unwrap_or(Err(SymbolicFamilyError::Resource(
                            "symbolic_movement_worker_panic",
                        )));
                        drop(task);
                        match result {
                            Ok(movement) => {
                                if sender.send(movement).is_err() {
                                    stop.handle().cancel();
                                    break;
                                }
                            }
                            Err(error) => {
                                fail(first_error, stop, error);
                                break;
                            }
                        }
                    }
                    active
                });
            match handle {
                Ok(handle) => {
                    handles.push(handle);
                    report.spawned_workers += 1;
                }
                Err(_) => {
                    fail(
                        &first_error,
                        &stop,
                        SymbolicFamilyError::Resource("symbolic_movement_worker_spawn"),
                    );
                    break;
                }
            }
        }
        drop(sender);
        while let Ok(movement) = receiver.recv() {
            let piece = movement.piece;
            symbolic_movement_progress::report_phase(
                control,
                piece,
                MovementPhase::Consuming,
                &movement.predicates,
            );
            let recorded = record_movement(&mut report, predicates, &movement);
            if let Err(error) = recorded {
                fail(&first_error, &stop, error);
            }
            if !stop.is_cancelled() {
                if let Err(error) = consume(&movement) {
                    fail(&first_error, &stop, error);
                }
            }
            // Drop the completed suffix before accepting another. Workers may
            // still hold private suffixes while blocked on their own send.
            drop(movement);
            symbolic_movement_progress::report_released(control, piece);
        }
        for handle in handles {
            match handle.join() {
                Ok(active) => report.active_workers += usize::from(active),
                Err(_) => fail(
                    &first_error,
                    &stop,
                    SymbolicFamilyError::Resource("symbolic_movement_worker_panic"),
                ),
            }
        }
        report.peak_concurrent_movement_tasks = peak_running.load(Ordering::Relaxed);
        if let Some(error) = observed_failure(&first_error) {
            return Err(error);
        }
        if control.is_cancelled() {
            return Err(SymbolicFamilyError::Cancelled);
        }
        if stop.is_cancelled() || report.completed_pieces != pieces.len() {
            return Err(SymbolicFamilyError::Resource(
                "symbolic_movement_incomplete",
            ));
        }
        Ok(report)
    });
    // Scope joins, receives and drops every worker-owned arena before storage
    // returns to the original owner. Do not change the original failure cause.
    drop(prefix);
    let thaw = predicates.thaw_prefix().map_err(SymbolicFamilyError::from);
    if thaw.is_ok() {
        control.report_progress("symbolic_storage_boolean_shared_nodes", 0, None);
        control.report_progress("symbolic_storage_boolean_shared_bytes", 0, None);
        control.report_progress(
            "symbolic_storage_boolean_private_nodes",
            predicates.private_node_count() as u64,
            None,
        );
        control.report_progress(
            "symbolic_storage_boolean_private_bytes",
            predicates.private_retained_bytes() as u64,
            None,
        );
    }
    match result {
        Err(error) => Err(error),
        Ok(report) => {
            thaw?;
            Ok(report)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compile_piece(
    board: &SymbolicBoard<'_>,
    prefix: Arc<FrozenBooleanPrefix>,
    templates: &SharedReachabilityTemplates,
    piece: PieceKind,
    control: &ExecutionControl,
    stop: &CancellationToken,
    progress: Option<&Arc<SharedProgress>>,
) -> Result<PieceMovement, SymbolicFamilyError> {
    let mut predicates = BooleanArena::fork_prefix(prefix)?;
    predicates.set_cancellation(control.cancellation.clone());
    predicates.set_secondary_cancellation(stop.clone());
    let progress = progress.map(|shared| {
        Arc::new(PieceProgress {
            shared: Arc::clone(shared),
            reported: AtomicU64::new(0),
            piece,
        })
    });
    if let Some(progress) = &progress {
        predicates.set_progress_sink(Some(Arc::clone(progress) as Arc<dyn ProgressSink>));
    }
    let result = (|| {
        let relation = board.compile_piece(&mut predicates, templates, piece, control)?;
        let work = relation.work();
        let mut legal_rows = Vec::new();
        symbolic_movement_progress::report_phase(
            control,
            piece,
            MovementPhase::LegalRows,
            &predicates,
        );
        for row in board.row_ids(piece) {
            let legal = relation.legal_row(&mut predicates, row, control)?;
            legal_rows.try_reserve(1).map_err(allocation)?;
            legal_rows.push((row, legal));
        }
        Ok::<_, SymbolicFamilyError>((legal_rows, work))
    })();
    if let Some(progress) = &progress {
        progress.report(ExecutionProgress::new(
            "symbolic_work_boolean",
            predicates.work_steps(),
            None,
        ));
    }
    let (legal_rows, work) = result?;
    // Every root is final. The coordinator only reads nodes through the bridge;
    // retaining private interning/apply maps would prolong their ownership
    // while this worker waits to publish and while its rows are consumed.
    let released_builder_bytes_estimate = predicates.seal_for_reading()?;
    if let Some(sink) = &control.progress_sink {
        symbolic_movement_progress::report_metric(
            sink.as_ref(),
            piece,
            MovementMetric::ReleasedBuilderBytes,
            released_builder_bytes_estimate as u64,
        );
    }
    symbolic_movement_progress::report_phase(control, piece, MovementPhase::Published, &predicates);
    Ok(PieceMovement {
        piece,
        predicates,
        legal_rows,
        work,
        released_builder_bytes_estimate,
    })
}

fn record_movement(
    report: &mut ParallelMovementReport,
    owner: &mut BooleanArena,
    movement: &PieceMovement,
) -> Result<(), SymbolicFamilyError> {
    let overflow = || SymbolicFamilyError::Resource("symbolic_movement_accounting_overflow");
    let work = movement.predicates.work_steps();
    let hits = movement.predicates.apply_cache_hits();
    owner.record_fork_work(work, hits)?;
    report.predicate_work_steps = report
        .predicate_work_steps
        .checked_add(work)
        .ok_or_else(overflow)?;
    report.predicate_apply_cache_hits = report
        .predicate_apply_cache_hits
        .checked_add(hits)
        .ok_or_else(overflow)?;
    report.private_node_peak_sum_upper_bound = report
        .private_node_peak_sum_upper_bound
        .checked_add(movement.predicates.peak_node_count() - report.shared_prefix_nodes)
        .ok_or_else(overflow)?;
    report.private_retained_bytes_sum_estimate = report
        .private_retained_bytes_sum_estimate
        .checked_add(movement.predicates.private_retained_bytes())
        .ok_or_else(overflow)?;
    report.released_builder_bytes_sum_estimate = report
        .released_builder_bytes_sum_estimate
        .checked_add(movement.released_builder_bytes_estimate)
        .ok_or_else(overflow)?;
    report.movement_work.poses = report
        .movement_work
        .poses
        .checked_add(movement.work.poses)
        .ok_or_else(overflow)?;
    report.movement_work.guarded_edges = report
        .movement_work
        .guarded_edges
        .checked_add(movement.work.guarded_edges)
        .ok_or_else(overflow)?;
    report.movement_work.propagation_edges = report
        .movement_work
        .propagation_edges
        .checked_add(movement.work.propagation_edges)
        .ok_or_else(overflow)?;
    report.movement_work.family_updates = report
        .movement_work
        .family_updates
        .checked_add(movement.work.family_updates)
        .ok_or_else(overflow)?;
    for (counter, additional) in [
        (
            &mut report.movement_work.rotation_candidate_checks,
            movement.work.rotation_candidate_checks,
        ),
        (
            &mut report.movement_work.rotation_candidates_pruned,
            movement.work.rotation_candidates_pruned,
        ),
        (
            &mut report.movement_work.rotation_guard_no_prior,
            movement.work.rotation_guard_no_prior,
        ),
        (
            &mut report.movement_work.rotation_guard_single_bit,
            movement.work.rotation_guard_single_bit,
        ),
        (
            &mut report.movement_work.rotation_guard_single_mask,
            movement.work.rotation_guard_single_mask,
        ),
        (
            &mut report.movement_work.rotation_guard_ordered,
            movement.work.rotation_guard_ordered,
        ),
        (
            &mut report.movement_work.rotation_ordered_prior_terms,
            movement.work.rotation_ordered_prior_terms,
        ),
    ] {
        *counter = counter.checked_add(additional).ok_or_else(overflow)?;
    }
    report.completed_pieces += 1;
    Ok(())
}

fn fail(
    first: &Mutex<Option<SymbolicFamilyError>>,
    stop: &CancellationToken,
    error: SymbolicFamilyError,
) {
    if let Ok(mut first) = first.lock() {
        if first.is_none() {
            *first = Some(error);
        }
    }
    stop.handle().cancel();
}

fn observed_failure(first: &Mutex<Option<SymbolicFamilyError>>) -> Option<SymbolicFamilyError> {
    match first.lock() {
        Ok(first) => *first,
        Err(_) => Some(SymbolicFamilyError::Resource(
            "symbolic_movement_error_state_poisoned",
        )),
    }
}

struct ActiveTask<'a>(&'a AtomicUsize);

impl<'a> ActiveTask<'a> {
    fn new(running: &'a AtomicUsize, peak: &AtomicUsize) -> Self {
        let active = running.fetch_add(1, Ordering::Relaxed) + 1;
        peak.fetch_max(active, Ordering::Relaxed);
        Self(running)
    }
}

impl Drop for ActiveTask<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

struct SharedProgress {
    completed: Mutex<u64>,
    sink: Arc<dyn ProgressSink>,
    stop: CancellationToken,
    first_error: Arc<Mutex<Option<SymbolicFamilyError>>>,
}

struct PieceProgress {
    shared: Arc<SharedProgress>,
    reported: AtomicU64,
    piece: PieceKind,
}

impl ProgressSink for PieceProgress {
    fn report(&self, progress: ExecutionProgress) {
        let metric = match progress.stage {
            "symbolic_storage_boolean_private_nodes" => Some(MovementMetric::PrivateNodes),
            "symbolic_storage_boolean_private_bytes" => Some(MovementMetric::PrivateBytes),
            _ => None,
        };
        if let Some(metric) = metric {
            symbolic_movement_progress::report_metric(
                self.shared.sink.as_ref(),
                self.piece,
                metric,
                progress.completed,
            );
            return;
        }
        if progress.stage != "symbolic_work_boolean" {
            return;
        }
        let previous = self.reported.swap(progress.completed, Ordering::Relaxed);
        let Some(delta) = progress.completed.checked_sub(previous) else {
            return;
        };
        if delta == 0 {
            return;
        }
        // One lock per existing coarse work pulse or piece completion, never
        // per Boolean operation. Keep callbacks monotonic across workers.
        let Ok(mut completed) = self.shared.completed.lock() else {
            fail(
                &self.shared.first_error,
                &self.shared.stop,
                SymbolicFamilyError::Resource("symbolic_movement_progress_state_poisoned"),
            );
            return;
        };
        let Some(next) = completed.checked_add(delta) else {
            fail(
                &self.shared.first_error,
                &self.shared.stop,
                SymbolicFamilyError::Resource("symbolic_movement_work_overflow"),
            );
            return;
        };
        *completed = next;
        self.shared
            .sink
            .report(ExecutionProgress::new("symbolic_work_boolean", next, None));
    }
}

fn allocation(_: std::collections::TryReserveError) -> SymbolicFamilyError {
    SymbolicFamilyError::Resource("symbolic_movement_allocation")
}

#[cfg(test)]
mod tests {
    use super::super::{
        catalog::GeometryCatalog,
        reachability::symbolic_family::SymbolicOccupancy,
        symbolic_boolean::{BooleanError, BooleanLimits},
    };
    use super::*;
    use clearra_rules::kicks::KickTableProfileId;

    fn empty_domain(arena: &mut BooleanArena, rows: usize) -> BooleanId {
        let mut domain = BooleanId::TRUE;
        for row in (0..rows as u32).rev() {
            domain = arena.mk(row, domain, BooleanId::FALSE).unwrap();
        }
        domain
    }

    #[derive(Default)]
    struct WorkProgress(Mutex<Vec<u64>>);

    impl ProgressSink for WorkProgress {
        fn report(&self, event: ExecutionProgress) {
            if event.stage != "symbolic_work_boolean" {
                return;
            }
            assert_eq!(event.total, None);
            self.0.lock().unwrap().push(event.completed);
        }
    }

    #[test]
    fn symbolic_parallel_piece_movements_match_serial_without_copying_the_prefix() {
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
        let mut owner =
            BooleanArena::new(catalog.skeleton_count() as u32, BooleanLimits::default()).unwrap();
        let progress = Arc::new(WorkProgress::default());
        let control = ExecutionControl::default().with_progress_sink(progress.clone());
        let occupancy = SymbolicOccupancy::compile(&catalog, &mut owner, &control).unwrap();
        let domain = empty_domain(&mut owner, catalog.skeleton_count());
        let board = occupancy.for_domain(&mut owner, domain, &control).unwrap();
        let checkpoint = owner.checkpoint();
        let templates = SharedReachabilityTemplates::new(4, 4, KickTableProfileId::Jstris180);
        let pieces = [PieceKind::I, PieceKind::T, PieceKind::O];
        let mut expected = Vec::new();
        for piece in pieces {
            let relation = board
                .compile_piece(&mut owner, &templates, piece, &control)
                .unwrap();
            for row in board.row_ids(piece) {
                let legal = relation.legal_row(&mut owner, row, &control).unwrap();
                expected.push((row, owner.evaluate(legal, |_| false)));
            }
        }
        expected.sort_unstable();
        owner.rewind(checkpoint).unwrap();
        let before_work = owner.work_steps();
        let mut actual = Vec::new();
        let report = run_piece_movements(
            &board,
            &mut owner,
            &templates,
            &pieces,
            9,
            &control,
            |movement| {
                assert_eq!(movement.predicates.private_unique_retained_bytes(), 0);
                assert_eq!(movement.predicates.private_apply_retained_bytes(), 0);
                for &(row, legal) in &movement.legal_rows {
                    actual.push((row, movement.predicates.evaluate(legal, |_| false)));
                }
                Ok(())
            },
        )
        .unwrap();
        actual.sort_unstable();
        assert_eq!(actual, expected);
        assert_eq!(report.requested_workers, 9);
        assert_eq!(report.eligible_pieces, 3);
        assert_eq!(report.spawned_workers, 3);
        assert!((1..=3).contains(&report.active_workers));
        assert_eq!(report.completed_pieces, 3);
        assert!(report.released_builder_bytes_sum_estimate > 0);
        assert_eq!(
            owner.work_steps(),
            before_work + report.predicate_work_steps
        );
        assert_eq!(owner.node_count(), report.shared_prefix_nodes);
        let observed = progress.0.lock().unwrap();
        assert!(observed.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(observed.last(), Some(&owner.work_steps()));
        owner.rewind(checkpoint).unwrap();
    }

    #[test]
    fn symbolic_parallel_relations_reject_sibling_owners_and_drain_on_consumer_failure() {
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
        let mut owner =
            BooleanArena::new(catalog.skeleton_count() as u32, BooleanLimits::default()).unwrap();
        let control = ExecutionControl::default();
        let occupancy = SymbolicOccupancy::compile(&catalog, &mut owner, &control).unwrap();
        let domain = empty_domain(&mut owner, catalog.skeleton_count());
        let board = occupancy.for_domain(&mut owner, domain, &control).unwrap();
        let templates = SharedReachabilityTemplates::new(4, 4, KickTableProfileId::SrsPlus);
        let prefix = owner.freeze_prefix().unwrap();
        let mut left = BooleanArena::fork_prefix(Arc::clone(&prefix)).unwrap();
        let mut right = BooleanArena::fork_prefix(Arc::clone(&prefix)).unwrap();
        let relation = board
            .compile_piece(&mut left, &templates, PieceKind::T, &control)
            .unwrap();
        let row = board.row_ids(PieceKind::T).next().unwrap();
        assert_eq!(
            relation.legal_row(&mut right, row, &control),
            Err(BooleanError::InvalidInput("symbolic_relation_arena_owner"))
        );
        drop(relation);
        drop(left);
        drop(right);
        drop(prefix);
        owner.thaw_prefix().unwrap();
        let failure = SymbolicFamilyError::Resource("finite-consumer-fixture");
        let result = run_piece_movements(
            &board,
            &mut owner,
            &templates,
            &[PieceKind::I, PieceKind::O],
            2,
            &control,
            |_| Err(failure),
        );
        assert!(matches!(result, Err(error) if error == failure));
        assert!(!control.is_cancelled());
        let prefix = owner.freeze_prefix().unwrap();
        drop(prefix);
        owner.thaw_prefix().unwrap();
    }
}
