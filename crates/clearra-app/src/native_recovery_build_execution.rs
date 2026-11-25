//! Host-owned persistent worker pool using the same exact pair wire as WASM.
//! The calling thread is the coordinator; every requested compute worker runs
//! fixed-pair searches. No worker-count reduction or serial retry is hidden.
use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    parallel_work::{
        try_for_each_chunk, with_compute_coordinator, with_compute_worker, ParallelWorkError,
    },
};
use clearra_forward_search::{
    RecoveryBuildError, RecoveryBuildParallelCoordinator as Coordinator,
    RecoveryBuildParallelError as Error, RecoveryBuildParallelProduce as Produce,
    RecoveryBuildParallelWorker as Worker, RecoveryBuildPopulation, RecoveryBuildQuery,
};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::mpsc,
    thread,
    time::Duration,
};

type WorkerCompletion = (usize, Result<Vec<u8>, Error>);

enum WorkerRequest {
    Consume(Vec<u8>),
    Produce {
        coordinator: Box<Coordinator>,
        completed: Vec<Vec<u8>>,
    },
}

enum PoolCompletion {
    Consumed(WorkerCompletion),
    Produced {
        index: usize,
        result: Result<(Box<Coordinator>, Produce, Vec<u8>), Error>,
    },
}

fn try_ready_completion<T>(receiver: &mpsc::Receiver<T>) -> Result<Option<T>, Error> {
    match receiver.try_recv() {
        Ok(completion) => Ok(Some(completion)),
        Err(mpsc::TryRecvError::Empty) => Ok(None),
        Err(mpsc::TryRecvError::Disconnected) => {
            Err(Error::InvalidState("recovery result channel closed"))
        }
    }
}

fn wait_for_completion<T>(
    receiver: &mpsc::Receiver<T>,
    timeout: Duration,
) -> Result<Option<T>, Error> {
    match receiver.recv_timeout(timeout) {
        Ok(completion) => Ok(Some(completion)),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(Error::InvalidState("recovery result channel closed"))
        }
    }
}

pub(crate) fn run_native_recovery_build(
    query: RecoveryBuildQuery,
    workers: usize,
    control: &ExecutionControl,
) -> Result<RecoveryBuildPopulation, Error> {
    let owned_control = control.clone().with_parallelism_if_unbound(workers.max(1));
    let control = &owned_control;
    let workers = control.parallelism();
    if control.is_cancelled() {
        return Err(RecoveryBuildError::Cancelled.into());
    }
    // Preparation and the final diagram/weighted-measure reduction also own
    // compute. Direct callers only wait; an existing same-budget compute owner
    // can reuse its slot. The pool below suspends that slot until every worker
    // has joined, so management never adds an (N + 1)th compute participant.
    on_recovery_compute_worker(control, || {
        if workers <= 1 {
            return query.search(control).map_err(Error::from);
        }
        run_parallel_recovery_build(query, workers, control)
    })
}

fn on_recovery_compute_worker<T: Send>(
    control: &ExecutionControl,
    operation: impl FnOnce() -> Result<T, Error> + Send,
) -> Result<T, Error> {
    let operation = std::sync::Mutex::new(Some(operation));
    let result = std::sync::Mutex::new(None);
    try_for_each_chunk(
        control,
        1,
        1,
        || Ok::<_, Error>(()),
        |_, _| {
            let operation = operation
                .lock()
                .map_err(|_| Error::InvalidState("recovery operation poisoned"))?
                .take()
                .ok_or(Error::InvalidState("recovery operation missing"))?;
            let report = operation()?;
            *result
                .lock()
                .map_err(|_| Error::InvalidState("recovery result poisoned"))? = Some(report);
            Ok(())
        },
    )
    .map_err(|error| match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => RecoveryBuildError::Cancelled.into(),
        ParallelWorkError::ThreadSpawnFailed => Error::InvalidState("recovery worker spawn failed"),
        ParallelWorkError::WorkerPanicked => Error::InvalidState("recovery worker panicked"),
    })?;
    result
        .into_inner()
        .map_err(|_| Error::InvalidState("recovery result poisoned"))?
        .ok_or(Error::InvalidState("recovery result missing"))
}

fn run_parallel_recovery_build(
    query: RecoveryBuildQuery,
    workers: usize,
    control: &ExecutionControl,
) -> Result<RecoveryBuildPopulation, Error> {
    let coordinator = Coordinator::new_with_control(query, workers, control)?;
    let init = coordinator.shared_worker_initialization();
    if control.is_cancelled() {
        return Err(RecoveryBuildError::Cancelled.into());
    }
    let coordinator = with_compute_coordinator(control, || {
        let (sender, receiver) = mpsc::channel::<PoolCompletion>();
        let mut senders = Vec::new();
        let mut handles: Vec<thread::JoinHandle<()>> = Vec::new();
        for index in 0..workers {
            let (tx, rx) = mpsc::sync_channel::<WorkerRequest>(1);
            let completion = sender.clone();
            let worker_control = control.clone();
            let init = init.clone();
            let Some(permit) = control
                .compute_budget()
                .acquire_worker(&control.cancellation)
            else {
                drop(senders);
                drop(sender);
                for handle in handles {
                    let _ = handle.join();
                }
                return Err(RecoveryBuildError::Cancelled.into());
            };
            let spawn = thread::Builder::new()
                .name(format!("clearra-recovery-{index}"))
                .spawn(move || {
                    let panic = catch_unwind(AssertUnwindSafe(|| {
                        with_compute_worker(permit, || {
                            let mut worker = match Worker::from_shared(init) {
                                Ok(worker) => worker,
                                Err(error) => {
                                    let _ = completion
                                        .send(PoolCompletion::Consumed((index, Err(error))));
                                    return;
                                }
                            };
                            while let Ok(request) = rx.recv() {
                                let (result, failed) = match request {
                                    WorkerRequest::Consume(packet) => {
                                        let result = catch_unwind(AssertUnwindSafe(|| {
                                            let mut output =
                                                worker.consume(&packet, &worker_control)?;
                                            while worker.has_pending_work() {
                                                output = worker.advance(&worker_control)?;
                                            }
                                            output.ok_or(Error::InvalidState(
                                                "recovery worker omitted result",
                                            ))
                                        }))
                                        .unwrap_or(Err(Error::InvalidState(
                                            "recovery worker panicked",
                                        )));
                                        let failed = result.is_err();
                                        (PoolCompletion::Consumed((index, result)), failed)
                                    }
                                    WorkerRequest::Produce {
                                        mut coordinator,
                                        completed,
                                    } => {
                                        let result = catch_unwind(AssertUnwindSafe(|| {
                                            for packet in completed {
                                                coordinator.absorb(&packet, &worker_control)?;
                                            }
                                            let (status, packet) =
                                                coordinator.produce(32, &worker_control)?;
                                            Ok((coordinator, status, packet))
                                        }))
                                        .unwrap_or(Err(Error::InvalidState(
                                            "recovery producer panicked",
                                        )));
                                        let failed = result.is_err();
                                        (PoolCompletion::Produced { index, result }, failed)
                                    }
                                };
                                if completion.send(result).is_err() || failed {
                                    break;
                                }
                            }
                        })
                    }));
                    if panic.is_err() {
                        let _ = completion.send(PoolCompletion::Consumed((
                            index,
                            Err(Error::InvalidState("recovery worker panicked")),
                        )));
                    }
                });
            match spawn {
                Ok(handle) => {
                    senders.push(tx);
                    handles.push(handle);
                }
                Err(_) => {
                    control.cancellation.handle().cancel();
                    drop(senders);
                    drop(sender);
                    for handle in handles {
                        let _ = handle.join();
                    }
                    return Err(Error::InvalidState("recovery worker spawn failed"));
                }
            }
        }
        drop(sender);
        let result = (|| {
            // Exactly one owner moves the coordinator between management and a
            // currently idle compute worker. Catalog preparation and result
            // reduction share the requested N slots with fixed-pair search.
            let mut coordinator = Some(Box::new(coordinator));
            let mut idle = vec![true; senders.len()];
            let mut completed = Vec::new();
            let mut waiting_for_results = false;
            let mut next_completion = None;
            loop {
                if control.is_cancelled() {
                    return Err(RecoveryBuildError::Cancelled.into());
                }
                let completion = match next_completion.take() {
                    Some(completion) => Some(completion),
                    None => try_ready_completion(&receiver)?,
                };
                if let Some(completion) = completion {
                    match completion {
                        PoolCompletion::Consumed((index, packet)) => {
                            if index >= idle.len() || idle[index] {
                                return Err(Error::InvalidState(
                                    "recovery worker completion repeated",
                                ));
                            }
                            completed.push(packet?);
                            idle[index] = true;
                        }
                        PoolCompletion::Produced { index, result } => {
                            if index >= idle.len() || idle[index] || coordinator.is_some() {
                                return Err(Error::InvalidState(
                                    "recovery producer completion repeated",
                                ));
                            }
                            let (returned, status, packet) = result?;
                            idle[index] = true;
                            waiting_for_results = returned.waiting_for_results();
                            match status {
                                Produce::Batch => {
                                    senders[index]
                                        .send(WorkerRequest::Consume(packet))
                                        .map_err(|_| {
                                            Error::InvalidState("recovery worker disconnected")
                                        })?;
                                    idle[index] = false;
                                }
                                Produce::Completed => return Ok(returned),
                                Produce::Cancelled => {
                                    return Err(RecoveryBuildError::Cancelled.into())
                                }
                                Produce::Pending => {}
                            }
                            coordinator = Some(returned);
                        }
                    }
                    continue;
                }
                if coordinator.is_some() && (!waiting_for_results || !completed.is_empty()) {
                    if let Some(index) = idle.iter().position(|free| *free) {
                        senders[index]
                            .send(WorkerRequest::Produce {
                                coordinator: coordinator.take().unwrap(),
                                completed: std::mem::take(&mut completed),
                            })
                            .map_err(|_| Error::InvalidState("recovery producer disconnected"))?;
                        idle[index] = false;
                    }
                }
                if idle.iter().all(|free| *free) {
                    return Err(Error::InvalidState("recovery coordinator stalled"));
                }
                // A bounded wait checks cancellation even if a worker is silent.
                next_completion = wait_for_completion(&receiver, Duration::from_millis(25))?;
            }
        })();
        if result.is_err() {
            control.cancellation.handle().cancel();
        }
        drop(senders);
        let mut joined = true;
        for handle in handles {
            joined &= handle.join().is_ok();
        }
        if !joined {
            return Err(Error::InvalidState("recovery worker join failed"));
        }
        result
    })?;
    coordinator.finish(control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::board::standard_pc_board::Board256Mask;
    use clearra_forward_search::{CrossStageEarlyLimit, RecoveryBuildFields};
    use clearra_rules::profile::rule_profile::RuleProfileId;
    use clearra_scoring::profile::SpinProfileId;

    fn query() -> RecoveryBuildQuery {
        let mask = |value| Board256Mask::from_words([value, 0, 0, 0]);
        RecoveryBuildQuery {
            chain_stages: Vec::new(),
            all_solutions: true,
            minimum_solutions: false,
            required_solution_keys: Vec::new(),
            minimum_source_identity: None,
            fields: RecoveryBuildFields {
                height: 8,
                initial: mask(0),
                middle: mask(0xf),
                result: mask(0xc030),
            },
            first_supply: "[IO]".into(),
            second_supply: "[IO]".into(),
            early_limit: CrossStageEarlyLimit::Auto,
            allow_piece_exchange: true,
            hold_enabled: true,
            preserve_b2b: false,
            initial_b2b: true,
            rule_profile: RuleProfileId::SrsPlus,
            spin_profile: SpinProfileId::AllSpinPlus,
        }
    }

    #[test]
    fn preparation_and_finish_use_compute_owner_and_manager_releases_all_slots() {
        let caller = thread::current().id();
        for workers in [1, 2, 8] {
            let control = ExecutionControl::default().with_parallelism(workers);
            on_recovery_compute_worker(&control, || {
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
                    assert_eq!(pool.len(), workers, "manager reserves no compute slot");
                });
                let mut spare = Vec::new();
                while let Some(permit) = control.compute_budget().try_acquire_worker() {
                    spare.push(permit);
                }
                assert_eq!(spare.len(), workers - 1, "finish resumes its one slot");
                Ok(())
            })
            .unwrap();
            let mut returned = Vec::new();
            while let Some(permit) = control.compute_budget().try_acquire_worker() {
                returned.push(permit);
            }
            assert_eq!(returned.len(), workers, "every compute owner drained");
        }
    }

    #[test]
    fn bound_recovery_owner_preserves_budget_and_initializer_panic_drains() {
        let control = ExecutionControl::default().with_parallelism(2);
        let caller = thread::current().id();
        let serial = query().search(&ExecutionControl::default()).unwrap();
        on_recovery_compute_worker(&control, || {
            let owner = thread::current().id();
            assert_ne!(owner, caller);
            assert_eq!(run_native_recovery_build(query(), 8, &control)?, serial);
            assert_eq!(thread::current().id(), owner);
            let spare = control.compute_budget().try_acquire_worker().unwrap();
            assert!(control.compute_budget().try_acquire_worker().is_none());
            drop(spare);
            Ok(())
        })
        .unwrap();
        let failed: Result<(), Error> = on_recovery_compute_worker(&control, || {
            panic!("injected recovery preparation failure")
        });
        assert!(matches!(
            failed,
            Err(Error::InvalidState("recovery worker panicked"))
        ));
        let first = control.compute_budget().try_acquire_worker().unwrap();
        let second = control.compute_budget().try_acquire_worker().unwrap();
        assert!(control.compute_budget().try_acquire_worker().is_none());
        drop((first, second));
    }

    #[test]
    fn ready_completion_precedes_preparation_and_closed_channels_fail() {
        let (sender, receiver) = mpsc::channel::<WorkerCompletion>();
        assert!(try_ready_completion(&receiver).unwrap().is_none());
        sender.send((1, Ok(vec![7]))).unwrap();
        let (index, bytes) = try_ready_completion(&receiver).unwrap().unwrap();
        assert_eq!((index, bytes.unwrap()), (1, vec![7]));
        assert!(wait_for_completion(&receiver, Duration::ZERO)
            .unwrap()
            .is_none());
        drop(sender);
        assert!(matches!(
            try_ready_completion(&receiver),
            Err(Error::InvalidState("recovery result channel closed"))
        ));
        assert!(matches!(
            wait_for_completion(&receiver, Duration::ZERO),
            Err(Error::InvalidState("recovery result channel closed"))
        ));
    }

    #[test]
    fn small_catalog_matches_serial_across_worker_counts_and_respects_cancel() {
        let control = ExecutionControl::default();
        let query = query();
        let serial = query.search(&control).unwrap();
        for workers in [1, 2, 3] {
            assert_eq!(
                run_native_recovery_build(query.clone(), workers, &control).unwrap(),
                serial
            );
        }
        let cancelled = ExecutionControl::default();
        cancelled.cancellation.handle().cancel();
        assert!(matches!(
            run_native_recovery_build(query, 3, &cancelled),
            Err(Error::Search(RecoveryBuildError::Cancelled))
        ));
    }
}
