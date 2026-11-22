//! Demand-driven physical residuals of complete labelled dependency proofs.
//!
//! A physical node represents exactly the pairs `(candidate, executed set)`
//! left by one common physical lock history. Its derivative by `(kind, cells)`
//! keeps every and only matching concrete edge. Candidate proofs and their
//! already-compiled kind suffixes are shared independently of coordinates.
//! No physical prefix node is constructed while candidates are produced.
//!
//! Only the policy evaluator may authorize `fully_revealed_node`: after the
//! relevant source is determined, existential completion of a concrete
//! residual equals membership in its exact kind-word projection. This erasure
//! is not valid before that authorization and is never inferred from the
//! subset of queues that happened to succeed.

use std::{
    cell::RefCell,
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};

use crate::backend::wasm_cpu::{
    piece_order_language::PieceOrderLanguageCache,
    placement_poset::CandidateGeometry,
    symbolic_prefix_orders::{
        live_operation_derivatives, ExactPrefixRelation, SymbolicPrefixOrders,
    },
};

use super::{
    invalid, piece_code, storage, ObservationLanguageNode, ObservationPieceLanguage,
    WasmExactSearchError, MAX_EDGES, MAX_NODES, MAX_UNION_MEMBERS, MAX_WORK, NO_NODE,
};

// Lazy residual nodes remain below MAX_NODES. The high-bit namespace reads
// the pinned kind arena directly; all-ones stays unavailable as a sentinel.
// CanonicalPieceEdge limits *children* to 29 bits, but root allocation has a
// wider u32 bound, so view admission also checks the actual arena size.
const KIND_NODE_TAG: u32 = 1_u32 << 31;
const KIND_NODE_MASK: u32 = KIND_NODE_TAG - 1;
const _: () = assert!(MAX_NODES < KIND_NODE_TAG as usize);

fn tagged_kind_node(kind: u32) -> Option<u32> {
    (kind < KIND_NODE_MASK).then_some(kind | KIND_NODE_TAG)
}

fn kind_node_id(node: u32) -> Option<u32> {
    (node & KIND_NODE_TAG != 0 && node != NO_NODE).then_some(node & KIND_NODE_MASK)
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct PrefixSuffix {
    prefix: u16,
    language: u32,
}

#[derive(Eq, PartialEq)]
struct CompactPrefixRelation {
    pieces: Vec<PieceKind>,
    // Actual operation rows followed by one actual-width active-prefix row.
    // Increasing the supported n does not add padding to existing n proofs.
    words: Vec<u64>,
}

impl CompactPrefixRelation {
    fn new(proof: &ExactPrefixRelation) -> Result<Self, WasmExactSearchError> {
        let mut pieces = Vec::new();
        pieces
            .try_reserve_exact(proof.operation_count())
            .map_err(|_| storage())?;
        pieces.extend_from_slice(proof.pieces());
        let mut words = Vec::new();
        words
            .try_reserve_exact((proof.operation_count() + 1) * proof.active_words().len())
            .map_err(|_| storage())?;
        for operation in 0..proof.operation_count() {
            words.extend_from_slice(proof.parent_words(operation));
        }
        words.extend_from_slice(proof.active_words());
        Ok(Self { pieces, words })
    }

    fn matches(&self, proof: &ExactPrefixRelation) -> bool {
        if self.pieces != proof.pieces() {
            return false;
        }
        let stride = proof.active_words().len();
        self.words.len() == (proof.operation_count() + 1) * stride
            && (0..proof.operation_count()).all(|operation| {
                self.words[operation * stride..(operation + 1) * stride]
                    == *proof.parent_words(operation)
            })
            && self.words[proof.operation_count() * stride..] == *proof.active_words()
    }

    fn live_operations_after(&self, prefix: u16) -> impl Iterator<Item = (usize, PieceKind)> + '_ {
        let stride = self.words.len() / (self.pieces.len() + 1);
        let active_start = self.pieces.len() * stride;
        live_operation_derivatives(
            &self.pieces,
            &self.words[..active_start],
            stride,
            &self.words[active_start..],
            prefix,
        )
    }

    fn retained_bytes(&self) -> usize {
        self.pieces.capacity() * core::mem::size_of::<PieceKind>()
            + self.words.capacity() * core::mem::size_of::<u64>()
    }
}

struct SharedRelation {
    proof: CompactPrefixRelation,
    suffixes: Vec<PrefixSuffix>,
    #[cfg(any(test, feature = "parallel"))]
    hash: u64,
    next_same_hash: u32,
}

impl SharedRelation {
    fn suffix(&self, prefix: u16) -> Option<u32> {
        let index = self
            .suffixes
            .binary_search_by_key(&prefix, |suffix| suffix.prefix)
            .ok()?;
        Some(self.suffixes[index].language)
    }
}

struct Candidate {
    geometry: CandidateGeometry,
    relation: u32,
}

fn relation_hash(proof: &ExactPrefixRelation) -> u64 {
    let mut hash = DefaultHasher::new();
    proof.operation_count().hash(&mut hash);
    proof.pieces().hash(&mut hash);
    for operation in 0..proof.operation_count() {
        proof.parent_words(operation).hash(&mut hash);
    }
    proof.active_words().hash(&mut hash);
    hash.finish()
}

#[derive(Default)]
pub(super) struct FactoredActionRelations {
    relations: Vec<SharedRelation>,
    relation_heads: HashMap<u64, u32>,
    candidates: Vec<Candidate>,
    total_depth: Option<u8>,
    hidden_horizon: u8,
    work: usize,
}

impl FactoredActionRelations {
    /// A hit reuses the complete suffix binding of this exact labelled
    /// relation. The caller pins the kind arena before asking for reuse.
    /// Coordinates are intentionally absent here: they remain per candidate
    /// and are consulted when each physical derivative is requested.
    pub(super) fn registered_language(
        &mut self,
        proof: &ExactPrefixRelation,
        control: &ExecutionControl,
    ) -> Result<Option<(u32, u32)>, WasmExactSearchError> {
        self.charge(1, control)?;
        let hash = relation_hash(proof);
        let mut relation = self.relation_heads.get(&hash).copied().unwrap_or(NO_NODE);
        while relation != NO_NODE {
            self.charge(1, control)?;
            let existing = &self.relations[relation as usize];
            if existing.proof.matches(proof) {
                let root = existing
                    .suffix(0)
                    .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                return Ok(Some((relation, root)));
            }
            relation = existing.next_same_hash;
        }
        Ok(None)
    }

    #[cfg(test)]
    pub(super) fn register(
        &mut self,
        orders: SymbolicPrefixOrders,
        suffixes: &[u32],
        hidden_horizon: u8,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.register_prepared(orders, suffixes, hidden_horizon, None, control)
    }

    pub(super) fn register_prepared(
        &mut self,
        orders: SymbolicPrefixOrders,
        suffixes: &[u32],
        hidden_horizon: u8,
        reused_relation: Option<u32>,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.charge(1, control)?;
        let (proof, geometry) = orders.into_observation_parts()?;
        if !proof.contains_prefix(0) {
            return Ok(());
        }
        if reused_relation.is_none() && suffixes.len() != usize::from(proof.terminal_prefix()) + 1 {
            return Err(invalid("wasm_observation_suffix_language_missing"));
        }
        let depth = u8::try_from(proof.operation_count())
            .map_err(|_| invalid("wasm_observation_physical_depth_mismatch"))?;
        let hidden_horizon = hidden_horizon.min(depth);
        if let Some(total_depth) = self.total_depth {
            if total_depth != depth || self.hidden_horizon != hidden_horizon {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
        }
        if self.candidates.len() >= MAX_NODES {
            return Err(invalid("wasm_observation_physical_relation_limit"));
        }
        if let Some(relation) = reused_relation {
            let existing = self
                .relations
                .get(relation as usize)
                .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
            // Preparing one proof cannot authorize a later, different
            // candidate's operation relation, even if numeric roots agree.
            if !existing.proof.matches(&proof) {
                return Err(invalid("wasm_observation_prepared_relation_changed"));
            }
            self.candidates.try_reserve(1).map_err(|_| storage())?;
            self.candidates.push(Candidate { geometry, relation });
            return Ok(());
        }
        let hash = relation_hash(&proof);
        let head = self.relation_heads.get(&hash).copied().unwrap_or(NO_NODE);
        let mut relation = head;
        while relation != NO_NODE {
            self.charge(1, control)?;
            let existing = &self.relations[relation as usize];
            if existing.proof.matches(&proof) {
                // Numeric roots are meaningful only in the pinned shared
                // kind arena. Detect a mismatching rebind instead of quietly
                // consuming a different candidate's cached suffixes.
                if existing
                    .suffixes
                    .iter()
                    .any(|suffix| suffixes[usize::from(suffix.prefix)] != suffix.language)
                {
                    return Err(invalid("wasm_observation_suffix_arena_changed"));
                }
                break;
            }
            relation = existing.next_same_hash;
        }
        if relation == NO_NODE {
            if self.relations.len() >= MAX_NODES {
                return Err(invalid("wasm_observation_physical_relation_limit"));
            }
            let mut retained = Vec::new();
            for (word, &active) in proof.active_words().iter().enumerate() {
                let mut remaining = active;
                while remaining != 0 {
                    let lane = remaining.trailing_zeros();
                    remaining &= remaining - 1;
                    let prefix = (word * 64 + lane as usize) as u16;
                    if prefix.count_ones() > u32::from(hidden_horizon) {
                        continue;
                    }
                    let language = suffixes[usize::from(prefix)];
                    if language == NO_NODE {
                        return Err(invalid("wasm_observation_suffix_language_missing"));
                    }
                    retained.try_reserve(1).map_err(|_| storage())?;
                    retained.push(PrefixSuffix { prefix, language });
                }
            }
            self.charge(retained.len(), control)?;
            self.relations.try_reserve(1).map_err(|_| storage())?;
            self.relation_heads.try_reserve(1).map_err(|_| storage())?;
            relation = self.relations.len() as u32;
            self.relations.push(SharedRelation {
                proof: CompactPrefixRelation::new(&proof)?,
                suffixes: retained,
                #[cfg(any(test, feature = "parallel"))]
                hash,
                next_same_hash: head,
            });
            self.relation_heads.insert(hash, relation);
        }
        self.candidates.try_reserve(1).map_err(|_| storage())?;
        self.candidates.push(Candidate { geometry, relation });
        self.total_depth = Some(depth);
        self.hidden_horizon = hidden_horizon;
        Ok(())
    }

    pub(super) fn has_roots(&self) -> bool {
        !self.candidates.is_empty()
    }

    /// Merge candidate proofs, never worker-optimal policy values. `rebound`
    /// was produced by exactly importing the source worker's kind arena.
    /// Candidate geometry moves unchanged and shared proofs are re-interned
    /// after all numeric suffix IDs have been rebound.
    #[cfg(any(test, feature = "parallel"))]
    pub(super) fn merge(
        &mut self,
        source: Self,
        rebound: &[u32],
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.charge(source.work.saturating_add(1), control)?;
        let Some(depth) = source.total_depth else {
            return Ok(());
        };
        if let Some(total_depth) = self.total_depth {
            if total_depth != depth || self.hidden_horizon != source.hidden_horizon {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
        }
        self.candidates
            .len()
            .checked_add(source.candidates.len())
            .filter(|count| *count <= MAX_NODES)
            .ok_or_else(|| invalid("wasm_observation_physical_relation_limit"))?;
        let mut relations = Vec::new();
        relations
            .try_reserve_exact(source.relations.len())
            .map_err(|_| storage())?;
        for mut incoming in source.relations {
            self.charge(incoming.suffixes.len().saturating_add(1), control)?;
            for suffix in &mut incoming.suffixes {
                suffix.language = *rebound
                    .get(suffix.language as usize)
                    .filter(|&&language| language != NO_NODE)
                    .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
            }
            let head = self
                .relation_heads
                .get(&incoming.hash)
                .copied()
                .unwrap_or(NO_NODE);
            let mut existing = head;
            while existing != NO_NODE {
                self.charge(1, control)?;
                let relation = &self.relations[existing as usize];
                if relation.proof == incoming.proof {
                    if relation.suffixes != incoming.suffixes {
                        return Err(invalid("wasm_observation_suffix_arena_changed"));
                    }
                    break;
                }
                existing = relation.next_same_hash;
            }
            if existing == NO_NODE {
                if self.relations.len() >= MAX_NODES {
                    return Err(invalid("wasm_observation_physical_relation_limit"));
                }
                self.relations.try_reserve(1).map_err(|_| storage())?;
                self.relation_heads.try_reserve(1).map_err(|_| storage())?;
                existing = self.relations.len() as u32;
                incoming.next_same_hash = head;
                self.relation_heads.insert(incoming.hash, existing);
                self.relations.push(incoming);
            }
            relations.push(existing);
        }
        self.candidates
            .try_reserve(source.candidates.len())
            .map_err(|_| storage())?;
        for (index, mut candidate) in source.candidates.into_iter().enumerate() {
            if index & 255 == 0 && control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            candidate.relation = *relations
                .get(candidate.relation as usize)
                .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
            self.candidates.push(candidate);
        }
        self.total_depth = Some(depth);
        self.hidden_horizon = source.hidden_horizon;
        Ok(())
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.candidates.capacity() * core::mem::size_of::<Candidate>()
            + self.relations.capacity() * core::mem::size_of::<SharedRelation>()
            + self.relation_heads.capacity() * (core::mem::size_of::<(u64, u32)>() + 1)
            + self
                .relations
                .iter()
                .map(|relation| {
                    relation.suffixes.capacity() * core::mem::size_of::<PrefixSuffix>()
                        + relation.proof.retained_bytes()
                })
                .sum::<usize>()
    }

    pub(super) fn view<'a>(
        &'a self,
        kinds: &'a PieceOrderLanguageCache,
        control: &ExecutionControl,
    ) -> Result<Option<FactoredPhysicalActionLanguageView<'a>>, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let Some(total_depth) = self.total_depth else {
            return Ok(None);
        };
        if kinds.node_count() >= KIND_NODE_TAG as usize {
            return Err(invalid("wasm_observation_kind_namespace_exhausted"));
        }
        let mut sources = Vec::new();
        sources
            .try_reserve_exact(self.candidates.len())
            .map_err(|_| storage())?;
        sources.extend(
            (0..self.candidates.len()).map(|candidate| ConcreteResidual {
                candidate: candidate as u32,
                prefix: 0,
            }),
        );
        let mut arena = ResidualArena {
            work: self.work,
            ..Default::default()
        };
        let root = arena.intern(total_depth, Sources::Physical(sources), control)?;
        Ok(Some(FactoredPhysicalActionLanguageView {
            source: self,
            kinds,
            arena: RefCell::new(arena),
            root,
            total_depth,
        }))
    }

    fn charge(
        &mut self,
        steps: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        charge(&mut self.work, steps, control)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ConcreteResidual {
    candidate: u32,
    prefix: u16,
}

#[derive(Eq, Hash, PartialEq)]
enum Sources {
    Physical(Vec<ConcreteResidual>),
    Revealed(Vec<u32>),
}

impl Sources {
    fn canonicalize(&mut self) {
        match self {
            Self::Physical(sources) => {
                sources.sort_unstable();
                sources.dedup();
            }
            Self::Revealed(sources) => {
                sources.sort_unstable();
                sources.dedup();
            }
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Physical(sources) => sources.len(),
            Self::Revealed(sources) => sources.len(),
        }
    }

    fn compact(&mut self) -> Result<(), WasmExactSearchError> {
        fn exact_capacity<T: Copy>(values: &mut Vec<T>) -> Result<(), WasmExactSearchError> {
            if values.capacity() != values.len() {
                let mut compact = Vec::new();
                compact
                    .try_reserve_exact(values.len())
                    .map_err(|_| storage())?;
                compact.extend_from_slice(values);
                *values = compact;
            }
            Ok(())
        }
        match self {
            Self::Physical(sources) => exact_capacity(sources),
            Self::Revealed(sources) => exact_capacity(sources),
        }
    }

    fn capacity(&self) -> usize {
        match self {
            Self::Physical(sources) => sources.capacity(),
            Self::Revealed(sources) => sources.capacity(),
        }
    }

    fn retained_bytes(&self) -> usize {
        match self {
            Self::Physical(sources) => {
                sources.capacity() * core::mem::size_of::<ConcreteResidual>()
            }
            Self::Revealed(sources) => sources.capacity() * core::mem::size_of::<u32>(),
        }
    }
}

struct ResidualNode {
    sources: Sources,
    // Piece slices are appended once. A later observation preparing another
    // piece must not invalidate a previously selected transition index.
    edges: Vec<(u8, u32)>,
    remaining_depth: u8,
    prepared: u8,
    revealed: Option<u32>,
    next_same_hash: u32,
}

#[derive(Default)]
struct ResidualArena {
    nodes: Vec<ResidualNode>,
    heads: HashMap<u64, u32>,
    members: usize,
    edges: usize,
    work: usize,
    physical_source_visits: usize,
    kind_source_visits: usize,
    revealed_source_visits: usize,
}

impl ResidualArena {
    fn intern(
        &mut self,
        remaining_depth: u8,
        mut sources: Sources,
        control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        charge(&mut self.work, sources.len().saturating_add(1), control)?;
        sources.canonicalize();
        if sources.len() == 0 {
            return Err(invalid("wasm_observation_physical_root_invalid"));
        }
        let mut hash = DefaultHasher::new();
        remaining_depth.hash(&mut hash);
        sources.hash(&mut hash);
        let hash = hash.finish();
        let head = self.heads.get(&hash).copied().unwrap_or(NO_NODE);
        let mut existing = head;
        while existing != NO_NODE {
            charge(&mut self.work, 1, control)?;
            let node = &self.nodes[existing as usize];
            if node.remaining_depth == remaining_depth && node.sources == sources {
                return Ok(existing);
            }
            existing = node.next_same_hash;
        }
        // A revealed quotient can reduce hundreds of thousands of concrete
        // residuals to a few exact kind roots. Do not retain the pre-quotient
        // input capacity in a newly stored canonical node.
        sources.compact()?;
        let members = self
            .members
            .checked_add(sources.capacity())
            .filter(|members| *members <= MAX_UNION_MEMBERS)
            .ok_or_else(|| invalid("wasm_observation_physical_union_limit"))?;
        if self.nodes.len() >= MAX_NODES {
            return Err(invalid("wasm_observation_physical_relation_limit"));
        }
        self.nodes.try_reserve(1).map_err(|_| storage())?;
        self.heads.try_reserve(1).map_err(|_| storage())?;
        let id = self.nodes.len() as u32;
        self.nodes.push(ResidualNode {
            sources,
            edges: Vec::new(),
            remaining_depth,
            prepared: 0,
            revealed: None,
            next_same_hash: head,
        });
        self.heads.insert(hash, id);
        self.members = members;
        Ok(id)
    }

    fn append_edges(
        &mut self,
        node: u32,
        piece_bit: u8,
        edges: &[(u8, u32)],
    ) -> Result<(), WasmExactSearchError> {
        let total = self
            .edges
            .checked_add(edges.len())
            .filter(|count| *count <= MAX_EDGES)
            .ok_or_else(|| invalid("wasm_observation_physical_relation_limit"))?;
        let target = self
            .nodes
            .get_mut(node as usize)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        target
            .edges
            .try_reserve(edges.len())
            .map_err(|_| storage())?;
        target.edges.extend_from_slice(edges);
        target.prepared |= piece_bit;
        self.edges = total;
        Ok(())
    }

    fn retained_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<ResidualNode>()
            + self.heads.capacity() * (core::mem::size_of::<(u64, u32)>() + 1)
            + self
                .nodes
                .iter()
                .map(|node| {
                    node.sources.retained_bytes()
                        + node.edges.capacity() * core::mem::size_of::<(u8, u32)>()
                })
                .sum::<usize>()
    }
}

pub(in crate::backend::wasm_cpu) struct FactoredPhysicalActionLanguageView<'a> {
    source: &'a FactoredActionRelations,
    kinds: &'a PieceOrderLanguageCache,
    arena: RefCell<ResidualArena>,
    root: u32,
    total_depth: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::backend::wasm_cpu) struct FactoredActionMetrics {
    pub candidate_factors: usize,
    pub proof_factors: usize,
    pub suffix_bindings: usize,
    pub physical_nodes: usize,
    pub revealed_nodes: usize,
    pub edges: usize,
    pub residual_members: usize,
    pub physical_source_visits: usize,
    pub kind_source_visits: usize,
    pub revealed_source_visits: usize,
    pub work: usize,
}

impl FactoredPhysicalActionLanguageView<'_> {
    pub(in crate::backend::wasm_cpu) fn retained_bytes(&self) -> usize {
        self.arena.borrow().retained_bytes()
    }

    pub(in crate::backend::wasm_cpu) fn metrics(&self) -> FactoredActionMetrics {
        let arena = self.arena.borrow();
        let physical_nodes = arena
            .nodes
            .iter()
            .filter(|node| matches!(node.sources, Sources::Physical(_)))
            .count();
        FactoredActionMetrics {
            candidate_factors: self.source.candidates.len(),
            proof_factors: self.source.relations.len(),
            suffix_bindings: self
                .source
                .relations
                .iter()
                .map(|relation| relation.suffixes.len())
                .sum(),
            physical_nodes,
            revealed_nodes: arena.nodes.len() - physical_nodes,
            edges: arena.edges,
            residual_members: arena.members,
            physical_source_visits: arena.physical_source_visits,
            kind_source_visits: arena.kind_source_visits,
            revealed_source_visits: arena.revealed_source_visits,
            work: arena.work,
        }
    }

    fn physical_derivatives(
        &self,
        sources: &[ConcreteResidual],
        piece: u8,
        control: &ExecutionControl,
    ) -> Result<Vec<(u64, ConcreteResidual)>, WasmExactSearchError> {
        let mut output = Vec::new();
        for (index, residual) in sources.iter().enumerate() {
            if index & 255 == 0 && control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            let candidate = self
                .source
                .candidates
                .get(residual.candidate as usize)
                .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
            let relation = &self.source.relations[candidate.relation as usize];
            if residual.prefix.count_ones() >= u32::from(self.source.hidden_horizon) {
                return Err(invalid("wasm_observation_physical_hidden_horizon_exceeded"));
            }
            let deleted = candidate.geometry.symbolic_deleted_rows(residual.prefix);
            let (_, placements) = candidate.geometry.symbolic_frame(deleted);
            for (operation, kind) in relation.proof.live_operations_after(residual.prefix) {
                if piece_code(kind) != piece {
                    continue;
                }
                let cells = placements[operation];
                if cells.count_ones() != 4 {
                    return Err(invalid("wasm_observation_physical_lock_invalid"));
                }
                output.try_reserve(1).map_err(|_| storage())?;
                output.push((
                    cells,
                    ConcreteResidual {
                        candidate: residual.candidate,
                        prefix: residual.prefix | (1_u16 << operation),
                    },
                ));
            }
        }
        output.sort_unstable();
        output.dedup();
        Ok(output)
    }
}

impl ObservationPieceLanguage for FactoredPhysicalActionLanguageView<'_> {
    fn root(&self) -> u32 {
        self.root
    }

    fn fixed_horizon(&self) -> Option<u8> {
        Some(self.total_depth)
    }

    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        if let Some(kind) = kind_node_id(node) {
            let node = self.kinds.node(kind)?;
            return Some(ObservationLanguageNode {
                accepting: node.accepting,
                depth: self.total_depth.checked_sub(node.remaining_depth)?,
            });
        }
        let arena = self.arena.borrow();
        let node = arena.nodes.get(node as usize)?;
        Some(ObservationLanguageNode {
            accepting: node.remaining_depth == 0,
            depth: self.total_depth.checked_sub(node.remaining_depth)?,
        })
    }

    fn edge_count(&self, node: u32) -> Option<usize> {
        if let Some(kind) = kind_node_id(node) {
            return self.kinds.node_edge_count(kind);
        }
        self.arena
            .borrow()
            .nodes
            .get(node as usize)
            .map(|node| node.edges.len())
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        if let Some(kind) = kind_node_id(node) {
            let edge = self.kinds.edge_at(kind, index)?;
            return Some((edge.piece_code(), tagged_kind_node(edge.child())?));
        }
        self.arena
            .borrow()
            .nodes
            .get(node as usize)?
            .edges
            .get(index)
            .copied()
    }

    fn prepare_edges(
        &self,
        node: u32,
        piece_mask: u8,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if piece_mask & !0x7f != 0 {
            return Err(invalid("wasm_observation_physical_piece_mask_invalid"));
        }
        if let Some(kind) = kind_node_id(node) {
            return self
                .kinds
                .node(kind)
                .map(|_| ())
                .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"));
        }
        let mut arena = self.arena.borrow_mut();
        let target = arena
            .nodes
            .get(node as usize)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        let remaining_depth = target.remaining_depth;
        if remaining_depth == 0 {
            return Ok(());
        }
        let mut pending = piece_mask & !target.prepared;
        while pending != 0 {
            let index = pending.trailing_zeros() as u8;
            let bit = 1_u8 << index;
            pending ^= bit;
            let piece = index + 1;
            let members = arena.nodes[node as usize].sources.len();
            charge(&mut arena.work, members.saturating_add(1), control)?;
            match arena.nodes[node as usize].sources {
                Sources::Physical(_) => {
                    arena.physical_source_visits =
                        arena.physical_source_visits.saturating_add(members);
                }
                Sources::Revealed(_) => {
                    arena.kind_source_visits = arena.kind_source_visits.saturating_add(members);
                }
            }
            let mut edges = Vec::new();
            match &arena.nodes[node as usize].sources {
                Sources::Physical(sources) => {
                    let output = self.physical_derivatives(sources, piece, control)?;
                    let mut first = 0;
                    while first < output.len() {
                        let cells = output[first].0;
                        let mut end = first + 1;
                        while end < output.len() && output[end].0 == cells {
                            end += 1;
                        }
                        let mut children = Vec::new();
                        children
                            .try_reserve_exact(end - first)
                            .map_err(|_| storage())?;
                        children.extend(output[first..end].iter().map(|entry| entry.1));
                        let child = arena.intern(
                            remaining_depth - 1,
                            Sources::Physical(children),
                            control,
                        )?;
                        edges.try_reserve(1).map_err(|_| storage())?;
                        edges.push((piece, child));
                        first = end;
                    }
                }
                Sources::Revealed(sources) => {
                    let mut children = Vec::new();
                    for source in sources {
                        let source_node = self
                            .kinds
                            .node(*source)
                            .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                        if source_node.remaining_depth != remaining_depth {
                            return Err(invalid("wasm_observation_physical_depth_mismatch"));
                        }
                        let suffix_edges = self
                            .kinds
                            .edges_for_piece(*source, piece)
                            .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                        children
                            .try_reserve(suffix_edges.len())
                            .map_err(|_| storage())?;
                        children.extend(suffix_edges.iter().map(|edge| edge.child()));
                    }
                    if !children.is_empty() {
                        let child = arena.intern(
                            remaining_depth - 1,
                            Sources::Revealed(children),
                            control,
                        )?;
                        edges.push((piece, child));
                    }
                }
            }
            arena.append_edges(node, bit, &edges)?;
        }
        Ok(())
    }

    fn fully_revealed_node(
        &self,
        node: u32,
        control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if let Some(kind) = kind_node_id(node) {
            return self
                .kinds
                .node(kind)
                .map(|_| node)
                .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"));
        }
        let mut arena = self.arena.borrow_mut();
        let target = arena
            .nodes
            .get(node as usize)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        if let Some(revealed) = target.revealed {
            return Ok(revealed);
        }
        if target.remaining_depth == 0 || matches!(target.sources, Sources::Revealed(_)) {
            return Ok(node);
        }
        let remaining_depth = target.remaining_depth;
        let Sources::Physical(sources) = &target.sources else {
            return Err(invalid("wasm_observation_physical_root_invalid"));
        };
        let mut roots = Vec::new();
        roots
            .try_reserve_exact(sources.len())
            .map_err(|_| storage())?;
        for residual in sources {
            let candidate = &self.source.candidates[residual.candidate as usize];
            let relation = &self.source.relations[candidate.relation as usize];
            let root = relation
                .suffix(residual.prefix)
                .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
            if self
                .kinds
                .node(root)
                .is_none_or(|node| node.remaining_depth != remaining_depth)
            {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
            roots.push(root);
        }
        arena.revealed_source_visits = arena.revealed_source_visits.saturating_add(roots.len());
        let revealed = arena.intern(remaining_depth, Sources::Revealed(roots), control)?;
        arena.nodes[node as usize].revealed = Some(revealed);
        Ok(revealed)
    }

    fn revealed_alternatives(
        &self,
        node: u32,
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u32>>, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if let Some(kind) = kind_node_id(node) {
            if self.kinds.node(kind).is_none() {
                return Err(invalid("wasm_observation_suffix_language_missing"));
            }
            // A canonical source root is a membership problem, not another
            // choice of roots. Returning itself would create an epsilon cycle.
            return Ok(None);
        }
        let arena = self.arena.borrow();
        let target = arena
            .nodes
            .get(node as usize)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        let Sources::Revealed(sources) = &target.sources else {
            // Never erase a physical choice merely because this method was
            // queried. Only fully_revealed_node can create Revealed sources.
            return Ok(None);
        };
        let mut roots = Vec::new();
        roots
            .try_reserve_exact(sources.len())
            .map_err(|_| storage())?;
        for source in sources {
            let suffix = self
                .kinds
                .node(*source)
                .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
            if suffix.remaining_depth != target.remaining_depth {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
            roots.push(
                tagged_kind_node(*source)
                    .ok_or_else(|| invalid("wasm_observation_kind_namespace_exhausted"))?,
            );
        }
        Ok(Some(roots))
    }
}

fn charge(
    work: &mut usize,
    steps: usize,
    control: &ExecutionControl,
) -> Result<(), WasmExactSearchError> {
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    *work = work
        .checked_add(steps)
        .filter(|work| *work <= MAX_WORK)
        .ok_or_else(|| invalid("wasm_observation_physical_work_limit"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_rules::kicks::KickTableProfileId;

    use crate::backend::wasm_cpu::{
        catalog::GeometryCatalog, geometry::GeometryCandidate, reachability::ReachabilityWorkspace,
        symbolic_prefix_orders::SymbolicPrefixWork,
    };

    use super::*;

    fn two_grounded_os() -> SymbolicPrefixOrders {
        two_os(false)
    }

    fn two_os(stacked: bool) -> SymbolicPrefixOrders {
        let left = 3 | (3 << 10);
        let right = left << if stacked { 20 } else { 2 };
        let height = if stacked { 4 } else { 2 };
        let required = left | right;
        let initial = ((1_u64 << (10 * height)) - 1) & !required;
        let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
            10, height, initial, required,
        )
        .unwrap();
        let rows = [left, right].map(|cells| catalog.skeleton_id(PieceKind::O, cells).unwrap());
        let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
        SymbolicPrefixOrders::compile_product_with_workspace(
            &catalog,
            &candidate,
            KickTableProfileId::SrsPlus,
            &ExecutionControl::default(),
            &mut ReachabilityWorkspace::default(),
            &mut SymbolicPrefixWork::default(),
            0,
        )
        .unwrap()
    }

    fn register(
        source: &mut FactoredActionRelations,
        kinds: &mut PieceOrderLanguageCache,
        horizon: u8,
    ) {
        let orders = two_grounded_os();
        let mut suffixes = Vec::new();
        assert!(orders
            .compile_product_language_with_residuals(
                kinds,
                &mut SymbolicPrefixWork::default(),
                &mut suffixes,
            )
            .unwrap()
            .is_some());
        source
            .register(orders, &suffixes, horizon, &ExecutionControl::default())
            .unwrap();
    }

    #[test]
    fn shared_proofs_expand_only_requested_physical_derivatives() {
        let control = ExecutionControl::default();
        let mut source = FactoredActionRelations::default();
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut source, &mut kinds, 2);
        register(&mut source, &mut kinds, 2);
        assert_eq!(source.candidates.len(), 2);
        assert_eq!(source.relations.len(), 1, "share proof and suffix bindings");

        let view = source.view(&kinds, &control).unwrap().unwrap();
        assert_eq!(view.arena.borrow().nodes.len(), 1);
        assert_eq!(view.edge_count(view.root()), Some(0));
        view.prepare_edges(view.root(), 1 << 1, &control).unwrap();
        assert_eq!(view.edge_count(view.root()), Some(2));
        let first = view.edge(view.root(), 0).unwrap();
        let second = view.edge(view.root(), 1).unwrap();
        assert_eq!(first.0, piece_code(PieceKind::O));
        assert_eq!(second.0, first.0);
        assert_ne!(first.1, second.1, "different physical locks stay distinct");

        // A later observation can request another kind without changing an
        // already-selected physical action index or its continuation.
        view.prepare_edges(view.root(), 1, &control).unwrap();
        assert_eq!(view.edge_count(view.root()), Some(2));
        assert_eq!(view.edge(view.root(), 0), Some(first));
        assert_eq!(view.edge(view.root(), 1), Some(second));
        for (_, child) in [first, second] {
            view.prepare_edges(child, 1 << 1, &control).unwrap();
            let (_, terminal) = view.edge(child, 0).unwrap();
            assert!(view.node(terminal).unwrap().accepting);
        }

        // Once the source is known, the two concrete OO witnesses have the
        // same exact suffix word; this projection is explicitly authorized.
        let revealed = view.fully_revealed_node(view.root(), &control).unwrap();
        view.prepare_edges(revealed, 1 << 1, &control).unwrap();
        assert_eq!(view.edge_count(revealed), Some(1));
        assert_eq!(view.node(revealed).unwrap().depth, 0);
        assert_eq!(view.fixed_horizon(), Some(2));
    }

    #[test]
    fn registered_relation_reuses_suffix_work_without_merging_physical_locks() {
        use super::super::PhysicalActionLanguage;

        let control = ExecutionControl::default();
        let mut owner = PhysicalActionLanguage::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let mut work = SymbolicPrefixWork::default();
        let first = two_grounded_os();
        let root = owner
            .compile_relation_language(&first, &mut kinds, &mut work, &control)
            .unwrap()
            .unwrap();
        let compiled_edges = work.language_transitions;
        assert!(compiled_edges > 0);
        owner.register_relation(first, 2, &control).unwrap();
        let node_count = kinds.node_count();
        // Moving an arena preserves its identity; rebuilding an identical
        // relation must reuse its previous root, without visiting suffixes.
        let mut moved_kinds = kinds;
        for _ in 0..3 {
            let repeated = two_grounded_os();
            assert_eq!(
                owner
                    .compile_relation_language(&repeated, &mut moved_kinds, &mut work, &control,)
                    .unwrap(),
                Some(root),
            );
            assert!(owner.prefix_suffix_scratch.is_empty());
            owner.register_relation(repeated, 2, &control).unwrap();
        }
        assert_eq!(work.language_transitions, compiled_edges);
        assert_eq!(moved_kinds.node_count(), node_count);
        let view = owner
            .factored_view(&moved_kinds, &control)
            .unwrap()
            .unwrap();
        assert_eq!(view.metrics().candidate_factors, 4);
        assert_eq!(view.metrics().proof_factors, 1);
        view.prepare_edges(view.root(), 1 << 1, &control).unwrap();
        assert_eq!(view.edge_count(view.root()), Some(2));
        let left = view.edge(view.root(), 0).unwrap();
        let right = view.edge(view.root(), 1).unwrap();
        assert_ne!(
            left.1, right.1,
            "different lock commitments remain distinct"
        );
        for (_, child) in [left, right] {
            view.prepare_edges(child, 1 << 1, &control).unwrap();
            let (_, terminal) = view.edge(child, 0).unwrap();
            assert!(view.node(terminal).unwrap().accepting);
        }
    }

    #[test]
    fn relation_reuse_rejects_replaced_or_reset_kind_arenas() {
        use super::super::PhysicalActionLanguage;

        let control = ExecutionControl::default();
        let mut owner = PhysicalActionLanguage::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let mut work = SymbolicPrefixWork::default();
        let first = two_grounded_os();
        owner
            .compile_relation_language(&first, &mut kinds, &mut work, &control)
            .unwrap();
        owner.register_relation(first, 2, &control).unwrap();
        let repeated = two_grounded_os();
        let mut replacement = PieceOrderLanguageCache::default();
        // Matching numeric histories do not make two request arenas equal.
        repeated
            .compile_product_language(&mut replacement, &mut work)
            .unwrap();
        assert_eq!(replacement.node_count(), kinds.node_count());
        assert!(matches!(
            owner.compile_relation_language(&repeated, &mut replacement, &mut work, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_arena_changed"
            )),
        ));
        assert!(owner.factored_view(&replacement, &control).is_err());
        kinds.clear_retain_capacity();
        repeated
            .compile_product_language(&mut kinds, &mut work)
            .unwrap();
        assert!(matches!(
            owner.compile_relation_language(&repeated, &mut kinds, &mut work, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_arena_changed"
            )),
        ));
    }

    #[test]
    fn unregistered_supply_failure_does_not_pin_a_recycled_arena() {
        use super::super::PhysicalActionLanguage;

        let control = ExecutionControl::default();
        let mut owner = PhysicalActionLanguage::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let mut work = SymbolicPrefixWork::default();
        let orders = two_grounded_os();
        owner
            .compile_relation_language(&orders, &mut kinds, &mut work, &control)
            .unwrap();
        // Match the product lifecycle: a failed supply intersection retains
        // no candidate, so recycling the arena remains permitted.
        kinds.clear_retain_capacity();
        owner
            .compile_relation_language(&orders, &mut kinds, &mut work, &control)
            .unwrap();
        owner.register_relation(orders, 2, &control).unwrap();
        assert!(owner.factored_view(&kinds, &control).unwrap().is_some());
    }

    #[test]
    fn prepared_relation_cannot_authorize_a_different_operation_dependency() {
        use super::super::PhysicalActionLanguage;

        let control = ExecutionControl::default();
        let mut owner = PhysicalActionLanguage::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let mut work = SymbolicPrefixWork::default();
        let first = two_grounded_os();
        owner
            .compile_relation_language(&first, &mut kinds, &mut work, &control)
            .unwrap();
        owner.register_relation(first, 2, &control).unwrap();
        owner
            .compile_relation_language(&two_grounded_os(), &mut kinds, &mut work, &control)
            .unwrap();
        // Both candidates have kind word OO, but the stacked candidate has
        // a physical predecessor constraint. A kind-root match is insufficient.
        assert!(matches!(
            owner.register_relation(two_os(true), 2, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_prepared_relation_changed"
            )),
        ));
        assert_eq!(owner.factored.candidates.len(), 1);
        control.cancellation.handle().cancel();
        assert!(matches!(
            owner.compile_relation_language(&two_grounded_os(), &mut kinds, &mut work, &control),
            Err(WasmExactSearchError::Cancelled),
        ));
    }

    #[test]
    fn hidden_cutoff_and_resource_refusal_cannot_be_empty_languages() {
        let control = ExecutionControl::default();
        let mut source = FactoredActionRelations::default();
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut source, &mut kinds, 0);
        let view = source.view(&kinds, &control).unwrap().unwrap();
        assert!(matches!(
            view.prepare_edges(view.root(), 1 << 1, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_hidden_horizon_exceeded"
            ))
        ));
        let revealed = view.fully_revealed_node(view.root(), &control).unwrap();
        view.arena.borrow_mut().work = MAX_WORK;
        assert!(matches!(
            view.prepare_edges(revealed, 1 << 1, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        assert!(!view.node(revealed).unwrap().accepting);
        control.cancellation.handle().cancel();
        assert!(matches!(
            view.fully_revealed_node(revealed, &control),
            Err(WasmExactSearchError::Cancelled)
        ));
    }

    #[test]
    fn worker_merge_rebinds_suffix_ids_before_sharing_proofs() {
        use crate::backend::wasm_cpu::symbolic_prefix_domain::MAX_PREFIX_WORDS;

        let control = ExecutionControl::default();
        let mut source = FactoredActionRelations::default();
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut source, &mut kinds, 2);

        let mut worker = FactoredActionRelations::default();
        let mut worker_kinds = PieceOrderLanguageCache::default();
        // Give this arena a different numeric history before the same proof.
        let mut parents = [[0; MAX_PREFIX_WORDS]; 1];
        parents[0][0] = 1;
        let mut active = [0; MAX_PREFIX_WORDS];
        active[0] = 3;
        worker_kinds
            .compile_product_closed_prefix_language(&[PieceKind::I], &parents, &active, &mut 0)
            .unwrap();
        register(&mut worker, &mut worker_kinds, 2);
        let rebound = kinds.import_node_map(&worker_kinds).unwrap();
        assert!(rebound
            .iter()
            .enumerate()
            .any(|(old, &new)| old != new as usize));
        source.merge(worker, &rebound, &control).unwrap();

        assert_eq!(source.candidates.len(), 2);
        assert_eq!(source.relations.len(), 1);
        let view = source.view(&kinds, &control).unwrap().unwrap();
        view.prepare_edges(view.root(), 1 << 1, &control).unwrap();
        assert_eq!(view.edge_count(view.root()), Some(2));
        let revealed = view.fully_revealed_node(view.root(), &control).unwrap();
        view.prepare_edges(revealed, 1 << 1, &control).unwrap();
        assert_eq!(view.edge_count(revealed), Some(1));
        assert_eq!(view.metrics().proof_factors, 1);
        assert_eq!(view.metrics().candidate_factors, 2);
    }

    #[test]
    fn revealed_membership_reads_tagged_kind_nodes_without_union_expansion() {
        let control = ExecutionControl::default();
        let mut source = FactoredActionRelations::default();
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut source, &mut kinds, 2);
        let view = source.view(&kinds, &control).unwrap().unwrap();
        assert!(view
            .revealed_alternatives(view.root(), &control)
            .unwrap()
            .is_none());
        let revealed = view.fully_revealed_node(view.root(), &control).unwrap();
        let alternatives = view
            .revealed_alternatives(revealed, &control)
            .unwrap()
            .unwrap();
        assert_eq!(alternatives.len(), 1);
        let before = view.metrics();
        let mut node = alternatives[0];
        for depth in 0..2 {
            assert_ne!(node & KIND_NODE_TAG, 0);
            assert_eq!(view.node(node).unwrap().depth, depth);
            assert_eq!(view.fully_revealed_node(node, &control).unwrap(), node);
            assert!(view
                .revealed_alternatives(node, &control)
                .unwrap()
                .is_none());
            view.prepare_edges(node, 0x7f, &control).unwrap();
            assert_eq!(view.edge_count(node), Some(1));
            let (piece, child) = view.edge(node, 0).unwrap();
            assert_eq!(piece, piece_code(PieceKind::O));
            node = child;
        }
        assert!(view.node(node).unwrap().accepting);
        assert_eq!(view.edge_count(node), Some(0));
        let after = view.metrics();
        assert_eq!(after.physical_nodes, before.physical_nodes);
        assert_eq!(after.revealed_nodes, before.revealed_nodes);
        assert_eq!(after.edges, before.edges);
        assert_eq!(
            after.work, before.work,
            "immutable reads build no owner graph"
        );
        assert_ne!(tagged_kind_node(0), Some(view.root()));
        assert_eq!(tagged_kind_node(KIND_NODE_MASK - 1), Some(NO_NODE - 1));
        assert_eq!(tagged_kind_node(KIND_NODE_MASK), None);
        assert_eq!(tagged_kind_node(KIND_NODE_TAG), None);
        assert_eq!(kind_node_id(NO_NODE), None);
        assert!(view.node(NO_NODE).is_none());
        assert!(view.prepare_edges(NO_NODE, 1, &control).is_err());
    }
}
