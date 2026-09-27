//! Host-owned persistent worker pool using the same exact pair wire as WASM.
//! The calling thread is the coordinator; the remaining requested workers run
//! fixed-pair searches. No worker-count reduction or serial retry is hidden.
use std::{panic::{catch_unwind, AssertUnwindSafe}, sync::mpsc, thread};
use clearra_forward_search::{RecoveryBuildQuery, RecoveryBuildPopulation, RecoveryBuildError,
    RecoveryBuildParallelCoordinator as Coordinator, RecoveryBuildParallelWorker as Worker,
    RecoveryBuildParallelError as Error, RecoveryBuildParallelProduce as Produce};
use clearra_core_domain::execution_cancellation::ExecutionControl;

pub(crate) fn run_native_recovery_build(query: RecoveryBuildQuery, workers: usize,
    control: &ExecutionControl) -> Result<RecoveryBuildPopulation, Error> {
    if control.is_cancelled() { return Err(RecoveryBuildError::Cancelled.into()); }
    if workers <= 1 { return query.search(control).map_err(Into::into); }
    let mut coordinator = Coordinator::new(query, workers)?;
    let init = coordinator.worker_initialization();
    let ready = (0..workers - 1).map(|_| Worker::new(&init)).collect::<Result<Vec<_>,_>>()?;
    let (sender, receiver) = mpsc::channel();
    let mut senders = Vec::new();
    let mut handles = Vec::new();
    for (index, mut worker) in ready.into_iter().enumerate() {
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(1);
        let completion = sender.clone();
        let worker_control = control.clone();
        let spawn = thread::Builder::new().name(format!("clearra-recovery-{index}"))
            .spawn(move || {
                while let Ok(packet) = rx.recv() {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        let mut output = worker.consume(&packet, &worker_control)?;
                        while worker.has_pending_work() {
                            output = worker.advance(&worker_control)?;
                        }
                        output.ok_or(Error::InvalidState("recovery worker omitted result"))
                    })).unwrap_or(Err(Error::InvalidState("recovery worker panicked")));
                    let failed = result.is_err();
                    if completion.send((index, result)).is_err() || failed { break; }
                }
            });
        match spawn {
            Ok(handle) => { senders.push(tx); handles.push(handle); },
            Err(_) => {
                control.cancellation.handle().cancel();
                drop(senders); drop(sender);
                for handle in handles { let _ = handle.join(); }
                return Err(Error::InvalidState("recovery worker spawn failed"));
            }
        }
    }
    drop(sender);
    let result = (|| {
        let mut idle = vec![true; senders.len()];
        loop {
            if control.is_cancelled() { return Err(RecoveryBuildError::Cancelled.into()); }
            for (index, free) in idle.iter_mut().enumerate() {
                if !*free { continue; }
                let (status, packet) = coordinator.produce(32, control)?;
                match status {
                    Produce::Batch => {
                        senders[index].send(packet).map_err(|_| Error::InvalidState("recovery worker disconnected"))?;
                        *free = false;
                    },
                    Produce::Completed => return coordinator.finish(control),
                    Produce::Cancelled => return Err(RecoveryBuildError::Cancelled.into()),
                    Produce::Pending => break,
                }
            }
            if idle.iter().all(|free| *free) {
                return Err(Error::InvalidState("recovery coordinator stalled"));
            }
            let (index, packet) = receiver.recv().map_err(|_| Error::InvalidState("recovery result channel closed"))?;
            if index >= idle.len() || idle[index] { return Err(Error::InvalidState("recovery worker completion repeated")); }
            coordinator.absorb(&packet?, control)?;
            idle[index] = true;
        }
    })();
    if result.is_err() { control.cancellation.handle().cancel(); }
    drop(senders);
    let mut joined = true;
    for handle in handles { joined &= handle.join().is_ok(); }
    if !joined { return Err(Error::InvalidState("recovery worker join failed")); }
    result
}
