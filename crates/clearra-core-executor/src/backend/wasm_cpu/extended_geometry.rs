use crate::resource::{ExecutionMemoryBound, SharedExecutionNodeBudget};
use std::{collections::HashMap, sync::Arc};

#[path = "extended_geometry_parallel.rs"]
mod parallel;
pub(super) use parallel::{ExtendedParallelGeometryBranch, ExtendedParallelGeometryPlan};

use clearra_supply::pattern_universe::{
    piece_multiset_group::PackingMultisetFamily, MaterializedPatternUniverse,
    PatternPiecePositionIndex,
};

use super::{
    extended_board::ExtendedBoard,
    extended_geometry_component::{
        checked_component_scratch_upper_bound, compile_component_plan, ExtendedComponentPlanResult,
    },
    extended_geometry_dense::{DenseCompileAdvance, DenseExtendedFamilyCompiler},
    extended_geometry_domain::{
        feasible_piece_mask, ExtendedDomainPropagation, ExtendedDomainResult,
        ExtendedDomainWorkspace,
    },
    extended_inverse_catalog::ExtendedInverseCatalog,
    geometry_family::{FamilyNodeKind, GeometrySolutionFamily, FAMILY_EMPTY, FAMILY_INVALID},
    piece_index, WasmExactSearchError,
};

const MAX_EXTENDED_PIECES: usize = 60;
const NO_ROW: u32 = u32::MAX;
// A family node uses u32 identities, so 32 binary-carry levels can combine
// every representable branch while keeping the hot DFS frame smaller.
const UNION_LEVEL_COUNT: usize = 32;

pub(super) struct ExtendedGeometryCandidate {
    rows: [u32; MAX_EXTENDED_PIECES],
    row_count: u8,
    pub pattern_index: Arc<PatternPiecePositionIndex>,
}

impl ExtendedGeometryCandidate {
    pub fn row_ids(&self) -> &[u32] {
        &self.rows[..usize::from(self.row_count)]
    }
}

// Advance payloads are single-owner state transfers, so the large variant remains inline.
#[allow(clippy::large_enum_variant)]
pub(super) enum ExtendedGeometryAdvance {
    Pending,
    Candidate(ExtendedGeometryCandidate),
    Complete,
    ResourceIncomplete(&'static str),
}

#[derive(Clone)]
pub(super) struct GeometryTarget {
    pub counts: [u8; 7],
    pub pattern_index: Arc<PatternPiecePositionIndex>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ResidualKey {
    remaining: ExtendedBoard,
    used_counts: [u8; 7],
}

#[derive(Clone, Copy)]
struct CompileFrame {
    remaining: ExtendedBoard,
    key: ResidualKey,
    support_cursor: usize,
    support_end: usize,
    feasible_piece_mask: u8,
    depth: u8,
    entered: bool,
    chosen_row: u32,
    domain: ExtendedDomainPropagation,
    union_levels: [u32; UNION_LEVEL_COUNT],
}

impl CompileFrame {
    fn root(remaining: ExtendedBoard) -> Self {
        Self::child(remaining, 0, NO_ROW)
    }

    fn child(remaining: ExtendedBoard, depth: u8, chosen_row: u32) -> Self {
        Self {
            remaining,
            key: ResidualKey {
                remaining,
                used_counts: [0; 7],
            },
            support_cursor: 0,
            support_end: 0,
            feasible_piece_mask: 0,
            depth,
            entered: false,
            chosen_row,
            domain: ExtendedDomainPropagation::empty(),
            union_levels: [FAMILY_INVALID; UNION_LEVEL_COUNT],
        }
    }
}

enum CompileAdvance {
    Pending,
    Complete,
    ResourceIncomplete,
    NodeBudgetExceeded,
}

struct ExtendedFamilyCompiler {
    targets: Vec<GeometryTarget>,
    target_counts: Vec<[u8; 7]>,
    target_depth: u8,
    used_counts: [u8; 7],
    stack: Vec<CompileFrame>,
    residual_memo: HashMap<ResidualKey, u32>,
    memo_insertion_disabled: bool,
    domain_workspace: ExtendedDomainWorkspace,
    family: GeometrySolutionFamily,
    root_family: u32,
    expanded_nodes: usize,
    peak_frontier: usize,
    domain_pruned_states: usize,
    hall_pruned_states: usize,
    column_pruned_states: usize,
    component_pruned_states: usize,
    component_compositions: usize,
    allow_component_optimization: bool,
}

impl ExtendedFamilyCompiler {
    fn new(required_cells: ExtendedBoard, targets: Vec<GeometryTarget>) -> Self {
        let target_depth = targets
            .first()
            .map_or(0, |target| target.counts.iter().copied().sum());
        let target_counts = targets.iter().map(|target| target.counts).collect();
        Self {
            targets,
            target_counts,
            target_depth,
            used_counts: [0; 7],
            stack: vec![CompileFrame::root(required_cells)],
            residual_memo: HashMap::new(),
            memo_insertion_disabled: false,
            domain_workspace: ExtendedDomainWorkspace::new(),
            family: GeometrySolutionFamily::new(),
            root_family: FAMILY_INVALID,
            expanded_nodes: 0,
            peak_frontier: 1,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_pruned_states: 0,
            component_compositions: 0,
            allow_component_optimization: true,
        }
    }

    fn admit_advance(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        node_budget: Option<&SharedExecutionNodeBudget>,
        bound: ExecutionMemoryBound,
        coexisting_bytes: u128,
    ) -> Result<u128, WasmExactSearchError> {
        let observed = coexisting_bytes
            .checked_add(self.retained_bytes() as u128)
            .ok_or_else(memory_projection_error)?;
        // Final path-count and Vec-to-Arc allocations are admitted separately
        // after the last compiler step, against the final (not previous) DAG.
        let mut future = 0_u128;
        if self.stack.len() == self.stack.capacity() {
            future = future
                .checked_add(
                    (self
                        .stack
                        .len()
                        .checked_add(1)
                        .ok_or_else(memory_projection_error)? as u128)
                        .checked_mul(2 * core::mem::size_of::<CompileFrame>() as u128)
                        .ok_or_else(memory_projection_error)?,
                )
                .ok_or_else(memory_projection_error)?;
        }
        if !self.stack.is_empty()
            && !self.memo_insertion_disabled
            && self.residual_memo.len() == self.residual_memo.capacity()
        {
            future = future
                .checked_add(
                    (self
                        .residual_memo
                        .len()
                        .checked_add(1)
                        .ok_or_else(memory_projection_error)? as u128)
                        .checked_mul(4 * (core::mem::size_of::<(ResidualKey, u32)>() + 1) as u128)
                        .and_then(|bytes| bytes.checked_add(16))
                        .ok_or_else(memory_projection_error)?,
                )
                .ok_or_else(memory_projection_error)?;
        }
        if let Some(top) = self.stack.last() {
            if !top.entered {
                future = future
                    .checked_add(
                        self.domain_workspace
                            .checked_compile_future_bytes(
                                catalog,
                                top.remaining,
                                self.used_counts,
                                &self.target_counts,
                            )
                            .ok_or_else(memory_projection_error)?,
                    )
                    .ok_or_else(memory_projection_error)?;
            }
            let component_future = checked_component_scratch_upper_bound(
                top.remaining.count_ones(),
                feasible_piece_mask(&self.target_counts, self.used_counts),
            );
            self.allow_component_optimization = !top.entered
                && self.target_depth >= 7
                && node_budget.is_none()
                && component_future
                    .and_then(|extra| future.checked_add(extra))
                    .is_some_and(|extra| bound.ensure(observed, extra).is_ok());
            if self.allow_component_optimization {
                future = future
                    .checked_add(component_future.ok_or_else(memory_projection_error)?)
                    .ok_or_else(memory_projection_error)?;
            }
        }
        bound
            .ensure(observed, future)
            .map_err(WasmExactSearchError::resource_admission)?;
        let base = observed
            .checked_sub(self.family.retained_bytes() as u128)
            .and_then(|bytes| bytes.checked_add(future))
            .ok_or_else(memory_projection_error)?;
        self.family
            .set_retained_limit_bytes(Some(bound.cap_bytes() - base));
        Ok(base)
    }

    fn advance(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        node_budget: Option<&SharedExecutionNodeBudget>,
    ) -> CompileAdvance {
        if self.stack.is_empty() {
            return CompileAdvance::Complete;
        }
        let top_index = self.stack.len() - 1;
        if !self.stack[top_index].entered {
            let remaining = self.stack[top_index].remaining;
            let key = ResidualKey {
                remaining,
                used_counts: self.used_counts,
            };
            self.stack[top_index].key = key;
            if let Some(family) = self.residual_memo.get(&key).copied() {
                return self.finish_top(catalog, family, false);
            }

            if node_budget.is_some_and(|budget| !budget.try_consume()) {
                return CompileAdvance::NodeBudgetExceeded;
            }
            self.expanded_nodes = self.expanded_nodes.saturating_add(1);
            self.peak_frontier = self.peak_frontier.max(self.stack.len());
            let depth = self.stack[top_index].depth;
            if remaining.is_empty() {
                let family = if depth == self.target_depth && self.completed_target().is_some() {
                    FAMILY_EMPTY
                } else {
                    FAMILY_INVALID
                };
                return self.finish_top(catalog, family, true);
            }
            if depth >= self.target_depth
                || remaining.count_ones() as usize
                    != usize::from(self.target_depth.saturating_sub(depth)) * 4
            {
                return self.finish_top(catalog, FAMILY_INVALID, true);
            }

            let domain = match self.domain_workspace.compile(
                catalog,
                remaining,
                self.used_counts,
                &self.target_counts,
                usize::from(depth),
            ) {
                ExtendedDomainResult::Supported(domain) => domain,
                ExtendedDomainResult::Empty => {
                    self.domain_pruned_states = self.domain_pruned_states.saturating_add(1);
                    return self.finish_top(catalog, FAMILY_INVALID, true);
                }
                ExtendedDomainResult::HallImpossible => {
                    self.hall_pruned_states = self.hall_pruned_states.saturating_add(1);
                    return self.finish_top(catalog, FAMILY_INVALID, true);
                }
                ExtendedDomainResult::ProjectionImpossible => {
                    self.column_pruned_states = self.column_pruned_states.saturating_add(1);
                    return self.finish_top(catalog, FAMILY_INVALID, true);
                }
            };
            let feasible_piece_mask = feasible_piece_mask(&self.target_counts, self.used_counts);
            // The synchronous component optimizer has its own optimization
            // budget. Explicit request caps use the incremental exact DFS so
            // every visited node is debited before work, not retroactively.
            if self.target_depth >= 7 && node_budget.is_none() && self.allow_component_optimization
            {
                match compile_component_plan(
                    catalog,
                    remaining,
                    depth,
                    self.used_counts,
                    &self.target_counts,
                    feasible_piece_mask,
                    &mut self.family,
                ) {
                    ExtendedComponentPlanResult::NotApplicable => {}
                    ExtendedComponentPlanResult::Impossible => {
                        self.component_pruned_states =
                            self.component_pruned_states.saturating_add(1);
                        return self.finish_top(catalog, FAMILY_INVALID, true);
                    }
                    ExtendedComponentPlanResult::StorageUnavailable => {
                        return CompileAdvance::ResourceIncomplete;
                    }
                    ExtendedComponentPlanResult::Complete {
                        family,
                        expanded_nodes,
                    } => {
                        self.expanded_nodes = self.expanded_nodes.saturating_add(expanded_nodes);
                        self.component_compositions = self.component_compositions.saturating_add(1);
                        return self.finish_top(catalog, family, true);
                    }
                }
            }
            let frame = &mut self.stack[top_index];
            frame.support_cursor = 0;
            frame.support_end = catalog.support(domain.pivot_cell).len();
            frame.feasible_piece_mask = feasible_piece_mask;
            frame.domain = domain;
            frame.entered = true;
            return CompileAdvance::Pending;
        }

        let frame = self.stack[top_index];
        if frame.support_cursor < frame.support_end {
            self.stack[top_index].support_cursor += 1;
            let row_id = catalog.support(frame.domain.pivot_cell)[frame.support_cursor];
            if !frame.domain.row_allowed(
                catalog,
                row_id,
                frame.remaining,
                frame.feasible_piece_mask,
            ) {
                return CompileAdvance::Pending;
            }
            let row = catalog.skeleton(row_id);
            self.used_counts[piece_index(row.piece)] += 1;
            if self.stack.try_reserve(1).is_err() {
                self.used_counts[piece_index(row.piece)] -= 1;
                return CompileAdvance::ResourceIncomplete;
            }
            self.stack.push(CompileFrame::child(
                frame.remaining.without(row.cells),
                frame.depth + 1,
                row_id,
            ));
            return CompileAdvance::Pending;
        }

        let Some(family) = self.finalize_union(top_index) else {
            return CompileAdvance::ResourceIncomplete;
        };
        self.finish_top(catalog, family, true)
    }

    fn finish_top(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        suffix_family: u32,
        memoize: bool,
    ) -> CompileAdvance {
        let frame = self.stack.pop().expect("extended geometry frame exists");
        if memoize {
            self.memoize(frame.key, suffix_family);
        }
        if frame.chosen_row == NO_ROW {
            self.root_family = suffix_family;
            return CompileAdvance::Complete;
        }
        let piece = piece_index(catalog.skeleton(frame.chosen_row).piece);
        self.used_counts[piece] -= 1;
        if suffix_family == FAMILY_INVALID {
            return CompileAdvance::Pending;
        }
        let Some(branch) = self.family.append(frame.chosen_row, suffix_family) else {
            return CompileAdvance::ResourceIncomplete;
        };
        if self.add_branch_to_parent(branch) {
            CompileAdvance::Pending
        } else {
            CompileAdvance::ResourceIncomplete
        }
    }

    fn memoize(&mut self, key: ResidualKey, family: u32) {
        if self.memo_insertion_disabled || self.residual_memo.contains_key(&key) {
            return;
        }
        if self.residual_memo.try_reserve(1).is_err() {
            self.memo_insertion_disabled = true;
            return;
        }
        self.residual_memo.insert(key, family);
    }

    fn add_branch_to_parent(&mut self, mut branch: u32) -> bool {
        let Some(parent_index) = self.stack.len().checked_sub(1) else {
            return false;
        };
        for level in 0..UNION_LEVEL_COUNT {
            let existing = self.stack[parent_index].union_levels[level];
            if existing == FAMILY_INVALID {
                self.stack[parent_index].union_levels[level] = branch;
                return true;
            }
            self.stack[parent_index].union_levels[level] = FAMILY_INVALID;
            let Some(union) = self.family.union(existing, branch) else {
                return false;
            };
            branch = union;
        }
        false
    }

    fn finalize_union(&mut self, frame_index: usize) -> Option<u32> {
        let mut result = FAMILY_INVALID;
        for level in (0..UNION_LEVEL_COUNT).rev() {
            let branch = self.stack[frame_index].union_levels[level];
            if branch != FAMILY_INVALID {
                result = self.family.union(result, branch)?;
            }
        }
        Some(result)
    }

    fn completed_target(&self) -> Option<&GeometryTarget> {
        self.targets
            .binary_search_by_key(&self.used_counts, |target| target.counts)
            .ok()
            .and_then(|index| self.targets.get(index))
    }

    fn retained_bytes(&self) -> usize {
        self.targets.capacity() * core::mem::size_of::<GeometryTarget>()
            + self.target_counts.capacity() * core::mem::size_of::<[u8; 7]>()
            + self.stack.capacity() * core::mem::size_of::<CompileFrame>()
            + self.residual_memo.capacity().saturating_mul(8).div_ceil(7)
                * (core::mem::size_of::<(ResidualKey, u32)>() + 1)
            + 16
            + self.domain_workspace.retained_bytes()
            + self.family.retained_bytes()
    }

    fn into_enumerator(self) -> ExtendedFamilyEnumerator {
        ExtendedFamilyEnumerator::new(
            self.targets,
            self.family,
            self.root_family,
            self.target_depth,
        )
    }
}

pub(super) fn memory_projection_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_geometry_memory_projection_unavailable")
}

pub(super) fn checked_compiler_completion_future_bytes(
    node_count: u32,
    target_count: usize,
) -> Option<u128> {
    (u128::from(node_count) + 2)
        .checked_mul(core::mem::size_of::<u128>() as u128)?
        .checked_add(
            (target_count as u128)
                .checked_mul(2 * core::mem::size_of::<GeometryTarget>() as u128)?,
        )?
        .checked_add(4 * core::mem::size_of::<TraversalTask>() as u128 + 64)
}

#[derive(Clone, Copy)]
struct TraversalTask {
    family: u32,
    continuations: [u32; MAX_EXTENDED_PIECES],
    depth: u8,
    continuation_count: u8,
}

struct ExtendedFamilyEnumerator {
    targets: Arc<[GeometryTarget]>,
    family: Arc<GeometrySolutionFamily>,
    tasks: Vec<TraversalTask>,
    rows: [u32; MAX_EXTENDED_PIECES],
    target_depth: u8,
    task_memory_limit: Option<u128>,
    refused_task_bytes: Option<u128>,
}

impl ExtendedFamilyEnumerator {
    fn new(
        targets: Vec<GeometryTarget>,
        family: GeometrySolutionFamily,
        root_family: u32,
        target_depth: u8,
    ) -> Self {
        let mut tasks = Vec::new();
        if root_family != FAMILY_INVALID {
            tasks.push(TraversalTask {
                family: root_family,
                continuations: [FAMILY_INVALID; MAX_EXTENDED_PIECES],
                depth: 0,
                continuation_count: 0,
            });
        }
        Self {
            targets: targets.into(),
            family: Arc::new(family),
            tasks,
            rows: [0; MAX_EXTENDED_PIECES],
            target_depth,
            task_memory_limit: None,
            refused_task_bytes: None,
        }
    }

    fn next_candidate(
        &mut self,
        catalog: &ExtendedInverseCatalog,
    ) -> Result<Option<ExtendedGeometryCandidate>, ()> {
        self.next_candidate_with_allocation_guard(catalog, |_| Ok(()))
    }

    fn next_candidate_with_allocation_guard(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        mut guard: impl FnMut(u128) -> Result<(), ()>,
    ) -> Result<Option<ExtendedGeometryCandidate>, ()> {
        while let Some(mut task) = self.tasks.pop() {
            loop {
                if task.family == FAMILY_INVALID {
                    break;
                }
                if task.family == FAMILY_EMPTY {
                    if task.continuation_count != 0 {
                        task.continuation_count -= 1;
                        task.family = task.continuations[usize::from(task.continuation_count)];
                        continue;
                    }
                    if task.depth != self.target_depth {
                        return Err(());
                    }
                    return self.candidate(catalog, task.depth).map(Some).ok_or(());
                }
                let node = self.family.node(task.family).ok_or(())?;
                match node.kind {
                    FamilyNodeKind::Append => {
                        if usize::from(task.depth) >= MAX_EXTENDED_PIECES
                            || node.row_id as usize >= catalog.skeletons().len()
                        {
                            return Err(());
                        }
                        self.rows[usize::from(task.depth)] = node.row_id;
                        task.depth += 1;
                        task.family = node.left;
                    }
                    FamilyNodeKind::Union => {
                        let mut right = task;
                        right.family = node.right;
                        self.push_task(right, &mut guard)?;
                        task.family = node.left;
                    }
                    FamilyNodeKind::Product => {
                        let index = usize::from(task.continuation_count);
                        if index >= MAX_EXTENDED_PIECES {
                            return Err(());
                        }
                        task.continuations[index] = node.right;
                        task.continuation_count += 1;
                        task.family = node.left;
                    }
                }
            }
        }
        Ok(None)
    }

    fn push_task(
        &mut self,
        task: TraversalTask,
        guard: &mut impl FnMut(u128) -> Result<(), ()>,
    ) -> Result<(), ()> {
        let future_capacity = if self.tasks.len() == self.tasks.capacity() {
            self.tasks.len().saturating_add(1).saturating_mul(2).max(4)
        } else {
            self.tasks.capacity()
        };
        let future_bytes = future_capacity as u128 * core::mem::size_of::<TraversalTask>() as u128;
        guard(future_bytes)?;
        if let Some(limit) = self.task_memory_limit {
            let future_capacity = if self.tasks.len() == self.tasks.capacity() {
                self.tasks.len().saturating_add(1).saturating_mul(2).max(4)
            } else {
                self.tasks.capacity()
            };
            let bytes = future_capacity as u128 * core::mem::size_of::<TraversalTask>() as u128;
            if bytes > limit {
                self.refused_task_bytes = Some(bytes);
                return Err(());
            }
        }
        self.tasks.try_reserve(1).map_err(|_| ())?;
        self.tasks.push(task);
        Ok(())
    }

    fn candidate(
        &self,
        catalog: &ExtendedInverseCatalog,
        row_count: u8,
    ) -> Option<ExtendedGeometryCandidate> {
        let mut counts = [0_u8; 7];
        for row_id in &self.rows[..usize::from(row_count)] {
            counts[piece_index(catalog.skeleton(*row_id).piece)] += 1;
        }
        let target = self
            .targets
            .binary_search_by_key(&counts, |target| target.counts)
            .ok()
            .and_then(|index| self.targets.get(index))?;
        Some(ExtendedGeometryCandidate {
            rows: self.rows,
            row_count,
            pattern_index: Arc::clone(&target.pattern_index),
        })
    }

    fn retained_bytes(&self) -> usize {
        usize::from(Arc::strong_count(&self.targets) == 1)
            * self.targets.len()
            * core::mem::size_of::<GeometryTarget>()
            + usize::from(Arc::strong_count(&self.family) == 1) * self.family.retained_bytes()
            + self.tasks.capacity() * core::mem::size_of::<TraversalTask>()
    }
}

pub(super) struct ExtendedGeometrySearch {
    dense_compiler: Option<DenseExtendedFamilyCompiler>,
    compiler: Option<ExtendedFamilyCompiler>,
    enumerator: Option<ExtendedFamilyEnumerator>,
    external_targets: Option<Arc<[GeometryTarget]>>,
    pattern_index_bytes: usize,
    expanded_nodes: usize,
    domain_pruned_states: usize,
    hall_pruned_states: usize,
    column_pruned_states: usize,
    component_pruned_states: usize,
    component_compositions: usize,
    peak_frontier: usize,
    candidate_count: usize,
    candidate_family_count: Option<u128>,
    dense_static_geometry: bool,
}

impl ExtendedGeometrySearch {
    pub fn new_bounded(
        universe: &MaterializedPatternUniverse,
        family: &PackingMultisetFamily,
        catalog: &ExtendedInverseCatalog,
        bound: ExecutionMemoryBound,
        coexisting_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        let piece_count = catalog.required_cells().count_ones() as usize / 4;
        let sequence_len = match universe.structure() {
            clearra_supply::pattern_universe::materialized_pattern_universe::MaterializedPatternUniverseStructure::Explicit =>
                (0..universe.pattern_count()).map(|index| universe.sequence_len_at(index)).max().unwrap_or(0),
            clearra_supply::pattern_universe::materialized_pattern_universe::MaterializedPatternUniverseStructure::Standard7BagLexicographic { sequence_len }
            | clearra_supply::pattern_universe::materialized_pattern_universe::MaterializedPatternUniverseStructure::ObservedStandard7BagLexicographic { sequence_len, .. }
            | clearra_supply::pattern_universe::materialized_pattern_universe::MaterializedPatternUniverseStructure::FactorizedQueueExpression { sequence_len } => usize::from(sequence_len),
        };
        let mut future = (family.len() as u128)
            .checked_mul((2 * core::mem::size_of::<GeometryTarget>() + 7 + 32) as u128)
            .ok_or_else(memory_projection_error)?;
        future = future
            .checked_add(
                universe
                    .checked_retained_capacity_bytes()
                    .ok_or_else(memory_projection_error)?,
            )
            .ok_or_else(memory_projection_error)?;
        future = future
            .checked_add(
                (sequence_len as u128)
                    .checked_mul(
                        2 * core::mem::size_of::<clearra_core_domain::piece::piece_kind::PieceKind>(
                        ) as u128,
                    )
                    .ok_or_else(memory_projection_error)?,
            )
            .ok_or_else(memory_projection_error)?;
        for group in family.groups() {
            let local = group.shared_pattern_bits().count_ones() as u128;
            let index_bytes = local
                .checked_mul(4)
                .and_then(|bytes| {
                    bytes.checked_add(
                        local
                            .div_ceil(64)
                            .checked_mul(sequence_len as u128)?
                            .checked_mul(7 * 8)?,
                    )
                })
                .ok_or_else(memory_projection_error)?;
            future = future
                .checked_add(index_bytes)
                .ok_or_else(memory_projection_error)?;
        }
        let bucket_count = match piece_count {
            0..=7 => 1_u128 << 14,
            8..=12 => 1 << 17,
            _ => 1 << 18,
        };
        future = future
            .checked_add(
                bucket_count * 4 + 16 * 1024 + core::mem::size_of::<CompileFrame>() as u128 + 4096,
            )
            .ok_or_else(memory_projection_error)?;
        let observed = coexisting_bytes
            .checked_add(
                family
                    .checked_retained_bytes()
                    .ok_or_else(memory_projection_error)?,
            )
            .ok_or_else(memory_projection_error)?;
        bound
            .ensure(observed, future)
            .map_err(WasmExactSearchError::resource_admission)?;
        Self::new(universe, family, catalog)
    }

    pub fn new(
        universe: &MaterializedPatternUniverse,
        family: &PackingMultisetFamily,
        catalog: &ExtendedInverseCatalog,
    ) -> Result<Self, WasmExactSearchError> {
        let required_cells = catalog.required_cells();
        let required_piece_count = required_cells.count_ones() as usize / 4;
        let mut targets = Vec::new();
        targets.try_reserve_exact(family.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_geometry_target_storage_unavailable",
            )
        })?;
        let mut pattern_index_bytes = 0usize;
        for group in family.groups() {
            let counts = group.key().counts();
            if counts
                .iter()
                .map(|count| usize::from(*count))
                .sum::<usize>()
                != required_piece_count
            {
                continue;
            }
            let membership = group.shared_pattern_bits();
            let pattern_index = Arc::new(
                PatternPiecePositionIndex::compile_subset_before(
                    universe,
                    membership.as_ref(),
                    universe.pattern_count(),
                )
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_extended_pattern_index_compile_failed",
                    )
                })?,
            );
            pattern_index_bytes =
                pattern_index_bytes.saturating_add(pattern_index.retained_bytes());
            targets.push(GeometryTarget {
                counts,
                pattern_index,
            });
        }
        targets.sort_unstable_by_key(|target| target.counts);
        if targets
            .windows(2)
            .any(|pair| pair[0].counts == pair[1].counts)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_geometry_duplicate_multiset_target",
            ));
        }
        let dense_required = catalog
            .dense_geometry()
            .and_then(|dense| dense.encode(required_cells));
        let dense_static_geometry = dense_required.is_some();
        let (dense_compiler, compiler) = match dense_required {
            Some(required) => (
                Some(DenseExtendedFamilyCompiler::new(required, targets)),
                None,
            ),
            None => (
                None,
                Some(ExtendedFamilyCompiler::new(required_cells, targets)),
            ),
        };
        Ok(Self {
            dense_compiler,
            compiler,
            enumerator: None,
            external_targets: None,
            pattern_index_bytes,
            expanded_nodes: 0,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_pruned_states: 0,
            component_compositions: 0,
            peak_frontier: 0,
            candidate_count: 0,
            candidate_family_count: None,
            dense_static_geometry,
        })
    }

    pub fn prepare_external(&mut self) -> bool {
        if self.enumerator.is_some() || self.external_targets.is_some() {
            return false;
        }
        let targets = if let Some(compiler) = self.dense_compiler.take() {
            compiler.into_targets()
        } else if let Some(compiler) = self.compiler.take() {
            compiler.targets
        } else {
            return false;
        };
        self.external_targets = Some(targets.into());
        true
    }

    /// A native verifier consumes only candidates from the parent's admitted
    /// exact family. It owns no compiler, target index or enumeration DAG.
    pub fn private_verifier_placeholder() -> Self {
        Self {
            dense_compiler: None,
            compiler: None,
            enumerator: None,
            external_targets: None,
            pattern_index_bytes: 0,
            expanded_nodes: 0,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_pruned_states: 0,
            component_compositions: 0,
            peak_frontier: 0,
            candidate_count: 0,
            candidate_family_count: None,
            dense_static_geometry: false,
        }
    }

    pub(super) fn checked_external_verifier_future_bytes(&self) -> Option<u128> {
        if self.external_targets.is_some() || self.enumerator.is_some() {
            return Some(0);
        }
        let count = if let Some(dense) = self.dense_compiler.as_ref() {
            dense.targets().len()
        } else {
            self.compiler.as_ref()?.targets.len()
        };
        // `Vec<GeometryTarget>` and the resulting Arc slice briefly coexist.
        (count as u128)
            .checked_mul(2 * core::mem::size_of::<GeometryTarget>() as u128)?
            .checked_add(32)
    }

    /// Pattern indexes remain Arc-shared. No compiler/family DAG or private
    /// traversal workspace is copied into a packet verifier.
    pub(super) fn external_verifier_from_owner(&self) -> Option<Self> {
        let targets = if let Some(targets) = self.external_targets.as_ref() {
            Arc::clone(targets)
        } else if let Some(enumerator) = self.enumerator.as_ref() {
            Arc::clone(&enumerator.targets)
        } else if let Some(dense) = self.dense_compiler.as_ref() {
            Arc::from(dense.targets().to_vec())
        } else {
            Arc::from(self.compiler.as_ref()?.targets.clone())
        };
        let mut verifier = Self::private_verifier_placeholder();
        verifier.external_targets = Some(targets);
        Some(verifier)
    }

    pub fn is_compiling(&self) -> bool {
        self.dense_compiler.is_some() || self.compiler.is_some()
    }

    pub fn complete_parallel_enumeration(
        &mut self,
        candidate_count: usize,
    ) -> Result<(), WasmExactSearchError> {
        if self.enumerator.is_some()
            || self.is_compiling()
            || self.candidate_family_count != Some(candidate_count as u128)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_parallel_geometry_family_count_mismatch",
            ));
        }
        self.candidate_count = candidate_count;
        Ok(())
    }

    pub fn advance(&mut self, catalog: &ExtendedInverseCatalog) -> ExtendedGeometryAdvance {
        self.advance_with_node_budget(catalog, None, None)
            .expect("unbounded Geometry does not perform memory admission")
    }

    /// Credits compiler scratch and DAG growth before allocation. Shared-owner
    /// callers pass the live bytes outside this Geometry owner, not a per-worker
    /// copy of the whole request envelope.
    pub fn advance_bounded(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        node_budget: Option<&SharedExecutionNodeBudget>,
        bound: ExecutionMemoryBound,
        coexisting_bytes: u128,
    ) -> Result<ExtendedGeometryAdvance, WasmExactSearchError> {
        let family_base = if let Some(compiler) = self.compiler.as_mut() {
            Some(
                compiler.admit_advance(
                    catalog,
                    node_budget,
                    bound,
                    coexisting_bytes
                        .checked_add(self.pattern_index_bytes as u128)
                        .ok_or_else(memory_projection_error)?,
                )?,
            )
        } else if let Some(compiler) = self.dense_compiler.as_mut() {
            Some(
                compiler.admit_advance(
                    catalog,
                    node_budget,
                    bound,
                    coexisting_bytes
                        .checked_add(self.pattern_index_bytes as u128)
                        .ok_or_else(memory_projection_error)?,
                )?,
            )
        } else {
            None
        };
        let enumerator_base = if let Some(enumerator) = self.enumerator.as_mut() {
            let retained = enumerator.retained_bytes() as u128;
            let tasks =
                enumerator.tasks.capacity() as u128 * core::mem::size_of::<TraversalTask>() as u128;
            let base = coexisting_bytes
                .checked_add(self.pattern_index_bytes as u128)
                .and_then(|bytes| bytes.checked_add(retained))
                .and_then(|bytes| bytes.checked_sub(tasks))
                .ok_or_else(memory_projection_error)?;
            bound
                .ensure(base, tasks)
                .map_err(WasmExactSearchError::resource_admission)?;
            enumerator.task_memory_limit = Some(bound.cap_bytes() - base);
            Some(base)
        } else {
            None
        };
        let result =
            self.advance_with_node_budget(catalog, node_budget, Some((bound, coexisting_bytes)))?;
        let refused = self
            .compiler
            .as_ref()
            .and_then(|compiler| compiler.family.refused_retained_bytes())
            .or_else(|| {
                self.dense_compiler
                    .as_ref()
                    .and_then(DenseExtendedFamilyCompiler::refused_family_bytes)
            });
        if let (Some(base), Some(bytes)) = (family_base, refused) {
            return Err(WasmExactSearchError::resource_admission(
                bound
                    .ensure(base, bytes)
                    .expect_err("family refused a preallocation beyond its assigned credit"),
            ));
        }
        if let (Some(base), Some(bytes)) = (
            enumerator_base,
            self.enumerator
                .as_ref()
                .and_then(|enumerator| enumerator.refused_task_bytes),
        ) {
            return Err(WasmExactSearchError::resource_admission(
                bound
                    .ensure(base, bytes)
                    .expect_err("enumerator refused a preallocation beyond its assigned credit"),
            ));
        }
        Ok(result)
    }

    pub(super) fn advance_with_node_budget(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        node_budget: Option<&SharedExecutionNodeBudget>,
        completion_bound: Option<(ExecutionMemoryBound, u128)>,
    ) -> Result<ExtendedGeometryAdvance, WasmExactSearchError> {
        if let Some(compiler) = self.dense_compiler.as_mut() {
            let Some(dense_catalog) = catalog.dense_geometry() else {
                return Ok(ExtendedGeometryAdvance::ResourceIncomplete(
                    "dense_geometry_catalog_missing",
                ));
            };
            match compiler.advance(catalog, dense_catalog, node_budget) {
                DenseCompileAdvance::Pending => {
                    self.observe_compiler_metrics();
                    return Ok(ExtendedGeometryAdvance::Pending);
                }
                DenseCompileAdvance::ResourceIncomplete => {
                    self.observe_compiler_metrics();
                    return Ok(ExtendedGeometryAdvance::ResourceIncomplete(
                        "geometry_solution_family_storage_unavailable",
                    ));
                }
                DenseCompileAdvance::NodeBudgetExceeded => {
                    self.observe_compiler_metrics();
                    return Ok(ExtendedGeometryAdvance::ResourceIncomplete(
                        "node_budget_exceeded",
                    ));
                }
                DenseCompileAdvance::Complete => {
                    if let Some((bound, outside)) = completion_bound {
                        let observed = outside
                            .checked_add(self.pattern_index_bytes as u128)
                            .and_then(|bytes| bytes.checked_add(compiler.retained_bytes() as u128))
                            .ok_or_else(memory_projection_error)?;
                        let future = compiler
                            .checked_completion_future_bytes()
                            .ok_or_else(memory_projection_error)?;
                        bound
                            .ensure(observed, future)
                            .map_err(WasmExactSearchError::resource_admission)?;
                    }
                    self.observe_compiler_metrics();
                    let compiler = self
                        .dense_compiler
                        .take()
                        .expect("dense extended compiler exists");
                    self.candidate_family_count = compiler.candidate_family_count();
                    let (targets, family, root_family, target_depth) = compiler.into_parts();
                    self.enumerator = Some(ExtendedFamilyEnumerator::new(
                        targets,
                        family,
                        root_family,
                        target_depth,
                    ));
                    return Ok(ExtendedGeometryAdvance::Pending);
                }
            }
        }
        if let Some(compiler) = self.compiler.as_mut() {
            match compiler.advance(catalog, node_budget) {
                CompileAdvance::Pending => {
                    self.observe_compiler_metrics();
                    return Ok(ExtendedGeometryAdvance::Pending);
                }
                CompileAdvance::ResourceIncomplete => {
                    self.observe_compiler_metrics();
                    return Ok(ExtendedGeometryAdvance::ResourceIncomplete(
                        "geometry_solution_family_storage_unavailable",
                    ));
                }
                CompileAdvance::NodeBudgetExceeded => {
                    self.observe_compiler_metrics();
                    return Ok(ExtendedGeometryAdvance::ResourceIncomplete(
                        "node_budget_exceeded",
                    ));
                }
                CompileAdvance::Complete => {
                    if let Some((bound, outside)) = completion_bound {
                        let observed = outside
                            .checked_add(self.pattern_index_bytes as u128)
                            .and_then(|bytes| bytes.checked_add(compiler.retained_bytes() as u128))
                            .ok_or_else(memory_projection_error)?;
                        let future = checked_compiler_completion_future_bytes(
                            compiler.family.node_count(),
                            compiler.targets.len(),
                        )
                        .ok_or_else(memory_projection_error)?;
                        bound
                            .ensure(observed, future)
                            .map_err(WasmExactSearchError::resource_admission)?;
                    }
                    self.observe_compiler_metrics();
                    let compiler = self.compiler.take().expect("extended compiler exists");
                    self.candidate_family_count = compiler.family.path_count(compiler.root_family);
                    self.enumerator = Some(compiler.into_enumerator());
                    return Ok(ExtendedGeometryAdvance::Pending);
                }
            }
        }
        let Some(enumerator) = self.enumerator.as_mut() else {
            return Ok(ExtendedGeometryAdvance::Complete);
        };
        Ok(match enumerator.next_candidate(catalog) {
            Ok(Some(candidate)) => {
                self.candidate_count = self.candidate_count.saturating_add(1);
                ExtendedGeometryAdvance::Candidate(candidate)
            }
            Ok(None) => {
                self.enumerator = None;
                ExtendedGeometryAdvance::Complete
            }
            Err(()) => ExtendedGeometryAdvance::ResourceIncomplete(
                "geometry_solution_family_traversal_unavailable",
            ),
        })
    }

    pub fn external_candidate(
        &self,
        catalog: &ExtendedInverseCatalog,
        row_ids: &[u32],
    ) -> Option<ExtendedGeometryCandidate> {
        if row_ids.len() > MAX_EXTENDED_PIECES
            || row_ids.len() != catalog.required_cells().count_ones() as usize / 4
        {
            return None;
        }
        let mut rows = [0_u32; MAX_EXTENDED_PIECES];
        let mut counts = [0_u8; 7];
        let mut covered = ExtendedBoard::EMPTY;
        for (index, row_id) in row_ids.iter().copied().enumerate() {
            let row = catalog.skeletons().get(row_id as usize)?;
            if covered.intersects(row.cells) || !row.cells.is_subset_of(catalog.required_cells()) {
                return None;
            }
            covered = covered.union(row.cells);
            counts[piece_index(row.piece)] = counts[piece_index(row.piece)].checked_add(1)?;
            rows[index] = row_id;
        }
        if covered != catalog.required_cells() {
            return None;
        }
        let targets = if let Some(compiler) = self.dense_compiler.as_ref() {
            compiler.targets()
        } else if let Some(compiler) = self.compiler.as_ref() {
            compiler.targets.as_slice()
        } else if let Some(enumerator) = self.enumerator.as_ref() {
            enumerator.targets.as_ref()
        } else {
            self.external_targets.as_ref()?.as_ref()
        };
        let target = targets
            .binary_search_by_key(&counts, |target| target.counts)
            .ok()
            .and_then(|index| targets.get(index))?;
        Some(ExtendedGeometryCandidate {
            rows,
            row_count: row_ids.len() as u8,
            pattern_index: Arc::clone(&target.pattern_index),
        })
    }

    fn observe_compiler_metrics(&mut self) {
        if let Some(compiler) = self.dense_compiler.as_ref() {
            self.expanded_nodes = compiler.expanded_nodes();
            self.domain_pruned_states = compiler.domain_pruned_states();
            self.hall_pruned_states = compiler.hall_pruned_states();
            self.column_pruned_states = compiler.column_pruned_states();
            self.component_pruned_states = compiler.component_pruned_states();
            self.component_compositions = compiler.component_compositions();
            self.peak_frontier = compiler.peak_frontier();
            return;
        }
        let Some(compiler) = self.compiler.as_ref() else {
            return;
        };
        self.expanded_nodes = compiler.expanded_nodes;
        self.domain_pruned_states = compiler.domain_pruned_states;
        self.hall_pruned_states = compiler.hall_pruned_states;
        self.column_pruned_states = compiler.column_pruned_states;
        self.component_pruned_states = compiler.component_pruned_states;
        self.component_compositions = compiler.component_compositions;
        self.peak_frontier = compiler.peak_frontier;
    }

    pub const fn expanded_nodes(&self) -> usize {
        self.expanded_nodes
    }

    pub const fn peak_frontier(&self) -> usize {
        self.peak_frontier
    }

    pub const fn candidate_count(&self) -> usize {
        self.candidate_count
    }

    pub const fn candidate_family_count(&self) -> Option<u128> {
        self.candidate_family_count
    }

    pub const fn domain_pruned_states(&self) -> usize {
        self.domain_pruned_states
    }

    pub const fn hall_pruned_states(&self) -> usize {
        self.hall_pruned_states
    }

    pub const fn column_pruned_states(&self) -> usize {
        self.column_pruned_states
    }

    pub const fn component_pruned_states(&self) -> usize {
        self.component_pruned_states
    }

    pub const fn component_compositions(&self) -> usize {
        self.component_compositions
    }

    pub const fn uses_dense_static_geometry(&self) -> bool {
        self.dense_static_geometry
    }

    pub fn retained_bytes(&self) -> usize {
        self.pattern_index_bytes
            + self
                .dense_compiler
                .as_ref()
                .map_or(0, DenseExtendedFamilyCompiler::retained_bytes)
            + self
                .compiler
                .as_ref()
                .map_or(0, ExtendedFamilyCompiler::retained_bytes)
            + self
                .enumerator
                .as_ref()
                .map_or(0, ExtendedFamilyEnumerator::retained_bytes)
            + self.external_targets.as_ref().map_or(0, |targets| {
                usize::from(Arc::strong_count(targets) == 1)
                    * targets.len()
                    * core::mem::size_of::<GeometryTarget>()
            })
    }
}
