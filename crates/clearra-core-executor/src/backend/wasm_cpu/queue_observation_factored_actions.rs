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
//!
//! The last hidden derivative stores only its exact suffix roots in a dormant
//! Frontier. It has no executable edges until the same observation authority
//! reveals it. This avoids retaining and then re-scanning candidate/prefix
//! identities after their physical choice has already been committed.

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

#[cfg(test)]
use super::MAX_WORK;
use super::{
    checked_work, invalid, piece_code, storage, GenerationLimits, ObservationLanguageNode,
    ObservationPieceLanguage, PhysicalActionEdge, WasmExactSearchError, NO_NODE,
};

// Lazy residual nodes remain below KIND_NODE_TAG. The high-bit namespace reads
// the pinned kind arena directly; all-ones stays unavailable as a sentinel.
// CanonicalPieceEdge limits *children* to 29 bits, but root allocation has a
// wider u32 bound, so view admission also checks the actual arena size.
const KIND_NODE_TAG: u32 = 1_u32 << 31;
const KIND_NODE_MASK: u32 = KIND_NODE_TAG - 1;

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
    // Live operation rows followed by the unchanged active-prefix row.
    // This is equality of productive labelled relations, not of local guard
    // tables: guards on inactive endpoints cannot affect a complete path.
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
            words.extend(live_parent_words(proof, operation));
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
                    .iter()
                    .copied()
                    .eq(live_parent_words(proof, operation))
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

/// Both consumers start at an active prefix and retain only pending-operation
/// edges with an active child. Thus P_i(S) & A(S) & !S_i & A(S | {i}) preserves
/// every labelled derivative and every suffix at each retained prefix. Keep
/// the pieces and active set exact; geometry remains candidate-local. Hashing,
/// storage and reuse comparison all use this same normalization. The typed
/// producer still owns the validity of its local guard table.
fn live_parent_words(
    proof: &ExactPrefixRelation,
    operation: usize,
) -> impl Iterator<Item = u64> + '_ {
    proof
        .parent_words(operation)
        .iter()
        .enumerate()
        .map(move |(word, &parents)| {
            live_parent_word(parents, proof.active_words(), operation, word)
        })
}

fn live_parent_word(parents: u64, active: &[u64], operation: usize, word: usize) -> u64 {
    if operation < 6 {
        let shift = 1_usize << operation;
        // In each 2*shift lane block the lower half has operation i pending.
        // Moving those lanes to their children stays in this same u64 word.
        const PENDING: [u64; 6] = [
            0x5555_5555_5555_5555,
            0x3333_3333_3333_3333,
            0x0f0f_0f0f_0f0f_0f0f,
            0x00ff_00ff_00ff_00ff,
            0x0000_ffff_0000_ffff,
            0x0000_0000_ffff_ffff,
        ];
        let pending = PENDING[operation];
        parents & active[word] & pending & (active[word] >> shift)
    } else {
        // Higher operation bits select a different word and preserve its lane.
        let added = 1_usize << (operation - 6);
        if word & added != 0 {
            0
        } else {
            parents & active[word] & active[word | added]
        }
    }
}

fn relation_hash(proof: &ExactPrefixRelation) -> u64 {
    let mut hash = DefaultHasher::new();
    proof.operation_count().hash(&mut hash);
    proof.pieces().hash(&mut hash);
    for operation in 0..proof.operation_count() {
        for word in live_parent_words(proof, operation) {
            word.hash(&mut hash);
        }
    }
    proof.active_words().hash(&mut hash);
    hash.finish()
}

#[derive(Default)]
pub(super) struct FactoredActionRelations {
    limits: GenerationLimits,
    relations: Vec<SharedRelation>,
    relation_heads: HashMap<u64, u32>,
    candidates: Vec<Candidate>,
    total_depth: Option<u8>,
    hidden_horizon: u8,
    reveal_floor: u8,
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
        self.register_prepared(orders, suffixes, hidden_horizon, 0, None, control)
    }

    pub(super) fn register_prepared(
        &mut self,
        orders: SymbolicPrefixOrders,
        suffixes: &[u32],
        hidden_horizon: u8,
        reveal_floor: u8,
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
        if reveal_floor > hidden_horizon {
            return Err(invalid("wasm_observation_physical_reveal_floor_invalid"));
        }
        if let Some(total_depth) = self.total_depth {
            if total_depth != depth
                || self.hidden_horizon != hidden_horizon
                || self.reveal_floor != reveal_floor
            {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
        }
        if self.candidates.len() >= self.limits.nodes(NO_NODE as usize) {
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
            if self.relations.len() >= self.limits.nodes(NO_NODE as usize) {
                return Err(invalid("wasm_observation_physical_relation_limit"));
            }
            let mut retained = Vec::new();
            for (word, &active) in proof.active_words().iter().enumerate() {
                let mut remaining = active;
                while remaining != 0 {
                    let lane = remaining.trailing_zeros();
                    remaining &= remaining - 1;
                    let prefix = (word * 64 + lane as usize) as u16;
                    let prefix_depth = prefix.count_ones();
                    // The original source, before any candidate filtering,
                    // proves that no reveal can query these middle depths.
                    // Prefix zero remains the exact relation-reuse root.
                    if prefix_depth > u32::from(hidden_horizon)
                        || (prefix != 0 && prefix_depth < u32::from(reveal_floor))
                    {
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
        self.reveal_floor = reveal_floor;
        Ok(())
    }

    pub(super) fn has_roots(&self) -> bool {
        !self.candidates.is_empty()
    }

    /// Merge candidate proofs, never worker-optimal policy values. `rebound`
    /// was produced by exactly importing the source worker's kind arena.
    /// Candidate geometry moves unchanged and shared proofs are re-interned
    /// with every numeric suffix ID checked against that import. A duplicate
    /// proof validates the rebind against its existing suffixes in one pass;
    /// it does not first rewrite an array that will immediately be discarded.
    #[cfg(any(test, feature = "parallel"))]
    pub(super) fn merge(
        &mut self,
        source: Self,
        rebound: &[u32],
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.charge(checked_work(source.work, 1)?, control)?;
        let Some(depth) = source.total_depth else {
            return Ok(());
        };
        if let Some(total_depth) = self.total_depth {
            if total_depth != depth
                || self.hidden_horizon != source.hidden_horizon
                || self.reveal_floor != source.reveal_floor
            {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
        }
        self.candidates
            .len()
            .checked_add(source.candidates.len())
            .filter(|count| *count <= self.limits.nodes(NO_NODE as usize))
            .ok_or_else(|| invalid("wasm_observation_physical_relation_limit"))?;
        let mut relations = Vec::new();
        relations
            .try_reserve_exact(source.relations.len())
            .map_err(|_| storage())?;
        for mut incoming in source.relations {
            self.charge(checked_work(incoming.suffixes.len(), 1)?, control)?;
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
                    break;
                }
                existing = relation.next_same_hash;
            }
            if existing == NO_NODE {
                for suffix in &mut incoming.suffixes {
                    suffix.language = rebound_suffix(rebound, suffix.language)?;
                }
                if self.relations.len() >= self.limits.nodes(NO_NODE as usize) {
                    return Err(invalid("wasm_observation_physical_relation_limit"));
                }
                self.relations.try_reserve(1).map_err(|_| storage())?;
                self.relation_heads.try_reserve(1).map_err(|_| storage())?;
                existing = self.relations.len() as u32;
                incoming.next_same_hash = head;
                self.relation_heads.insert(incoming.hash, existing);
                self.relations.push(incoming);
            } else {
                let suffixes = &self.relations[existing as usize].suffixes;
                if suffixes.len() != incoming.suffixes.len() {
                    return Err(invalid("wasm_observation_suffix_arena_changed"));
                }
                for (incoming, existing) in incoming.suffixes.iter().zip(suffixes) {
                    let language = rebound_suffix(rebound, incoming.language)?;
                    if incoming.prefix != existing.prefix || language != existing.language {
                        return Err(invalid("wasm_observation_suffix_arena_changed"));
                    }
                }
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
        self.reveal_floor = source.reveal_floor;
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
        let mut arena = ResidualArena {
            limits: self.limits,
            work: self.work,
            ..Default::default()
        };
        let sources = if self.hidden_horizon == 0 {
            // An H=0 request must still obtain the evaluator's original-source
            // certificate. A Frontier cannot execute a kind edge on its own.
            let mut roots = Vec::new();
            roots
                .try_reserve_exact(self.candidates.len())
                .map_err(|_| storage())?;
            charge(
                arena.limits,
                &mut arena.work,
                self.candidates.len(),
                control,
            )?;
            for (index, candidate) in self.candidates.iter().enumerate() {
                if index & 255 == 0 && control.is_cancelled() {
                    return Err(WasmExactSearchError::Cancelled);
                }
                let root = self.relations[candidate.relation as usize]
                    .suffix(0)
                    .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                validate_suffix_root(kinds, root, total_depth)?;
                roots.push(root);
            }
            arena.revealed_source_visits = roots.len();
            roots.sort_unstable();
            roots.dedup();
            Sources::Frontier(roots)
        } else {
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
            Sources::Physical(sources)
        };
        let root = arena.intern(total_depth, sources, control)?;
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
        let result = charge(self.limits, &mut self.work, steps, control);
        #[cfg(feature = "local-search-ab")]
        if matches!(
            result,
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ) {
            eprintln!(
                "clearra_observation_registration_work_limit candidates={} relations={} suffix_bindings={} work={} attempted_steps={}",
                self.candidates.len(),
                self.relations.len(),
                self.relations
                    .iter()
                    .map(|relation| relation.suffixes.len())
                    .sum::<usize>(),
                self.work,
                steps,
            );
        }
        result
    }
}

#[cfg(any(test, feature = "parallel"))]
fn rebound_suffix(rebound: &[u32], source: u32) -> Result<u32, WasmExactSearchError> {
    rebound
        .get(source as usize)
        .copied()
        .filter(|&language| language != NO_NODE)
        .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ConcreteResidual {
    candidate: u32,
    prefix: u16,
}

#[derive(Eq, Hash, PartialEq)]
enum Sources {
    Physical(Vec<ConcreteResidual>),
    // These suffixes are stored after the last physical commitment, but are
    // not a kind-action language until fully_revealed_node authorizes them.
    Frontier(Vec<u32>),
    Revealed(Vec<u32>),
}

impl Sources {
    fn canonicalize(&mut self) {
        match self {
            Self::Physical(sources) => {
                sources.sort_unstable();
                sources.dedup();
            }
            Self::Frontier(sources) | Self::Revealed(sources) => {
                sources.sort_unstable();
                sources.dedup();
            }
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Physical(sources) => sources.len(),
            Self::Frontier(sources) | Self::Revealed(sources) => sources.len(),
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
            Self::Frontier(sources) | Self::Revealed(sources) => exact_capacity(sources),
        }
    }

    fn capacity(&self) -> usize {
        match self {
            Self::Physical(sources) => sources.capacity(),
            Self::Frontier(sources) | Self::Revealed(sources) => sources.capacity(),
        }
    }

    fn retained_bytes(&self) -> usize {
        match self {
            Self::Physical(sources) => {
                sources.capacity() * core::mem::size_of::<ConcreteResidual>()
            }
            Self::Frontier(sources) | Self::Revealed(sources) => {
                sources.capacity() * core::mem::size_of::<u32>()
            }
        }
    }
}

struct ResidualNode {
    sources: Sources,
    // Piece slices are appended once. A later observation preparing another
    // piece must not invalidate a previously selected transition index.
    edges: Vec<PhysicalActionEdge>,
    remaining_depth: u8,
    prepared: u8,
    revealed: Option<u32>,
    next_same_hash: u32,
}

#[derive(Default)]
struct ResidualArena {
    limits: GenerationLimits,
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
        charge(
            self.limits,
            &mut self.work,
            checked_work(sources.len(), 1)?,
            control,
        )?;
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
            charge(self.limits, &mut self.work, 1, control)?;
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
            .filter(|members| *members <= self.limits.members())
            .ok_or_else(|| invalid("wasm_observation_physical_union_limit"))?;
        if self.nodes.len() >= self.limits.nodes(KIND_NODE_MASK as usize) {
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
        edges: &[PhysicalActionEdge],
    ) -> Result<(), WasmExactSearchError> {
        let total = self
            .edges
            .checked_add(edges.len())
            .filter(|count| *count <= self.limits.edges(usize::MAX))
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
                        + node.edges.capacity() * core::mem::size_of::<PhysicalActionEdge>()
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
    pub frontier_nodes: usize,
    pub frontier_members: usize,
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
        let frontier_nodes = arena
            .nodes
            .iter()
            .filter(|node| matches!(node.sources, Sources::Frontier(_)))
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
            revealed_nodes: arena.nodes.len() - physical_nodes - frontier_nodes,
            edges: arena.edges,
            residual_members: arena.members,
            physical_source_visits: arena.physical_source_visits,
            kind_source_visits: arena.kind_source_visits,
            revealed_source_visits: arena.revealed_source_visits,
            work: arena.work,
            frontier_nodes,
            frontier_members: arena
                .nodes
                .iter()
                .filter_map(|node| match &node.sources {
                    Sources::Frontier(roots) => Some(roots.len()),
                    _ => None,
                })
                .sum(),
        }
    }

    fn physical_derivatives<T: Ord>(
        &self,
        sources: &[ConcreteResidual],
        piece: u8,
        control: &ExecutionControl,
        mut continuation: impl FnMut(
            ConcreteResidual,
            &SharedRelation,
        ) -> Result<T, WasmExactSearchError>,
    ) -> Result<Vec<(u64, T)>, WasmExactSearchError> {
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
                    continuation(
                        ConcreteResidual {
                            candidate: residual.candidate,
                            prefix: residual.prefix | (1_u16 << operation),
                        },
                        relation,
                    )?,
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

    fn has_physical_actions(&self, node: u32) -> bool {
        kind_node_id(node).is_none()
            && self
                .arena
                .borrow()
                .nodes
                .get(node as usize)
                .is_some_and(|node| {
                    node.remaining_depth > 0 && matches!(node.sources, Sources::Physical(_))
                })
    }

    fn physical_action_cells(&self, node: u32, index: usize) -> Option<u64> {
        if !self.has_physical_actions(node) {
            return None;
        }
        self.arena
            .borrow()
            .nodes
            .get(node as usize)?
            .edges
            .get(index)
            .map(|edge| edge.cells)
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
            .map(|edge| (edge.piece, edge.child))
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
        if matches!(target.sources, Sources::Frontier(_)) {
            // Compact storage is not an observation certificate. Only the
            // evaluator may turn this boundary into an executable language.
            return Err(invalid("wasm_observation_physical_hidden_horizon_exceeded"));
        }
        let mut pending = piece_mask & !target.prepared;
        while pending != 0 {
            let index = pending.trailing_zeros() as u8;
            let bit = 1_u8 << index;
            pending ^= bit;
            let piece = index + 1;
            let members = arena.nodes[node as usize].sources.len();
            charge(
                arena.limits,
                &mut arena.work,
                checked_work(members, 1)?,
                control,
            )?;
            match arena.nodes[node as usize].sources {
                Sources::Physical(_) => {
                    arena.physical_source_visits =
                        arena.physical_source_visits.saturating_add(members);
                }
                Sources::Revealed(_) => {
                    arena.kind_source_visits = arena.kind_source_visits.saturating_add(members);
                }
                Sources::Frontier(_) => {
                    return Err(invalid("wasm_observation_physical_hidden_horizon_exceeded"));
                }
            }
            let mut edges = Vec::new();
            match &arena.nodes[node as usize].sources {
                Sources::Physical(sources) => {
                    let child_depth = self
                        .total_depth
                        .checked_sub(remaining_depth)
                        .and_then(|depth| depth.checked_add(1))
                        .ok_or_else(|| invalid("wasm_observation_physical_depth_mismatch"))?;
                    if child_depth == self.source.hidden_horizon {
                        let mut projected = 0_usize;
                        let output = self.physical_derivatives(
                            sources,
                            piece,
                            control,
                            |residual, relation| {
                                let root = relation.suffix(residual.prefix).ok_or_else(|| {
                                    invalid("wasm_observation_suffix_language_missing")
                                })?;
                                validate_suffix_root(self.kinds, root, remaining_depth - 1)?;
                                projected += 1;
                                Ok(root)
                            },
                        )?;
                        arena.revealed_source_visits =
                            arena.revealed_source_visits.saturating_add(projected);
                        edges = intern_physical_groups(
                            &mut arena,
                            remaining_depth - 1,
                            piece,
                            &output,
                            Sources::Frontier,
                            control,
                        )?;
                    } else {
                        let output =
                            self.physical_derivatives(sources, piece, control, |residual, _| {
                                Ok(residual)
                            })?;
                        edges = intern_physical_groups(
                            &mut arena,
                            remaining_depth - 1,
                            piece,
                            &output,
                            Sources::Physical,
                            control,
                        )?;
                    }
                }
                Sources::Frontier(_) => {
                    return Err(invalid("wasm_observation_physical_hidden_horizon_exceeded"));
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
                        edges.push(PhysicalActionEdge {
                            piece,
                            cells: 0,
                            child,
                        });
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
        let mut roots = Vec::new();
        roots
            .try_reserve_exact(target.sources.len())
            .map_err(|_| storage())?;
        let mut projected = 0_usize;
        match &target.sources {
            Sources::Physical(sources) => {
                for (index, residual) in sources.iter().enumerate() {
                    if index & 255 == 0 && control.is_cancelled() {
                        return Err(WasmExactSearchError::Cancelled);
                    }
                    let candidate = &self.source.candidates[residual.candidate as usize];
                    let relation = &self.source.relations[candidate.relation as usize];
                    let root = relation
                        .suffix(residual.prefix)
                        .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                    validate_suffix_root(self.kinds, root, remaining_depth)?;
                    roots.push(root);
                }
                projected = sources.len();
            }
            Sources::Frontier(sources) => roots.extend_from_slice(sources),
            Sources::Revealed(_) => {
                return Err(invalid("wasm_observation_physical_root_invalid"));
            }
        }
        arena.revealed_source_visits = arena.revealed_source_visits.saturating_add(projected);
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

fn validate_suffix_root(
    kinds: &PieceOrderLanguageCache,
    root: u32,
    remaining_depth: u8,
) -> Result<(), WasmExactSearchError> {
    if kinds
        .node(root)
        .is_none_or(|node| node.remaining_depth != remaining_depth)
    {
        return Err(invalid("wasm_observation_physical_depth_mismatch"));
    }
    Ok(())
}

/// The caller has sorted and deduplicated (physical cells, continuation)
/// pairs. Preserve each lock choice while unioning exactly its continuations.
fn intern_physical_groups<T: Copy>(
    arena: &mut ResidualArena,
    remaining_depth: u8,
    piece: u8,
    output: &[(u64, T)],
    sources: impl Fn(Vec<T>) -> Sources,
    control: &ExecutionControl,
) -> Result<Vec<PhysicalActionEdge>, WasmExactSearchError> {
    let mut edges = Vec::new();
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
        let child = arena.intern(remaining_depth, sources(children), control)?;
        edges.try_reserve(1).map_err(|_| storage())?;
        edges.push(PhysicalActionEdge {
            piece,
            cells,
            child,
        });
        first = end;
    }
    Ok(edges)
}

fn charge(
    limits: GenerationLimits,
    work: &mut usize,
    steps: usize,
    control: &ExecutionControl,
) -> Result<(), WasmExactSearchError> {
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    *work = work
        .checked_add(steps)
        .filter(|work| *work <= limits.work())
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
    fn dead_guard_bits_share_live_rows_across_lane_and_word_boundaries() {
        // One complete chain of 11 labelled operations, plus guards wholly
        // inside an unreachable, nonproductive component. No added guard
        // repeats an already executed operation or joins the live chain.
        const OPERATIONS: usize = 11;
        const PREFIXES: usize = 1 << OPERATIONS;
        let mut active = vec![0_u64; PREFIXES / 64];
        for depth in 0..=OPERATIONS {
            let prefix = (1_usize << depth) - 1;
            active[prefix / 64] |= 1_u64 << (prefix % 64);
        }
        let mut words = Vec::new();
        for operation in 0..OPERATIONS {
            let mut clean = vec![0_u64; active.len()];
            let parent = (1_usize << operation) - 1;
            clean[parent / 64] = 1_u64 << (parent % 64);
            let mut noisy = clean.clone();
            for prefix in 0..PREFIXES {
                let child = prefix | (1 << operation);
                if child != prefix
                    && active[prefix / 64] & (1_u64 << (prefix % 64)) == 0
                    && active[child / 64] & (1_u64 << (child % 64)) == 0
                {
                    noisy[prefix / 64] |= 1_u64 << (prefix % 64);
                }
            }
            assert_ne!(clean, noisy);
            for word in 0..active.len() {
                let normalized = live_parent_word(noisy[word], &active, operation, word);
                assert_eq!(
                    normalized, clean[word],
                    "operation {operation}, word {word}"
                );
                words.push(normalized);
            }
        }
        words.extend_from_slice(&active);
        let proof = CompactPrefixRelation {
            pieces: vec![PieceKind::I; OPERATIONS],
            words,
        };
        for depth in 0..=OPERATIONS {
            let prefix = (1_u16 << depth) - 1;
            let expected = if depth == OPERATIONS {
                Vec::new()
            } else {
                vec![(depth, PieceKind::I)]
            };
            assert_eq!(
                proof.live_operations_after(prefix).collect::<Vec<_>>(),
                expected
            );
        }
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
    fn hidden_boundary_keeps_physical_choice_but_deduplicates_dormant_suffixes() {
        let control = ExecutionControl::default();
        let mut source = FactoredActionRelations::default();
        let mut kinds = PieceOrderLanguageCache::default();
        for _ in 0..32 {
            register(&mut source, &mut kinds, 1);
        }
        let view = source.view(&kinds, &control).unwrap().unwrap();
        view.prepare_edges(view.root(), 1 << 1, &control).unwrap();
        assert_eq!(view.edge_count(view.root()), Some(2));
        let left = view.edge(view.root(), 0).unwrap();
        let right = view.edge(view.root(), 1).unwrap();
        assert_eq!(left.0, piece_code(PieceKind::O));
        assert_eq!(
            left, right,
            "both committed locks have the same suffix word O"
        );
        let boundary = left.1;
        assert_eq!(view.node(boundary).unwrap().depth, 1);
        assert!(!view.node(boundary).unwrap().accepting);
        assert_eq!(view.metrics().frontier_nodes, 1);
        assert_eq!(view.metrics().frontier_members, 1);
        assert_eq!(view.metrics().residual_members, 33);
        assert_eq!(view.edge_count(boundary), Some(0));
        assert_eq!(
            view.revealed_alternatives(boundary, &control).unwrap(),
            None
        );
        assert!(matches!(
            view.prepare_edges(boundary, 1 << 1, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_hidden_horizon_exceeded"
            )),
        ));

        // Only the observation evaluator's certificate enables these roots.
        let revealed = view.fully_revealed_node(boundary, &control).unwrap();
        assert_ne!(revealed, boundary);
        assert_eq!(view.node(revealed).unwrap().depth, 1);
        view.prepare_edges(revealed, 1 << 1, &control).unwrap();
        let (_, terminal) = view.edge(revealed, 0).unwrap();
        assert!(view.node(terminal).unwrap().accepting);
        assert_eq!(
            view.fully_revealed_node(boundary, &control).unwrap(),
            revealed
        );
    }

    #[test]
    fn zero_hidden_horizon_starts_with_a_dormant_exact_kind_union() {
        let control = ExecutionControl::default();
        let mut source = FactoredActionRelations::default();
        let mut kinds = PieceOrderLanguageCache::default();
        for _ in 0..16 {
            register(&mut source, &mut kinds, 0);
        }
        let view = source.view(&kinds, &control).unwrap().unwrap();
        assert_eq!(view.metrics().physical_nodes, 0);
        assert_eq!(view.metrics().frontier_nodes, 1);
        assert_eq!(view.metrics().frontier_members, 1);
        assert_eq!(view.metrics().residual_members, 1);
        assert!(view.prepare_edges(view.root(), 1 << 1, &control).is_err());
        let revealed = view.fully_revealed_node(view.root(), &control).unwrap();
        assert_eq!(view.node(revealed).unwrap().depth, 0);
        view.prepare_edges(revealed, 1 << 1, &control).unwrap();
        assert_eq!(view.edge_count(revealed), Some(1));
    }

    #[test]
    fn reveal_floor_keeps_reuse_root_and_physical_paths_but_rejects_mixed_owners() {
        let control = ExecutionControl::default();
        let mut source = FactoredActionRelations::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let orders = two_grounded_os();
        let mut suffixes = Vec::new();
        orders
            .compile_product_language_with_residuals(
                &mut kinds,
                &mut SymbolicPrefixWork::default(),
                &mut suffixes,
            )
            .unwrap();
        source
            .register_prepared(orders, &suffixes, 2, 2, None, &control)
            .unwrap();
        assert_eq!(source.relations[0].suffixes.len(), 2);
        assert!(source.relations[0].suffix(0).is_some());
        assert_eq!(source.relations[0].suffix(1), None);
        assert_eq!(source.relations[0].suffix(2), None);
        assert!(source.relations[0].suffix(3).is_some());
        let repeated = two_grounded_os();
        let (relation, root) = source
            .registered_language(repeated.observation_relation().unwrap(), &control)
            .unwrap()
            .unwrap();
        assert_eq!(Some(root), source.relations[0].suffix(0));
        source
            .register_prepared(repeated, &[], 2, 2, Some(relation), &control)
            .unwrap();
        let view = source.view(&kinds, &control).unwrap().unwrap();
        view.prepare_edges(view.root(), 1 << 1, &control).unwrap();
        assert_eq!(view.edge_count(view.root()), Some(2));
        for index in 0..2 {
            let (_, child) = view.edge(view.root(), index).unwrap();
            view.prepare_edges(child, 1 << 1, &control).unwrap();
            let (_, terminal) = view.edge(child, 0).unwrap();
            assert!(view.node(terminal).unwrap().accepting);
        }
        drop(view);

        assert!(matches!(
            source.register_prepared(two_grounded_os(), &suffixes, 2, 0, None, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_depth_mismatch"
            )),
        ));
        let mut worker = FactoredActionRelations::default();
        let mut worker_kinds = PieceOrderLanguageCache::default();
        register(&mut worker, &mut worker_kinds, 2);
        let rebound = kinds.import_node_map(&worker_kinds).unwrap();
        assert!(matches!(
            source.merge(worker, &rebound, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_depth_mismatch"
            )),
        ));
        assert_eq!(source.candidates.len(), 2);
        assert_eq!(source.relations.len(), 1);
        assert!(matches!(
            FactoredActionRelations::default().register_prepared(
                two_grounded_os(),
                &suffixes,
                1,
                2,
                None,
                &control,
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_reveal_floor_invalid"
            )),
        ));
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
        source.limits = GenerationLimits::DEVELOPMENT;
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
    fn duplicate_worker_proof_still_rejects_a_wrong_suffix_rebind() {
        let control = ExecutionControl::default();
        let mut source = FactoredActionRelations::default();
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut source, &mut kinds, 2);
        let mut worker = FactoredActionRelations::default();
        let mut worker_kinds = PieceOrderLanguageCache::default();
        register(&mut worker, &mut worker_kinds, 2);
        let root = worker.relations[0].suffix(0).unwrap();
        let terminal = worker.relations[0].suffix(3).unwrap();
        let mut rebound = kinds.import_node_map(&worker_kinds).unwrap();
        assert_ne!(rebound[root as usize], rebound[terminal as usize]);
        // Every ID remains in range; equal proofs cannot authorize the wrong
        // semantic binding even when no incoming suffix array is rewritten.
        rebound[root as usize] = rebound[terminal as usize];
        assert!(matches!(
            source.merge(worker, &rebound, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_arena_changed"
            ))
        ));
        assert_eq!(source.candidates.len(), 1);
        assert_eq!(source.relations.len(), 1);
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
