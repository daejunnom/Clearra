//! Native scheduling for the same privately issued exact tasks as the browser.
//! The control role handles source-bound receipts and memory admission only.
use super::*;
use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    parallel_work::{try_for_each_chunk, with_compute_coordinator, ParallelWorkError},
};
use clearra_coverage::cover::{
    ExactAtMostParallelError, ExactAtMostShardAdvance, ExactAtMostShardOutcome,
    ExactAtMostShardSession,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
    time::Duration,
};

struct Admission {
    slot: usize,
    requested: u128,
    response: mpsc::SyncSender<Result<(), ExactMinimumCoverError>>,
}

impl CoveragePortfolioAlternativeSetPreparation {
    /// Execute one available window of privately issued exact AtMost tasks.
    /// The supplied guard sees the complete portfolio owner plus every live
    /// native shard/receipt. It remains on the calling control role, so callers
    /// do not need to make their existing admission authority Send or Clone.
    pub(crate) fn drive_native_parallel_tasks(
        &mut self,
        control: &ExecutionControl,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<bool, PortfolioAlternativeError> {
        if cancelled() || control.is_cancelled() {
            return Err(PortfolioAlternativeError::PageReplayCancelled);
        }
        if self.parallel_partitions.is_none() {
            self.enable_parallel(control.parallelism().saturating_mul(4).max(2))?;
        }
        let Some(query) = self.parallel_query().cloned() else {
            return Ok(false);
        };
        let shared_query = query
            .checked_retained_bytes()
            .ok_or_else(portfolio_projection_overflow)?;
        let mut tasks = Vec::new();
        while let Some(task) = self.take_parallel_task() {
            tasks.push(Mutex::new(Some(task)));
        }
        if tasks.is_empty() {
            return Ok(false);
        }
        let receipts = (0..tasks.len())
            .map(|_| Mutex::new(None))
            .collect::<Vec<_>>();
        let mut live = tasks
            .iter()
            .map(|task| {
                task.lock()
                    .map_err(|_| worker_error("minimum_native_task_poisoned"))?
                    .as_ref()
                    .unwrap()
                    .checked_retained_bytes()
                    .ok_or_else(portfolio_projection_overflow)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let base =
            self.checked_retained_capacity_bytes()
                .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() as u128))
                .and_then(|bytes| {
                    bytes.checked_add((tasks.capacity() as u128).checked_mul(
                        core::mem::size_of::<Mutex<Option<ExactAtMostTask>>>() as u128,
                    )?)
                })
                .and_then(|bytes| {
                    bytes.checked_add((receipts.capacity() as u128).checked_mul(
                        core::mem::size_of::<Mutex<Option<ExactAtMostReceipt>>>() as u128,
                    )?)
                })
                .and_then(|bytes| {
                    bytes.checked_add(
                        (live.capacity() as u128)
                            .checked_mul(core::mem::size_of::<u128>() as u128)?,
                    )
                })
                .ok_or_else(portfolio_projection_overflow)?;
        memory_guard(total_live(base, &live)?).map_err(portfolio_minimum_cover_error)?;
        let failed = AtomicBool::new(false);
        let completed = with_compute_coordinator(control, || {
            std::thread::scope(|scope| {
                let (sender, receiver) =
                    mpsc::sync_channel::<Admission>(control.parallelism().max(1));
                let tasks = &tasks;
                let receipts = &receipts;
                let failed = &failed;
                // This extra thread only starts/joins the common shared-budget
                // helper. It holds no permit and never evaluates a solver node.
                let driver = std::thread::Builder::new()
                    .name("clearra-minimum-pool".into())
                    .spawn_scoped(scope, move || {
                        try_for_each_chunk(
                            control,
                            tasks.len(),
                            1,
                            || Ok::<_, ExactAtMostParallelError>(()),
                            |_, range| {
                                for slot in range {
                                    let task = tasks[slot]
                                        .lock()
                                        .map_err(|_| ExactAtMostParallelError::InvalidTask)?
                                        .take()
                                        .ok_or(ExactAtMostParallelError::InvalidTask)?;
                                    let (reply, replies) = mpsc::sync_channel(1);
                                    let mut admitted = 0_u128;
                                    let mut guard =
                                        |requested: u128| -> Result<(), ExactMinimumCoverError> {
                                            let requested = requested
                                                .checked_sub(shared_query)
                                                .and_then(|bytes| {
                                                    bytes.checked_add(core::mem::size_of::<
                                                        ExactAtMostShardSession,
                                                    >(
                                                    )
                                                        as u128)
                                                })
                                                .ok_or(
                                                    ExactMinimumCoverError::ProjectionOverflow,
                                                )?;
                                            // Capacity high-water admission avoids a
                                            // control round trip at every solver node.
                                            if requested <= admitted {
                                                return Ok(());
                                            }
                                            sender
                                                .send(Admission {
                                                    slot,
                                                    requested,
                                                    response: reply.clone(),
                                                })
                                                .map_err(|_| {
                                                    ExactMinimumCoverError::MemoryGuardRejected
                                                })?;
                                            replies.recv().map_err(|_| {
                                                ExactMinimumCoverError::MemoryGuardRejected
                                            })??;
                                            admitted = requested;
                                            Ok(())
                                        };
                                    let mut is_cancelled =
                                        || control.is_cancelled() || failed.load(Ordering::Acquire);
                                    let mut shard = ExactAtMostShardSession::prepare(
                                        query.clone(),
                                        task,
                                        &mut guard,
                                        &mut is_cancelled,
                                    )?;
                                    let receipt = loop {
                                        match shard.advance(512, &mut guard, &mut is_cancelled)? {
                                            ExactAtMostShardAdvance::Pending { .. } => {}
                                            ExactAtMostShardAdvance::Terminal(receipt) => {
                                                break receipt
                                            }
                                        }
                                    };
                                    drop(shard);
                                    let retained = receipt
                                        .task()
                                        .checked_retained_bytes()
                                        .and_then(|bytes| {
                                            bytes.checked_add(match receipt.outcome() {
                                                ExactAtMostShardOutcome::Found(rows) => {
                                                    (rows.capacity() as u128).checked_mul(
                                                        core::mem::size_of::<usize>() as u128,
                                                    )?
                                                }
                                                _ => 0,
                                            })
                                        })
                                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                                    sender
                                        .send(Admission {
                                            slot,
                                            requested: retained,
                                            response: reply,
                                        })
                                        .map_err(|_| ExactMinimumCoverError::MemoryGuardRejected)?;
                                    replies.recv().map_err(|_| {
                                        ExactMinimumCoverError::MemoryGuardRejected
                                    })??;
                                    *receipts[slot]
                                        .lock()
                                        .map_err(|_| ExactAtMostParallelError::InvalidTask)? =
                                        Some(receipt);
                                }
                                Ok(())
                            },
                        )
                    })
                    .map_err(|_| worker_error("minimum_native_manager_spawn_failed"))?;
                let mut first_error = None;
                loop {
                    if cancelled() {
                        control.cancellation.handle().cancel();
                    }
                    match receiver.recv_timeout(Duration::from_millis(25)) {
                        Ok(admission) => {
                            let old = live[admission.slot];
                            live[admission.slot] = admission.requested;
                            let answer = total_live(base, &live)
                                .map_err(|_| ExactMinimumCoverError::ProjectionOverflow)
                                .and_then(&mut *memory_guard);
                            if let Err(error) = answer {
                                live[admission.slot] = old;
                                failed.store(true, Ordering::Release);
                                first_error.get_or_insert(error);
                            }
                            let _ = admission.response.send(answer);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                let result = driver
                    .join()
                    .map_err(|_| worker_error("minimum_native_manager_panicked"))?;
                if let Some(error) = first_error {
                    return Err(portfolio_minimum_cover_error(error));
                }
                result.map_err(|error| match error {
                    ParallelWorkError::Worker(ExactAtMostParallelError::Exact(error)) => {
                        portfolio_minimum_cover_error(error)
                    }
                    ParallelWorkError::Worker(ExactAtMostParallelError::Cancelled)
                    | ParallelWorkError::Cancelled => {
                        PortfolioAlternativeError::PageReplayCancelled
                    }
                    ParallelWorkError::Worker(_) => {
                        worker_error("minimum_native_worker_result_invalid")
                    }
                    ParallelWorkError::ThreadSpawnFailed => {
                        worker_error("minimum_native_worker_spawn_failed")
                    }
                    ParallelWorkError::WorkerPanicked => {
                        worker_error("minimum_native_worker_panicked")
                    }
                })
            })
        });
        completed?;
        if control.is_cancelled() {
            return Err(PortfolioAlternativeError::PageReplayCancelled);
        }
        for receipt in receipts {
            let receipt = receipt
                .into_inner()
                .map_err(|_| worker_error("minimum_native_receipt_poisoned"))?
                .ok_or_else(|| worker_error("minimum_native_receipt_missing"))?;
            self.accept_parallel_receipt(receipt)?;
        }
        Ok(true)
    }
}

fn total_live(base: u128, live: &[u128]) -> Result<u128, PortfolioAlternativeError> {
    live.iter().try_fold(base, |sum, bytes| {
        sum.checked_add(*bytes)
            .ok_or_else(portfolio_projection_overflow)
    })
}
fn worker_error(component: &'static str) -> PortfolioAlternativeError {
    PortfolioAlternativeError::NativeWorkerFailure { component }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> (
        PortfolioAlternativeSetIdentity,
        Vec<String>,
        PatternBitSet,
        Vec<PatternBitSet>,
    ) {
        let identity = PortfolioAlternativeSetIdentity::new(
            "native-query",
            "native-source",
            "native-profile",
            "native-universe",
            "native-build",
        )
        .unwrap();
        let rows = [[0, 1], [1, 2], [2, 3], [0, 3], [0, 2], [1, 3]]
            .into_iter()
            .map(|indices| PatternBitSet::from_pattern_indices(4, indices.to_vec()).unwrap())
            .collect::<Vec<_>>();
        let required = PatternBitSet::from_pattern_indices(4, vec![0, 1, 2, 3]).unwrap();
        let keys = (0..rows.len())
            .map(|index| format!("candidate-{index}"))
            .collect();
        (identity, keys, required, rows)
    }
    #[test]
    fn native_exact_tasks_preserve_canonical_portfolio_and_guard_owner_at_every_width() {
        let (identity, keys, required, rows) = input();
        let expected = CoveragePortfolioAlternativeSet::new_canonical(
            identity.clone(),
            keys.clone(),
            required.clone(),
            rows.clone(),
        )
        .unwrap();
        for workers in [1, 2, 4, 8] {
            let mut preparation = CoveragePortfolioAlternativeSetPreparation::new(
                identity.clone(),
                keys.clone(),
                required.clone(),
                rows.clone(),
            )
            .unwrap();
            let control = ExecutionControl::default().with_parallelism(workers);
            let owner = std::thread::current().id();
            let mut peak = 0;
            let actual = loop {
                preparation
                    .drive_native_parallel_tasks(
                        &control,
                        &mut |bytes| {
                            assert_eq!(std::thread::current().id(), owner);
                            peak = peak.max(bytes);
                            Ok(())
                        },
                        &mut || false,
                    )
                    .unwrap();
                match preparation.advance(16, &mut || false).unwrap() {
                    CoveragePortfolioAlternativeSetPreparationAdvance::Pending { .. } => {}
                    CoveragePortfolioAlternativeSetPreparationAdvance::Completed(set) => break set,
                    CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled { .. } => {
                        panic!("uncancelled fixture")
                    }
                }
            };
            assert_eq!(actual.canonical_page(), expected.canonical_page());
            assert!(peak > 0);
        }
    }
}
