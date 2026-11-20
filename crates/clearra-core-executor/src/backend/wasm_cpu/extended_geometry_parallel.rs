//! Exact, contiguous splits of one unconsumed four-word Geometry family.
//! Rows and Product continuations are private; the DAG and target indexes have
//! one immutable owner. No concrete placement family is copied per worker.
use crate::resource::ExecutionMemoryBound;
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
use crate::resource::ExecutionMemoryOwner;

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
    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    pub(in crate::backend::wasm_cpu) fn next_candidate_with_owner(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        owner: &ExecutionMemoryOwner,
        coexisting_private_bytes: u128,
    ) -> Result<Option<ExtendedGeometryCandidate>, WasmExactSearchError> {
        let mut refused = None;
        let result = self
            .enumerator
            .next_candidate_with_allocation_guard(catalog, |bytes| {
                let total = coexisting_private_bytes.saturating_add(bytes);
                owner.ensure(total).map_err(|report| {
                    refused = Some(report);
                })
            });
        if let Some(report) = refused {
            return Err(WasmExactSearchError::resource_admission(report));
        }
        result.map_err(|_| {
            WasmExactSearchError::InvalidProblem("extended_parallel_geometry_traversal_failed")
        })
    }

    #[cfg(test)]
    pub(in crate::backend::wasm_cpu) fn next_candidate(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        memory_bound: ExecutionMemoryBound,
        coexisting_private_bytes: u128,
    ) -> Result<Option<ExtendedGeometryCandidate>, WasmExactSearchError> {
        self.next_candidate_bounded(catalog, memory_bound, coexisting_private_bytes)
    }

    /// Applies the realm's assigned credit before growing traversal backing.
    /// The immutable family and target owner stay shared within that realm.
    pub(in crate::backend::wasm_cpu) fn next_candidate_bounded(
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
            append_branch(enumerator, &mut branches, &mut ordinal, seed)?;
        }
        self.seal_parallel_plan(branches, ordinal).map(Some)
    }

    fn seal_parallel_plan(
        &mut self,
        branches: Vec<ExtendedParallelGeometryBranch>,
        ordinal: u128,
    ) -> Result<ExtendedParallelGeometryPlan, WasmExactSearchError> {
        if Some(ordinal) != self.candidate_family_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_parallel_geometry_split_count_mismatch",
            ));
        }
        let enumerator = self.enumerator.as_ref().ok_or_else(traversal_error)?;
        let plan = ExtendedParallelGeometryPlan {
            branches,
            targets: Arc::clone(&enumerator.targets),
            family: Arc::clone(&enumerator.family),
            pattern_index_bytes: self.pattern_index_bytes,
            candidate_count: ordinal,
        };
        self.enumerator = None;
        self.pattern_index_bytes = 0;
        Ok(plan)
    }
}

fn append_branch(
    enumerator: &ExtendedFamilyEnumerator,
    branches: &mut Vec<ExtendedParallelGeometryBranch>,
    ordinal: &mut u128,
    seed: Seed,
) -> Result<(), WasmExactSearchError> {
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
        first_ordinal: *ordinal,
        candidate_count: seed.weight,
    });
    *ordinal = ordinal
        .checked_add(seed.weight)
        .ok_or(WasmExactSearchError::InvalidProblem(
            "extended_parallel_geometry_count_overflow",
        ))?;
    Ok(())
}

fn split_seed(
    family: &GeometrySolutionFamily,
    counts: &[u128],
    mut seed: Seed,
) -> Result<Option<(Seed, Seed)>, WasmExactSearchError> {
    loop {
        match advance_split_seed(family, counts, &mut seed)? {
            SplitSeedAdvance::Pending => {}
            SplitSeedAdvance::Unsplittable => return Ok(None),
            SplitSeedAdvance::Split(left, right) => return Ok(Some((left, right))),
        }
    }
}

// Two seed summaries are temporary, bounded stack values (each has two arrays
// of exactly MAX_EXTENDED_PIECES entries).
#[allow(clippy::large_enum_variant)]
enum SplitSeedAdvance {
    Pending,
    Unsplittable,
    Split(Seed, Seed),
}

fn advance_split_seed(
    family: &GeometrySolutionFamily,
    counts: &[u128],
    seed: &mut Seed,
) -> Result<SplitSeedAdvance, WasmExactSearchError> {
    match seed.task.family {
        FAMILY_INVALID => return Ok(SplitSeedAdvance::Unsplittable),
        FAMILY_EMPTY => {
            if seed.task.continuation_count == 0 {
                return Ok(SplitSeedAdvance::Unsplittable);
            }
            seed.task.continuation_count -= 1;
            seed.task.family = seed.task.continuations[usize::from(seed.task.continuation_count)];
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
                    let mut left = *seed;
                    let mut right = *seed;
                    left.task.family = node.left;
                    right.task.family = node.right;
                    left.weight = traversal_weight(left.task, counts)?;
                    right.weight = traversal_weight(right.task, counts)?;
                    return Ok(SplitSeedAdvance::Split(left, right));
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
    Ok(SplitSeedAdvance::Pending)
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

// Native callers with a cooperative polling contract use this cursor.
// Native synchronous and distributed callers retain the direct preparation.
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
pub(super) mod cooperative {
    use super::*;
    use crate::backend::wasm_cpu::extended_geometry::ExtendedGeometryAdvance;
    use crate::resource::SharedExecutionNodeBudget;
    use clearra_core_domain::execution_cancellation::ExecutionControl;
    use core::mem::size_of;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::backend::wasm_cpu) enum PreparationStatus {
        Pending,
        Ready,
        Cancelled,
        ResourceIncomplete(&'static str),
    }

    #[allow(clippy::large_enum_variant)]
    enum Phase {
        Compile,
        Count,
        Select { next: usize, best: Option<usize> },
        Split { index: usize, seed: Seed },
        Emit { index: usize, ordinal: u128 },
        Done,
        Stopped,
    }

    pub(in crate::backend::wasm_cpu) struct Preparation {
        geometry: ExtendedGeometrySearch,
        catalog: Arc<ExtendedInverseCatalog>,
        desired: usize,
        phase: Phase,
        counts: Vec<u128>,
        seeds: Vec<Seed>,
        branches: Vec<ExtendedParallelGeometryBranch>,
        ready: Option<ExtendedParallelGeometryPlan>,
        // Once an emitted branch clones the Arcs, retain the original
        // Geometry charge rather than relying on its Arc strong-count check.
        frozen_geometry_bytes: u128,
        scratch_credit: u128,
    }

    impl Preparation {
        pub(in crate::backend::wasm_cpu) fn expanded_nodes(&self) -> usize {
            self.geometry.expanded_nodes()
        }

        pub(in crate::backend::wasm_cpu) fn new(
            geometry: ExtendedGeometrySearch,
            catalog: Arc<ExtendedInverseCatalog>,
            desired: usize,
        ) -> Result<Self, WasmExactSearchError> {
            if geometry.external_targets.is_some() || geometry.candidate_count != 0 {
                return Err(traversal_error());
            }
            Ok(Self {
                geometry,
                catalog,
                desired: desired.max(1),
                phase: Phase::Compile,
                counts: Vec::new(),
                seeds: Vec::new(),
                branches: Vec::new(),
                ready: None,
                frozen_geometry_bytes: 0,
                scratch_credit: 0,
            })
        }

        pub(in crate::backend::wasm_cpu) fn advance(
            &mut self,
            node_budget: Option<&SharedExecutionNodeBudget>,
            bound: ExecutionMemoryBound,
            outside: u128,
            budget: usize,
            control: &ExecutionControl,
        ) -> Result<(PreparationStatus, usize), WasmExactSearchError> {
            let outside = outside
                .checked_add(size_of::<Self>() as u128)
                .ok_or_else(preparation_projection_error)?;
            let mut used = 0;
            loop {
                if control.is_cancelled() {
                    self.phase = Phase::Stopped;
                    return Ok((PreparationStatus::Cancelled, used));
                }
                if !matches!(&self.phase, Phase::Compile | Phase::Stopped) {
                    if let Err(error) = self.check_scratch(bound, outside, 0) {
                        self.phase = Phase::Stopped;
                        return Err(error);
                    }
                }
                match &self.phase {
                    Phase::Done => return Ok((PreparationStatus::Ready, used)),
                    Phase::Stopped => return Err(traversal_error()),
                    _ => {}
                }
                if used == budget {
                    return Ok((PreparationStatus::Pending, used));
                }
                used += 1;
                if let Some(reason) = self.step(node_budget, bound, outside)? {
                    return Ok((PreparationStatus::ResourceIncomplete(reason), used));
                }
            }
        }

        pub(in crate::backend::wasm_cpu) fn into_result(
            self,
        ) -> Result<
            (ExtendedGeometrySearch, Option<ExtendedParallelGeometryPlan>),
            WasmExactSearchError,
        > {
            if !matches!(self.phase, Phase::Done) {
                return Err(traversal_error());
            }
            Ok((self.geometry, self.ready))
        }

        #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
        pub(in crate::backend::wasm_cpu) fn ready_retained_upper_bound(&self) -> Option<u128> {
            if !matches!(self.phase, Phase::Done) {
                return None;
            }
            self.frozen_geometry_bytes.checked_add(self.scratch_credit)
        }

        fn step(
            &mut self,
            node_budget: Option<&SharedExecutionNodeBudget>,
            bound: ExecutionMemoryBound,
            outside: u128,
        ) -> Result<Option<&'static str>, WasmExactSearchError> {
            let phase = core::mem::replace(&mut self.phase, Phase::Stopped);
            self.phase = match phase {
                Phase::Compile if self.geometry.is_compiling() => {
                    match self.geometry.advance_bounded_with_count_policy(
                        &self.catalog,
                        node_budget,
                        bound,
                        outside,
                        true,
                    )? {
                        ExtendedGeometryAdvance::Pending => Phase::Compile,
                        ExtendedGeometryAdvance::ResourceIncomplete(reason) => {
                            return Ok(Some(reason));
                        }
                        _ => return Err(traversal_error()),
                    }
                }
                Phase::Compile => {
                    let Some(enumerator) = self.geometry.enumerator.as_ref() else {
                        self.phase = Phase::Done;
                        return Ok(None);
                    };
                    if enumerator.tasks.is_empty() {
                        self.geometry.candidate_family_count = Some(0);
                        self.phase = Phase::Done;
                        return Ok(None);
                    }
                    if enumerator.tasks.len() != 1 {
                        return Err(traversal_error());
                    }
                    let entries = count_entries(&enumerator.family)?;
                    self.frozen_geometry_bytes = (self.geometry.retained_bytes() as u128)
                        .checked_add(if Arc::strong_count(&enumerator.family) == 1 {
                            0
                        } else {
                            enumerator.family.retained_bytes() as u128
                        })
                        .and_then(|bytes| {
                            bytes.checked_add(if Arc::strong_count(&enumerator.targets) == 1 {
                                0
                            } else {
                                core::mem::size_of_val(enumerator.targets.as_ref()) as u128
                            })
                        })
                        .ok_or_else(preparation_projection_error)?;
                    let count_bytes = (entries as u128) * size_of::<u128>() as u128;
                    self.check_scratch(bound, outside, count_bytes)?;
                    self.counts
                        .try_reserve_exact(entries)
                        .map_err(|_| storage_error())?;
                    self.counts.extend([0, 1]);
                    self.scratch_credit =
                        self.counts.capacity() as u128 * size_of::<u128>() as u128;
                    self.check_scratch(bound, outside, 0)?;
                    Phase::Count
                }
                Phase::Count => {
                    let enumerator = self
                        .geometry
                        .enumerator
                        .as_ref()
                        .ok_or_else(traversal_error)?;
                    if self.counts.len() < count_entries(&enumerator.family)? {
                        let reference = u32::try_from(self.counts.len())
                            .map_err(|_| preparation_projection_error())?;
                        let count = enumerator
                            .family
                            .path_count_for_reference(reference, &self.counts)
                            .ok_or_else(count_unavailable)?;
                        self.counts.push(count);
                        Phase::Count
                    } else {
                        let task = enumerator.tasks[0];
                        let rows = enumerator.rows;
                        let weight = traversal_weight(task, &self.counts)?;
                        if self
                            .geometry
                            .candidate_family_count
                            .is_some_and(|known| known != weight)
                        {
                            return Err(traversal_error());
                        }
                        self.geometry.candidate_family_count = Some(weight);
                        if weight == 0 {
                            self.phase = Phase::Done;
                            return Ok(None);
                        }
                        self.desired = self
                            .desired
                            .min(usize::try_from(weight).unwrap_or(usize::MAX));
                        let layouts = (size_of::<Seed>()
                            + size_of::<ExtendedParallelGeometryBranch>()
                            + size_of::<TraversalTask>())
                            as u128;
                        let future = (self.desired as u128)
                            .checked_mul(layouts)
                            .and_then(|bytes| bytes.checked_add(64))
                            .ok_or_else(preparation_projection_error)?;
                        self.check_scratch(bound, outside, future)?;
                        self.seeds
                            .try_reserve_exact(self.desired)
                            .map_err(|_| storage_error())?;
                        self.branches
                            .try_reserve_exact(self.desired)
                            .map_err(|_| storage_error())?;
                        self.scratch_credit = self
                            .scratch_credit
                            .checked_add(self.seeds.capacity() as u128 * size_of::<Seed>() as u128)
                            .and_then(|bytes| {
                                bytes.checked_add(
                                    self.branches.capacity() as u128
                                        * (size_of::<ExtendedParallelGeometryBranch>()
                                            + size_of::<TraversalTask>())
                                            as u128,
                                )
                            })
                            .and_then(|bytes| bytes.checked_add(64))
                            .ok_or_else(preparation_projection_error)?;
                        self.check_scratch(bound, outside, 0)?;
                        self.seeds.push(Seed {
                            task,
                            rows,
                            weight,
                            splittable: true,
                        });
                        Phase::Select {
                            next: 0,
                            best: None,
                        }
                    }
                }
                Phase::Select { next, mut best } => {
                    if self.seeds.len() == self.desired {
                        Phase::Emit {
                            index: 0,
                            ordinal: 0,
                        }
                    } else if let Some(seed) = self.seeds.get(next) {
                        // Same tie rule as max_by_key: the last maximum wins.
                        if seed.splittable
                            && best.is_none_or(|index| self.seeds[index].weight <= seed.weight)
                        {
                            best = Some(next);
                        }
                        Phase::Select {
                            next: next + 1,
                            best,
                        }
                    } else {
                        match best {
                            Some(index) => Phase::Split {
                                index,
                                seed: self.seeds[index],
                            },
                            None => Phase::Emit {
                                index: 0,
                                ordinal: 0,
                            },
                        }
                    }
                }
                Phase::Split { index, mut seed } => {
                    let enumerator = self
                        .geometry
                        .enumerator
                        .as_ref()
                        .ok_or_else(traversal_error)?;
                    match advance_split_seed(&enumerator.family, &self.counts, &mut seed)? {
                        SplitSeedAdvance::Pending => Phase::Split { index, seed },
                        SplitSeedAdvance::Unsplittable => {
                            self.seeds[index].splittable = false;
                            Phase::Select {
                                next: 0,
                                best: None,
                            }
                        }
                        SplitSeedAdvance::Split(left, right) => {
                            self.seeds[index] = left;
                            self.seeds.insert(index + 1, right);
                            Phase::Select {
                                next: 0,
                                best: None,
                            }
                        }
                    }
                }
                Phase::Emit { index, mut ordinal } => {
                    if let Some(&seed) = self.seeds.get(index) {
                        let enumerator = self
                            .geometry
                            .enumerator
                            .as_ref()
                            .ok_or_else(traversal_error)?;
                        append_branch(enumerator, &mut self.branches, &mut ordinal, seed)?;
                        Phase::Emit {
                            index: index + 1,
                            ordinal,
                        }
                    } else {
                        self.ready = Some(
                            self.geometry
                                .seal_parallel_plan(core::mem::take(&mut self.branches), ordinal)?,
                        );
                        Phase::Done
                    }
                }
                _ => return Err(traversal_error()),
            };
            Ok(None)
        }

        fn check_scratch(
            &self,
            bound: ExecutionMemoryBound,
            outside: u128,
            future: u128,
        ) -> Result<(), WasmExactSearchError> {
            let live = outside
                .checked_add(self.frozen_geometry_bytes)
                .and_then(|bytes| bytes.checked_add(self.scratch_credit))
                .ok_or_else(preparation_projection_error)?;
            bound
                .ensure(live, future)
                .map_err(WasmExactSearchError::resource_admission)
        }
    }

    fn count_entries(family: &GeometrySolutionFamily) -> Result<usize, WasmExactSearchError> {
        usize::try_from(family.node_count())
            .ok()
            .and_then(|count| count.checked_add(2))
            .ok_or_else(preparation_projection_error)
    }

    fn count_unavailable() -> WasmExactSearchError {
        WasmExactSearchError::InvalidProblem("extended_parallel_geometry_count_unavailable")
    }

    fn preparation_projection_error() -> WasmExactSearchError {
        WasmExactSearchError::InvalidProblem(
            "extended_parallel_geometry_memory_projection_overflow",
        )
    }
}

#[cfg(test)]
#[path = "extended_geometry_parallel_tests.rs"]
mod tests;
