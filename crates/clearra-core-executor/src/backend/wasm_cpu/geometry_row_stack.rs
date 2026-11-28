//! Local A/B adapter for the row grammar. The ordinary candidate verifier,
//! supply owner, worker pool and result reducer remain the product owners.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, OnceLock,
};

use super::super::row_stack_geometry::{
    RowStackGeometryAdvance, RowStackGeometrySearch, RowStackTarget,
};
use super::{
    GeometryAdvance, GeometryCandidate, GeometryCatalog, GeometrySearch, PhysicalPrefixSummary,
    WasmExactSearchError,
};
#[cfg(feature = "parallel")]
use super::{ParallelGeometryPlan, ParallelGeometryPreparation};

pub(super) struct RowStackOwner {
    pub(super) search: RowStackGeometrySearch,
}

static DISPATCH_SEARCHES: AtomicU64 = AtomicU64::new(0);
static DISPATCH_PRIVATE_BYTES: AtomicU64 = AtomicU64::new(0);
static COMPLETED_SEARCHES: AtomicU64 = AtomicU64::new(0);
static EXPANDED_NODES: AtomicU64 = AtomicU64::new(0);
static EMITTED_CANDIDATES: AtomicU64 = AtomicU64::new(0);

/// One completed benchmark process. Private bytes describe the dispatch
/// checkpoint, not an RSS peak or catalog/verifier storage.
#[derive(Clone, Copy, Debug, Default)]
pub struct RowStackExperimentTotals {
    pub dispatch_searches: u64,
    pub dispatch_private_bytes: u64,
    pub completed_searches: u64,
    pub expanded_nodes: u64,
    pub emitted_candidates: u64,
}

pub fn row_stack_experiment_totals() -> RowStackExperimentTotals {
    RowStackExperimentTotals {
        dispatch_searches: DISPATCH_SEARCHES.load(Ordering::Relaxed),
        dispatch_private_bytes: DISPATCH_PRIVATE_BYTES.load(Ordering::Relaxed),
        completed_searches: COMPLETED_SEARCHES.load(Ordering::Relaxed),
        expanded_nodes: EXPANDED_NODES.load(Ordering::Relaxed),
        emitted_candidates: EMITTED_CANDIDATES.load(Ordering::Relaxed),
    }
}

fn requested() -> bool {
    static REQUESTED: OnceLock<bool> = OnceLock::new();
    *REQUESTED.get_or_init(|| std::env::var("CLEARRA_ROW_STACK_GEOMETRY").as_deref() == Ok("1"))
}

fn targets_for_search(
    targets: &[super::TargetGroup],
) -> Result<Vec<RowStackTarget>, WasmExactSearchError> {
    let mut result = Vec::new();
    result.try_reserve_exact(targets.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("row_stack_target_storage_unavailable")
    })?;
    for (index, target) in targets.iter().enumerate() {
        result.push(RowStackTarget {
            counts: target.key.counts(),
            target_index: u32::try_from(index).map_err(|_| {
                WasmExactSearchError::InvalidProblem("row_stack_target_index_overflow")
            })?,
        });
    }
    Ok(result)
}

impl GeometrySearch {
    /// The caller qualifies the physical PC objective and uncapped execution.
    /// Raw tilings, ordinal partitions and capped routes retain their engine.
    pub(in crate::backend::wasm_cpu) fn enable_row_stack_experiment(&mut self, qualified: bool) {
        self.row_stack_enabled = qualified && requested();
    }

    pub(super) fn advance_row_stack(
        &mut self,
        catalog: &GeometryCatalog,
        retained_limit_bytes: Option<u128>,
    ) -> Option<GeometryAdvance> {
        if self.row_stack.is_none() {
            if !self.row_stack_enabled {
                return None;
            }
            let compiler = self.compiler.as_ref()?;
            // This adapter changes only an untouched internal generator. An
            // externally partitioned family already has an ordinal contract.
            if compiler.expanded_nodes != 0 {
                return None;
            }
            let targets = Arc::clone(&compiler.targets);
            let row_targets = match targets_for_search(&targets) {
                Ok(targets) => targets,
                Err(error) => return Some(GeometryAdvance::ResourceIncomplete(error.reason())),
            };
            let needed = RowStackGeometrySearch::checked_retained_upper_bound(
                row_targets.len(),
                catalog.required_cells().count_ones() as usize / 4,
                usize::from(catalog.height()),
            );
            if retained_limit_bytes.is_some_and(|limit| needed.is_none_or(|bytes| bytes > limit)) {
                return Some(GeometryAdvance::ResourceIncomplete(
                    "row_stack_geometry_memory_capacity_exceeded",
                ));
            }
            let search = match RowStackGeometrySearch::new(catalog, &row_targets) {
                Ok(search) => search,
                Err(error) => return Some(GeometryAdvance::ResourceIncomplete(error.reason())),
            };
            self.compiler = None;
            self.external_targets = Some(targets);
            self.prefix = PhysicalPrefixSummary::default();
            self.candidate_family_count = None;
            self.row_stack = Some(RowStackOwner { search });
            // Yield at the ownership transition, before producing any leaf.
            // Parallel preparation uses this boundary to partition row roots.
            return Some(GeometryAdvance::Pending);
        }

        let owner = self.row_stack.as_mut()?;
        if retained_limit_bytes.is_some_and(|limit| owner.search.retained_bytes() as u128 > limit) {
            return Some(GeometryAdvance::ResourceIncomplete(
                "row_stack_geometry_memory_capacity_exceeded",
            ));
        }
        let advance = owner.search.advance(catalog, 64, None, None);
        self.expanded_nodes = owner.search.expanded_nodes();
        self.peak_frontier = self
            .peak_frontier
            .max(usize::from(catalog.height()).saturating_add(1));
        Some(match advance {
            RowStackGeometryAdvance::Pending => GeometryAdvance::Pending,
            RowStackGeometryAdvance::Candidate(candidate) => {
                let Some(candidate) = GeometryCandidate::from_rows(
                    catalog,
                    candidate.target_index,
                    candidate.row_ids(),
                ) else {
                    return Some(GeometryAdvance::ResourceIncomplete(
                        "row_stack_candidate_identity_invalid",
                    ));
                };
                GeometryAdvance::Candidate(candidate)
            }
            RowStackGeometryAdvance::Complete => {
                COMPLETED_SEARCHES.fetch_add(1, Ordering::Relaxed);
                EXPANDED_NODES.fetch_add(owner.search.expanded_nodes() as u64, Ordering::Relaxed);
                EMITTED_CANDIDATES
                    .fetch_add(owner.search.emitted_candidates() as u64, Ordering::Relaxed);
                self.candidate_family_count = Some(owner.search.emitted_candidates() as u128);
                self.row_stack = None;
                GeometryAdvance::Complete
            }
            RowStackGeometryAdvance::Cancelled => {
                GeometryAdvance::ResourceIncomplete("row_stack_cancelled")
            }
            RowStackGeometryAdvance::NodeBudgetExceeded => {
                GeometryAdvance::ResourceIncomplete("row_stack_node_budget_exceeded")
            }
        })
    }

    #[cfg(feature = "parallel")]
    pub(super) fn prepare_row_stack_parallel(
        self,
        catalog: &GeometryCatalog,
        requested_workers: usize,
    ) -> Result<ParallelGeometryPreparation, WasmExactSearchError> {
        if requested_workers < 2 {
            return Ok(ParallelGeometryPreparation::Serial {
                geometry: self,
                reason: "row-stack-single-worker",
            });
        }
        if self
            .row_stack
            .as_ref()
            .is_some_and(|owner| owner.search.expanded_nodes() != 0)
        {
            return Ok(ParallelGeometryPreparation::Serial {
                geometry: self,
                reason: "row-stack-already-started",
            });
        }
        let targets =
            self.external_targets
                .as_ref()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "row_stack_shared_targets_missing",
                ))?;
        let row_targets = targets_for_search(targets)?;
        // More root partitions than workers lets the existing worker queue
        // absorb uneven subtrees. Ownership is decided after at most two
        // first-row piece starts, before the remaining row/permutation work.
        let partition_count =
            requested_workers
                .checked_mul(4)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "row_stack_partition_count_overflow",
                ))?;
        let mut searches = Vec::new();
        searches.try_reserve_exact(partition_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("row_stack_partition_storage_unavailable")
        })?;
        for index in 0..partition_count {
            let mut search = RowStackGeometrySearch::new(catalog, &row_targets)?;
            search
                .set_root_partition(index, partition_count)
                .map_err(WasmExactSearchError::InvalidProblem)?;
            let mut geometry = Self::placeholder();
            geometry.external_targets = Some(Arc::clone(targets));
            geometry.candidate_family_count = None;
            geometry.row_stack = Some(RowStackOwner { search });
            geometry.row_stack_enabled = true;
            searches.push(geometry);
        }
        DISPATCH_SEARCHES.fetch_add(searches.len() as u64, Ordering::Relaxed);
        DISPATCH_PRIVATE_BYTES.fetch_add(
            searches
                .iter()
                .filter_map(|geometry| geometry.row_stack.as_ref())
                .map(|owner| owner.search.retained_bytes() as u64)
                .sum::<u64>(),
            Ordering::Relaxed,
        );
        Ok(ParallelGeometryPreparation::Ready {
            plan: ParallelGeometryPlan {
                targets: Arc::clone(targets),
                searches,
                group_pattern_index_bytes: self.group_pattern_index_bytes,
                shared_family_bytes: 0,
            },
            balanced_workers: requested_workers,
        })
    }
}
