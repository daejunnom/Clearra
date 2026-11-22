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
    execution_automaton::{
        SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState,
        SupplyObservationIdentity, SupplyProvenanceId,
    },
    hold::hold_policy::HoldPolicy,
    pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
    piece_source::{PieceSourceId, PieceSourceKind},
    QueueObservationPolicy,
};

use super::{
    exact_collections::{ExactHashMap, ExactHashSet},
    piece_order_language::PieceOrderLanguageCache,
    WasmExactSearchError,
};

#[path = "queue_observation_kind_union.rs"]
pub(super) mod kind_union;
#[cfg(feature = "placement-dependency-pc")]
use kind_union::KnownSupplyQuery;
#[path = "queue_observation_physical_actions.rs"]
mod physical_actions;
#[cfg(all(feature = "placement-dependency-pc", feature = "parallel"))]
pub(super) use physical_actions::board_actions::shared_producer::{
    SharedObservationOutput, SharedObservationProducer,
};
#[cfg(feature = "placement-dependency-pc")]
pub(super) use physical_actions::board_actions::BoardObservationPlan;
#[cfg(test)]
pub(super) use physical_actions::PhysicalActionEdge;
pub(super) use physical_actions::PhysicalActionLanguage;

#[cfg(test)]
#[path = "queue_observation_policy_tests.rs"]
mod physical_tie_tests;

#[cfg(test)]
#[path = "queue_observation_dependency_tests.rs"]
mod dependency_tests;

#[cfg(all(test, feature = "placement-dependency-pc"))]
#[path = "placement_dependency_observation_tests.rs"]
mod product_dependency_tests;

const NO_NODE: u32 = u32::MAX;
const NO_TERMINAL: u32 = u32::MAX;
const CANCELLATION_POLL_MASK: u32 = 0xff;
const MAX_REVEAL_BRANCHES: usize = 49;
const TERMINAL_USED_HELD_CODE: u8 = 8;
const MAX_PRE_REVEAL_VALUES: usize = 65_536;

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

    /// True for physical board nodes, including those with no outgoing edges.
    /// Revealed kind-only aliases and frontier unions keep the default.
    fn has_physical_actions(&self, _node: u32) -> bool {
        false
    }

    /// The four current-frame cells locked by this physical edge. Every edge
    /// of a physical node must have a label, and each same-kind subsequence
    /// must be ordered by increasing cells, independently of child node IDs.
    /// Distinct cell labels remain distinct actions even if children coincide.
    fn physical_action_cells(&self, _node: u32, _index: usize) -> Option<u64> {
        None
    }

    /// Materialize only actions that the current supply state can execute.
    /// Existing edge indices must remain stable when another piece is prepared.
    fn prepare_edges(
        &self,
        _node: u32,
        _piece_mask: u8,
        _control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        Ok(())
    }

    /// A consumer may call this only after the original observation domain
    /// proves that every source value relevant to completion is determined. The returned
    /// node preserves existential execution for that known source and hold.
    fn fully_revealed_node(
        &self,
        node: u32,
        _control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        Ok(node)
    }

    /// Ask for supply counts only at a still-unfiltered frontier. Most kind
    /// states need no count extraction before their policy memo lookup.
    fn wants_revealed_supply(&self, _node: u32) -> bool {
        false
    }

    /// The evaluator has actually observed every draw relevant to completion.
    /// Counts include those draws and at most one usable held piece, so they
    /// bound every possible remaining lock multiset. A determined but hidden
    /// future alone never authorizes this hook.
    fn fully_revealed_node_with_supply(
        &self,
        node: u32,
        _available: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        self.fully_revealed_node(node, control)
    }

    /// An optional exact membership kernel for one already-observed suffix.
    /// Eligibility alone never grants permission to read future source values.
    #[cfg(feature = "placement-dependency-pc")]
    fn supports_revealed_word_membership(&self, _node: u32) -> bool {
        false
    }

    /// Some(false) is an exact negative certificate, while None requests the
    /// existing policy recurrence. The evaluator supplies only actual observed
    /// draws, without projected EOF/held-release or terminal-target semantics.
    #[cfg(feature = "placement-dependency-pc")]
    fn revealed_word_membership(
        &self,
        _node: u32,
        _query: &KnownSupplyQuery,
        _control: &ExecutionControl,
    ) -> Result<Option<bool>, WasmExactSearchError> {
        Ok(None)
    }

    /// Every accepting path has this depth. Variable-horizon consumers keep
    /// the default and do not acquire the complete-coverage early exit.
    fn fixed_horizon(&self) -> Option<u8> {
        None
    }

    /// Exact existential roots of an already-authorized revealed union.
    /// Direct roots have no further alternatives. Their node IDs must use a
    /// disjoint namespace and the same immutable kind arena and horizon.
    fn revealed_alternatives(
        &self,
        _node: u32,
        _control: &ExecutionControl,
    ) -> Result<Option<Vec<u32>>, WasmExactSearchError> {
        Ok(None)
    }

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
                kind_union::append_kind_children(
                    self.source,
                    source_node,
                    piece_index as u8 + 1,
                    child_set,
                )?;
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

#[derive(Clone, Copy, Debug)]
pub(super) struct QueueObservationMetrics {
    pub policy_states: usize,
    pub action_checks: usize,
    pub observation_nodes: usize,
    pub retained_bytes: usize,
    pub physical_actions: [usize; 13],
    pub board_actions: [usize; 14],
    pub kind_union: [usize; 5],
    pub supply_multiset: [usize; 5],
    pub multiset_groups: [usize; 9],
    pub branch_bound: [usize; 4],
    pub prefix_cones: [usize; 5],
    pub prefix_cone_exact: [usize; 9],
    pub boundary_classes: [usize; 7],
    pub word_bitmap: [usize; 9],
    pub word_bitmap_board: [usize; 6],
    pub shared_producer: [usize; 8],
    pub ordered_classes: [usize; 8],
    pub prefix_masks: [usize; 6],
    pub suffix_import: [usize; 13],
    pub source_existence: [usize; 6],
    pub source_filter: [usize; 4],
    pub pre_reveal: [usize; 4],
    pub pre_reveal_retained_bytes: usize,
    pub pre_reveal_peak_bytes: usize,
    pub collection: [usize; 2],
    pub physical_representation: &'static str,
    // Detailed memo diagnostics are consumed only by the native opt-in
    // `CLEARRA_SETUP_POLICY_DIAGNOSTICS` reporter.
    #[cfg(not(target_arch = "wasm32"))]
    pub local_values: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pub local_zero_scores: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pub shared_values: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pub shared_zero_scores: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pub local_value_capacity: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pub local_zero_score_capacity: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pub shared_value_capacity: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pub shared_zero_score_capacity: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pub shared_hits: usize,
}

impl Default for QueueObservationMetrics {
    fn default() -> Self {
        Self {
            policy_states: 0,
            action_checks: 0,
            observation_nodes: 0,
            retained_bytes: 0,
            physical_actions: [0; 13],
            board_actions: [0; 14],
            kind_union: [0; 5],
            supply_multiset: [0; 5],
            multiset_groups: [0; 9],
            branch_bound: [0; 4],
            prefix_cones: [0; 5],
            prefix_cone_exact: [0; 9],
            boundary_classes: [0; 7],
            word_bitmap: [0; 9],
            word_bitmap_board: [0; 6],
            shared_producer: [0; 8],
            ordered_classes: [0; 8],
            prefix_masks: [0; 6],
            suffix_import: [0; 13],
            source_existence: [0; 6],
            source_filter: [0; 4],
            pre_reveal: [0; 4],
            pre_reveal_retained_bytes: 0,
            pre_reveal_peak_bytes: 0,
            collection: [0; 2],
            physical_representation: "none",
            #[cfg(not(target_arch = "wasm32"))]
            local_values: 0,
            #[cfg(not(target_arch = "wasm32"))]
            local_zero_scores: 0,
            #[cfg(not(target_arch = "wasm32"))]
            shared_values: 0,
            #[cfg(not(target_arch = "wasm32"))]
            shared_zero_scores: 0,
            #[cfg(not(target_arch = "wasm32"))]
            local_value_capacity: 0,
            #[cfg(not(target_arch = "wasm32"))]
            local_zero_score_capacity: 0,
            #[cfg(not(target_arch = "wasm32"))]
            shared_value_capacity: 0,
            #[cfg(not(target_arch = "wasm32"))]
            shared_zero_score_capacity: 0,
            #[cfg(not(target_arch = "wasm32"))]
            shared_hits: 0,
        }
    }
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
    deterministic_future_classes: Vec<bool>,
    // Positive, bitwise-identical input weights admit exact count scores.
    // This includes standard bags, whose actual weights are 1/N, not 1.
    uniform_weight: Option<f64>,
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

#[derive(Clone, Copy)]
enum ObservationPatternDomain<'a> {
    // Whole-universe observation needs the ordinal identity mapping, not
    // position-by-piece bitplanes or an allocated Vec containing 0..N.
    OriginalUniverse,
    // Subset consumers retain their established dense-to-global mapping.
    Indexed(&'a PatternPiecePositionIndex),
}

impl ObservationTrie {
    #[allow(clippy::too_many_arguments)]
    fn compile(
        universe: &MaterializedPatternUniverse,
        domain: ObservationPatternDomain<'_>,
        initial_cursor: usize,
        visible_piece_count: usize,
        projects_standard_bag_lookahead: bool,
    ) -> Result<Self, WasmExactSearchError> {
        let (global_pattern_count, local_pattern_count, materialized_sequence_len) = match domain {
            ObservationPatternDomain::OriginalUniverse => {
                let count = universe.pattern_count();
                if count != 0 && u32::try_from(count - 1).is_err() {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_pattern_index_overflow",
                    ));
                }
                (count, count, 0)
            }
            ObservationPatternDomain::Indexed(index) => (
                index.global_pattern_count(),
                index.local_pattern_count(),
                index.sequence_len(),
            ),
        };
        let mut trie = Self {
            nodes: vec![ObservationTrieNode::root()],
            terminals: Vec::new(),
            initial_observations: Vec::new(),
            future_classes: Vec::new(),
            deterministic_future_classes: Vec::new(),
            uniform_weight: None,
            sequence_len: 0,
            materialized_sequence_len,
            global_pattern_count,
        };
        let mut sequence = Vec::new();
        let mut expected_len = None;
        let mut first_weight = None;
        let mut uniform_positive = true;
        for local_pattern_index in 0..local_pattern_count {
            let global_pattern_index = match domain {
                ObservationPatternDomain::OriginalUniverse => local_pattern_index,
                ObservationPatternDomain::Indexed(index) => index
                    .global_pattern_index(local_pattern_index)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_pattern_index_missing",
                    ))?,
            };
            universe.write_sequence_at(global_pattern_index, &mut sequence);
            if matches!(domain, ObservationPatternDomain::OriginalUniverse) {
                // Match the index's maximum ORIGINAL length, before an
                // optional projected final-bag piece changes the sequence.
                // The existing post-projection uniform-length check below
                // still decides admission; no new uniform-source assumption.
                trie.materialized_sequence_len = trie.materialized_sequence_len.max(sequence.len());
            }
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
            let common = *first_weight.get_or_insert(weight);
            uniform_positive &= weight > 0.0 && weight.to_bits() == common.to_bits();
            trie.insert(
                &sequence,
                u32::try_from(global_pattern_index).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_observation_pattern_index_overflow")
                })?,
                weight,
            )?;
        }
        trie.sequence_len = expected_len.unwrap_or(0);
        trie.uniform_weight = first_weight.filter(|_| uniform_positive);
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
        let mut deterministic_classes = Vec::new();
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
                subtree_weight_bits: if self.uniform_weight.is_some() {
                    f64::from(node.subtree_count).to_bits()
                } else {
                    node.subtree_weight.to_bits()
                },
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
                let mut branches = 0_usize;
                let mut determined = true;
                for child in children.iter().copied().filter(|child| *child != NO_NODE) {
                    branches += 1;
                    determined &= *deterministic_classes.get(child as usize).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_observation_future_class_not_topological",
                        ),
                    )?;
                }
                deterministic_classes.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_observation_future_class_storage_unavailable",
                    )
                })?;
                deterministic_classes.push(determined && branches <= 1);
                interned.insert(signature, class);
                class
            };
            classes[node_index] = class;
        }
        self.future_classes = classes;
        self.deterministic_future_classes = deterministic_classes;
        Ok(())
    }

    /// This certificate comes from the original input information set, never
    /// from the queues that happen to succeed for a candidate or action.
    fn future_is_determined(&self, node: u32) -> Result<bool, WasmExactSearchError> {
        let class =
            *self
                .future_classes
                .get(node as usize)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_future_class_missing",
                ))?;
        self.deterministic_future_classes
            .get(class as usize)
            .copied()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_observation_future_class_missing",
            ))
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
            + self.deterministic_future_classes.capacity() * core::mem::size_of::<bool>()
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PhysicalPolicyAction {
    kind: u8,
    cells: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PolicyTransition {
    child: u32,
    source_cursor: u16,
    hold_code: u8,
    action: SupplyAction,
    physical_action: Option<PhysicalPolicyAction>,
}

impl PolicyTransition {
    fn precedes(self, other: Self) -> bool {
        match (self.physical_action, other.physical_action) {
            (Some(left), Some(right)) => {
                (self.action, left.kind, left.cells) < (other.action, right.kind, right.cells)
            }
            (None, None) => {
                (self.child, self.source_cursor, self.hold_code, self.action)
                    < (
                        other.child,
                        other.source_cursor,
                        other.hold_code,
                        other.action,
                    )
            }
            _ => unreachable!("one policy state cannot mix physical and kind-only action ties"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PolicyValue {
    // Uniform positive inputs use exact integer counts here. Other inputs
    // retain the established weighted recurrence and summation order.
    score: f64,
    pattern_count: u32,
}

impl PolicyValue {
    const REJECT: Self = Self {
        score: 0.0,
        pattern_count: 0,
    };
}

/// A transition-local proof that an action cannot replace the exact incumbent.
/// This is deliberately distinct from a zero policy value: an unfinished
/// action is never entered into any policy, zero-score, or pre-reveal memo.
enum TransitionValue {
    Exact(PolicyValue),
    Dominated,
}

#[derive(Clone, Copy)]
struct TransitionBound {
    incumbent_count: u32,
    wins_equal_count: bool,
}

impl TransitionBound {
    fn dominates(self, upper_count: u32) -> bool {
        upper_count < self.incumbent_count
            || (upper_count == self.incumbent_count && !self.wins_equal_count)
    }
}

/// An optional DP value cache before revealed-language normalization. Keys
/// retain the exact original supply memo state and one raw language class.
/// Only completed Ok values, including exact rejections, enter this cache;
/// the canonical memo remains populated for sharing and reconstruction.
/// Probes use the existing DP memo accounting and are reported separately.
/// Skipped language normalization really does not run; no language work
/// counter is reset or adjusted after a query has already been performed.
#[derive(Default)]
struct PreRevealMemo {
    values: ExactHashMap<PolicyMemoState, PolicyValue>,
    // probes, hits, successful insertions, full-cache admission fallbacks
    metrics: [usize; 4],
    peak_bytes: usize,
}

impl PreRevealMemo {
    fn clear_values(&mut self) {
        self.values.clear();
    }

    fn begin_evaluation(&mut self) {
        self.clear_values();
        self.metrics = [0; 4];
        self.peak_bytes = self.retained_bytes();
    }

    fn get(&mut self, key: &PolicyMemoState) -> Option<PolicyValue> {
        self.metrics[0] = self.metrics[0].saturating_add(1);
        let value = self.values.get(key).copied();
        if value.is_some() {
            self.metrics[1] = self.metrics[1].saturating_add(1);
        }
        value
    }

    fn insert(
        &mut self,
        key: PolicyMemoState,
        value: PolicyValue,
    ) -> Result<(), WasmExactSearchError> {
        if self.values.len() >= MAX_PRE_REVEAL_VALUES {
            self.metrics[3] = self.metrics[3].saturating_add(1);
            return Ok(());
        }
        let old_capacity = self.values.capacity();
        let old_bytes = self.retained_bytes();
        self.values.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_observation_pre_reveal_storage_unavailable")
        })?;
        self.peak_bytes = self.peak_bytes.max(self.retained_bytes().saturating_add(
            if self.values.capacity() != old_capacity {
                old_bytes
            } else {
                0
            },
        ));
        self.values.insert(key, value);
        self.metrics[2] = self.metrics[2].saturating_add(1);
        Ok(())
    }

    fn retained_bytes(&self) -> usize {
        let capacity = self.values.capacity();
        if capacity == 0 {
            return 0;
        }
        // Match the conservative bucket/control-byte convention of other
        // compact memo owners. This is an owned allocation estimate, not RSS.
        let buckets = capacity
            .checked_add(1)
            .and_then(usize::checked_next_power_of_two)
            .unwrap_or(usize::MAX);
        buckets
            .saturating_mul(core::mem::size_of::<(PolicyMemoState, PolicyValue)>() + 1)
            .saturating_add(core::mem::align_of::<(PolicyMemoState, PolicyValue)>() + 16)
    }
}

pub(super) struct QueueObservationPolicyEvaluator {
    trie: Arc<ObservationTrie>,
    policy: QueueObservationPolicy,
    initial_cursor: usize,
    initial_hold_code: u8,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    supply_identity: SupplyExecutionState,
    terminal_supply_target: Option<SetupTerminalSupplyTarget>,
    memo: ExactHashMap<PolicyMemoState, PolicyValue>,
    zero_scores: ExactHashSet<PolicyMemoState>,
    shared_memo: ExactHashMap<PolicyMemoState, PolicyValue>,
    shared_zero_scores: ExactHashSet<PolicyMemoState>,
    shared_terminal_supply_target: Option<SetupTerminalSupplyTarget>,
    shared_memo_domain: Option<u64>,
    next_shared_memo_domain: u64,
    pre_reveal_enabled: bool,
    pre_reveal_memo: PreRevealMemo,
    selected_patterns: Vec<u32>,
    selected_min_depth: u8,
    selected_max_depth: u8,
    cancellation_poll_counter: u32,
    action_checks: usize,
    shared_hits: usize,
    collect_full: bool,
    collection: [usize; 2],
    branch_bound_enabled: bool,
    // Eligible action attempts, dominated actions, skipped reveal descendants,
    // and exact integer upper-bound comparisons. These are DP diagnostics;
    // no language work counter is discounted after an operation has run.
    branch_bound: [usize; 4],
    initial_observation_progress: [usize; 2],
    peer_abort: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    raw_memo_keys: bool,
}

impl QueueObservationPolicyEvaluator {
    /// Conservative pre-allocation credit for a non-reusable policy solve.
    /// The trie has at most one node per input prefix; a policy state is fixed
    /// by one language node, trie node, cursor and one of the nine hold codes
    /// (empty, seven pieces, terminal-held release). Hash tables compare full
    /// keys and can only quotient that Cartesian product, never enlarge it.
    /// Factors cover bucket growth with old/new tables coexisting, vector
    /// capacity growth, future-class interning and DFS/reveal scratch. This
    /// credit authorizes allocation, not a claim of observed peak usage.
    pub fn checked_evaluation_peak_upper_bound(
        pattern_count: usize,
        sequence_len: usize,
        language_node_count: u128,
    ) -> Option<u128> {
        let patterns = pattern_count as u128;
        let cursors = (sequence_len as u128).checked_add(1)?;
        let trie_nodes = patterns.checked_mul(sequence_len as u128)?.checked_add(1)?;
        let states = language_node_count
            .checked_mul(trie_nodes)?
            .checked_mul(cursors)?
            .checked_mul(9)?;
        let memo_entry = (core::mem::size_of::<PolicyMemoState>()
            + core::mem::size_of::<PolicyValue>()
            + 32) as u128;
        let memo_bytes = states.checked_mul(memo_entry)?.checked_mul(16)?;
        let trie_entry = (core::mem::size_of::<ObservationTrieNode>()
            + core::mem::size_of::<ObservationFutureSignature>()
            + core::mem::size_of::<u32>() * 4
            + 32) as u128;
        let trie_bytes = trie_nodes.checked_mul(trie_entry)?.checked_mul(8)?;
        let terminal_bytes = patterns
            .checked_mul((core::mem::size_of::<TerminalPattern>() + 32) as u128)?
            .checked_mul(8)?;
        let recursion_bytes = cursors
            .checked_mul((MAX_REVEAL_BRANCHES * core::mem::size_of::<u32>() + 256) as u128)?
            .checked_mul(4)?;
        memo_bytes
            .checked_add(trie_bytes)?
            .checked_add(terminal_bytes)?
            .checked_add(recursion_bytes)?
            .checked_add(32 * 1024)
    }

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
        Self::from_domain(
            universe,
            ObservationPatternDomain::Indexed(pattern_index),
            policy,
            initial_cursor,
            initial_hold,
            hold_enabled,
            projects_unplaced_lookahead,
            projects_standard_bag_lookahead,
            terminal_supply_target,
        )
    }

    /// Observe the complete original universe in canonical ID order. This
    /// shares the indexed constructor's trie, weights, and evaluator kernel
    /// without compiling unused bitplanes or an identity-ID vector first.
    #[allow(clippy::too_many_arguments)]
    pub fn for_original_universe(
        universe: &MaterializedPatternUniverse,
        policy: QueueObservationPolicy,
        initial_cursor: u16,
        initial_hold: Option<clearra_core_domain::piece::piece_kind::PieceKind>,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
        projects_standard_bag_lookahead: bool,
        terminal_supply_target: Option<SetupTerminalSupplyTarget>,
    ) -> Result<Self, WasmExactSearchError> {
        Self::from_domain(
            universe,
            ObservationPatternDomain::OriginalUniverse,
            policy,
            initial_cursor,
            initial_hold,
            hold_enabled,
            projects_unplaced_lookahead,
            projects_standard_bag_lookahead,
            terminal_supply_target,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_domain(
        universe: &MaterializedPatternUniverse,
        domain: ObservationPatternDomain<'_>,
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
            domain,
            initial_cursor,
            visible_piece_count,
            projects_standard_bag_lookahead,
        )?;
        let supply_identity = universe.pattern_universe_id().get();
        Ok(Self {
            trie: Arc::new(trie),
            policy,
            initial_cursor,
            initial_hold_code: initial_hold.map_or(0, piece_code),
            hold_enabled,
            projects_unplaced_lookahead,
            projects_standard_bag_lookahead,
            supply_identity: SupplyExecutionState::with_contract(
                PieceSourceId::new(supply_identity),
                PieceSourceKind::ObservedWindow,
                u16::try_from(initial_cursor).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_observation_initial_cursor_overflow")
                })?,
                initial_hold,
                if hold_enabled {
                    HoldPolicy::Allowed
                } else {
                    HoldPolicy::Forbidden
                },
                0,
                0,
                SupplyObservationIdentity::new(policy, supply_identity),
                SupplyProvenanceId(supply_identity),
            ),
            terminal_supply_target,
            memo: ExactHashMap::default(),
            zero_scores: ExactHashSet::default(),
            shared_memo: ExactHashMap::default(),
            shared_zero_scores: ExactHashSet::default(),
            shared_terminal_supply_target: terminal_supply_target,
            shared_memo_domain: None,
            next_shared_memo_domain: 0,
            pre_reveal_enabled: pre_reveal_enabled()?,
            pre_reveal_memo: PreRevealMemo::default(),
            selected_patterns: Vec::new(),
            selected_min_depth: u8::MAX,
            selected_max_depth: 0,
            cancellation_poll_counter: 0,
            action_checks: 0,
            shared_hits: 0,
            collect_full: collect_full_enabled()?,
            collection: [0; 2],
            branch_bound_enabled: branch_bound_enabled()?,
            branch_bound: [0; 4],
            initial_observation_progress: [0; 2],
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
        self.pre_reveal_memo.begin_evaluation();
        self.selected_patterns.clear();
        self.selected_min_depth = u8::MAX;
        self.selected_max_depth = 0;
        self.cancellation_poll_counter = 0;
        self.action_checks = 0;
        self.shared_hits = 0;
        self.collection = [0; 2];
        self.branch_bound = [0; 4];
        let root = language.root();
        let initial_observations = self.trie.initial_observations.clone();
        self.initial_observation_progress = [0, initial_observations.len()];
        let mut total_score = 0.0;
        let mut total_count = 0usize;
        #[cfg(feature = "local-search-ab")]
        let mut initial_observations_done = 0usize;
        for observation_node in initial_observations.iter().copied() {
            self.pre_reveal_memo.clear_values();
            let state = PolicyState {
                language_node: root,
                observation_node,
                source_cursor: u16::try_from(self.initial_cursor).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_observation_initial_cursor_overflow")
                })?,
                hold_code: self.initial_hold_code,
            };
            let value = self.solve(language, state, control)?;
            total_score += value.score;
            total_count = total_count.saturating_add(value.pattern_count as usize);
            self.initial_observation_progress[0] += 1;
            #[cfg(feature = "local-search-ab")]
            {
                initial_observations_done += 1;
                if initial_observations_done == 1
                    || initial_observations_done & 0xff == 0
                    || initial_observations_done == initial_observations.len()
                {
                    eprintln!(
                        "clearra_observation_policy_progress initial_observations_done={initial_observations_done} initial_observations_total={} policy_states={} action_checks={}",
                        initial_observations.len(),
                        self.memo.len()
                            .saturating_add(self.zero_scores.len())
                            .saturating_add(self.shared_memo.len())
                            .saturating_add(self.shared_zero_scores.len()),
                        self.action_checks,
                    );
                }
            }
        }
        for observation_node in initial_observations {
            self.pre_reveal_memo.clear_values();
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
            covered_weight: self
                .trie
                .uniform_weight
                .map_or(total_score, |weight| total_count as f64 * weight),
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
                physical_actions: [0; 13],
                board_actions: [0; 14],
                kind_union: [0; 5],
                supply_multiset: [0; 5],
                multiset_groups: [0; 9],
                branch_bound: self.branch_bound_metrics(),
                prefix_cones: [0; 5],
                prefix_cone_exact: [0; 9],
                boundary_classes: [0; 7],
                word_bitmap: [0; 9],
                word_bitmap_board: [0; 6],
                shared_producer: [0; 8],
                ordered_classes: [0; 8],
                prefix_masks: [0; 6],
                suffix_import: [0; 13],
                source_existence: [0; 6],
                source_filter: [0; 4],
                pre_reveal: self.pre_reveal_metrics(),
                pre_reveal_retained_bytes: self.pre_reveal_retained_bytes(),
                pre_reveal_peak_bytes: self.pre_reveal_peak_bytes(),
                collection: self.collection,
                physical_representation: "none",
                #[cfg(not(target_arch = "wasm32"))]
                local_values: self.memo.len(),
                #[cfg(not(target_arch = "wasm32"))]
                local_zero_scores: self.zero_scores.len(),
                #[cfg(not(target_arch = "wasm32"))]
                shared_values: self.shared_memo.len(),
                #[cfg(not(target_arch = "wasm32"))]
                shared_zero_scores: self.shared_zero_scores.len(),
                #[cfg(not(target_arch = "wasm32"))]
                local_value_capacity: self.memo.capacity(),
                #[cfg(not(target_arch = "wasm32"))]
                local_zero_score_capacity: self.zero_scores.capacity(),
                #[cfg(not(target_arch = "wasm32"))]
                shared_value_capacity: self.shared_memo.capacity(),
                #[cfg(not(target_arch = "wasm32"))]
                shared_zero_score_capacity: self.shared_zero_scores.capacity(),
                #[cfg(not(target_arch = "wasm32"))]
                shared_hits: self.shared_hits,
            },
        })
    }

    pub fn set_terminal_supply_target(&mut self, target: Option<SetupTerminalSupplyTarget>) {
        if self.terminal_supply_target != target {
            self.pre_reveal_memo.clear_values();
        }
        if self.terminal_supply_target != target && self.shared_memo_domain.is_some() {
            self.shared_memo.clear();
            self.shared_zero_scores.clear();
            self.shared_memo_domain = None;
            self.shared_terminal_supply_target = target;
        }
        self.terminal_supply_target = target;
    }

    // Only native setup workers fork the evaluator for parallel target scans.
    #[cfg(not(target_family = "wasm"))]
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
            projects_standard_bag_lookahead: self.projects_standard_bag_lookahead,
            supply_identity: self.supply_identity,
            terminal_supply_target: self.terminal_supply_target,
            memo: ExactHashMap::default(),
            zero_scores: ExactHashSet::default(),
            shared_memo: ExactHashMap::default(),
            shared_zero_scores: ExactHashSet::default(),
            shared_terminal_supply_target: self.terminal_supply_target,
            shared_memo_domain: None,
            next_shared_memo_domain: 0,
            pre_reveal_enabled: self.pre_reveal_enabled,
            pre_reveal_memo: PreRevealMemo::default(),
            selected_patterns: Vec::new(),
            selected_min_depth: u8::MAX,
            selected_max_depth: 0,
            cancellation_poll_counter: 0,
            action_checks: 0,
            shared_hits: 0,
            collect_full: self.collect_full,
            collection: [0; 2],
            branch_bound_enabled: self.branch_bound_enabled,
            branch_bound: [0; 4],
            initial_observation_progress: [0; 2],
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
        self.pre_reveal_memo.clear_values();
        self.shared_memo.clear();
        self.shared_zero_scores.clear();
        let domain = self.next_shared_memo_domain;
        self.next_shared_memo_domain = self.next_shared_memo_domain.wrapping_add(1);
        self.shared_memo_domain = Some(domain);
        self.shared_terminal_supply_target = self.terminal_supply_target;
        domain
    }

    // Browser workers are isolated and coordinate cancellation outside this
    // evaluator; the shared abort flag belongs to native worker threads.
    #[cfg(not(target_family = "wasm"))]
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
            + self.pre_reveal_memo.retained_bytes()
            + self.selected_patterns.capacity() * core::mem::size_of::<u32>()
    }

    pub(super) fn pre_reveal_metrics(&self) -> [usize; 4] {
        self.pre_reveal_memo.metrics
    }

    pub(super) fn pre_reveal_retained_bytes(&self) -> usize {
        self.pre_reveal_memo.retained_bytes()
    }

    pub(super) fn pre_reveal_peak_bytes(&self) -> usize {
        self.pre_reveal_memo.peak_bytes
    }

    pub(super) fn branch_bound_metrics(&self) -> [usize; 4] {
        self.branch_bound
    }

    #[cfg(feature = "local-search-ab")]
    pub(super) fn initial_observation_progress(&self) -> [usize; 2] {
        self.initial_observation_progress
    }

    #[cfg(test)]
    fn set_branch_bound_enabled(&mut self, enabled: bool) {
        self.branch_bound_enabled = enabled;
    }

    #[cfg(all(test, feature = "placement-dependency-pc"))]
    pub(in crate::backend::wasm_cpu) fn set_collect_full_for_test(&mut self, enabled: bool) {
        self.collect_full = enabled;
    }

    fn solve<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        control: &ExecutionControl,
    ) -> Result<PolicyValue, WasmExactSearchError> {
        self.poll_cancellation(control)?;
        #[cfg(feature = "placement-dependency-pc")]
        if let Some(value) = self.revealed_word_value(language, state, control)? {
            return Ok(value);
        }
        let raw_key = if self.pre_reveal_enabled
            && language.wants_revealed_supply(state.language_node)
            && !language.memo_reusable(state.language_node)
        {
            let key = self.memo_state(language, state)?;
            if let Some(value) = self.pre_reveal_memo.get(&key) {
                // Normalization normally checks this on every reveal. A DP
                // hit must not weaken that cancellation boundary.
                if control.is_cancelled() {
                    return Err(WasmExactSearchError::Cancelled);
                }
                return Ok(value);
            }
            Some(key)
        } else {
            None
        };
        let value = self.solve_normalized(language, state, control)?;
        if let Some(key) = raw_key {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            self.pre_reveal_memo.insert(key, value)?;
        }
        Ok(value)
    }

    /// The one existing recurrence, including canonical memo lookup. A raw
    /// cache miss always runs this path; collect_selected continues to use
    /// its original normalization and replay, never the raw value as a path.
    fn solve_normalized<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        control: &ExecutionControl,
    ) -> Result<PolicyValue, WasmExactSearchError> {
        let state = self.observation_language_state(language, state, control)?;
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
                score: if self.trie.uniform_weight.is_some() {
                    f64::from(pattern_count)
                } else {
                    weight
                },
                pattern_count,
            };
            self.insert_memo(memo_state, value, reusable)?;
            return Ok(value);
        }
        let best = match self.best_revealed_alternative(language, state, control)? {
            Some((value, _)) => value,
            None => self.best_transition(language, state, control, true)?.0,
        };
        self.insert_memo(memo_state, best, reusable)?;
        Ok(best)
    }

    fn best_revealed_alternative<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        control: &ExecutionControl,
    ) -> Result<Option<(PolicyValue, Option<u32>)>, WasmExactSearchError> {
        if self.trie.uniform_weight.is_none()
            || language.fixed_horizon().is_none()
            || self.terminal_supply_target.is_some()
        {
            return Ok(None);
        }
        let Some(alternatives) = language.revealed_alternatives(state.language_node, control)?
        else {
            return Ok(None);
        };
        let observed = self
            .trie
            .nodes
            .get(state.observation_node as usize)
            .copied()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_observation_node_out_of_range",
            ))?;
        for alternative in alternatives {
            if alternative == state.language_node {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_alternative_cycle",
                ));
            }
            let value = self.solve(
                language,
                PolicyState {
                    language_node: alternative,
                    ..state
                },
                control,
            )?;
            if value.pattern_count == 0 {
                continue;
            }
            // All relevant draws are fixed throughout this original
            // information set. A single root therefore accepts all IDs or
            // none. A partial answer signals a broken certificate, not a
            // sound reason to reject or prune any queue.
            if value.pattern_count != observed.subtree_count {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_revealed_membership_not_uniform",
                ));
            }
            return Ok(Some((
                PolicyValue {
                    score: f64::from(observed.subtree_count),
                    pattern_count: observed.subtree_count,
                },
                Some(alternative),
            )));
        }
        Ok(Some((PolicyValue::REJECT, None)))
    }

    fn observed_completion_end<G: ObservationPieceLanguage>(
        &self,
        language: &G,
        state: PolicyState,
    ) -> Option<usize> {
        // A fixed-depth Boolean suffix can read at most one source draw per
        // lock, plus one when an initially empty hold is first filled. Keep
        // the original observation/EOF state; only erase physical choices
        // when all source values that can influence them are already known.
        self.terminal_supply_target
            .is_none()
            .then(|| language.fixed_horizon())
            .flatten()
            .and_then(|horizon| {
                let node = language.node(state.language_node)?;
                let remaining = horizon.checked_sub(node.depth)?;
                let extra =
                    usize::from(self.hold_enabled && state.hold_code == 0 && remaining != 0);
                let required_end = usize::from(state.source_cursor)
                    .checked_add(usize::from(remaining))?
                    .checked_add(extra)?;
                // Current/Store may consume the projected final draw. The
                // materialized length is only the terminal held-release gate.
                let end = required_end.min(self.trie.sequence_len);
                self.trie
                    .nodes
                    .get(state.observation_node as usize)
                    .filter(|observed| usize::from(observed.depth) >= end)
                    .map(|_| end)
            })
    }

    #[cfg(feature = "placement-dependency-pc")]
    fn revealed_word_value<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        control: &ExecutionControl,
    ) -> Result<Option<PolicyValue>, WasmExactSearchError> {
        if self.trie.uniform_weight.is_none()
            || self.terminal_supply_target.is_some()
            || self.projects_unplaced_lookahead
            || self.projects_standard_bag_lookahead
            || !language.supports_revealed_word_membership(state.language_node)
        {
            return Ok(None);
        }
        let Some(end) = self.observed_completion_end(language, state) else {
            return Ok(None);
        };
        let cursor = usize::from(state.source_cursor);
        let Some(len) = end.checked_sub(cursor).filter(|&len| len <= 7) else {
            return Ok(None);
        };
        if state.hold_code > 7 {
            return Ok(None);
        }
        // H-board IDs occupy a different namespace from normalized kind
        // states. The original supply future/visible window/hold key is kept;
        // no selected-ID set or unfinished branch is cached as a Boolean.
        let key = self.memo_state(language, state)?;
        if let Some(value) = self.memo.get(&key).copied() {
            self.check_cancellation(control)?;
            return Ok(Some(value));
        }
        if self.zero_scores.contains(&key) {
            self.check_cancellation(control)?;
            return Ok(Some(PolicyValue::REJECT));
        }
        let mut query = KnownSupplyQuery {
            queue: [0; 7],
            len: len as u8,
            hold_code: state.hold_code,
            hold_enabled: self.hold_enabled,
        };
        for (offset, position) in (cursor..end).enumerate() {
            query.queue[offset] = self.trie.piece_at(state.observation_node, position).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_observation_visible_piece_missing"),
            )?;
        }
        let Some(accepts) =
            language.revealed_word_membership(state.language_node, &query, control)?
        else {
            return Ok(None);
        };
        self.check_cancellation(control)?;
        let count = if accepts {
            self.trie.nodes[state.observation_node as usize].subtree_count
        } else {
            0
        };
        let value = PolicyValue {
            score: f64::from(count),
            pattern_count: count,
        };
        self.insert_memo(key, value, false)?;
        Ok(Some(value))
    }

    fn observation_language_state<G: ObservationPieceLanguage>(
        &self,
        language: &G,
        mut state: PolicyState,
        control: &ExecutionControl,
    ) -> Result<PolicyState, WasmExactSearchError> {
        let relevant_end = self.observed_completion_end(language, state);
        if language.wants_revealed_supply(state.language_node) {
            if let Some(available) = relevant_end.and_then(|end| {
                self.revealed_supply_counts(
                    state.observation_node,
                    state.source_cursor,
                    state.hold_code,
                    end,
                )
            }) {
                state.language_node = language.fully_revealed_node_with_supply(
                    state.language_node,
                    available,
                    control,
                )?;
                return Ok(state);
            }
        }
        if self
            .policy
            .fully_observes_source(self.trie.sequence_len, usize::from(state.source_cursor))
            || self.trie.future_is_determined(state.observation_node)?
            || relevant_end.is_some()
        {
            state.language_node = language.fully_revealed_node(state.language_node, control)?;
        }
        Ok(state)
    }

    fn revealed_supply_counts(
        &self,
        observation_node: u32,
        cursor: u16,
        hold_code: u8,
        end: usize,
    ) -> Option<[u8; 7]> {
        let cursor = usize::from(cursor);
        if cursor > end {
            return None;
        }
        let mut counts = self
            .trie
            .sequence_range_counts(observation_node, cursor, end)?;
        if self.hold_enabled && (1..=7).contains(&hold_code) {
            let count = &mut counts[usize::from(hold_code - 1)];
            *count = count.checked_add(1)?;
        }
        Some(counts)
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
        let next_position = cursor
            .checked_add(1)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_observation_queue_position_overflow",
            ))?;
        let next_piece = self.trie.piece_at(state.observation_node, next_position);
        let mut piece_mask = current_piece.map_or(0_u8, |piece| 1 << (piece - 1));
        if self.hold_enabled {
            if (1..=7).contains(&state.hold_code) {
                piece_mask |= 1 << (state.hold_code - 1);
            } else if state.hold_code == 0 && current_piece.is_some() {
                piece_mask |= next_piece.map_or(0, |piece| 1 << (piece - 1));
            }
        }
        language.prepare_edges(state.language_node, piece_mask, control)?;
        let edge_count = language.edge_count(state.language_node).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_observation_language_node_out_of_range"),
        )?;
        if language.has_physical_actions(state.language_node) {
            return self.best_physical_transition(
                language,
                state,
                current_piece,
                next_piece,
                edge_count,
                control,
                record_action_checks,
            );
        }
        let mut best = PolicyValue::REJECT;
        let mut best_transition = None;
        let complete_count_cutoff =
            self.trie.uniform_weight.is_some() && language.fixed_horizon().is_some();
        let observation = self.trie.nodes[state.observation_node as usize];
        for edge_index in 0..edge_count {
            let (desired_piece, child) = language.edge(state.language_node, edge_index).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_observation_language_edge_out_of_range"),
            )?;
            if current_piece == Some(desired_piece) {
                let transition = self.supply_transition(
                    state,
                    child,
                    SupplyBranchKind::Current,
                    desired_piece,
                    None,
                )?;
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
                    let transition = self.supply_transition(
                        state,
                        child,
                        SupplyBranchKind::SwapHeld,
                        current_piece,
                        None,
                    )?;
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
                    let transition = self.supply_transition(
                        state,
                        child,
                        SupplyBranchKind::StoreCurrent,
                        current_piece,
                        Some(desired_piece),
                    )?;
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
                    physical_action: None,
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
            // Each reveal branch contains disjoint original pattern IDs.
            // Covering the whole information set reaches its exact integer
            // upper bound. The underlying language retains all other paths.
            if complete_count_cutoff && best.pattern_count == observation.subtree_count {
                best.score = f64::from(observation.subtree_count);
                return Ok((best, best_transition));
            }
        }
        Ok((best, best_transition))
    }

    #[allow(clippy::too_many_arguments)]
    fn best_physical_transition<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        current_piece: Option<u8>,
        next_piece: Option<u8>,
        edge_count: usize,
        control: &ExecutionControl,
        record_action_checks: bool,
    ) -> Result<(PolicyValue, Option<PolicyTransition>), WasmExactSearchError> {
        let held_piece = (1..=7)
            .contains(&state.hold_code)
            .then_some(state.hold_code);
        let requests = [
            (SupplyAction::UseCurrent, current_piece),
            (
                SupplyAction::SwapHeld,
                (self.hold_enabled && current_piece.is_some())
                    .then_some(held_piece)
                    .flatten(),
            ),
            (
                SupplyAction::StoreCurrentUseNext,
                (self.hold_enabled && state.hold_code == 0 && current_piece.is_some())
                    .then_some(next_piece)
                    .flatten(),
            ),
            (
                SupplyAction::ReleaseHeldAtTerminal,
                (self.projects_unplaced_lookahead
                    && self.hold_enabled
                    && usize::from(state.source_cursor) == self.trie.materialized_sequence_len)
                    .then_some(held_piece)
                    .flatten(),
            ),
        ];
        let complete_count_cutoff =
            self.trie.uniform_weight.is_some() && language.fixed_horizon().is_some();
        let observation = self.trie.nodes[state.observation_node as usize];
        let mut best = PolicyValue::REJECT;
        let mut best_transition = None;
        // Physical actions use a semantic order: supply action, kind, cells.
        // A supply action fixes one kind in this information set. Providers
        // order that kind's edges by cells, so the complete-count early exit
        // selects the same minimum key as exhaustive tie comparison. Child
        // IDs only identify execution states. This deliberately replaces the
        // legacy numeric-child tie and may choose another equally good policy.
        for (action, desired_piece) in requests {
            let Some(desired_piece) = desired_piece else {
                continue;
            };
            let mut previous_cells = None;
            for edge_index in 0..edge_count {
                let (piece, child) = language.edge(state.language_node, edge_index).ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "wasm_observation_language_edge_out_of_range",
                    ),
                )?;
                if piece != desired_piece {
                    continue;
                }
                let cells = language
                    .physical_action_cells(state.language_node, edge_index)
                    .filter(|cells| cells.count_ones() == 4)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_physical_action_cells_invalid",
                    ))?;
                if previous_cells.is_some_and(|previous| previous >= cells) {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_physical_action_order_invalid",
                    ));
                }
                previous_cells = Some(cells);
                let mut transition = match action {
                    SupplyAction::UseCurrent => self.supply_transition(
                        state,
                        child,
                        SupplyBranchKind::Current,
                        desired_piece,
                        None,
                    )?,
                    SupplyAction::SwapHeld => self.supply_transition(
                        state,
                        child,
                        SupplyBranchKind::SwapHeld,
                        current_piece.expect("swap request has a current piece"),
                        None,
                    )?,
                    SupplyAction::StoreCurrentUseNext => self.supply_transition(
                        state,
                        child,
                        SupplyBranchKind::StoreCurrent,
                        current_piece.expect("store request has a current piece"),
                        Some(desired_piece),
                    )?,
                    SupplyAction::ReleaseHeldAtTerminal => {
                        if !language.node(child).is_some_and(|child| child.accepting) {
                            continue;
                        }
                        PolicyTransition {
                            child,
                            source_cursor: state.source_cursor,
                            hold_code: TERMINAL_USED_HELD_CODE,
                            action,
                            physical_action: None,
                        }
                    }
                };
                transition.physical_action = Some(PhysicalPolicyAction {
                    kind: desired_piece,
                    cells,
                });
                self.consider(
                    language,
                    state,
                    transition,
                    &mut best,
                    &mut best_transition,
                    control,
                    record_action_checks,
                )?;
                if complete_count_cutoff && best.pattern_count == observation.subtree_count {
                    best.score = f64::from(observation.subtree_count);
                    return Ok((best, best_transition));
                }
            }
        }
        Ok((best, best_transition))
    }

    fn supply_transition(
        &self,
        state: PolicyState,
        child: u32,
        branch_kind: SupplyBranchKind,
        current_piece: u8,
        next_piece: Option<u8>,
    ) -> Result<PolicyTransition, WasmExactSearchError> {
        let hold_piece = match state.hold_code {
            0 => None,
            1..=7 => Some(piece_from_code(state.hold_code)),
            _ => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_hold_state_invalid",
                ))
            }
        };
        let mut supply_state = SupplyExecutionState {
            cursor: state.source_cursor,
            hold_piece,
            hold_empty: hold_piece.is_none(),
            ..self.supply_identity
        };
        supply_state.observation.observation_id =
            self.supply_identity.observation.observation_id ^ u64::from(state.observation_node);
        let step = SupplyExecutionAutomaton::sequence()
            .transition(
                supply_state,
                branch_kind,
                piece_from_code(current_piece),
                next_piece.map(piece_from_code),
            )
            .map_err(|error| match error {
                clearra_supply::SupplyExecutionError::CursorExhausted => {
                    WasmExactSearchError::InvalidProblem("wasm_observation_queue_position_overflow")
                }
                _ => WasmExactSearchError::InvalidProblem(
                    "wasm_observation_supply_transition_invalid",
                ),
            })?;
        Ok(PolicyTransition {
            child,
            source_cursor: step.next_state.cursor,
            hold_code: step.next_state.hold_piece.map_or(0, piece_code),
            action: match step.evidence.branch_kind {
                SupplyBranchKind::Current => SupplyAction::UseCurrent,
                SupplyBranchKind::SwapHeld => SupplyAction::SwapHeld,
                SupplyBranchKind::StoreCurrent => SupplyAction::StoreCurrentUseNext,
            },
            physical_action: None,
        })
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
        if value.score == 0.0 && value.pattern_count == 0 {
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
        // With a uniform positive source, score is the exact integer count.
        // Keep the first legal action, including an exact zero, and preserve
        // both physical semantic ties and the established raw-child tie.
        let bound = if self.branch_bound_enabled
            && self.trie.uniform_weight.is_some()
            && language.fixed_horizon().is_some()
            && self.terminal_supply_target.is_none()
        {
            best_transition.map(|incumbent| TransitionBound {
                incumbent_count: best.pattern_count,
                wins_equal_count: transition.precedes(incumbent),
            })
        } else {
            None
        };
        let TransitionValue::Exact(value) =
            self.transition_value(language, state, transition, bound, control)?
        else {
            return Ok(());
        };
        let better = value.score.total_cmp(&best.score).is_gt()
            || (value.score.total_cmp(&best.score).is_eq()
                && (value.pattern_count > best.pattern_count
                    || (value.pattern_count == best.pattern_count
                        && best_transition.is_none_or(|current| transition.precedes(current)))));
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
        bound: Option<TransitionBound>,
        control: &ExecutionControl,
    ) -> Result<TransitionValue, WasmExactSearchError> {
        let _child =
            language
                .node(transition.child)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_language_node_out_of_range",
                ))?;
        if transition.action == SupplyAction::ReleaseHeldAtTerminal {
            return self
                .solve(
                    language,
                    PolicyState {
                        language_node: transition.child,
                        observation_node: state.observation_node,
                        source_cursor: transition.source_cursor,
                        hold_code: transition.hold_code,
                    },
                    control,
                )
                .map(TransitionValue::Exact);
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
        // Known-source singleton transitions have no later reveal branch to
        // skip. Keep their hot path free of bound arithmetic and extra polls.
        let bound = bound.filter(|_| observation_count > 1);
        let mut score = 0.0;
        let mut pattern_count = 0_u32;
        // The trie contains equal-length original sequences. Descendants at
        // one later observed depth therefore partition this information set,
        // including distinct IDs with duplicate queue values. Reuse its exact
        // count instead of summing all descendants before every action.
        let mut remaining_count = if bound.is_some() {
            self.branch_bound[0] = self.branch_bound[0].saturating_add(1);
            self.trie
                .nodes
                .get(state.observation_node as usize)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_node_out_of_range",
                ))?
                .subtree_count
        } else {
            0
        };
        for (index, observation_node) in observations[..observation_count]
            .iter()
            .copied()
            .enumerate()
        {
            if let Some(bound) = bound {
                self.poll_cancellation(control)?;
                let upper_count = pattern_count.checked_add(remaining_count).ok_or(
                    WasmExactSearchError::InvalidProblem("wasm_observation_pattern_count_overflow"),
                )?;
                self.branch_bound[3] = self.branch_bound[3].saturating_add(1);
                if bound.dominates(upper_count) {
                    // The next solve (and its cancellation poll) will not run.
                    // Honor both caller cancellation and a native peer abort
                    // before returning this transition-local certificate.
                    self.check_cancellation(control)?;
                    self.branch_bound[1] = self.branch_bound[1].saturating_add(1);
                    self.branch_bound[2] =
                        self.branch_bound[2].saturating_add(observation_count - index);
                    return Ok(TransitionValue::Dominated);
                }
            }
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
            if bound.is_some() {
                let observed_count = self.trie.nodes[observation_node as usize].subtree_count;
                if value.pattern_count > observed_count {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_policy_count_exceeds_information_set",
                    ));
                }
                remaining_count = remaining_count.checked_sub(observed_count).ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "wasm_observation_reveal_count_not_disjoint",
                    ),
                )?;
            }
            score += value.score;
            pattern_count = pattern_count.checked_add(value.pattern_count).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_observation_pattern_count_overflow"),
            )?;
        }
        Ok(TransitionValue::Exact(PolicyValue {
            score,
            pattern_count,
        }))
    }

    fn collect_selected<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.poll_cancellation(control)?;
        self.collection[0] = self.collection[0].saturating_add(1);
        #[cfg(feature = "placement-dependency-pc")]
        if let Some(value) = self.revealed_word_value(language, state, control)? {
            if value.pattern_count == 0 {
                return Ok(());
            }
            let horizon = language
                .fixed_horizon()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_word_bitmap_horizon_missing",
                ))?;
            if value.pattern_count != self.trie.nodes[state.observation_node as usize].subtree_count
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_revealed_membership_not_uniform",
                ));
            }
            return self.collect_full_information_set(state.observation_node, horizon, control);
        }
        let state = self.observation_language_state(language, state, control)?;
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
        if value.pattern_count == 0 {
            return Ok(());
        }
        if self.collect_full {
            if let Some(horizon) = language.fixed_horizon() {
                let observed = self.trie.nodes.get(state.observation_node as usize).ok_or(
                    WasmExactSearchError::InvalidProblem("wasm_observation_node_out_of_range"),
                )?;
                if value.pattern_count == observed.subtree_count {
                    // Every policy selects a subset of these original IDs;
                    // reveal descendants are disjoint. Equal positive
                    // cardinality therefore identifies the whole information
                    // set, independently of weights and whether it is revealed.
                    // The solved value already includes terminal supply/EOF
                    // requirements: do not test them on this earlier state.
                    return self.collect_full_information_set(
                        state.observation_node,
                        horizon,
                        control,
                    );
                }
            }
        }
        if let Some((_, alternative)) = self.best_revealed_alternative(language, state, control)? {
            return match alternative {
                Some(language_node) => self.collect_selected(
                    language,
                    PolicyState {
                        language_node,
                        ..state
                    },
                    control,
                ),
                None => Ok(()),
            };
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

    fn collect_full_information_set(
        &mut self,
        observation_node: u32,
        horizon: u8,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.collection[1] = self.collection[1].saturating_add(1);
        self.selected_min_depth = self.selected_min_depth.min(horizon);
        self.selected_max_depth = self.selected_max_depth.max(horizon);
        self.trie
            .collect_patterns(observation_node, &mut self.selected_patterns, control)
    }

    fn poll_cancellation(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.cancellation_poll_counter = self.cancellation_poll_counter.wrapping_add(1);
        if self.cancellation_poll_counter & CANCELLATION_POLL_MASK == 0 {
            self.check_cancellation(control)?;
        }
        Ok(())
    }

    fn check_cancellation(&self, control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
        if control.is_cancelled()
            || self
                .peer_abort
                .as_ref()
                .is_some_and(|abort| abort.load(AtomicOrdering::Acquire))
        {
            return Err(WasmExactSearchError::Cancelled);
        }
        Ok(())
    }
}

fn collect_full_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_COLLECT_FULL") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => return Ok(true),
            Err(std::env::VarError::NotPresent) => {}
            _ => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_collect_full_mode_invalid",
                ))
            }
        }
    }
    Ok(true)
}

fn branch_bound_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_BRANCH_BOUND") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => return Ok(true),
            Err(std::env::VarError::NotPresent) => {}
            _ => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_branch_bound_mode_invalid",
                ))
            }
        }
    }
    Ok(true)
}

fn pre_reveal_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_PRE_REVEAL_MEMO") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => return Ok(true),
            Err(std::env::VarError::NotPresent) => {}
            _ => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_pre_reveal_mode_invalid",
                ))
            }
        }
    }
    Ok(false)
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
    fn original_universe_constructor_preserves_weighted_duplicate_ids_and_ties() {
        let (universe, index) = aliased_tie_universe();
        let language = AliasedTieLanguage::new();
        let control = ExecutionControl::default();
        let mut indexed = evaluator(&universe, &index);
        let mut original = QueueObservationPolicyEvaluator::for_original_universe(
            &universe,
            QueueObservationPolicy::VisibleSeven,
            0,
            None,
            true,
            true,
            false,
            None,
        )
        .unwrap();
        let expected = indexed.evaluate(&language, &control).unwrap();
        let actual = original.evaluate(&language, &control).unwrap();
        assert_eq!(
            actual.covered_patterns.words(),
            expected.covered_patterns.words()
        );
        assert_eq!(actual.covered_pattern_count, 4);
        assert_eq!(actual.covered_pattern_count, expected.covered_pattern_count);
        assert_eq!(
            actual.covered_weight.to_bits(),
            expected.covered_weight.to_bits()
        );
        assert_eq!(actual.covered_weight, 0.5);
        assert_eq!(actual.min_accepted_depth, expected.min_accepted_depth);
        assert_eq!(actual.max_accepted_depth, expected.max_accepted_depth);
        assert_eq!(original.trie.future_classes, indexed.trie.future_classes);
        assert_eq!(
            original.trie.initial_observations,
            indexed.trie.initial_observations
        );
        assert_eq!(
            original.trie.materialized_sequence_len,
            indexed.trie.materialized_sequence_len
        );
    }

    #[test]
    fn original_universe_constructor_preserves_preprojection_length_and_hold_eof() {
        use PieceKind::{I, J, L, O, S, T, Z};

        // Both rows become the same observed seven-piece word. The original
        // length must still be max(6, 7), not the first row's length.
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(801),
            PatternWeightModelId::new(802),
            vec![vec![I, O, T, S, Z, J], vec![I, O, T, S, Z, J, L]],
            vec![
                ProbabilityValue::new(0.25).unwrap(),
                ProbabilityValue::new(0.75).unwrap(),
            ],
            2,
            true,
            None,
        )
        .unwrap();
        let index = PatternPiecePositionIndex::compile(&universe).unwrap();
        let control = ExecutionControl::default();
        for (cursor, hold, accepted) in [
            (0, None, vec![I, O, T, S, Z, J, L]),
            (1, Some(I), vec![O, T, S, Z, J, L, I]),
        ] {
            let language = TestLanguage::from_sequences(&[accepted]);
            let mut indexed = QueueObservationPolicyEvaluator::new(
                &universe,
                &index,
                QueueObservationPolicy::VisibleSeven,
                cursor,
                hold,
                true,
                true,
                true,
                None,
            )
            .unwrap();
            let mut original = QueueObservationPolicyEvaluator::for_original_universe(
                &universe,
                QueueObservationPolicy::VisibleSeven,
                cursor,
                hold,
                true,
                true,
                true,
                None,
            )
            .unwrap();
            assert_eq!(original.trie.sequence_len, 7);
            assert_eq!(original.trie.materialized_sequence_len, 7);
            let expected = indexed.evaluate(&language, &control).unwrap();
            let actual = original.evaluate(&language, &control).unwrap();
            assert_eq!(actual.covered_pattern_count, 2);
            assert_eq!(actual.covered_weight, 1.0);
            assert_eq!(actual.min_accepted_depth, Some(7));
            assert_eq!(actual.max_accepted_depth, Some(7));
            assert_eq!(
                actual.covered_patterns.words(),
                expected.covered_patterns.words()
            );
            assert_eq!(actual.covered_pattern_count, expected.covered_pattern_count);
            assert_eq!(
                actual.covered_weight.to_bits(),
                expected.covered_weight.to_bits()
            );
            assert_eq!(actual.min_accepted_depth, expected.min_accepted_depth);
            assert_eq!(actual.max_accepted_depth, expected.max_accepted_depth);
        }
    }

    #[test]
    fn indexed_constructor_keeps_noncontiguous_original_subset_ids() {
        use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

        let (universe, _) = aliased_tie_universe();
        let selected =
            PatternBitSet::from_pattern_indices(universe.pattern_count(), vec![1, 6]).unwrap();
        let index = PatternPiecePositionIndex::compile_subset_before(
            &universe,
            &selected,
            universe.pattern_count(),
        )
        .unwrap();
        let language = TestLanguage::from_sequences(&[Vec::new()]);
        let coverage = evaluator(&universe, &index)
            .evaluate(&language, &ExecutionControl::default())
            .unwrap();
        assert_eq!(coverage.covered_patterns.words(), selected.words());
        assert_eq!(coverage.covered_pattern_count, 2);
        assert_eq!(coverage.covered_weight, 0.25);
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

    /// Distinct first edges are physical commitments even when their kinds
    /// match. The alternate root is their exact complete kind-language union.
    struct CommitmentProjectionLanguage {
        physical: TestLanguage,
        revealed: TestLanguage,
        horizon: u8,
        root_projections: std::cell::Cell<usize>,
    }

    impl CommitmentProjectionLanguage {
        const REVEALED: u32 = 1 << 31;

        fn new(sequences: &[Vec<PieceKind>]) -> Self {
            let horizon = u8::try_from(sequences[0].len()).unwrap();
            let mut physical = TestLanguage {
                nodes: vec![TestLanguageNode::default()],
            };
            for sequence in sequences {
                assert_eq!(sequence.len(), usize::from(horizon));
                let mut node = 0;
                for (index, &piece) in sequence.iter().enumerate() {
                    let child = physical.nodes.len() as u32;
                    physical.nodes.push(TestLanguageNode {
                        accepting: index + 1 == sequence.len(),
                        depth: (index + 1) as u8,
                        edges: Vec::new(),
                    });
                    physical.nodes[node as usize]
                        .edges
                        .push((piece_code(piece), child));
                    node = child;
                }
            }
            Self {
                physical,
                revealed: TestLanguage::from_sequences(sequences),
                horizon,
                root_projections: std::cell::Cell::new(0),
            }
        }
    }

    impl ObservationPieceLanguage for CommitmentProjectionLanguage {
        fn root(&self) -> u32 {
            0
        }

        fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
            if node & Self::REVEALED == 0 {
                self.physical.node(node)
            } else {
                self.revealed.node(node & !Self::REVEALED)
            }
        }

        fn edge_count(&self, node: u32) -> Option<usize> {
            if node & Self::REVEALED == 0 {
                self.physical.edge_count(node)
            } else {
                self.revealed.edge_count(node & !Self::REVEALED)
            }
        }

        fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
            if node & Self::REVEALED == 0 {
                self.physical.edge(node, index)
            } else {
                self.revealed
                    .edge(node & !Self::REVEALED, index)
                    .map(|(piece, child)| (piece, child | Self::REVEALED))
            }
        }

        fn fixed_horizon(&self) -> Option<u8> {
            Some(self.horizon)
        }

        fn fully_revealed_node(
            &self,
            node: u32,
            _control: &ExecutionControl,
        ) -> Result<u32, WasmExactSearchError> {
            if node == 0 {
                self.root_projections.set(self.root_projections.get() + 1);
                Ok(Self::REVEALED)
            } else {
                Ok(node)
            }
        }
    }

    #[test]
    fn relevant_draw_certificate_preserves_empty_hold_commitment_boundary() {
        use PieceKind::{I, J, L, O, S, T, Z};

        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(795),
            PatternWeightModelId::new(795),
            vec![vec![I, O, T, S, Z, J, L, T], vec![I, O, T, S, Z, J, L, S]],
            vec![ProbabilityValue::new(0.5).unwrap(); 2],
            2,
            true,
            None,
        )
        .unwrap();
        let index = PatternPiecePositionIndex::compile(&universe).unwrap();
        let make_evaluator = || {
            QueueObservationPolicyEvaluator::new(
                &universe,
                &index,
                QueueObservationPolicy::VisibleSeven,
                0,
                None,
                true,
                false,
                false,
                None,
            )
            .unwrap()
        };
        let control = ExecutionControl::default();

        // Seven locks can use eight draws: storing I must commit to one O
        // placement before the final T/S is revealed. Neither queue has the
        // multiplicity needed by the other complete trace.
        let seven = CommitmentProjectionLanguage::new(&[
            vec![O, T, S, Z, J, L, T],
            vec![O, T, S, Z, J, L, S],
        ]);
        let coverage = make_evaluator().evaluate(&seven, &control).unwrap();
        assert_eq!(seven.root_projections.get(), 0);
        assert_eq!(coverage.covered_pattern_count, 1);
        assert_eq!(coverage.covered_weight, 0.5);
        assert_eq!(coverage.min_accepted_depth, Some(7));
        assert_eq!(coverage.max_accepted_depth, Some(7));
        let premature = make_evaluator()
            .evaluate(&seven.revealed, &control)
            .unwrap();
        assert_eq!(premature.covered_pattern_count, 2);
        assert_eq!(premature.covered_weight, 1.0);

        // Six locks use at most seven draws. The same hidden eighth piece
        // cannot affect either completion, so root projection is permitted.
        let six =
            CommitmentProjectionLanguage::new(&[vec![O, T, S, Z, J, L], vec![O, T, S, Z, J, L]]);
        let coverage = make_evaluator().evaluate(&six, &control).unwrap();
        assert!(six.root_projections.get() > 0);
        assert_eq!(coverage.covered_pattern_count, 2);
        assert_eq!(coverage.covered_weight, 1.0);
        assert_eq!(coverage.min_accepted_depth, Some(6));
        assert_eq!(coverage.max_accepted_depth, Some(6));
    }

    #[test]
    fn adjacent_positive_weights_keep_the_weighted_commitment_choice() {
        use PieceKind::{I, J, L, O, S, T, Z};

        let lower = f64::from_bits(0.5_f64.to_bits() - 1);
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(796),
            PatternWeightModelId::new(796),
            vec![vec![I, O, T, S, Z, J, L, T], vec![I, O, T, S, Z, J, L, S]],
            vec![
                ProbabilityValue::new(lower).unwrap(),
                ProbabilityValue::new(0.5).unwrap(),
            ],
            2,
            true,
            None,
        )
        .unwrap();
        let index = PatternPiecePositionIndex::compile(&universe).unwrap();
        let language = CommitmentProjectionLanguage::new(&[
            vec![O, T, S, Z, J, L, T],
            vec![O, T, S, Z, J, L, S],
        ]);
        let mut evaluator = QueueObservationPolicyEvaluator::new(
            &universe,
            &index,
            QueueObservationPolicy::VisibleSeven,
            0,
            None,
            true,
            false,
            false,
            None,
        )
        .unwrap();
        assert!(evaluator.trie.uniform_weight.is_none());
        let coverage = evaluator
            .evaluate(&language, &ExecutionControl::default())
            .unwrap();

        // Equal-count choices must still prefer the larger actual weight.
        // An epsilon-based uniform certificate would lose this distinction.
        assert_eq!(coverage.covered_patterns.words(), &[2]);
        assert_eq!(coverage.covered_pattern_count, 1);
        assert_eq!(coverage.covered_weight, 0.5);
        assert_eq!(language.root_projections.get(), 0);
    }
}
