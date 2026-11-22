//! Exact union of complete physical lock languages for one PC request.
//!
//! A key is a piece kind and its four occupied cells in the current physical
//! board. Equal keys are merged only along a common physical action history.
//! Thus a policy can postpone choosing a tiling, but cannot postpone choosing
//! a physical lock until hidden supply has been revealed. Input nodes have
//! already been proved by the dependency relation; this owner performs no
//! movement or BuildUp search.

use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};

use super::{piece_code, ObservationLanguageNode, ObservationPieceLanguage, WasmExactSearchError};

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
}

impl PhysicalActionLanguage {
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
        !self.roots.is_empty()
    }

    pub(in crate::backend::wasm_cpu) fn view(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<Option<PhysicalActionLanguageView<'_>>, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
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
        self.nodes.capacity() * core::mem::size_of::<PhysicalNode>()
            + self.edges.capacity() * core::mem::size_of::<PhysicalActionEdge>()
            + self.hash_heads.capacity() * (core::mem::size_of::<(u64, u32)>() + 1)
            + self.roots.capacity() * core::mem::size_of::<u32>()
            + self.unions.capacity() * (core::mem::size_of::<(Vec<u32>, u32)>() + 1)
            + self
                .unions
                .keys()
                .map(|key| key.capacity() * core::mem::size_of::<u32>())
                .sum::<usize>()
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
        inputs.sort_unstable();
        inputs.dedup();
        let mut edges = Vec::new();
        edges
            .try_reserve_exact(inputs.len())
            .map_err(|_| storage())?;
        let mut first = 0;
        while first < inputs.len() {
            let action = inputs[first].action();
            let mut end = first + 1;
            while end < inputs.len() && inputs[end].action() == action {
                end += 1;
            }
            let mut children = Vec::new();
            children
                .try_reserve_exact(end - first)
                .map_err(|_| storage())?;
            children.extend(inputs[first..end].iter().map(|edge| edge.child));
            let child = self.union_sources(children, control)?;
            edges.push(PhysicalActionEdge {
                piece: action.0,
                cells: action.1,
                child,
            });
            first = end;
        }
        let root = self.intern_node(remaining_depth, &mut edges, control)?;
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
