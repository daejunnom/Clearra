//! Exact, contiguous splits of one unconsumed four-word Geometry family.
//! Rows and Product continuations are private; the DAG and target indexes have
//! one immutable owner. No concrete placement family is copied per worker.
use crate::resource::ExecutionMemoryBound;

use super::{
    Arc, ExtendedFamilyEnumerator, ExtendedGeometryCandidate, ExtendedGeometrySearch,
    ExtendedInverseCatalog, FamilyNodeKind, GeometrySolutionFamily, GeometryTarget, TraversalTask,
    WasmExactSearchError, FAMILY_EMPTY, FAMILY_INVALID, MAX_EXTENDED_PIECES,
};

#[derive(Clone, Copy)]
struct Seed {
    task: TraversalTask,
    rows: [u32; MAX_EXTENDED_PIECES],
    weight: u128,
    splittable: bool,
}

pub(in crate::backend::wasm_cpu) struct ExtendedParallelGeometryBranch {
    enumerator: ExtendedFamilyEnumerator,
    pub(in crate::backend::wasm_cpu) first_ordinal: u128,
    pub(in crate::backend::wasm_cpu) candidate_count: u128,
}

impl ExtendedParallelGeometryBranch {
    pub(in crate::backend::wasm_cpu) fn next_candidate(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        memory_bound: ExecutionMemoryBound,
        coexisting_private_bytes: u128,
    ) -> Result<Option<ExtendedGeometryCandidate>, WasmExactSearchError> {
        memory_bound
            .ensure(
                coexisting_private_bytes,
                self.private_retained_bytes() as u128,
            )
            .map_err(WasmExactSearchError::resource_admission)?;
        self.enumerator.task_memory_limit =
            Some(memory_bound.cap_bytes() - coexisting_private_bytes);
        match self.enumerator.next_candidate(catalog) {
            Ok(candidate) => Ok(candidate),
            Err(()) => {
                if let Some(bytes) = self.enumerator.refused_task_bytes {
                    return Err(WasmExactSearchError::resource_admission(
                        memory_bound
                            .ensure(coexisting_private_bytes, bytes)
                            .expect_err(
                                "traversal refused an allocation beyond its private credit",
                            ),
                    ));
                }
                Err(WasmExactSearchError::InvalidProblem(
                    "extended_parallel_geometry_traversal_failed",
                ))
            }
        }
    }

    pub(in crate::backend::wasm_cpu) fn private_retained_bytes(&self) -> usize {
        self.enumerator.tasks.capacity() * core::mem::size_of::<TraversalTask>()
    }
}

pub(in crate::backend::wasm_cpu) struct ExtendedParallelGeometryPlan {
    pub(in crate::backend::wasm_cpu) branches: Vec<ExtendedParallelGeometryBranch>,
    targets: Arc<[GeometryTarget]>,
    family: Arc<GeometrySolutionFamily>,
    pattern_index_bytes: usize,
    pub(in crate::backend::wasm_cpu) candidate_count: u128,
}

impl ExtendedParallelGeometryPlan {
    pub(in crate::backend::wasm_cpu) fn shared_retained_bytes(&self) -> usize {
        // Explicit owner accounting must not disappear when Arc::strong_count
        // changes while branches move between the queue and live workers.
        self.pattern_index_bytes
            + core::mem::size_of_val(self.targets.as_ref())
            + core::mem::size_of::<GeometrySolutionFamily>()
            + self.family.retained_bytes()
            + 64 // Arc control blocks, including target allocation metadata.
    }

    pub(in crate::backend::wasm_cpu) fn branch_retained_bytes(&self) -> usize {
        self.branches.capacity() * core::mem::size_of::<ExtendedParallelGeometryBranch>()
            + self
                .branches
                .iter()
                .map(ExtendedParallelGeometryBranch::private_retained_bytes)
                .sum::<usize>()
    }
}

impl ExtendedGeometrySearch {
    pub(in crate::backend::wasm_cpu) fn take_parallel_plan(
        &mut self,
        desired_partitions: usize,
        memory_bound: ExecutionMemoryBound,
        checked_live_retained_bytes: u128,
    ) -> Result<Option<ExtendedParallelGeometryPlan>, WasmExactSearchError> {
        if self.is_compiling() || self.candidate_count != 0 || self.external_targets.is_some() {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_parallel_geometry_requires_unconsumed_family",
            ));
        }
        let Some(enumerator) = self.enumerator.as_ref() else {
            return Ok(None);
        };
        if self.candidate_family_count == Some(0) {
            return Ok(None);
        }
        if enumerator.tasks.len() != 1 {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_parallel_geometry_root_missing",
            ));
        }
        let desired = desired_partitions.max(1).min(
            usize::try_from(self.candidate_family_count.ok_or(
                WasmExactSearchError::InvalidProblem(
                    "extended_parallel_geometry_count_unavailable",
                ),
            )?)
            .unwrap_or(usize::MAX),
        );
        let future = (enumerator.family.node_count() as u128 + 2)
            .checked_mul(core::mem::size_of::<u128>() as u128)
            .and_then(|bytes| {
                bytes.checked_add(
                    (desired as u128)
                        * (core::mem::size_of::<Seed>()
                            + core::mem::size_of::<ExtendedParallelGeometryBranch>()
                            + core::mem::size_of::<TraversalTask>())
                            as u128,
                )
            })
            .and_then(|bytes| bytes.checked_add(64))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_parallel_geometry_memory_projection_overflow",
            ))?;
        memory_bound
            .ensure(checked_live_retained_bytes, future)
            .map_err(WasmExactSearchError::resource_admission)?;

        let counts =
            enumerator
                .family
                .path_count_table()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "extended_parallel_geometry_count_unavailable",
                ))?;
        let mut seeds = Vec::new();
        seeds
            .try_reserve_exact(desired)
            .map_err(|_| storage_error())?;
        let task = enumerator.tasks[0];
        seeds.push(Seed {
            task,
            rows: enumerator.rows,
            weight: traversal_weight(task, &counts)?,
            splittable: true,
        });
        while seeds.len() < desired {
            let Some(index) = seeds
                .iter()
                .enumerate()
                .filter(|(_, seed)| seed.splittable)
                .max_by_key(|(_, seed)| seed.weight)
                .map(|(index, _)| index)
            else {
                break;
            };
            match split_seed(&enumerator.family, &counts, seeds[index])? {
                Some((left, right)) => {
                    seeds[index] = left;
                    // Keep serial DFS order even when the heaviest suffix is
                    // split first. This makes each ordinal interval disjoint.
                    seeds.insert(index + 1, right);
                }
                None => seeds[index].splittable = false,
            }
        }
        let mut branches = Vec::new();
        branches
            .try_reserve_exact(seeds.len())
            .map_err(|_| storage_error())?;
        let mut ordinal = 0_u128;
        for seed in seeds {
            let mut tasks = Vec::new();
            tasks.try_reserve_exact(1).map_err(|_| storage_error())?;
            tasks.push(seed.task);
            branches.push(ExtendedParallelGeometryBranch {
                enumerator: ExtendedFamilyEnumerator {
                    targets: Arc::clone(&enumerator.targets),
                    family: Arc::clone(&enumerator.family),
                    tasks,
                    rows: seed.rows,
                    target_depth: enumerator.target_depth,
                    task_memory_limit: None,
                    refused_task_bytes: None,
                },
                first_ordinal: ordinal,
                candidate_count: seed.weight,
            });
            ordinal =
                ordinal
                    .checked_add(seed.weight)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_parallel_geometry_count_overflow",
                    ))?;
        }
        if Some(ordinal) != self.candidate_family_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_parallel_geometry_split_count_mismatch",
            ));
        }
        let plan = ExtendedParallelGeometryPlan {
            branches,
            targets: Arc::clone(&enumerator.targets),
            family: Arc::clone(&enumerator.family),
            pattern_index_bytes: self.pattern_index_bytes,
            candidate_count: ordinal,
        };
        self.enumerator = None;
        self.pattern_index_bytes = 0;
        Ok(Some(plan))
    }
}

fn split_seed(
    family: &GeometrySolutionFamily,
    counts: &[u128],
    mut seed: Seed,
) -> Result<Option<(Seed, Seed)>, WasmExactSearchError> {
    loop {
        match seed.task.family {
            FAMILY_INVALID => return Ok(None),
            FAMILY_EMPTY => {
                if seed.task.continuation_count == 0 {
                    return Ok(None);
                }
                seed.task.continuation_count -= 1;
                seed.task.family =
                    seed.task.continuations[usize::from(seed.task.continuation_count)];
            }
            reference => {
                let node = family.node(reference).ok_or_else(traversal_error)?;
                match node.kind {
                    FamilyNodeKind::Append => {
                        let row = seed
                            .rows
                            .get_mut(usize::from(seed.task.depth))
                            .ok_or_else(traversal_error)?;
                        *row = node.row_id;
                        seed.task.depth += 1;
                        seed.task.family = node.left;
                    }
                    FamilyNodeKind::Union => {
                        let mut left = seed;
                        let mut right = seed;
                        left.task.family = node.left;
                        right.task.family = node.right;
                        left.weight = traversal_weight(left.task, counts)?;
                        right.weight = traversal_weight(right.task, counts)?;
                        return Ok(Some((left, right)));
                    }
                    FamilyNodeKind::Product => {
                        let continuation = seed
                            .task
                            .continuations
                            .get_mut(usize::from(seed.task.continuation_count))
                            .ok_or_else(traversal_error)?;
                        *continuation = node.right;
                        seed.task.continuation_count += 1;
                        seed.task.family = node.left;
                    }
                }
            }
        }
    }
}

fn traversal_weight(task: TraversalTask, counts: &[u128]) -> Result<u128, WasmExactSearchError> {
    let mut weight = *counts
        .get(task.family as usize)
        .ok_or_else(traversal_error)?;
    for continuation in task.continuations[..usize::from(task.continuation_count)].iter() {
        weight = weight
            .checked_mul(
                *counts
                    .get(*continuation as usize)
                    .ok_or_else(traversal_error)?,
            )
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_parallel_geometry_count_overflow",
            ))?;
    }
    Ok(weight)
}

fn storage_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_parallel_geometry_storage_unavailable")
}

fn traversal_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_parallel_geometry_family_invalid")
}

#[cfg(test)]
#[path = "extended_geometry_parallel_tests.rs"]
mod tests;
