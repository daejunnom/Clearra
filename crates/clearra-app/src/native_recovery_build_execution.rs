//! Host-owned persistent worker pool using the same exact pair wire as WASM.
//! The calling thread is the coordinator; the remaining requested workers run
//! fixed-pair searches. No worker-count reduction or serial retry is hidden.
use clearra_core_domain::execution_cancellation::ExecutionControl;
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

fn try_ready_completion(
    receiver: &mpsc::Receiver<WorkerCompletion>,
) -> Result<Option<WorkerCompletion>, Error> {
    match receiver.try_recv() {
        Ok(completion) => Ok(Some(completion)),
        Err(mpsc::TryRecvError::Empty) => Ok(None),
        Err(mpsc::TryRecvError::Disconnected) => {
            Err(Error::InvalidState("recovery result channel closed"))
        }
    }
}

fn wait_for_completion(
    receiver: &mpsc::Receiver<WorkerCompletion>,
    timeout: Duration,
) -> Result<Option<WorkerCompletion>, Error> {
    match receiver.recv_timeout(timeout) {
        Ok(completion) => Ok(Some(completion)),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(Error::InvalidState("recovery result channel closed"))
        }
    }
}

fn can_advance_preparation(idle: &[bool], preparing: bool, waiting_for_results: bool) -> bool {
    idle.iter().any(|free| *free) && preparing && !waiting_for_results
}

fn absorb_completion(
    coordinator: &mut Coordinator,
    idle: &mut [bool],
    (index, packet): WorkerCompletion,
    control: &ExecutionControl,
) -> Result<(), Error> {
    if index >= idle.len() || idle[index] {
        return Err(Error::InvalidState("recovery worker completion repeated"));
    }
    coordinator.absorb(&packet?, control)?;
    idle[index] = true;
    Ok(())
}

pub(crate) fn run_native_recovery_build(
    query: RecoveryBuildQuery,
    workers: usize,
    control: &ExecutionControl,
) -> Result<RecoveryBuildPopulation, Error> {
    if control.is_cancelled() {
        return Err(RecoveryBuildError::Cancelled.into());
    }
    if workers <= 1 {
        return query.search(control).map_err(Into::into);
    }
    let mut coordinator = Coordinator::new_with_control(query, workers, control)?;
    let init = coordinator.worker_initialization();
    let ready = (0..workers - 1)
        .map(|_| {
            if control.is_cancelled() {
                Err(RecoveryBuildError::Cancelled.into())
            } else {
                Worker::new(&init)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if control.is_cancelled() {
        return Err(RecoveryBuildError::Cancelled.into());
    }
    let (sender, receiver) = mpsc::channel();
    let mut senders = Vec::new();
    let mut handles = Vec::new();
    for (index, mut worker) in ready.into_iter().enumerate() {
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(1);
        let completion = sender.clone();
        let worker_control = control.clone();
        let spawn = thread::Builder::new()
            .name(format!("clearra-recovery-{index}"))
            .spawn(move || {
                while let Ok(packet) = rx.recv() {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        let mut output = worker.consume(&packet, &worker_control)?;
                        while worker.has_pending_work() {
                            output = worker.advance(&worker_control)?;
                        }
                        output.ok_or(Error::InvalidState("recovery worker omitted result"))
                    }))
                    .unwrap_or(Err(Error::InvalidState("recovery worker panicked")));
                    let failed = result.is_err();
                    if completion.send((index, result)).is_err() || failed {
                        break;
                    }
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
        let mut idle = vec![true; senders.len()];
        loop {
            if control.is_cancelled() {
                return Err(RecoveryBuildError::Cancelled.into());
            }
            // Merge ready work before producing another bounded geometry
            // quantum, so completions cannot starve while preparation runs.
            if let Some(completion) = try_ready_completion(&receiver)? {
                absorb_completion(&mut coordinator, &mut idle, completion, control)?;
                continue;
            }
            let mut producer_pending = false;
            for (index, free) in idle.iter_mut().enumerate() {
                if !*free {
                    continue;
                }
                let (status, packet) = coordinator.produce(32, control)?;
                match status {
                    Produce::Batch => {
                        senders[index]
                            .send(packet)
                            .map_err(|_| Error::InvalidState("recovery worker disconnected"))?;
                        *free = false;
                    }
                    Produce::Completed => return coordinator.finish(control),
                    Produce::Cancelled => return Err(RecoveryBuildError::Cancelled.into()),
                    Produce::Pending => {
                        producer_pending = true;
                        break;
                    }
                }
            }
            if producer_pending
                && can_advance_preparation(
                    &idle,
                    coordinator.has_pending_preparation(),
                    coordinator.waiting_for_results(),
                )
            {
                // Pending can mean a geometry quantum ended, not that a
                // worker result is needed. Keep the remaining slots supplied.
                thread::yield_now();
                continue;
            }
            if idle.iter().all(|free| *free) {
                return Err(Error::InvalidState("recovery coordinator stalled"));
            }
            // A bounded wait checks cancellation even if a worker is silent.
            if let Some(completion) = wait_for_completion(&receiver, Duration::from_millis(25))? {
                absorb_completion(&mut coordinator, &mut idle, completion, control)?;
            }
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
    fn pending_preparation_only_advances_when_a_slot_and_local_work_exist() {
        assert!(can_advance_preparation(&[false, true], true, false));
        assert!(can_advance_preparation(&[true, true], true, false));
        assert!(!can_advance_preparation(&[false, false], true, false));
        assert!(!can_advance_preparation(&[false, true], false, false));
        assert!(!can_advance_preparation(&[false, true], true, true));
    }

    #[test]
    fn ready_completion_precedes_preparation_and_closed_channels_fail() {
        let (sender, receiver) = mpsc::channel();
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
