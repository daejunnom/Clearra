//! Partial-set membership against an immutable exact-cover expression.
//!
//! Let every cover in F tile the same required cell set R without overlap.
//! For a row r, G_r(S) rejects every selected row t != r that overlaps r.
//! Then S is a subset of some C in F exactly when
//!     OR_{C in F} AND_{r in C} G_r(S)
//! holds. A selected t absent from C overlaps C at a cell of nonempty t, so
//! it fails a guard. This needs nonempty catalog rows contained in R; the
//! constructor checks those preconditions and the expression's cover regions.
//!
//! Append becomes guard conjunction, Union disjunction, and Product
//! conjunction. Applying these predicates to the joint ZDD preserves source
//! correlations without deriving new geometry nodes or enumerating covers.
//! A child predicate is local compatibility, not the child's downward closure:
//! the unit geometry accepts selections outside its empty region.

use core::mem::size_of;
use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;

use super::{
    exact_collections::ExactHashMap,
    geometry_family::{FamilyNodeKind, GeometrySolutionFamily, FAMILY_EMPTY, FAMILY_INVALID},
    geometry_partial_family::{WorkBudget, ZddArena, ZddError, ZddId, ZddLimits, ZddNode},
};

#[cfg(test)]
#[path = "geometry_compatibility_filter_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FilterKey {
    geometry: u32,
    joint: ZddId,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct GuardKey {
    row: u32,
    joint: ZddId,
}

enum FilterFrame {
    Visit(FilterKey),
    Finish(FilterKey),
    UnionLeft { key: FilterKey, right: u32 },
    UnionRight { key: FilterKey, left: ZddId },
    ProductLeft { key: FilterKey, right: u32 },
}

enum GuardFrame {
    Visit(ZddId),
    Elided(ZddId),
    Low {
        root: ZddId,
        node: ZddNode,
    },
    High {
        root: ZddId,
        variable: u32,
        low: ZddId,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CompatibilityMetrics {
    pub(super) filter_misses: u64,
    pub(super) guard_misses: u64,
    pub(super) saturated_unions: u64,
    pub(super) union_remainder_calls: u64,
    pub(super) union_remainder_created_nodes: u64,
    pub(super) union_remainder_work_steps: u64,
}

pub(super) struct GeometryCompatibilityFilter<'a> {
    family: &'a GeometrySolutionFamily,
    root: u32,
    row_cells: Vec<u64>,
    filtered: ExactHashMap<FilterKey, ZddId>,
    guarded: ExactHashMap<GuardKey, ZddId>,
    epoch: Option<u64>,
    arena_owner: Option<Arc<()>>,
    budget: WorkBudget,
    metrics: CompatibilityMetrics,
}

impl<'a> GeometryCompatibilityFilter<'a> {
    pub(super) fn new(
        family: &'a GeometrySolutionFamily,
        root: u32,
        row_cells: Vec<u64>,
        required_cells: u64,
        limits: ZddLimits,
        control: &ExecutionControl,
    ) -> Result<Self, ZddError> {
        let mut budget = WorkBudget::new(limits, "symbolic_work_down").with_control(control);
        budget.check_cancelled()?;
        certify_exact_cover(family, root, &row_cells, required_cells, &mut budget)?;
        Ok(Self {
            family,
            root,
            row_cells,
            filtered: ExactHashMap::default(),
            guarded: ExactHashMap::default(),
            epoch: None,
            arena_owner: None,
            budget,
            metrics: CompatibilityMetrics::default(),
        })
    }

    pub(super) fn work_steps(&self) -> u64 {
        self.budget.work_steps()
    }

    pub(super) const fn metrics(&self) -> CompatibilityMetrics {
        self.metrics
    }

    /// Backing capacity estimate, excluding allocator and operation stacks.
    pub(super) fn retained_bytes(&self) -> usize {
        self.row_cells.capacity() * size_of::<u64>()
            + self.filtered.capacity() * (size_of::<FilterKey>() + size_of::<ZddId>() + 1)
            + self.guarded.capacity() * (size_of::<GuardKey>() + size_of::<ZddId>() + 1)
    }

    /// Derived ZDD IDs cannot survive compaction. Release before the old and
    /// new arenas coexist, instead of waiting for the next membership call.
    pub(super) fn clear_memo(&mut self) {
        self.filtered = ExactHashMap::default();
        self.guarded = ExactHashMap::default();
        self.epoch = None;
        self.arena_owner = None;
    }

    pub(super) fn filter_extendible(
        &mut self,
        arena: &mut ZddArena,
        joint: ZddId,
    ) -> Result<ZddId, ZddError> {
        self.budget.check_cancelled()?;
        arena.node(joint)?;
        if self.epoch != Some(arena.epoch())
            || !self
                .arena_owner
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, arena.owner_token()))
        {
            self.clear_memo();
            self.epoch = Some(arena.epoch());
            self.arena_owner = Some(Arc::clone(arena.owner_token()));
        }
        let mut stack = Vec::new();
        push(
            &mut stack,
            FilterFrame::Visit(FilterKey {
                geometry: self.root,
                joint,
            }),
        )?;
        let mut value = ZddId::EMPTY;
        while let Some(frame) = stack.pop() {
            self.step(arena)?;
            match frame {
                FilterFrame::Visit(key) => {
                    if let Some(known) = self.filtered_value(arena, key)? {
                        value = known;
                        continue;
                    }
                    self.metrics.filter_misses = self.metrics.filter_misses.saturating_add(1);
                    let node = self
                        .family
                        .node(key.geometry)
                        .ok_or(ZddError::InvalidGeometry)?;
                    match node.kind {
                        FamilyNodeKind::Append => {
                            let guarded = self.guard(arena, key.joint, node.row_id)?;
                            push(&mut stack, FilterFrame::Finish(key))?;
                            push(
                                &mut stack,
                                FilterFrame::Visit(FilterKey {
                                    geometry: node.left,
                                    joint: guarded,
                                }),
                            )?;
                        }
                        FamilyNodeKind::Union => {
                            push(
                                &mut stack,
                                FilterFrame::UnionLeft {
                                    key,
                                    right: node.right,
                                },
                            )?;
                            push(
                                &mut stack,
                                FilterFrame::Visit(FilterKey {
                                    geometry: node.left,
                                    joint: key.joint,
                                }),
                            )?;
                        }
                        FamilyNodeKind::Product => {
                            push(
                                &mut stack,
                                FilterFrame::ProductLeft {
                                    key,
                                    right: node.right,
                                },
                            )?;
                            push(
                                &mut stack,
                                FilterFrame::Visit(FilterKey {
                                    geometry: node.left,
                                    joint: key.joint,
                                }),
                            )?;
                        }
                    }
                }
                FilterFrame::Finish(key) => self.remember_filter(key, value)?,
                FilterFrame::UnionLeft { key, right } => {
                    // The left predicate already covers the entire input.
                    // Canonical equality makes this an exact implication test.
                    if value == key.joint {
                        self.metrics.saturated_unions =
                            self.metrics.saturated_unions.saturating_add(1);
                        self.remember_filter(key, value)?;
                    } else {
                        // Each child is a pointwise restriction of this same
                        // input. With L = filter(left, J), disjointly partition
                        // the answer as L union filter(right, J - L). Keep all
                        // source bits in the difference; projecting them here
                        // would change the joint relation. Empty L needs no
                        // subtraction or additional ZDD work.
                        let remaining = if value == ZddId::EMPTY {
                            key.joint
                        } else {
                            let nodes_before = arena.node_count();
                            let work_before = arena.work_steps();
                            self.metrics.union_remainder_calls =
                                self.metrics.union_remainder_calls.saturating_add(1);
                            let remaining = arena.difference(key.joint, value);
                            self.metrics.union_remainder_created_nodes =
                                self.metrics.union_remainder_created_nodes.saturating_add(
                                    arena.node_count().saturating_sub(nodes_before) as u64,
                                );
                            self.metrics.union_remainder_work_steps = self
                                .metrics
                                .union_remainder_work_steps
                                .saturating_add(arena.work_steps().saturating_sub(work_before));
                            remaining?
                        };
                        push(&mut stack, FilterFrame::UnionRight { key, left: value })?;
                        push(
                            &mut stack,
                            FilterFrame::Visit(FilterKey {
                                geometry: right,
                                joint: remaining,
                            }),
                        )?;
                    }
                }
                FilterFrame::UnionRight { key, left } => {
                    value = arena.union(left, value)?;
                    self.remember_filter(key, value)?;
                }
                FilterFrame::ProductLeft { key, right } => {
                    if value == ZddId::EMPTY {
                        self.remember_filter(key, value)?;
                    } else {
                        // Sequential restriction keeps the same joint member
                        // in both factors; multiplying independent projections
                        // would invent row/source combinations.
                        push(&mut stack, FilterFrame::Finish(key))?;
                        push(
                            &mut stack,
                            FilterFrame::Visit(FilterKey {
                                geometry: right,
                                joint: value,
                            }),
                        )?;
                    }
                }
            }
        }
        Ok(value)
    }

    fn filtered_value(&self, arena: &ZddArena, key: FilterKey) -> Result<Option<ZddId>, ZddError> {
        if key.joint == ZddId::EMPTY || key.geometry == FAMILY_INVALID {
            return Ok(Some(ZddId::EMPTY));
        }
        if key.geometry == FAMILY_EMPTY || self.has_no_rows(arena, key.joint)? {
            return Ok(Some(key.joint));
        }
        Ok(self.filtered.get(&key).copied())
    }

    fn has_no_rows(&self, arena: &ZddArena, joint: ZddId) -> Result<bool, ZddError> {
        Ok(arena
            .node(joint)?
            .is_none_or(|node| node.variable as usize >= self.row_cells.len()))
    }

    fn guard(&mut self, arena: &mut ZddArena, joint: ZddId, row: u32) -> Result<ZddId, ZddError> {
        let cells = self.row_cells[row as usize];
        let mut stack = Vec::new();
        push(&mut stack, GuardFrame::Visit(joint))?;
        let mut value = ZddId::EMPTY;
        while let Some(frame) = stack.pop() {
            self.step(arena)?;
            match frame {
                GuardFrame::Visit(root) => {
                    if self.has_no_rows(arena, root)? {
                        value = root;
                        continue;
                    }
                    if let Some(&known) = self.guarded.get(&GuardKey { row, joint: root }) {
                        value = known;
                        continue;
                    }
                    self.metrics.guard_misses = self.metrics.guard_misses.saturating_add(1);
                    let node = arena.node(root)?.ok_or(ZddError::InvalidFamily)?;
                    if node.variable != row && self.row_cells[node.variable as usize] & cells != 0 {
                        push(&mut stack, GuardFrame::Elided(root))?;
                        push(&mut stack, GuardFrame::Visit(node.low))?;
                    } else {
                        push(&mut stack, GuardFrame::Low { root, node })?;
                        push(&mut stack, GuardFrame::Visit(node.low))?;
                    }
                }
                GuardFrame::Elided(root) => {
                    self.remember_guard(GuardKey { row, joint: root }, value)?
                }
                GuardFrame::Low { root, node } => {
                    push(
                        &mut stack,
                        GuardFrame::High {
                            root,
                            variable: node.variable,
                            low: value,
                        },
                    )?;
                    push(&mut stack, GuardFrame::Visit(node.high))?;
                }
                GuardFrame::High {
                    root,
                    variable,
                    low,
                } => {
                    value = arena.mk(variable, low, value)?;
                    self.remember_guard(GuardKey { row, joint: root }, value)?;
                }
            }
        }
        Ok(value)
    }

    fn step(&mut self, arena: &mut ZddArena) -> Result<(), ZddError> {
        self.budget.step()?;
        if self.budget.storage_pulse_due() {
            self.budget.report_storage(
                "symbolic_storage_compatibility_memo_entries",
                self.filtered.len().saturating_add(self.guarded.len()),
            );
            self.budget.report_storage(
                "symbolic_storage_compatibility_retained_bytes",
                self.retained_bytes(),
            );
            // This filter retains the original geometry expression. ZDD
            // growth is a separate representation, never new geometry nodes.
            self.budget
                .report_storage("symbolic_storage_generated_geometry_nodes", 0);
        }
        arena.checkpoint()
    }

    fn reserve_memo(&self) -> Result<(), ZddError> {
        self.budget
            .reserve_memo(self.filtered.len().saturating_add(self.guarded.len()))
    }

    fn remember_filter(&mut self, key: FilterKey, value: ZddId) -> Result<(), ZddError> {
        self.reserve_memo()?;
        self.filtered
            .try_reserve(1)
            .map_err(|_| ZddError::Allocation)?;
        self.filtered.insert(key, value);
        Ok(())
    }

    fn remember_guard(&mut self, key: GuardKey, value: ZddId) -> Result<(), ZddError> {
        self.reserve_memo()?;
        self.guarded
            .try_reserve(1)
            .map_err(|_| ZddError::Allocation)?;
        self.guarded.insert(key, value);
        Ok(())
    }
}

fn certify_exact_cover(
    family: &GeometrySolutionFamily,
    root: u32,
    row_cells: &[u64],
    required_cells: u64,
    budget: &mut WorkBudget,
) -> Result<(), ZddError> {
    if row_cells.len() > u32::MAX as usize
        || row_cells
            .iter()
            .any(|&cells| cells == 0 || cells & !required_cells != 0)
    {
        return Err(ZddError::InvalidGeometry);
    }
    let size = (family.node_count() as usize)
        .checked_add(2)
        .ok_or(ZddError::NodeIdExhausted)?;
    if root as usize >= size {
        return Err(ZddError::InvalidGeometry);
    }
    let mut regions = Vec::new();
    regions
        .try_reserve_exact(size)
        .map_err(|_| ZddError::Allocation)?;
    regions.extend([0_u64, 0]);
    for reference in 2..size {
        budget.step()?;
        let node = family
            .node(reference as u32)
            .ok_or(ZddError::InvalidGeometry)?;
        if node.left == FAMILY_INVALID || node.left as usize >= reference {
            return Err(ZddError::InvalidGeometry);
        }
        let left = regions[node.left as usize];
        let region = match node.kind {
            FamilyNodeKind::Append => {
                let cells = *row_cells
                    .get(node.row_id as usize)
                    .ok_or(ZddError::InvalidGeometry)?;
                if left & cells != 0 {
                    return Err(ZddError::InvalidGeometry);
                }
                left | cells
            }
            FamilyNodeKind::Union | FamilyNodeKind::Product => {
                if node.right == FAMILY_INVALID || node.right as usize >= reference {
                    return Err(ZddError::InvalidGeometry);
                }
                let right = regions[node.right as usize];
                if (node.kind == FamilyNodeKind::Union && left != right)
                    || (node.kind == FamilyNodeKind::Product && left & right != 0)
                {
                    return Err(ZddError::InvalidGeometry);
                }
                left | right
            }
        };
        regions.push(region);
    }
    if root != FAMILY_INVALID && regions[root as usize] != required_cells {
        return Err(ZddError::InvalidGeometry);
    }
    Ok(())
}

fn push<T>(stack: &mut Vec<T>, frame: T) -> Result<(), ZddError> {
    stack.try_reserve(1).map_err(|_| ZddError::Allocation)?;
    stack.push(frame);
    Ok(())
}
