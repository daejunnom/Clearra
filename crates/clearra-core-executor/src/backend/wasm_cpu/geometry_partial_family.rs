//! Exact finite set families for symbolic partial-placement search.
//!
//! Variables are global catalog/source IDs. A ZDD root denotes sets of those
//! IDs, not paths or an occupancy union. Missing variables are false. Every
//! nonterminal has increasing variable order and a nonempty high branch;
//! interning therefore makes equality of roots exact within one arena epoch.
//! Refusal never denotes the empty family. Successful intermediate nodes stay
//! valid after a failed operation. Only explicit compaction changes IDs.

use core::mem::size_of;
use std::sync::Arc;

use clearra_core_domain::execution_cancellation::{
    CancellationToken, ExecutionProgress, ProgressSink,
};

use super::{
    exact_collections::ExactHashMap,
    geometry_family::{FamilyNodeKind, GeometrySolutionFamily, FAMILY_EMPTY, FAMILY_INVALID},
};

#[cfg(test)]
#[path = "geometry_partial_family_tests.rs"]
mod tests;

const WORK_PROGRESS_MASK: u64 = (1 << 18) - 1;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct ZddId(u32);

impl ZddId {
    pub(super) const EMPTY: Self = Self(0);
    pub(super) const UNIT: Self = Self(1);
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct ZddNode {
    pub(super) variable: u32,
    pub(super) low: ZddId,
    pub(super) high: ZddId,
}

const _: () = assert!(size_of::<ZddNode>() == 12);

/// Explicit cumulative resource limits. Zero means unlimited in each field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ZddLimits {
    /// Owned nonterminal nodes; the two implicit terminals allocate no slots.
    pub(super) max_nodes: usize,
    pub(super) max_work: u64,
    pub(super) max_memo_entries: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ZddError {
    InvalidFamily,
    InvalidVariableOrder,
    InvalidGeometry,
    NodeLimit,
    MemoLimit,
    WorkLimit,
    Allocation,
    Cancelled,
    NodeIdExhausted,
    CountOverflow,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ZddMetrics {
    pub(super) apply_misses: u64,
    pub(super) unary_misses: u64,
    pub(super) created_nodes: u64,
    pub(super) count_nodes: u64,
    pub(super) compactions: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ZddCompaction {
    pub(super) previous_nodes: usize,
    pub(super) retained_nodes: usize,
    pub(super) epoch: u64,
    /// Includes old/new storage, mark/remap and traversal backing together.
    pub(super) peak_retained_bytes: usize,
}

struct WorkBudget {
    limits: ZddLimits,
    steps: u64,
    cancellation: Option<CancellationToken>,
    progress_sink: Option<Arc<dyn ProgressSink>>,
    progress_stage: &'static str,
}

impl WorkBudget {
    fn new(limits: ZddLimits, progress_stage: &'static str) -> Self {
        Self {
            limits,
            steps: 0,
            cancellation: None,
            progress_sink: None,
            progress_stage,
        }
    }

    fn check_cancelled(&self) -> Result<(), ZddError> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            Err(ZddError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn step(&mut self) -> Result<(), ZddError> {
        if self.steps & 1023 == 0 {
            self.check_cancelled()?;
        }
        if self.limits.max_work != 0 && self.steps >= self.limits.max_work {
            return Err(ZddError::WorkLimit);
        }
        self.steps = self.steps.saturating_add(1);
        if self.steps & WORK_PROGRESS_MASK == 0 {
            if let Some(sink) = &self.progress_sink {
                sink.report(ExecutionProgress::new(
                    self.progress_stage,
                    self.steps,
                    None,
                ));
            }
        }
        Ok(())
    }

    fn reserve_node(&self, current: usize) -> Result<(), ZddError> {
        if self.limits.max_nodes != 0 && current >= self.limits.max_nodes {
            Err(ZddError::NodeLimit)
        } else {
            Ok(())
        }
    }

    fn reserve_memo(&self, current: usize) -> Result<(), ZddError> {
        if self.limits.max_memo_entries != 0 && current >= self.limits.max_memo_entries {
            Err(ZddError::MemoLimit)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum BinaryOperation {
    Union,
    Intersection,
    Difference,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ApplyKey {
    operation: BinaryOperation,
    left: ZddId,
    right: ZddId,
}

impl ApplyKey {
    fn new(operation: BinaryOperation, mut left: ZddId, mut right: ZddId) -> Self {
        if operation != BinaryOperation::Difference && right < left {
            core::mem::swap(&mut left, &mut right);
        }
        Self {
            operation,
            left,
            right,
        }
    }
}

enum ApplyFrame {
    Visit(ApplyKey),
    Finish {
        key: ApplyKey,
        variable: u32,
        low: ApplyKey,
        high: ApplyKey,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum UnaryOperation {
    Add,
    Present,
    Absent,
    Remove,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct UnaryKey {
    operation: UnaryOperation,
    variable: u32,
    root: ZddId,
}

enum UnaryFrame {
    Visit(ZddId),
    Finish(ZddId, ZddNode),
}

enum NodeFrame {
    Visit(ZddId),
    Finish(ZddId, ZddNode),
}

pub(super) struct ZddArena {
    nodes: Vec<ZddNode>,
    unique: ExactHashMap<ZddNode, ZddId>,
    applied: ExactHashMap<ApplyKey, ZddId>,
    unary: ExactHashMap<UnaryKey, ZddId>,
    counts: ExactHashMap<ZddId, u128>,
    scratch_memo_entries: usize,
    budget: WorkBudget,
    metrics: ZddMetrics,
    epoch: u64,
}

impl ZddArena {
    pub(super) fn new(limits: ZddLimits) -> Self {
        Self {
            nodes: Vec::new(),
            unique: ExactHashMap::default(),
            applied: ExactHashMap::default(),
            unary: ExactHashMap::default(),
            counts: ExactHashMap::default(),
            scratch_memo_entries: 0,
            budget: WorkBudget::new(limits, "symbolic_work_zdd"),
            metrics: ZddMetrics::default(),
            epoch: 0,
        }
    }

    pub(super) fn set_cancellation(&mut self, cancellation: CancellationToken) {
        self.budget.cancellation = Some(cancellation);
    }

    pub(super) fn set_progress_sink(&mut self, progress_sink: Option<Arc<dyn ProgressSink>>) {
        self.budget.progress_sink = progress_sink;
    }

    pub(super) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(super) fn work_steps(&self) -> u64 {
        self.budget.steps
    }

    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(super) fn metrics(&self) -> ZddMetrics {
        self.metrics
    }

    /// Capacity-based payload accounting, excluding allocator metadata.
    pub(super) fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            .saturating_add(self.nodes.capacity().saturating_mul(size_of::<ZddNode>()))
            .saturating_add(map_bytes(&self.unique))
            .saturating_add(map_bytes(&self.applied))
            .saturating_add(map_bytes(&self.unary))
            .saturating_add(map_bytes(&self.counts))
    }

    /// Lets a fused caller charge its own iterative relation work and poll
    /// cancellation without inventing a second implicit work deadline.
    pub(super) fn checkpoint(&mut self) -> Result<(), ZddError> {
        self.budget.step()
    }

    pub(super) fn node(&self, root: ZddId) -> Result<Option<ZddNode>, ZddError> {
        if root.0 < 2 {
            return Ok(None);
        }
        self.nodes
            .get((root.0 - 2) as usize)
            .copied()
            .map(Some)
            .ok_or(ZddError::InvalidFamily)
    }

    /// Tests one strictly increasing member without allocating or changing
    /// arena state. Variables omitted from a ZDD branch are absent, including
    /// every remaining variable after UNIT. Invalid input order is rejected
    /// even when an earlier variable already rules out membership.
    pub(super) fn contains(
        &self,
        mut root: ZddId,
        variables: impl IntoIterator<Item = u32>,
    ) -> Result<bool, ZddError> {
        self.node(root)?;
        let mut previous = None;
        let mut possible = true;
        for variable in variables {
            if previous.is_some_and(|prior| prior >= variable) {
                return Err(ZddError::InvalidVariableOrder);
            }
            previous = Some(variable);
            if !possible {
                continue;
            }
            loop {
                match self.node(root)? {
                    Some(node) if node.variable < variable => root = node.low,
                    Some(node) if node.variable == variable => {
                        root = node.high;
                        break;
                    }
                    _ => {
                        possible = false;
                        break;
                    }
                }
            }
        }
        if !possible {
            return Ok(false);
        }
        while let Some(node) = self.node(root)? {
            root = node.low;
        }
        Ok(root == ZddId::UNIT)
    }

    /// High-empty reduction is the ZDD rule. Equal low/high branches must
    /// remain distinct: one branch contains `variable`, the other does not.
    pub(super) fn mk(&mut self, variable: u32, low: ZddId, high: ZddId) -> Result<ZddId, ZddError> {
        self.budget.check_cancelled()?;
        self.budget.step()?;
        let low_node = self.node(low)?;
        let high_node = self.node(high)?;
        if high == ZddId::EMPTY {
            return Ok(low);
        }
        if low_node.is_some_and(|node| node.variable <= variable)
            || high_node.is_some_and(|node| node.variable <= variable)
        {
            return Err(ZddError::InvalidVariableOrder);
        }
        let node = ZddNode {
            variable,
            low,
            high,
        };
        if let Some(root) = self.unique.get(&node).copied() {
            return Ok(root);
        }
        self.budget.reserve_node(self.nodes.len())?;
        let index = self
            .nodes
            .len()
            .checked_add(2)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(ZddError::NodeIdExhausted)?;
        self.nodes
            .try_reserve(1)
            .map_err(|_| ZddError::Allocation)?;
        self.unique
            .try_reserve(1)
            .map_err(|_| ZddError::Allocation)?;
        let root = ZddId(index);
        self.nodes.push(node);
        self.unique.insert(node, root);
        self.metrics.created_nodes = self.metrics.created_nodes.saturating_add(1);
        Ok(root)
    }

    pub(super) fn union(&mut self, left: ZddId, right: ZddId) -> Result<ZddId, ZddError> {
        self.apply(BinaryOperation::Union, left, right)
    }

    pub(super) fn intersection(&mut self, left: ZddId, right: ZddId) -> Result<ZddId, ZddError> {
        self.apply(BinaryOperation::Intersection, left, right)
    }

    pub(super) fn difference(&mut self, left: ZddId, right: ZddId) -> Result<ZddId, ZddError> {
        self.apply(BinaryOperation::Difference, left, right)
    }

    fn apply(
        &mut self,
        operation: BinaryOperation,
        left: ZddId,
        right: ZddId,
    ) -> Result<ZddId, ZddError> {
        self.budget.check_cancelled()?;
        self.node(left)?;
        self.node(right)?;
        let initial = ApplyKey::new(operation, left, right);
        let mut stack = Vec::new();
        push(&mut stack, ApplyFrame::Visit(initial))?;
        while let Some(frame) = stack.pop() {
            self.budget.step()?;
            match frame {
                ApplyFrame::Visit(key) => {
                    if self.applied_value(key).is_some() {
                        continue;
                    }
                    self.metrics.apply_misses = self.metrics.apply_misses.saturating_add(1);
                    let left_node = self.node(key.left)?;
                    let right_node = self.node(key.right)?;
                    let variable = match (left_node, right_node) {
                        (Some(left), Some(right)) => left.variable.min(right.variable),
                        (Some(node), None) | (None, Some(node)) => node.variable,
                        (None, None) => return Err(ZddError::InvalidFamily),
                    };
                    let (left_low, left_high) = split_at(key.left, left_node, variable);
                    let (right_low, right_high) = split_at(key.right, right_node, variable);
                    let low = ApplyKey::new(operation, left_low, right_low);
                    let high = ApplyKey::new(operation, left_high, right_high);
                    stack.try_reserve(3).map_err(|_| ZddError::Allocation)?;
                    stack.push(ApplyFrame::Finish {
                        key,
                        variable,
                        low,
                        high,
                    });
                    stack.push(ApplyFrame::Visit(high));
                    stack.push(ApplyFrame::Visit(low));
                }
                ApplyFrame::Finish {
                    key,
                    variable,
                    low,
                    high,
                } => {
                    let low = self.applied_value(low).ok_or(ZddError::InvalidFamily)?;
                    let high = self.applied_value(high).ok_or(ZddError::InvalidFamily)?;
                    let result = self.mk(variable, low, high)?;
                    self.reserve_memo()?;
                    self.applied
                        .try_reserve(1)
                        .map_err(|_| ZddError::Allocation)?;
                    self.applied.insert(key, result);
                }
            }
        }
        self.applied_value(initial).ok_or(ZddError::InvalidFamily)
    }

    fn applied_value(&self, key: ApplyKey) -> Option<ZddId> {
        let identity = match key.operation {
            BinaryOperation::Union => {
                if key.left == ZddId::EMPTY || key.left == key.right {
                    Some(key.right)
                } else if key.right == ZddId::EMPTY {
                    Some(key.left)
                } else {
                    None
                }
            }
            BinaryOperation::Intersection => {
                if key.left == ZddId::EMPTY || key.right == ZddId::EMPTY {
                    Some(ZddId::EMPTY)
                } else if key.left == key.right {
                    Some(key.left)
                } else {
                    None
                }
            }
            BinaryOperation::Difference => {
                if key.left == ZddId::EMPTY || key.left == key.right {
                    Some(ZddId::EMPTY)
                } else if key.right == ZddId::EMPTY {
                    Some(key.left)
                } else {
                    None
                }
            }
        };
        identity.or_else(|| self.applied.get(&key).copied())
    }

    /// Adds a variable to every member and merges coincident results.
    pub(super) fn add(&mut self, root: ZddId, variable: u32) -> Result<ZddId, ZddError> {
        self.apply_unary(root, variable, UnaryOperation::Add)
    }

    /// Filters membership while retaining the selected variable in each set.
    pub(super) fn select(
        &mut self,
        root: ZddId,
        variable: u32,
        present: bool,
    ) -> Result<ZddId, ZddError> {
        self.apply_unary(
            root,
            variable,
            if present {
                UnaryOperation::Present
            } else {
                UnaryOperation::Absent
            },
        )
    }

    /// Existentially removes one variable, not merely its high branch.
    pub(super) fn remove(&mut self, root: ZddId, variable: u32) -> Result<ZddId, ZddError> {
        self.apply_unary(root, variable, UnaryOperation::Remove)
    }

    fn apply_unary(
        &mut self,
        root: ZddId,
        variable: u32,
        operation: UnaryOperation,
    ) -> Result<ZddId, ZddError> {
        self.budget.check_cancelled()?;
        self.node(root)?;
        let mut stack = Vec::new();
        push(&mut stack, UnaryFrame::Visit(root))?;
        while let Some(frame) = stack.pop() {
            self.budget.step()?;
            let (current, finish) = match frame {
                UnaryFrame::Visit(current) => (current, None),
                UnaryFrame::Finish(current, node) => (current, Some(node)),
            };
            let key = UnaryKey {
                operation,
                variable,
                root: current,
            };
            if self.unary_value(key).is_some() {
                continue;
            }
            let result = if let Some(node) = finish {
                let low = self
                    .unary_value(UnaryKey {
                        root: node.low,
                        ..key
                    })
                    .ok_or(ZddError::InvalidFamily)?;
                let high = self
                    .unary_value(UnaryKey {
                        root: node.high,
                        ..key
                    })
                    .ok_or(ZddError::InvalidFamily)?;
                self.mk(node.variable, low, high)?
            } else {
                self.metrics.unary_misses = self.metrics.unary_misses.saturating_add(1);
                match self.node(current)? {
                    Some(node) if node.variable < variable => {
                        stack.try_reserve(3).map_err(|_| ZddError::Allocation)?;
                        stack.push(UnaryFrame::Finish(current, node));
                        stack.push(UnaryFrame::Visit(node.high));
                        stack.push(UnaryFrame::Visit(node.low));
                        continue;
                    }
                    Some(node) if node.variable == variable => match operation {
                        UnaryOperation::Add => {
                            let child = self.union(node.low, node.high)?;
                            self.mk(variable, ZddId::EMPTY, child)?
                        }
                        UnaryOperation::Present => self.mk(variable, ZddId::EMPTY, node.high)?,
                        UnaryOperation::Absent => node.low,
                        UnaryOperation::Remove => self.union(node.low, node.high)?,
                    },
                    _ => match operation {
                        UnaryOperation::Add => self.mk(variable, ZddId::EMPTY, current)?,
                        UnaryOperation::Present => ZddId::EMPTY,
                        UnaryOperation::Absent | UnaryOperation::Remove => current,
                    },
                }
            };
            self.reserve_memo()?;
            self.unary
                .try_reserve(1)
                .map_err(|_| ZddError::Allocation)?;
            self.unary.insert(key, result);
        }
        self.unary_value(UnaryKey {
            operation,
            variable,
            root,
        })
        .ok_or(ZddError::InvalidFamily)
    }

    fn unary_value(&self, key: UnaryKey) -> Option<ZddId> {
        if key.root == ZddId::EMPTY {
            Some(ZddId::EMPTY)
        } else {
            self.unary.get(&key).copied()
        }
    }

    /// Removes sorted, distinct variables in one DAG walk. Source-variable
    /// projection therefore does not scan the placement family once per bit.
    pub(super) fn project(&mut self, root: ZddId, removed: &[u32]) -> Result<ZddId, ZddError> {
        self.budget.check_cancelled()?;
        self.node(root)?;
        if removed.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ZddError::InvalidVariableOrder);
        }
        if removed.is_empty() {
            return Ok(root);
        }
        let previous_scratch = self.scratch_memo_entries;
        let result = self.project_inner(root, removed);
        self.scratch_memo_entries = previous_scratch;
        result
    }

    fn project_inner(&mut self, root: ZddId, removed: &[u32]) -> Result<ZddId, ZddError> {
        let mut results = ExactHashMap::default();
        let mut stack = Vec::new();
        push(&mut stack, NodeFrame::Visit(root))?;
        while let Some(frame) = stack.pop() {
            self.budget.step()?;
            match frame {
                NodeFrame::Visit(current) => {
                    if projected_value(&results, current).is_some() {
                        continue;
                    }
                    let node = self.node(current)?.ok_or(ZddError::InvalidFamily)?;
                    stack.try_reserve(3).map_err(|_| ZddError::Allocation)?;
                    stack.push(NodeFrame::Finish(current, node));
                    stack.push(NodeFrame::Visit(node.high));
                    stack.push(NodeFrame::Visit(node.low));
                }
                NodeFrame::Finish(current, node) => {
                    let low = projected_value(&results, node.low).ok_or(ZddError::InvalidFamily)?;
                    let high =
                        projected_value(&results, node.high).ok_or(ZddError::InvalidFamily)?;
                    let result = if removed.binary_search(&node.variable).is_ok() {
                        self.union(low, high)?
                    } else {
                        self.mk(node.variable, low, high)?
                    };
                    self.reserve_memo()?;
                    results.try_reserve(1).map_err(|_| ZddError::Allocation)?;
                    results.insert(current, result);
                    self.scratch_memo_entries += 1;
                }
            }
        }
        projected_value(&results, root).ok_or(ZddError::InvalidFamily)
    }

    pub(super) fn count(&mut self, root: ZddId) -> Result<u128, ZddError> {
        self.budget.check_cancelled()?;
        self.node(root)?;
        let mut stack = Vec::new();
        push(&mut stack, NodeFrame::Visit(root))?;
        while let Some(frame) = stack.pop() {
            self.budget.step()?;
            match frame {
                NodeFrame::Visit(current) => {
                    if self.counted(current).is_some() {
                        continue;
                    }
                    let node = self.node(current)?.ok_or(ZddError::InvalidFamily)?;
                    stack.try_reserve(3).map_err(|_| ZddError::Allocation)?;
                    stack.push(NodeFrame::Finish(current, node));
                    stack.push(NodeFrame::Visit(node.high));
                    stack.push(NodeFrame::Visit(node.low));
                }
                NodeFrame::Finish(current, node) => {
                    let low = self.counted(node.low).ok_or(ZddError::InvalidFamily)?;
                    let high = self.counted(node.high).ok_or(ZddError::InvalidFamily)?;
                    let count = low.checked_add(high).ok_or(ZddError::CountOverflow)?;
                    self.reserve_memo()?;
                    self.counts
                        .try_reserve(1)
                        .map_err(|_| ZddError::Allocation)?;
                    self.counts.insert(current, count);
                    self.metrics.count_nodes = self.metrics.count_nodes.saturating_add(1);
                }
            }
        }
        self.counted(root).ok_or(ZddError::InvalidFamily)
    }

    fn counted(&self, root: ZddId) -> Option<u128> {
        match root {
            ZddId::EMPTY => Some(0),
            ZddId::UNIT => Some(1),
            _ => self.counts.get(&root).copied(),
        }
    }

    /// Extracts at most `limit` sets in low-before-high order. Counted subtrees
    /// are skipped by rank; earlier members are never enumerated to reach an
    /// offset. This explicit output boundary is the only set enumeration API.
    pub(super) fn page(
        &mut self,
        root: ZddId,
        offset: u128,
        limit: usize,
    ) -> Result<Vec<Vec<u32>>, ZddError> {
        self.budget.check_cancelled()?;
        self.node(root)?;
        if limit == 0 {
            return Ok(Vec::new());
        }
        let total = self.count(root)?;
        if offset >= total {
            return Ok(Vec::new());
        }
        let requested = usize::try_from(total - offset).map_or(limit, |left| left.min(limit));
        let mut output = Vec::new();
        output
            .try_reserve_exact(requested)
            .map_err(|_| ZddError::Allocation)?;
        for index in 0..requested {
            let mut ordinal = offset + index as u128;
            let mut current = root;
            let mut selected = Vec::new();
            while let Some(node) = self.node(current)? {
                self.budget.step()?;
                let low_count = self.counted(node.low).ok_or(ZddError::InvalidFamily)?;
                if ordinal < low_count {
                    current = node.low;
                } else {
                    ordinal -= low_count;
                    push(&mut selected, node.variable)?;
                    current = node.high;
                }
            }
            if current != ZddId::UNIT || ordinal != 0 {
                return Err(ZddError::InvalidFamily);
            }
            output.push(selected);
        }
        Ok(output)
    }

    /// Compacts only at an explicit owner boundary. Pass every retained root
    /// and invalidate external node-ID caches after success. On failure the
    /// old arena and all supplied roots remain valid and unchanged.
    pub(super) fn compact(&mut self, roots: &mut [ZddId]) -> Result<ZddCompaction, ZddError> {
        self.budget.check_cancelled()?;
        for &root in roots.iter() {
            self.node(root)?;
        }
        let mut remap = Vec::new();
        remap
            .try_reserve_exact(self.nodes.len())
            .map_err(|_| ZddError::Allocation)?;
        remap.resize(self.nodes.len(), 0_u32);
        let mut stack = Vec::new();
        stack
            .try_reserve(roots.len())
            .map_err(|_| ZddError::Allocation)?;
        stack.extend_from_slice(roots);
        let mut live = 0_usize;
        while let Some(root) = stack.pop() {
            self.budget.step()?;
            if root.0 < 2 {
                continue;
            }
            let index = (root.0 - 2) as usize;
            if remap[index] != 0 {
                continue;
            }
            remap[index] = 1;
            live += 1;
            let node = self.nodes[index];
            stack.try_reserve(2).map_err(|_| ZddError::Allocation)?;
            stack.push(node.low);
            stack.push(node.high);
        }

        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(live)
            .map_err(|_| ZddError::Allocation)?;
        let mut unique = ExactHashMap::default();
        unique.try_reserve(live).map_err(|_| ZddError::Allocation)?;
        for index in 0..self.nodes.len() {
            self.budget.step()?;
            if remap[index] == 0 {
                continue;
            }
            let node = self.nodes[index];
            let node = ZddNode {
                variable: node.variable,
                low: remapped(&remap, node.low)?,
                high: remapped(&remap, node.high)?,
            };
            let root =
                ZddId(u32::try_from(nodes.len() + 2).map_err(|_| ZddError::NodeIdExhausted)?);
            remap[index] = root.0;
            nodes.push(node);
            unique.insert(node, root);
        }
        self.budget.check_cancelled()?;
        let peak_retained_bytes = self
            .retained_bytes()
            .saturating_add(remap.capacity().saturating_mul(size_of::<u32>()))
            .saturating_add(stack.capacity().saturating_mul(size_of::<ZddId>()))
            .saturating_add(nodes.capacity().saturating_mul(size_of::<ZddNode>()))
            .saturating_add(map_bytes(&unique));
        // All fallible work is complete before changing any public root.
        for root in roots.iter_mut() {
            if root.0 >= 2 {
                *root = ZddId(remap[(root.0 - 2) as usize]);
            }
        }
        let previous_nodes = self.nodes.len();
        self.nodes = nodes;
        self.unique = unique;
        self.applied = ExactHashMap::default();
        self.unary = ExactHashMap::default();
        self.counts = ExactHashMap::default();
        self.epoch = self.epoch.wrapping_add(1);
        self.metrics.compactions = self.metrics.compactions.saturating_add(1);
        Ok(ZddCompaction {
            previous_nodes,
            retained_nodes: live,
            epoch: self.epoch,
            peak_retained_bytes,
        })
    }

    fn reserve_memo(&self) -> Result<(), ZddError> {
        self.budget.reserve_memo(
            self.applied
                .len()
                .saturating_add(self.unary.len())
                .saturating_add(self.counts.len())
                .saturating_add(self.scratch_memo_entries),
        )
    }
}

fn split_at(root: ZddId, node: Option<ZddNode>, variable: u32) -> (ZddId, ZddId) {
    match node {
        Some(node) if node.variable == variable => (node.low, node.high),
        _ => (root, ZddId::EMPTY),
    }
}

fn projected_value(results: &ExactHashMap<ZddId, ZddId>, root: ZddId) -> Option<ZddId> {
    if root.0 < 2 {
        Some(root)
    } else {
        results.get(&root).copied()
    }
}

fn remapped(remap: &[u32], root: ZddId) -> Result<ZddId, ZddError> {
    if root.0 < 2 {
        return Ok(root);
    }
    match remap.get((root.0 - 2) as usize).copied() {
        Some(mapped) if mapped >= 2 => Ok(ZddId(mapped)),
        _ => Err(ZddError::InvalidFamily),
    }
}

fn push<T>(values: &mut Vec<T>, value: T) -> Result<(), ZddError> {
    values.try_reserve(1).map_err(|_| ZddError::Allocation)?;
    values.push(value);
    Ok(())
}

fn map_bytes<K, V>(map: &ExactHashMap<K, V>) -> usize {
    if map.capacity() == 0 {
        return 0;
    }
    // Swiss-table usable capacity excludes empty slots and control bytes.
    let buckets = map.capacity().saturating_mul(8).div_ceil(7);
    buckets
        .saturating_mul(size_of::<(K, V)>().saturating_add(1))
        .saturating_add(32)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum DownNodeKind {
    Append,
    Union,
    Product,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DownNode {
    kind: DownNodeKind,
    row: u32,
    left: u32,
    right: u32,
}

enum DerivativeFrame {
    Visit(u32),
    Finish(u32, DownNode),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DownFilterKey {
    selected: ZddId,
    geometry: u32,
}

enum DownFilterFrame {
    Visit(DownFilterKey),
    Finish {
        key: DownFilterKey,
        variable: u32,
        low: DownFilterKey,
        high: DownFilterKey,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct GeometryDownMetrics {
    pub(super) derivative_misses: u64,
    pub(super) filter_misses: u64,
    pub(super) created_nodes: u64,
    pub(super) overlapping_row_prunes: u64,
}

/// A lazy Down view of one immutable, compiler-certified geometry family.
///
/// The input must retain the compiler's no-repeated-row Append contract and
/// disjoint row domains at Product nodes. No canonical conversion, cover walk,
/// or per-node support table is built. A selected high branch requests only
/// D_r(F) = {C - {r}: C in F, r in C}; a selected low branch imposes no condition
/// on whether a completing cover contains r. Variables >= row_count are source
/// variables and pass through unchanged, preserving the joint relation.
///
/// Original node IDs belong to the borrowed family. Only newly needed
/// derivative expressions allocate here. Limits apply to this owned storage
/// and work, separately from the borrowed input and the caller's ZDD arena.
pub(super) struct GeometryDownView<'a> {
    original: &'a GeometrySolutionFamily,
    root: u32,
    row_count: u32,
    row_cells: Option<Vec<u64>>,
    first_generated: u32,
    generated: Vec<DownNode>,
    unique: ExactHashMap<DownNode, u32>,
    derivatives: ExactHashMap<(u32, u32), u32>,
    filter_memo_entries: usize,
    budget: WorkBudget,
    metrics: GeometryDownMetrics,
}

impl<'a> GeometryDownView<'a> {
    pub(super) fn new(
        original: &'a GeometrySolutionFamily,
        root: u32,
        row_count: u32,
        limits: ZddLimits,
    ) -> Result<Self, ZddError> {
        if root >= 2 && original.node(root).is_none() {
            return Err(ZddError::InvalidGeometry);
        }
        let first_generated = original
            .node_count()
            .checked_add(2)
            .ok_or(ZddError::NodeIdExhausted)?;
        Ok(Self {
            original,
            root,
            row_count,
            row_cells: None,
            first_generated,
            generated: Vec::new(),
            unique: ExactHashMap::default(),
            derivatives: ExactHashMap::default(),
            filter_memo_entries: 0,
            budget: WorkBudget::new(limits, "symbolic_work_down"),
            metrics: GeometryDownMetrics::default(),
        })
    }

    pub(super) fn set_cancellation(&mut self, cancellation: CancellationToken) {
        self.budget.cancellation = Some(cancellation);
    }

    pub(super) fn set_progress_sink(&mut self, progress_sink: Option<Arc<dyn ProgressSink>>) {
        self.budget.progress_sink = progress_sink;
    }

    /// Bind the original catalog's row masks once. In a valid exact cover,
    /// distinct overlapping placements cannot coexist. The derivative can
    /// reject such an Append without visiting its potentially huge suffix.
    pub(super) fn set_row_cells(&mut self, cells: &[u64]) -> Result<(), ZddError> {
        self.budget.check_cancelled()?;
        if cells.len() != self.row_count as usize || cells.iter().any(|mask| mask.count_ones() != 4)
        {
            return Err(ZddError::InvalidGeometry);
        }
        if let Some(existing) = &self.row_cells {
            return if existing == cells {
                Ok(())
            } else {
                Err(ZddError::InvalidGeometry)
            };
        }
        let mut stored = Vec::new();
        stored
            .try_reserve_exact(cells.len())
            .map_err(|_| ZddError::Allocation)?;
        stored.extend_from_slice(cells);
        self.row_cells = Some(stored);
        Ok(())
    }

    pub(super) fn generated_node_count(&self) -> usize {
        self.generated.len()
    }

    pub(super) fn work_steps(&self) -> u64 {
        self.budget.steps
    }

    pub(super) fn metrics(&self) -> GeometryDownMetrics {
        self.metrics
    }

    /// Excludes the borrowed immutable geometry arena.
    pub(super) fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            .saturating_add(
                self.generated
                    .capacity()
                    .saturating_mul(size_of::<DownNode>()),
            )
            .saturating_add(
                self.row_cells
                    .as_ref()
                    .map_or(0, |cells| cells.capacity().saturating_mul(size_of::<u64>())),
            )
            .saturating_add(map_bytes(&self.unique))
            .saturating_add(map_bytes(&self.derivatives))
    }

    /// No derivative ID escapes this owner. An explicit rank boundary can
    /// release all derivative scratch without changing any ZDD or source root.
    pub(super) fn clear_derivatives(&mut self) {
        self.generated = Vec::new();
        self.unique = ExactHashMap::default();
        self.derivatives = ExactHashMap::default();
    }

    pub(super) fn filter_extendible(
        &mut self,
        arena: &mut ZddArena,
        joint_root: ZddId,
    ) -> Result<ZddId, ZddError> {
        self.budget.check_cancelled()?;
        arena.budget.check_cancelled()?;
        arena.node(joint_root)?;
        let result = self.filter_inner(arena, joint_root);
        // The filter cache belongs to one call and one ZDD epoch. Derivatives
        // involve only immutable geometry IDs and survive ZDD compaction.
        self.filter_memo_entries = 0;
        result
    }

    fn filter_inner(&mut self, arena: &mut ZddArena, joint_root: ZddId) -> Result<ZddId, ZddError> {
        let initial = DownFilterKey {
            selected: joint_root,
            geometry: self.root,
        };
        let mut results = ExactHashMap::default();
        let mut stack = Vec::new();
        push(&mut stack, DownFilterFrame::Visit(initial))?;
        while let Some(frame) = stack.pop() {
            self.step(arena)?;
            match frame {
                DownFilterFrame::Visit(key) => {
                    if self.filtered_value(arena, &results, key)?.is_some() {
                        continue;
                    }
                    self.metrics.filter_misses = self.metrics.filter_misses.saturating_add(1);
                    let node = arena.node(key.selected)?.ok_or(ZddError::InvalidFamily)?;
                    let derivative = self.derivative(key.geometry, node.variable, arena)?;
                    let low = DownFilterKey {
                        selected: node.low,
                        geometry: key.geometry,
                    };
                    let high = DownFilterKey {
                        selected: node.high,
                        geometry: derivative,
                    };
                    stack.try_reserve(3).map_err(|_| ZddError::Allocation)?;
                    stack.push(DownFilterFrame::Finish {
                        key,
                        variable: node.variable,
                        low,
                        high,
                    });
                    stack.push(DownFilterFrame::Visit(high));
                    stack.push(DownFilterFrame::Visit(low));
                }
                DownFilterFrame::Finish {
                    key,
                    variable,
                    low,
                    high,
                } => {
                    let low = self
                        .filtered_value(arena, &results, low)?
                        .ok_or(ZddError::InvalidFamily)?;
                    let high = self
                        .filtered_value(arena, &results, high)?
                        .ok_or(ZddError::InvalidFamily)?;
                    let result = arena.mk(variable, low, high)?;
                    self.reserve_memo()?;
                    results.try_reserve(1).map_err(|_| ZddError::Allocation)?;
                    results.insert(key, result);
                    self.filter_memo_entries += 1;
                }
            }
        }
        self.filtered_value(arena, &results, initial)?
            .ok_or(ZddError::InvalidFamily)
    }

    fn filtered_value(
        &self,
        arena: &ZddArena,
        results: &ExactHashMap<DownFilterKey, ZddId>,
        key: DownFilterKey,
    ) -> Result<Option<ZddId>, ZddError> {
        if key.selected == ZddId::EMPTY || key.geometry == FAMILY_INVALID {
            return Ok(Some(ZddId::EMPTY));
        }
        if key.selected == ZddId::UNIT
            || arena
                .node(key.selected)?
                .is_some_and(|node| node.variable >= self.row_count)
        {
            return Ok(Some(key.selected));
        }
        Ok(results.get(&key).copied())
    }

    fn derivative(&mut self, root: u32, row: u32, arena: &mut ZddArena) -> Result<u32, ZddError> {
        if row >= self.row_count {
            return Err(ZddError::InvalidGeometry);
        }
        let mut stack = Vec::new();
        push(&mut stack, DerivativeFrame::Visit(root))?;
        while let Some(frame) = stack.pop() {
            self.step(arena)?;
            let (current, finish) = match frame {
                DerivativeFrame::Visit(current) => (current, None),
                DerivativeFrame::Finish(current, node) => (current, Some(node)),
            };
            if self.derived_value(current, row).is_some() {
                continue;
            }
            let result = if let Some(node) = finish {
                let left = self
                    .derived_value(node.left, row)
                    .ok_or(ZddError::InvalidGeometry)?;
                match node.kind {
                    DownNodeKind::Append => self.append(node.row, left)?,
                    DownNodeKind::Union => {
                        let right = self
                            .derived_value(node.right, row)
                            .ok_or(ZddError::InvalidGeometry)?;
                        self.union(left, right)?
                    }
                    DownNodeKind::Product => {
                        let right = self
                            .derived_value(node.right, row)
                            .ok_or(ZddError::InvalidGeometry)?;
                        // The compiler certifies disjoint row domains. A
                        // selected row cannot occur in both factors. Detect a
                        // violated contract when that row is first queried.
                        if left != FAMILY_INVALID && right != FAMILY_INVALID {
                            return Err(ZddError::InvalidGeometry);
                        }
                        let from_left = self.product(left, node.right)?;
                        let from_right = self.product(node.left, right)?;
                        self.union(from_left, from_right)?
                    }
                }
            } else {
                self.metrics.derivative_misses = self.metrics.derivative_misses.saturating_add(1);
                let node = self.node(current)?;
                if node.kind == DownNodeKind::Append && node.row == row {
                    node.left
                } else if node.kind == DownNodeKind::Append
                    && self
                        .row_cells
                        .as_ref()
                        .is_some_and(|cells| cells[node.row as usize] & cells[row as usize] != 0)
                {
                    self.metrics.overlapping_row_prunes =
                        self.metrics.overlapping_row_prunes.saturating_add(1);
                    FAMILY_INVALID
                } else {
                    let children = if node.kind == DownNodeKind::Append {
                        1
                    } else {
                        2
                    };
                    stack
                        .try_reserve(children + 1)
                        .map_err(|_| ZddError::Allocation)?;
                    stack.push(DerivativeFrame::Finish(current, node));
                    if children == 2 {
                        stack.push(DerivativeFrame::Visit(node.right));
                    }
                    stack.push(DerivativeFrame::Visit(node.left));
                    continue;
                }
            };
            self.reserve_memo()?;
            self.derivatives
                .try_reserve(1)
                .map_err(|_| ZddError::Allocation)?;
            self.derivatives.insert((current, row), result);
        }
        self.derived_value(root, row)
            .ok_or(ZddError::InvalidGeometry)
    }

    fn derived_value(&self, root: u32, row: u32) -> Option<u32> {
        if root == FAMILY_INVALID || root == FAMILY_EMPTY {
            Some(FAMILY_INVALID)
        } else {
            self.derivatives.get(&(root, row)).copied()
        }
    }

    fn node(&self, root: u32) -> Result<DownNode, ZddError> {
        let node = if root < self.first_generated {
            let node = self.original.node(root).ok_or(ZddError::InvalidGeometry)?;
            DownNode {
                kind: match node.kind {
                    FamilyNodeKind::Append => DownNodeKind::Append,
                    FamilyNodeKind::Union => DownNodeKind::Union,
                    FamilyNodeKind::Product => DownNodeKind::Product,
                },
                row: node.row_id,
                left: node.left,
                right: node.right,
            }
        } else {
            self.generated
                .get((root - self.first_generated) as usize)
                .copied()
                .ok_or(ZddError::InvalidGeometry)?
        };
        if node.kind == DownNodeKind::Append && node.row >= self.row_count {
            return Err(ZddError::InvalidGeometry);
        }
        Ok(node)
    }

    fn append(&mut self, row: u32, child: u32) -> Result<u32, ZddError> {
        if child == FAMILY_INVALID {
            return Ok(FAMILY_INVALID);
        }
        self.intern(DownNode {
            kind: DownNodeKind::Append,
            row,
            left: child,
            right: FAMILY_INVALID,
        })
    }

    fn union(&mut self, mut left: u32, mut right: u32) -> Result<u32, ZddError> {
        if left == FAMILY_INVALID {
            return Ok(right);
        }
        if right == FAMILY_INVALID || left == right {
            return Ok(left);
        }
        if right < left {
            core::mem::swap(&mut left, &mut right);
        }
        self.intern(DownNode {
            kind: DownNodeKind::Union,
            row: 0,
            left,
            right,
        })
    }

    fn product(&mut self, mut left: u32, mut right: u32) -> Result<u32, ZddError> {
        if left == FAMILY_INVALID || right == FAMILY_INVALID {
            return Ok(FAMILY_INVALID);
        }
        if left == FAMILY_EMPTY {
            return Ok(right);
        }
        if right == FAMILY_EMPTY {
            return Ok(left);
        }
        if right < left {
            core::mem::swap(&mut left, &mut right);
        }
        self.intern(DownNode {
            kind: DownNodeKind::Product,
            row: 0,
            left,
            right,
        })
    }

    fn intern(&mut self, node: DownNode) -> Result<u32, ZddError> {
        self.budget.step()?;
        if let Some(root) = self.unique.get(&node).copied() {
            return Ok(root);
        }
        self.budget.reserve_node(self.generated.len())?;
        let root = u32::try_from(self.generated.len())
            .ok()
            .and_then(|offset| self.first_generated.checked_add(offset))
            .ok_or(ZddError::NodeIdExhausted)?;
        self.generated
            .try_reserve(1)
            .map_err(|_| ZddError::Allocation)?;
        self.unique
            .try_reserve(1)
            .map_err(|_| ZddError::Allocation)?;
        self.generated.push(node);
        self.unique.insert(node, root);
        self.metrics.created_nodes = self.metrics.created_nodes.saturating_add(1);
        Ok(root)
    }

    fn step(&mut self, arena: &mut ZddArena) -> Result<(), ZddError> {
        self.budget.step()?;
        arena.checkpoint()
    }

    fn reserve_memo(&self) -> Result<(), ZddError> {
        self.budget.reserve_memo(
            self.derivatives
                .len()
                .saturating_add(self.filter_memo_entries),
        )
    }
}
