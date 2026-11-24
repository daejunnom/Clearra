//! Bounded dynamic CPU work on one request's shared compute budget.
//!
//! The request coordinator manages N compute workers and never evaluates a
//! chunk. Workers begin claiming work without a startup barrier. A stage starts
//! at most one private workspace per available chunk, while retaining the full
//! request budget. This helper never changes a resource limit or retries work.
//! Callers retain responsibility for admitting every private workspace and
//! writing results to their original source ordinal.

use std::{
    ops::Range,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

use crate::execution_cancellation::{ComputePermit, ExecutionControl};

#[cfg(not(target_family = "wasm"))]
std::thread_local! {
    static ACTIVE_BUDGET: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

#[cfg(not(target_family = "wasm"))]
struct WorkerScope(Option<usize>);

#[cfg(not(target_family = "wasm"))]
impl WorkerScope {
    fn enter(permit: &ComputePermit) -> Self {
        let id = permit.budget_identity();
        Self(ACTIVE_BUDGET.with(|active| active.replace(Some(id))))
    }
}

/// Bind an already acquired compute permit to a native pool job. Existing
/// executors use this entrypoint so nested bulk stages reuse the same slot.
/// The permit is returned even if `work` unwinds.
pub fn with_compute_worker<T>(permit: ComputePermit, work: impl FnOnce() -> T) -> T {
    #[cfg(not(target_family = "wasm"))]
    let _worker = WorkerScope::enter(&permit);
    let _permit = permit;
    work()
}

/// Temporarily make an existing compute worker a control-only coordinator for
/// an executor that explicitly borrows N permits and joins its own jobs.
/// `work` must only manage and drain that pool; do preparation and reduction
/// outside this closure. An ordinary coordinator owns no slot to suspend.
///
/// Reacquisition happens before the outer worker can compute again, including
/// cancellation and unwinding. The original permit remains the sole owner;
/// the suspension guard balances its temporary release without cloning it.
pub fn with_compute_coordinator<T>(control: &ExecutionControl, work: impl FnOnce() -> T) -> T {
    #[cfg(not(target_family = "wasm"))]
    let _suspension = {
        let budget_id = std::sync::Arc::as_ptr(control.compute_budget()) as usize;
        if ACTIVE_BUDGET.with(|active| active.get() == Some(budget_id)) {
            let previous = ACTIVE_BUDGET.with(|active| active.replace(None));
            control.compute_budget().release_worker_slot();
            Some(SuspendedWorker {
                budget: control.compute_budget(),
                previous,
            })
        } else {
            None
        }
    };
    #[cfg(target_family = "wasm")]
    let _ = control;
    work()
}

#[cfg(not(target_family = "wasm"))]
struct SuspendedWorker<'a> {
    budget: &'a crate::execution_cancellation::ComputeBudget,
    previous: Option<usize>,
}

#[cfg(not(target_family = "wasm"))]
impl Drop for SuspendedWorker<'_> {
    fn drop(&mut self) {
        self.budget.resume_suspended_worker();
        ACTIVE_BUDGET.with(|active| active.set(self.previous));
    }
}

#[cfg(not(target_family = "wasm"))]
impl Drop for WorkerScope {
    fn drop(&mut self) {
        ACTIVE_BUDGET.with(|active| active.set(self.0));
    }
}

#[derive(Debug)]
pub enum ParallelWorkError<E> {
    Worker(E),
    Cancelled,
    ThreadSpawnFailed,
    WorkerPanicked,
}

/// Process every index exactly once, unless a worker fails or cancellation
/// is requested. Dynamic chunks prevent a large final static partition from
/// retaining all remaining work. A worker reuses one initialized state.
///
/// Nested invocations must receive the same control. They borrow available
/// permits from the common request instead of starting another N-wide pool.
/// Only a thread already holding that request's compute permit may execute
/// nested work itself; an ordinary calling coordinator only spawns and joins.
/// WebAssembly workers execute their assigned chunk on the calling thread.
pub fn try_for_each_chunk<S, E>(
    control: &ExecutionControl,
    len: usize,
    chunk_size: usize,
    initialize: impl Fn() -> Result<S, E> + Sync,
    visit: impl Fn(&mut S, Range<usize>) -> Result<(), E> + Sync,
) -> Result<(), ParallelWorkError<E>>
where
    S: Send,
    E: Send,
{
    if control.is_cancelled() {
        return Err(ParallelWorkError::Cancelled);
    }
    if len == 0 {
        return Ok(());
    }
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let error = Mutex::new(None);
    let chunk_size = chunk_size.max(1);
    let record_error = |failure| {
        stop.store(true, Ordering::Release);
        let mut slot = error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(failure);
        }
    };
    let run = || {
        if stop.load(Ordering::Acquire) || next.load(Ordering::Relaxed) >= len {
            return;
        }
        let mut state = match initialize() {
            Ok(state) => state,
            Err(failure) => {
                record_error(ParallelWorkError::Worker(failure));
                return;
            }
        };
        loop {
            if stop.load(Ordering::Acquire) {
                return;
            }
            if control.is_cancelled() {
                record_error(ParallelWorkError::Cancelled);
                return;
            }
            let start = match next.fetch_update(Ordering::AcqRel, Ordering::Acquire, |start| {
                (start < len).then(|| start.saturating_add(chunk_size).min(len))
            }) {
                Ok(start) => start,
                Err(_) => return,
            };
            if let Err(failure) =
                visit(&mut state, start..start.saturating_add(chunk_size).min(len))
            {
                record_error(ParallelWorkError::Worker(failure));
                return;
            }
        }
    };

    #[cfg(not(target_family = "wasm"))]
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        let budget_id = std::sync::Arc::as_ptr(control.compute_budget()) as usize;
        let already_worker = ACTIVE_BUDGET.with(|active| active.get() == Some(budget_id));
        // Admissions count only workspaces that can own a chunk in this stage.
        // Keep the request's full budget, but do not race N initializers for a
        // short terminal batch containing fewer than N chunks.
        let workers = control.parallelism().min(len.div_ceil(chunk_size));
        let additional = workers.saturating_sub(usize::from(already_worker));
        for index in 0..additional {
            if stop.load(Ordering::Acquire) {
                break;
            }
            let permit = if already_worker {
                control.compute_budget().try_acquire_worker()
            } else {
                control
                    .compute_budget()
                    .acquire_worker(&control.cancellation)
            };
            let Some(permit) = permit else {
                break;
            };
            let run_ref = &run;
            let record_error_ref = &record_error;
            let handle = std::thread::Builder::new()
                .name(format!("clearra-product-{index}"))
                .spawn_scoped(scope, move || {
                    with_compute_worker(permit, || {
                        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(run_ref)).is_err()
                        {
                            record_error_ref(ParallelWorkError::WorkerPanicked);
                        }
                    });
                });
            match handle {
                Ok(handle) => handles.push(handle),
                Err(_) => {
                    record_error(ParallelWorkError::ThreadSpawnFailed);
                    break;
                }
            }
        }
        // Reuse an existing compute worker's slot for nested work. The request
        // coordinator never enters this path, even for a one-worker request.
        if already_worker && std::panic::catch_unwind(std::panic::AssertUnwindSafe(&run)).is_err() {
            record_error(ParallelWorkError::WorkerPanicked);
        }
        for handle in handles {
            if handle.join().is_err() {
                record_error(ParallelWorkError::WorkerPanicked);
            }
        }
    });

    #[cfg(target_family = "wasm")]
    run();

    match error
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
    {
        Some(failure) => Err(failure),
        None if control.is_cancelled() => Err(ParallelWorkError::Cancelled),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn dynamic_chunks_preserve_every_original_index_and_nested_width() {
        let control = ExecutionControl::default().with_parallelism(8);
        let counts: Vec<_> = (0..1003).map(|_| AtomicUsize::new(0)).collect();
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let coordinator = std::thread::current().id();
        try_for_each_chunk(
            &control,
            counts.len(),
            3,
            || Ok::<_, ()>(()),
            |_, range| {
                assert_ne!(std::thread::current().id(), coordinator);
                for index in range {
                    try_for_each_chunk(
                        &control,
                        7,
                        1,
                        || Ok::<_, ()>(()),
                        |_, inner| {
                            assert_ne!(std::thread::current().id(), coordinator);
                            let live = active.fetch_add(1, Ordering::AcqRel) + 1;
                            peak.fetch_max(live, Ordering::Relaxed);
                            for inner_index in inner {
                                if inner_index == 0 {
                                    counts[index].fetch_add(1, Ordering::Relaxed);
                                }
                                std::hint::spin_loop();
                            }
                            active.fetch_sub(1, Ordering::AcqRel);
                            Ok(())
                        },
                    )
                    .map_err(|_| ())?;
                }
                Ok(())
            },
        )
        .unwrap();
        assert!(counts
            .iter()
            .all(|count| count.load(Ordering::Relaxed) == 1));
        assert!(peak.load(Ordering::Relaxed) <= 8);
        assert_eq!(active.load(Ordering::Relaxed), 0);
        assert!(Arc::ptr_eq(
            control.compute_budget(),
            control.clone().with_parallelism(8).compute_budget()
        ));
    }

    #[test]
    fn error_and_cancellation_return_after_workers_drain() {
        let control = ExecutionControl::default().with_parallelism(4);
        let result = try_for_each_chunk(
            &control,
            100,
            1,
            || Ok::<_, u8>(()),
            |_, range| {
                if range.start == 3 {
                    Err(7)
                } else {
                    Ok(())
                }
            },
        );
        assert!(matches!(result, Err(ParallelWorkError::Worker(7))));
        // The failed run returned every permit; failure cannot silently shrink
        // the width of the next stage.
        let permits: Vec<_> = (0..4)
            .map(|_| control.compute_budget().try_acquire_worker().unwrap())
            .collect();
        assert!(control.compute_budget().try_acquire_worker().is_none());
        drop(permits);
        control.cancellation.handle().cancel();
        assert!(matches!(
            try_for_each_chunk(&control, 1, 1, || Ok::<_, ()>(()), |_, _| Ok(())),
            Err(ParallelWorkError::Cancelled)
        ));
    }

    #[test]
    fn single_worker_still_keeps_the_coordinator_out_of_compute() {
        let control = ExecutionControl::default();
        let coordinator = std::thread::current().id();
        let count = AtomicUsize::new(0);
        try_for_each_chunk(
            &control,
            9,
            1,
            || {
                assert_ne!(std::thread::current().id(), coordinator);
                Ok::<_, ()>(())
            },
            |_, range| {
                assert_ne!(std::thread::current().id(), coordinator);
                count.fetch_add(range.len(), Ordering::Relaxed);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(count.load(Ordering::Relaxed), 9);
    }

    #[test]
    fn short_stages_admit_only_their_live_chunk_workspaces() {
        let control = ExecutionControl::default().with_parallelism(8);
        let coordinator = std::thread::current().id();
        for (len, chunk_size, expected_maximum) in [(1, 16, 1), (17, 16, 2), (3, 0, 3)] {
            let initialized = AtomicUsize::new(0);
            let visited = AtomicUsize::new(0);
            try_for_each_chunk(
                &control,
                len,
                chunk_size,
                || {
                    assert_ne!(std::thread::current().id(), coordinator);
                    initialized.fetch_add(1, Ordering::AcqRel);
                    Ok::<_, ()>(())
                },
                |_, range| {
                    assert_ne!(std::thread::current().id(), coordinator);
                    visited.fetch_add(range.len(), Ordering::AcqRel);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(visited.load(Ordering::Acquire), len);
            assert!((1..=expected_maximum).contains(&initialized.load(Ordering::Acquire)));
            assert_eq!(control.parallelism(), 8);
            let permits: Vec<_> = (0..8)
                .map(|_| control.compute_budget().try_acquire_worker().unwrap())
                .collect();
            assert!(control.compute_budget().try_acquire_worker().is_none());
            drop(permits);
        }
    }

    #[test]
    fn nested_explicit_pool_suspends_its_coordinator_and_restores_after_unwind() {
        for width in [1, 8] {
            let control = ExecutionControl::default().with_parallelism(width);
            let request_thread = std::thread::current().id();
            try_for_each_chunk(
                &control,
                1,
                1,
                || Ok::<_, ()>(()),
                |_, _| {
                    let outer_worker = std::thread::current().id();
                    let visited = AtomicUsize::new(0);
                    with_compute_coordinator(&control, || {
                        let permits: Vec<_> = (0..width)
                            .map(|_| {
                                control
                                    .compute_budget()
                                    .acquire_worker(&control.cancellation)
                                    .unwrap()
                            })
                            .collect();
                        assert!(control.compute_budget().try_acquire_worker().is_none());
                        std::thread::scope(|scope| {
                            for permit in permits {
                                let visited = &visited;
                                scope.spawn(move || {
                                    with_compute_worker(permit, || {
                                        assert_ne!(std::thread::current().id(), request_thread);
                                        assert_ne!(std::thread::current().id(), outer_worker);
                                        visited.fetch_add(1, Ordering::AcqRel);
                                    })
                                });
                            }
                        });
                    });
                    assert_eq!(visited.load(Ordering::Acquire), width);
                    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        with_compute_coordinator(&control, || panic!("test unwind"))
                    }));
                    assert!(panic.is_err());
                    // One original slot is held again, even after the panic.
                    let spare: Vec<_> = (1..width)
                        .map(|_| control.compute_budget().try_acquire_worker().unwrap())
                        .collect();
                    assert!(control.compute_budget().try_acquire_worker().is_none());
                    drop(spare);
                    // TLS role was restored too: a nested one-slot stage reuses
                    // this worker and never blocks on its own held permit.
                    try_for_each_chunk(
                        &control,
                        1,
                        1,
                        || Ok::<_, ()>(()),
                        |_, _| {
                            assert_eq!(std::thread::current().id(), outer_worker);
                            Ok(())
                        },
                    )
                    .map_err(|_| ())
                },
            )
            .unwrap();
            let all: Vec<_> = (0..width)
                .map(|_| control.compute_budget().try_acquire_worker().unwrap())
                .collect();
            assert!(control.compute_budget().try_acquire_worker().is_none());
            drop(all);
        }
    }
}
