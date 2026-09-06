//! SRP rationale: own one portable exact AtMost query and its cooperative
//! worker shard independently of Geometry's verifier and product projection.

use clearra_coverage::cover::{ExactAtMostQuery, ExactAtMostShardAdvance, ExactAtMostShardSession};

use crate::{minimum_parallel_wire, WasmCommandRuntimeError};

/// A real local execution lease, retained beside the shared App finalizer.
/// The physical floor can include the entire WASM linear allocation. Later
/// checks add only growth above the captured App/ABI owners and the additional
/// shard peak; the shared query is already counted by the App envelope.
pub(crate) struct MinimumCoordinatorMemory {
    _authority: clearra_core_executor::WasmCpuTerminalResourceAuthority,
    limit: u128,
    baseline: u128,
    app_baseline: u128,
    outer_baseline: u128,
}

impl MinimumCoordinatorMemory {
    pub(crate) fn partition_preview(
        &self,
        host_grant: u128,
        app_bytes: u128,
        outer_bytes: u128,
        physical_floor: u128,
        remote_count: usize,
        host_carriers: u128,
    ) -> Option<u128> {
        let baseline = app_bytes.checked_add(outer_bytes)?.max(physical_floor);
        let available = self
            .limit
            .min(host_grant)
            .checked_sub(host_carriers)?
            .checked_sub(baseline)?;
        let slice = available.checked_div((remote_count as u128).checked_add(1)?)?;
        (remote_count > 0 && slice > 0).then_some(slice)
    }

    pub(crate) fn partition(
        &mut self,
        host_grant: u128,
        app_bytes: u128,
        outer_bytes: u128,
        physical_floor: u128,
        remote_count: usize,
        host_carriers: u128,
    ) -> Option<u128> {
        let total = self.limit.min(host_grant);
        let baseline = app_bytes.checked_add(outer_bytes)?.max(physical_floor);
        let available = total.checked_sub(host_carriers)?.checked_sub(baseline)?;
        let slice = available.checked_div((remote_count as u128).checked_add(1)?)?;
        if remote_count == 0 || slice == 0 {
            return None;
        }
        self.limit = baseline.checked_add(slice)?;
        self.baseline = baseline;
        self.app_baseline = app_bytes;
        self.outer_baseline = outer_bytes;
        Some(slice)
    }

    pub(crate) fn try_acquire(
        request_limit: Option<u128>,
        app_bytes: u128,
        outer_bytes: u128,
        physical_floor: u128,
    ) -> Option<Self> {
        let authority =
            clearra_core_executor::WasmCpuTerminalResourceAuthority::try_acquire_full_capacity()
                .ok()?;
        let limit = request_limit.map_or(authority.memory_capacity_bytes(), |requested| {
            requested.min(authority.memory_capacity_bytes())
        });
        let baseline = app_bytes.checked_add(outer_bytes)?.max(physical_floor);
        if baseline >= limit {
            return None;
        }
        Some(Self {
            _authority: authority,
            limit,
            baseline,
            app_baseline: app_bytes,
            outer_baseline: outer_bytes,
        })
    }

    pub(crate) fn ensure(
        &self,
        app_bytes: u128,
        outer_bytes: u128,
        additional_shard_peak: u128,
    ) -> Result<(), clearra_coverage::cover::ExactMinimumCoverError> {
        use clearra_coverage::cover::ExactMinimumCoverError;
        let required = self
            .baseline
            .checked_add(app_bytes.saturating_sub(self.app_baseline))
            .and_then(|bytes| bytes.checked_add(outer_bytes.saturating_sub(self.outer_baseline)))
            .and_then(|bytes| bytes.checked_add(additional_shard_peak))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        #[cfg(target_arch = "wasm32")]
        let required = required.max(
            (core::arch::wasm32::memory_size(0) as u128)
                .checked_mul(65_536)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        );
        if required > self.limit {
            return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes: required,
                max_memory_bytes: self.limit,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod coordinator_memory_tests {
    use super::MinimumCoordinatorMemory;

    #[test]
    fn local_guard_counts_whole_floor_growth_and_additional_peak_once() {
        let memory = MinimumCoordinatorMemory::try_acquire(Some(100), 20, 10, 40)
            .expect("synthetic owner fits a real local lease");
        assert!(memory.ensure(20, 10, 60).is_ok());
        assert!(memory.ensure(20, 10, 61).is_err());
        assert!(memory.ensure(25, 12, 53).is_ok());
        assert!(memory.ensure(25, 12, 54).is_err());
        assert!(
            memory.ensure(15, 8, 60).is_ok(),
            "shrinking owners do not invent extra credit"
        );
        assert!(
            memory.ensure(u128::MAX, 10, 1).is_err(),
            "overflow is unavailable, never zero"
        );
        drop(memory);
        assert!(MinimumCoordinatorMemory::try_acquire(Some(30), 20, 10, 40).is_none());
        assert!(MinimumCoordinatorMemory::try_acquire(None, u128::MAX, 1, 0).is_none());
    }

    #[test]
    fn control_partition_reserves_host_carriers_and_fixed_remote_slices() {
        for remotes in [1, 2, 10, 11, 31, 32] {
            let mut memory =
                MinimumCoordinatorMemory::try_acquire(Some(10_000), 200, 100, 500).unwrap();
            let before = memory.limit;
            assert_eq!(
                memory.partition_preview(400, 200, 100, 500, remotes, 50),
                None
            );
            assert_eq!(
                memory.limit, before,
                "decline cannot poison shared fallback admission"
            );
            let slice = memory.partition(8_000, 200, 100, 500, remotes, 50).unwrap();
            assert_eq!(slice, (8_000 - 500 - 50) / (remotes as u128 + 1));
            assert!(memory.limit + slice * remotes as u128 + 50 <= 8_000);
            assert!(memory.ensure(200, 100, slice).is_ok());
            assert!(
                memory.ensure(201, 100, slice).is_err(),
                "manager cannot borrow idle remote reservations"
            );
            assert!(
                memory.ensure(200, 101, slice).is_err(),
                "ABI carrier growth is not free"
            );
            assert!(memory.ensure(u128::MAX, 100, 1).is_err());
        }
    }
}

/// One remote instance keeps a real authority and a fixed, source-bound slice
/// for its lifetime. A decoded query is never authority to multiply the whole
/// request cap by the number of workers.
struct MinimumRemoteMemory {
    _authority: clearra_core_executor::WasmCpuTerminalResourceAuthority,
    limit: u128,
    baseline: u128,
    outer_baseline: u128,
}

fn remote_memory_error(reason: impl Into<String>) -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_MINIMUM_PARALLEL_RESOURCE", reason)
}

fn remote_shard_error(
    error: clearra_coverage::cover::ExactAtMostParallelError,
) -> WasmCommandRuntimeError {
    use clearra_coverage::cover::{ExactAtMostParallelError, ExactMinimumCoverError};
    let code = if matches!(
        error,
        ExactAtMostParallelError::Exact(
            ExactMinimumCoverError::MemoryCapacityExceeded { .. }
                | ExactMinimumCoverError::ProjectionOverflow
                | ExactMinimumCoverError::AllocationFailed { .. }
        )
    ) {
        "E_WASM_MINIMUM_PARALLEL_RESOURCE"
    } else {
        "E_WASM_MINIMUM_PARALLEL_STATE"
    };
    WasmCommandRuntimeError::new(code, format!("minimum remote shard rejected: {error:?}"))
}

impl MinimumRemoteMemory {
    fn acquire(
        cap: Option<u128>,
        outer: u128,
        physical: u128,
    ) -> Result<Self, WasmCommandRuntimeError> {
        let authority =
            clearra_core_executor::WasmCpuTerminalResourceAuthority::try_acquire_full_capacity()
                .map_err(|_| {
                    remote_memory_error("minimum remote resource admission unavailable")
                })?;
        let limit = cap.map_or(authority.memory_capacity_bytes(), |cap| {
            cap.min(authority.memory_capacity_bytes())
        });
        let baseline = outer
            .checked_add(core::mem::size_of::<WasmMinimumParallelWorker>() as u128)
            .ok_or_else(|| remote_memory_error("minimum remote owner overflow"))?
            .max(physical);
        if baseline >= limit {
            return Err(remote_memory_error(
                "minimum remote baseline exceeds its admitted slice",
            ));
        }
        Ok(Self {
            _authority: authority,
            limit,
            baseline,
            outer_baseline: outer,
        })
    }

    fn ensure(
        &self,
        outer: u128,
        additional: u128,
    ) -> Result<(), clearra_coverage::cover::ExactMinimumCoverError> {
        use clearra_coverage::cover::ExactMinimumCoverError;
        let required = self
            .baseline
            .checked_add(outer.saturating_sub(self.outer_baseline))
            .and_then(|bytes| bytes.checked_add(additional))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        #[cfg(target_arch = "wasm32")]
        let required = required.max(
            (core::arch::wasm32::memory_size(0) as u128)
                .checked_mul(65_536)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        );
        if required > self.limit {
            return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes: required,
                max_memory_bytes: self.limit,
            });
        }
        Ok(())
    }

    fn ensure_wire(&self, outer: u128, additional: u128) -> Result<(), WasmCommandRuntimeError> {
        self.ensure(outer, additional)
            .map_err(|error| remote_memory_error(format!("minimum remote memory: {error:?}")))
    }
}

pub struct WasmMinimumParallelWorker {
    query: ExactAtMostQuery,
    shard: Option<ExactAtMostShardSession>,
    memory: MinimumRemoteMemory,
}

impl WasmMinimumParallelWorker {
    /// Admit an ABI buffer replacement while the old capacity is still live.
    /// The caller includes both old and prospective new carrier capacities in
    /// `outer_bytes`; this method adds the unchanged complete worker owner.
    pub fn ensure_outer_capacity(&self, outer_bytes: u128) -> Result<(), WasmCommandRuntimeError> {
        let retained = match &self.shard {
            Some(shard) => shard.checked_retained_bytes(),
            None => self.query.checked_retained_bytes(),
        }
        .ok_or_else(|| remote_memory_error("minimum remote owner projection overflow"))?;
        self.memory.ensure_wire(outer_bytes, retained)
    }

    pub fn has_active_shard(&self) -> bool {
        self.shard.is_some()
    }

    pub fn initialize(query: &[u8]) -> Result<Self, WasmCommandRuntimeError> {
        Self::initialize_guarded(query, query.len() as u128, 0)
    }

    /// `outer_bytes` includes the detached packet capacity and all ABI/runtime
    /// owners, but excludes this worker's query/shard. Native callers provide
    /// an explicit owner projection; WASM also supplies its physical floor.
    pub fn initialize_guarded(
        query: &[u8],
        outer_bytes: u128,
        physical_floor: u128,
    ) -> Result<Self, WasmCommandRuntimeError> {
        let cap = minimum_parallel_wire::query_memory_cap(query)?;
        let memory = MinimumRemoteMemory::acquire(cap, outer_bytes, physical_floor)?;
        let query = minimum_parallel_wire::decode_query_with_memory_guard(query, &mut |bytes| {
            memory.ensure_wire(outer_bytes, bytes)
        })?;
        memory.ensure_wire(
            outer_bytes,
            query
                .checked_retained_bytes()
                .ok_or_else(|| remote_memory_error("minimum remote query owner overflow"))?,
        )?;
        Ok(Self {
            query,
            shard: None,
            memory,
        })
    }

    pub fn start(&mut self, task: &[u8]) -> Result<(), WasmCommandRuntimeError> {
        self.start_guarded(task, task.len() as u128)
    }

    pub fn start_guarded(
        &mut self,
        task: &[u8],
        outer_bytes: u128,
    ) -> Result<(), WasmCommandRuntimeError> {
        if self.shard.is_some() {
            return Err(WasmCommandRuntimeError::new(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum worker already has an active shard",
            ));
        }
        let query_bytes = self
            .query
            .checked_retained_bytes()
            .ok_or_else(|| remote_memory_error("minimum remote query owner overflow"))?;
        let memory = &self.memory;
        let task = minimum_parallel_wire::decode_task_with_memory_guard(task, &mut |task_bytes| {
            memory.ensure_wire(
                outer_bytes,
                query_bytes
                    .checked_add(task_bytes)
                    .ok_or_else(|| remote_memory_error("minimum remote task owner overflow"))?,
            )
        })?;
        let shard = ExactAtMostShardSession::prepare(
            self.query.clone(),
            task,
            &mut |bytes| memory.ensure(outer_bytes, bytes),
            &mut || false,
        )
        .map_err(remote_shard_error)?;
        memory.ensure_wire(
            outer_bytes,
            shard
                .checked_retained_bytes()
                .ok_or_else(|| remote_memory_error("minimum remote shard owner overflow"))?,
        )?;
        self.shard = Some(shard);
        Ok(())
    }

    /// Pending owns no output; terminal returns only a core-minted receipt.
    pub fn advance(
        &mut self,
        maximum_work: usize,
    ) -> Result<Option<Vec<u8>>, WasmCommandRuntimeError> {
        self.advance_guarded(maximum_work, 0)
    }

    pub fn advance_guarded(
        &mut self,
        maximum_work: usize,
        outer_bytes: u128,
    ) -> Result<Option<Vec<u8>>, WasmCommandRuntimeError> {
        self.advance_controlled(maximum_work, false, outer_bytes)
    }

    pub fn cancel(&mut self) -> Result<Vec<u8>, WasmCommandRuntimeError> {
        self.cancel_guarded(0)
    }

    pub fn cancel_guarded(
        &mut self,
        outer_bytes: u128,
    ) -> Result<Vec<u8>, WasmCommandRuntimeError> {
        self.advance_controlled(0, true, outer_bytes)?
            .ok_or_else(|| {
                WasmCommandRuntimeError::new(
                    "E_WASM_MINIMUM_PARALLEL_STATE",
                    "cancelled shard did not return its terminal receipt",
                )
            })
    }

    fn advance_controlled(
        &mut self,
        maximum_work: usize,
        cancelled: bool,
        outer_bytes: u128,
    ) -> Result<Option<Vec<u8>>, WasmCommandRuntimeError> {
        let shard = self.shard.as_mut().ok_or_else(|| {
            WasmCommandRuntimeError::new(
                "E_WASM_MINIMUM_PARALLEL_STATE",
                "minimum worker has no active shard",
            )
        })?;
        let memory = &self.memory;
        memory.ensure_wire(
            outer_bytes,
            shard
                .checked_retained_bytes()
                .ok_or_else(|| remote_memory_error("minimum remote shard owner overflow"))?,
        )?;
        match shard
            .advance(
                maximum_work as u64,
                &mut |bytes| memory.ensure(outer_bytes, bytes),
                &mut || cancelled,
            )
            .map_err(remote_shard_error)?
        {
            ExactAtMostShardAdvance::Pending { .. } => Ok(None),
            ExactAtMostShardAdvance::Terminal(receipt) => {
                self.shard = None;
                // The shard is dropped before allocating transport output.
                // The immutable query, moved receipt and encoded Vec coexist.
                let retained = self
                    .query
                    .checked_retained_bytes()
                    .and_then(|bytes| {
                        bytes.checked_add(core::mem::size_of::<
                            clearra_coverage::cover::ExactAtMostReceipt,
                        >() as u128)
                    })
                    .and_then(|bytes| bytes.checked_add(receipt.task().checked_retained_bytes()?))
                    .and_then(|bytes| {
                        bytes.checked_add(match receipt.outcome() {
                            clearra_coverage::cover::ExactAtMostShardOutcome::Found(rows) => {
                                (rows.capacity() as u128)
                                    .checked_mul(core::mem::size_of::<usize>() as u128)?
                            }
                            _ => 0,
                        })
                    })
                    .ok_or_else(|| remote_memory_error("minimum remote receipt owner overflow"))?;
                minimum_parallel_wire::encode_receipt_with_memory_guard(&receipt, &mut |bytes| {
                    memory.ensure_wire(
                        outer_bytes,
                        retained.checked_add(bytes).ok_or_else(|| {
                            remote_memory_error("minimum remote receipt owner overflow")
                        })?,
                    )
                })
                .map(Some)
            }
        }
    }
}
