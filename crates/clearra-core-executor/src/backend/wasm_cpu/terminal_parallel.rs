//! Canonical result assembly over the caller's existing allocations.
//!
//! Large native results can share disjoint borrowed slices without transferring
//! them through the persistent pool's `'static` queue or copying their payload.
//! Small results and WebAssembly keep the serial path.

use clearra_core_domain::solution::normalized_tiling_solution::NormalizedTilingSolutionKey;
use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;

use super::WasmExactSearchError;

const MIN_SORT_ITEMS_PER_WORKER: usize = 65_536;
const MIN_KEY_ITEMS_PER_WORKER: usize = 8_192;

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
use crate::resource::SCOPED_THREAD_STACK_BYTES;

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
#[derive(Clone, Copy)]
enum ThreadLocality {
    Inherit,
    Apply,
}

fn useful_workers(items: usize, requested: usize, minimum: usize) -> usize {
    if cfg!(all(feature = "parallel", not(target_family = "wasm"))) {
        requested.min(items / minimum).max(1)
    } else {
        1
    }
}

pub(super) fn sort_identities(identities: &mut [StandardBoard64TilingIdentity], workers: usize) {
    let workers = useful_workers(identities.len(), workers, MIN_SORT_ITEMS_PER_WORKER);
    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    if workers > 1 {
        // Keep the standard sort's linear behavior for already ordered runs.
        // On unordered input these checks stop at the first counterexample.
        if identities.is_sorted() {
            return;
        }
        if identities.windows(2).all(|pair| pair[0] >= pair[1]) {
            identities.reverse();
            return;
        }
        if let Some(_scratch) = crate::resource::try_acquire_scoped_thread_memory(workers - 1) {
            sort_partition(identities, workers, 0, ThreadLocality::Inherit);
            return;
        }
    }
    let _ = workers;
    identities.sort_unstable();
}

pub(super) fn canonical_keys(
    identities: &[StandardBoard64TilingIdentity],
    workers: usize,
) -> Result<Vec<String>, WasmExactSearchError> {
    let mut keys = Vec::new();
    keys.try_reserve_exact(identities.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_result_normalized_key_storage_unavailable")
    })?;
    let workers = useful_workers(identities.len(), workers, MIN_KEY_ITEMS_PER_WORKER);
    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    if workers > 1 {
        if let Some(_scratch) = crate::resource::try_acquire_scoped_thread_memory(workers - 1) {
            // String::new owns no allocation. Each worker writes only its existing
            // result slots and first-touches the strings it will produce.
            keys.resize_with(identities.len(), String::new);
            fill_key_partition(identities, &mut keys, workers, 0, ThreadLocality::Inherit);
            return Ok(keys);
        }
    }
    let _ = workers;
    keys.extend(identities.iter().copied().map(canonical_key));
    Ok(keys)
}

fn canonical_key(identity: StandardBoard64TilingIdentity) -> String {
    NormalizedTilingSolutionKey::from_standard_board64_identity(identity).into_string()
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn sort_partition(
    identities: &mut [StandardBoard64TilingIdentity],
    workers: usize,
    first: usize,
    locality: ThreadLocality,
) {
    let workers = useful_workers(identities.len(), workers, MIN_SORT_ITEMS_PER_WORKER);
    if workers <= 1 {
        if matches!(locality, ThreadLocality::Apply) {
            crate::cpu_worker_pool::apply_worker_locality(first);
        }
        identities.sort_unstable();
        return;
    }
    let left_workers = workers / 2;
    let middle = identities.len() / workers * left_workers;
    identities.select_nth_unstable(middle);
    let (left, pivot_and_right) = identities.split_at_mut(middle);
    let (_, right) = pivot_and_right
        .split_first_mut()
        .expect("parallel sort has at least two nonempty partitions");
    with_worker(
        left_workers,
        first,
        |workers, locality| sort_partition(left, workers, first, locality),
        || {
            sort_partition(
                right,
                workers - left_workers,
                first + left_workers,
                locality,
            )
        },
    );
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn fill_key_partition(
    identities: &[StandardBoard64TilingIdentity],
    keys: &mut [String],
    workers: usize,
    first: usize,
    locality: ThreadLocality,
) {
    let workers = useful_workers(identities.len(), workers, MIN_KEY_ITEMS_PER_WORKER);
    if workers <= 1 {
        if matches!(locality, ThreadLocality::Apply) {
            crate::cpu_worker_pool::apply_worker_locality(first);
        }
        for (slot, identity) in keys.iter_mut().zip(identities.iter().copied()) {
            *slot = canonical_key(identity);
        }
        return;
    }
    let left_workers = workers / 2;
    let middle = identities.len() / workers * left_workers;
    let (left_identities, right_identities) = identities.split_at(middle);
    let (left_keys, right_keys) = keys.split_at_mut(middle);
    with_worker(
        left_workers,
        first,
        |workers, locality| {
            fill_key_partition(left_identities, left_keys, workers, first, locality)
        },
        || {
            fill_key_partition(
                right_identities,
                right_keys,
                workers - left_workers,
                first + left_workers,
                locality,
            )
        },
    );
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn with_worker(
    workers: usize,
    worker_index: usize,
    work: impl FnOnce(usize, ThreadLocality) + Send,
    caller_work: impl FnOnce(),
) {
    // This tiny stack-owned slot keeps the borrowed work recoverable when the
    // OS refuses a thread. It is locked once per partition, never per result.
    let work = std::sync::Mutex::new(Some(work));
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name(format!("clearra-terminal-{worker_index}"))
            .stack_size(SCOPED_THREAD_STACK_BYTES)
            .spawn_scoped(scope, || {
                let task = work
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                    .expect("the new worker owns its partition once");
                // Descendants must inherit the original permission before this
                // worker pins itself. Its leaf binds once after its own forks.
                task(workers, ThreadLocality::Apply);
            });
        // Useful work starts immediately; there is no worker creation barrier.
        caller_work();
        match handle {
            Ok(handle) => handle.join().expect("terminal result worker panicked"),
            Err(_) => {
                let task = work
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                    .expect("a failed spawn leaves the partition with its caller");
                task(1, ThreadLocality::Inherit);
            }
        }
    });
}

#[cfg(test)]
#[path = "terminal_parallel_tests.rs"]
mod tests;
