//! Bounded determinization of a proved concrete-operation language.
//!
//! For any particular kind prefix, consumed counts are fixed. With at most
//! four duplicated kinds and each multiplicity at most two, there are at
//! most sixteen concrete operation sets in the frontier. Neither a count
//! vector by itself nor a hash is an identity for a frontier.

use std::collections::HashMap;

use clearra_core_domain::piece::piece_kind::PieceKind;

use super::{
    piece_code, CanonicalPieceEdge, PieceOrderLanguageCache, WasmExactSearchError, EMPTY_REFERENCE,
};
use crate::backend::wasm_cpu::symbolic_prefix_domain::{MAX_OPERATIONS, MAX_PREFIX_WORDS};

pub(super) const MAX_FRONTIER_STATES: usize = 256;
const MAX_EDGE_INSPECTIONS: usize = 8_192;
const MAX_DUPLICATED_KINDS: usize = 4;
const MAX_VARIANTS: usize = 1 << MAX_DUPLICATED_KINDS;
const NO_CHILD: u16 = u16::MAX;
const INVALID: &str = "wasm_symbolic_prefix_language_bound_inconsistent";

pub(super) fn experiment_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    ))]
    {
        use std::sync::OnceLock;

        // The switch is an experiment axis for one process, never read per edge.
        static ENABLED: OnceLock<Result<bool, &'static str>> = OnceLock::new();
        let enabled =
            *ENABLED.get_or_init(|| match std::env::var("CLEARRA_STRICT_LANGUAGE_FRONTIER") {
                Ok(value) if value == "1" => Ok(true),
                Ok(value) if value == "0" => Ok(false),
                Err(std::env::VarError::NotPresent) => Ok(false),
                _ => Err("wasm_strict_language_frontier_mode_invalid"),
            });
        enabled.map_err(WasmExactSearchError::InvalidProblem)
    }
    #[cfg(not(all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )))]
    Ok(false)
}

#[derive(Clone, Copy, Default)]
struct KindOperations {
    first: u16,
    second: u16,
    variant_bit: usize,
}

struct FrontierLayout {
    kinds: [KindOperations; 7],
    kind_by_operation: [usize; MAX_OPERATIONS],
    subset_toggle: [u16; MAX_VARIANTS],
}

impl FrontierLayout {
    fn new(pieces: &[PieceKind]) -> Option<Self> {
        let mut layout = Self {
            kinds: [KindOperations::default(); 7],
            kind_by_operation: [0; MAX_OPERATIONS],
            subset_toggle: [0; MAX_VARIANTS],
        };
        let mut variants = 1;
        for (operation, &piece) in pieces.iter().enumerate() {
            let kind = usize::from(piece_code(piece) - 1);
            layout.kind_by_operation[operation] = kind;
            let group = &mut layout.kinds[kind];
            let operation_bit = 1_u16 << operation;
            if group.first == 0 {
                group.first = operation_bit;
            } else {
                if group.second != 0 || variants == MAX_VARIANTS {
                    return None;
                }
                group.second = operation_bit;
                group.variant_bit = variants;
                for previous in 0..variants {
                    layout.subset_toggle[variants + previous] =
                        layout.subset_toggle[previous] ^ group.first ^ group.second;
                }
                variants *= 2;
            }
        }
        // Without repeated kinds this cannot remove any nondeterminism.
        (variants > 1).then_some(layout)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Frontier {
    /// Uses the first concrete operation whenever exactly one of a pair was
    /// executed. Thus the base uniquely identifies the consumed kind counts.
    base: u16,
    /// Each bit selects a variant that flips zero or more half-used pairs.
    /// Variants never flip unused or fully consumed pairs.
    members: u16,
}

struct FrontierNode {
    frontier: Frontier,
    children: [u16; 7],
    language_reference: u32,
}

/// Worker-owned scratch. Keys, indexes and references never survive as
/// reusable certificates between candidates or language arena epochs.
#[derive(Default)]
pub(super) struct PrefixFrontierScratch {
    nodes: Vec<FrontierNode>,
    indexes: HashMap<Frontier, u16>,
}

impl PrefixFrontierScratch {
    pub(super) fn retained_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<FrontierNode>()
            + self.indexes.capacity() * core::mem::size_of::<(Frontier, u16)>()
    }

    fn insert(&mut self, frontier: Frontier, state_budget: usize) -> Option<u16> {
        if let Some(&index) = self.indexes.get(&frontier) {
            return Some(index);
        }
        if self.nodes.len() == state_budget {
            return None;
        }
        let index = u16::try_from(self.nodes.len()).ok()?;
        self.nodes.try_reserve(1).ok()?;
        self.indexes.try_reserve(1).ok()?;
        self.nodes.push(FrontierNode {
            frontier,
            children: [NO_CHILD; 7],
            language_reference: EMPTY_REFERENCE,
        });
        self.indexes.insert(frontier, index);
        Some(index)
    }

    /// Finish both budget checks before touching the language arena. On
    /// refusal the caller uses the ordinary exact compiler for this candidate.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn compile(
        &mut self,
        cache: &mut PieceOrderLanguageCache,
        pieces: &[PieceKind],
        permitted_parents: &[[u64; MAX_PREFIX_WORDS]],
        active: &[u64; MAX_PREFIX_WORDS],
        full: usize,
        state_budget: usize,
        inspected_edges: &mut usize,
    ) -> Result<Option<u32>, WasmExactSearchError> {
        let Some(layout) = FrontierLayout::new(pieces) else {
            return Ok(None);
        };
        let state_budget = state_budget.min(MAX_FRONTIER_STATES);
        // Keep even scratch growth bounded by the validated representation.
        self.indexes.clear();
        self.nodes.clear();
        if self
            .insert(
                Frontier {
                    base: 0,
                    members: 1,
                },
                state_budget,
            )
            .is_none()
        {
            return Ok(None);
        }
        let invalid = || WasmExactSearchError::InvalidProblem(INVALID);
        let mut visited = [0_u64; MAX_PREFIX_WORDS];
        let mut index = 0;
        let mut inspected = 0;
        // BFS discovers all states at rank r before processing any of that
        // rank. Every transition increases rank by one.
        while index < self.nodes.len() {
            let Frontier { base, mut members } = self.nodes[index].frontier;
            let mut successors = [0_u16; 7];
            while members != 0 {
                let variant = members.trailing_zeros() as usize;
                members &= members - 1;
                let subset = usize::from(base ^ layout.subset_toggle[variant]);
                let word = subset / 64;
                let bit = 1_u64 << (subset % 64);
                visited[word] |= bit;
                let mut has_path = subset == full;
                for (operation, parent) in permitted_parents.iter().enumerate() {
                    if parent[word] & bit == 0 {
                        continue;
                    }
                    inspected += 1;
                    *inspected_edges += 1;
                    if inspected > MAX_EDGE_INSPECTIONS {
                        return Ok(None);
                    }
                    let operation_bit = 1_u16 << operation;
                    let child = subset | usize::from(operation_bit);
                    if child == subset {
                        return Err(invalid());
                    }
                    if active[child / 64] & (1_u64 << (child % 64)) == 0 {
                        continue;
                    }
                    has_path = true;
                    let kind = layout.kind_by_operation[operation];
                    let group = layout.kinds[kind];
                    let child_variant = if base & group.first != 0 {
                        variant & !group.variant_bit
                    } else if operation_bit == group.second {
                        variant | group.variant_bit
                    } else {
                        variant
                    };
                    successors[kind] |= 1 << child_variant;
                }
                if !has_path {
                    return Err(invalid());
                }
            }
            for (kind, members) in successors.into_iter().enumerate() {
                if members == 0 {
                    continue;
                }
                let group = layout.kinds[kind];
                let add = if base & group.first == 0 {
                    group.first
                } else {
                    group.second
                };
                if add == 0 || base & add != 0 {
                    return Err(invalid());
                }
                let Some(child) = self.insert(
                    Frontier {
                        base: base | add,
                        members,
                    },
                    state_budget,
                ) else {
                    return Ok(None);
                };
                if usize::from(child) <= index {
                    return Err(invalid());
                }
                self.nodes[index].children[kind] = child;
            }
            index += 1;
        }
        // The existing compiler examines every active state. In case its
        // input included disconnected active bits, let it check those too.
        if &visited != active {
            return Ok(None);
        }

        cache.ensure_buckets();
        cache.edge_scratch.clear();
        cache.edge_scratch.try_reserve(7).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_symbolic_prefix_language_scratch_unavailable",
            )
        })?;
        for index in (0..self.nodes.len()).rev() {
            let node = &self.nodes[index];
            let accepting = usize::from(node.frontier.base) == full;
            cache.edge_scratch.clear();
            for (kind, child) in node.children.into_iter().enumerate() {
                if child != NO_CHILD {
                    let reference = self.nodes[usize::from(child)].language_reference;
                    cache.edge_scratch.push(
                        CanonicalPieceEdge::new(reference, kind as u8 + 1).ok_or_else(invalid)?,
                    );
                }
            }
            let depth = cache.remaining_depth(accepting)?;
            self.nodes[index].language_reference = cache.intern_node(accepting, depth)?;
        }
        Ok(Some(self.nodes[0].language_reference))
    }
}
