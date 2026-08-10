use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering as AtomicOrdering},
        Arc,
    },
};

// SRP rationale: this exact queue-observation policy engine has one change reason:
// preserving observation-equivalent actions across its trie, hold transitions,
// weighted policy evaluation, and coverage materialization.

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_problem::SetupTerminalSupplyTarget;
use clearra_supply::{
    pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
    QueueObservationPolicy,
};

use super::{
    exact_collections::{ExactHashMap, ExactHashSet},
    piece_order_language::PieceOrderLanguageCache,
    WasmExactSearchError,
};

const NO_NODE: u32 = u32::MAX;
const NO_TERMINAL: u32 = u32::MAX;
const CANCELLATION_POLL_MASK: u32 = 0xff;
const MAX_REVEAL_BRANCHES: usize = 49;
const TERMINAL_USED_HELD_CODE: u8 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ObservationLanguageNode {
    pub accepting: bool,
    pub depth: u8,
}

pub(super) trait ObservationPieceLanguage {
    fn root(&self) -> u32;
    fn node(&self, node: u32) -> Option<ObservationLanguageNode>;
    fn edge_count(&self, node: u32) -> Option<usize>;
    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)>;

    fn memo_class(&self, node: u32) -> Option<u32> {
        self.node(node).map(|_| node)
    }

    /// True only when this node's memo class has the same language semantics
    /// in later evaluations performed by the same evaluator.
    fn memo_reusable(&self, _node: u32) -> bool {
        false
    }

    fn reusable_memo_domain(&self) -> Option<u64> {
        None
    }
}

#[derive(Clone, Copy, Debug)]
struct UnionLanguageNode {
    children: [u32; 7],
    accepting: bool,
    depth: u8,
}

pub(super) struct RootedPieceLanguageUnion {
    nodes: Vec<UnionLanguageNode>,
    root: u32,
}

impl RootedPieceLanguageUnion {
    pub fn new(
        language: &PieceOrderLanguageCache,
        roots: &[u32],
    ) -> Result<Self, WasmExactSearchError> {
        let total_depth = roots
            .first()
            .and_then(|root| language.node(*root))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_observation_language_root_out_of_range",
            ))?
            .remaining_depth;
        let mut root_set = Vec::new();
        root_set.try_reserve_exact(roots.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_observation_language_union_storage_unavailable",
            )
        })?;
        root_set.extend_from_slice(roots);
        let mut builder = PieceLanguageUnionBuilder {
            source: language,
            nodes: Vec::new(),
            interned: HashMap::new(),
            total_depth,
        };
        let root = builder.intern_set(root_set)?;
        Ok(Self {
            nodes: builder.nodes,
            root,
        })
    }

    pub fn retained_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<UnionLanguageNode>()
    }
}

struct PieceLanguageUnionBuilder<'a> {
    source: &'a PieceOrderLanguageCache,
    nodes: Vec<UnionLanguageNode>,
    interned: HashMap<Vec<u32>, u32>,
    total_depth: u8,
}

impl PieceLanguageUnionBuilder<'_> {
    fn intern_set(&mut self, mut source_nodes: Vec<u32>) -> Result<u32, WasmExactSearchError> {
        source_nodes.sort_unstable();
        source_nodes.dedup();
        if let Some(reference) = self.interned.get(source_nodes.as_slice()).copied() {
            return Ok(reference);
        }
        let first = source_nodes
            .first()
            .and_then(|node| self.source.node(*node))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_observation_language_root_out_of_range",
            ))?;
        let remaining_depth = first.remaining_depth;
        let mut accepting = false;
        let mut child_sets: [Vec<u32>; 7] = std::array::from_fn(|_| Vec::new());
        for source_node in source_nodes.iter().copied() {
            let node =
                self.source
                    .node(source_node)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_language_node_out_of_range",
                    ))?;
            if node.remaining_depth != remaining_depth {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_language_union_depth_mismatch",
                ));
            }
            accepting |= node.accepting;
            for (piece_index, child_set) in child_sets.iter_mut().enumerate() {
                let edges = self
                    .source
                    .edges_for_piece(source_node, piece_index as u8 + 1)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_language_edge_out_of_range",
                    ))?;
                child_set.try_reserve(edges.len()).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_observation_language_union_storage_unavailable",
                    )
                })?;
                child_set.extend(edges.iter().map(|edge| edge.child()));
            }
        }

        let mut children = [NO_NODE; 7];
        for (piece_index, mut child_set) in child_sets.into_iter().enumerate() {
            if child_set.is_empty() {
                continue;
            }
            child_set.sort_unstable();
            child_set.dedup();
            children[piece_index] = self.intern_set(child_set)?;
        }
        let reference = u32::try_from(self.nodes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_observation_language_union_node_index_overflow",
            )
        })?;
        self.nodes.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_observation_language_union_storage_unavailable",
            )
        })?;
        self.interned.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_observation_language_union_storage_unavailable",
            )
        })?;
        self.nodes.push(UnionLanguageNode {
            children,
            accepting,
            depth: self.total_depth.checked_sub(remaining_depth).ok_or(
                WasmExactSearchError::InvalidProblem(
                    "wasm_observation_language_union_depth_mismatch",
                ),
            )?,
        });
        self.interned.insert(source_nodes, reference);
        Ok(reference)
    }
}

impl ObservationPieceLanguage for RootedPieceLanguageUnion {
    fn root(&self) -> u32 {
        self.root
    }

    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        self.nodes
            .get(node as usize)
            .map(|node| ObservationLanguageNode {
                accepting: node.accepting,
                depth: node.depth,
            })
    }

    fn edge_count(&self, node: u32) -> Option<usize> {
        self.nodes.get(node as usize).map(|node| {
            node.children
                .iter()
                .filter(|child| **child != NO_NODE)
                .count()
        })
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        self.nodes
            .get(node as usize)?
            .children
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, child)| *child != NO_NODE)
            .nth(index)
            .map(|(piece_index, child)| (piece_index as u8 + 1, child))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct QueueObservationMetrics {
    pub policy_states: usize,
    pub action_checks: usize,
    pub observation_nodes: usize,
    pub retained_bytes: usize,
    pub local_values: usize,
    pub local_zero_scores: usize,
    pub shared_values: usize,
    pub shared_zero_scores: usize,
    pub local_value_capacity: usize,
    pub local_zero_score_capacity: usize,
    pub shared_value_capacity: usize,
    pub shared_zero_score_capacity: usize,
    pub shared_hits: usize,
}

pub(super) struct QueueObservationCoverage {
    pub covered_patterns: PatternBitSet,
    pub covered_pattern_count: usize,
    pub covered_weight: f64,
    pub min_accepted_depth: Option<u8>,
    pub max_accepted_depth: Option<u8>,
    pub metrics: QueueObservationMetrics,
}

#[derive(Clone, Copy, Debug)]
struct ObservationTrieNode {
    children: [u32; 7],
    parent: u32,
    first_terminal: u32,
    subtree_weight: f64,
    subtree_count: u32,
    depth: u16,
    piece_code: u8,
}

impl ObservationTrieNode {
    fn root() -> Self {
        Self {
            children: [NO_NODE; 7],
            parent: NO_NODE,
            first_terminal: NO_TERMINAL,
            subtree_weight: 0.0,
            subtree_count: 0,
            depth: 0,
            piece_code: 0,
        }
    }

    fn child(parent: u32, depth: u16, piece_code: u8) -> Self {
        Self {
            children: [NO_NODE; 7],
            parent,
            first_terminal: NO_TERMINAL,
            subtree_weight: 0.0,
            subtree_count: 0,
            depth,
            piece_code,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TerminalPattern {
    pattern_id: u32,
    weight: f64,
    next: u32,
}

struct ObservationTrie {
    nodes: Vec<ObservationTrieNode>,
    terminals: Vec<TerminalPattern>,
    initial_observations: Vec<u32>,
    future_classes: Vec<u32>,
    sequence_len: usize,
    materialized_sequence_len: usize,
    global_pattern_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ObservationFutureSignature {
    children: [u32; 7],
    subtree_weight_bits: u64,
    subtree_count: u32,
}

impl ObservationTrie {
    #[allow(clippy::too_many_arguments)]
    fn compile(
        universe: &MaterializedPatternUniverse,
        pattern_index: &PatternPiecePositionIndex,
        initial_cursor: usize,
        visible_piece_count: usize,
        projects_standard_bag_lookahead: bool,
    ) -> Result<Self, WasmExactSearchError> {
        let mut trie = Self {
            nodes: vec![ObservationTrieNode::root()],
            terminals: Vec::new(),
            initial_observations: Vec::new(),
            future_classes: Vec::new(),
            sequence_len: 0,
            materialized_sequence_len: pattern_index.sequence_len(),
            global_pattern_count: pattern_index.global_pattern_count(),
        };
        let mut sequence = Vec::new();
        let mut expected_len = None;
        for local_pattern_index in 0..pattern_index.local_pattern_count() {
            let global_pattern_index = pattern_index
                .global_pattern_index(local_pattern_index)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_pattern_index_missing",
                ))?;
            universe.write_sequence_at(global_pattern_index, &mut sequence);
            if projects_standard_bag_lookahead {
                append_projected_standard_bag_piece(&mut sequence)?;
            }
            match expected_len {
                Some(len) if len != sequence.len() => {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_requires_uniform_sequence_length",
                    ));
                }
                None => expected_len = Some(sequence.len()),
                _ => {}
            }
            let weight = universe.weight_at(global_pattern_index).get();
            trie.insert(
                &sequence,
                u32::try_from(global_pattern_index).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_observation_pattern_index_overflow")
                })?,
                weight,
            )?;
        }
        trie.sequence_len = expected_len.unwrap_or(0);
        if initial_cursor > trie.sequence_len {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_initial_cursor_out_of_range",
            ));
        }
        let initial_depth = initial_cursor
            .saturating_add(visible_piece_count)
            .min(trie.sequence_len);
        let mut initial_observations = Vec::new();
        trie.collect_descendants_vec(0, initial_depth, &mut initial_observations)?;
        trie.initial_observations = initial_observations;
        Ok(trie)
    }

    fn compile_future_classes(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.future_classes.len() == self.nodes.len() {
            return Ok(());
        }
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let mut classes = Vec::new();
        classes.try_reserve_exact(self.nodes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_observation_future_class_storage_unavailable",
            )
        })?;
        classes.resize(self.nodes.len(), NO_NODE);
        let mut interned = ExactHashMap::<ObservationFutureSignature, u32>::default();
        let mut work = 0_u32;
        for node_index in (0..self.nodes.len()).rev() {
            work = work.wrapping_add(1);
            if work & CANCELLATION_POLL_MASK == 0 && control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            let node = self.nodes[node_index];
            let mut children = [NO_NODE; 7];
            for (piece_index, child) in node.children.iter().copied().enumerate() {
                if child == NO_NODE {
                    continue;
                }
                children[piece_index] =
                    *classes
                        .get(child as usize)
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_observation_future_class_child_out_of_range",
                        ))?;
                if children[piece_index] == NO_NODE {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_future_class_not_topological",
                    ));
                }
            }
            let signature = ObservationFutureSignature {
                children,
                subtree_weight_bits: node.subtree_weight.to_bits(),
                subtree_count: node.subtree_count,
            };
            let class = if let Some(class) = interned.get(&signature).copied() {
                class
            } else {
                let class = u32::try_from(interned.len()).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_observation_future_class_index_overflow",
                    )
                })?;
                interned.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_observation_future_class_storage_unavailable",
                    )
                })?;
                interned.insert(signature, class);
                class
            };
            classes[node_index] = class;
        }
        self.future_classes = classes;
        Ok(())
    }

    fn insert(
        &mut self,
        sequence: &[clearra_core_domain::piece::piece_kind::PieceKind],
        pattern_id: u32,
        weight: f64,
    ) -> Result<(), WasmExactSearchError> {
        let mut node_index = 0_u32;
        self.include_pattern(node_index, weight)?;
        for (depth, piece) in sequence.iter().copied().enumerate() {
            let piece_code = piece_code(piece);
            let piece_index = usize::from(piece_code - 1);
            let existing = self.nodes[node_index as usize].children[piece_index];
            let child = if existing != NO_NODE {
                existing
            } else {
                let child = u32::try_from(self.nodes.len()).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_observation_trie_node_index_overflow",
                    )
                })?;
                self.nodes.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_observation_trie_storage_unavailable",
                    )
                })?;
                self.nodes.push(ObservationTrieNode::child(
                    node_index,
                    u16::try_from(depth + 1).map_err(|_| {
                        WasmExactSearchError::InvalidProblem("wasm_observation_sequence_too_long")
                    })?,
                    piece_code,
                ));
                self.nodes[node_index as usize].children[piece_index] = child;
                child
            };
            node_index = child;
            self.include_pattern(node_index, weight)?;
        }
        let terminal_index = u32::try_from(self.terminals.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_observation_terminal_index_overflow")
        })?;
        self.terminals.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_observation_terminal_storage_unavailable")
        })?;
        self.terminals.push(TerminalPattern {
            pattern_id,
            weight,
            next: self.nodes[node_index as usize].first_terminal,
        });
        self.nodes[node_index as usize].first_terminal = terminal_index;
        Ok(())
    }

    fn include_pattern(
        &mut self,
        node_index: u32,
        weight: f64,
    ) -> Result<(), WasmExactSearchError> {
        let node = &mut self.nodes[node_index as usize];
        node.subtree_count =
            node.subtree_count
                .checked_add(1)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_pattern_count_overflow",
                ))?;
        node.subtree_weight += weight;
        Ok(())
    }

    fn piece_at(&self, mut node: u32, position: usize) -> Option<u8> {
        let target_depth = position.checked_add(1)?;
        while usize::from(self.nodes.get(node as usize)?.depth) > target_depth {
            node = self.nodes[node as usize].parent;
        }
        (usize::from(self.nodes.get(node as usize)?.depth) == target_depth)
            .then_some(self.nodes[node as usize].piece_code)
    }

    fn collect_descendants_vec(
        &self,
        node: u32,
        target_depth: usize,
        output: &mut Vec<u32>,
    ) -> Result<(), WasmExactSearchError> {
        let depth = usize::from(
            self.nodes
                .get(node as usize)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_trie_node_out_of_range",
                ))?
                .depth,
        );
        if depth == target_depth {
            output.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_observation_frontier_storage_unavailable",
                )
            })?;
            output.push(node);
            return Ok(());
        }
        if depth > target_depth {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_depth_regressed",
            ));
        }
        for child in self.nodes[node as usize].children {
            if child != NO_NODE {
                self.collect_descendants_vec(child, target_depth, output)?;
            }
        }
        Ok(())
    }

    fn collect_revealed_descendants(
        &self,
        node: u32,
        target_depth: usize,
        output: &mut [u32; MAX_REVEAL_BRANCHES],
        len: &mut usize,
    ) -> Result<(), WasmExactSearchError> {
        let depth = usize::from(
            self.nodes
                .get(node as usize)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_trie_node_out_of_range",
                ))?
                .depth,
        );
        if depth == target_depth {
            let slot = output
                .get_mut(*len)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_reveal_branch_capacity_exceeded",
                ))?;
            *slot = node;
            *len += 1;
            return Ok(());
        }
        if depth > target_depth {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_depth_regressed",
            ));
        }
        for child in self.nodes[node as usize].children {
            if child != NO_NODE {
                self.collect_revealed_descendants(child, target_depth, output, len)?;
            }
        }
        Ok(())
    }

    fn collect_patterns(
        &self,
        root: u32,
        output: &mut Vec<u32>,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let mut stack = vec![root];
        let mut work = 0_u32;
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        while let Some(node_index) = stack.pop() {
            work = work.wrapping_add(1);
            if work & CANCELLATION_POLL_MASK == 0 && control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            let node =
                self.nodes
                    .get(node_index as usize)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_trie_node_out_of_range",
                    ))?;
            let mut terminal = node.first_terminal;
            while terminal != NO_TERMINAL {
                let entry = self.terminals.get(terminal as usize).ok_or(
                    WasmExactSearchError::InvalidProblem("wasm_observation_terminal_out_of_range"),
                )?;
                output.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_observation_coverage_storage_unavailable",
                    )
                })?;
                output.push(entry.pattern_id);
                terminal = entry.next;
            }
            for child in node.children {
                if child != NO_NODE {
                    stack.push(child);
                }
            }
        }
        Ok(())
    }

    fn acceptance_value(
        &self,
        root: u32,
        source_cursor: u16,
        hold_code: u8,
        target: Option<SetupTerminalSupplyTarget>,
        control: &ExecutionControl,
    ) -> Result<(f64, u32), WasmExactSearchError> {
        let Some(target) = target else {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            let node =
                self.nodes
                    .get(root as usize)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_trie_node_out_of_range",
                    ))?;
            return Ok((node.subtree_weight, node.subtree_count));
        };
        let mut weight = 0.0;
        let mut pattern_count = 0_u32;
        self.visit_terminals(
            root,
            |leaf, terminal| {
                if self.terminal_supply_target_accepts(leaf, source_cursor, hold_code, target) {
                    weight += terminal.weight;
                    pattern_count = pattern_count.saturating_add(1);
                }
            },
            control,
        )?;
        Ok((weight, pattern_count))
    }

    fn collect_accepted_patterns(
        &self,
        root: u32,
        source_cursor: u16,
        hold_code: u8,
        target: Option<SetupTerminalSupplyTarget>,
        output: &mut Vec<u32>,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let Some(target) = target else {
            return self.collect_patterns(root, output, control);
        };
        self.visit_terminals(
            root,
            |leaf, terminal| {
                if self.terminal_supply_target_accepts(leaf, source_cursor, hold_code, target) {
                    output.push(terminal.pattern_id);
                }
            },
            control,
        )
    }

    fn visit_terminals(
        &self,
        root: u32,
        mut visit: impl FnMut(u32, &TerminalPattern),
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let mut stack = vec![root];
        let mut work = 0_u32;
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        while let Some(node_index) = stack.pop() {
            work = work.wrapping_add(1);
            if work & CANCELLATION_POLL_MASK == 0 && control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            let node =
                self.nodes
                    .get(node_index as usize)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_trie_node_out_of_range",
                    ))?;
            let mut terminal = node.first_terminal;
            while terminal != NO_TERMINAL {
                let entry = self.terminals.get(terminal as usize).ok_or(
                    WasmExactSearchError::InvalidProblem("wasm_observation_terminal_out_of_range"),
                )?;
                visit(node_index, entry);
                terminal = entry.next;
            }
            for child in node.children {
                if child != NO_NODE {
                    stack.push(child);
                }
            }
        }
        Ok(())
    }

    fn terminal_supply_target_accepts(
        &self,
        leaf: u32,
        source_cursor: u16,
        hold_code: u8,
        target: SetupTerminalSupplyTarget,
    ) -> bool {
        let queue_position = usize::from(source_cursor);
        let logical_hold = if hold_code == TERMINAL_USED_HELD_CODE {
            0
        } else {
            hold_code
        };
        let mut suffix_counts = target.counts();
        if logical_hold != 0 {
            let Some(count) = suffix_counts.get_mut(usize::from(logical_hold - 1)) else {
                return false;
            };
            if *count == 0 {
                return false;
            }
            *count -= 1;
        }
        let first_boundary = usize::from(target.first_bag_boundary());
        if queue_position < first_boundary {
            return self.sequence_range_counts(leaf, queue_position, first_boundary)
                == Some(suffix_counts);
        }
        let consumed_in_bag = (queue_position - first_boundary) % 7;
        if consumed_in_bag == 0 {
            let expected = if logical_hold == 0 { 1 } else { 0 };
            return suffix_counts.iter().all(|count| *count == expected);
        }
        if suffix_counts.iter().any(|count| *count > 1)
            || suffix_counts
                .iter()
                .map(|count| usize::from(*count))
                .sum::<usize>()
                != 7 - consumed_in_bag
        {
            return false;
        }
        let bag_start = queue_position - consumed_in_bag;
        let Some(consumed_counts) = self.sequence_range_counts(leaf, bag_start, queue_position)
        else {
            return false;
        };
        consumed_counts
            .iter()
            .zip(suffix_counts)
            .all(|(consumed, remaining)| consumed.saturating_add(remaining) == 1)
    }

    fn sequence_range_counts(&self, leaf: u32, start: usize, end: usize) -> Option<[u8; 7]> {
        let mut counts = [0_u8; 7];
        for position in start..end {
            let code = self.piece_at(leaf, position)?;
            let count = counts.get_mut(usize::from(code.checked_sub(1)?))?;
            *count = count.checked_add(1)?;
        }
        Some(counts)
    }

    fn retained_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<ObservationTrieNode>()
            + self.terminals.capacity() * core::mem::size_of::<TerminalPattern>()
            + self.initial_observations.capacity() * core::mem::size_of::<u32>()
            + self.future_classes.capacity() * core::mem::size_of::<u32>()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PolicyState {
    language_node: u32,
    observation_node: u32,
    source_cursor: u16,
    hold_code: u8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
struct PolicyMemoState(u128);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SupplyAction {
    UseCurrent,
    SwapHeld,
    StoreCurrentUseNext,
    ReleaseHeldAtTerminal,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PolicyTransition {
    child: u32,
    source_cursor: u16,
    hold_code: u8,
    action: SupplyAction,
}

#[derive(Clone, Copy, Debug)]
struct PolicyValue {
    weight: f64,
    pattern_count: u32,
}

impl PolicyValue {
    const REJECT: Self = Self {
        weight: 0.0,
        pattern_count: 0,
    };
}

pub(super) struct QueueObservationPolicyEvaluator {
    trie: Arc<ObservationTrie>,
    policy: QueueObservationPolicy,
    initial_cursor: usize,
    initial_hold_code: u8,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    terminal_supply_target: Option<SetupTerminalSupplyTarget>,
    memo: ExactHashMap<PolicyMemoState, PolicyValue>,
    zero_scores: ExactHashSet<PolicyMemoState>,
    shared_memo: ExactHashMap<PolicyMemoState, PolicyValue>,
    shared_zero_scores: ExactHashSet<PolicyMemoState>,
    shared_terminal_supply_target: Option<SetupTerminalSupplyTarget>,
    shared_memo_domain: Option<u64>,
    next_shared_memo_domain: u64,
    selected_patterns: Vec<u32>,
    selected_min_depth: u8,
    selected_max_depth: u8,
    cancellation_poll_counter: u32,
    action_checks: usize,
    shared_hits: usize,
    peer_abort: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    raw_memo_keys: bool,
}

impl QueueObservationPolicyEvaluator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        universe: &MaterializedPatternUniverse,
        pattern_index: &PatternPiecePositionIndex,
        policy: QueueObservationPolicy,
        initial_cursor: u16,
        initial_hold: Option<clearra_core_domain::piece::piece_kind::PieceKind>,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
        projects_standard_bag_lookahead: bool,
        terminal_supply_target: Option<SetupTerminalSupplyTarget>,
    ) -> Result<Self, WasmExactSearchError> {
        let visible_piece_count = usize::from(policy.visible_piece_count().ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_observation_evaluator_requires_visible_window",
            ),
        )?);
        let initial_cursor = usize::from(initial_cursor);
        let trie = ObservationTrie::compile(
            universe,
            pattern_index,
            initial_cursor,
            visible_piece_count,
            projects_standard_bag_lookahead,
        )?;
        Ok(Self {
            trie: Arc::new(trie),
            policy,
            initial_cursor,
            initial_hold_code: initial_hold.map_or(0, piece_code),
            hold_enabled,
            projects_unplaced_lookahead,
            terminal_supply_target,
            memo: ExactHashMap::default(),
            zero_scores: ExactHashSet::default(),
            shared_memo: ExactHashMap::default(),
            shared_zero_scores: ExactHashSet::default(),
            shared_terminal_supply_target: terminal_supply_target,
            shared_memo_domain: None,
            next_shared_memo_domain: 0,
            selected_patterns: Vec::new(),
            selected_min_depth: u8::MAX,
            selected_max_depth: 0,
            cancellation_poll_counter: 0,
            action_checks: 0,
            shared_hits: 0,
            peer_abort: None,
            #[cfg(test)]
            raw_memo_keys: false,
        })
    }

    pub fn evaluate<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        control: &ExecutionControl,
    ) -> Result<QueueObservationCoverage, WasmExactSearchError> {
        self.prepare_trie(control)?;
        if let Some(domain) = language.reusable_memo_domain() {
            if self.shared_memo_domain != Some(domain)
                || self.shared_terminal_supply_target != self.terminal_supply_target
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_shared_memo_domain_mismatch",
                ));
            }
        }
        self.memo.clear();
        self.zero_scores.clear();
        self.selected_patterns.clear();
        self.selected_min_depth = u8::MAX;
        self.selected_max_depth = 0;
        self.cancellation_poll_counter = 0;
        self.action_checks = 0;
        self.shared_hits = 0;
        let root = language.root();
        let initial_observations = self.trie.initial_observations.clone();
        let mut total_weight = 0.0;
        let mut total_count = 0usize;
        for observation_node in initial_observations.iter().copied() {
            let state = PolicyState {
                language_node: root,
                observation_node,
                source_cursor: u16::try_from(self.initial_cursor).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_observation_initial_cursor_overflow")
                })?,
                hold_code: self.initial_hold_code,
            };
            let value = self.solve(language, state, control)?;
            total_weight += value.weight;
            total_count = total_count.saturating_add(value.pattern_count as usize);
        }
        for observation_node in initial_observations {
            self.collect_selected(
                language,
                PolicyState {
                    language_node: root,
                    observation_node,
                    source_cursor: u16::try_from(self.initial_cursor).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_observation_initial_cursor_overflow",
                        )
                    })?,
                    hold_code: self.initial_hold_code,
                },
                control,
            )?;
        }
        let covered_patterns = PatternBitSet::from_pattern_indices(
            self.trie.global_pattern_count,
            self.selected_patterns.clone(),
        )
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_observation_coverage_materialization_failed")
        })?;
        Ok(QueueObservationCoverage {
            covered_patterns,
            covered_pattern_count: total_count,
            covered_weight: total_weight,
            min_accepted_depth: (self.selected_min_depth != u8::MAX)
                .then_some(self.selected_min_depth),
            max_accepted_depth: (self.selected_min_depth != u8::MAX)
                .then_some(self.selected_max_depth),
            metrics: QueueObservationMetrics {
                policy_states: self
                    .memo
                    .len()
                    .saturating_add(self.zero_scores.len())
                    .saturating_add(self.shared_memo.len())
                    .saturating_add(self.shared_zero_scores.len()),
                action_checks: self.action_checks,
                observation_nodes: self.trie.nodes.len(),
                retained_bytes: self.retained_bytes(),
                local_values: self.memo.len(),
                local_zero_scores: self.zero_scores.len(),
                shared_values: self.shared_memo.len(),
                shared_zero_scores: self.shared_zero_scores.len(),
                local_value_capacity: self.memo.capacity(),
                local_zero_score_capacity: self.zero_scores.capacity(),
                shared_value_capacity: self.shared_memo.capacity(),
                shared_zero_score_capacity: self.shared_zero_scores.capacity(),
                shared_hits: self.shared_hits,
            },
        })
    }

    pub fn set_terminal_supply_target(&mut self, target: Option<SetupTerminalSupplyTarget>) {
        if self.terminal_supply_target != target && self.shared_memo_domain.is_some() {
            self.shared_memo.clear();
            self.shared_zero_scores.clear();
            self.shared_memo_domain = None;
            self.shared_terminal_supply_target = target;
        }
        self.terminal_supply_target = target;
    }

    pub fn fork_empty(&self) -> Result<Self, WasmExactSearchError> {
        if self.trie.future_classes.len() != self.trie.nodes.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_trie_fork_before_prepare",
            ));
        }
        Ok(Self {
            trie: Arc::clone(&self.trie),
            policy: self.policy,
            initial_cursor: self.initial_cursor,
            initial_hold_code: self.initial_hold_code,
            hold_enabled: self.hold_enabled,
            projects_unplaced_lookahead: self.projects_unplaced_lookahead,
            terminal_supply_target: self.terminal_supply_target,
            memo: ExactHashMap::default(),
            zero_scores: ExactHashSet::default(),
            shared_memo: ExactHashMap::default(),
            shared_zero_scores: ExactHashSet::default(),
            shared_terminal_supply_target: self.terminal_supply_target,
            shared_memo_domain: None,
            next_shared_memo_domain: 0,
            selected_patterns: Vec::new(),
            selected_min_depth: u8::MAX,
            selected_max_depth: 0,
            cancellation_poll_counter: 0,
            action_checks: 0,
            shared_hits: 0,
            peer_abort: None,
            #[cfg(test)]
            raw_memo_keys: self.raw_memo_keys,
        })
    }

    fn prepare_trie(&mut self, control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
        if self.trie.future_classes.len() == self.trie.nodes.len() {
            return Ok(());
        }
        Arc::get_mut(&mut self.trie)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_observation_shared_trie_not_prepared",
            ))?
            .compile_future_classes(control)
    }

    pub fn begin_reusable_memo_domain(&mut self) -> u64 {
        self.shared_memo.clear();
        self.shared_zero_scores.clear();
        let domain = self.next_shared_memo_domain;
        self.next_shared_memo_domain = self.next_shared_memo_domain.wrapping_add(1);
        self.shared_memo_domain = Some(domain);
        self.shared_terminal_supply_target = self.terminal_supply_target;
        domain
    }

    pub fn set_peer_abort(&mut self, abort: Arc<AtomicBool>) {
        self.peer_abort = Some(abort);
    }

    pub fn retained_bytes(&self) -> usize {
        self.trie.retained_bytes()
            + self.memo.capacity()
                * (core::mem::size_of::<PolicyMemoState>() + core::mem::size_of::<PolicyValue>())
            + self.zero_scores.capacity() * core::mem::size_of::<PolicyMemoState>()
            + self.shared_memo.capacity()
                * (core::mem::size_of::<PolicyMemoState>() + core::mem::size_of::<PolicyValue>())
            + self.shared_zero_scores.capacity() * core::mem::size_of::<PolicyMemoState>()
            + self.selected_patterns.capacity() * core::mem::size_of::<u32>()
    }

    fn solve<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        control: &ExecutionControl,
    ) -> Result<PolicyValue, WasmExactSearchError> {
        self.poll_cancellation(control)?;
        let memo_state = self.memo_state(language, state)?;
        let reusable = language.memo_reusable(state.language_node);
        if reusable {
            if let Some(value) = self.shared_memo.get(&memo_state).copied() {
                self.shared_hits = self.shared_hits.saturating_add(1);
                return Ok(value);
            }
            if self.shared_zero_scores.contains(&memo_state) {
                self.shared_hits = self.shared_hits.saturating_add(1);
                return Ok(PolicyValue::REJECT);
            }
        } else {
            if let Some(value) = self.memo.get(&memo_state).copied() {
                return Ok(value);
            }
            if self.zero_scores.contains(&memo_state) {
                return Ok(PolicyValue::REJECT);
            }
        }
        let node =
            language
                .node(state.language_node)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_language_node_out_of_range",
                ))?;
        if node.accepting {
            let (weight, pattern_count) = self.trie.acceptance_value(
                state.observation_node,
                state.source_cursor,
                state.hold_code,
                self.terminal_supply_target,
                control,
            )?;
            let value = PolicyValue {
                weight,
                pattern_count,
            };
            self.insert_memo(memo_state, value, reusable)?;
            return Ok(value);
        }
        let (best, _) = self.best_transition(language, state, control, true)?;
        self.insert_memo(memo_state, best, reusable)?;
        Ok(best)
    }

    fn best_transition<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        control: &ExecutionControl,
        record_action_checks: bool,
    ) -> Result<(PolicyValue, Option<PolicyTransition>), WasmExactSearchError> {
        let cursor = usize::from(state.source_cursor);
        let current_piece = self.trie.piece_at(state.observation_node, cursor);
        let next_piece = self
            .trie
            .piece_at(state.observation_node, cursor.saturating_add(1));
        let edge_count = language.edge_count(state.language_node).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_observation_language_node_out_of_range"),
        )?;
        let mut best = PolicyValue::REJECT;
        let mut best_transition = None;
        for edge_index in 0..edge_count {
            let (desired_piece, child) = language.edge(state.language_node, edge_index).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_observation_language_edge_out_of_range"),
            )?;
            if current_piece == Some(desired_piece) {
                let transition = PolicyTransition {
                    child,
                    source_cursor: state.source_cursor.checked_add(1).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_observation_queue_position_overflow",
                        ),
                    )?,
                    hold_code: state.hold_code,
                    action: SupplyAction::UseCurrent,
                };
                self.consider(
                    language,
                    state,
                    transition,
                    &mut best,
                    &mut best_transition,
                    control,
                    record_action_checks,
                )?;
            }
            if self.hold_enabled && state.hold_code != 0 && state.hold_code == desired_piece {
                if let Some(current_piece) = current_piece {
                    let transition = PolicyTransition {
                        child,
                        source_cursor: state.source_cursor.checked_add(1).ok_or(
                            WasmExactSearchError::InvalidProblem(
                                "wasm_observation_queue_position_overflow",
                            ),
                        )?,
                        hold_code: current_piece,
                        action: SupplyAction::SwapHeld,
                    };
                    self.consider(
                        language,
                        state,
                        transition,
                        &mut best,
                        &mut best_transition,
                        control,
                        record_action_checks,
                    )?;
                }
            } else if self.hold_enabled && state.hold_code == 0 && next_piece == Some(desired_piece)
            {
                if let Some(current_piece) = current_piece {
                    let transition = PolicyTransition {
                        child,
                        source_cursor: state.source_cursor.checked_add(2).ok_or(
                            WasmExactSearchError::InvalidProblem(
                                "wasm_observation_queue_position_overflow",
                            ),
                        )?,
                        hold_code: current_piece,
                        action: SupplyAction::StoreCurrentUseNext,
                    };
                    self.consider(
                        language,
                        state,
                        transition,
                        &mut best,
                        &mut best_transition,
                        control,
                        record_action_checks,
                    )?;
                }
            }
            if self.projects_unplaced_lookahead
                && self.hold_enabled
                && state.hold_code == desired_piece
                && cursor == self.trie.materialized_sequence_len
                && language.node(child).is_some_and(|child| child.accepting)
            {
                let transition = PolicyTransition {
                    child,
                    source_cursor: state.source_cursor,
                    hold_code: TERMINAL_USED_HELD_CODE,
                    action: SupplyAction::ReleaseHeldAtTerminal,
                };
                self.consider(
                    language,
                    state,
                    transition,
                    &mut best,
                    &mut best_transition,
                    control,
                    record_action_checks,
                )?;
            }
        }
        Ok((best, best_transition))
    }

    fn memo_state<G: ObservationPieceLanguage>(
        &self,
        language: &G,
        state: PolicyState,
    ) -> Result<PolicyMemoState, WasmExactSearchError> {
        #[cfg(test)]
        if self.raw_memo_keys {
            let packed = u128::from(state.language_node)
                | (u128::from(state.observation_node) << 32)
                | (u128::from(state.source_cursor) << 64)
                | (u128::from(state.hold_code) << 80);
            return Ok(PolicyMemoState(packed));
        }
        let observation = self.trie.nodes.get(state.observation_node as usize).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_observation_trie_node_out_of_range"),
        )?;
        let source_cursor = usize::from(state.source_cursor);
        let observed_depth = usize::from(observation.depth);
        if source_cursor > observed_depth {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_cursor_beyond_observation",
            ));
        }
        let visible_len = observed_depth - source_cursor;
        if visible_len > 7 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_visible_window_too_long",
            ));
        }
        let mut visible_window = 0_u32;
        for (offset, position) in (source_cursor..observed_depth).enumerate() {
            let piece = self.trie.piece_at(state.observation_node, position).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_observation_visible_piece_missing"),
            )?;
            visible_window |= u32::from(piece) << (offset * 3);
        }

        let mut consumed_bag_counts = 0_u32;
        if let Some(target) = self.terminal_supply_target {
            let first_boundary = usize::from(target.first_bag_boundary());
            if source_cursor >= first_boundary {
                let consumed_in_bag = (source_cursor - first_boundary) % 7;
                let bag_start = source_cursor - consumed_in_bag;
                let counts = self
                    .trie
                    .sequence_range_counts(state.observation_node, bag_start, source_cursor)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_consumed_bag_history_missing",
                    ))?;
                for (piece_index, count) in counts.into_iter().enumerate() {
                    consumed_bag_counts |= u32::from(count) << (piece_index * 3);
                }
            }
        }

        let future_class = *self
            .trie
            .future_classes
            .get(state.observation_node as usize)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_observation_future_class_missing",
            ))?;
        let language_class = language.memo_class(state.language_node).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_observation_language_memo_class_missing"),
        )?;
        if visible_window >= (1 << 21)
            || consumed_bag_counts >= (1 << 21)
            || state.hold_code >= (1 << 4)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_memo_state_out_of_range",
            ));
        }
        let packed = u128::from(language_class)
            | (u128::from(future_class) << 32)
            | (u128::from(visible_window) << 64)
            | (u128::from(consumed_bag_counts) << 85)
            | (u128::from(state.source_cursor) << 106)
            | (u128::from(state.hold_code) << 122);
        Ok(PolicyMemoState(packed))
    }

    fn insert_memo(
        &mut self,
        state: PolicyMemoState,
        value: PolicyValue,
        reusable: bool,
    ) -> Result<(), WasmExactSearchError> {
        if value.weight == 0.0 && value.pattern_count == 0 {
            let zero_scores = if reusable {
                &mut self.shared_zero_scores
            } else {
                &mut self.zero_scores
            };
            zero_scores.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_observation_policy_storage_unavailable")
            })?;
            zero_scores.insert(state);
            return Ok(());
        }
        let memo = if reusable {
            &mut self.shared_memo
        } else {
            &mut self.memo
        };
        memo.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_observation_policy_storage_unavailable")
        })?;
        memo.insert(state, value);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn consider<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        transition: PolicyTransition,
        best: &mut PolicyValue,
        best_transition: &mut Option<PolicyTransition>,
        control: &ExecutionControl,
        record_action_check: bool,
    ) -> Result<(), WasmExactSearchError> {
        if record_action_check {
            self.action_checks = self.action_checks.saturating_add(1);
        }
        let value = self.transition_value(language, state, transition, control)?;
        let better = value.weight.total_cmp(&best.weight).is_gt()
            || (value.weight.total_cmp(&best.weight).is_eq()
                && (value.pattern_count > best.pattern_count
                    || (value.pattern_count == best.pattern_count
                        && best_transition.is_none_or(|current| transition < current))));
        if better {
            *best = value;
            *best_transition = Some(transition);
        }
        Ok(())
    }

    fn transition_value<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        transition: PolicyTransition,
        control: &ExecutionControl,
    ) -> Result<PolicyValue, WasmExactSearchError> {
        let _child =
            language
                .node(transition.child)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_language_node_out_of_range",
                ))?;
        if transition.action == SupplyAction::ReleaseHeldAtTerminal {
            return self.solve(
                language,
                PolicyState {
                    language_node: transition.child,
                    observation_node: state.observation_node,
                    source_cursor: transition.source_cursor,
                    hold_code: transition.hold_code,
                },
                control,
            );
        }
        let target_depth = usize::from(transition.source_cursor)
            .saturating_add(usize::from(
                self.policy
                    .visible_piece_count()
                    .expect("evaluator requires a visible window"),
            ))
            .min(self.trie.sequence_len);
        let mut observations = [NO_NODE; MAX_REVEAL_BRANCHES];
        let mut observation_count = 0usize;
        self.trie.collect_revealed_descendants(
            state.observation_node,
            target_depth,
            &mut observations,
            &mut observation_count,
        )?;
        let mut weight = 0.0;
        let mut pattern_count = 0_u32;
        for observation_node in observations[..observation_count].iter().copied() {
            let value = self.solve(
                language,
                PolicyState {
                    language_node: transition.child,
                    observation_node,
                    source_cursor: transition.source_cursor,
                    hold_code: transition.hold_code,
                },
                control,
            )?;
            weight += value.weight;
            pattern_count = pattern_count.saturating_add(value.pattern_count);
        }
        Ok(PolicyValue {
            weight,
            pattern_count,
        })
    }

    fn collect_selected<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.poll_cancellation(control)?;
        let memo_state = self.memo_state(language, state)?;
        let reusable = language.memo_reusable(state.language_node);
        let memo = if reusable {
            &self.shared_memo
        } else {
            &self.memo
        };
        let zero_scores = if reusable {
            &self.shared_zero_scores
        } else {
            &self.zero_scores
        };
        let value = memo.get(&memo_state).copied().unwrap_or_else(|| {
            debug_assert!(zero_scores.contains(&memo_state));
            PolicyValue::REJECT
        });
        let node =
            language
                .node(state.language_node)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_language_node_out_of_range",
                ))?;
        if node.accepting {
            if value.pattern_count != 0 {
                self.selected_min_depth = self.selected_min_depth.min(node.depth);
                self.selected_max_depth = self.selected_max_depth.max(node.depth);
            }
            return self.trie.collect_accepted_patterns(
                state.observation_node,
                state.source_cursor,
                state.hold_code,
                self.terminal_supply_target,
                &mut self.selected_patterns,
                control,
            );
        }
        let (_, Some(transition)) = self.best_transition(language, state, control, false)? else {
            return Ok(());
        };
        let _child =
            language
                .node(transition.child)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_language_node_out_of_range",
                ))?;
        if transition.action == SupplyAction::ReleaseHeldAtTerminal {
            return self.collect_selected(
                language,
                PolicyState {
                    language_node: transition.child,
                    observation_node: state.observation_node,
                    source_cursor: transition.source_cursor,
                    hold_code: transition.hold_code,
                },
                control,
            );
        }
        let target_depth = usize::from(transition.source_cursor)
            .saturating_add(usize::from(
                self.policy
                    .visible_piece_count()
                    .expect("evaluator requires a visible window"),
            ))
            .min(self.trie.sequence_len);
        let mut observations = [NO_NODE; MAX_REVEAL_BRANCHES];
        let mut observation_count = 0usize;
        self.trie.collect_revealed_descendants(
            state.observation_node,
            target_depth,
            &mut observations,
            &mut observation_count,
        )?;
        for observation_node in observations[..observation_count].iter().copied() {
            self.collect_selected(
                language,
                PolicyState {
                    language_node: transition.child,
                    observation_node,
                    source_cursor: transition.source_cursor,
                    hold_code: transition.hold_code,
                },
                control,
            )?;
        }
        Ok(())
    }

    fn poll_cancellation(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.cancellation_poll_counter = self.cancellation_poll_counter.wrapping_add(1);
        if self.cancellation_poll_counter & CANCELLATION_POLL_MASK == 0
            && (control.is_cancelled()
                || self
                    .peer_abort
                    .as_ref()
                    .is_some_and(|abort| abort.load(AtomicOrdering::Acquire)))
        {
            return Err(WasmExactSearchError::Cancelled);
        }
        Ok(())
    }
}

fn append_projected_standard_bag_piece(
    sequence: &mut Vec<clearra_core_domain::piece::piece_kind::PieceKind>,
) -> Result<(), WasmExactSearchError> {
    if sequence.len() % 7 != 6 {
        return Ok(());
    }
    let start = sequence.len() - 6;
    let mut mask = 0_u8;
    for piece in &sequence[start..] {
        mask |= 1_u8 << (piece_code(*piece) - 1);
    }
    let missing = (!mask) & 0x7f;
    if missing.count_ones() != 1 {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_observation_projected_bag_piece_invalid",
        ));
    }
    sequence.push(piece_from_code(missing.trailing_zeros() as u8 + 1));
    Ok(())
}

const fn piece_code(piece: clearra_core_domain::piece::piece_kind::PieceKind) -> u8 {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    match piece {
        PieceKind::I => 1,
        PieceKind::O => 2,
        PieceKind::T => 3,
        PieceKind::S => 4,
        PieceKind::Z => 5,
        PieceKind::J => 6,
        PieceKind::L => 7,
    }
}

const fn piece_from_code(code: u8) -> clearra_core_domain::piece::piece_kind::PieceKind {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    match code {
        1 => PieceKind::I,
        2 => PieceKind::O,
        3 => PieceKind::T,
        4 => PieceKind::S,
        5 => PieceKind::Z,
        6 => PieceKind::J,
        7 => PieceKind::L,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
        probability::probability_value::ProbabilityValue,
    };
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };
    use clearra_problem::{compile_setup_search_conditions, SetupSearchQuery};
    use clearra_supply::{
        pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
        QueueObservationPolicy,
    };

    use super::{
        piece_code, ObservationLanguageNode, ObservationPieceLanguage,
        QueueObservationPolicyEvaluator, WasmExactSearchError,
    };

    #[derive(Default)]
    struct TestLanguageNode {
        accepting: bool,
        depth: u8,
        edges: Vec<(u8, u32)>,
    }

    struct TestLanguage {
        nodes: Vec<TestLanguageNode>,
    }

    impl TestLanguage {
        fn from_sequences(sequences: &[Vec<PieceKind>]) -> Self {
            let mut language = Self {
                nodes: vec![TestLanguageNode::default()],
            };
            for sequence in sequences {
                let mut node = 0_u32;
                for piece in sequence {
                    let code = piece_code(*piece);
                    let existing = language.nodes[node as usize]
                        .edges
                        .iter()
                        .find_map(|(edge, child)| (*edge == code).then_some(*child));
                    node = existing.unwrap_or_else(|| {
                        let child = u32::try_from(language.nodes.len()).expect("test node");
                        let depth = language.nodes[node as usize].depth + 1;
                        language.nodes.push(TestLanguageNode {
                            accepting: false,
                            depth,
                            edges: Vec::new(),
                        });
                        language.nodes[node as usize].edges.push((code, child));
                        child
                    });
                }
                language.nodes[node as usize].accepting = true;
            }
            language
        }
    }

    impl ObservationPieceLanguage for TestLanguage {
        fn root(&self) -> u32 {
            0
        }

        fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
            self.nodes
                .get(node as usize)
                .map(|node| ObservationLanguageNode {
                    accepting: node.accepting,
                    depth: node.depth,
                })
        }

        fn edge_count(&self, node: u32) -> Option<usize> {
            self.nodes.get(node as usize).map(|node| node.edges.len())
        }

        fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
            self.nodes.get(node as usize)?.edges.get(index).copied()
        }
    }

    struct AliasedTieLanguage {
        nodes: Vec<TestLanguageNode>,
        memo_classes: Vec<u32>,
    }

    impl AliasedTieLanguage {
        fn new() -> Self {
            use PieceKind::{I, J, L, O, S, T, Z};

            let mut language = Self {
                nodes: (0..7).map(|_| TestLanguageNode::default()).collect(),
                memo_classes: vec![0, 1, 1, 10, 20, 20, 10],
            };
            language.nodes[0].edges = vec![(piece_code(I), 1), (piece_code(O), 2)];
            language.nodes[1].depth = 1;
            language.nodes[1].edges = vec![(piece_code(T), 3), (piece_code(T), 4)];
            language.nodes[2].depth = 1;
            language.nodes[2].edges = vec![(piece_code(T), 5), (piece_code(T), 6)];
            for node in &mut language.nodes[3..7] {
                node.depth = 2;
            }
            language.append_path(3, &[S, Z, J, L, O, S, I], 100);
            language.append_path(6, &[S, Z, J, L, O, S, I], 100);
            language.append_path(4, &[S, Z, J, L, O, S, O], 200);
            language.append_path(5, &[S, Z, J, L, O, S, O], 200);
            language
        }

        fn append_path(&mut self, start: u32, pieces: &[PieceKind], class_base: u32) {
            let mut node = start;
            for (offset, piece) in pieces.iter().copied().enumerate() {
                let child = self.nodes.len() as u32;
                let depth = self.nodes[node as usize].depth + 1;
                self.nodes.push(TestLanguageNode {
                    accepting: false,
                    depth,
                    edges: Vec::new(),
                });
                self.memo_classes.push(class_base + offset as u32 + 1);
                self.nodes[node as usize]
                    .edges
                    .push((piece_code(piece), child));
                node = child;
            }
            self.nodes[node as usize].accepting = true;
        }
    }

    impl ObservationPieceLanguage for AliasedTieLanguage {
        fn root(&self) -> u32 {
            0
        }

        fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
            self.nodes
                .get(node as usize)
                .map(|node| ObservationLanguageNode {
                    accepting: node.accepting,
                    depth: node.depth,
                })
        }

        fn edge_count(&self, node: u32) -> Option<usize> {
            self.nodes.get(node as usize).map(|node| node.edges.len())
        }

        fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
            self.nodes.get(node as usize)?.edges.get(index).copied()
        }

        fn memo_class(&self, node: u32) -> Option<u32> {
            self.memo_classes.get(node as usize).copied()
        }
    }

    struct ReusableLanguage<'a> {
        inner: &'a AliasedTieLanguage,
        domain: u64,
    }

    impl ObservationPieceLanguage for ReusableLanguage<'_> {
        fn root(&self) -> u32 {
            self.inner.root()
        }

        fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
            self.inner.node(node)
        }

        fn edge_count(&self, node: u32) -> Option<usize> {
            self.inner.edge_count(node)
        }

        fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
            self.inner.edge(node, index)
        }

        fn memo_class(&self, node: u32) -> Option<u32> {
            self.inner.memo_class(node)
        }

        fn memo_reusable(&self, node: u32) -> bool {
            self.inner.node(node).is_some()
        }

        fn reusable_memo_domain(&self) -> Option<u64> {
            Some(self.domain)
        }
    }

    fn two_hidden_suffix_universe() -> (MaterializedPatternUniverse, PatternPiecePositionIndex) {
        use PieceKind::{I, J, L, O, S, T, Z};

        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(1),
            PatternWeightModelId::new(1),
            vec![vec![I, O, T, S, Z, J, L, I], vec![I, O, T, S, Z, J, L, O]],
            vec![
                ProbabilityValue::new(0.5).expect("weight"),
                ProbabilityValue::new(0.5).expect("weight"),
            ],
            2,
            true,
            None,
        )
        .expect("pattern universe");
        let index = PatternPiecePositionIndex::compile(&universe).expect("pattern index");
        (universe, index)
    }

    fn evaluator(
        universe: &MaterializedPatternUniverse,
        index: &PatternPiecePositionIndex,
    ) -> QueueObservationPolicyEvaluator {
        QueueObservationPolicyEvaluator::new(
            universe,
            index,
            QueueObservationPolicy::VisibleSeven,
            0,
            None,
            true,
            true,
            false,
            None,
        )
        .expect("visible-seven evaluator")
    }

    fn aliased_tie_universe() -> (MaterializedPatternUniverse, PatternPiecePositionIndex) {
        use PieceKind::{I, J, L, O, S, T, Z};

        let i_hidden_i = vec![I, T, S, Z, J, L, O, S, I];
        let i_hidden_o = vec![I, T, S, Z, J, L, O, S, O];
        let o_hidden_i = vec![O, T, S, Z, J, L, O, S, I];
        let o_hidden_o = vec![O, T, S, Z, J, L, O, S, O];
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(2),
            PatternWeightModelId::new(2),
            vec![
                i_hidden_i.clone(),
                i_hidden_i,
                i_hidden_o.clone(),
                i_hidden_o,
                o_hidden_i.clone(),
                o_hidden_i,
                o_hidden_o.clone(),
                o_hidden_o,
            ],
            vec![
                ProbabilityValue::new(0.05).expect("weight"),
                ProbabilityValue::new(0.2).expect("weight"),
                ProbabilityValue::new(0.05).expect("weight"),
                ProbabilityValue::new(0.2).expect("weight"),
                ProbabilityValue::new(0.05).expect("weight"),
                ProbabilityValue::new(0.2).expect("weight"),
                ProbabilityValue::new(0.05).expect("weight"),
                ProbabilityValue::new(0.2).expect("weight"),
            ],
            8,
            true,
            None,
        )
        .expect("aliased universe");
        let index = PatternPiecePositionIndex::compile(&universe).expect("pattern index");
        (universe, index)
    }

    #[test]
    fn hidden_eighth_piece_cannot_select_a_different_first_action() {
        use PieceKind::{I, J, L, O, S, T, Z};

        let (universe, index) = two_hidden_suffix_universe();
        let language = TestLanguage::from_sequences(&[
            vec![I, O, T, S, Z, J, L, I],
            vec![O, I, T, S, Z, J, L, O],
        ]);
        let coverage = evaluator(&universe, &index)
            .evaluate(&language, &ExecutionControl::default())
            .expect("policy coverage");

        assert_eq!(coverage.covered_pattern_count, 1);
        assert_eq!(coverage.covered_weight, 0.5);
        assert_eq!(coverage.min_accepted_depth, Some(8));
        assert_eq!(coverage.max_accepted_depth, Some(8));
    }

    #[test]
    fn policy_can_branch_after_the_eighth_piece_is_revealed() {
        use PieceKind::{I, J, L, O, S, T, Z};

        let (universe, index) = two_hidden_suffix_universe();
        let language = TestLanguage::from_sequences(&[
            vec![I, O, T, S, Z, J, L, I],
            vec![I, O, T, S, Z, J, L, O],
        ]);
        let coverage = evaluator(&universe, &index)
            .evaluate(&language, &ExecutionControl::default())
            .expect("policy coverage");

        assert_eq!(coverage.covered_pattern_count, 2);
        assert_eq!(coverage.covered_weight, 1.0);
        assert_eq!(coverage.min_accepted_depth, Some(8));
        assert_eq!(coverage.max_accepted_depth, Some(8));
    }

    #[test]
    fn cancelled_policy_materialization_fails_closed() {
        let (universe, index) = two_hidden_suffix_universe();
        let language = TestLanguage::from_sequences(&[Vec::new()]);
        let control = ExecutionControl::default();
        control.cancellation.handle().cancel();

        let result = evaluator(&universe, &index).evaluate(&language, &control);

        assert!(matches!(result, Err(WasmExactSearchError::Cancelled)));
    }

    #[test]
    fn memo_quotient_preserves_raw_tie_selected_patterns_exactly() {
        let (universe, index) = aliased_tie_universe();
        let language = AliasedTieLanguage::new();
        let control = ExecutionControl::default();
        let mut optimized = evaluator(&universe, &index);
        let mut identity = evaluator(&universe, &index);
        identity.raw_memo_keys = true;

        let optimized_coverage = optimized
            .evaluate(&language, &control)
            .expect("quotient coverage");
        let identity_coverage = identity
            .evaluate(&language, &control)
            .expect("identity coverage");

        assert_eq!(language.memo_class(1), language.memo_class(2));
        assert_ne!(language.edge(1, 0), language.edge(2, 0));
        assert_eq!(
            optimized_coverage.covered_patterns.covered_patterns(),
            identity_coverage.covered_patterns.covered_patterns()
        );
        assert_eq!(
            optimized_coverage.covered_pattern_count,
            identity_coverage.covered_pattern_count
        );
        assert_eq!(
            optimized_coverage.covered_weight,
            identity_coverage.covered_weight
        );
        assert_eq!(
            optimized_coverage.min_accepted_depth,
            identity_coverage.min_accepted_depth
        );
        assert_eq!(
            optimized_coverage.max_accepted_depth,
            identity_coverage.max_accepted_depth
        );
        assert_eq!(optimized_coverage.covered_pattern_count, 4);
        assert_eq!(optimized_coverage.covered_weight, 0.5);
        assert!(optimized_coverage.metrics.policy_states < identity_coverage.metrics.policy_states);
    }

    #[test]
    fn reusable_score_memo_preserves_raw_replay_on_later_evaluations() {
        let (universe, index) = aliased_tie_universe();
        let language = AliasedTieLanguage::new();
        let control = ExecutionControl::default();
        let mut evaluator = evaluator(&universe, &index);
        let language = ReusableLanguage {
            inner: &language,
            domain: evaluator.begin_reusable_memo_domain(),
        };

        let first = evaluator
            .evaluate(&language, &control)
            .expect("first coverage");
        let second = evaluator
            .evaluate(&language, &control)
            .expect("reused coverage");

        assert_eq!(
            first.covered_patterns.covered_patterns(),
            second.covered_patterns.covered_patterns()
        );
        assert_eq!(first.covered_pattern_count, second.covered_pattern_count);
        assert_eq!(first.covered_weight, second.covered_weight);
        assert_eq!(first.min_accepted_depth, second.min_accepted_depth);
        assert_eq!(first.max_accepted_depth, second.max_accepted_depth);
        assert!(first.metrics.action_checks > 0);
        assert_eq!(second.metrics.action_checks, 0);
    }

    #[test]
    fn terminal_target_change_invalidates_the_reusable_memo_domain() {
        use PieceKind::{I, O, S, T, Z};

        let (universe, index) = aliased_tie_universe();
        let inner = AliasedTieLanguage::new();
        let control = ExecutionControl::default();
        let mut evaluator = evaluator(&universe, &index);
        let old_domain = evaluator.begin_reusable_memo_domain();
        let old_language = ReusableLanguage {
            inner: &inner,
            domain: old_domain,
        };
        evaluator
            .evaluate(&old_language, &control)
            .expect("initial reusable coverage");

        let target = compile_setup_search_conditions(
            &SetupSearchQuery::default()
                .with_remaining_pieces(vec![I, O, T, S])
                .with_next_cycle_remaining_pieces(vec![Z]),
        )
        .expect("terminal condition")
        .remove(0)
        .terminal_supply_target()
        .expect("terminal target");
        evaluator.set_terminal_supply_target(Some(target));
        assert!(matches!(
            evaluator.evaluate(&old_language, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_shared_memo_domain_mismatch"
            ))
        ));

        let new_language = ReusableLanguage {
            inner: &inner,
            domain: evaluator.begin_reusable_memo_domain(),
        };
        evaluator
            .evaluate(&new_language, &control)
            .expect("new terminal memo domain");
    }

    #[test]
    fn zero_valued_legal_transition_is_memoized_and_replayed() {
        let (universe, index) = two_hidden_suffix_universe();
        let language = TestLanguage {
            nodes: vec![
                TestLanguageNode {
                    accepting: false,
                    depth: 0,
                    edges: vec![(piece_code(PieceKind::I), 1)],
                },
                TestLanguageNode {
                    accepting: false,
                    depth: 1,
                    edges: Vec::new(),
                },
            ],
        };
        let mut evaluator = evaluator(&universe, &index);
        let coverage = evaluator
            .evaluate(&language, &ExecutionControl::default())
            .expect("zero transition coverage");

        assert_eq!(coverage.covered_pattern_count, 0);
        assert_eq!(coverage.covered_weight, 0.0);
        assert!(evaluator.zero_scores.len() >= 2);
    }

    #[test]
    fn zero_valued_accept_is_not_collapsed_into_rejection() {
        use PieceKind::{I, O, S, T, Z};

        let (universe, index) = two_hidden_suffix_universe();
        let target = compile_setup_search_conditions(
            &SetupSearchQuery::default()
                .with_remaining_pieces(vec![I, O, T, S])
                .with_next_cycle_remaining_pieces(vec![Z]),
        )
        .expect("terminal condition")
        .remove(0)
        .terminal_supply_target()
        .expect("terminal target");
        let language = TestLanguage::from_sequences(&[Vec::new()]);
        let mut evaluator = evaluator(&universe, &index);
        evaluator.set_terminal_supply_target(Some(target));
        let coverage = evaluator
            .evaluate(&language, &ExecutionControl::default())
            .expect("zero accept coverage");

        assert_eq!(coverage.covered_pattern_count, 0);
        assert_eq!(coverage.covered_weight, 0.0);
        assert!(!evaluator.zero_scores.is_empty());
    }

    #[test]
    fn memo_quotient_matches_raw_after_bag_boundary_and_terminal_hold_release() {
        use PieceKind::{I, J, L, O, S, T, Z};

        let queue = vec![I, O, T, S, Z, J, L, I, O];
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(93),
            PatternWeightModelId::new(94),
            vec![queue.clone()],
            vec![ProbabilityValue::new(1.0).expect("weight")],
            1,
            true,
            None,
        )
        .expect("terminal hold universe");
        let index = PatternPiecePositionIndex::compile(&universe).expect("pattern index");
        let target = compile_setup_search_conditions(
            &SetupSearchQuery::default()
                .with_remaining_pieces(vec![I, O, T, S])
                .with_next_cycle_remaining_pieces(vec![Z]),
        )
        .expect("terminal condition")
        .remove(0)
        .terminal_supply_target()
        .expect("terminal target");
        let mut accepted = queue;
        accepted.push(T);
        let language = TestLanguage::from_sequences(&[accepted]);
        let mut optimized = QueueObservationPolicyEvaluator::new(
            &universe,
            &index,
            QueueObservationPolicy::VisibleSeven,
            0,
            Some(T),
            true,
            true,
            false,
            Some(target),
        )
        .expect("optimized evaluator");
        let mut raw = QueueObservationPolicyEvaluator::new(
            &universe,
            &index,
            QueueObservationPolicy::VisibleSeven,
            0,
            Some(T),
            true,
            true,
            false,
            Some(target),
        )
        .expect("raw evaluator");
        raw.raw_memo_keys = true;
        let control = ExecutionControl::default();
        let quotient = optimized
            .evaluate(&language, &control)
            .expect("quotient coverage");
        let identity = raw.evaluate(&language, &control).expect("raw coverage");

        assert_eq!(
            quotient.covered_patterns.covered_patterns(),
            identity.covered_patterns.covered_patterns()
        );
        assert_eq!(
            quotient.covered_pattern_count,
            identity.covered_pattern_count
        );
        assert_eq!(quotient.covered_weight, identity.covered_weight);
        assert_eq!(quotient.min_accepted_depth, identity.min_accepted_depth);
        assert_eq!(quotient.max_accepted_depth, identity.max_accepted_depth);
        assert!(optimized
            .memo
            .keys()
            .chain(optimized.zero_scores.iter())
            .any(|state| {
                let hold_code = ((state.0 >> 122) & 0xf) as u8;
                let consumed_bag_counts = ((state.0 >> 85) & ((1_u128 << 21) - 1)) as u32;
                hold_code == super::TERMINAL_USED_HELD_CODE && consumed_bag_counts != 0
            }));
    }
}
