//! Quotient only complete H-boundary root sets, before allocating global H boards.
//!
//! The caller retains every source kind owner and its checked rebound map for
//! this entire batch. No worker-local root ID participates in class equality.
//! Ordinary registration/merge closes after success; pre-H edge closure remains
//! permitted. Physical child keys are checked before their identities are erased.

use std::{collections::hash_map::Entry, mem::size_of};

use super::*;

#[derive(Clone, Copy)]
struct ClassLink {
    node: u32,
    next: u32,
}

#[derive(Clone, Copy)]
enum SourceRemap {
    Physical(u32),
    BoundaryGroup(u32),
}

impl SourceRemap {
    fn physical(self) -> Option<u32> {
        match self {
            Self::Physical(node) => Some(node),
            Self::BoundaryGroup(_) => None,
        }
    }

    fn resolve(self, groups: &[BoundaryGroup]) -> Option<u32> {
        match self {
            Self::Physical(node) => Some(node),
            Self::BoundaryGroup(group) => groups.get(group as usize)?.class,
        }
    }
}

#[derive(Default)]
struct BoundaryGroup {
    members: Vec<(usize, u32)>,
    class: Option<u32>,
}

#[derive(Default)]
struct Staging {
    remaps: Vec<Vec<SourceRemap>>,
    remap_capacity_bytes: usize,
    pre_h_parents: Vec<(usize, u32)>,
    group_index: HashMap<BoardKey, usize>,
    groups: Vec<BoundaryGroup>,
    group_member_bytes: usize,
    roots: Vec<u32>,
    // A generation stamp deduplicates roots within one complete board group
    // without repeatedly clearing or visiting a large hash table. IDs here
    // are all rebound into the one pinned destination arena.
    root_seen: HashMap<u32, usize>,
    heads: HashMap<u64, u32>,
    links: Vec<ClassLink>,
}

impl Staging {
    fn retained_bytes(&self) -> usize {
        self.remaps.capacity() * size_of::<Vec<SourceRemap>>()
            + self.remap_capacity_bytes
            + self.pre_h_parents.capacity() * size_of::<(usize, u32)>()
            + self.group_index.capacity() * (size_of::<(BoardKey, usize)>() + 1)
            + self.groups.capacity() * size_of::<BoundaryGroup>()
            + self.group_member_bytes
            + self.roots.capacity() * size_of::<u32>()
            + self.root_seen.capacity() * (size_of::<(u32, usize)>() + 1)
            + self.heads.capacity() * (size_of::<(u64, u32)>() + 1)
            + self.links.capacity() * size_of::<ClassLink>()
    }
}

impl BoardActionFamily {
    /// `incoming_owner_bytes` includes all still-live source physical/kind
    /// owners and checked import maps; the destination kind arena is already
    /// owned by the enclosing workspace and is not counted a second time.
    pub(in crate::backend::wasm_cpu) fn merge_completed_boundaries(
        &mut self,
        sources: Vec<Self>,
        rebounds: &[&[u32]],
        incoming_owner_bytes: usize,
        kinds: &PieceOrderLanguageCache,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let result = self.merge_completed_boundaries_inner(
            sources,
            rebounds,
            incoming_owner_bytes,
            kinds,
            control,
        );
        self.merge_peak_bytes = 0;
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn boundary_charge(
        &mut self,
        steps: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.charge(steps, control)?;
        self.boundary_class_metrics[6] = self.boundary_class_metrics[6].saturating_add(steps);
        Ok(())
    }

    fn observe_boundary_staging(&mut self, incoming: usize, stage: &Staging) {
        // Allocation upper estimate: all live incoming owners/maps plus
        // scratch and at most another scratch-sized old/new growth overlap.
        self.merge_peak_bytes = incoming.saturating_add(stage.retained_bytes().saturating_mul(2));
        self.boundary_class_metrics[5] = self.boundary_class_metrics[5].max(self.merge_peak_bytes);
        self.observe_peak(0);
    }

    fn boundary_root_set_hash(
        &mut self,
        roots: &[u32],
        remaining: u8,
        control: &ExecutionControl,
    ) -> Result<u64, WasmExactSearchError> {
        fn mix(mut value: u64) -> u64 {
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            value ^ (value >> 31)
        }
        let mut sum = 0_u64;
        let mut xor = 0_u64;
        for &root in roots {
            self.boundary_charge(1, control)?;
            let word = mix(u64::from(root).wrapping_add(0x9e3779b97f4a7c15));
            sum = sum.wrapping_add(word);
            xor ^= word;
        }
        // This permutation-invariant fingerprint only chooses a bucket. It
        // never proves set equality, even when cardinality also agrees.
        Ok(
            mix(sum ^ (roots.len() as u64).wrapping_mul(257) ^ u64::from(remaining))
                ^ xor.rotate_left(29),
        )
    }

    fn find_boundary_class(
        &mut self,
        stage: &Staging,
        group: usize,
        remaining: u8,
        hash: u64,
        control: &ExecutionControl,
    ) -> Result<Option<u32>, WasmExactSearchError> {
        self.boundary_charge(1, control)?;
        let mut link = stage.heads.get(&hash).copied().unwrap_or(NO_NODE);
        while link != NO_NODE {
            self.boundary_charge(1, control)?;
            let entry = stage.links[link as usize];
            let node = &self.nodes[entry.node as usize];
            let mut equal = node.remaining() == remaining
                && node.suffixes.frozen().unwrap().len() == stage.roots.len();
            if equal {
                for index in 0..stage.roots.len() {
                    self.boundary_charge(1, control)?;
                    let root = self.nodes[entry.node as usize].suffixes.frozen().unwrap()[index];
                    if stage.root_seen.get(&root).copied() != Some(group) {
                        equal = false;
                        break;
                    }
                }
            }
            // Both sets are unique. Equal size and membership of every
            // stored root prove equality without sorting this group's roots.
            if equal {
                return Ok(Some(entry.node));
            }
            link = entry.next;
        }
        Ok(None)
    }

    fn intern_complete_boundary_set(
        &mut self,
        stage: &mut Staging,
        group: usize,
        remaining: u8,
        incoming: usize,
        already_retained: bool,
        control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        let hash = self.boundary_root_set_hash(&stage.roots, remaining, control)?;
        if let Some(class) = self.find_boundary_class(stage, group, remaining, hash, control)? {
            self.boundary_class_metrics[3] += 1;
            if already_retained {
                // The complete equivalent set is now represented by an
                // existing immutable class. Only these actually discarded
                // references leave the live member count; work is unchanged.
                self.metrics.suffix_roots = self
                    .metrics
                    .suffix_roots
                    .checked_sub(stage.roots.len())
                    .ok_or_else(|| invalid("wasm_observation_boundary_member_count_invalid"))?;
            }
            return Ok(class);
        }
        if self.nodes.len() >= MAX_NODES {
            return Err(invalid("wasm_observation_physical_node_limit"));
        }
        let added = if already_retained {
            0
        } else {
            stage.roots.len()
        };
        let count = self
            .metrics
            .suffix_roots
            .checked_add(added)
            .filter(|&count| count <= MAX_UNION_MEMBERS)
            .ok_or_else(|| invalid("wasm_observation_physical_union_limit"))?;
        stage.links.try_reserve(1).map_err(|_| storage())?;
        self.observe_boundary_staging(incoming, stage);
        stage.heads.try_reserve(1).map_err(|_| storage())?;
        self.observe_boundary_staging(incoming, stage);
        let old = self.nodes.capacity() * size_of::<BoardNode>();
        self.nodes.try_reserve(1).map_err(|_| storage())?;
        self.observe_peak(if old != self.nodes.capacity() * size_of::<BoardNode>() {
            old
        } else {
            0
        });
        let node = self.nodes.len() as u32;
        let roots = std::mem::take(&mut stage.roots);
        self.suffix_capacity_bytes += roots.capacity() * size_of::<u32>();
        self.boundary_class_metrics[4] += roots.len();
        self.nodes.push(BoardNode {
            state: BoardState::Boundary { remaining },
            edges: Vec::new(),
            suffixes: SuffixRoots::FrozenUniqueUnordered(roots),
        });
        self.metrics.nodes += 1;
        self.metrics.suffix_roots = count;
        self.boundary_class_metrics[2] += 1;
        let next = stage
            .heads
            .insert(hash, stage.links.len() as u32)
            .unwrap_or(NO_NODE);
        stage.links.push(ClassLink { node, next });
        Ok(node)
    }

    /// Close the one request-shared registration owner. H groups were never
    /// executable nodes and their retained roots were charged on insertion.
    /// This barrier reuses the same exact class interner as shard batching,
    /// without importing or scanning any worker graph a second time.
    pub(super) fn finish_pending_boundaries(
        &mut self,
        kinds: &PieceOrderLanguageCache,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.producing(control)?;
        if !self.plan.boundary_classes_selected()
            || self
                .kind_arena
                .as_ref()
                .is_none_or(|owner| !Arc::ptr_eq(owner, kinds.arena_identity()))
        {
            return Err(invalid("wasm_observation_shared_producer_scope_invalid"));
        }
        self.boundary_charge(1, control)?;
        let mut pending = self
            .pending_boundaries
            .take()
            .ok_or_else(|| invalid("wasm_observation_shared_producer_closed"))?;
        let mut stage = Staging::default();
        let mut classes = Vec::new();
        self.observe_boundary_staging(pending.retained_bytes(), &stage);
        classes
            .try_reserve_exact(pending.groups.len())
            .map_err(|_| storage())?;
        let class_bytes = classes.capacity() * size_of::<u32>();
        self.observe_boundary_staging(pending.retained_bytes() + class_bytes, &stage);
        let remaining = self.plan.total_depth - self.plan.hidden_horizon;
        for group in 0..pending.groups.len() {
            self.boundary_charge(1, control)?;
            let key = pending.groups[group].key;
            if key.remaining != remaining
                || key.board & !self.plan.board_mask != 0
                || pending.groups[group].roots.len() == 0
                || self.root.is_none()
            {
                return Err(invalid("wasm_observation_boundary_source_invalid"));
            }
            self.boundary_class_metrics[0] += 1;
            self.boundary_class_metrics[1] += 1;
            stage.roots.clear();
            // Keep the old and new capacities in the peak estimate while
            // copying this unique set to the class interner's reused scratch,
            // exactly as ordinary Large->frozen root storage does.
            let roots = std::mem::take(&mut pending.groups[group].roots);
            let old_bytes = roots.retained_bytes();
            let values = match &roots {
                SuffixRoots::Small(roots) => RetainedSuffixRoots::Small(roots.iter()),
                SuffixRoots::Large(roots) => RetainedSuffixRoots::Large(roots.iter()),
                SuffixRoots::FrozenUniqueUnordered(_) => {
                    return Err(invalid("wasm_observation_boundary_source_already_closed"));
                }
            };
            for root in values {
                self.boundary_charge(1, control)?;
                let metadata = kinds
                    .node(root)
                    .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                if metadata.remaining_depth != remaining || metadata.accepting {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                stage.root_seen.try_reserve(1).map_err(|_| storage())?;
                self.observe_boundary_staging(pending.retained_bytes() + class_bytes, &stage);
                stage.roots.try_reserve(1).map_err(|_| storage())?;
                self.observe_boundary_staging(pending.retained_bytes() + class_bytes, &stage);
                self.boundary_charge(1, control)?;
                if stage.root_seen.insert(root, group) == Some(group) {
                    return Err(invalid("wasm_observation_boundary_root_set_not_unique"));
                }
                stage.roots.push(root);
            }
            drop(roots);
            pending.root_capacity_bytes -= old_bytes;
            let class = self.intern_complete_boundary_set(
                &mut stage,
                group,
                remaining,
                pending.retained_bytes() + class_bytes,
                true,
                control,
            )?;
            classes.push(class);
            self.observe_boundary_staging(pending.retained_bytes() + class_bytes, &stage);
        }

        for (&parent, edges) in &pending.edges {
            self.boundary_charge(1, control)?;
            let key = self
                .nodes
                .get(parent as usize)
                .and_then(BoardNode::physical_key)
                .ok_or_else(|| invalid("wasm_observation_boundary_requires_proved_edge"))?;
            if key.remaining != remaining + 1 || !self.nodes[parent as usize].edges.is_empty() {
                return Err(invalid("wasm_observation_boundary_requires_proved_edge"));
            }
            let old = self.nodes[parent as usize].edges.capacity() * size_of::<BoardEdge>();
            self.nodes[parent as usize]
                .edges
                .try_reserve_exact(edges.len())
                .map_err(|_| storage())?;
            let new = self.nodes[parent as usize].edges.capacity() * size_of::<BoardEdge>();
            self.edge_capacity_bytes += new - old;
            self.observe_boundary_staging(pending.retained_bytes() + class_bytes, &stage);
            self.observe_peak(if old != new { old } else { 0 });
            for edge in edges {
                self.boundary_charge(1, control)?;
                let child = *classes
                    .get(edge.group as usize)
                    .ok_or_else(|| invalid("wasm_observation_physical_child_invalid"))?;
                // Insertion already checked the immutable parent/group keys
                // and exact place/clear transition. Resolving that same group
                // changes its representation, never its physical authority.
                if self.nodes[child as usize].remaining() != remaining {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                self.nodes[parent as usize].edges.push(BoardEdge {
                    piece: edge.piece,
                    cells: edge.cells,
                    child,
                });
            }
        }
        // Counts already include every staged edge and root. Moving an edge
        // into its executable parent does not count or charge it twice. Exact
        // class reuse may discard equivalent root references, never work.
        if self.metrics.nodes > MAX_NODES
            || self.metrics.edges > MAX_EDGES
            || self.metrics.suffix_roots > MAX_UNION_MEMBERS
        {
            return Err(invalid("wasm_observation_shared_resource_count_invalid"));
        }
        self.prefix_cone_exact_metrics[0] += self.exact_cones.len;
        self.prefix_cone_exact_metrics[1] += self.exact_cones.capacity;
        self.exact_cones = ExactConeCache::default();
        self.prefix_cones = Vec::new();
        self.prefixes.clear();
        self.cone_misses.clear();
        self.cone_exact_misses.clear();
        self.producer_closed = true;
        self.merge_peak_bytes = 0;
        self.observe_peak(0);
        Ok(())
    }

    fn merge_completed_boundaries_inner(
        &mut self,
        sources: Vec<Self>,
        rebounds: &[&[u32]],
        incoming_owner_bytes: usize,
        kinds: &PieceOrderLanguageCache,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.producing(control)?;
        if !self.plan.boundary_classes_selected()
            || !self.nodes.is_empty()
            || self.root.is_some()
            || self.metrics.candidate_count != 0
            || sources.len() != rebounds.len()
        {
            return Err(invalid("wasm_observation_boundary_batch_scope_invalid"));
        }
        let incoming = incoming_owner_bytes.saturating_add(sources.capacity() * size_of::<Self>());
        let mut stage = Staging::default();
        self.observe_boundary_staging(incoming, &stage);
        // Account for work already performed by every worker exactly once.
        // Quotienting retained nodes cannot refund that completed work.
        for source in &sources {
            source.producing(control)?;
            if !Arc::ptr_eq(&self.plan, &source.plan) {
                return Err(invalid("wasm_observation_board_request_scope_changed"));
            }
            self.charge(source.metrics.work, control)?;
            self.boundary_charge(1, control)?;
        }
        stage
            .remaps
            .try_reserve_exact(sources.len())
            .map_err(|_| storage())?;
        self.observe_boundary_staging(incoming, &stage);
        let remaining = self.plan.total_depth - self.plan.hidden_horizon;
        for (shard, source) in sources.iter().enumerate() {
            let mut remap = Vec::new();
            remap
                .try_reserve_exact(source.nodes.len())
                .map_err(|_| storage())?;
            stage.remap_capacity_bytes += remap.capacity() * size_of::<SourceRemap>();
            stage.remaps.push(remap);
            self.observe_boundary_staging(incoming, &stage);
            // Push each remap during the mandatory source-node visit. There
            // is no separate NO_NODE fill followed by a second write. An H
            // entry names a group until all shards have completed that group.
            for (index, node) in source.nodes.iter().enumerate() {
                self.boundary_charge(1, control)?;
                let key = node
                    .physical_key()
                    .ok_or_else(|| invalid("wasm_observation_boundary_source_already_closed"))?;
                if key.board & !self.plan.board_mask != 0 {
                    return Err(invalid("wasm_observation_board_state_invalid"));
                }
                let depth = self
                    .plan
                    .total_depth
                    .checked_sub(key.remaining)
                    .ok_or_else(|| invalid("wasm_observation_physical_depth_mismatch"))?;
                if depth > self.plan.hidden_horizon
                    || (depth == self.plan.hidden_horizon
                        && (!node.edges.is_empty() || node.suffixes.len() == 0))
                    || (depth < self.plan.hidden_horizon && node.suffixes.len() != 0)
                {
                    return Err(invalid("wasm_observation_boundary_source_invalid"));
                }
                let remapped = if depth == self.plan.hidden_horizon {
                    // One hash grouping pass replaces sorting every worker's
                    // H indices. No class is allocated until this pass has
                    // consumed every shard, so equal partial sets never merge.
                    self.boundary_charge(1, control)?;
                    let group = if let Some(&group) = stage.group_index.get(&key) {
                        group
                    } else {
                        stage.group_index.try_reserve(1).map_err(|_| storage())?;
                        self.observe_boundary_staging(incoming, &stage);
                        stage.groups.try_reserve(1).map_err(|_| storage())?;
                        self.observe_boundary_staging(incoming, &stage);
                        let group = stage.groups.len();
                        stage.groups.push(BoundaryGroup::default());
                        self.boundary_charge(1, control)?;
                        stage.group_index.insert(key, group);
                        self.boundary_class_metrics[1] += 1;
                        group
                    };
                    let group_id = u32::try_from(group)
                        .map_err(|_| invalid("wasm_observation_boundary_group_limit"))?;
                    let old_capacity = stage.groups[group].members.capacity();
                    stage.groups[group]
                        .members
                        .try_reserve(1)
                        .map_err(|_| storage())?;
                    stage.group_member_bytes += (stage.groups[group].members.capacity()
                        - old_capacity)
                        * size_of::<(usize, u32)>();
                    self.observe_boundary_staging(incoming, &stage);
                    stage.groups[group].members.push((shard, index as u32));
                    self.boundary_class_metrics[0] += 1;
                    SourceRemap::BoundaryGroup(group_id)
                } else {
                    let imported = self.intern_board(key)?;
                    if !node.edges.is_empty() {
                        stage.pre_h_parents.try_reserve(1).map_err(|_| storage())?;
                        self.observe_boundary_staging(incoming, &stage);
                        stage.pre_h_parents.push((shard, index as u32));
                    }
                    SourceRemap::Physical(imported)
                };
                stage.remaps[shard].push(remapped);
            }
            if let Some(root) = source.root {
                let imported = stage.remaps[shard]
                    .get(root as usize)
                    .copied()
                    .and_then(SourceRemap::physical)
                    .ok_or_else(|| invalid("wasm_observation_board_root_changed"))?;
                let key = self.nodes[imported as usize].physical_key().unwrap();
                if key
                    != (BoardKey {
                        board: self.plan.initial_board,
                        remaining: self.plan.total_depth,
                    })
                    || self.root.is_some_and(|root| root != imported)
                {
                    return Err(invalid("wasm_observation_board_root_changed"));
                }
                self.root = Some(imported);
            }
        }
        // Each group is now the immutable, complete contribution for one H
        // board. The unique root slice is immutable once its class is issued;
        // set equality and revealed membership do not require a sorted slice.
        for group in 0..stage.groups.len() {
            stage.roots.clear();
            for member in 0..stage.groups[group].members.len() {
                self.boundary_charge(1, control)?;
                let (shard, node_id) = stage.groups[group].members[member];
                let node = &sources[shard].nodes[node_id as usize];
                let roots = match &node.suffixes {
                    SuffixRoots::Small(roots) => RetainedSuffixRoots::Small(roots.iter()),
                    SuffixRoots::Large(roots) => RetainedSuffixRoots::Large(roots.iter()),
                    SuffixRoots::FrozenUniqueUnordered(_) => {
                        return Err(invalid("wasm_observation_boundary_source_already_closed"));
                    }
                };
                for root in roots {
                    self.boundary_charge(1, control)?;
                    let rebound = rebounds[shard]
                        .get(root as usize)
                        .copied()
                        .filter(|&root| root != NO_NODE && root < KIND_TAG - 1)
                        .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                    let metadata = kinds
                        .node(rebound)
                        .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                    if metadata.remaining_depth != remaining || metadata.accepting {
                        return Err(invalid("wasm_observation_physical_depth_mismatch"));
                    }
                    stage.root_seen.try_reserve(1).map_err(|_| storage())?;
                    self.observe_boundary_staging(incoming, &stage);
                    self.boundary_charge(1, control)?;
                    let first_in_group = match stage.root_seen.entry(rebound) {
                        Entry::Occupied(mut seen) => {
                            let first = *seen.get() != group;
                            *seen.get_mut() = group;
                            first
                        }
                        Entry::Vacant(seen) => {
                            seen.insert(group);
                            true
                        }
                    };
                    if first_in_group {
                        stage.roots.try_reserve(1).map_err(|_| storage())?;
                        self.observe_boundary_staging(incoming, &stage);
                        stage.roots.push(rebound);
                    }
                }
            }
            let class = self.intern_complete_boundary_set(
                &mut stage, group, remaining, incoming, false, control,
            )?;
            // Resolve all provisional references through this one completed
            // group binding; do not rewrite every incoming H node's remap.
            stage.groups[group].class = Some(class);
            self.observe_boundary_staging(incoming, &stage);
        }
        // H nodes were already checked to have no edges. Retain their complete
        // suffix sets above without visiting those empty nodes a second time.
        for &(shard, index) in &stage.pre_h_parents {
            let source = &sources[shard];
            let node = &source.nodes[index as usize];
            self.boundary_charge(1, control)?;
            let parent = stage.remaps[shard][index as usize]
                .physical()
                .ok_or_else(|| invalid("wasm_observation_boundary_requires_proved_edge"))?;
            let original_parent = node.physical_key().unwrap();
            for &edge in &node.edges {
                self.boundary_charge(1, control)?;
                let original_child = source
                    .nodes
                    .get(edge.child as usize)
                    .and_then(BoardNode::physical_key)
                    .ok_or_else(|| invalid("wasm_observation_physical_child_invalid"))?;
                // Preserve the original typed physical proof before the
                // child BoardKey is replaced by an exact language class.
                source.validate_edge_keys(original_parent, edge, original_child)?;
                let child = stage.remaps[shard]
                    .get(edge.child as usize)
                    .copied()
                    .and_then(|mapped| mapped.resolve(&stage.groups))
                    .ok_or_else(|| invalid("wasm_observation_physical_child_invalid"))?;
                let mapped = BoardEdge { child, ..edge };
                match self.nodes[child as usize].state {
                    BoardState::Physical(_) => self.insert_edge(parent, mapped)?,
                    BoardState::Boundary {
                        remaining: child_remaining,
                    } => {
                        let key = self.nodes[parent as usize].physical_key().ok_or_else(|| {
                            invalid("wasm_observation_boundary_requires_proved_edge")
                        })?;
                        if key != original_parent || child_remaining != original_child.remaining {
                            return Err(invalid("wasm_observation_board_transition_invalid"));
                        }
                        match self.nodes[parent as usize]
                            .edges
                            .binary_search_by_key(&(mapped.piece, mapped.cells), |edge| {
                                (edge.piece, edge.cells)
                            }) {
                            Ok(index)
                                if self.nodes[parent as usize].edges[index].child != child =>
                            {
                                return Err(invalid(
                                    "wasm_observation_board_action_not_deterministic",
                                ));
                            }
                            Ok(_) => (),
                            Err(index) => self.insert_new_edge(parent, mapped, index)?,
                        }
                    }
                }
            }
        }
        for source in &sources {
            self.metrics.candidate_count += source.metrics.candidate_count;
            self.metrics.prefix_visits += source.metrics.prefix_visits;
            self.metrics.edge_attempts += source.metrics.edge_attempts;
            self.metrics.source_pruned_prefixes += source.metrics.source_pruned_prefixes;
            self.metrics.dead_source_prefixes += source.metrics.dead_source_prefixes;
            for (total, incoming) in self
                .prefix_mask_metrics
                .iter_mut()
                .zip(source.prefix_mask_metrics)
            {
                *total += incoming;
            }
            for (total, incoming) in self
                .prefix_cone_metrics
                .iter_mut()
                .zip(source.prefix_cone_metrics)
            {
                *total += incoming;
            }
            for (total, incoming) in self
                .prefix_cone_exact_metrics
                .iter_mut()
                .zip(source.prefix_cone_exact_metrics())
            {
                *total += incoming;
            }
        }
        self.kind_arena = Some(Arc::clone(kinds.arena_identity()));
        self.producer_closed = true;
        self.observe_boundary_staging(incoming, &stage);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{initial, register_orders, two_is, two_os};
    use super::*;

    fn plan() -> Arc<BoardObservationPlan> {
        let mut plan = BoardObservationPlan::new(10, 2, initial(), 2, 1, 0, None).unwrap();
        let scope = Arc::get_mut(&mut plan).unwrap();
        scope.boundary_suffixes_only = true;
        scope.fresh_bag_observation = true;
        scope.source_filter = true;
        scope.boundary_classes_enabled = true;
        scope.edge_closure = EdgeClosureMode::BeforeLastPhysicalLayer;
        plan
    }

    fn worker(
        plan: &Arc<BoardObservationPlan>,
        with_is: bool,
    ) -> (BoardActionFamily, PieceOrderLanguageCache) {
        let mut kinds = PieceOrderLanguageCache::default();
        let mut family = BoardActionFamily::new(Arc::clone(plan));
        register_orders(&mut family, &mut kinds, two_os());
        if with_is {
            register_orders(&mut family, &mut kinds, two_is());
        }
        (family, kinds)
    }

    fn frozen_view<'a>(
        family: &'a mut BoardActionFamily,
        kinds: &'a PieceOrderLanguageCache,
        control: &ExecutionControl,
    ) -> BoardActionLanguageView<'a> {
        let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
            10,
            2,
            initial(),
            ((1_u64 << 20) - 1) & !initial(),
        )
        .unwrap();
        family
            .complete_physical_edges(&catalog, &mut ReachabilityWorkspace::default(), control)
            .unwrap();
        family.view_with_kind_union(kinds, control, true).unwrap()
    }

    #[test]
    fn completed_boundary_classes_rebind_and_preserve_distinct_physical_actions() {
        let scope = plan();
        let (left, left_kinds) = worker(&scope, false);
        let (right, right_kinds) = worker(&scope, true);
        let mut kinds = PieceOrderLanguageCache::default();
        let left_map = kinds.import_node_map(&left_kinds).unwrap();
        let right_map = kinds.import_node_map(&right_kinds).unwrap();
        let source_work = left.metrics.work + right.metrics.work;
        let mut ordinary = BoardActionFamily::new(Arc::clone(&scope));
        let (first, first_kinds) = worker(&scope, false);
        let (second, second_kinds) = worker(&scope, true);
        let first_map = kinds.import_node_map(&first_kinds).unwrap();
        let second_map = kinds.import_node_map(&second_kinds).unwrap();
        let control = ExecutionControl::default();
        ordinary.merge(first, &first_map, &control).unwrap();
        ordinary.merge(second, &second_map, &control).unwrap();
        let expected_actions: Vec<_> = ordinary.nodes[ordinary.root.unwrap() as usize]
            .edges
            .iter()
            .map(|edge| (edge.piece, edge.cells))
            .collect();
        let original_count = ordinary.metrics.nodes;
        let mut quotient = BoardActionFamily::new(scope);
        quotient
            .merge_completed_boundaries(
                vec![left, right],
                &[&left_map, &right_map],
                123,
                &kinds,
                &control,
            )
            .unwrap();
        assert!(quotient.metrics.nodes < original_count);
        assert_eq!(
            quotient.metrics.work,
            source_work + quotient.boundary_class_metrics[6]
        );
        assert!(quotient.boundary_class_metrics[0] > quotient.boundary_class_metrics[1]);
        assert!(quotient.boundary_class_metrics[1] > quotient.boundary_class_metrics[2]);
        assert!(quotient.boundary_class_metrics[3] > 0);
        assert!(quotient.boundary_class_metrics[5] >= 123);
        assert_eq!(
            quotient.nodes[quotient.root.unwrap() as usize]
                .edges
                .iter()
                .map(|edge| (edge.piece, edge.cells))
                .collect::<Vec<_>>(),
            expected_actions
        );
        let ordinary_view = frozen_view(&mut ordinary, &kinds, &control);
        let view = frozen_view(&mut quotient, &kinds, &control);
        for index in 0..view.edge_count(view.root()).unwrap() {
            let (_, child) = view.edge(view.root(), index).unwrap();
            assert_eq!(view.node(child).unwrap().depth, 1);
            assert!(!view.node(child).unwrap().accepting);
            assert!(!view.has_physical_actions(child));
            assert!(view.prepare_edges(child, 0x7f, &control).is_err());
            let revealed = view.fully_revealed_node(child, &control).unwrap();
            let (_, old_child) = ordinary_view.edge(ordinary_view.root(), index).unwrap();
            let old = ordinary_view
                .fully_revealed_node(old_child, &control)
                .unwrap();
            view.prepare_edges(revealed, 0x7f, &control).unwrap();
            ordinary_view.prepare_edges(old, 0x7f, &control).unwrap();
            let kinds_at = |language: &BoardActionLanguageView<'_>, node| {
                (0..language.edge_count(node).unwrap())
                    .map(|edge| language.edge(node, edge).unwrap().0)
                    .collect::<Vec<_>>()
            };
            assert_eq!(kinds_at(&view, revealed), kinds_at(&ordinary_view, old));
        }
    }

    #[test]
    fn completed_boundary_classes_keep_original_clear_validation_before_remap() {
        let scope = plan();
        let (mut source, kinds) = worker(&scope, false);
        let root = source.root.unwrap() as usize;
        let child = source.nodes[root].edges[0].child as usize;
        let BoardState::Physical(ref mut key) = source.nodes[child].state else {
            unreachable!()
        };
        key.board ^= 1;
        let rebound: Vec<_> = (0..kinds.node_count() as u32).collect();
        let mut destination = BoardActionFamily::new(scope);
        let error = destination.merge_completed_boundaries(
            vec![source],
            &[&rebound],
            0,
            &kinds,
            &ExecutionControl::default(),
        );
        assert!(matches!(
            error,
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_board_transition_invalid"
            ))
        ));
        assert!(destination.failed);
    }

    #[test]
    fn completed_boundary_classes_use_exact_membership_despite_hash_collisions() {
        let mut family = BoardActionFamily::new(plan());
        // Force distinct remaining/sets into one fingerprint bucket. The
        // lookup must pass both false candidates and return the original ID.
        for (remaining, roots) in [(2, vec![9, 3]), (1, vec![10, 3]), (1, vec![9, 3])] {
            family.nodes.push(BoardNode {
                state: BoardState::Boundary { remaining },
                edges: Vec::new(),
                suffixes: SuffixRoots::FrozenUniqueUnordered(roots),
            });
        }
        let mut stage = Staging::default();
        stage.roots = vec![3, 9];
        let group = 7;
        stage.root_seen.insert(9, group);
        stage.root_seen.insert(3, group);
        stage.root_seen.insert(10, group - 1);
        let control = ExecutionControl::default();
        let hash = family
            .boundary_root_set_hash(&stage.roots, 1, &control)
            .unwrap();
        assert_eq!(
            hash,
            family.boundary_root_set_hash(&[9, 3], 1, &control).unwrap()
        );
        stage.heads.insert(hash, 0);
        stage.links = vec![
            ClassLink { node: 0, next: 1 },
            ClassLink { node: 1, next: 2 },
            ClassLink {
                node: 2,
                next: NO_NODE,
            },
        ];
        assert_eq!(
            family
                .find_boundary_class(&stage, group, 1, hash, &control)
                .unwrap(),
            Some(2)
        );
        // Class equality reads unique sets, without canonicalizing either
        // the incoming or immutable stored order.
        assert_eq!(stage.roots, [3, 9]);
        assert_eq!(family.nodes[2].suffixes.frozen(), Some(&[9, 3][..]));
        stage.root_seen.insert(9, group - 1);
        assert_eq!(
            family
                .find_boundary_class(&stage, group, 1, hash, &control)
                .unwrap(),
            None
        );
    }

    #[test]
    fn completed_boundary_classes_keep_unordered_roots_immutable_and_small_sorted() {
        let mut frozen = SuffixRoots::FrozenUniqueUnordered(vec![9, 3]);
        assert_eq!(frozen.freeze().unwrap(), 0);
        assert_eq!(frozen.frozen(), Some(&[9, 3][..]));
        assert!(matches!(
            frozen.insert(7, true),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_boundary_producer_closed"
            ))
        ));
        assert_eq!(frozen.frozen(), Some(&[9, 3][..]));
        let mut small = SuffixRoots::default();
        small.insert(9, true).unwrap();
        small.insert(3, true).unwrap();
        small.insert(7, true).unwrap();
        small.insert(3, true).unwrap();
        assert_eq!(small.frozen(), Some(&[3, 7, 9][..]));
    }

    #[test]
    fn completed_boundary_classes_close_production_but_allow_final_freeze() {
        let scope = plan();
        let (source, mut kinds) = worker(&scope, false);
        let rebound: Vec<_> = (0..kinds.node_count() as u32).collect();
        let mut destination = BoardActionFamily::new(Arc::clone(&scope));
        let control = ExecutionControl::default();
        destination
            .merge_completed_boundaries(vec![source], &[&rebound], 0, &kinds, &control)
            .unwrap();
        assert!(destination.producer_closed);
        assert!(!destination.frozen);
        assert!(destination.mutable(&control).is_ok());
        assert!(matches!(
            destination.producing(&control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_boundary_producer_closed"
            ))
        ));
        assert!(matches!(
            destination.retained_suffix_roots(&kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_boundary_producer_closed"
            ))
        ));
        // A view can still finalize closure/freeze; public production cannot
        // append to already interned complete root-set classes.
        frozen_view(&mut destination, &kinds, &control);
        let (incoming, incoming_kinds) = worker(&scope, false);
        let incoming_map = kinds.import_node_map(&incoming_kinds).unwrap();
        assert!(destination
            .merge(incoming, &incoming_map, &control)
            .is_err());
    }

    #[test]
    fn completed_boundary_classes_reject_existing_destination_and_foreign_plan() {
        let scope = plan();
        let (mut destination, kinds) = worker(&scope, false);
        let (source, source_kinds) = worker(&scope, false);
        let rebound: Vec<_> = (0..source_kinds.node_count() as u32).collect();
        assert!(destination
            .merge_completed_boundaries(
                vec![source],
                &[&rebound],
                0,
                &kinds,
                &ExecutionControl::default()
            )
            .is_err());
        let (source, source_kinds) = worker(&plan(), false);
        let rebound: Vec<_> = (0..source_kinds.node_count() as u32).collect();
        let mut destination = BoardActionFamily::new(scope);
        assert!(matches!(
            destination.merge_completed_boundaries(
                vec![source],
                &[&rebound],
                0,
                &source_kinds,
                &ExecutionControl::default()
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_board_request_scope_changed"
            ))
        ));
    }

    #[test]
    fn completed_boundary_classes_do_not_refund_source_work() {
        let scope = plan();
        let (mut source, kinds) = worker(&scope, false);
        source.metrics.work = MAX_WORK;
        let rebound: Vec<_> = (0..kinds.node_count() as u32).collect();
        let mut destination = BoardActionFamily::new(scope);
        assert!(matches!(
            destination.merge_completed_boundaries(
                vec![source],
                &[&rebound],
                0,
                &kinds,
                &ExecutionControl::default()
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        assert_eq!(destination.metrics.work, MAX_WORK);
        assert!(destination.failed);
    }
}
