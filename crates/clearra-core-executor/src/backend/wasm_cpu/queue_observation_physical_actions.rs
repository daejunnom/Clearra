//! Exact union of complete physical lock languages for one PC request.
//!
//! A key is a piece kind and its four occupied cells in the current physical
//! board. Equal keys are merged only along a common physical action history.
//! Thus a policy can postpone choosing a tiling, but cannot postpone choosing
//! a physical lock until hidden supply has been revealed. Input nodes have
//! already been proved by a dependency relation or a concrete BuildUp graph;
//! this owner performs no movement or BuildUp search.
//!
//! Product requests retain factored proofs and expand them on demand. The
//! compact fallback imports its existing concrete graph into the same exact
//! physical-action union, before that graph can lose its lock footprints.

use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
};

#[cfg(feature = "placement-dependency-pc")]
use std::sync::Arc;

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use super::{piece_code, ObservationLanguageNode, ObservationPieceLanguage, WasmExactSearchError};
use crate::backend::wasm_cpu::buildup::{BuildEdge, BuildOrderGraph};

#[cfg(feature = "placement-dependency-pc")]
#[path = "queue_observation_factored_actions.rs"]
mod factored;

const NO_NODE: u32 = u32::MAX;
const MAX_NODES: usize = 1_000_000;
const MAX_EDGES: usize = 8_000_000;
const MAX_UNION_MEMBERS: usize = 8_000_000;
const MAX_WORK: usize = 64_000_000;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::backend::wasm_cpu) struct PhysicalActionEdge {
    piece: u8,
    cells: u64,
    child: u32,
}

impl PhysicalActionEdge {
    pub(in crate::backend::wasm_cpu) fn new(
        piece: PieceKind,
        cells: u64,
        child: u32,
    ) -> Result<Self, WasmExactSearchError> {
        if cells.count_ones() != 4 {
            return Err(invalid("wasm_observation_physical_lock_invalid"));
        }
        Ok(Self {
            piece: piece_code(piece),
            cells,
            child,
        })
    }

    fn action(self) -> (u8, u64) {
        (self.piece, self.cells)
    }
}

#[derive(Clone, Copy)]
struct PhysicalNode {
    first_edge: u32,
    edge_count: u32,
    next_same_hash: u32,
    remaining_depth: u8,
    is_root: bool,
}

/// The caller owns one exact initial board, rule, supply, and PC target family.
/// Node IDs are local to this owner. A hash never authorizes an equivalence:
/// interning additionally compares every physical action and child exactly.
#[derive(Default)]
pub(in crate::backend::wasm_cpu) struct PhysicalActionLanguage {
    nodes: Vec<PhysicalNode>,
    edges: Vec<PhysicalActionEdge>,
    hash_heads: HashMap<u64, u32>,
    roots: Vec<u32>,
    unions: HashMap<Vec<u32>, u32>,
    union_members: usize,
    work: usize,
    compact_width: Option<u8>,
    #[cfg(feature = "placement-dependency-pc")]
    factored: factored::FactoredActionRelations,
    #[cfg(feature = "placement-dependency-pc")]
    prefix_suffix_scratch: Vec<u32>,
    #[cfg(feature = "placement-dependency-pc")]
    suffix_arena: Option<Arc<()>>,
    #[cfg(feature = "placement-dependency-pc")]
    prepared_relation: Option<u32>,
}

impl PhysicalActionLanguage {
    /// Import an already-proved fixed-depth compact graph. Raw operation
    /// edges retain the physical poses that piece-language canonicalization
    /// intentionally forgets. Only equal four-cell locks may share a union.
    pub(in crate::backend::wasm_cpu) fn register_graph(
        &mut self,
        graph: &BuildOrderGraph,
        width: u8,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.charge(graph.nodes.len(), control)?;
        if self.has_factored_roots() {
            return Err(invalid("wasm_observation_eager_and_factored_owners_mixed"));
        }
        if width == 0 || width > 64 {
            return Err(invalid("wasm_observation_physical_width_invalid"));
        }
        let root = graph
            .nodes
            .get(graph.root as usize)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        if !root.live {
            return Ok(());
        }
        if root.depth != 0 {
            return Err(invalid("wasm_observation_physical_depth_mismatch"));
        }
        if self.compact_width.is_some_and(|previous| previous != width) {
            return Err(invalid("wasm_observation_physical_width_mismatch"));
        }

        // Mark the live root-reachable subgraph first. Unreachable accepting
        // nodes cannot change this root's horizon or authorize a continuation.
        const PENDING: u32 = u32::MAX - 1;
        let mut classes = Vec::new();
        classes
            .try_reserve_exact(graph.nodes.len())
            .map_err(|_| storage())?;
        classes.resize(graph.nodes.len(), NO_NODE);
        classes[graph.root as usize] = PENDING;
        let mut horizon = None;
        for (index, node) in graph.nodes.iter().enumerate() {
            if classes[index] != PENDING {
                continue;
            }
            let edges = graph.edges(index);
            self.charge(edges.len().saturating_add(1), control)?;
            if node.accepting() {
                if !edges.is_empty() || horizon.is_some_and(|depth| depth != node.depth) {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                horizon = Some(node.depth);
            }
            for edge in edges {
                let child_index = edge.to as usize;
                let child = graph
                    .nodes
                    .get(child_index)
                    .ok_or_else(|| invalid("wasm_observation_physical_child_invalid"))?;
                if child_index <= index || node.depth.checked_add(1) != Some(child.depth) {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                if child.live {
                    classes[child_index] = PENDING;
                }
            }
        }
        let horizon = horizon.ok_or_else(|| invalid("wasm_observation_physical_depth_mismatch"))?;
        if self
            .roots
            .first()
            .is_some_and(|&root| self.nodes[root as usize].remaining_depth != horizon)
        {
            return Err(invalid("wasm_observation_physical_depth_mismatch"));
        }
        self.compact_width = Some(width);
        let mut edges = Vec::new();
        for (index, node) in graph.nodes.iter().enumerate().rev() {
            if classes[index] != PENDING {
                continue;
            }
            self.charge(1, control)?;
            edges.clear();
            edges
                .try_reserve(graph.edges(index).len())
                .map_err(|_| storage())?;
            for edge in graph.edges(index) {
                if !graph.nodes[edge.to as usize].live {
                    continue;
                }
                let child = classes[edge.to as usize];
                if child == PENDING || child == NO_NODE {
                    return Err(invalid("wasm_observation_physical_child_invalid"));
                }
                edges.push(PhysicalActionEdge::new(
                    edge.piece,
                    compact_lock_cells(*edge, width)?,
                    child,
                )?);
            }
            let remaining_depth = horizon
                .checked_sub(node.depth)
                .ok_or_else(|| invalid("wasm_observation_physical_depth_mismatch"))?;
            if node.accepting() != (remaining_depth == 0) {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
            classes[index] = self.intern_action_union(remaining_depth, &mut edges, control)?;
        }
        self.insert_root(classes[graph.root as usize])
    }

    /// Intern one already-proved concrete continuation. Callers retain their
    /// scratch vector. Two distinct physical locks of the same kind remain
    /// distinct edges, even when their continuation is identical.
    pub(in crate::backend::wasm_cpu) fn intern_node(
        &mut self,
        remaining_depth: u8,
        edges: &mut Vec<PhysicalActionEdge>,
        control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        self.charge(edges.len().saturating_add(1), control)?;
        edges.sort_unstable();
        edges.dedup();
        if (remaining_depth == 0) != edges.is_empty() {
            return Err(invalid("wasm_observation_physical_depth_mismatch"));
        }
        for (index, edge) in edges.iter().enumerate() {
            let child = self
                .nodes
                .get(edge.child as usize)
                .ok_or_else(|| invalid("wasm_observation_physical_child_invalid"))?;
            if child.remaining_depth.checked_add(1) != Some(remaining_depth) {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
            if index != 0 && edges[index - 1].action() == edge.action() {
                // Unioning equal physical actions is a separate exact step.
                // Leaving them as alternatives would commit to a tiling too
                // early, while merging by kind would commit too late.
                return Err(invalid("wasm_observation_physical_action_not_unioned"));
            }
        }
        let mut hash = DefaultHasher::new();
        remaining_depth.hash(&mut hash);
        edges.hash(&mut hash);
        let hash = hash.finish();
        let head = self.hash_heads.get(&hash).copied().unwrap_or(NO_NODE);
        let mut existing = head;
        while existing != NO_NODE {
            self.charge(1, control)?;
            let node = self.nodes[existing as usize];
            if node.remaining_depth == remaining_depth && self.node_edges(node) == edges.as_slice()
            {
                return Ok(existing);
            }
            existing = node.next_same_hash;
        }
        if self.nodes.len() >= MAX_NODES || self.edges.len().saturating_add(edges.len()) > MAX_EDGES
        {
            return Err(invalid("wasm_observation_physical_relation_limit"));
        }
        self.nodes.try_reserve(1).map_err(|_| storage())?;
        self.edges.try_reserve(edges.len()).map_err(|_| storage())?;
        self.hash_heads.try_reserve(1).map_err(|_| storage())?;
        let reference = self.nodes.len() as u32;
        self.nodes.push(PhysicalNode {
            first_edge: self.edges.len() as u32,
            edge_count: edges.len() as u32,
            next_same_hash: head,
            remaining_depth,
            is_root: false,
        });
        self.edges.extend_from_slice(edges);
        self.hash_heads.insert(hash, reference);
        Ok(reference)
    }

    pub(in crate::backend::wasm_cpu) fn insert_root(
        &mut self,
        root: u32,
    ) -> Result<(), WasmExactSearchError> {
        let remaining_depth = self
            .nodes
            .get(root as usize)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?
            .remaining_depth;
        if self
            .roots
            .first()
            .is_some_and(|&first| self.nodes[first as usize].remaining_depth != remaining_depth)
        {
            return Err(invalid("wasm_observation_physical_depth_mismatch"));
        }
        let node = self
            .nodes
            .get_mut(root as usize)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        if !node.is_root {
            self.roots.try_reserve(1).map_err(|_| storage())?;
            self.roots.push(root);
            node.is_root = true;
        }
        Ok(())
    }

    pub(in crate::backend::wasm_cpu) fn has_roots(&self) -> bool {
        !self.roots.is_empty() || self.has_factored_roots()
    }

    pub(in crate::backend::wasm_cpu) fn has_factored_roots(&self) -> bool {
        #[cfg(feature = "placement-dependency-pc")]
        if self.factored.has_roots() {
            return true;
        }
        false
    }

    /// Rebind an eager worker's physical nodes by exact edge equality. The
    /// caller has already checked that both workers belong to one request.
    #[cfg(any(test, feature = "parallel"))]
    pub(in crate::backend::wasm_cpu) fn merge_eager(
        &mut self,
        source: Self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.charge(0, control)?;
        if source.has_factored_roots() {
            return Err(invalid("wasm_observation_eager_and_factored_owners_mixed"));
        }
        if source.roots.is_empty() {
            return Ok(());
        }
        if self.has_factored_roots() {
            return Err(invalid("wasm_observation_eager_and_factored_owners_mixed"));
        }
        if let (Some(left), Some(right)) = (self.compact_width, source.compact_width) {
            if left != right {
                return Err(invalid("wasm_observation_physical_width_mismatch"));
            }
        }
        if self.nodes.is_empty() {
            *self = source;
            return Ok(());
        }
        self.compact_width = self.compact_width.or(source.compact_width);
        self.charge(source.nodes.len(), control)?;
        let mut rebound = Vec::new();
        rebound
            .try_reserve_exact(source.nodes.len())
            .map_err(|_| storage())?;
        let mut edges = Vec::new();
        for node in source.nodes.iter().copied() {
            self.charge(node.edge_count as usize + 1, control)?;
            edges.clear();
            edges
                .try_reserve(node.edge_count as usize)
                .map_err(|_| storage())?;
            for edge in source.node_edges(node) {
                let child = rebound
                    .get(edge.child as usize)
                    .copied()
                    .ok_or_else(|| invalid("wasm_observation_physical_child_invalid"))?;
                edges.push(PhysicalActionEdge { child, ..*edge });
            }
            rebound.push(self.intern_node(node.remaining_depth, &mut edges, control)?);
        }
        for root in source.roots {
            self.insert_root(rebound[root as usize])?;
        }
        Ok(())
    }

    /// Retain one proof factor and its own geometry. Physical action nodes
    /// are expanded only when the observation policy requests a derivative.
    #[cfg(feature = "placement-dependency-pc")]
    pub(in crate::backend::wasm_cpu) fn register_relation(
        &mut self,
        orders: crate::backend::wasm_cpu::symbolic_prefix_orders::SymbolicPrefixOrders,
        hidden_horizon: u8,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if !self.roots.is_empty() {
            return Err(invalid("wasm_observation_eager_and_factored_owners_mixed"));
        }
        self.factored.register_prepared(
            orders,
            &self.prefix_suffix_scratch,
            hidden_horizon,
            self.prepared_relation.take(),
            control,
        )
    }

    /// Reuse the exact registered relation before repeating the bottom-up
    /// suffix compilation. A relation hit retains no new suffix array and
    /// still leaves the concrete geometry to `register_relation`.
    #[cfg(feature = "placement-dependency-pc")]
    pub(in crate::backend::wasm_cpu) fn compile_relation_language(
        &mut self,
        orders: &crate::backend::wasm_cpu::symbolic_prefix_orders::SymbolicPrefixOrders,
        kinds: &mut crate::backend::wasm_cpu::piece_order_language::PieceOrderLanguageCache,
        work: &mut crate::backend::wasm_cpu::symbolic_prefix_orders::SymbolicPrefixWork,
        control: &ExecutionControl,
    ) -> Result<Option<u32>, WasmExactSearchError> {
        self.prepared_relation = None;
        self.prefix_suffix_scratch.clear();
        if !self.roots.is_empty() {
            return Err(invalid("wasm_observation_eager_and_factored_owners_mixed"));
        }
        self.bind_suffix_arena(kinds)?;
        if let Some((relation, root)) = self
            .factored
            .registered_language(orders.observation_relation()?, control)?
        {
            self.prepared_relation = Some(relation);
            return Ok(Some(root));
        }
        orders.compile_product_language_with_residuals(kinds, work, &mut self.prefix_suffix_scratch)
    }

    #[cfg(all(test, feature = "placement-dependency-pc"))]
    pub(in crate::backend::wasm_cpu) fn prefix_suffix_scratch(&mut self) -> &mut Vec<u32> {
        &mut self.prefix_suffix_scratch
    }

    #[cfg(feature = "placement-dependency-pc")]
    fn bind_suffix_arena(
        &mut self,
        kinds: &crate::backend::wasm_cpu::piece_order_language::PieceOrderLanguageCache,
    ) -> Result<(), WasmExactSearchError> {
        if self.factored.has_roots() {
            self.check_suffix_arena(kinds)?;
        }
        // A candidate may fail the supply product before it is registered.
        // Until a physical root survives, the workspace may recycle its kind
        // arena. No retained suffix is authorized by that discarded attempt.
        if self
            .suffix_arena
            .as_ref()
            .is_none_or(|identity| !Arc::ptr_eq(identity, kinds.arena_identity()))
        {
            self.suffix_arena = Some(Arc::clone(kinds.arena_identity()));
        }
        Ok(())
    }

    #[cfg(feature = "placement-dependency-pc")]
    fn check_suffix_arena(
        &self,
        kinds: &crate::backend::wasm_cpu::piece_order_language::PieceOrderLanguageCache,
    ) -> Result<(), WasmExactSearchError> {
        if self
            .suffix_arena
            .as_ref()
            .is_some_and(|identity| !Arc::ptr_eq(identity, kinds.arena_identity()))
        {
            return Err(invalid("wasm_observation_suffix_arena_changed"));
        }
        Ok(())
    }

    #[cfg(all(feature = "placement-dependency-pc", any(test, feature = "parallel")))]
    pub(in crate::backend::wasm_cpu) fn merge_factored(
        &mut self,
        source: Self,
        rebound: &[u32],
        control: &ExecutionControl,
        kinds: &crate::backend::wasm_cpu::piece_order_language::PieceOrderLanguageCache,
    ) -> Result<(), WasmExactSearchError> {
        if !source.roots.is_empty() {
            return self.merge_eager(source, control);
        }
        if !source.factored.has_roots() {
            self.charge(0, control)?;
            return Ok(());
        }
        if !self.roots.is_empty() {
            return Err(invalid("wasm_observation_eager_and_factored_owners_mixed"));
        }
        self.bind_suffix_arena(kinds)?;
        self.prepared_relation = None;
        self.prefix_suffix_scratch.clear();
        self.factored.merge(source.factored, rebound, control)
    }

    /// `kinds` is the same pinned arena that produced every retained prefix
    /// suffix ID. The view owns only demand-driven residual/edge memoization.
    #[cfg(feature = "placement-dependency-pc")]
    pub(in crate::backend::wasm_cpu) fn factored_view<'a>(
        &'a self,
        kinds: &'a crate::backend::wasm_cpu::piece_order_language::PieceOrderLanguageCache,
        control: &ExecutionControl,
    ) -> Result<Option<factored::FactoredPhysicalActionLanguageView<'a>>, WasmExactSearchError>
    {
        if !self.roots.is_empty() {
            return Err(invalid("wasm_observation_eager_and_factored_owners_mixed"));
        }
        self.check_suffix_arena(kinds)?;
        self.factored.view(kinds, control)
    }

    pub(in crate::backend::wasm_cpu) fn view(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<Option<PhysicalActionLanguageView<'_>>, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        #[cfg(feature = "placement-dependency-pc")]
        if self.factored.has_roots() {
            return Err(invalid(
                "wasm_observation_factored_owner_requires_suffix_arena",
            ));
        }
        if self.roots.is_empty() {
            return Ok(None);
        }
        let mut roots = Vec::new();
        roots
            .try_reserve_exact(self.roots.len())
            .map_err(|_| storage())?;
        roots.extend_from_slice(&self.roots);
        let root = self.union_sources(roots, control)?;
        let total_depth = self.nodes[root as usize].remaining_depth;
        Ok(Some(PhysicalActionLanguageView {
            owner: self,
            root,
            total_depth,
        }))
    }

    pub(in crate::backend::wasm_cpu) fn retained_bytes(&self) -> usize {
        let bytes = self.nodes.capacity() * core::mem::size_of::<PhysicalNode>()
            + self.edges.capacity() * core::mem::size_of::<PhysicalActionEdge>()
            + self.hash_heads.capacity() * (core::mem::size_of::<(u64, u32)>() + 1)
            + self.roots.capacity() * core::mem::size_of::<u32>()
            + self.unions.capacity() * (core::mem::size_of::<(Vec<u32>, u32)>() + 1)
            + self
                .unions
                .keys()
                .map(|key| key.capacity() * core::mem::size_of::<u32>())
                .sum::<usize>();
        #[cfg(feature = "placement-dependency-pc")]
        let bytes = bytes
            .saturating_add(self.factored.retained_bytes())
            .saturating_add(self.prefix_suffix_scratch.capacity() * core::mem::size_of::<u32>());
        bytes
    }

    fn node_edges(&self, node: PhysicalNode) -> &[PhysicalActionEdge] {
        let first = node.first_edge as usize;
        &self.edges[first..first + node.edge_count as usize]
    }

    fn charge(
        &mut self,
        steps: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        self.work = self
            .work
            .checked_add(steps)
            .filter(|work| *work <= MAX_WORK)
            .ok_or_else(|| invalid("wasm_observation_physical_work_limit"))?;
        Ok(())
    }

    fn union_sources(
        &mut self,
        mut sources: Vec<u32>,
        control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        self.charge(sources.len().saturating_add(1), control)?;
        sources.sort_unstable();
        sources.dedup();
        if sources.len() == 1 {
            return Ok(sources[0]);
        }
        if let Some(&root) = self.unions.get(sources.as_slice()) {
            return Ok(root);
        }
        let first = sources
            .first()
            .and_then(|&node| self.nodes.get(node as usize))
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        let remaining_depth = first.remaining_depth;
        let mut edge_count = 0_usize;
        for &source in &sources {
            let node = self
                .nodes
                .get(source as usize)
                .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
            if node.remaining_depth != remaining_depth {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
            edge_count = edge_count
                .checked_add(node.edge_count as usize)
                .filter(|count| *count <= MAX_EDGES)
                .ok_or_else(|| invalid("wasm_observation_physical_relation_limit"))?;
        }
        self.charge(edge_count, control)?;
        let mut inputs = Vec::new();
        inputs
            .try_reserve_exact(edge_count)
            .map_err(|_| storage())?;
        for &source in &sources {
            inputs.extend_from_slice(self.node_edges(self.nodes[source as usize]));
        }
        let root = self.intern_action_union(remaining_depth, &mut inputs, control)?;
        let members = self
            .union_members
            .checked_add(sources.capacity())
            .filter(|members| *members <= MAX_UNION_MEMBERS)
            .ok_or_else(|| invalid("wasm_observation_physical_union_limit"))?;
        self.unions.try_reserve(1).map_err(|_| storage())?;
        self.unions.insert(sources, root);
        self.union_members = members;
        Ok(root)
    }

    /// Equal actions share the union of their residuals; differing locks
    /// remain alternatives even when they have the same piece and suffix.
    fn intern_action_union(
        &mut self,
        remaining_depth: u8,
        edges: &mut Vec<PhysicalActionEdge>,
        control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        self.charge(edges.len(), control)?;
        edges.sort_unstable();
        edges.dedup();
        let mut first = 0;
        let mut output = 0;
        while first < edges.len() {
            let action = edges[first].action();
            let mut end = first + 1;
            while end < edges.len() && edges[end].action() == action {
                end += 1;
            }
            let child = if end == first + 1 {
                edges[first].child
            } else {
                let mut children = Vec::new();
                children
                    .try_reserve_exact(end - first)
                    .map_err(|_| storage())?;
                children.extend(edges[first..end].iter().map(|edge| edge.child));
                self.union_sources(children, control)?
            };
            edges[output] = PhysicalActionEdge {
                piece: action.0,
                cells: action.1,
                child,
            };
            output += 1;
            first = end;
        }
        edges.truncate(output);
        self.intern_node(remaining_depth, edges, control)
    }
}

pub(in crate::backend::wasm_cpu) struct PhysicalActionLanguageView<'a> {
    owner: &'a PhysicalActionLanguage,
    root: u32,
    total_depth: u8,
}

impl ObservationPieceLanguage for PhysicalActionLanguageView<'_> {
    fn root(&self) -> u32 {
        self.root
    }

    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        let node = self.owner.nodes.get(node as usize)?;
        Some(ObservationLanguageNode {
            accepting: node.remaining_depth == 0,
            depth: self.total_depth.checked_sub(node.remaining_depth)?,
        })
    }

    fn edge_count(&self, node: u32) -> Option<usize> {
        Some(self.owner.nodes.get(node as usize)?.edge_count as usize)
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        let node = *self.owner.nodes.get(node as usize)?;
        let edge = self.owner.node_edges(node).get(index)?;
        Some((edge.piece, edge.child))
    }

    fn fixed_horizon(&self) -> Option<u8> {
        Some(self.total_depth)
    }
}

fn compact_lock_cells(edge: BuildEdge, width: u8) -> Result<u64, WasmExactSearchError> {
    let (_, piece, _, rotation, x, y, _) = edge.canonical_key();
    let shape = standard_tetromino_registry()
        .get(piece)
        .ok_or_else(|| invalid("wasm_observation_physical_lock_invalid"))?
        .shape(rotation);
    let mut cells = 0_u64;
    for cell in shape.cells() {
        let x = i16::from(x) + i16::from(cell.x());
        let y = i16::from(y) + i16::from(cell.y());
        if x < 0 || x >= i16::from(width) || y < 0 {
            return Err(invalid("wasm_observation_physical_lock_invalid"));
        }
        let bit = u32::try_from(y * i16::from(width) + x)
            .ok()
            .filter(|&bit| bit < u64::BITS)
            .ok_or_else(|| invalid("wasm_observation_physical_lock_invalid"))?;
        cells |= 1_u64 << bit;
    }
    Ok(cells)
}

fn invalid(message: &'static str) -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem(message)
}

fn storage() -> WasmExactSearchError {
    invalid("wasm_observation_physical_storage_unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::{
        piece::rotation::RotationState, probability::probability_value::ProbabilityValue,
    };
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };
    use clearra_supply::{
        bag::BagState,
        hold_automaton::{HoldAutomatonState, SupplyProvenanceId},
        pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
        piece_source::PieceSourceId,
        QueueObservationPolicy,
    };

    use super::super::{QueueObservationPolicyEvaluator, RootedPieceLanguageUnion};
    use crate::backend::wasm_cpu::{
        buildup::BuildOrderNodeSpec, coverage_product::CoverageProductEvaluator,
        piece_order_language::PieceOrderLanguageCache,
    };

    /// A concrete BuildOrderGraph fixture with valid individual lock poses.
    /// Its finite transition relation qualifies the importer, not movement
    /// reachability of a particular tiling or equivalence of two kick rules.
    fn graph_paths(paths: &[&[PieceKind]], first_x: &[i8]) -> BuildOrderGraph {
        assert_eq!(paths.len(), first_x.len());
        let mut nodes = vec![BuildOrderNodeSpec {
            edge_start: 0,
            edge_count: paths.len() as u32,
            depth: 0,
            accepting: false,
        }];
        let mut edges = Vec::new();
        let mut next_root = 1;
        for (path, &x) in paths.iter().zip(first_x) {
            assert!(!path.is_empty());
            edges.push(BuildOrderGraph::edge(
                next_root,
                0,
                path[0],
                RotationState::Zero,
                x,
                0,
                0,
            ));
            next_root += path.len() as u32;
        }
        for path in paths {
            for depth in 1..=path.len() {
                let accepting = depth == path.len();
                nodes.push(BuildOrderNodeSpec {
                    edge_start: edges.len() as u32,
                    edge_count: u32::from(!accepting),
                    depth: depth as u8,
                    accepting,
                });
                if !accepting {
                    edges.push(BuildOrderGraph::edge(
                        nodes.len() as u32,
                        depth as u8,
                        path[depth],
                        RotationState::Zero,
                        0,
                        0,
                        0,
                    ));
                }
            }
        }
        BuildOrderGraph::from_topological_parts(nodes, edges, 0, 0, false).unwrap()
    }

    fn hidden_eighth_queues() -> (MaterializedPatternUniverse, PatternPiecePositionIndex) {
        use PieceKind::{I, J, L, O, S, T, Z};
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(311),
            PatternWeightModelId::new(311),
            vec![vec![I, O, T, S, Z, J, L, I], vec![I, O, T, S, Z, J, L, O]],
            vec![ProbabilityValue::new(0.5).unwrap(); 2],
            2,
            true,
            None,
        )
        .unwrap();
        let index = PatternPiecePositionIndex::compile(&universe).unwrap();
        (universe, index)
    }

    fn policy_count(
        universe: &MaterializedPatternUniverse,
        index: &PatternPiecePositionIndex,
        language: &impl ObservationPieceLanguage,
        policy: QueueObservationPolicy,
    ) -> usize {
        QueueObservationPolicyEvaluator::new(
            universe, index, policy, 0, None, false, false, false, None,
        )
        .unwrap()
        .evaluate(language, &ExecutionControl::default())
        .unwrap()
        .covered_pattern_count
    }

    #[test]
    fn graph_import_keeps_physical_commitments_before_hidden_reveal() {
        use PieceKind::{I, J, L, O, S, T, Z};
        let paths: [&[PieceKind]; 2] = [&[I, O, T, S, Z, J, L, I], &[I, O, T, S, Z, J, L, O]];
        let (universe, index) = hidden_eighth_queues();
        let control = ExecutionControl::default();
        for (first_x, expected) in [([0, 4], 1), ([0, 0], 2)] {
            let graph = graph_paths(&paths, &first_x);
            let mut physical = PhysicalActionLanguage::default();
            physical.register_graph(&graph, 10, &control).unwrap();
            let view = physical.view(&control).unwrap().unwrap();
            assert_eq!(view.fixed_horizon(), Some(8));
            assert_eq!(
                policy_count(
                    &universe,
                    &index,
                    &view,
                    QueueObservationPolicy::VisibleSeven
                ),
                expected,
            );
            // The ordinary exact coverage product is the oracle consumer;
            // the observation evaluator deliberately accepts visible-window
            // policies only.
            let oracle = CoverageProductEvaluator::default()
                .evaluate(
                    &graph,
                    &index,
                    HoldAutomatonState::new(
                        PieceSourceId::new(311),
                        0,
                        None,
                        0,
                        BagState::fresh_standard_7_bag().packed_remainder_key(),
                        SupplyProvenanceId(311),
                    ),
                    false,
                    false,
                    false,
                    false,
                    false,
                    &control,
                )
                .unwrap();
            assert_eq!(oracle.coverage_bits.count_ones(), 2);

            // The previous fallback erased the difference between both
            // graphs: one kind transition could postpone the first lock.
            let mut kinds = PieceOrderLanguageCache::default();
            let root = kinds.canonicalize(&graph).unwrap();
            let kind_union = RootedPieceLanguageUnion::new(&kinds, &[root]).unwrap();
            assert_eq!(
                policy_count(
                    &universe,
                    &index,
                    &kind_union,
                    QueueObservationPolicy::VisibleSeven,
                ),
                2,
            );
        }
    }

    #[test]
    fn eager_shards_rebind_nodes_and_union_only_matching_locks() {
        use PieceKind::{I, J, L, O, S, T, Z};
        let paths: [&[PieceKind]; 2] = [&[I, O, T, S, Z, J, L, I], &[I, O, T, S, Z, J, L, O]];
        let (universe, index) = hidden_eighth_queues();
        let control = ExecutionControl::default();
        for (second_x, expected) in [(0, 2), (4, 1)] {
            let mut merged = PhysicalActionLanguage::default();
            for (path, x) in paths.iter().zip([0, second_x]) {
                let mut shard = PhysicalActionLanguage::default();
                shard
                    .register_graph(&graph_paths(&[*path], &[x]), 10, &control)
                    .unwrap();
                merged.merge_eager(shard, &control).unwrap();
            }
            let view = merged.view(&control).unwrap().unwrap();
            assert_eq!(
                policy_count(
                    &universe,
                    &index,
                    &view,
                    QueueObservationPolicy::VisibleSeven
                ),
                expected,
            );
        }
    }

    #[test]
    fn graph_import_quotients_pose_aliases_without_changing_raw_path_edges() {
        let graph = BuildOrderGraph::from_topological_parts(
            vec![
                BuildOrderNodeSpec {
                    edge_start: 0,
                    edge_count: 2,
                    depth: 0,
                    accepting: false,
                },
                BuildOrderNodeSpec {
                    edge_start: 2,
                    edge_count: 0,
                    depth: 1,
                    accepting: true,
                },
            ],
            [RotationState::Zero, RotationState::Right]
                .into_iter()
                .map(|rotation| BuildOrderGraph::edge(1, 0, PieceKind::O, rotation, 0, 0, 0))
                .collect(),
            0,
            0,
            false,
        )
        .unwrap();
        let control = ExecutionControl::default();
        let mut owner = PhysicalActionLanguage::default();
        owner.register_graph(&graph, 10, &control).unwrap();
        let view = owner.view(&control).unwrap().unwrap();
        assert_eq!(view.edge_count(view.root()), Some(1));
        assert_eq!(graph.edges(graph.root as usize).len(), 2);
    }

    #[test]
    fn graph_import_refuses_invalid_cells_and_mixed_horizons_without_a_root() {
        use PieceKind::{I, O};
        let control = ExecutionControl::default();
        assert!(compact_lock_cells(
            BuildOrderGraph::edge(1, 0, I, RotationState::Zero, 0, 7, 0),
            10,
        )
        .is_err());
        for (graph, width) in [
            (graph_paths(&[&[I]], &[0]), 0),
            (graph_paths(&[&[I]], &[8]), 10),
            (graph_paths(&[&[I], &[I, O]], &[0, 0]), 10),
        ] {
            let mut owner = PhysicalActionLanguage::default();
            assert!(owner.register_graph(&graph, width, &control).is_err());
            assert!(!owner.has_roots());
        }
        let mut owner = PhysicalActionLanguage::default();
        owner
            .register_graph(&graph_paths(&[&[I]], &[0]), 10, &control)
            .unwrap();
        assert!(matches!(
            owner.register_graph(&graph_paths(&[&[I]], &[0]), 8, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_width_mismatch"
            )),
        ));
        control.cancellation.handle().cancel();
        assert!(matches!(
            owner.register_graph(&graph_paths(&[&[I]], &[0]), 10, &control),
            Err(WasmExactSearchError::Cancelled),
        ));
    }

    #[test]
    fn identical_actions_union_continuations_but_distinct_locks_do_not() {
        let control = ExecutionControl::default();
        let mut owner = PhysicalActionLanguage::default();
        let terminal = owner.intern_node(0, &mut Vec::new(), &control).unwrap();
        let suffix_a = owner
            .intern_node(
                1,
                &mut vec![PhysicalActionEdge::new(PieceKind::O, 0x33, terminal).unwrap()],
                &control,
            )
            .unwrap();
        let suffix_b = owner
            .intern_node(
                1,
                &mut vec![PhysicalActionEdge::new(PieceKind::T, 0x72, terminal).unwrap()],
                &control,
            )
            .unwrap();
        let mut roots = Vec::new();
        for (cells, child) in [(0x0f, suffix_a), (0x0f, suffix_b), (0xf0, suffix_a)] {
            let root = owner
                .intern_node(
                    2,
                    &mut vec![PhysicalActionEdge::new(PieceKind::I, cells, child).unwrap()],
                    &control,
                )
                .unwrap();
            owner.insert_root(root).unwrap();
            roots.push(root);
        }
        owner.insert_root(roots[0]).unwrap();
        assert_eq!(owner.roots.len(), 3);
        let view = owner.view(&control).unwrap().unwrap();
        assert_eq!(view.edge_count(view.root()), Some(2));
        let first = view.edge(view.root(), 0).unwrap();
        let second = view.edge(view.root(), 1).unwrap();
        assert_eq!(first.0, second.0);
        assert_eq!(view.edge_count(first.1), Some(2));
        assert_eq!(view.edge_count(second.1), Some(1));
        assert_eq!(view.node(first.1).unwrap().depth, 1);
        assert_eq!(view.node(terminal).unwrap().depth, 2);
    }

    #[test]
    fn invalid_edges_and_bounded_refusals_are_not_empty_languages() {
        let control = ExecutionControl::default();
        let mut owner = PhysicalActionLanguage::default();
        assert!(PhysicalActionEdge::new(PieceKind::I, 0x07, 0).is_err());
        let terminal = owner.intern_node(0, &mut Vec::new(), &control).unwrap();
        assert!(owner
            .intern_node(
                2,
                &mut vec![PhysicalActionEdge::new(PieceKind::I, 0x0f, terminal).unwrap()],
                &control,
            )
            .is_err());
        owner.insert_root(terminal).unwrap();
        owner.work = MAX_WORK;
        assert!(matches!(
            owner.view(&control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        control.cancellation.handle().cancel();
        assert!(matches!(
            owner.view(&control),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert!(owner.has_roots());
    }
}
