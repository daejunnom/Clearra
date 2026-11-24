use std::{collections::HashMap, sync::Arc};

#[cfg(any(test, feature = "local-search-ab", feature = "placement-dependency-pc"))]
use clearra_core_domain::piece::piece_kind::PieceKind;

use super::{buildup::BuildOrderGraph, mix_digest, WasmExactSearchError};

#[cfg(any(
    test,
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
#[path = "closed_prefix_frontier.rs"]
mod closed_prefix_frontier;

const EMPTY_REFERENCE: u32 = u32::MAX;
const INITIAL_BUCKET_COUNT: usize = 1024;
const COMPACT_PIECE_RANGE_EDGE_LIMIT: usize = u8::MAX as usize;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub(super) struct CanonicalPieceEdge(u32);

impl CanonicalPieceEdge {
    const CHILD_BITS: u32 = 29;
    const CHILD_MASK: u32 = (1_u32 << Self::CHILD_BITS) - 1;

    fn new(child: u32, piece_code: u8) -> Option<Self> {
        let piece_index = piece_code.checked_sub(1)?;
        (piece_index < 7 && child <= Self::CHILD_MASK)
            .then_some(Self((u32::from(piece_index) << Self::CHILD_BITS) | child))
    }

    pub const fn child(self) -> u32 {
        self.0 & Self::CHILD_MASK
    }

    pub const fn piece_code(self) -> u8 {
        ((self.0 >> Self::CHILD_BITS) as u8) + 1
    }
}

const _: () = assert!(core::mem::size_of::<CanonicalPieceEdge>() == 4);

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct CanonicalPieceNode {
    edge_start: u32,
    next_same_bucket: u32,
    edge_count: u16,
    remaining_depth: u8,
    accepting: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PieceLanguageNodeView {
    pub accepting: bool,
    pub remaining_depth: u8,
}

const _: () = assert!(core::mem::size_of::<CanonicalPieceNode>() == 12);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CoverageCacheKey {
    language_id: u32,
    pattern_index_id: u32,
}

/// Query-local canonical piece-order language and coverage cache.
///
/// The cache is namespaced by one exact SearchProblem session, so its fixed
/// PieceSource, initial HoldAutomaton state, rules, and universe identity are
/// implicit. A language id is assigned only after exact edge-list comparison;
/// hashes never authorize merging.
#[derive(Default)]
pub(super) struct PieceOrderLanguageCache {
    // Numeric node IDs are reusable after recycling. The token brands one
    // live arena without a counter wrap or a per-query allocation.
    #[cfg(feature = "placement-dependency-pc")]
    arena_identity: Arc<()>,
    nodes: Vec<CanonicalPieceNode>,
    edges: Vec<CanonicalPieceEdge>,
    piece_range_ends: Vec<[u8; 7]>,
    bucket_heads: Vec<u32>,
    graph_classes: Vec<u32>,
    edge_scratch: Vec<CanonicalPieceEdge>,
    interning_disabled: bool,
    coverage: HashMap<CoverageCacheKey, Arc<[u64]>>,
    seen_once: HashMap<CoverageCacheKey, ()>,
    coverage_hits: usize,
    coverage_misses: usize,
    #[cfg(any(
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    prefix_frontier: closed_prefix_frontier::PrefixFrontierScratch,
    #[cfg(any(
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    prefix_frontier_compilations: usize,
}

pub(super) enum CoverageCacheLookup {
    Hit(Arc<[u64]>),
    Miss { admit_after_compute: bool },
}

/// A language constructed from a separately certified placement poset.
/// These are residual class-count states, not physical/subset build states.
#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PosetLanguageResult {
    pub root: u32,
    pub residual_states: usize,
    pub operation_classes: usize,
}

/// Proof of the existence of an exact Boolean transition, not its
/// multiplicity, score, replay witness, or placement identity.
#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConditionalTransitionProof {
    Allowed,
    Forbidden,
    Unknown,
}

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConditionalLanguageCompilation {
    Certified(ConditionalLanguageResult),
    Unknown,
    BudgetExceeded,
    Unsupported,
}

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ConditionalLanguageResult {
    /// A certified empty language has no root. A nonempty root belongs to
    /// this cache and cannot outlive arena recycling.
    pub root: Option<u32>,
    pub residual_states: usize,
    pub checked_transitions: usize,
}

#[cfg(any(test, feature = "local-search-ab"))]
const MAX_CONDITIONAL_STATES: usize = 64;
#[cfg(any(test, feature = "local-search-ab"))]
const MAX_CONDITIONAL_TRANSITIONS: usize = 256;

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy)]
struct ConditionalResidual {
    executed: u16,
    allowed: u16,
}

#[cfg(any(test, feature = "local-search-ab"))]
const MAX_POSET_OPERATIONS: usize = 15;

#[cfg(any(test, feature = "local-search-ab"))]
struct PosetOperationClass {
    members: u16,
    piece_code: u8,
    count: u8,
    predecessors: u16,
    stride: u32,
}

/// Exact source-queue counts under the restricted contract of
/// `count_distinct_no_hold_one_bag_poset`, without a queue decision diagram.
#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DistinctNoHoldOneBagCount {
    pub accepted_queue_count: u128,
    pub total_queue_count: u128,
    pub decomposed_subproblems: usize,
    pub ideal_states: usize,
}

impl PieceOrderLanguageCache {
    #[cfg(feature = "placement-dependency-pc")]
    pub fn arena_identity(&self) -> &Arc<()> {
        &self.arena_identity
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Each deterministic-union node is a prefix of at least one source
    /// path. Summing source prefixes (without quotienting duplicate words) is
    /// therefore an upper bound, even when several roots share suffixes.
    /// The caller admits `node_count * size_of::<u128>()` before this scratch.
    pub fn checked_prefix_path_upper_bound(&self, roots: &[u32]) -> Option<u128> {
        let mut counts = Vec::new();
        counts.try_reserve_exact(self.nodes.len()).ok()?;
        for (index, node) in self.nodes.iter().copied().enumerate() {
            let mut prefixes = 1_u128;
            for edge in self.node_edges(node) {
                if edge.child() as usize >= index {
                    return None;
                }
                prefixes = prefixes.checked_add(*counts.get(edge.child() as usize)?)?;
            }
            counts.push(prefixes);
        }
        roots.iter().try_fold(0_u128, |sum, root| {
            sum.checked_add(*counts.get(*root as usize)?)
        })
    }

    pub fn maximum_root_depth(&self, roots: &[u32]) -> Option<u8> {
        roots.iter().try_fold(0_u8, |depth, root| {
            Some(depth.max(self.nodes.get(*root as usize)?.remaining_depth))
        })
    }

    /// Imports one complete worker arena by exact node/edge equality and
    /// returns its source-ID to destination-ID map. Factored policy shards
    /// rebind all retained suffixes through this one shared import.
    #[cfg(any(test, feature = "parallel"))]
    pub fn import_node_map(&mut self, source: &Self) -> Result<Vec<u32>, WasmExactSearchError> {
        self.ensure_buckets();
        let mut rebound = Vec::new();
        rebound.try_reserve_exact(source.nodes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_import_storage_unavailable")
        })?;
        for (index, node) in source.nodes.iter().copied().enumerate() {
            self.edge_scratch.clear();
            let edges = source.node_edges(node);
            self.edge_scratch.try_reserve(edges.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_piece_language_import_storage_unavailable",
                )
            })?;
            for edge in edges {
                // Children are interned before parents in every producer.
                // Validate that invariant rather than trusting a hash/root ID.
                let child = rebound.get(edge.child() as usize).copied().ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_import_not_topological",
                    ),
                )?;
                if edge.child() as usize >= index {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_import_not_topological",
                    ));
                }
                self.edge_scratch
                    .push(CanonicalPieceEdge::new(child, edge.piece_code()).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_piece_language_child_index_overflow",
                        ),
                    )?);
            }
            self.edge_scratch.sort_unstable();
            self.edge_scratch.dedup();
            let depth = self.remaining_depth(node.accepting)?;
            if depth != node.remaining_depth {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_piece_language_import_depth_mismatch",
                ));
            }
            rebound.push(self.intern_node(node.accepting, depth)?);
        }
        Ok(rebound)
    }

    /// A numeric root is scoped to its source owner and must never be copied
    /// into another cache without rebinding all child references first.
    #[cfg(any(test, all(feature = "parallel", not(target_family = "wasm"))))]
    pub fn import_roots(
        &mut self,
        source: &Self,
        roots: &[u32],
    ) -> Result<Vec<u32>, WasmExactSearchError> {
        let rebound = self.import_node_map(source)?;
        let mut imported = Vec::new();
        imported.try_reserve_exact(roots.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_import_storage_unavailable")
        })?;
        for root in roots {
            imported.push(*rebound.get(*root as usize).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_observation_language_root_out_of_range"),
            )?);
        }
        Ok(imported)
    }

    pub fn canonicalize(&mut self, graph: &BuildOrderGraph) -> Result<u32, WasmExactSearchError> {
        self.prepare_graph_classes(graph.nodes.len())?;

        for node_index in (0..graph.nodes.len()).rev() {
            let source = &graph.nodes[node_index];
            if !source.live {
                continue;
            }
            self.edge_scratch.clear();
            for edge in graph
                .piece_edges(node_index)
                .iter()
                .filter(|edge| graph.nodes[edge.to as usize].live)
            {
                let child = self.graph_classes[edge.to as usize];
                if child == EMPTY_REFERENCE {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_not_topological",
                    ));
                }
                self.edge_scratch.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_edge_scratch_unavailable",
                    )
                })?;
                self.edge_scratch.push(
                    CanonicalPieceEdge::new(child, piece_code(edge.piece)).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_piece_language_child_index_overflow",
                        ),
                    )?,
                );
            }
            self.edge_scratch.sort_unstable();
            self.edge_scratch.dedup();
            let remaining_depth = self.remaining_depth(source.accepting())?;
            let reference = self.intern_node(source.accepting(), remaining_depth)?;
            self.graph_classes[node_index] = reference;
        }

        self.graph_classes
            .get(graph.root as usize)
            .copied()
            .filter(|reference| *reference != EMPTY_REFERENCE)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_root_not_live",
            ))
    }

    /// Construct the kind-word language of a supplied exact placement poset.
    /// Geometry, grounding, line-clear and kick correctness must already be
    /// certified by the caller; this method certifies only the poset language.
    /// No BuildOrderGraph, physical board or reachability state is constructed.
    ///
    /// Same-kind operations are exchangeable only when their complete strict
    /// predecessor AND successor sets are identical. Such incomparable twins
    /// become one count coordinate. In particular, a free repeated multiset
    /// uses a product of `(multiplicity + 1)` coordinates, not labeled subsets.
    /// Other same-kind alternatives remain distinct edges with union semantics.
    ///
    /// `None` means unsupported/invalid input or a refused state budget, never
    /// infeasibility. Budget admission finishes before mutating the language
    /// arena. The returned root has the usual owner/recycling lifetime.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub fn compile_poset_language(
        &mut self,
        pieces: &[PieceKind],
        predecessors: &[u16],
        state_budget: usize,
    ) -> Result<Option<PosetLanguageResult>, WasmExactSearchError> {
        if pieces.len() != predecessors.len() || state_budget == 0 {
            return Ok(None);
        }
        let Some(closure) = poset_predecessor_closure(predecessors) else {
            return Ok(None);
        };
        let Some((classes, initial)) = poset_operation_classes(pieces, &closure) else {
            return Ok(None);
        };
        let Some(states) = admitted_poset_residual_states(&classes, initial, state_budget) else {
            return Ok(None);
        };
        let mut references = Vec::new();
        references.try_reserve_exact(states.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_poset_language_storage_unavailable")
        })?;
        self.ensure_buckets();
        for (index, &state) in states.iter().enumerate() {
            self.edge_scratch.clear();
            let (remaining, unfinished, depth) = poset_residual_counts(&classes, state);
            for (class_index, class) in classes.iter().enumerate() {
                if remaining[class_index] == 0 || class.predecessors & unfinished != 0 {
                    continue;
                }
                let child_index = states.binary_search(&(state - class.stride)).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_poset_language_child_missing")
                })?;
                if child_index >= index {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_poset_language_not_topological",
                    ));
                }
                self.edge_scratch.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_poset_language_storage_unavailable")
                })?;
                self.edge_scratch.push(
                    CanonicalPieceEdge::new(references[child_index], class.piece_code).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_piece_language_child_index_overflow",
                        ),
                    )?,
                );
            }
            self.edge_scratch.sort_unstable();
            self.edge_scratch.dedup();
            // In an acyclic poset every nonterminal residual has a minimal
            // class. Check before interning so an invalid empty language can
            // never be mistaken for an accepting terminal.
            if state != 0 && self.edge_scratch.is_empty() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_poset_language_nonterminal_has_no_edges",
                ));
            }
            references.push(self.intern_node(state == 0, depth)?);
        }
        Ok(Some(PosetLanguageResult {
            root: *references.last().expect("admitted poset includes its root"),
            residual_states: states.len(),
            operation_classes: classes.len(),
        }))
    }

    /// Compile exact kind words for a small Boolean relation over concrete
    /// executed-operation masks. The caller must establish that the mask is
    /// a sufficient physical state: row events, occupancy and all relevant
    /// board semantics must be derived exactly, with no hidden prefix state.
    /// The callback is for proofs only, not for rerunning physical search.
    ///
    /// Finish admission and check every outgoing transition of every
    /// reachable nonterminal mask before touching the language arena. A
    /// refused budget or Unknown delegates the whole language to the existing
    /// exact producer. Kind equality is not operation exchangeability:
    /// concrete residuals remain distinct until exact suffix interning.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub fn compile_conditional_language(
        &mut self,
        pieces: &[PieceKind],
        state_budget: usize,
        transition_budget: usize,
        mut prove: impl FnMut(u16, usize) -> ConditionalTransitionProof,
    ) -> Result<ConditionalLanguageCompilation, WasmExactSearchError> {
        use ConditionalLanguageCompilation::{BudgetExceeded, Certified, Unknown, Unsupported};

        if pieces.is_empty() || pieces.len() > MAX_POSET_OPERATIONS {
            return Ok(Unsupported);
        }
        let state_limit = state_budget.min(MAX_CONDITIONAL_STATES);
        let transition_limit = transition_budget.min(MAX_CONDITIONAL_TRANSITIONS);
        if state_limit == 0 || transition_limit == 0 {
            return Ok(BudgetExceeded);
        }
        let full = (1_u16 << pieces.len()) - 1;
        let mut residuals = [ConditionalResidual {
            executed: 0,
            allowed: 0,
        }; MAX_CONDITIONAL_STATES];
        let mut residual_count = 1;
        let mut checked_transitions = 0;
        let mut index = 0;
        while index < residual_count {
            let executed = residuals[index].executed;
            let mut pending = full ^ executed;
            let mut allowed = 0;
            while pending != 0 {
                if checked_transitions == transition_limit {
                    return Ok(BudgetExceeded);
                }
                let operation = pending.trailing_zeros() as usize;
                let bit = 1_u16 << operation;
                pending ^= bit;
                checked_transitions += 1;
                match prove(executed, operation) {
                    ConditionalTransitionProof::Unknown => return Ok(Unknown),
                    ConditionalTransitionProof::Forbidden => continue,
                    ConditionalTransitionProof::Allowed => allowed |= bit,
                }

                // Adding a concrete operation strictly increases its mask.
                // Keep the pending suffix sorted to merge duplicates while
                // obtaining topological order without a mask-sized table.
                let child = executed | bit;
                let next = index + 1;
                if let Err(offset) = residuals[next..residual_count]
                    .binary_search_by_key(&child, |state| state.executed)
                {
                    if residual_count == state_limit {
                        return Ok(BudgetExceeded);
                    }
                    let insert = next + offset;
                    residuals.copy_within(insert..residual_count, insert + 1);
                    residuals[insert] = ConditionalResidual {
                        executed: child,
                        allowed: 0,
                    };
                    residual_count += 1;
                }
            }
            residuals[index].allowed = allowed;
            index += 1;
        }

        let result = |root| ConditionalLanguageResult {
            root,
            residual_states: residual_count,
            checked_transitions,
        };
        if residuals[residual_count - 1].executed != full {
            return Ok(Certified(result(None)));
        }

        self.ensure_buckets();
        let mut references = [EMPTY_REFERENCE; MAX_CONDITIONAL_STATES];
        for index in (0..residual_count).rev() {
            let ConditionalResidual { executed, allowed } = residuals[index];
            let mut edges = allowed;
            self.edge_scratch.clear();
            self.edge_scratch
                .try_reserve(allowed.count_ones() as usize)
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_conditional_language_edge_scratch_unavailable",
                    )
                })?;
            while edges != 0 {
                let operation = edges.trailing_zeros() as usize;
                let bit = 1_u16 << operation;
                edges ^= bit;
                let next = index + 1;
                let offset = residuals[next..residual_count]
                    .binary_search_by_key(&(executed | bit), |state| state.executed)
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_conditional_language_child_missing",
                        )
                    })?;
                let child = references[next + offset];
                if child != EMPTY_REFERENCE {
                    self.edge_scratch.push(
                        CanonicalPieceEdge::new(child, piece_code(pieces[operation])).ok_or(
                            WasmExactSearchError::InvalidProblem(
                                "wasm_piece_language_child_index_overflow",
                            ),
                        )?,
                    );
                }
            }
            let accepting = executed == full;
            if accepting || !self.edge_scratch.is_empty() {
                self.edge_scratch.sort_unstable();
                self.edge_scratch.dedup();
                let depth = self.remaining_depth(accepting)?;
                references[index] = self.intern_node(accepting, depth)?;
            }
        }
        if references[0] == EMPTY_REFERENCE {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_conditional_language_certified_terminal_not_live",
            ));
        }
        Ok(Certified(result(Some(references[0]))))
    }

    /// Intern the Boolean words supported by a strict prefix relation. Both
    /// directions have already been closed by word-level composition: these
    /// are the live prefixes reachable from zero, not a traversal frontier.
    #[cfg(any(
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    pub(super) fn compile_closed_prefix_language(
        &mut self,
        pieces: &[PieceKind],
        permitted_parents: &[[u64; super::symbolic_prefix_domain::MAX_PREFIX_WORDS]],
        active: &[u64; super::symbolic_prefix_domain::MAX_PREFIX_WORDS],
        inspected_edges: &mut usize,
    ) -> Result<Option<u32>, WasmExactSearchError> {
        self.compile_closed_prefix_language_with_mode(
            pieces,
            permitted_parents,
            active,
            inspected_edges,
            None,
            None,
        )
    }

    #[cfg(feature = "placement-dependency-pc")]
    pub(super) fn compile_product_closed_prefix_language(
        &mut self,
        pieces: &[PieceKind],
        permitted_parents: &[[u64; super::symbolic_prefix_domain::MAX_PREFIX_WORDS]],
        active: &[u64; super::symbolic_prefix_domain::MAX_PREFIX_WORDS],
        inspected_edges: &mut usize,
    ) -> Result<Option<u32>, WasmExactSearchError> {
        // Product uses the existing exact bottom-up subset language, irrespective
        // of whether a local experiment mode is also compiled into this binary.
        self.compile_closed_prefix_language_with_mode(
            pieces,
            permitted_parents,
            active,
            inspected_edges,
            Some(0),
            None,
        )
    }

    #[cfg(any(
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    pub(super) fn frontier_compilations(&self) -> usize {
        self.prefix_frontier_compilations
    }

    #[cfg(any(
        test,
        feature = "placement-dependency-pc",
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    fn compile_closed_prefix_language_with_mode(
        &mut self,
        pieces: &[PieceKind],
        permitted_parents: &[[u64; super::symbolic_prefix_domain::MAX_PREFIX_WORDS]],
        active: &[u64; super::symbolic_prefix_domain::MAX_PREFIX_WORDS],
        inspected_edges: &mut usize,
        // None selects the process experiment mode; Some(0) forces the
        // reference, while a small positive bound tests exact fallback.
        _explicit_frontier_budget: Option<usize>,
        mut residuals: Option<&mut Vec<u32>>,
    ) -> Result<Option<u32>, WasmExactSearchError> {
        use super::symbolic_prefix_domain::{PrefixDomain, MAX_PREFIX_WORDS};

        let invalid = || {
            WasmExactSearchError::InvalidProblem("wasm_symbolic_prefix_language_bound_inconsistent")
        };
        if let Some(residuals) = residuals.as_mut() {
            residuals.clear();
        }
        let domain = PrefixDomain::new(pieces.len()).ok_or_else(invalid)?;
        if pieces.len() != permitted_parents.len() {
            return Err(invalid());
        }
        if active.iter().all(|&word| word == 0) {
            return Ok(None);
        }
        let full = domain.terminal();
        let last = full / 64;
        let terminal_bit = 1_u64 << (full % 64);
        let last_valid = domain.nonterminal_lanes(last) | terminal_bit;
        if active[0] & 1 == 0
            || active[last] & terminal_bit == 0
            || active[last] & !last_valid != 0
            || active[last + 1..].iter().any(|&word| word != 0)
        {
            return Err(invalid());
        }
        #[cfg(any(
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        {
            let frontier_budget = if let Some(budget) = _explicit_frontier_budget {
                budget
            } else if closed_prefix_frontier::experiment_enabled()? {
                closed_prefix_frontier::MAX_FRONTIER_STATES
            } else {
                0
            };
            if frontier_budget != 0 {
                let mut frontier = std::mem::take(&mut self.prefix_frontier);
                let root = frontier.compile(
                    self,
                    pieces,
                    permitted_parents,
                    active,
                    full,
                    frontier_budget,
                    inspected_edges,
                );
                self.prefix_frontier = frontier;
                if let Some(root) = root? {
                    self.prefix_frontier_compilations =
                        self.prefix_frontier_compilations.saturating_add(1);
                    return Ok(Some(root));
                }
            }
        }
        self.ensure_buckets();
        self.edge_scratch.clear();
        self.edge_scratch.try_reserve(pieces.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_symbolic_prefix_language_scratch_unavailable",
            )
        })?;
        let mut classes = [EMPTY_REFERENCE; MAX_PREFIX_WORDS * 64];
        for (word, &active_word) in active[..=last].iter().enumerate().rev() {
            let mut uncompiled = active_word;
            while uncompiled != 0 {
                let lane = (u64::BITS - 1 - uncompiled.leading_zeros()) as usize;
                let subset = word * 64 + lane;
                let bit = 1_u64 << lane;
                uncompiled ^= bit;
                self.edge_scratch.clear();
                for (operation, (&piece, parent)) in
                    pieces.iter().zip(permitted_parents).enumerate()
                {
                    if parent[word] & bit == 0 {
                        continue;
                    }
                    *inspected_edges += 1;
                    let child = subset | (1 << operation);
                    if child == subset {
                        return Err(invalid());
                    }
                    let class = classes[child];
                    if class != EMPTY_REFERENCE {
                        self.edge_scratch.push(
                            CanonicalPieceEdge::new(class, piece_code(piece))
                                .ok_or_else(invalid)?,
                        );
                    }
                }
                let accepting = subset == full;
                if !accepting && self.edge_scratch.is_empty() {
                    return Err(invalid());
                }
                self.edge_scratch.sort_unstable();
                self.edge_scratch.dedup();
                let depth = self.remaining_depth(accepting)?;
                classes[subset] = self.intern_node(accepting, depth)?;
            }
        }
        if let Some(residuals) = residuals {
            residuals.try_reserve_exact(full + 1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_observation_dependency_storage_unavailable",
                )
            })?;
            residuals.extend_from_slice(&classes[..=full]);
        }
        Ok(Some(classes[0]))
    }

    /// Consume an exact BFS operation-mask traversal, without repeating any
    /// physical query. The owner performed terminal acceptance and emitted a
    /// bit only for an already verified operation. Unvisited entries in the
    /// dense subset lookup may still contain data from an earlier traversal.
    #[cfg(any(test, all(feature = "local-search-ab", feature = "parallel")))]
    pub(super) fn compile_traversed_subset_masks(
        &mut self,
        pieces: &[PieceKind],
        subsets: &[u16],
        allowed_operations: &[u16],
        node_id_by_subset: &[u32],
        terminal_accepted: bool,
    ) -> Result<Option<u32>, WasmExactSearchError> {
        let invalid_traversal =
            || WasmExactSearchError::InvalidProblem("wasm_piece_language_invalid_subset_traversal");
        if pieces.is_empty() || pieces.len() > MAX_POSET_OPERATIONS {
            return Err(invalid_traversal());
        }
        let subset_count = 1_usize << pieces.len();
        if node_id_by_subset.len() != subset_count
            || subsets.len() != allowed_operations.len()
            || subsets.len() > subset_count
            || subsets.first() != Some(&0)
        {
            return Err(invalid_traversal());
        }
        let full_subset = (subset_count - 1) as u16;
        // Validate shape before authoring any language nodes. Use the
        // existing class buffer as a reachability mark, not an extra graph.
        // Each mark is overwritten below before it can be used as a node ID.
        self.prepare_graph_classes(subsets.len())?;
        self.graph_classes[0] = 0;
        let mut terminal_seen = false;
        for (index, (&subset, &allowed)) in subsets.iter().zip(allowed_operations).enumerate() {
            if self.graph_classes[index] == EMPTY_REFERENCE
                || node_id_by_subset.get(subset as usize).copied() != Some(index as u32)
                || allowed & !(full_subset & !subset) != 0
            {
                return Err(invalid_traversal());
            }
            terminal_seen |= subset == full_subset;
            let mut outgoing = allowed;
            while outgoing != 0 {
                let bit = 1_u16 << outgoing.trailing_zeros();
                outgoing ^= bit;
                let child = subset | bit;
                let child_index = node_id_by_subset[child as usize] as usize;
                if child_index <= index || subsets.get(child_index).copied() != Some(child) {
                    return Err(invalid_traversal());
                }
                self.graph_classes[child_index] = 0;
            }
        }
        if !terminal_accepted {
            return Ok(None);
        }
        if !terminal_seen {
            return Err(invalid_traversal());
        }

        // Child indices are strictly later in traversal order; reverse BFS,
        // not numeric subset order, supplies their already-computed classes.
        for index in (0..subsets.len()).rev() {
            let subset = subsets[index];
            let accepting = subset == full_subset;
            let mut outgoing = allowed_operations[index];
            self.edge_scratch.clear();
            self.edge_scratch
                .try_reserve(outgoing.count_ones() as usize)
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_edge_scratch_unavailable",
                    )
                })?;
            while outgoing != 0 {
                let operation = outgoing.trailing_zeros() as usize;
                let bit = 1_u16 << operation;
                outgoing ^= bit;
                let child_index = node_id_by_subset[(subset | bit) as usize] as usize;
                let child = self.graph_classes[child_index];
                if child != EMPTY_REFERENCE {
                    self.edge_scratch.push(
                        CanonicalPieceEdge::new(child, piece_code(pieces[operation])).ok_or(
                            WasmExactSearchError::InvalidProblem(
                                "wasm_piece_language_child_index_overflow",
                            ),
                        )?,
                    );
                }
            }
            self.graph_classes[index] = if !accepting && self.edge_scratch.is_empty() {
                EMPTY_REFERENCE
            } else {
                self.edge_scratch.sort_unstable();
                self.edge_scratch.dedup();
                let remaining_depth = self.remaining_depth(accepting)?;
                self.intern_node(accepting, remaining_depth)?
            };
        }
        let root = self.graph_classes[0];
        if root == EMPTY_REFERENCE {
            return Err(invalid_traversal());
        }
        Ok(Some(root))
    }

    /// Solve Boolean completion and the exact piece suffix language in one
    /// backward pass. Graph edges already contain qualified physical moves;
    /// this step performs no field or reachability query. Multiple same-kind
    /// operation edges remain a union of suffixes, not a path count.
    ///
    /// The returned ID belongs to this worker's language arena and is consumed
    /// before that arena can be recycled. CountAll continues to use operation
    /// edges and its separate weighted product instead of this quotient.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub fn canonicalize_and_mark_live(
        &mut self,
        graph: &mut BuildOrderGraph,
    ) -> Result<Option<u32>, WasmExactSearchError> {
        if graph.nodes.is_empty() || graph.root as usize >= graph.nodes.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_root_out_of_range",
            ));
        }
        self.prepare_graph_classes(graph.nodes.len())?;
        for node_index in (0..graph.nodes.len()).rev() {
            let accepting = graph.nodes[node_index].accepting();
            self.edge_scratch.clear();
            for edge in graph.piece_edges(node_index) {
                let child_index = edge.to as usize;
                if child_index <= node_index || child_index >= graph.nodes.len() {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_not_topological",
                    ));
                }
                let child = self.graph_classes[child_index];
                if child == EMPTY_REFERENCE {
                    // A dead child contributes the empty suffix language.
                    continue;
                }
                self.edge_scratch.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_edge_scratch_unavailable",
                    )
                })?;
                self.edge_scratch.push(
                    CanonicalPieceEdge::new(child, piece_code(edge.piece)).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_piece_language_child_index_overflow",
                        ),
                    )?,
                );
            }
            let live = accepting || !self.edge_scratch.is_empty();
            graph.nodes[node_index].live = live;
            if live {
                self.edge_scratch.sort_unstable();
                self.edge_scratch.dedup();
                let remaining_depth = self.remaining_depth(accepting)?;
                self.graph_classes[node_index] = self.intern_node(accepting, remaining_depth)?;
            }
        }
        Ok(self
            .graph_classes
            .get(graph.root as usize)
            .copied()
            .filter(|reference| *reference != EMPTY_REFERENCE))
    }

    fn prepare_graph_classes(&mut self, node_count: usize) -> Result<(), WasmExactSearchError> {
        self.ensure_buckets();
        if self.graph_classes.len() < node_count {
            self.graph_classes
                .try_reserve_exact(node_count - self.graph_classes.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_class_storage_unavailable",
                    )
                })?;
            self.graph_classes.resize(node_count, EMPTY_REFERENCE);
        }
        self.graph_classes[..node_count].fill(EMPTY_REFERENCE);
        Ok(())
    }

    pub fn coverage(
        &mut self,
        language_id: u32,
        pattern_index_id: u32,
    ) -> Result<CoverageCacheLookup, WasmExactSearchError> {
        let key = CoverageCacheKey {
            language_id,
            pattern_index_id,
        };
        if let Some(words) = self.coverage.get(&key) {
            self.coverage_hits = self.coverage_hits.saturating_add(1);
            return Ok(CoverageCacheLookup::Hit(Arc::clone(words)));
        }
        self.coverage_misses = self.coverage_misses.saturating_add(1);
        let admit_after_compute = self.seen_once.remove(&key).is_some();
        if !admit_after_compute {
            self.seen_once.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_piece_language_admission_storage_unavailable",
                )
            })?;
            self.seen_once.insert(key, ());
        }
        Ok(CoverageCacheLookup::Miss {
            admit_after_compute,
        })
    }

    pub fn insert_coverage(
        &mut self,
        language_id: u32,
        pattern_index_id: u32,
        local_words: &[u64],
    ) -> Result<(), WasmExactSearchError> {
        let key = CoverageCacheKey {
            language_id,
            pattern_index_id,
        };
        self.coverage.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_coverage_cache_unavailable")
        })?;
        let mut words = Vec::new();
        words.try_reserve_exact(local_words.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_coverage_words_unavailable")
        })?;
        words.extend_from_slice(local_words);
        self.coverage.insert(key, words.into());
        Ok(())
    }

    pub const fn coverage_hits(&self) -> usize {
        self.coverage_hits
    }

    pub const fn coverage_misses(&self) -> usize {
        self.coverage_misses
    }

    pub fn node(&self, reference: u32) -> Option<PieceLanguageNodeView> {
        self.nodes
            .get(reference as usize)
            .map(|node| PieceLanguageNodeView {
                accepting: node.accepting,
                remaining_depth: node.remaining_depth,
            })
    }

    pub fn edges_for_piece(&self, reference: u32, piece_code: u8) -> Option<&[CanonicalPieceEdge]> {
        let piece_index = usize::from(piece_code.checked_sub(1)?);
        if piece_index >= 7 {
            return None;
        }
        let node = self.nodes.get(reference as usize).copied()?;
        let edges = self.node_edges(node);
        if edges.len() > COMPACT_PIECE_RANGE_EDGE_LIMIT {
            let relative_start = edges.partition_point(|edge| edge.piece_code() < piece_code);
            let relative_end = edges.partition_point(|edge| edge.piece_code() <= piece_code);
            return edges.get(relative_start..relative_end);
        }
        let range_ends = self.piece_range_ends.get(reference as usize)?;
        let relative_start = if piece_index == 0 {
            0
        } else {
            usize::from(range_ends[piece_index - 1])
        };
        let relative_end = usize::from(range_ends[piece_index]);
        let edge_start = node.edge_start as usize;
        self.edges
            .get(edge_start + relative_start..edge_start + relative_end)
    }

    pub fn node_edge_count(&self, reference: u32) -> Option<usize> {
        self.nodes
            .get(reference as usize)
            .map(|node| usize::from(node.edge_count))
    }

    /// Direct immutable access for a known-source membership consumer. This
    /// reads the existing exact suffix graph without constructing a union
    /// automaton or allocating another edge representation.
    pub fn edge_at(&self, reference: u32, index: usize) -> Option<CanonicalPieceEdge> {
        let node = self.nodes.get(reference as usize).copied()?;
        self.node_edges(node).get(index).copied()
    }

    pub fn clear_coverage_caches(&mut self) {
        self.coverage.clear();
        self.seen_once.clear();
    }

    pub fn retained_bytes(&self) -> usize {
        let base = self.nodes.capacity() * core::mem::size_of::<CanonicalPieceNode>()
            + self.edges.capacity() * core::mem::size_of::<CanonicalPieceEdge>()
            + self.piece_range_ends.capacity() * core::mem::size_of::<[u8; 7]>()
            + self.bucket_heads.capacity() * core::mem::size_of::<u32>()
            + self.graph_classes.capacity() * core::mem::size_of::<u32>()
            + self.edge_scratch.capacity() * core::mem::size_of::<CanonicalPieceEdge>()
            + self.coverage.capacity()
                * (core::mem::size_of::<CoverageCacheKey>() + core::mem::size_of::<Arc<[u64]>>())
            + self.seen_once.capacity() * core::mem::size_of::<CoverageCacheKey>()
            + self
                .coverage
                .values()
                .map(|words| words.len() * core::mem::size_of::<u64>())
                .sum::<usize>();
        // The zero-sized identity has only the two Arc reference counters.
        // Bound membership consumers borrow this allocation; count it once
        // with the live arena, not once for every cloned Arc handle.
        #[cfg(feature = "placement-dependency-pc")]
        let base = base + 2 * core::mem::size_of::<usize>();
        #[cfg(any(
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        let base = base + self.prefix_frontier.retained_bytes();
        base
    }

    pub fn local_live_bytes(&self) -> usize {
        self.nodes.len() * core::mem::size_of::<CanonicalPieceNode>()
            + self.edges.len() * core::mem::size_of::<CanonicalPieceEdge>()
            + self.piece_range_ends.len() * core::mem::size_of::<[u8; 7]>()
            + self.coverage.len()
                * (core::mem::size_of::<CoverageCacheKey>() + core::mem::size_of::<Arc<[u64]>>())
            + self.seen_once.len() * core::mem::size_of::<CoverageCacheKey>()
    }

    pub fn clear_retain_capacity(&mut self) {
        #[cfg(feature = "placement-dependency-pc")]
        {
            self.arena_identity = Arc::new(());
        }
        self.nodes.clear();
        self.edges.clear();
        self.piece_range_ends.clear();
        self.bucket_heads.fill(EMPTY_REFERENCE);
        self.graph_classes.fill(EMPTY_REFERENCE);
        self.edge_scratch.clear();
        self.coverage.clear();
        self.seen_once.clear();
        self.interning_disabled = false;
    }

    fn remaining_depth(&self, accepting: bool) -> Result<u8, WasmExactSearchError> {
        if accepting {
            if !self.edge_scratch.is_empty() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_piece_language_accepting_node_has_edges",
                ));
            }
            return Ok(0);
        }
        let Some(first) = self.edge_scratch.first() else {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_live_node_has_no_edges",
            ));
        };
        let child_depth = self.nodes[first.child() as usize].remaining_depth;
        if self
            .edge_scratch
            .iter()
            .any(|edge| self.nodes[edge.child() as usize].remaining_depth != child_depth)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_depth_mismatch",
            ));
        }
        child_depth
            .checked_add(1)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_depth_overflow",
            ))
    }

    fn intern_node(
        &mut self,
        accepting: bool,
        remaining_depth: u8,
    ) -> Result<u32, WasmExactSearchError> {
        let hash = language_node_hash(accepting, remaining_depth, &self.edge_scratch);
        if !self.interning_disabled {
            let bucket = hash as usize & (self.bucket_heads.len() - 1);
            let mut reference = self.bucket_heads[bucket];
            while reference != EMPTY_REFERENCE {
                let node = self.nodes[reference as usize];
                if node.accepting == accepting
                    && node.remaining_depth == remaining_depth
                    && self.node_edges(node) == self.edge_scratch.as_slice()
                {
                    return Ok(reference);
                }
                reference = node.next_same_bucket;
            }
        }

        let reference = u32::try_from(self.nodes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_node_index_overflow")
        })?;
        let edge_start = u32::try_from(self.edges.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_edge_index_overflow")
        })?;
        let edge_count = u16::try_from(self.edge_scratch.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_edge_count_overflow")
        })?;
        self.nodes.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_node_storage_unavailable")
        })?;
        self.piece_range_ends.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_node_storage_unavailable")
        })?;
        self.edges
            .try_reserve_exact(self.edge_scratch.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_piece_language_edge_storage_unavailable")
            })?;
        let next_same_bucket = if self.interning_disabled {
            EMPTY_REFERENCE
        } else {
            self.bucket_heads[hash as usize & (self.bucket_heads.len() - 1)]
        };
        let piece_range_ends = if self.edge_scratch.len() <= COMPACT_PIECE_RANGE_EDGE_LIMIT {
            compact_piece_range_ends(&self.edge_scratch)
        } else {
            [0; 7]
        };
        self.edges.extend_from_slice(&self.edge_scratch);
        self.nodes.push(CanonicalPieceNode {
            edge_start,
            next_same_bucket,
            edge_count,
            remaining_depth,
            accepting,
        });
        self.piece_range_ends.push(piece_range_ends);
        if !self.interning_disabled {
            let bucket = hash as usize & (self.bucket_heads.len() - 1);
            self.bucket_heads[bucket] = reference;
            self.grow_buckets_if_needed();
        }
        Ok(reference)
    }

    fn node_edges(&self, node: CanonicalPieceNode) -> &[CanonicalPieceEdge] {
        let start = node.edge_start as usize;
        &self.edges[start..start + node.edge_count as usize]
    }

    fn ensure_buckets(&mut self) {
        if !self.bucket_heads.is_empty() || self.interning_disabled {
            return;
        }
        if self
            .bucket_heads
            .try_reserve_exact(INITIAL_BUCKET_COUNT)
            .is_err()
        {
            self.interning_disabled = true;
            return;
        }
        self.bucket_heads
            .resize(INITIAL_BUCKET_COUNT, EMPTY_REFERENCE);
    }

    fn grow_buckets_if_needed(&mut self) {
        if self.interning_disabled
            || self.nodes.len().saturating_mul(4) < self.bucket_heads.len().saturating_mul(3)
        {
            return;
        }
        let Some(capacity) = self.bucket_heads.len().checked_mul(2) else {
            self.interning_disabled = true;
            return;
        };
        let mut replacement = Vec::new();
        if replacement.try_reserve_exact(capacity).is_err() {
            self.interning_disabled = true;
            return;
        }
        replacement.resize(capacity, EMPTY_REFERENCE);
        for reference in 0..self.nodes.len() {
            let node = self.nodes[reference];
            let hash =
                language_node_hash(node.accepting, node.remaining_depth, self.node_edges(node));
            let bucket = hash as usize & (capacity - 1);
            self.nodes[reference].next_same_bucket = replacement[bucket];
            replacement[bucket] = reference as u32;
        }
        self.bucket_heads = replacement;
    }
}

#[cfg(any(test, feature = "local-search-ab"))]
fn poset_predecessor_closure(predecessors: &[u16]) -> Option<Vec<u16>> {
    let count = predecessors.len();
    if count > MAX_POSET_OPERATIONS {
        return None;
    }
    let all = (1_u16 << count) - 1;
    if predecessors
        .iter()
        .enumerate()
        .any(|(index, &mask)| mask & !all != 0 || mask & (1_u16 << index) != 0)
    {
        return None;
    }
    let mut closure = Vec::new();
    closure.try_reserve_exact(count).ok()?;
    closure.extend_from_slice(predecessors);
    for via in 0..count {
        let ancestors = closure[via];
        for index in 0..count {
            if closure[index] & (1_u16 << via) != 0 {
                closure[index] |= ancestors;
            }
        }
    }
    if closure
        .iter()
        .enumerate()
        .any(|(index, &mask)| mask & (1_u16 << index) != 0)
    {
        return None;
    }
    Some(closure)
}

#[cfg(any(test, feature = "local-search-ab"))]
fn poset_successors(predecessors: &[u16]) -> [u16; MAX_POSET_OPERATIONS] {
    let mut successors = [0_u16; MAX_POSET_OPERATIONS];
    for (index, &predecessor_mask) in predecessors.iter().enumerate() {
        let mut remaining = predecessor_mask;
        while remaining != 0 {
            let before = remaining.trailing_zeros() as usize;
            remaining &= remaining - 1;
            successors[before] |= 1_u16 << index;
        }
    }
    successors
}

#[cfg(any(test, feature = "local-search-ab"))]
fn poset_operation_classes(
    pieces: &[PieceKind],
    predecessors: &[u16],
) -> Option<(Vec<PosetOperationClass>, u32)> {
    let successors = poset_successors(predecessors);
    let mut classes = Vec::<PosetOperationClass>::new();
    classes.try_reserve_exact(pieces.len()).ok()?;
    for (index, &piece) in pieces.iter().enumerate() {
        if let Some(class) = classes.iter_mut().find(|class| {
            let representative = class.members.trailing_zeros() as usize;
            class.piece_code == piece_code(piece)
                && predecessors[representative] == predecessors[index]
                && successors[representative] == successors[index]
        }) {
            class.members |= 1_u16 << index;
            class.count += 1;
        } else {
            classes.push(PosetOperationClass {
                members: 1_u16 << index,
                piece_code: piece_code(piece),
                count: 1,
                predecessors: 0,
                stride: 0,
            });
        }
    }
    let mut stride = 1_u32;
    for index in 0..classes.len() {
        let representative = classes[index].members.trailing_zeros() as usize;
        let mut class_predecessors = 0_u16;
        for (before, class) in classes.iter().enumerate() {
            let overlap = predecessors[representative] & class.members;
            // Exact twins are all predecessors or none. Never quotient a
            // partial relation between two count coordinates.
            if overlap != 0 {
                if overlap != class.members {
                    return None;
                }
                class_predecessors |= 1_u16 << before;
            }
        }
        classes[index].predecessors = class_predecessors;
        classes[index].stride = stride;
        stride = stride.checked_mul(u32::from(classes[index].count) + 1)?;
    }
    Some((classes, stride - 1))
}

#[cfg(any(test, feature = "local-search-ab"))]
fn poset_residual_counts(
    classes: &[PosetOperationClass],
    state: u32,
) -> ([u8; MAX_POSET_OPERATIONS], u16, u8) {
    let mut remaining = [0_u8; MAX_POSET_OPERATIONS];
    let mut unfinished = 0_u16;
    let mut depth = 0_u8;
    for (index, class) in classes.iter().enumerate() {
        let count = ((state / class.stride) % (u32::from(class.count) + 1)) as u8;
        remaining[index] = count;
        depth += count;
        if count != 0 {
            unfinished |= 1_u16 << index;
        }
    }
    (remaining, unfinished, depth)
}

#[cfg(any(test, feature = "local-search-ab"))]
fn admitted_poset_residual_states(
    classes: &[PosetOperationClass],
    initial: u32,
    budget: usize,
) -> Option<Vec<u32>> {
    if budget == 0 {
        return None;
    }
    let mut states = Vec::new();
    let mut seen = std::collections::HashSet::new();
    states.try_reserve(1).ok()?;
    seen.try_reserve(1).ok()?;
    states.push(initial);
    seen.insert(initial);
    let mut cursor = 0;
    while cursor < states.len() {
        let state = states[cursor];
        cursor += 1;
        let (remaining, unfinished, _) = poset_residual_counts(classes, state);
        for (index, class) in classes.iter().enumerate() {
            if remaining[index] == 0 || class.predecessors & unfinished != 0 {
                continue;
            }
            let child = state - class.stride;
            if seen.contains(&child) {
                continue;
            }
            if states.len() == budget {
                return None;
            }
            states.try_reserve(1).ok()?;
            seen.try_reserve(1).ok()?;
            seen.insert(child);
            states.push(child);
        }
    }
    // A placement decrements one mixed-radix coordinate, so child keys are
    // strictly smaller. This is a topological order without operation edges.
    states.sort_unstable();
    Some(states)
}

/// Count queues for a certified poset with at most seven distinct piece kinds,
/// no hold, and exactly `k = pieces.len()` draws from a fresh standard bag.
/// Every target piece must occupy the first k positions; unrelated pieces may
/// not occur before completion. The unconditional denominator is 7Pk, not k!.
/// A full seven-piece bag gives `linear_extensions / 7!`; ignored trailing
/// draws give the equivalent `linear_extensions * (7-k)! / 7!`.
///
/// Disconnected components combine by a multinomial shuffle; ordinal-sum
/// components multiply. Only irreducible suborders use memoized ideal states.
/// No source permutations, language nodes, field states or movement searches
/// are constructed. Duplicate kinds, malformed orders and cycles return None;
/// labeled extension counts must never be divided by repeated-kind factorials.
#[cfg(any(test, feature = "local-search-ab"))]
pub fn count_distinct_no_hold_one_bag_poset(
    pieces: &[PieceKind],
    predecessors: &[u16],
) -> Option<DistinctNoHoldOneBagCount> {
    if pieces.len() != predecessors.len() || pieces.len() > 7 {
        return None;
    }
    let mut kinds = 0_u8;
    for &piece in pieces {
        let bit = 1_u8 << (piece_code(piece) - 1);
        if kinds & bit != 0 {
            return None;
        }
        kinds |= bit;
    }
    let closure = poset_predecessor_closure(predecessors)?;
    let successors = poset_successors(&closure);
    let mut comparable = [0_u16; MAX_POSET_OPERATIONS];
    for index in 0..pieces.len() {
        comparable[index] = closure[index] | successors[index];
    }
    let mut counter = PosetExtensionCounter {
        predecessors: &closure,
        comparable,
        memo: [None; 1 << 7],
        decomposed_subproblems: 0,
        ideal_states: 0,
    };
    let accepted_queue_count = counter.count((1_u16 << pieces.len()) - 1)?;
    let total_queue_count = (0..pieces.len()).try_fold(1_u128, |product, index| {
        product.checked_mul((7 - index) as u128)
    })?;
    Some(DistinctNoHoldOneBagCount {
        accepted_queue_count,
        total_queue_count,
        decomposed_subproblems: counter.decomposed_subproblems,
        ideal_states: counter.ideal_states,
    })
}

#[cfg(any(test, feature = "local-search-ab"))]
struct PosetExtensionCounter<'a> {
    predecessors: &'a [u16],
    comparable: [u16; MAX_POSET_OPERATIONS],
    memo: [Option<u128>; 1 << 7],
    decomposed_subproblems: usize,
    ideal_states: usize,
}

#[cfg(any(test, feature = "local-search-ab"))]
impl PosetExtensionCounter<'_> {
    fn count(&mut self, active: u16) -> Option<u128> {
        if let Some(value) = self.memo[usize::from(active)] {
            return Some(value);
        }
        let size = active.count_ones() as usize;
        if size <= 1 {
            self.memo[usize::from(active)] = Some(1);
            return Some(1);
        }
        let (parallel, parallel_count) = self.components(active, false);
        let result = if parallel_count > 1 {
            self.decomposed_subproblems += 1;
            let mut product = 1_u128;
            let mut preceding_size = 0;
            for &component in &parallel[..parallel_count] {
                let component_size = component.count_ones() as usize;
                product = product.checked_mul(self.count(component)?)?;
                product = product.checked_mul(small_binomial(
                    preceding_size + component_size,
                    component_size,
                )?)?;
                preceding_size += component_size;
            }
            product
        } else {
            let (series, series_count) = self.components(active, true);
            if series_count > 1 {
                self.decomposed_subproblems += 1;
                let mut product = 1_u128;
                for &component in &series[..series_count] {
                    product = product.checked_mul(self.count(component)?)?;
                }
                product
            } else {
                self.ideal_states += 1;
                let mut sum = 0_u128;
                let mut choices = active;
                while choices != 0 {
                    let index = choices.trailing_zeros() as usize;
                    let bit = 1_u16 << index;
                    choices &= choices - 1;
                    if self.predecessors[index] & active == 0 {
                        sum = sum.checked_add(self.count(active & !bit)?)?;
                    }
                }
                sum
            }
        };
        self.memo[usize::from(active)] = Some(result);
        Some(result)
    }

    fn components(
        &self,
        active: u16,
        incomparability: bool,
    ) -> ([u16; MAX_POSET_OPERATIONS], usize) {
        let mut components = [0_u16; MAX_POSET_OPERATIONS];
        let mut count = 0;
        let mut remaining = active;
        while remaining != 0 {
            let mut frontier = 1_u16 << remaining.trailing_zeros();
            let mut component = 0;
            while frontier != 0 {
                let index = frontier.trailing_zeros() as usize;
                let bit = 1_u16 << index;
                frontier &= frontier - 1;
                component |= bit;
                let neighbors = if incomparability {
                    !self.comparable[index] & !bit
                } else {
                    self.comparable[index]
                };
                frontier |= neighbors & active & !component;
            }
            components[count] = component;
            count += 1;
            remaining &= !component;
        }
        (components, count)
    }
}

#[cfg(any(test, feature = "local-search-ab"))]
fn small_binomial(n: usize, k: usize) -> Option<u128> {
    let k = k.min(n.checked_sub(k)?);
    (1..=k).try_fold(1_u128, |value, index| {
        Some(value.checked_mul((n - k + index) as u128)? / index as u128)
    })
}

fn compact_piece_range_ends(edges: &[CanonicalPieceEdge]) -> [u8; 7] {
    debug_assert!(edges.len() <= COMPACT_PIECE_RANGE_EDGE_LIMIT);
    let mut range_ends = [0_u8; 7];
    let mut cursor = 0usize;
    for piece_code in 1..=7 {
        while cursor < edges.len() && edges[cursor].piece_code() <= piece_code {
            cursor += 1;
        }
        range_ends[piece_code as usize - 1] =
            u8::try_from(cursor).expect("compact piece ranges fit in u8");
    }
    range_ends
}

fn language_node_hash(accepting: bool, remaining_depth: u8, edges: &[CanonicalPieceEdge]) -> u64 {
    let mut hash = mix_digest(0, u64::from(accepting));
    hash = mix_digest(hash, u64::from(remaining_depth));
    hash = mix_digest(hash, edges.len() as u64);
    for edge in edges {
        hash = mix_digest(hash, u64::from(edge.piece_code()));
        hash = mix_digest(hash, u64::from(edge.child()));
    }
    hash
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

    use super::{
        count_distinct_no_hold_one_bag_poset, CanonicalPieceEdge, ConditionalLanguageCompilation,
        ConditionalTransitionProof, PieceOrderLanguageCache,
    };
    use crate::backend::wasm_cpu::buildup::{BuildOrderGraph, BuildOrderNodeSpec};
    use crate::backend::wasm_cpu::queue_observation_policy::{
        ObservationPieceLanguage, RootedPieceLanguageUnion,
    };

    fn branching_operation_graph(accepting: bool) -> BuildOrderGraph {
        let specs = [
            (0, 4, 0, false),
            (4, 2, 1, false),
            (6, 1, 1, false),
            (7, 1, 1, false),
            (8, 0, 2, accepting),
            (8, 0, 2, accepting),
            (8, 0, 2, false),
        ]
        .into_iter()
        .map(
            |(edge_start, edge_count, depth, accepting)| BuildOrderNodeSpec {
                edge_start,
                edge_count,
                depth,
                accepting,
            },
        )
        .collect();
        let edges = [
            (1, 0, PieceKind::I),
            (2, 1, PieceKind::I),
            (2, 6, PieceKind::I),
            (3, 2, PieceKind::T),
            (4, 3, PieceKind::O),
            (5, 4, PieceKind::T),
            (4, 3, PieceKind::O),
            (6, 5, PieceKind::Z),
        ]
        .into_iter()
        .map(|(to, operation, piece)| {
            BuildOrderGraph::edge(to, operation, piece, RotationState::Zero, 0, 0, 0)
        })
        .collect();
        BuildOrderGraph::from_topological_parts(specs, edges, 0, 0, false).unwrap()
    }

    fn words(cache: &PieceOrderLanguageCache, root: u32) -> BTreeSet<Vec<u8>> {
        fn visit(
            cache: &PieceOrderLanguageCache,
            node: u32,
            prefix: &mut Vec<u8>,
            output: &mut BTreeSet<Vec<u8>>,
        ) {
            if cache.node(node).unwrap().accepting {
                output.insert(prefix.clone());
            }
            for kind in 1..=7 {
                for edge in cache.edges_for_piece(node, kind).unwrap() {
                    prefix.push(kind);
                    visit(cache, edge.child(), prefix, output);
                    prefix.pop();
                }
            }
        }
        let mut output = BTreeSet::new();
        visit(cache, root, &mut Vec::new(), &mut output);
        output
    }

    fn prefix_path_masks<const N: usize>(
        paths: &[[usize; N]],
    ) -> (
        [[u64; super::super::symbolic_prefix_domain::MAX_PREFIX_WORDS]; N],
        [u64; super::super::symbolic_prefix_domain::MAX_PREFIX_WORDS],
    ) {
        use super::super::symbolic_prefix_domain::MAX_PREFIX_WORDS;
        let mut parents = [[0; MAX_PREFIX_WORDS]; N];
        let mut active = [0; MAX_PREFIX_WORDS];
        active[0] = 1;
        for path in paths {
            let mut subset = 0;
            for &operation in path {
                assert_eq!(subset & (1 << operation), 0);
                parents[operation][subset / 64] |= 1 << (subset % 64);
                subset |= 1 << operation;
                active[subset / 64] |= 1 << (subset % 64);
            }
            assert_eq!(subset, (1 << N) - 1);
        }
        (parents, active)
    }

    #[test]
    fn closed_prefix_frontier_unions_incomparable_same_kind_successors_and_falls_back() {
        use PieceKind::{I, O};

        let pieces = [I, I, O];
        // Both I copies are admissible first, with different suffixes. A
        // second concrete IOI trail makes the union overlap as well.
        let (parents, active) = prefix_path_masks(&[[0, 2, 1], [1, 0, 2], [1, 2, 0]]);
        let expected = BTreeSet::from([vec![1, 1, 2], vec![1, 2, 1]]);
        for (budget, expected_children, expected_frontiers) in [(0, 2, 0), (256, 1, 1), (1, 2, 0)] {
            let mut cache = PieceOrderLanguageCache::default();
            let root = cache
                .compile_closed_prefix_language_with_mode(
                    &pieces,
                    &parents,
                    &active,
                    &mut 0,
                    Some(budget),
                    None,
                )
                .unwrap()
                .unwrap();
            assert_eq!(words(&cache, root), expected);
            assert_eq!(cache.node(root).unwrap().remaining_depth, 3);
            assert_eq!(
                cache.edges_for_piece(root, 1).unwrap().len(),
                expected_children
            );
            assert_eq!(cache.frontier_compilations(), expected_frontiers);
        }
    }

    #[cfg(feature = "placement-dependency-pc")]
    #[test]
    fn product_prefix_language_preserves_overlapping_same_kind_suffixes() {
        use PieceKind::{I, O};

        let (parents, active) = prefix_path_masks(&[[0, 2, 1], [1, 0, 2], [1, 2, 0]]);
        let mut cache = PieceOrderLanguageCache::default();
        let root = cache
            .compile_product_closed_prefix_language(&[I, I, O], &parents, &active, &mut 0)
            .unwrap()
            .unwrap();
        assert_eq!(
            words(&cache, root),
            BTreeSet::from([vec![1, 1, 2], vec![1, 2, 1]])
        );
        assert_eq!(cache.frontier_compilations(), 0);
    }

    #[test]
    fn closed_prefix_frontier_retains_all_sixteen_variants_across_words() {
        use PieceKind::{I, J, L, O, S, T};

        // Interleave the pairs across the operation-mask word boundary.
        let pieces = [I, J, L, O, T, S, I, J, L, O];
        let mut paths = Vec::new();
        for chosen in 0..16 {
            let mut path = [0; 10];
            for pair in 0..4 {
                let use_second = chosen & (1 << pair) != 0;
                path[pair] = if use_second { 6 + pair } else { pair };
                path[6 + pair] = if use_second { pair } else { 6 + pair };
            }
            // A distinction that a kind-count-only merger cannot recover.
            (path[4], path[5]) = if chosen == 15 { (5, 4) } else { (4, 5) };
            paths.push(path);
        }
        let (parents, active) = prefix_path_masks(&paths);
        let expected: BTreeSet<_> = paths
            .iter()
            .map(|path| {
                path.iter()
                    .map(|&id| super::piece_code(pieces[id]))
                    .collect()
            })
            .collect();
        assert_eq!(expected.len(), 2);
        let mut cache = PieceOrderLanguageCache::default();
        for budget in [0, 256, 2] {
            let root = cache
                .compile_closed_prefix_language_with_mode(
                    &pieces,
                    &parents,
                    &active,
                    &mut 0,
                    Some(budget),
                    None,
                )
                .unwrap()
                .unwrap();
            assert_eq!(words(&cache, root), expected);
        }
        assert_eq!(cache.frontier_compilations(), 1);
    }

    #[test]
    fn closed_prefix_frontier_falls_back_for_unsupported_multiplicities() {
        use PieceKind::{I, O};

        for pieces in [[I, O, PieceKind::L, PieceKind::T], [I, I, I, O]] {
            let (parents, active) = prefix_path_masks(&[[0, 1, 2, 3]]);
            let mut cache = PieceOrderLanguageCache::default();
            let root = cache
                .compile_closed_prefix_language_with_mode(
                    &pieces,
                    &parents,
                    &active,
                    &mut 0,
                    Some(256),
                    None,
                )
                .unwrap()
                .unwrap();
            assert_eq!(
                words(&cache, root),
                BTreeSet::from([pieces
                    .into_iter()
                    .map(super::piece_code)
                    .collect::<Vec<_>>()])
            );
            assert_eq!(cache.frontier_compilations(), 0);
        }
    }

    fn explicit_poset_words(pieces: &[PieceKind], predecessors: &[u16]) -> BTreeSet<Vec<u8>> {
        fn visit(
            pieces: &[PieceKind],
            predecessors: &[u16],
            placed: u16,
            word: &mut Vec<u8>,
            output: &mut BTreeSet<Vec<u8>>,
        ) {
            if word.len() == pieces.len() {
                output.insert(word.clone());
                return;
            }
            for (index, &piece) in pieces.iter().enumerate() {
                let bit = 1_u16 << index;
                if placed & bit == 0 && predecessors[index] & !placed == 0 {
                    word.push(super::piece_code(piece));
                    visit(pieces, predecessors, placed | bit, word, output);
                    word.pop();
                }
            }
        }
        let mut output = BTreeSet::new();
        visit(pieces, predecessors, 0, &mut Vec::new(), &mut output);
        output
    }

    #[test]
    fn conditional_language_preserves_or_support_and_visits_each_residual_once() {
        use ConditionalLanguageCompilation::Certified;
        use ConditionalTransitionProof::{Allowed, Forbidden};
        use PieceKind::{I, O, T};

        let mut cache = PieceOrderLanguageCache::default();
        let mut checked = BTreeSet::new();
        let compiled = cache
            .compile_conditional_language(&[I, O, T], 64, 256, |executed, operation| {
                assert!(checked.insert((executed, operation)));
                if operation == 0 && executed & 0b110 == 0 {
                    Forbidden
                } else {
                    Allowed
                }
            })
            .unwrap();
        let Certified(result) = compiled else {
            panic!("complete Boolean proof was refused: {compiled:?}");
        };
        assert_eq!(result.residual_states, 7);
        assert_eq!(result.checked_transitions, 10);
        assert_eq!(result.checked_transitions, checked.len());
        let expected: BTreeSet<Vec<u8>> = [[O, I, T], [O, T, I], [T, I, O], [T, O, I]]
            .into_iter()
            .map(|word| word.into_iter().map(super::piece_code).collect())
            .collect();
        assert_eq!(words(&cache, result.root.unwrap()), expected);
    }

    #[test]
    fn conditional_language_reuses_suffixes_without_merging_distinct_same_kind_operations() {
        use ConditionalLanguageCompilation::Certified;
        use ConditionalTransitionProof::{Allowed, Forbidden};
        use PieceKind::{I, O, T};

        let mut cache = PieceOrderLanguageCache::default();
        let pieces = [I, I, T, O];
        let predecessors = [0, 0, 0b0001, 0b0010];
        let expected = explicit_poset_words(&pieces, &predecessors);
        let Certified(result) = cache
            .compile_conditional_language(&pieces, 64, 256, |executed, operation| {
                if predecessors[operation] & !executed == 0 {
                    Allowed
                } else {
                    Forbidden
                }
            })
            .unwrap()
        else {
            panic!("complete Boolean proof was refused");
        };
        let root = result.root.unwrap();
        assert_eq!(words(&cache, root), expected);
        assert_eq!(
            cache
                .edges_for_piece(root, super::piece_code(I))
                .unwrap()
                .len(),
            2
        );
        let poset = cache
            .compile_poset_language(&pieces, &predecessors, 64)
            .unwrap()
            .unwrap();
        assert_eq!(poset.root, root);
    }

    #[test]
    fn conditional_language_unknown_or_budget_refusal_does_not_intern() {
        use ConditionalLanguageCompilation::{BudgetExceeded, Unknown};
        use ConditionalTransitionProof::Allowed;
        use PieceKind::{I, O, T};

        let mut cache = PieceOrderLanguageCache::default();
        let prior = cache
            .compile_poset_language(&[I], &[0], 2)
            .unwrap()
            .unwrap();
        let before = (
            cache.node_count(),
            cache.edge_count(),
            words(&cache, prior.root),
        );
        let pieces = [I, O, T];
        let mut checked = 0;
        assert_eq!(
            cache
                .compile_conditional_language(&pieces, 64, 256, |executed, operation| {
                    checked += 1;
                    if executed == 0b110 && operation == 0 {
                        ConditionalTransitionProof::Unknown
                    } else {
                        Allowed
                    }
                })
                .unwrap(),
            Unknown
        );
        assert!(checked > 3);
        assert_eq!(
            cache
                .compile_conditional_language(&pieces, 1, 256, |_, _| Allowed)
                .unwrap(),
            BudgetExceeded
        );
        let mut checked = 0;
        assert_eq!(
            cache
                .compile_conditional_language(&pieces, 64, 2, |_, _| {
                    checked += 1;
                    Allowed
                })
                .unwrap(),
            BudgetExceeded
        );
        assert_eq!(checked, 2);
        assert_eq!(
            cache
                .compile_conditional_language(&[I; 15], usize::MAX, usize::MAX, |_, _| Allowed)
                .unwrap(),
            BudgetExceeded
        );
        assert_eq!(
            (
                cache.node_count(),
                cache.edge_count(),
                words(&cache, prior.root)
            ),
            before
        );
    }

    #[test]
    fn conditional_language_exact_empty_and_dead_branches_are_not_unknown() {
        use ConditionalLanguageCompilation::Certified;
        use ConditionalTransitionProof::{Allowed, Forbidden};
        use PieceKind::{I, O};

        let mut cache = PieceOrderLanguageCache::default();
        let Certified(dead) = cache
            .compile_conditional_language(&[I, O], 64, 256, |executed, _| {
                if executed == 0 {
                    Allowed
                } else {
                    Forbidden
                }
            })
            .unwrap()
        else {
            panic!("exact dead relation was refused");
        };
        assert_eq!(dead.root, None);
        assert_eq!(dead.residual_states, 3);
        assert_eq!(cache.node_count(), 0);
        let Certified(one) = cache
            .compile_conditional_language(&[I, O], 64, 256, |executed, _| {
                if executed == 0b01 {
                    Forbidden
                } else {
                    Allowed
                }
            })
            .unwrap()
        else {
            panic!("exact partly dead relation was refused");
        };
        assert_eq!(
            words(&cache, one.root.unwrap()),
            BTreeSet::from([vec![super::piece_code(O), super::piece_code(I)]])
        );
    }

    #[test]
    fn conditional_language_matches_explicit_words_for_every_three_operation_relation() {
        use ConditionalLanguageCompilation::Certified;
        use ConditionalTransitionProof::{Allowed, Forbidden};
        use PieceKind::{I, O, T};

        const PERMUTATIONS: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let mut transition = [[0; 3]; 8];
        let mut bits = 0;
        for (executed, row) in transition.iter_mut().enumerate() {
            for (operation, value) in row.iter_mut().enumerate() {
                if executed & (1 << operation) == 0 {
                    *value = 1_u16 << bits;
                    bits += 1;
                }
            }
        }
        assert_eq!(bits, 12);
        let mut cache = PieceOrderLanguageCache::default();
        for pieces in [[I, O, T], [I, I, O]] {
            for relation in 0..(1_u16 << bits) {
                let expected: BTreeSet<_> = PERMUTATIONS
                    .iter()
                    .filter(|order| {
                        let mut executed = 0;
                        order.iter().all(|&operation| {
                            let allowed = relation & transition[executed][operation] != 0;
                            executed |= 1 << operation;
                            allowed
                        })
                    })
                    .map(|order| {
                        order
                            .iter()
                            .map(|&operation| super::piece_code(pieces[operation]))
                            .collect()
                    })
                    .collect();
                let Certified(result) = cache
                    .compile_conditional_language(&pieces, 8, 12, |executed, operation| {
                        if relation & transition[usize::from(executed)][operation] != 0 {
                            Allowed
                        } else {
                            Forbidden
                        }
                    })
                    .unwrap()
                else {
                    panic!("three-operation mask relation exceeded its full bound");
                };
                let actual = result
                    .root
                    .map(|root| words(&cache, root))
                    .unwrap_or_default();
                assert_eq!(
                    actual, expected,
                    "relation={relation:012b}, pieces={pieces:?}"
                );
            }
        }
    }

    #[test]
    fn poset_language_matches_every_four_operation_forward_dag_and_repeated_kind_words() {
        let pairs = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];
        for edge_bits in 0..(1_u16 << pairs.len()) {
            let mut predecessors = [0_u16; 4];
            for (edge_index, &(before, after)) in pairs.iter().enumerate() {
                if edge_bits & (1_u16 << edge_index) != 0 {
                    predecessors[after] |= 1_u16 << before;
                }
            }
            for pieces in [
                [PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S],
                [PieceKind::I, PieceKind::O, PieceKind::I, PieceKind::O],
            ] {
                let expected = explicit_poset_words(&pieces, &predecessors);
                let mut cache = PieceOrderLanguageCache::default();
                let compiled = cache
                    .compile_poset_language(&pieces, &predecessors, 16)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    words(&cache, compiled.root),
                    expected,
                    "edge mask {edge_bits:#x}, pieces {pieces:?}"
                );
                assert!(compiled.residual_states <= 16);
                if pieces[0] != pieces[2] {
                    let scalar = count_distinct_no_hold_one_bag_poset(&pieces, &predecessors)
                        .expect("distinct kind arithmetic contract");
                    assert_eq!(scalar.accepted_queue_count, expected.len() as u128);
                    assert_eq!(scalar.total_queue_count, 7 * 6 * 5 * 4);
                }
            }
        }
    }

    #[test]
    fn poset_language_free_multiset_and_twins_use_count_coordinates() {
        let mut cache = PieceOrderLanguageCache::default();
        let repeated = cache
            .compile_poset_language(&[PieceKind::I; 15], &[0; 15], 16)
            .unwrap()
            .unwrap();
        assert_eq!(repeated.operation_classes, 1);
        assert_eq!(repeated.residual_states, 16);
        assert_eq!(words(&cache, repeated.root), BTreeSet::from([vec![1; 15]]));

        let free = cache
            .compile_poset_language(
                &[
                    PieceKind::I,
                    PieceKind::I,
                    PieceKind::O,
                    PieceKind::O,
                    PieceKind::T,
                ],
                &[0; 5],
                18,
            )
            .unwrap()
            .unwrap();
        assert_eq!(free.operation_classes, 3);
        assert_eq!(free.residual_states, 3 * 3 * 2);
        assert_eq!(words(&cache, free.root).len(), 30);

        let twins = cache
            .compile_poset_language(
                &[PieceKind::I, PieceKind::I, PieceKind::O, PieceKind::T],
                &[0, 0, 0b0011, 0b0100],
                5,
            )
            .unwrap()
            .unwrap();
        assert_eq!(twins.operation_classes, 3);
        assert_eq!(twins.residual_states, 5);
        assert_eq!(
            words(&cache, twins.root),
            BTreeSet::from([vec![1, 1, 2, 3]])
        );
    }

    #[test]
    fn poset_language_does_not_merge_same_kind_operations_with_different_successors() {
        // I0 < O, while I1 is free. Three labeled extensions project to two
        // words: IIO has two witnesses, IOI has one. Dividing 3 by 2! is wrong.
        let pieces = [PieceKind::I, PieceKind::I, PieceKind::O];
        let predecessors = [0, 0, 1];
        let mut cache = PieceOrderLanguageCache::default();
        let compiled = cache
            .compile_poset_language(&pieces, &predecessors, 8)
            .unwrap()
            .unwrap();
        assert_eq!(compiled.operation_classes, 3);
        assert_eq!(compiled.residual_states, 6);
        assert_eq!(
            words(&cache, compiled.root),
            BTreeSet::from([vec![1, 1, 2], vec![1, 2, 1]])
        );
        assert!(count_distinct_no_hold_one_bag_poset(&pieces, &predecessors).is_none());
    }

    #[test]
    fn poset_language_budget_and_invalid_input_leave_the_arena_unchanged() {
        let mut cache = PieceOrderLanguageCache::default();
        let terminal = cache.compile_poset_language(&[], &[], 1).unwrap().unwrap();
        let before = (cache.node_count(), cache.edge_count());
        assert!(cache
            .compile_poset_language(
                &[PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S],
                &[0; 4],
                15,
            )
            .unwrap()
            .is_none());
        for (pieces, predecessors, budget) in [
            (vec![PieceKind::I], vec![0], 0),
            (vec![PieceKind::I], vec![], 16),
            (vec![PieceKind::I], vec![1], 16),
            (vec![PieceKind::I], vec![2], 16),
            (vec![PieceKind::I, PieceKind::O], vec![2, 1], 16),
            (vec![PieceKind::I; 16], vec![0; 16], 17),
        ] {
            assert!(cache
                .compile_poset_language(&pieces, &predecessors, budget)
                .unwrap()
                .is_none());
        }
        assert_eq!((cache.node_count(), cache.edge_count()), before);
        assert_eq!(words(&cache, terminal.root), BTreeSet::from([Vec::new()]));
        let valid = cache
            .compile_poset_language(
                &[PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S],
                &[0; 4],
                16,
            )
            .unwrap()
            .unwrap();
        assert_eq!(words(&cache, valid.root).len(), 24);
    }

    #[test]
    fn poset_scalar_count_uses_series_parallel_arithmetic_and_fresh_bag_denominator() {
        let pieces = PieceKind::STANDARD_TETROMINOES;
        let free = count_distinct_no_hold_one_bag_poset(&pieces, &[0; 7]).unwrap();
        assert_eq!(
            (free.accepted_queue_count, free.total_queue_count),
            (5040, 5040)
        );
        assert_eq!(free.ideal_states, 0);
        assert!(free.decomposed_subproblems > 0);

        let chain =
            count_distinct_no_hold_one_bag_poset(&pieces, &[0, 1, 2, 4, 8, 16, 32]).unwrap();
        assert_eq!(
            (chain.accepted_queue_count, chain.total_queue_count),
            (1, 5040)
        );
        assert_eq!(chain.ideal_states, 0);

        // Parallel chains of lengths two and three, plus two free pieces.
        let parallel =
            count_distinct_no_hold_one_bag_poset(&pieces, &[0, 1, 0, 4, 8, 0, 0]).unwrap();
        assert_eq!(parallel.accepted_queue_count, 5040 / (2 * 6));
        assert_eq!(parallel.ideal_states, 0);

        let prefix = count_distinct_no_hold_one_bag_poset(&pieces[..3], &[0, 0, 1]).unwrap();
        assert_eq!(prefix.accepted_queue_count, 3);
        assert_eq!(prefix.total_queue_count, 7 * 6 * 5);

        // The four-element N order is not a series or parallel composition.
        let prime = count_distinct_no_hold_one_bag_poset(&pieces[..4], &[0, 0, 3, 2]).unwrap();
        assert_eq!(prime.accepted_queue_count, 5);
        assert!(prime.ideal_states > 0);
        assert!(count_distinct_no_hold_one_bag_poset(&pieces[..2], &[2, 1]).is_none());
    }

    #[test]
    fn fused_language_preserves_same_kind_branches_dead_edges_and_exact_queue_union() {
        let mut graph = branching_operation_graph(true);
        let expected_live = graph.nodes.iter().map(|node| node.live).collect::<Vec<_>>();
        let operation_edges = (0..graph.nodes.len())
            .map(|node| {
                graph
                    .edges(node)
                    .iter()
                    .map(|edge| edge.canonical_key())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut cache = PieceOrderLanguageCache::default();
        let baseline_root = cache.canonicalize(&graph).unwrap();
        assert_eq!(operation_edges.iter().map(Vec::len).sum::<usize>(), 8);
        let expected_words = BTreeSet::from([vec![1, 2], vec![1, 3]]);
        assert_eq!(words(&cache, baseline_root), expected_words);

        // Fused completion derives liveness; it must not trust stale flags.
        for node in &mut graph.nodes {
            node.live = !node.live;
        }
        let fused_root = cache
            .canonicalize_and_mark_live(&mut graph)
            .unwrap()
            .unwrap();
        assert_eq!(fused_root, baseline_root, "same exact suffix identities");
        assert_eq!(words(&cache, fused_root), expected_words);
        assert_eq!(
            graph.nodes.iter().map(|node| node.live).collect::<Vec<_>>(),
            expected_live
        );
        assert_eq!(
            (0..graph.nodes.len())
                .map(|node| graph
                    .edges(node)
                    .iter()
                    .map(|edge| edge.canonical_key())
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            operation_edges,
            "Boolean language construction must not remove CountAll operation edges",
        );
    }

    #[test]
    fn fused_language_dead_root_cannot_reuse_previous_graph_classes() {
        let mut cache = PieceOrderLanguageCache::default();
        let mut live = branching_operation_graph(true);
        let expected = cache.canonicalize_and_mark_live(&mut live).unwrap();
        assert!(expected.is_some());
        let mut dead = branching_operation_graph(false);
        for node in &mut dead.nodes {
            node.live = true;
        }
        assert_eq!(cache.canonicalize_and_mark_live(&mut dead).unwrap(), None);
        assert!(dead.nodes.iter().all(|node| !node.live));
        assert_eq!(
            cache.canonicalize_and_mark_live(&mut live).unwrap(),
            expected
        );
    }

    #[test]
    fn fused_language_rejects_invalid_root_instead_of_publishing_a_language() {
        let mut graph = branching_operation_graph(true);
        graph.root = graph.nodes.len() as u32;
        assert!(PieceOrderLanguageCache::default()
            .canonicalize_and_mark_live(&mut graph)
            .is_err());
    }

    #[test]
    fn traversed_subset_masks_preserve_exact_same_kind_edge_languages() {
        use PieceKind::{I, O};

        // Independently spell the old concrete graph and the corresponding
        // BFS masks. Distinct I operations leave different exact suffixes;
        // the O prefix is dead. The BFS order is not numeric subset order.
        let specs = [
            (0, 3, 0, false),
            (3, 1, 1, false),
            (4, 1, 1, false),
            (5, 1, 1, false),
            (6, 1, 2, false),
            (7, 0, 2, false),
            (7, 1, 2, false),
            (8, 0, 3, true),
        ]
        .map(
            |(edge_start, edge_count, depth, accepting)| BuildOrderNodeSpec {
                edge_start,
                edge_count,
                depth,
                accepting,
            },
        );
        let edges = [
            (1, 0, I),
            (2, 1, O),
            (3, 2, I),
            (4, 1, O),
            (5, 2, I),
            (6, 0, I),
            (7, 2, I),
            (7, 1, O),
        ]
        .map(|(to, operation, piece)| {
            BuildOrderGraph::edge(to, operation, piece, RotationState::Zero, 0, 0, 0)
        });
        let graph =
            BuildOrderGraph::from_topological_parts(specs.into(), edges.into(), 0, 0, false)
                .unwrap();
        let mut cache = PieceOrderLanguageCache::default();
        let baseline = cache.canonicalize(&graph).unwrap();
        let expected = BTreeSet::from([vec![1, 2, 1], vec![1, 1, 2]]);
        assert_eq!(words(&cache, baseline), expected);

        let subsets = [0, 1, 2, 4, 3, 6, 5, 7];
        let allowed = [7, 2, 4, 1, 4, 0, 2, 0];
        let lookup = [0, 1, 2, 4, 3, 6, 5, 7];
        let before = (cache.node_count(), cache.edge_count());
        let masked = cache
            .compile_traversed_subset_masks(&[I, O, I], &subsets, &allowed, &lookup, true)
            .unwrap();
        assert_eq!(
            masked,
            Some(baseline),
            "reuse the exact existing suffix nodes"
        );
        assert_eq!(words(&cache, masked.unwrap()), expected);
        assert_eq!((cache.node_count(), cache.edge_count()), before);
        assert_eq!(
            cache
                .compile_traversed_subset_masks(&[I, O, I], &subsets, &allowed, &lookup, false)
                .unwrap(),
            None,
            "a reached full subset is not acceptance"
        );
        assert_eq!((cache.node_count(), cache.edge_count()), before);
        assert_eq!(words(&cache, baseline), expected);
    }

    #[test]
    fn traversed_subset_masks_reject_invalid_traversals_before_interning() {
        use PieceKind::{I, O};

        let mut cache = PieceOrderLanguageCache::default();
        let before = (cache.node_count(), cache.edge_count());
        for (subsets, allowed, lookup, accepted) in [
            (&[0, 1, 3][..], &[1, 2, 0][..], [0, 0, 7, 2], true), // stale child ID
            (&[0, 1, 1, 3], &[1, 2, 2, 0], [0, 1, 7, 3], true),   // duplicated subset
            (&[0, 3, 1], &[1, 0, 2], [0, 2, 7, 1], true),         // backward child
            (&[0, 1, 2, 3], &[1, 2, 1, 0], [0, 1, 2, 3], true),   // orphan
            (&[0, 1], &[1, 1], [0, 1, 7, 7], false),              // reused operation
            (&[0], &[0], [0, 0, 0, 0], true),                     // invalid terminal claim
        ] {
            assert!(cache
                .compile_traversed_subset_masks(&[I, O], subsets, allowed, &lookup, accepted)
                .is_err());
            assert_eq!((cache.node_count(), cache.edge_count()), before);
        }
        assert_eq!(
            cache
                .compile_traversed_subset_masks(&[I, O], &[0], &[0], &[0, 0, 0, 0], false)
                .unwrap(),
            None,
            "an exact dead traversal ignores unrelated stale lookup slots"
        );
    }

    #[test]
    fn determinized_union_preserves_all_distinct_piece_branches_once() {
        let mut cache = PieceOrderLanguageCache::default();
        cache.ensure_buckets();
        let terminal = cache.intern_node(true, 0).expect("terminal");

        cache
            .edge_scratch
            .push(CanonicalPieceEdge::new(terminal, 1).expect("I edge"));
        let i_root = cache.intern_node(false, 1).expect("I language");
        cache.edge_scratch.clear();
        cache
            .edge_scratch
            .push(CanonicalPieceEdge::new(terminal, 2).expect("O edge"));
        let o_root = cache.intern_node(false, 1).expect("O language");
        cache.edge_scratch.clear();

        let language =
            RootedPieceLanguageUnion::new(&cache, &[i_root, o_root, i_root]).expect("union");
        let root = language.root();
        assert_eq!(language.edge_count(root), Some(2));
        let i_child = language.edge(root, 0).expect("I union edge");
        let o_child = language.edge(root, 1).expect("O union edge");
        assert_eq!(i_child.0, 1);
        assert_eq!(o_child.0, 2);
        assert_eq!(i_child.1, o_child.1);
        assert!(language
            .node(i_child.1)
            .is_some_and(|node| node.accepting && node.depth == 1));
        assert_eq!(
            language.node(root),
            Some(
                crate::backend::wasm_cpu::queue_observation_policy::ObservationLanguageNode {
                    accepting: false,
                    depth: 0,
                }
            )
        );
    }

    #[test]
    fn determinized_union_preserves_shared_prefix_continuations() {
        let mut cache = PieceOrderLanguageCache::default();
        cache.ensure_buckets();
        let terminal = cache.intern_node(true, 0).expect("terminal");

        cache
            .edge_scratch
            .push(CanonicalPieceEdge::new(terminal, 2).expect("O edge"));
        let o_suffix = cache.intern_node(false, 1).expect("O suffix");
        cache.edge_scratch.clear();
        cache
            .edge_scratch
            .push(CanonicalPieceEdge::new(terminal, 3).expect("T edge"));
        let t_suffix = cache.intern_node(false, 1).expect("T suffix");
        cache.edge_scratch.clear();

        cache
            .edge_scratch
            .push(CanonicalPieceEdge::new(o_suffix, 1).expect("IO root"));
        let io_root = cache.intern_node(false, 2).expect("IO language");
        cache.edge_scratch.clear();
        cache
            .edge_scratch
            .push(CanonicalPieceEdge::new(t_suffix, 1).expect("IT root"));
        let it_root = cache.intern_node(false, 2).expect("IT language");
        cache.edge_scratch.clear();

        let language = RootedPieceLanguageUnion::new(&cache, &[io_root, it_root]).expect("union");
        let root = language.root();
        assert_eq!(language.edge_count(root), Some(1));
        let (piece, shared_prefix) = language.edge(root, 0).expect("shared I edge");
        assert_eq!(piece, 1);
        assert_eq!(language.edge_count(shared_prefix), Some(2));
        assert_eq!(language.edge(shared_prefix, 0).map(|edge| edge.0), Some(2));
        assert_eq!(language.edge(shared_prefix, 1).map(|edge| edge.0), Some(3));
    }

    #[test]
    fn piece_ranges_remain_exact_above_the_compact_edge_limit() {
        let mut cache = PieceOrderLanguageCache::default();
        cache.ensure_buckets();
        for piece_code in 1..=3 {
            for child in 0..128 {
                cache
                    .edge_scratch
                    .push(CanonicalPieceEdge::new(child, piece_code).expect("edge"));
            }
        }

        let root = cache.intern_node(false, 1).expect("wide language node");

        assert_eq!(cache.node_edge_count(root), Some(384));
        for piece_code in 1..=3 {
            let edges = cache
                .edges_for_piece(root, piece_code)
                .expect("piece range");
            assert_eq!(edges.len(), 128);
            assert!(edges.iter().all(|edge| edge.piece_code() == piece_code));
        }
        for piece_code in 4..=7 {
            assert_eq!(
                cache
                    .edges_for_piece(root, piece_code)
                    .expect("empty piece range")
                    .len(),
                0
            );
        }
    }
}
