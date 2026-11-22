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
    // classes are representative raw IDs in the original encoded namespace.
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
                #[cfg(feature = "local-search-ab")]
                if !self.failed {
                    eprintln!(
                        "clearra_observation_ordered_class_failed stage=prepare error={error:?} ordered_classes={:?} board={:?} retained_bytes={} peak_bytes={}",
                        self.ordered_class_metrics,
                        self.metrics,
                        self.retained_bytes(),
                        self.peak_bytes(),
                    );
                }
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
            || node_count > self.limits.nodes(BOARD_NODE_CAPACITY)
            || self.metrics.nodes != node_count
            || self.root.is_none_or(|root| root as usize >= node_count)
        {
            return Err(invalid("wasm_observation_physical_root_invalid"));
        }
        let remaining_members = self
            .limits
            .members()
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
        self.ordered_charge(
            node_count
                .checked_mul(4)
                .ok_or_else(|| invalid("wasm_observation_physical_work_limit"))?,
            control,
        )?;
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
        if physical_nodes == 0 || self.ordered_class_metrics[3] > self.limits.edges(usize::MAX) {
            return Err(invalid("wasm_observation_ordered_class_scope_invalid"));
        }
        // ChoiceRange stores a u32 start and reserves NO_NODE for identity
        // slices. This bound belongs to that representation, not the budget.
        stage.index_capacity_limit = self.ordered_class_metrics[3]
            .min(remaining_members)
            .min(NO_NODE as usize);
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

#[cfg(test)]
mod tests {
    use clearra_core_domain::probability::probability_value::ProbabilityValue;
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };
    use clearra_rules::kicks::KickTableProfileId;
    use clearra_supply::{pattern_universe::MaterializedPatternUniverse, QueueObservationPolicy};

    use crate::backend::wasm_cpu::{
        queue_observation_policy::{QueueObservationCoverage, QueueObservationPolicyEvaluator},
        symbolic_prefix_orders::SymbolicPrefixWork,
    };

    use super::super::super::PhysicalActionLanguage;
    use super::super::tests::{fresh_bag_orders_for_tail, source_filter_plan};
    use super::*;

    const ROOT: u32 = 0;
    const X: u32 = 1;
    const LEFT: u32 = 2;
    const Y: u32 = 3;
    const RIGHT: u32 = 4;

    fn word(kinds: &mut PieceOrderLanguageCache, pieces: &[PieceKind]) -> u32 {
        let predecessors: Vec<_> = (0..pieces.len()).map(|index| (1 << index) - 1).collect();
        kinds
            .compile_poset_language(pieces, &predecessors, 64)
            .unwrap()
            .unwrap()
            .root
    }

    fn edge(piece: PieceKind, cells: u64, child: u32) -> BoardEdge {
        BoardEdge {
            piece: piece_code(piece),
            cells,
            child,
        }
    }

    /// A small frozen policy-game fixture. It isolates ordering from movement
    /// proof construction; the separate ten-lock test below obtains the same
    /// freeze authority through real catalog/reachability/complete-family code.
    /// Raw IDs deliberately mix depths. Both pre-H boards have X,Y,X choices,
    /// with different physical cells; reversing the right list changes its tie.
    fn fixture(enabled: bool, reverse_right: bool) -> (BoardActionFamily, PieceOrderLanguageCache) {
        use PieceKind::{I, J, L, O, S, T, Z};
        let mut plan = BoardObservationPlan::new(10, 4, 0, 8, 2, 1, None).unwrap();
        let scope = Arc::get_mut(&mut plan).unwrap();
        scope.boundary_suffixes_only = true;
        scope.fresh_bag_observation = true;
        scope.source_filter = true;
        scope.boundary_classes_enabled = true;
        scope.edge_closure = EdgeClosureMode::BeforeLastPhysicalLayer;
        scope.ordered_classes_enabled = enabled;
        let mut kinds = PieceOrderLanguageCache::default();
        let x = word(&mut kinds, &[S, Z, L, J, T, I]);
        let y = word(&mut kinds, &[S, Z, L, J, T, O]);
        let o = 3 | (3 << 10);
        let right_children = if reverse_right { [Y, X, Y] } else { [X, Y, X] };
        let mut family = BoardActionFamily::new(plan);
        family.nodes = vec![
            BoardNode {
                state: BoardState::Physical(BoardKey {
                    board: 0,
                    remaining: 8,
                }),
                edges: vec![edge(I, 0xf, LEFT), edge(I, 0xf0, RIGHT)],
                suffixes: SuffixRoots::default(),
            },
            BoardNode {
                state: BoardState::Boundary { remaining: 6 },
                edges: Vec::new(),
                suffixes: SuffixRoots::FrozenUniqueUnordered(vec![x]),
            },
            BoardNode {
                state: BoardState::Physical(BoardKey {
                    board: 0xf,
                    remaining: 7,
                }),
                edges: vec![edge(O, o << 4, X), edge(O, o << 6, Y), edge(O, o << 8, X)],
                suffixes: SuffixRoots::default(),
            },
            BoardNode {
                state: BoardState::Boundary { remaining: 6 },
                edges: Vec::new(),
                suffixes: SuffixRoots::FrozenUniqueUnordered(vec![y]),
            },
            BoardNode {
                state: BoardState::Physical(BoardKey {
                    board: 0xf0,
                    remaining: 7,
                }),
                edges: [o, o << 2, o << 14]
                    .into_iter()
                    .zip(right_children)
                    .map(|(cells, child)| edge(O, cells, child))
                    .collect(),
                suffixes: SuffixRoots::default(),
            },
        ];
        family.root = Some(ROOT);
        family.metrics.nodes = family.nodes.len();
        family.metrics.edges = family.nodes.iter().map(|node| node.edges.len()).sum();
        family.metrics.suffix_roots = 2;
        family.edge_capacity_bytes = family
            .nodes
            .iter()
            .map(|node| node.edges.capacity() * core::mem::size_of::<BoardEdge>())
            .sum();
        family.suffix_capacity_bytes = family
            .nodes
            .iter()
            .map(|node| node.suffixes.retained_bytes())
            .sum();
        family.producer_closed = true;
        family.frozen = true;
        family.edge_closure_complete = true;
        family.kind_arena = Some(Arc::clone(kinds.arena_identity()));
        (family, kinds)
    }

    struct RootAt<'a, 'b> {
        view: &'a BoardActionLanguageView<'b>,
        root: u32,
    }

    impl ObservationPieceLanguage for RootAt<'_, '_> {
        fn root(&self) -> u32 {
            self.root
        }
        fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
            self.view.node(node)
        }
        fn edge_count(&self, node: u32) -> Option<usize> {
            self.view.edge_count(node)
        }
        fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
            self.view.edge(node, index)
        }
        fn has_physical_actions(&self, node: u32) -> bool {
            self.view.has_physical_actions(node)
        }
        fn physical_action_cells(&self, node: u32, index: usize) -> Option<u64> {
            self.view.physical_action_cells(node, index)
        }
        fn prepare_edges(
            &self,
            node: u32,
            pieces: u8,
            control: &ExecutionControl,
        ) -> Result<(), WasmExactSearchError> {
            self.view.prepare_edges(node, pieces, control)
        }
        fn fixed_horizon(&self) -> Option<u8> {
            self.view.fixed_horizon()
        }
        fn fully_revealed_node(
            &self,
            node: u32,
            control: &ExecutionControl,
        ) -> Result<u32, WasmExactSearchError> {
            self.view.fully_revealed_node(node, control)
        }
        fn revealed_alternatives(
            &self,
            node: u32,
            control: &ExecutionControl,
        ) -> Result<Option<Vec<u32>>, WasmExactSearchError> {
            self.view.revealed_alternatives(node, control)
        }
        fn memo_class(&self, node: u32) -> Option<u32> {
            self.view.memo_class(node)
        }
    }

    fn source_universe(tails: &[PieceKind]) -> MaterializedPatternUniverse {
        use PieceKind::{I, J, L, O, S, T, Z};
        let sequences = tails
            .iter()
            .map(|&tail| vec![I, O, T, S, Z, L, J, T, tail])
            .collect();
        MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(2401),
            PatternWeightModelId::new(2402),
            sequences,
            vec![ProbabilityValue::new(1.0 / tails.len() as f64).unwrap(); tails.len()],
            tails.len() as u128,
            true,
            None,
        )
        .unwrap()
    }

    fn evaluate(
        view: &BoardActionLanguageView<'_>,
        universe: &MaterializedPatternUniverse,
        root: u32,
        cursor: u16,
        collect: bool,
    ) -> QueueObservationCoverage {
        let mut evaluator = QueueObservationPolicyEvaluator::for_original_universe(
            universe,
            QueueObservationPolicy::VisibleSeven,
            cursor,
            None,
            true,
            false,
            false,
            None,
        )
        .unwrap();
        evaluator.set_collect_full_for_test(collect);
        evaluator
            .evaluate(&RootAt { view, root }, &ExecutionControl::default())
            .unwrap()
    }

    fn same_coverage(left: &QueueObservationCoverage, right: &QueueObservationCoverage) {
        assert_eq!(
            left.covered_patterns.words(),
            right.covered_patterns.words()
        );
        assert_eq!(left.covered_pattern_count, right.covered_pattern_count);
        assert_eq!(
            left.covered_weight.to_bits(),
            right.covered_weight.to_bits()
        );
        assert_eq!(left.min_accepted_depth, right.min_accepted_depth);
        assert_eq!(left.max_accepted_depth, right.max_accepted_depth);
    }

    fn witness(family: &BoardActionFamily) -> Vec<(Option<BoardKey>, Vec<BoardEdge>)> {
        family
            .nodes
            .iter()
            .map(|node| (node.physical_key(), node.edges.clone()))
            .collect()
    }

    #[test]
    fn ordered_classes_keep_first_real_edges_and_early_reveal_namespaces() {
        let control = ExecutionControl::default();
        let (mut family, kinds) = fixture(true, false);
        let before = witness(&family);
        let original_bytes = family.retained_bytes();
        let view = family.view_with_kind_union(&kinds, &control, true).unwrap();
        assert_ne!(before[LEFT as usize].0, before[RIGHT as usize].0);
        assert_eq!(view.memo_class(LEFT), view.memo_class(RIGHT));
        assert_eq!(view.edge_count(ROOT), Some(1));
        assert_eq!(view.edge(ROOT, 0), Some((piece_code(PieceKind::I), LEFT)));
        assert_eq!(view.physical_action_cells(ROOT, 0), Some(0xf));
        for node in [LEFT, RIGHT] {
            assert_eq!(view.edge_count(node), Some(2));
            assert_eq!(view.edge(node, 0).unwrap().1, X);
            assert_eq!(view.edge(node, 1).unwrap().1, Y);
            assert_eq!(
                view.physical_action_cells(node, 0),
                Some(before[node as usize].1[0].cells)
            );
            assert_eq!(
                view.physical_action_cells(node, 1),
                Some(before[node as usize].1[1].cells)
            );
            let revealed = view.fully_revealed_node(node, &control).unwrap();
            assert_eq!(revealed, node);
            assert!(view.has_physical_actions(revealed));
            assert_eq!(view.memo_class(revealed), view.memo_class(node));
        }
        let revealed = view.fully_revealed_node(X, &control).unwrap();
        assert_ne!(revealed, X);
        assert_eq!(view.memo_class(revealed), Some(revealed));
        assert!(view.prepare_edges(X, 0x7f, &control).is_err());
        let metrics = view.ordered_class_metrics();
        assert_eq!(&metrics[..6], &[3, 2, 1, 8, 5, 3]);
        assert!(metrics[6] > 0 && metrics[7] > 0);
        drop(view);
        assert_eq!(witness(&family), before);
        let quotient = family.ordered_quotient.as_ref().unwrap();
        assert_eq!(quotient.member_count(), 5);
        assert_eq!(
            family.retained_bytes(),
            original_bytes + quotient.retained_bytes()
        );
        assert_eq!(family.metrics.work, metrics[6]);
        let again = family
            .view_with_kind_union(&kinds, &control, false)
            .unwrap();
        assert_eq!(again.ordered_class_metrics(), metrics);
    }

    #[test]
    fn ordered_classes_preserve_original_ids_and_distinguish_reordered_ties() {
        use PieceKind::{I, O, Z};
        let universe = source_universe(&[I, O, I, O]);
        let control = ExecutionControl::default();
        for reverse in [false, true] {
            let mut expected = None;
            for enabled in [false, true] {
                let (mut family, kinds) = fixture(enabled, reverse);
                for union in [false, true] {
                    let view = family
                        .view_with_kind_union(&kinds, &control, union)
                        .unwrap();
                    if enabled {
                        assert_eq!(view.memo_class(LEFT) == view.memo_class(RIGHT), !reverse);
                    }
                    for collect in [false, true] {
                        let root = evaluate(&view, &universe, ROOT, 0, collect);
                        assert_eq!(root.covered_patterns.words(), &[0b0101]);
                        assert_eq!(root.covered_pattern_count, 2);
                        assert_eq!(root.covered_weight, 0.5);
                        if let Some(expected) = &expected {
                            same_coverage(&root, expected);
                        } else {
                            expected = Some(root);
                        }
                        let left = evaluate(&view, &universe, LEFT, 1, collect);
                        let right = evaluate(&view, &universe, RIGHT, 1, collect);
                        assert_eq!(left.covered_patterns.words(), &[0b0101]);
                        assert_eq!(
                            right.covered_patterns.words(),
                            &[if reverse { 0b1010 } else { 0b0101 }]
                        );
                    }
                    // Inferred complete supply is already known at depth 0;
                    // the boundary-only provider still keeps physical choices.
                    let known = source_universe(&[I, I]);
                    assert_eq!(
                        evaluate(&view, &known, ROOT, 0, false).covered_pattern_count,
                        2
                    );
                    // All legal actions fail this source. Dedup keeps the
                    // original first edge rather than deleting a zero class.
                    let rejected = source_universe(&[Z, Z]);
                    assert_eq!(
                        evaluate(&view, &rejected, ROOT, 0, false).covered_pattern_count,
                        0
                    );
                    assert_eq!(view.physical_action_cells(ROOT, 0), Some(0xf));
                }
            }
        }
    }

    #[test]
    fn ordered_classes_match_actual_ten_lock_completed_family() {
        use PieceKind::{I, J, L, O, S, T, Z};
        let first = vec![J, I, O, T, Z, S, L, Z, T, I, O];
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(2411),
            PatternWeightModelId::new(2412),
            vec![first.clone(), vec![J, I, O, T, Z, S, L, Z, T, I, S], first],
            vec![ProbabilityValue::new(1.0 / 3.0).unwrap(); 3],
            3,
            true,
            None,
        )
        .unwrap();
        let control = ExecutionControl::default();
        let mut expected = None;
        for enabled in [false, true] {
            let mut plan = source_filter_plan(true);
            let scope = Arc::get_mut(&mut plan).unwrap();
            scope.boundary_classes_enabled = true;
            scope.edge_closure = EdgeClosureMode::BeforeLastPhysicalLayer;
            scope.ordered_classes_enabled = enabled;
            let mut physical = PhysicalActionLanguage::default();
            physical.set_board_plan(Arc::clone(&plan)).unwrap();
            let mut kinds = PieceOrderLanguageCache::default();
            for alternate in [false, true] {
                physical
                    .produce_board_relation(
                        fresh_bag_orders_for_tail(alternate),
                        &mut kinds,
                        &mut SymbolicPrefixWork::default(),
                        &control,
                        0,
                    )
                    .unwrap();
            }
            let source = physical.board.take().unwrap();
            let identity: Vec<_> = (0..kinds.node_count() as u32).collect();
            let mut family = BoardActionFamily::new(plan);
            family
                .merge_completed_boundaries(vec![source], &[&identity], 0, &kinds, &control)
                .unwrap();
            let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
                10,
                4,
                0,
                (1_u64 << 40) - 1,
            )
            .unwrap();
            let mut reachability = ReachabilityWorkspace::default();
            reachability.configure_kick_profile(KickTableProfileId::Jstris180, false);
            family
                .complete_physical_edges(&catalog, &mut reachability, &control)
                .unwrap();
            let before = witness(&family);
            for union in [false, true] {
                let view = family
                    .view_with_kind_union(&kinds, &control, union)
                    .unwrap();
                for collect in [false, true] {
                    let coverage = evaluate(&view, &universe, view.root(), 0, collect);
                    assert_eq!(coverage.covered_pattern_count, 3);
                    if let Some(expected) = &expected {
                        same_coverage(&coverage, expected);
                    } else {
                        expected = Some(coverage);
                    }
                }
            }
            assert_eq!(witness(&family), before);
            assert_eq!(family.ordered_class_metrics()[0] > 0, enabled);
        }
    }

    #[test]
    fn ordered_classes_fail_closed_without_publishing_partial_tables() {
        let control = ExecutionControl::default();
        let (mut scope, _) = fixture(true, false);
        Arc::get_mut(&mut scope.plan).unwrap().source_filter = false;
        assert!(matches!(
            scope.prepare_ordered_policy_quotient(&control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_ordered_class_scope_invalid"
            ))
        ));
        assert!(scope.failed && scope.ordered_quotient.is_none());

        let (mut cancelled, _) = fixture(true, false);
        let stop = ExecutionControl::default();
        stop.cancellation.handle().cancel();
        assert!(matches!(
            cancelled.prepare_ordered_policy_quotient(&stop),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert!(cancelled.failed && cancelled.ordered_quotient.is_none());
        assert!(cancelled.prepare_ordered_policy_quotient(&control).is_err());

        let (mut limited, _) = fixture(true, false);
        limited.limits = GenerationLimits::DEVELOPMENT;
        limited.metrics.work = MAX_WORK - 1;
        assert!(matches!(
            limited.prepare_ordered_policy_quotient(&control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        assert_eq!(limited.metrics.work, MAX_WORK - 1);
        assert!(limited.failed && limited.ordered_quotient.is_none());
        assert!(limited.ordered_class_metrics()[7] > 0);

        let (mut members, _) = fixture(true, false);
        members.limits = GenerationLimits::DEVELOPMENT;
        members.metrics.suffix_roots = MAX_UNION_MEMBERS;
        assert!(matches!(
            members.prepare_ordered_policy_quotient(&control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_union_limit"
            ))
        ));
        assert!(members.failed && members.ordered_quotient.is_none());
        assert!(members.metrics.work > 0);

        let (mut malformed, _) = fixture(true, false);
        malformed.nodes[RIGHT as usize].edges[2].cells =
            malformed.nodes[RIGHT as usize].edges[1].cells;
        assert!(matches!(
            malformed.prepare_ordered_policy_quotient(&control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_action_order_invalid"
            ))
        ));
        assert!(malformed.failed && malformed.ordered_quotient.is_none());
    }
}
