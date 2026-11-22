//! Two independent read-only passes over one frozen family. This changes only
//! preparation scheduling; neither table is a positive build/source proof.

use std::io;
use std::sync::Arc;
use std::thread;

use clearra_core_domain::execution_cancellation::ExecutionControl;

use crate::cpu_worker_pool;
use crate::performance::ExecutorSearchStage;
use crate::performance::SearchStageSpan;
use crate::performance::WorkerSearchProfile;
use crate::resource::try_acquire_scoped_thread_memory;
use crate::resource::SCOPED_THREAD_STACK_BYTES;

use super::row_order::FamilyRowOrderBuilder;
use super::row_order::FamilyRowOrderTable;
use super::row_order::RowOrderBuildAdvance;
use super::FamilyCompiler;
use super::FamilyEnumerator;
use super::GeometryCatalog;
use super::GeometrySolutionFamily;
use super::TargetGroup;
use super::WasmExactSearchError;
use super::FAMILY_EMPTY;
use super::ROW_ORDER_PREPARATION_WORK_QUANTUM;

const MIN_PARALLEL_FAMILY_NODES: u32 = 262_144;

pub(super) struct PreparedFamily {
    pub(super) enumerator: FamilyEnumerator,
    pub(super) path_counts: Option<Vec<u128>>,
    pub(super) candidate_count: Option<u128>,
}

struct PreparedFamilyTables {
    path_counts: Option<Vec<u128>>,
    row_order: Option<FamilyRowOrderTable>,
}

struct FrozenFamily {
    targets: Arc<[TargetGroup]>,
    family: Arc<GeometrySolutionFamily>,
    root: u32,
    depth: u8,
}

fn freeze_compiler(mut compiler: FamilyCompiler) -> FrozenFamily {
    // This consumed helper returns before either table starts. Besides the
    // interner, all residual/domain/projection/compiler scratch is dropped
    // here, so overlapping the tables cannot retain that scratch longer.
    let stage = SearchStageSpan::begin(ExecutorSearchStage::WasmGeometryConstructionIndexRelease);
    let released = compiler.family.release_construction_index();
    stage.finish(released as u64);
    FrozenFamily {
        targets: compiler.targets,
        family: Arc::new(compiler.family),
        root: compiler.root_family,
        depth: compiler.target_depth,
    }
}

#[derive(Clone, Copy)]
enum PreparationDispatch {
    Scoped,
    #[cfg(test)]
    Sequential,
    #[cfg(test)]
    SpawnUnavailable,
    #[cfg(test)]
    MemoryUnavailable,
    #[cfg(test)]
    ScopedWithIsolatedMemory,
}

fn overlap_is_worthwhile(nodes: u32, workers: usize, retained_limit: Option<u128>) -> bool {
    workers > 1 && nodes >= MIN_PARALLEL_FAMILY_NODES && retained_limit.is_none()
}

impl FamilyCompiler {
    pub(super) fn can_overlap_preparation(&self, requested_workers: usize) -> bool {
        !self.resource_authoritative
            && overlap_is_worthwhile(
                self.family.node_count(),
                requested_workers,
                self.family.retained_limit_bytes(),
            )
    }

    pub(super) fn into_overlapped_enumerator(
        self,
        catalog: &GeometryCatalog,
        control: &ExecutionControl,
    ) -> Result<PreparedFamily, WasmExactSearchError> {
        let frozen = freeze_compiler(self);
        let tables = prepare_tables(
            &frozen.family,
            catalog,
            control,
            PreparationDispatch::Scoped,
        )?;
        let candidate_count = tables
            .path_counts
            .as_ref()
            .and_then(|counts| counts.get(frozen.root as usize))
            .copied();
        let mut enumerator = FamilyEnumerator::from_shared_completed_family(
            frozen.targets,
            frozen.family,
            frozen.root,
            frozen.depth,
            candidate_count,
        );
        // None is an attempted optional-table fallback, not unfinished work.
        // The caller enables the original prefix policy on this same owner.
        enumerator.row_order_attempted = true;
        enumerator.row_order_table = tables.row_order.map(Arc::new);
        Ok(PreparedFamily {
            enumerator,
            path_counts: tables.path_counts,
            candidate_count,
        })
    }
}

fn prepare_tables(
    family: &Arc<GeometrySolutionFamily>,
    catalog: &GeometryCatalog,
    control: &ExecutionControl,
    dispatch: PreparationDispatch,
) -> Result<PreparedFamilyTables, WasmExactSearchError> {
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    #[cfg(test)]
    if matches!(dispatch, PreparationDispatch::Sequential) {
        return Ok(PreparedFamilyTables {
            path_counts: prepare_path_counts(family, control)?,
            row_order: prepare_row_order(family, catalog, control)?,
        });
    }
    #[cfg(test)]
    let deny_spawn = matches!(dispatch, PreparationDispatch::SpawnUnavailable);
    #[cfg(not(test))]
    let deny_spawn = false;
    #[cfg(not(test))]
    let _ = dispatch;
    // Existing request compute width already covers caller + one worker. Only
    // the new transient stack/metadata needs another global memory lease.
    let memory_lease = if deny_spawn {
        None // Test-only refused spawn creates no thread or stack.
    } else {
        #[cfg(test)]
        let memory_lease = match dispatch {
            PreparationDispatch::ScopedWithIsolatedMemory => tests::isolated_thread_memory(),
            PreparationDispatch::MemoryUnavailable => None,
            _ => try_acquire_scoped_thread_memory(1),
        };
        #[cfg(not(test))]
        let memory_lease = try_acquire_scoped_thread_memory(1);
        let Some(memory_lease) = memory_lease else {
            return Ok(PreparedFamilyTables {
                path_counts: prepare_path_counts(family, control)?,
                row_order: prepare_row_order(family, catalog, control)?,
            });
        };
        Some(memory_lease)
    };
    let result = thread::scope(|scope| {
        let profile = WorkerSearchProfile::capture();
        let spawned = if deny_spawn {
            Err(io::Error::other("test-only preparation spawn refusal"))
        } else {
            thread::Builder::new()
                .name("clearra-family-row-order".to_owned())
                .stack_size(SCOPED_THREAD_STACK_BYTES)
                .spawn_scoped(scope, move || {
                    cpu_worker_pool::apply_worker_locality(1);
                    profile.measure(|| prepare_row_order(family, catalog, control))
                })
        };
        // This scope has at most one background thread plus its caller. Both
        // borrow the same Arc without cloning the family or either result Vec.
        let path_counts = prepare_path_counts(family, control);
        let row_order = match spawned {
            Ok(worker) => worker.join().map_err(|_| {
                WasmExactSearchError::InvalidProblem("geometry_family_preparation_worker_panicked")
            })?,
            Err(_) => prepare_row_order(family, catalog, control),
        };
        // Join before propagating cancellation/error so no preparation task can
        // outlive its borrowed owner. Cancellation is never an optional None.
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        Ok(PreparedFamilyTables {
            path_counts: path_counts?,
            row_order: row_order?,
        })
    });
    // Retain the lease through successful work, cancellation and spawn failure;
    // every scoped thread has joined before its stack credit is returned.
    drop(memory_lease);
    result
}

fn prepare_path_counts(
    family: &GeometrySolutionFamily,
    control: &ExecutionControl,
) -> Result<Option<Vec<u128>>, WasmExactSearchError> {
    let stage = SearchStageSpan::begin(ExecutorSearchStage::WasmGeometryPathCount);
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    let Some(entries) = (family.node_count() as usize).checked_add(2) else {
        return Ok(None);
    };
    let mut counts = Vec::new();
    if counts.try_reserve_exact(entries).is_err() {
        return Ok(None);
    }
    counts.resize(entries, 0_u128);
    counts[FAMILY_EMPTY as usize] = 1;
    for reference in 2..family.node_count() + 2 {
        if reference & 4095 == 0 && control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let Some(count) = family.path_count_for_reference(reference, &counts) else {
            // Preserve whole-table overflow/unavailability even when the chosen
            // root itself could be counted. No partial table becomes authority.
            return Ok(None);
        };
        counts[reference as usize] = count;
    }
    stage.finish(u64::from(family.node_count()));
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    Ok(Some(counts))
}

fn prepare_row_order(
    family: &Arc<GeometrySolutionFamily>,
    catalog: &GeometryCatalog,
    control: &ExecutionControl,
) -> Result<Option<FamilyRowOrderTable>, WasmExactSearchError> {
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    let stage = SearchStageSpan::begin(ExecutorSearchStage::WasmGeometryRowOrderPrepare);
    let builder = FamilyRowOrderBuilder::try_new(catalog, family, None);
    stage.finish(0);
    let Some(mut builder) = builder else {
        return Ok(None);
    };
    let mut steps = 0usize;
    loop {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let stage = SearchStageSpan::begin(ExecutorSearchStage::WasmGeometryRowOrderPrepare);
        let before = builder.completed_nodes();
        let advance = builder.advance(family, ROW_ORDER_PREPARATION_WORK_QUANTUM);
        stage.finish(builder.completed_nodes().saturating_sub(before) as u64);
        steps = steps.wrapping_add(1);
        if steps == 1 || steps & 63 == 0 || advance != RowOrderBuildAdvance::Pending {
            control.report_progress(
                "geometry-row-order",
                builder.completed_nodes() as u64,
                Some(u64::from(family.node_count())),
            );
        }
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        match advance {
            RowOrderBuildAdvance::Pending => {}
            RowOrderBuildAdvance::Complete => return Ok(builder.finish()),
            RowOrderBuildAdvance::Unavailable => return Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "geometry_family_preparation_tests.rs"]
mod tests;
