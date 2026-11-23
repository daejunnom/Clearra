//! Boolean predicates over global placement IDs for direct family evaluation.
//!
//! Variables are not limited to one machine word. Apply uses an explicit stack,
//! and IDs are scoped to their arena and checkpoint lifetime. This arena owns
//! predicates only; the ZDD arena preserves selected placements and source IDs.
//! The low ID bit complements an edge, so a function and its complement share
//! decision storage. Stored nodes always have an uncomplemented low edge.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use clearra_core_domain::execution_cancellation::{
    CancellationToken, ExecutionProgress, ProgressSink,
};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
const WORK_PROGRESS_MASK: u64 = (1 << 18) - 1;
const MAX_DECISION_NODES: usize = (u32::MAX >> 1) as usize;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct BooleanId(u32);

impl BooleanId {
    pub const FALSE: Self = Self(0);
    pub const TRUE: Self = Self(1);

    const fn complemented(self) -> Self {
        Self(self.0 ^ 1)
    }

    const fn is_complemented(self) -> bool {
        self.0 & 1 != 0
    }

    const fn regular(self) -> Self {
        Self(self.0 & !1)
    }

    const fn with_complement(self, complement: bool) -> Self {
        Self(self.0 ^ complement as u32)
    }

    fn node_index(self) -> Option<usize> {
        (self.0 >> 1).checked_sub(1).map(|index| index as usize)
    }

    fn from_node_index(index: usize) -> Result<Self, BooleanError> {
        if index >= MAX_DECISION_NODES {
            return Err(BooleanError::NodeLimit);
        }
        Ok(Self(((index as u32) + 1) << 1))
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct BooleanNode {
    pub variable: u32,
    pub low: BooleanId,
    pub high: BooleanId,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct BooleanLimits {
    /// Stored decision nodes plus the two logical terminals. Complemented
    /// references do not allocate nodes. Zero means no explicit admission bound;
    /// the 31-bit physical node address still returns NodeLimit on exhaustion.
    pub max_nodes: usize,
    pub max_work: u64,
    /// Per apply cache or temporary cofactor memo. Zero means unlimited.
    pub max_memo_entries: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BooleanError {
    Cancelled,
    Allocation,
    NodeLimit,
    WorkLimit,
    MemoLimit,
    CountOverflow,
    InvalidInput(&'static str),
    Unsupported(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct BooleanCheckpoint {
    owner: u64,
    node_count: usize,
}

// OR uses De Morgan's identity and therefore shares this AND-only cache.
type ApplyKey = (BooleanId, BooleanId);
type UniqueKey = (u32, BooleanId, BooleanId);

enum ApplyTask {
    Visit(ApplyKey),
    Finish(ApplyKey, u32),
}

enum RestrictTask {
    Visit(BooleanId),
    Finish(BooleanId, u32),
}

pub(super) struct BooleanArena {
    owner: u64,
    variable_count: u32,
    limits: BooleanLimits,
    cancellation: Option<CancellationToken>,
    secondary_cancellation: Option<CancellationToken>,
    progress_sink: Option<Arc<dyn ProgressSink>>,
    prefix: Option<Arc<FrozenBooleanPrefix>>,
    nodes: Vec<BooleanNode>,
    unique: HashMap<UniqueKey, BooleanId>,
    applied: HashMap<ApplyKey, BooleanId>,
    read_only: bool,
    work: u64,
    apply_hits: u64,
    peak_nodes: usize,
}

/// One rank's immutable storage, moved rather than copied from its owner.
/// Fork IDs share this prefix but their local suffix IDs remain owner-scoped.
pub(super) struct FrozenBooleanPrefix {
    owner: u64,
    variable_count: u32,
    nodes: Vec<BooleanNode>,
    unique: HashMap<UniqueKey, BooleanId>,
    applied: HashMap<ApplyKey, BooleanId>,
}

impl FrozenBooleanPrefix {
    pub fn node_count(&self) -> usize {
        self.nodes.len() + 2
    }

    pub fn retained_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<BooleanNode>()
            + self.unique.capacity() * (core::mem::size_of::<(UniqueKey, BooleanId)>() + 1)
            + self.applied.capacity() * (core::mem::size_of::<(ApplyKey, BooleanId)>() + 1)
    }
}

impl BooleanArena {
    pub fn new(variable_count: u32, limits: BooleanLimits) -> Result<Self, BooleanError> {
        if variable_count == u32::MAX || (limits.max_nodes != 0 && limits.max_nodes < 2) {
            return Err(BooleanError::InvalidInput("boolean_variable_or_node_limit"));
        }
        let owner = NEXT_OWNER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| BooleanError::CountOverflow)?;
        Ok(Self {
            owner,
            variable_count,
            limits,
            cancellation: None,
            secondary_cancellation: None,
            progress_sink: None,
            prefix: None,
            nodes: Vec::new(),
            unique: HashMap::new(),
            applied: HashMap::new(),
            read_only: false,
            work: 0,
            apply_hits: 0,
            peak_nodes: 2,
        })
    }

    pub fn set_cancellation(&mut self, token: CancellationToken) {
        self.cancellation = Some(token);
    }

    pub fn set_secondary_cancellation(&mut self, token: CancellationToken) {
        self.secondary_cancellation = Some(token);
    }

    pub fn set_progress_sink(&mut self, progress_sink: Option<Arc<dyn ProgressSink>>) {
        self.progress_sink = progress_sink;
    }

    pub const fn owner_id(&self) -> u64 {
        self.owner
    }

    /// Accept only this owner or the exact immutable prefix shared by a fork.
    /// This is for board inputs; a relation's local roots still need an exact
    /// owner match and must never be interpreted by a sibling fork.
    pub fn can_read_owner(&self, owner: u64) -> bool {
        self.owner == owner
            || self
                .prefix
                .as_ref()
                .is_some_and(|prefix| prefix.owner == owner)
    }

    pub fn has_unlimited_representation_bounds(&self) -> bool {
        self.limits.max_nodes == 0 && self.limits.max_work == 0 && self.limits.max_memo_entries == 0
    }

    /// Move the backing allocations into one shared prefix. No node, interner
    /// or apply-table cloning occurs. The caller must thaw before rewinding
    /// below this boundary, after every fork and borrowed result is finished.
    pub fn freeze_prefix(&mut self) -> Result<Arc<FrozenBooleanPrefix>, BooleanError> {
        self.check_writable()?;
        if !self.has_unlimited_representation_bounds() {
            return Err(BooleanError::Unsupported(
                "boolean_parallel_explicit_bounds",
            ));
        }
        if self.prefix.is_some() {
            return Err(BooleanError::InvalidInput("boolean_prefix_already_frozen"));
        }
        let prefix = Arc::new(FrozenBooleanPrefix {
            owner: self.owner,
            variable_count: self.variable_count,
            nodes: core::mem::take(&mut self.nodes),
            unique: core::mem::take(&mut self.unique),
            applied: core::mem::take(&mut self.applied),
        });
        self.prefix = Some(Arc::clone(&prefix));
        Ok(prefix)
    }

    pub fn fork_prefix(prefix: Arc<FrozenBooleanPrefix>) -> Result<Self, BooleanError> {
        let owner = NEXT_OWNER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| BooleanError::CountOverflow)?;
        Ok(Self {
            owner,
            variable_count: prefix.variable_count,
            limits: BooleanLimits::default(),
            cancellation: None,
            secondary_cancellation: None,
            progress_sink: None,
            peak_nodes: prefix.node_count(),
            prefix: Some(prefix),
            nodes: Vec::new(),
            unique: HashMap::new(),
            applied: HashMap::new(),
            read_only: false,
            work: 0,
            apply_hits: 0,
        })
    }

    pub fn thaw_prefix(&mut self) -> Result<(), BooleanError> {
        if self.read_only {
            return Err(BooleanError::InvalidInput("boolean_arena_sealed"));
        }
        if !self.nodes.is_empty() || !self.unique.is_empty() || !self.applied.is_empty() {
            return Err(BooleanError::InvalidInput("boolean_frozen_owner_modified"));
        }
        let prefix = self
            .prefix
            .take()
            .ok_or(BooleanError::InvalidInput("boolean_prefix_not_frozen"))?;
        if prefix.owner != self.owner {
            self.prefix = Some(prefix);
            return Err(BooleanError::InvalidInput("boolean_prefix_thaw_owner"));
        }
        let prefix = match Arc::try_unwrap(prefix) {
            Ok(prefix) => prefix,
            Err(prefix) => {
                self.prefix = Some(prefix);
                return Err(BooleanError::InvalidInput("boolean_prefix_still_borrowed"));
            }
        };
        self.nodes = prefix.nodes;
        self.unique = prefix.unique;
        self.applied = prefix.applied;
        Ok(())
    }

    /// Fold actual fork work into this logical Boolean owner, without claiming
    /// that disjoint private node peaks occurred simultaneously.
    pub fn record_fork_work(&mut self, work: u64, hits: u64) -> Result<(), BooleanError> {
        if self.read_only {
            return Err(BooleanError::InvalidInput("boolean_arena_sealed"));
        }
        if !self.has_unlimited_representation_bounds() {
            return Err(BooleanError::Unsupported(
                "boolean_parallel_explicit_bounds",
            ));
        }
        self.work = self
            .work
            .checked_add(work)
            .ok_or(BooleanError::CountOverflow)?;
        self.apply_hits = self.apply_hits.saturating_add(hits);
        Ok(())
    }

    fn prefix_node_count(&self) -> usize {
        self.prefix.as_ref().map_or(0, |prefix| prefix.nodes.len())
    }

    fn stored_node(&self, root: BooleanId) -> Option<BooleanNode> {
        let Some(index) = root.node_index() else {
            return Some(BooleanNode {
                variable: u32::MAX,
                low: root,
                high: root,
            });
        };
        let prefix_count = self.prefix_node_count();
        let mut node = if index < prefix_count {
            self.prefix.as_ref()?.nodes.get(index).copied()
        } else {
            self.nodes.get(index - prefix_count).copied()
        }?;
        if root.is_complemented() {
            node.low = node.low.complemented();
            node.high = node.high.complemented();
        }
        Some(node)
    }

    fn cached_apply(&self, key: &ApplyKey) -> Option<BooleanId> {
        self.applied.get(key).copied().or_else(|| {
            self.prefix
                .as_ref()
                .and_then(|prefix| prefix.applied.get(key).copied())
        })
    }

    pub fn node(&self, root: BooleanId) -> Option<BooleanNode> {
        (root.0 >= 2).then(|| self.stored_node(root)).flatten()
    }

    pub fn validate(&self, root: BooleanId) -> Result<(), BooleanError> {
        if root
            .node_index()
            .is_none_or(|index| index < self.prefix_node_count() + self.nodes.len())
        {
            Ok(())
        } else {
            Err(BooleanError::InvalidInput("boolean_root_outside_arena"))
        }
    }

    pub fn node_count(&self) -> usize {
        self.prefix_node_count() + self.nodes.len() + 2
    }

    pub const fn peak_node_count(&self) -> usize {
        self.peak_nodes
    }

    pub const fn work_steps(&self) -> u64 {
        self.work
    }

    pub const fn apply_cache_hits(&self) -> u64 {
        self.apply_hits
    }

    /// Estimated retained payload, not allocator backing, RSS or a memory bound.
    /// HashMap capacity omits spare buckets, controls and alignment. Temporary
    /// apply/cofactor stacks and the cofactor memo also contribute to peak use.
    pub fn retained_bytes(&self) -> usize {
        self.private_retained_bytes()
            + self
                .prefix
                .as_ref()
                .map_or(0, |prefix| prefix.retained_bytes())
    }

    /// Estimated owned suffix payload only. Count a shared prefix once when
    /// aggregating workers; this excludes transient apply stacks and RSS.
    pub fn private_retained_bytes(&self) -> usize {
        self.private_nodes_retained_bytes()
            + self.private_unique_retained_bytes()
            + self.private_apply_retained_bytes()
    }

    /// Physical decision nodes owned here, excluding a shared prefix and the
    /// two logical terminals, which have no stored node payload.
    pub fn private_node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn private_nodes_retained_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<BooleanNode>()
    }

    pub fn private_unique_retained_bytes(&self) -> usize {
        self.unique.capacity() * (core::mem::size_of::<(UniqueKey, BooleanId)>() + 1)
    }

    pub fn private_apply_retained_bytes(&self) -> usize {
        self.applied.capacity() * (core::mem::size_of::<(ApplyKey, BooleanId)>() + 1)
    }

    /// Release private construction indexes once all published roots are final.
    /// Nodes, IDs, owner, shared prefix, work and peak counters remain valid.
    /// Later construction is refused; this is not an evict-and-recompute cache.
    /// Returns released payload estimate, not an observed RSS reduction.
    pub fn seal_for_reading(&mut self) -> Result<usize, BooleanError> {
        self.check_cancelled()?;
        if self.read_only {
            return Ok(0);
        }
        let released = self.private_unique_retained_bytes() + self.private_apply_retained_bytes();
        self.unique = HashMap::new();
        self.applied = HashMap::new();
        self.read_only = true;
        self.report_private_storage();
        Ok(released)
    }

    pub fn checkpoint(&self) -> BooleanCheckpoint {
        BooleanCheckpoint {
            owner: self.owner,
            node_count: self.node_count(),
        }
    }

    /// The caller has finished every predicate allocated after this checkpoint.
    /// Persistent occupancy roots keep their IDs; intermediate memo IDs do not.
    /// Backing capacities remain allocated for reuse; this is logical reclamation.
    pub fn rewind(&mut self, checkpoint: BooleanCheckpoint) -> Result<(), BooleanError> {
        // Cancellation must not prevent the caller's existing cleanup path.
        if self.read_only {
            return Err(BooleanError::InvalidInput("boolean_arena_sealed"));
        }
        if checkpoint.owner != self.owner
            || checkpoint.node_count > self.node_count()
            || checkpoint.node_count < self.prefix_node_count() + 2
        {
            return Err(BooleanError::InvalidInput("boolean_checkpoint_owner"));
        }
        while self.node_count() > checkpoint.node_count {
            let node = self.nodes.pop().expect("checkpoint length checked");
            self.unique.remove(&(node.variable, node.low, node.high));
        }
        self.applied.clear();
        Ok(())
    }

    pub fn variable(&mut self, variable: u32) -> Result<BooleanId, BooleanError> {
        self.mk(variable, BooleanId::FALSE, BooleanId::TRUE)
    }

    pub fn mk(
        &mut self,
        variable: u32,
        low: BooleanId,
        high: BooleanId,
    ) -> Result<BooleanId, BooleanError> {
        self.check_writable()?;
        self.tick()?;
        self.validate(low)?;
        self.validate(high)?;
        if variable >= self.variable_count {
            return Err(BooleanError::InvalidInput("boolean_variable_outside_arena"));
        }
        if low == high {
            return Ok(low);
        }
        if self.stored_node(low).expect("validated low root").variable <= variable
            || self
                .stored_node(high)
                .expect("validated high root")
                .variable
                <= variable
        {
            return Err(BooleanError::InvalidInput("boolean_variable_order"));
        }
        // Negating both branches negates the function. Keep just the form
        // whose low edge is regular, and carry the output polarity in its ID.
        let complemented = low.is_complemented();
        let low = low.with_complement(complemented);
        let high = high.with_complement(complemented);
        let key = (variable, low, high);
        if let Some(&root) = self.unique.get(&key) {
            return Ok(root.with_complement(complemented));
        }
        if let Some(root) = self
            .prefix
            .as_ref()
            .and_then(|prefix| prefix.unique.get(&key))
        {
            return Ok(root.with_complement(complemented));
        }
        if self.limits.max_nodes != 0 && self.node_count() >= self.limits.max_nodes {
            return Err(BooleanError::NodeLimit);
        }
        let root = BooleanId::from_node_index(self.node_count() - 2)?;
        self.nodes
            .try_reserve(1)
            .map_err(|_| BooleanError::Allocation)?;
        self.unique
            .try_reserve(1)
            .map_err(|_| BooleanError::Allocation)?;
        self.nodes.push(BooleanNode {
            variable,
            low,
            high,
        });
        self.unique.insert(key, root);
        self.peak_nodes = self.peak_nodes.max(self.node_count());
        Ok(root.with_complement(complemented))
    }

    pub fn and(&mut self, left: BooleanId, right: BooleanId) -> Result<BooleanId, BooleanError> {
        self.apply(left, right)
    }

    pub fn or(&mut self, left: BooleanId, right: BooleanId) -> Result<BooleanId, BooleanError> {
        self.apply(left.complemented(), right.complemented())
            .map(BooleanId::complemented)
    }

    pub fn not(&mut self, root: BooleanId) -> Result<BooleanId, BooleanError> {
        self.check_writable()?;
        self.tick()?;
        self.validate(root)?;
        Ok(root.complemented())
    }

    pub fn ite(
        &mut self,
        condition: BooleanId,
        when_true: BooleanId,
        when_false: BooleanId,
    ) -> Result<BooleanId, BooleanError> {
        self.check_writable()?;
        self.tick()?;
        self.validate(condition)?;
        self.validate(when_true)?;
        self.validate(when_false)?;
        if condition == BooleanId::TRUE || when_true == when_false {
            return Ok(when_true);
        }
        if condition == BooleanId::FALSE {
            return Ok(when_false);
        }
        let positive = self.and(condition, when_true)?;
        let negative_condition = self.not(condition)?;
        let negative = self.and(negative_condition, when_false)?;
        self.or(positive, negative)
    }

    pub fn cofactor(
        &mut self,
        root: BooleanId,
        variable: u32,
        value: bool,
    ) -> Result<BooleanId, BooleanError> {
        self.check_writable()?;
        self.tick()?;
        self.validate(root)?;
        if variable >= self.variable_count {
            return Err(BooleanError::InvalidInput("boolean_cofactor_variable"));
        }
        let node = self.stored_node(root).expect("validated cofactor root");
        if node.variable > variable {
            return Ok(root);
        }
        if node.variable == variable {
            return Ok(if value { node.high } else { node.low });
        }
        let mut tasks = Vec::new();
        let mut values = Vec::new();
        let mut memo = HashMap::new();
        tasks.try_reserve(1).map_err(|_| BooleanError::Allocation)?;
        tasks.push(RestrictTask::Visit(root));
        while let Some(task) = tasks.pop() {
            self.tick()?;
            match task {
                RestrictTask::Visit(id) => {
                    let regular = id.regular();
                    let node = self.stored_node(regular).expect("cofactor child in arena");
                    let known = if node.variable > variable {
                        Some(id)
                    } else if node.variable == variable {
                        Some(
                            (if value { node.high } else { node.low })
                                .with_complement(id.is_complemented()),
                        )
                    } else {
                        memo.get(&regular)
                            .copied()
                            .map(|root: BooleanId| root.with_complement(id.is_complemented()))
                    };
                    if let Some(known) = known {
                        values
                            .try_reserve(1)
                            .map_err(|_| BooleanError::Allocation)?;
                        values.push(known);
                    } else {
                        tasks.try_reserve(3).map_err(|_| BooleanError::Allocation)?;
                        tasks.push(RestrictTask::Finish(id, node.variable));
                        tasks.push(RestrictTask::Visit(node.high));
                        tasks.push(RestrictTask::Visit(node.low));
                    }
                }
                RestrictTask::Finish(id, node_variable) => {
                    let high = values.pop().expect("cofactor high completed");
                    let low = values.pop().expect("cofactor low completed");
                    let result = self.mk(node_variable, low, high)?;
                    let regular = id.regular();
                    if !memo.contains_key(&regular) {
                        self.check_memo_room(memo.len())?;
                        memo.try_reserve(1).map_err(|_| BooleanError::Allocation)?;
                    }
                    memo.insert(regular, result);
                    values.push(result.with_complement(id.is_complemented()));
                }
            }
        }
        values
            .pop()
            .ok_or(BooleanError::InvalidInput("boolean_cofactor_result"))
    }

    #[cfg(test)]
    pub fn evaluate(&self, mut root: BooleanId, selected: impl Fn(u32) -> bool) -> bool {
        while let Some(node) = self.node(root) {
            root = if selected(node.variable) {
                node.high
            } else {
                node.low
            };
        }
        root == BooleanId::TRUE
    }

    fn check_cancelled(&self) -> Result<(), BooleanError> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
            || self
                .secondary_cancellation
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
        {
            return Err(BooleanError::Cancelled);
        }
        Ok(())
    }

    fn check_writable(&self) -> Result<(), BooleanError> {
        self.check_cancelled()?;
        if self.read_only {
            return Err(BooleanError::InvalidInput("boolean_arena_sealed"));
        }
        Ok(())
    }

    fn check_memo_room(&self, entries: usize) -> Result<(), BooleanError> {
        if self.limits.max_memo_entries != 0 && entries >= self.limits.max_memo_entries {
            return Err(BooleanError::MemoLimit);
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), BooleanError> {
        if self.work & 255 == 0 {
            self.check_cancelled()?;
        }
        if self.limits.max_work != 0 && self.work >= self.limits.max_work {
            return Err(BooleanError::WorkLimit);
        }
        self.work = self
            .work
            .checked_add(1)
            .ok_or(BooleanError::CountOverflow)?;
        if self.work & WORK_PROGRESS_MASK == 0 {
            if let Some(sink) = &self.progress_sink {
                sink.report(ExecutionProgress::new(
                    "symbolic_work_boolean",
                    self.work,
                    None,
                ));
            }
            self.report_private_storage();
        }
        Ok(())
    }

    fn report_private_storage(&self) {
        if let Some(sink) = &self.progress_sink {
            sink.report(ExecutionProgress::new(
                "symbolic_storage_boolean_private_nodes",
                self.private_node_count() as u64,
                None,
            ));
            sink.report(ExecutionProgress::new(
                "symbolic_storage_boolean_private_bytes",
                self.private_retained_bytes() as u64,
                None,
            ));
        }
    }

    fn normalize(left: BooleanId, right: BooleanId) -> ApplyKey {
        if left > right {
            (right, left)
        } else {
            (left, right)
        }
    }

    fn trivial((left, right): ApplyKey) -> Option<BooleanId> {
        if left == BooleanId::FALSE || left == right.complemented() {
            Some(BooleanId::FALSE)
        } else if left == BooleanId::TRUE || left == right {
            Some(right)
        } else {
            None
        }
    }

    fn apply(&mut self, left: BooleanId, right: BooleanId) -> Result<BooleanId, BooleanError> {
        self.check_writable()?;
        self.tick()?;
        self.validate(left)?;
        self.validate(right)?;
        let initial = Self::normalize(left, right);
        if let Some(root) = Self::trivial(initial) {
            return Ok(root);
        }
        if let Some(root) = self.cached_apply(&initial) {
            self.apply_hits = self.apply_hits.saturating_add(1);
            return Ok(root);
        }
        self.check_memo_room(self.applied.len())?;
        let mut tasks = Vec::new();
        let mut values = Vec::new();
        tasks.try_reserve(1).map_err(|_| BooleanError::Allocation)?;
        tasks.push(ApplyTask::Visit(initial));
        while let Some(task) = tasks.pop() {
            self.tick()?;
            match task {
                ApplyTask::Visit(key) => {
                    if let Some(root) = Self::trivial(key) {
                        values
                            .try_reserve(1)
                            .map_err(|_| BooleanError::Allocation)?;
                        values.push(root);
                        continue;
                    }
                    if let Some(root) = self.cached_apply(&key) {
                        values
                            .try_reserve(1)
                            .map_err(|_| BooleanError::Allocation)?;
                        values.push(root);
                        self.apply_hits = self.apply_hits.saturating_add(1);
                        continue;
                    }
                    self.check_memo_room(self.applied.len())?;
                    let (left, right) = key;
                    let left_node = self.stored_node(left).expect("apply left in arena");
                    let right_node = self.stored_node(right).expect("apply right in arena");
                    let variable = left_node.variable.min(right_node.variable);
                    let (left_low, left_high) = if left_node.variable == variable {
                        (left_node.low, left_node.high)
                    } else {
                        (left, left)
                    };
                    let (right_low, right_high) = if right_node.variable == variable {
                        (right_node.low, right_node.high)
                    } else {
                        (right, right)
                    };
                    tasks.try_reserve(3).map_err(|_| BooleanError::Allocation)?;
                    tasks.push(ApplyTask::Finish(key, variable));
                    tasks.push(ApplyTask::Visit(Self::normalize(left_high, right_high)));
                    tasks.push(ApplyTask::Visit(Self::normalize(left_low, right_low)));
                }
                ApplyTask::Finish(key, variable) => {
                    let high = values.pop().expect("apply high completed");
                    let low = values.pop().expect("apply low completed");
                    let root = self.mk(variable, low, high)?;
                    if !self.applied.contains_key(&key) {
                        self.check_memo_room(self.applied.len())?;
                        self.applied
                            .try_reserve(1)
                            .map_err(|_| BooleanError::Allocation)?;
                    }
                    self.applied.insert(key, root);
                    values.push(root);
                }
            }
        }
        values
            .pop()
            .ok_or(BooleanError::InvalidInput("boolean_apply_result"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn truth_table_root(arena: &mut BooleanArena, truth: u8) -> BooleanId {
        fn compile(
            arena: &mut BooleanArena,
            truth: u8,
            variable: u32,
            assignment: u32,
        ) -> BooleanId {
            if variable == 3 {
                return if truth & (1 << assignment) == 0 {
                    BooleanId::FALSE
                } else {
                    BooleanId::TRUE
                };
            }
            let low = compile(arena, truth, variable + 1, assignment);
            let high = compile(arena, truth, variable + 1, assignment | (1 << variable));
            arena.mk(variable, low, high).unwrap()
        }
        compile(arena, truth, 0, 0)
    }

    #[test]
    fn symbolic_boolean_complemented_edges_share_storage_and_obey_all_truth_tables() {
        let mut arena = BooleanArena::new(3, BooleanLimits::default()).unwrap();
        let roots: Vec<_> = (0..=u8::MAX)
            .map(|truth| truth_table_root(&mut arena, truth))
            .collect();
        // Each pair of opposite functions has one ID polarity, not a second
        // decision graph. A semantic node read applies that polarity to both
        // children; callers need no awareness of its stored representation.
        for truth in 0..=u8::MAX {
            let root = roots[usize::from(truth)];
            let before_nodes = arena.node_count();
            let before_work = arena.work_steps();
            let before_memo = arena.applied.len();
            let opposite = arena.not(root).unwrap();
            assert_eq!(opposite, roots[usize::from(!truth)]);
            assert_eq!(arena.node_count(), before_nodes);
            assert_eq!(arena.work_steps(), before_work + 1);
            assert_eq!(arena.applied.len(), before_memo);
            for assignment in 0..8_u32 {
                assert_eq!(
                    arena.evaluate(root, |variable| assignment & (1 << variable) != 0),
                    truth & (1 << assignment) != 0,
                );
            }
            for variable in 0..3 {
                for value in [false, true] {
                    let restricted = arena.cofactor(root, variable, value).unwrap();
                    for assignment in 0..8_u32 {
                        let fixed = if value {
                            assignment | (1 << variable)
                        } else {
                            assignment & !(1 << variable)
                        };
                        assert_eq!(
                            arena.evaluate(restricted, |v| assignment & (1 << v) != 0),
                            truth & (1 << fixed) != 0,
                        );
                    }
                }
            }
        }
        // Exhaustive Boolean combinations over two-variable functions are a
        // finite semantic oracle, including complemented operands and roots.
        let two_variable = |truth: u8| roots[usize::from(truth | (truth << 4))];
        for left in 0..16_u8 {
            for right in 0..16_u8 {
                let a = two_variable(left);
                let b = two_variable(right);
                assert_eq!(arena.and(a, b).unwrap(), two_variable(left & right));
                assert_eq!(arena.or(a, b).unwrap(), two_variable(left | right));
                for condition in 0..16_u8 {
                    assert_eq!(
                        arena.ite(two_variable(condition), a, b).unwrap(),
                        two_variable((condition & left) | ((!condition & 15) & right)),
                    );
                }
            }
        }
        assert!(arena.nodes.iter().all(|node| !node.low.is_complemented()));
    }

    #[test]
    fn symbolic_boolean_complemented_limits_count_storage_and_not_is_one_step() {
        let mut arena = BooleanArena::new(
            2,
            BooleanLimits {
                max_nodes: 3,
                max_work: 5,
                max_memo_entries: 1,
            },
        )
        .unwrap();
        let a = arena.variable(0).unwrap();
        let opposite = arena.not(a).unwrap();
        assert_eq!(arena.node_count(), 3);
        assert_eq!(arena.private_node_count(), 1);
        assert_eq!(
            arena.mk(0, BooleanId::TRUE, BooleanId::FALSE).unwrap(),
            opposite,
        );
        assert_eq!(arena.variable(1), Err(BooleanError::NodeLimit));
        assert_eq!(arena.not(opposite).unwrap(), a);
        assert_eq!(arena.work_steps(), 5);
        assert_eq!(arena.not(a), Err(BooleanError::WorkLimit));
        assert_eq!(arena.work_steps(), 5);
        assert_eq!(
            BooleanId::from_node_index(MAX_DECISION_NODES),
            Err(BooleanError::NodeLimit),
        );
        let last = BooleanId::from_node_index(MAX_DECISION_NODES - 1).unwrap();
        assert_eq!(last.node_index(), Some(MAX_DECISION_NODES - 1));
        assert_eq!(last.complemented().node_index(), last.node_index());
        assert!(arena.validate(last).is_err());
        assert!(arena.validate(last.complemented()).is_err());
    }

    #[test]
    fn symbolic_boolean_sealed_fork_keeps_roots_and_releases_only_private_indexes() {
        let mut owner = BooleanArena::new(4, BooleanLimits::default()).unwrap();
        let a = owner.variable(1).unwrap();
        let b = owner.variable(3).unwrap();
        let shared = owner.and(a, b).unwrap();
        let prefix = owner.freeze_prefix().unwrap();
        let prefix_bytes = prefix.retained_bytes();
        let mut fork = BooleanArena::fork_prefix(Arc::clone(&prefix)).unwrap();
        let sibling = BooleanArena::fork_prefix(Arc::clone(&prefix)).unwrap();
        let c = fork.variable(0).unwrap();
        let root = fork.or(shared, c).unwrap();
        let opposite = fork.not(root).unwrap();
        let checkpoint = fork.checkpoint();
        let owner_id = fork.owner_id();
        let node_count = fork.node_count();
        let node_bytes = fork.private_nodes_retained_bytes();
        let index_bytes =
            fork.private_unique_retained_bytes() + fork.private_apply_retained_bytes();
        let work = fork.work_steps();
        let peak = fork.peak_node_count();
        assert!(index_bytes > 0);
        assert_eq!(fork.seal_for_reading().unwrap(), index_bytes);
        assert_eq!(fork.seal_for_reading().unwrap(), 0);
        assert_eq!(fork.private_retained_bytes(), node_bytes);
        assert_eq!(fork.private_unique_retained_bytes(), 0);
        assert_eq!(fork.private_apply_retained_bytes(), 0);
        assert_eq!(prefix.retained_bytes(), prefix_bytes);
        assert_eq!(fork.retained_bytes(), node_bytes + prefix_bytes);
        assert_eq!(fork.node_count(), node_count);
        assert_eq!(fork.owner_id(), owner_id);
        assert_eq!(fork.work_steps(), work);
        assert_eq!(fork.peak_node_count(), peak);
        assert!(fork.can_read_owner(owner.owner_id()));
        assert!(!fork.can_read_owner(sibling.owner_id()));
        for assignment in 0..16_u32 {
            let selected = |variable: u32| assignment & (1_u32 << variable) != 0;
            let expected = (selected(1) && selected(3)) || selected(0);
            assert_eq!(fork.evaluate(root, selected), expected);
            assert_eq!(fork.evaluate(opposite, selected), !expected);
        }
        let sealed = BooleanError::InvalidInput("boolean_arena_sealed");
        assert_eq!(fork.variable(2), Err(sealed));
        assert_eq!(fork.mk(0, a, b), Err(sealed));
        assert_eq!(fork.not(root), Err(sealed));
        assert_eq!(fork.and(root, shared), Err(sealed));
        assert_eq!(fork.or(root, shared), Err(sealed));
        assert_eq!(fork.ite(a, root, shared), Err(sealed));
        assert_eq!(fork.cofactor(root, 0, false), Err(sealed));
        assert_eq!(fork.rewind(checkpoint), Err(sealed));
        assert!(matches!(fork.freeze_prefix(), Err(error) if error == sealed));
        assert_eq!(fork.record_fork_work(0, 0), Err(sealed));
        assert_eq!(fork.work_steps(), work);
        drop(fork);
        drop(sibling);
        drop(prefix);
        owner.thaw_prefix().unwrap();
        assert_eq!(owner.and(a, b).unwrap(), shared);
    }

    #[test]
    fn symbolic_boolean_frozen_prefix_moves_storage_and_keeps_fork_ids_scoped() {
        let mut owner = BooleanArena::new(8, BooleanLimits::default()).unwrap();
        let a = owner.variable(2).unwrap();
        let b = owner.variable(6).unwrap();
        let shared = owner.and(a, b).unwrap();
        let checkpoint = owner.checkpoint();
        let allocation = owner.nodes.as_ptr();
        let original_count = owner.node_count();
        let prefix = owner.freeze_prefix().unwrap();
        assert_eq!(prefix.nodes.as_ptr(), allocation);
        assert_eq!(owner.node_count(), original_count);
        let mut left = BooleanArena::fork_prefix(Arc::clone(&prefix)).unwrap();
        let mut right = BooleanArena::fork_prefix(Arc::clone(&prefix)).unwrap();
        assert!(left.can_read_owner(owner.owner_id()));
        assert!(!left.can_read_owner(right.owner_id()));
        assert_eq!(left.and(a, b).unwrap(), shared);
        assert_eq!(right.variable(2).unwrap(), a);
        let left_only = left.variable(1).unwrap();
        let right_only = right.variable(3).unwrap();
        // Equal numeric IDs in different suffix owners have different meaning.
        assert_eq!(left_only, right_only);
        assert_eq!(left.node(left_only).unwrap().variable, 1);
        assert_eq!(right.node(right_only).unwrap().variable, 3);
        assert!(owner.thaw_prefix().is_err());
        let work = left.work_steps() + right.work_steps();
        let before = owner.work_steps();
        owner.record_fork_work(work, 0).unwrap();
        drop(left);
        drop(right);
        drop(prefix);
        owner.thaw_prefix().unwrap();
        owner.rewind(checkpoint).unwrap();
        assert_eq!(owner.nodes.as_ptr(), allocation);
        assert_eq!(owner.work_steps(), before + work);
        assert!(owner.evaluate(shared, |variable| variable == 2 || variable == 6));
    }

    #[test]
    fn symbolic_boolean_forking_preserves_explicit_bounds_and_secondary_cancel() {
        for limits in [
            BooleanLimits {
                max_nodes: 8,
                ..BooleanLimits::default()
            },
            BooleanLimits {
                max_work: 8,
                ..BooleanLimits::default()
            },
            BooleanLimits {
                max_memo_entries: 8,
                ..BooleanLimits::default()
            },
        ] {
            let mut owner = BooleanArena::new(8, limits).unwrap();
            let original = owner.variable(2).unwrap();
            assert!(matches!(
                owner.freeze_prefix(),
                Err(BooleanError::Unsupported(_))
            ));
            assert_eq!(owner.variable(2).unwrap(), original);
        }
        let mut owner = BooleanArena::new(8, BooleanLimits::default()).unwrap();
        let prefix = owner.freeze_prefix().unwrap();
        let mut fork = BooleanArena::fork_prefix(Arc::clone(&prefix)).unwrap();
        let stop = CancellationToken::new();
        fork.set_secondary_cancellation(stop.clone());
        stop.handle().cancel();
        assert_eq!(fork.variable(1), Err(BooleanError::Cancelled));
        drop(fork);
        drop(prefix);
        owner.thaw_prefix().unwrap();
        assert!(owner.variable(1).is_ok());
    }

    #[test]
    fn symbolic_boolean_preserves_wide_variables_and_boolean_meaning() {
        let mut arena = BooleanArena::new(20_000, BooleanLimits::default()).unwrap();
        let a = arena.variable(2).unwrap();
        let b = arena.variable(1000).unwrap();
        let c = arena.variable(19_999).unwrap();
        let not_a = arena.not(a).unwrap();
        let first = arena.and(a, b).unwrap();
        let second = arena.and(not_a, c).unwrap();
        let root = arena.or(first, second).unwrap();
        for bits in 0..8_u8 {
            let selected = |variable| match variable {
                2 => bits & 1 != 0,
                1000 => bits & 2 != 0,
                19_999 => bits & 4 != 0,
                _ => false,
            };
            assert_eq!(
                arena.evaluate(root, selected),
                if bits & 1 != 0 {
                    bits & 2 != 0
                } else {
                    bits & 4 != 0
                }
            );
        }
        assert_eq!(arena.cofactor(root, 2, true).unwrap(), b);
        assert_eq!(arena.cofactor(root, 2, false).unwrap(), c);
        assert_eq!(arena.ite(a, b, c).unwrap(), root);
    }

    #[test]
    fn symbolic_boolean_deep_apply_is_iterative_and_rewind_keeps_persistent_roots() {
        let mut arena = BooleanArena::new(5000, BooleanLimits::default()).unwrap();
        let persistent = arena.variable(4999).unwrap();
        let checkpoint = arena.checkpoint();
        let mut conjunction = persistent;
        for variable in (0..4999).rev() {
            conjunction = arena.mk(variable, BooleanId::FALSE, conjunction).unwrap();
        }
        // This apply must traverse the deep conjunction even though its final
        // canonical result was already interned. NOT itself no longer walks it.
        assert_eq!(arena.and(conjunction, persistent).unwrap(), conjunction);
        let before_nodes = arena.node_count();
        let before_work = arena.work_steps();
        let complement = arena.not(conjunction).unwrap();
        assert_eq!(arena.node_count(), before_nodes);
        assert_eq!(arena.work_steps(), before_work + 1);
        assert_eq!(arena.or(conjunction, complement).unwrap(), BooleanId::TRUE);
        assert!(!arena.evaluate(conjunction, |v| v != 1000));
        let token = CancellationToken::new();
        arena.set_cancellation(token.clone());
        token.handle().cancel();
        arena.rewind(checkpoint).unwrap();
        assert_eq!(arena.node_count(), 3);
        assert!(arena.validate(conjunction).is_err());
        assert!(arena.validate(complement).is_err());
        assert!(arena.evaluate(persistent, |v| v == 4999));
        let other = BooleanArena::new(5000, BooleanLimits::default()).unwrap();
        assert!(arena.rewind(other.checkpoint()).is_err());
    }

    #[test]
    fn symbolic_boolean_refusal_and_cancellation_are_not_false() {
        let mut arena = BooleanArena::new(
            1,
            BooleanLimits {
                max_nodes: 2,
                ..BooleanLimits::default()
            },
        )
        .unwrap();
        assert_eq!(arena.variable(0), Err(BooleanError::NodeLimit));
        let token = CancellationToken::new();
        token.handle().cancel();
        let mut arena = BooleanArena::new(1, BooleanLimits::default()).unwrap();
        arena.set_cancellation(token);
        assert_eq!(arena.variable(0), Err(BooleanError::Cancelled));
    }

    #[test]
    fn symbolic_boolean_memo_refusal_preserves_hits_and_checks_fast_path_cancellation() {
        let mut arena = BooleanArena::new(
            4,
            BooleanLimits {
                max_memo_entries: 1,
                ..BooleanLimits::default()
            },
        )
        .unwrap();
        let a = arena.variable(0).unwrap();
        let b = arena.variable(1).unwrap();
        let c = arena.variable(2).unwrap();
        let ab = arena.and(a, b).unwrap();
        let hits = arena.apply_cache_hits();
        assert_eq!(arena.and(a, b), Ok(ab));
        assert_eq!(arena.apply_cache_hits(), hits + 1);
        let not_a = arena.not(a).unwrap();
        let not_b = arena.not(b).unwrap();
        let not_ab = arena.not(ab).unwrap();
        let hits = arena.apply_cache_hits();
        assert_eq!(arena.or(not_a, not_b), Ok(not_ab));
        assert_eq!(arena.apply_cache_hits(), hits + 1);
        assert_eq!(arena.and(a, BooleanId::FALSE), Ok(BooleanId::FALSE));
        assert_eq!(arena.and(a, c), Err(BooleanError::MemoLimit));
        assert_eq!(arena.and(a, b), Ok(ab));

        // Two distinct cofactor subproblems need two memo entries. Refusing
        // the second must not return either a partial result or false.
        let d = arena.variable(3).unwrap();
        let left = arena.mk(2, BooleanId::FALSE, d).unwrap();
        let right = arena.mk(2, d, BooleanId::TRUE).unwrap();
        let root = arena.mk(0, left, right).unwrap();
        assert_eq!(arena.cofactor(root, 3, true), Err(BooleanError::MemoLimit));

        // Work is no longer at the periodic cancellation sample boundary.
        let token = CancellationToken::new();
        arena.set_cancellation(token.clone());
        token.handle().cancel();
        assert_eq!(arena.not(a), Err(BooleanError::Cancelled));
        assert_eq!(arena.and(a, b), Err(BooleanError::Cancelled));
        assert_eq!(arena.and(a, BooleanId::FALSE), Err(BooleanError::Cancelled));
        assert_eq!(
            arena.ite(BooleanId::TRUE, a, b),
            Err(BooleanError::Cancelled)
        );

        let mut limited = BooleanArena::new(
            1,
            BooleanLimits {
                max_work: 1,
                ..BooleanLimits::default()
            },
        )
        .unwrap();
        let a = limited.variable(0).unwrap();
        assert_eq!(
            limited.and(a, BooleanId::TRUE),
            Err(BooleanError::WorkLimit)
        );
    }
}
