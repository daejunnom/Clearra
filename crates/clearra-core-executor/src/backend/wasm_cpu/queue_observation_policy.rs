use std::collections::HashMap;

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

use super::{piece_order_language::PieceOrderLanguageCache, WasmExactSearchError};

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
}

pub(super) struct RootedPieceLanguage<'a> {
    language: &'a PieceOrderLanguageCache,
    root: u32,
    total_depth: u8,
}

impl<'a> RootedPieceLanguage<'a> {
    pub fn new(
        language: &'a PieceOrderLanguageCache,
        root: u32,
    ) -> Result<Self, WasmExactSearchError> {
        let total_depth = language
            .node(root)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_observation_language_root_out_of_range",
            ))?
            .remaining_depth;
        Ok(Self {
            language,
            root,
            total_depth,
        })
    }
}

impl ObservationPieceLanguage for RootedPieceLanguage<'_> {
    fn root(&self) -> u32 {
        self.root
    }

    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        self.language
            .node(node)
            .map(|view| ObservationLanguageNode {
                accepting: view.accepting,
                depth: self.total_depth.saturating_sub(view.remaining_depth),
            })
    }

    fn edge_count(&self, node: u32) -> Option<usize> {
        self.language.edge_count(node)
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        self.language
            .edge(node, index)
            .map(|edge| (edge.piece_code(), edge.child()))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct QueueObservationMetrics {
    pub policy_states: usize,
    pub action_checks: usize,
    pub observation_nodes: usize,
    pub retained_bytes: usize,
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
    sequence_len: usize,
    materialized_sequence_len: usize,
    global_pattern_count: usize,
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
    ) -> Result<(f64, usize), WasmExactSearchError> {
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
            return Ok((node.subtree_weight, node.subtree_count as usize));
        };
        let mut weight = 0.0;
        let mut pattern_count = 0usize;
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
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PolicyState {
    language_node: u32,
    observation_node: u32,
    source_cursor: u16,
    hold_code: u8,
}

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
enum PolicyChoice {
    Reject,
    Accept,
    Transition(PolicyTransition),
}

#[derive(Clone, Copy, Debug)]
struct PolicyValue {
    weight: f64,
    pattern_count: usize,
    choice: PolicyChoice,
}

impl PolicyValue {
    const REJECT: Self = Self {
        weight: 0.0,
        pattern_count: 0,
        choice: PolicyChoice::Reject,
    };
}

pub(super) struct QueueObservationPolicyEvaluator {
    trie: ObservationTrie,
    policy: QueueObservationPolicy,
    initial_cursor: usize,
    initial_hold_code: u8,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    terminal_supply_target: Option<SetupTerminalSupplyTarget>,
    memo: HashMap<PolicyState, PolicyValue>,
    selected_patterns: Vec<u32>,
    selected_min_depth: u8,
    selected_max_depth: u8,
    cancellation_poll_counter: u32,
    action_checks: usize,
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
            trie,
            policy,
            initial_cursor,
            initial_hold_code: initial_hold.map_or(0, piece_code),
            hold_enabled,
            projects_unplaced_lookahead,
            terminal_supply_target,
            memo: HashMap::new(),
            selected_patterns: Vec::new(),
            selected_min_depth: u8::MAX,
            selected_max_depth: 0,
            cancellation_poll_counter: 0,
            action_checks: 0,
        })
    }

    pub fn evaluate<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        control: &ExecutionControl,
    ) -> Result<QueueObservationCoverage, WasmExactSearchError> {
        self.memo.clear();
        self.selected_patterns.clear();
        self.selected_min_depth = u8::MAX;
        self.selected_max_depth = 0;
        self.cancellation_poll_counter = 0;
        self.action_checks = 0;
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
            total_count = total_count.saturating_add(value.pattern_count);
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
                policy_states: self.memo.len(),
                action_checks: self.action_checks,
                observation_nodes: self.trie.nodes.len(),
                retained_bytes: self.retained_bytes(),
            },
        })
    }

    pub fn set_terminal_supply_target(&mut self, target: Option<SetupTerminalSupplyTarget>) {
        self.terminal_supply_target = target;
    }

    pub fn retained_bytes(&self) -> usize {
        self.trie.retained_bytes()
            + self.memo.capacity()
                * (core::mem::size_of::<PolicyState>() + core::mem::size_of::<PolicyValue>())
            + self.selected_patterns.capacity() * core::mem::size_of::<u32>()
    }

    fn solve<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        control: &ExecutionControl,
    ) -> Result<PolicyValue, WasmExactSearchError> {
        self.poll_cancellation(control)?;
        if let Some(value) = self.memo.get(&state).copied() {
            return Ok(value);
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
                choice: PolicyChoice::Accept,
            };
            self.memo.insert(state, value);
            return Ok(value);
        }
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
                )?;
            }
        }
        best.choice = best_transition.map_or(PolicyChoice::Reject, PolicyChoice::Transition);
        self.memo.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_observation_policy_storage_unavailable")
        })?;
        self.memo.insert(state, best);
        Ok(best)
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
    ) -> Result<(), WasmExactSearchError> {
        self.action_checks = self.action_checks.saturating_add(1);
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
        let mut pattern_count = 0usize;
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
            choice: PolicyChoice::Reject,
        })
    }

    fn collect_selected<G: ObservationPieceLanguage>(
        &mut self,
        language: &G,
        state: PolicyState,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.poll_cancellation(control)?;
        let value = self
            .memo
            .get(&state)
            .copied()
            .unwrap_or(PolicyValue::REJECT);
        match value.choice {
            PolicyChoice::Reject => Ok(()),
            PolicyChoice::Accept => {
                if value.pattern_count != 0 {
                    let depth = language
                        .node(state.language_node)
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_observation_language_node_out_of_range",
                        ))?
                        .depth;
                    self.selected_min_depth = self.selected_min_depth.min(depth);
                    self.selected_max_depth = self.selected_max_depth.max(depth);
                }
                self.trie.collect_accepted_patterns(
                    state.observation_node,
                    state.source_cursor,
                    state.hold_code,
                    self.terminal_supply_target,
                    &mut self.selected_patterns,
                    control,
                )
            }
            PolicyChoice::Transition(transition) => {
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
        }
    }

    fn poll_cancellation(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.cancellation_poll_counter = self.cancellation_poll_counter.wrapping_add(1);
        if self.cancellation_poll_counter & CANCELLATION_POLL_MASK == 0 && control.is_cancelled() {
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
}
