//! Persistent CPU pool using the same bounded task/receipt protocol as Web.
//! A failed spawn or transport retires only that worker; owned tasks are replayed
//! locally before absorption. Search errors and user cancellation are not NoPath.
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_forward_search::{
    recovery_build::{RecoveryBuildCoordinator, RecoveryBuildProduce, RecoveryBuildWorker},
    RecoveryBuildError, RecoveryBuildPopulation, RecoveryBuildQuery,
};
use std::{
    sync::mpsc::{self, SyncSender},
    thread,
    time::Duration,
};

pub(crate) fn run_native_recovery(
    query: RecoveryBuildQuery,
    requested: usize,
    control: &ExecutionControl,
) -> Result<RecoveryBuildPopulation, RecoveryBuildError> {
    if requested < 2 {
        return query.search(control);
    }
    let mut coordinator = RecoveryBuildCoordinator::new(query.clone(), requested)?;
    if coordinator.progress().possible < 2 {
        return query.search(control);
    }
    let init = coordinator.worker_initialization();
    let mut local = RecoveryBuildWorker::new(&init)?;
    let (completed_tx, completed_rx) = mpsc::channel();
    let mut senders: Vec<Option<SyncSender<Vec<u8>>>> = Vec::new();
    let mut handles = Vec::new();
    for index in 0..requested.saturating_sub(1) {
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(1);
        let done = completed_tx.clone();
        let init = init.clone();
        let child_control = ExecutionControl::new(control.cancellation.clone());
        let spawned = thread::Builder::new()
            .name(format!("clearra-recovery-{index}"))
            .spawn(move || {
                let mut worker = match RecoveryBuildWorker::new(&init) {
                    Ok(worker) => worker,
                    Err(error) => {
                        let _ = done.send((index, Err(error), true));
                        return;
                    }
                };
                while let Ok(task) = rx.recv() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        worker.consume(&task, &child_control)
                    }));
                    let panicked = result.is_err();
                    let result = result.unwrap_or(Err(RecoveryBuildError::InvalidParallelState));
                    if done.send((index, result, panicked)).is_err() || panicked {
                        break;
                    }
                }
            });
        match spawned {
            Ok(handle) => {
                senders.push(Some(tx));
                handles.push(handle);
            }
            Err(_) => break,
        }
    }
    drop(completed_tx);
    let mut inflight: Vec<Option<Vec<u8>>> = vec![None; senders.len()];
    // The scope below always closes senders and joins workers, including errors.
    let result = (|| {
        loop {
            if control.is_cancelled() {
                return Err(RecoveryBuildError::Cancelled);
            }
            for index in 0..senders.len() {
                if senders[index].is_none() || inflight[index].is_some() {
                    continue;
                }
                let (state, task) = coordinator.produce(control)?;
                match state {
                    RecoveryBuildProduce::Batch => {
                        let sent = senders[index]
                            .as_ref()
                            .ok_or(RecoveryBuildError::InvalidParallelState)?
                            .send(task.clone());
                        if sent.is_err() {
                            senders[index] = None;
                            let (_, partial) = local.consume(&task, control)?;
                            coordinator.absorb(&partial, control)?;
                        } else {
                            inflight[index] = Some(task);
                        }
                    }
                    RecoveryBuildProduce::Cancelled => return Err(RecoveryBuildError::Cancelled),
                    _ => break,
                }
            }
            if inflight.iter().all(Option::is_none) {
                match coordinator.produce(control)? {
                    (RecoveryBuildProduce::Completed, _) => break,
                    (RecoveryBuildProduce::Batch, task) => {
                        let (_, partial) = local.consume(&task, control)?;
                        coordinator.absorb(&partial, control)?;
                        continue;
                    }
                    (RecoveryBuildProduce::Cancelled, _) => {
                        return Err(RecoveryBuildError::Cancelled)
                    }
                    _ => return Err(RecoveryBuildError::InvalidParallelState),
                }
            }
            match completed_rx.recv_timeout(Duration::from_millis(25)) {
                Ok((index, result, panicked)) => {
                    if index >= inflight.len() {
                        return Err(RecoveryBuildError::InvalidParallelState);
                    }
                    let task = inflight[index].take();
                    if panicked {
                        senders[index] = None;
                        if let Some(task) = task {
                            let (_, partial) = local.consume(&task, control)?;
                            coordinator.absorb(&partial, control)?;
                        }
                    } else {
                        if task.is_none() {
                            return Err(RecoveryBuildError::InvalidParallelState);
                        }
                        let (_, partial) = result?;
                        coordinator.absorb(&partial, control)?;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    for (sender, task) in senders.iter_mut().zip(&mut inflight) {
                        *sender = None;
                        if let Some(task) = task.take() {
                            let (_, partial) = local.consume(&task, control)?;
                            coordinator.absorb(&partial, control)?;
                        }
                    }
                }
            }
        }
        coordinator.finish(control)
    })();
    // On an execution error, stop outstanding peers before joining. Do not
    // turn an internal failure into an apparently successful reduced result.
    if result.is_err() {
        control.cancellation.handle().cancel();
    }
    drop(senders);
    for handle in handles {
        let _ = handle.join();
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
    #[test]
    fn recovery_native_pool_matches_serial_for_one_two_and_four_workers() {
        let q = RecoveryBuildQuery {
            fields: RecoveryBuildFields {
                height: 8,
                initial: Board256Mask::EMPTY,
                middle: Board256Mask::from_words([15, 0, 0, 0]),
                result: Board256Mask::from_words([0xc030, 0, 0, 0]),
            },
            first_supply: "[IJLOSTZ]".into(),
            second_supply: "[IJLOSTZ]".into(),
            early_limit: CrossStageEarlyLimit::Auto,
            allow_piece_exchange: true,
            hold_enabled: true,
            preserve_b2b: true,
            initial_b2b: true,
            rule_profile: RuleProfileId::SrsPlus,
            spin_profile: SpinProfileId::AllSpinPlus,
        };
        let control = ExecutionControl::default();
        let expected = q.search(&control).unwrap();
        for workers in [1, 2, 4] {
            assert_eq!(
                run_native_recovery(q.clone(), workers, &control).unwrap(),
                expected
            );
        }
    }
}
