//! Exact compact Tiling traversal over one immutable inverse catalog and DAG.
//! The caller's terminal lease covers all compute slots and all allocations.

use std::{
    mem::size_of,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, ThreadId},
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};

use crate::{
    cpu_worker_pool,
    performance::{
        profile_sample_scale, ExecutorSearchStage, SearchStageSpan, WorkerSearchProfile,
    },
    resource::ExecutionMemoryBound,
    tiling_solution_store::{pack_canonical_tiling_row_ids, PackedTilingRows},
};

use super::super::{
    catalog::GeometryCatalog,
    geometry::{CompactTilingBranch, CompactTilingPlan},
    mix_digest, WasmExactSearchError,
};

struct Work {
    catalog: Arc<GeometryCatalog>,
    rank: Arc<[u32]>,
    queue: Mutex<Vec<IndexedBranch>>,
    outcomes: Mutex<Vec<Option<BranchResult>>>,
    abort: AtomicBool,
    control: ExecutionControl,
    stack_capacity: usize,
}

struct IndexedBranch {
    index: usize,
    branch: CompactTilingBranch,
}

struct Member {
    identity: PackedTilingRows,
    hash: u64,
}

struct BranchResult {
    members: Vec<Member>,
    first_identity: Option<StandardBoard64TilingIdentity>,
}

struct WorkerActivity {
    id: ThreadId,
    candidates: usize,
}

pub(super) struct Outcome {
    pub identities: Vec<PackedTilingRows>,
    pub candidate_digest: u64,
    pub first_identity: Option<StandardBoard64TilingIdentity>,
    pub workers_used: usize,
    pub active_workers: usize,
    pub minimum_worker_candidates: usize,
    pub maximum_worker_candidates: usize,
    pub retained_upper_bound: u128,
}

pub(super) fn execute(
    plan: CompactTilingPlan,
    catalog: Arc<GeometryCatalog>,
    rank: Arc<[u32]>,
    control: &ExecutionControl,
    requested_workers: usize,
    cpu_warmup: bool,
    bound: ExecutionMemoryBound,
    base: u128,
) -> Result<Outcome, WasmExactSearchError> {
    let branch_count = plan.branches.len();
    let workers = requested_workers.min(branch_count);
    // One aggregate upper bound for the whole phase. The exact path count
    // bounds every identity buffer; a checked DAG recurrence caps each
    // traversal scratch. Include both identity representations through join.
    let candidate_bytes = (plan.candidate_count as u128)
        * (size_of::<Member>() + size_of::<PackedTilingRows>()) as u128;
    let stack_bytes = CompactTilingBranch::stack_bytes(plan.worker_stack_capacity)
        .checked_mul(workers as u128)
        .ok_or_else(projection_error)?;
    let coordinator_bytes = (branch_count as u128)
        * (size_of::<IndexedBranch>() + size_of::<Option<BranchResult>>()) as u128
        + (workers as u128)
            * (size_of::<Result<WorkerActivity, WasmExactSearchError>>() + 1024) as u128
        + size_of::<Work>() as u128;
    let retained_upper_bound = base
        .checked_add(plan.retained_upper_bound)
        .and_then(|bytes| bytes.checked_add(candidate_bytes))
        .and_then(|bytes| bytes.checked_add(stack_bytes))
        .and_then(|bytes| bytes.checked_add(coordinator_bytes))
        .ok_or_else(projection_error)?;
    bound
        .ensure(retained_upper_bound, 0)
        .map_err(WasmExactSearchError::resource_admission)?;
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }

    let mut queue = bounded_vec(branch_count)?;
    queue.extend(
        plan.branches
            .into_iter()
            .enumerate()
            .map(|(index, branch)| IndexedBranch { index, branch }),
    );
    queue.sort_unstable_by_key(|task| (task.branch.count(), task.index));
    let pool = if cpu_warmup {
        cpu_worker_pool::prewarm_cpu_workers(workers)
    } else {
        cpu_worker_pool::ensure_cpu_workers(workers)
    }
    .map_err(|_| pool_error())?;
    if pool.total_workers() != workers {
        return Err(pool_error());
    }
    let mut first_branches = bounded_vec(workers)?;
    for _ in 0..workers {
        first_branches.push(queue.pop().expect("one seed per useful worker"));
    }
    let mut outcomes = bounded_vec(branch_count)?;
    outcomes.resize_with(branch_count, || None);
    let work = Arc::new(Work {
        catalog,
        rank,
        queue: Mutex::new(queue),
        outcomes: Mutex::new(outcomes),
        abort: AtomicBool::new(false),
        control: control.clone(),
        stack_capacity: plan.worker_stack_capacity,
    });
    let mut results = bounded_vec(workers)?;
    let (sender, receiver) = mpsc::channel();
    let mut submit_error = None;
    let mut submitted = 0;
    let caller_seed = first_branches.pop().expect("the caller has one seed");
    for seed in first_branches {
        let work = Arc::clone(&work);
        let sender = sender.clone();
        let profile = WorkerSearchProfile::capture();
        if cpu_worker_pool::submit_cpu_job(move || {
            let result = profile.measure(|| safe_worker(&work, seed));
            drop(work);
            let _ = sender.send(result);
        })
        .is_err()
        {
            submit_error = Some(pool_error());
            break;
        }
        submitted += 1;
    }
    drop(sender);
    if submit_error.is_some() {
        work.abort.store(true, Ordering::Release);
    }
    results.push(safe_worker(&work, caller_seed));
    // Result ownership is returned only after dropping the job's Arc. Always
    // drain every submitted job before the terminal lease may be released.
    for _ in 0..submitted {
        results.push(receiver.recv().unwrap_or_else(|_| Err(pool_error())));
    }
    if let Some(error) = submit_error {
        return Err(error);
    }
    if let Some(error) = results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .find(|error| !matches!(error, WasmExactSearchError::Cancelled))
    {
        return Err(error.clone());
    }
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }

    let work = Arc::try_unwrap(work).map_err(|_| pool_error())?;
    let branches = work.outcomes.into_inner().map_err(|_| pool_error())?;
    drop(work.queue);
    let mut activity: Vec<WorkerActivity> = bounded_vec(workers)?;
    for result in results {
        let worker = result?;
        if worker.candidates == 0 {
            continue;
        }
        // A pool can execute more than one logical job on the same physical
        // worker during progressive startup. Report useful distinct threads.
        if let Some(previous) = activity
            .iter_mut()
            .find(|previous| previous.id == worker.id)
        {
            previous.candidates = previous
                .candidates
                .checked_add(worker.candidates)
                .ok_or_else(projection_error)?;
        } else {
            activity.push(worker);
        }
    }
    let active_workers = activity.len();
    let minimum_worker_candidates = activity
        .iter()
        .map(|worker| worker.candidates)
        .min()
        .unwrap_or(0);
    let maximum_worker_candidates = activity
        .iter()
        .map(|worker| worker.candidates)
        .max()
        .unwrap_or(0);
    let mut identities = bounded_vec(plan.candidate_count)?;
    let first_identity = branches
        .iter()
        .flatten()
        .find_map(|branch| branch.first_identity);
    let mut candidate_digest = 0;
    for branch in branches {
        let branch = branch.ok_or(WasmExactSearchError::InvalidProblem(
            "compact_parallel_tiling_branches_incomplete",
        ))?;
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        for member in branch.members {
            candidate_digest = mix_digest(candidate_digest, member.hash);
            identities.push(member.identity);
        }
    }
    if identities.len() != plan.candidate_count || first_identity.is_none() {
        return Err(WasmExactSearchError::InvalidProblem(
            "compact_parallel_tiling_count_mismatch",
        ));
    }
    Ok(Outcome {
        identities,
        candidate_digest,
        first_identity,
        workers_used: active_workers,
        active_workers,
        minimum_worker_candidates,
        maximum_worker_candidates,
        retained_upper_bound,
    })
}

fn safe_worker(work: &Work, seed: IndexedBranch) -> Result<WorkerActivity, WasmExactSearchError> {
    let result = catch_unwind(AssertUnwindSafe(|| run_worker(work, seed)))
        .map_err(|_| pool_error())
        .and_then(|result| result);
    if result.is_err() {
        work.abort.store(true, Ordering::Release);
    }
    result.map(|candidates| WorkerActivity {
        id: thread::current().id(),
        candidates,
    })
}

fn run_worker(work: &Work, seed: IndexedBranch) -> Result<usize, WasmExactSearchError> {
    let mut next = Some(seed);
    let mut total = 0_usize;
    while let Some(IndexedBranch { index, mut branch }) = next {
        if work.control.is_cancelled() || work.abort.load(Ordering::Acquire) {
            return Err(WasmExactSearchError::Cancelled);
        }
        branch
            .reserve_stack(work.stack_capacity)
            .ok_or_else(projection_error)?;
        let expected = usize::try_from(branch.count()).map_err(|_| projection_error())?;
        let mut members = bounded_vec(expected)?;
        let mut first_identity = None;
        loop {
            if work.control.is_cancelled() || work.abort.load(Ordering::Acquire) {
                return Err(WasmExactSearchError::Cancelled);
            }
            let span = SearchStageSpan::begin_scaled(
                ExecutorSearchStage::WasmGeometryAdvance,
                profile_sample_scale(members.len().saturating_add(1)),
            );
            let candidate = branch.next(&work.catalog);
            span.finish(1);
            let Some(candidate) = candidate? else { break };
            if members.len() == expected {
                return Err(WasmExactSearchError::InvalidProblem(
                    "compact_parallel_tiling_count_mismatch",
                ));
            }
            let identity = pack_canonical_tiling_row_ids(candidate.row_ids(), &work.rank).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_compact_tiling_identity_invalid"),
            )?;
            first_identity.get_or_insert(candidate.identity);
            members.push(Member {
                identity,
                hash: candidate.identity.bucket_hash(),
            });
        }
        if members.len() != expected {
            return Err(WasmExactSearchError::InvalidProblem(
                "compact_parallel_tiling_count_mismatch",
            ));
        }
        total = total.checked_add(expected).ok_or_else(projection_error)?;
        let mut outcomes = work.outcomes.lock().map_err(|_| pool_error())?;
        let slot = outcomes.get_mut(index).ok_or_else(projection_error)?;
        if slot.is_some() {
            return Err(WasmExactSearchError::InvalidProblem(
                "compact_parallel_tiling_duplicate_branch",
            ));
        }
        *slot = Some(BranchResult {
            members,
            first_identity,
        });
        drop(outcomes);
        drop(branch);
        next = work.queue.lock().map_err(|_| pool_error())?.pop();
    }
    Ok(total)
}

fn bounded_vec<T>(capacity: usize) -> Result<Vec<T>, WasmExactSearchError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| projection_error())?;
    if values.capacity() != capacity {
        return Err(projection_error());
    }
    Ok(values)
}

fn projection_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("compact_parallel_tiling_projection_unavailable")
}

fn pool_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("compact_parallel_tiling_worker_pool_unavailable")
}
