//! Ordered policy equivalence after the physical family is complete.
//!
//! The frozen graph remains the physical authority. This table never invents
//! a board, action, or child: it supplies a memo representative and, only when
//! necessary, indices of the first real edges with distinct (kind, child
//! class). Every accepting depth and every supply-action order is unchanged.
//! H classes keep their existing exact root-set identity. Before H, equal
//! per-kind ordered child-class lists have the same value and select the same
//! original source IDs by backward induction over remaining depth. Actual
//! cells may differ between equivalent boards; their order is preserved.

use super::*;

#[derive(Clone, Copy, Default)]
struct ChoiceRange {
    // NO_NODE denotes the original edge slice, with no extra references.
    start: u32,
    len: u32,
}

#[derive(Default)]
pub(super) struct OrderedPolicyQuotient {
    representatives: Vec<u32>,
    choices: Vec<ChoiceRange>,
    indices: Vec<u32>,
}

impl OrderedPolicyQuotient {
    pub(super) fn retained_bytes(&self) -> usize {
        self.representatives.capacity() * core::mem::size_of::<u32>()
            + self.choices.capacity() * core::mem::size_of::<ChoiceRange>()
            + self.indices.capacity() * core::mem::size_of::<u32>()
    }

    pub(super) fn member_count(&self) -> usize {
        self.indices.len()
    }

    pub(super) fn representative(&self, node: u32) -> Option<u32> {
        self.representatives
            .get(node as usize)
            .copied()
            .filter(|&representative| representative != NO_NODE)
    }

    pub(super) fn edge_count(&self, node: u32) -> Option<usize> {
        self.choices
            .get(node as usize)
            .map(|range| range.len as usize)
    }

    pub(super) fn original_edge_index(&self, node: u32, index: usize) -> Option<usize> {
        let range = self.choices.get(node as usize)?;
        if index >= range.len as usize {
            return None;
        }
        if range.start == NO_NODE {
            Some(index)
        } else {
            self.indices
                .get((range.start as usize).checked_add(index)?)
                .copied()
                .map(|index| index as usize)
        }
    }
}

#[derive(Clone, Copy)]
struct SignatureEntry {
    hash: u64,
    representative: u32,
    next: u32,
}

struct Staging {
    quotient: OrderedPolicyQuotient,
    heads: [u32; MAX_BOARD64_PIECES + 1],
    depth_next: Vec<u32>,
    // (raw node * 8 + actual kind) is an exact generation marker. Child
    // classes are representative raw IDs, bounded by the original node cap.
    seen: Vec<u32>,
    buckets: Vec<u32>,
    entries: Vec<SignatureEntry>,
    kept: Vec<u32>,
    index_capacity_limit: usize,
}

impl Default for Staging {
    fn default() -> Self {
        Self {
            quotient: OrderedPolicyQuotient::default(),
            heads: [NO_NODE; MAX_BOARD64_PIECES + 1],
            depth_next: Vec::new(),
            seen: Vec::new(),
            buckets: Vec::new(),
            entries: Vec::new(),
            kept: Vec::new(),
            index_capacity_limit: 0,
        }
    }
}

impl Staging {
    fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self.quotient.retained_bytes()
            + self.depth_next.capacity() * core::mem::size_of::<u32>()
            + self.seen.capacity() * core::mem::size_of::<u32>()
            + self.buckets.capacity() * core::mem::size_of::<u32>()
            + self.entries.capacity() * core::mem::size_of::<SignatureEntry>()
            + self.kept.capacity() * core::mem::size_of::<u32>()
    }
}

impl BoardActionFamily {
    pub(super) fn prepare_ordered_policy_quotient(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let mut stage = Staging::default();
        let result = self.build_ordered_policy_quotient(&mut stage, control);
        self.observe_ordered_staging(&stage, 0);
        match result {
            Ok(false) => Ok(()),
            Ok(true) => {
                // Publication is one move after every depth bucket, exact
                // comparison and resource check has completed. No partial
                // table may change the evaluator's memo or edge namespace.
                self.ordered_quotient = Some(core::mem::take(&mut stage.quotient));
                drop(stage);
                self.observe_peak(0);
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }

    fn build_ordered_policy_quotient(
        &mut self,
        stage: &mut Staging,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if self.failed {
            return Err(invalid("wasm_observation_board_family_failed"));
        }
        if !self.plan.ordered_classes_enabled
            || !self.plan.boundary_classes_selected()
            || !self.producer_closed
            || !self.frozen
            || !self.edge_closure_complete
            || self.pending_boundaries.is_some()
        {
            return Err(invalid("wasm_observation_ordered_class_scope_invalid"));
        }
        if self.ordered_quotient.is_some() {
            return Ok(false);
        }
        let node_count = self.nodes.len();
        if node_count == 0
            || node_count > MAX_NODES
            || self.metrics.nodes != node_count
            || self.root.is_none_or(|root| root as usize >= node_count)
        {
            return Err(invalid("wasm_observation_physical_root_invalid"));
        }
        let remaining_members = MAX_UNION_MEMBERS
            .checked_sub(self.metrics.suffix_roots)
            .ok_or_else(|| invalid("wasm_observation_physical_union_limit"))?;

        // Exact one-time reservations avoid moving any initialized table.
        // A successful reserve is observed before the next fallible action.
        stage
            .quotient
            .representatives
            .try_reserve_exact(node_count)
            .map_err(|_| storage())?;
        self.observe_ordered_staging(stage, 0);
        stage
            .quotient
            .choices
            .try_reserve_exact(node_count)
            .map_err(|_| storage())?;
        self.observe_ordered_staging(stage, 0);
        stage
            .depth_next
            .try_reserve_exact(node_count)
            .map_err(|_| storage())?;
        self.observe_ordered_staging(stage, 0);
        stage
            .seen
            .try_reserve_exact(node_count)
            .map_err(|_| storage())?;
        self.observe_ordered_staging(stage, 0);
        // Four initialized metadata entries per original node. The physical
        // graph already owns the boards and edges; no physical work is undone.
        self.ordered_charge(node_count.saturating_mul(4), control)?;
        let horizon = self.plan.hidden_horizon;
        for index in 0..node_count {
            let node = &self.nodes[index];
            let depth = self
                .plan
                .total_depth
                .checked_sub(node.remaining())
                .ok_or_else(|| invalid("wasm_observation_physical_depth_mismatch"))?;
            let is_physical = depth < horizon;
            if depth > horizon
                || node.accepting()
                || (is_physical && node.physical_key().is_none())
                || (!is_physical && (node.physical_key().is_some() || !node.edges.is_empty()))
            {
                return Err(invalid("wasm_observation_ordered_class_scope_invalid"));
            }
            stage
                .quotient
                .representatives
                .push(if is_physical { NO_NODE } else { index as u32 });
            stage.quotient.choices.push(ChoiceRange {
                start: NO_NODE,
                len: u32::try_from(node.edges.len())
                    .map_err(|_| invalid("wasm_observation_physical_edge_limit"))?,
            });
            stage.seen.push(0);
            let next = if is_physical {
                self.ordered_class_metrics[0] += 1;
                self.ordered_class_metrics[3] = self.ordered_class_metrics[3]
                    .checked_add(node.edges.len())
                    .ok_or_else(|| invalid("wasm_observation_physical_edge_limit"))?;
                let previous = stage.heads[usize::from(depth)];
                stage.heads[usize::from(depth)] = index as u32;
                previous
            } else {
                NO_NODE
            };
            stage.depth_next.push(next);
        }
        let physical_nodes = self.ordered_class_metrics[0];
        if physical_nodes == 0 || self.ordered_class_metrics[3] > MAX_EDGES {
            return Err(invalid("wasm_observation_ordered_class_scope_invalid"));
        }
        stage.index_capacity_limit = self.ordered_class_metrics[3].min(remaining_members);
        let bucket_count = physical_nodes
            .checked_next_power_of_two()
            .ok_or_else(|| invalid("wasm_observation_physical_node_limit"))?;
        stage
            .buckets
            .try_reserve_exact(bucket_count)
            .map_err(|_| storage())?;
        self.observe_ordered_staging(stage, 0);
        self.ordered_charge(bucket_count, control)?;
        stage.buckets.resize(bucket_count, NO_NODE);
        stage
            .entries
            .try_reserve_exact(physical_nodes)
            .map_err(|_| storage())?;
        self.observe_ordered_staging(stage, 0);

        // Raw IDs are not topological after interning/worker merging. Every
        // depth bucket is complete before any semantic normalization starts.
        for depth in (0..horizon).rev() {
            let mut node = stage.heads[usize::from(depth)];
            while node != NO_NODE {
                self.ordered_charge(1, control)?;
                let next = stage.depth_next[node as usize];
                self.normalize_ordered_node(node, stage, control)?;
                node = next;
            }
        }
        if self
            .root
            .and_then(|root| stage.quotient.representative(root))
            .is_none()
        {
            return Err(invalid("wasm_observation_physical_root_invalid"));
        }
        Ok(true)
    }

    fn normalize_ordered_node(
        &mut self,
        node_id: u32,
        stage: &mut Staging,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let remaining = self.nodes[node_id as usize].remaining();
        let edge_count = self.nodes[node_id as usize].edges.len();
        if edge_count == 0 {
            return Err(invalid("wasm_observation_board_family_incomplete"));
        }
        stage.kept.clear();
        if stage.kept.capacity() < edge_count {
            let old = stage.kept.capacity() * core::mem::size_of::<u32>();
            // len is zero, so allocation growth moves no initialized choices.
            stage
                .kept
                .try_reserve_exact(edge_count)
                .map_err(|_| storage())?;
            self.observe_ordered_staging(stage, old);
        }
        self.ordered_charge(1, control)?;
        let mut hash = signature_word(0x6a09_e667_f3bc_c909, u64::from(remaining));
        let mut previous = None;
        for index in 0..edge_count {
            self.ordered_charge(1, control)?;
            let edge = self.nodes[node_id as usize].edges[index];
            let child = self
                .nodes
                .get(edge.child as usize)
                .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
            let child_class = stage
                .quotient
                .representative(edge.child)
                .ok_or_else(|| invalid("wasm_observation_ordered_class_child_unprepared"))?;
            if !(1..=7).contains(&edge.piece)
                || edge.cells.count_ones() != 4
                || child.remaining().checked_add(1) != Some(remaining)
                || previous.is_some_and(|previous| previous >= (edge.piece, edge.cells))
            {
                return Err(invalid("wasm_observation_physical_action_order_invalid"));
            }
            previous = Some((edge.piece, edge.cells));
            let stamp = node_id * 8 + u32::from(edge.piece);
            let seen = stage
                .seen
                .get_mut(child_class as usize)
                .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
            if *seen == stamp {
                self.ordered_class_metrics[5] += 1;
                continue;
            }
            *seen = stamp;
            // One ordered signature word is examined and stored. The real
            // first edge is retained, including first legal zero choices.
            self.ordered_charge(1, control)?;
            hash = signature_word(hash, (u64::from(child_class) << 8) | u64::from(edge.piece));
            stage.kept.push(index as u32);
            self.ordered_class_metrics[4] += 1;
        }
        if stage.kept.len() != edge_count {
            let new_len = stage
                .quotient
                .indices
                .len()
                .checked_add(stage.kept.len())
                .filter(|&len| len <= stage.index_capacity_limit)
                .ok_or_else(|| invalid("wasm_observation_physical_union_limit"))?;
            if stage.quotient.indices.capacity() == 0 {
                // Reserve a finite upper bound once; no opaque reallocation
                // subsequently copies initialized member references. Capacity
                // bytes are counted even when far fewer choices are stored.
                stage
                    .quotient
                    .indices
                    .try_reserve_exact(stage.index_capacity_limit)
                    .map_err(|_| storage())?;
                self.observe_ordered_staging(stage, 0);
            }
            self.ordered_charge(stage.kept.len(), control)?;
            let start = stage.quotient.indices.len() as u32;
            stage.quotient.indices.extend_from_slice(&stage.kept);
            debug_assert_eq!(stage.quotient.indices.len(), new_len);
            stage.quotient.choices[node_id as usize] = ChoiceRange {
                start,
                len: stage.kept.len() as u32,
            };
        }
        self.ordered_charge(1, control)?;
        hash = signature_word(hash, stage.kept.len() as u64);
        let slot = hash as usize & (stage.buckets.len() - 1);
        let mut entry = stage.buckets[slot];
        while entry != NO_NODE {
            self.ordered_charge(1, control)?;
            let candidate = stage.entries[entry as usize];
            if candidate.hash == hash
                && self.ordered_signatures_equal(
                    node_id,
                    candidate.representative,
                    stage,
                    control,
                )?
            {
                stage.quotient.representatives[node_id as usize] = candidate.representative;
                self.ordered_class_metrics[2] += 1;
                return Ok(());
            }
            entry = candidate.next;
        }
        self.ordered_charge(1, control)?;
        let entry = stage.entries.len() as u32;
        stage.entries.push(SignatureEntry {
            hash,
            representative: node_id,
            next: stage.buckets[slot],
        });
        stage.buckets[slot] = entry;
        stage.quotient.representatives[node_id as usize] = node_id;
        self.ordered_class_metrics[1] += 1;
        Ok(())
    }

    fn ordered_signatures_equal(
        &mut self,
        left: u32,
        right: u32,
        stage: &Staging,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        self.ordered_charge(1, control)?;
        if self.nodes[left as usize].remaining() != self.nodes[right as usize].remaining()
            || stage.quotient.edge_count(left) != stage.quotient.edge_count(right)
        {
            return Ok(false);
        }
        let count = stage.quotient.edge_count(left).unwrap();
        for index in 0..count {
            self.ordered_charge(1, control)?;
            let left_index = stage.quotient.original_edge_index(left, index).unwrap();
            let right_index = stage.quotient.original_edge_index(right, index).unwrap();
            let a = self.nodes[left as usize].edges[left_index];
            let b = self.nodes[right as usize].edges[right_index];
            if a.piece != b.piece
                || stage.quotient.representative(a.child) != stage.quotient.representative(b.child)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn ordered_charge(
        &mut self,
        steps: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.charge(steps, control)?;
        self.ordered_class_metrics[6] += steps;
        Ok(())
    }

    fn observe_ordered_staging(&mut self, stage: &Staging, extra: usize) {
        let bytes = stage.retained_bytes().saturating_add(extra);
        self.ordered_class_metrics[7] = self.ordered_class_metrics[7].max(bytes);
        self.observe_peak(bytes);
    }
}

fn signature_word(hash: u64, word: u64) -> u64 {
    (hash ^ word)
        .rotate_left(23)
        .wrapping_mul(0x9ddf_ea08_eb38_2d69)
}
