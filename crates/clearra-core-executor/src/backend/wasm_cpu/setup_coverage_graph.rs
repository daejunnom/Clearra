use clearra_core_domain::piece::piece_kind::PieceKind;

use super::{
    mix_digest,
    queue_observation_policy::{ObservationLanguageNode, ObservationPieceLanguage},
    setup_partial_build::{PartialBuildGraph, PartialBuildNode},
    WasmExactSearchError,
};

pub(super) const EMPTY_COVERAGE_REFERENCE: u32 = u32::MAX;
const NO_SHAPE_INDEX: u32 = u32::MAX;
const INITIAL_BUCKET_COUNT: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub(super) struct SetupCoverageEdge(u32);

impl SetupCoverageEdge {
    const CHILD_BITS: u32 = 29;
    const CHILD_MASK: u32 = (1_u32 << Self::CHILD_BITS) - 1;

    pub(super) fn new(child: u32, piece: PieceKind) -> Result<Self, WasmExactSearchError> {
        let piece_index = piece_index(piece);
        if child > Self::CHILD_MASK {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_child_index_overflow",
            ));
        }
        Ok(Self((u32::from(piece_index) << Self::CHILD_BITS) | child))
    }

    pub(super) const fn child(self) -> u32 {
        self.0 & Self::CHILD_MASK
    }

    pub(super) fn with_child(self, child: u32) -> Result<Self, WasmExactSearchError> {
        if child > Self::CHILD_MASK {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_child_index_overflow",
            ));
        }
        Ok(Self((self.0 & !Self::CHILD_MASK) | child))
    }

    pub(super) const fn piece_code(self) -> u8 {
        ((self.0 >> Self::CHILD_BITS) as u8) + 1
    }

    pub(super) const fn raw(self) -> u32 {
        self.0
    }

    pub(super) fn from_raw(raw: u32) -> Result<Self, WasmExactSearchError> {
        let piece_code = (raw >> Self::CHILD_BITS) as u8 + 1;
        if !(1..=7).contains(&piece_code) {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_wire_piece_code_invalid",
            ));
        }
        Ok(Self(raw))
    }
}

const _: () = assert!(core::mem::size_of::<SetupCoverageEdge>() == 4);

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub(super) struct SetupCoverageNode {
    pub(super) edge_start: u32,
    next_same_bucket: u32,
    shape_index: u32,
    pub(super) edge_count: u16,
    pub(super) depth: u8,
    flags: u8,
}

const NODE_ACCEPTING: u8 = 1;

impl SetupCoverageNode {
    pub(super) const fn accepting(self) -> bool {
        self.flags & NODE_ACCEPTING != 0
    }

    pub(super) const fn shape_index(self) -> Option<u32> {
        if self.shape_index == NO_SHAPE_INDEX {
            None
        } else {
            Some(self.shape_index)
        }
    }

    pub(super) const fn flags(self) -> u8 {
        self.flags
    }

    pub(super) fn from_wire(
        edge_start: u32,
        edge_count: u16,
        shape_index: u32,
        depth: u8,
        flags: u8,
    ) -> Result<Self, WasmExactSearchError> {
        if flags & !NODE_ACCEPTING != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_wire_flags_invalid",
            ));
        }
        Ok(Self {
            edge_start,
            next_same_bucket: EMPTY_COVERAGE_REFERENCE,
            shape_index,
            edge_count,
            depth,
            flags,
        })
    }
}

const _: () = assert!(core::mem::size_of::<SetupCoverageNode>() == 16);

pub(super) struct SetupCoverageGraph {
    pub(super) nodes: Vec<SetupCoverageNode>,
    pub(super) edges: Vec<SetupCoverageEdge>,
    pub(super) root: u32,
    source_classes: Vec<u32>,
}

impl SetupCoverageGraph {
    pub(super) fn compile(source: &PartialBuildGraph) -> Result<Self, WasmExactSearchError> {
        SetupCoverageGraphCompiler::new(source.nodes.len())?.compile_prefix(source)
    }

    pub(super) fn from_wire_parts(
        nodes: Vec<SetupCoverageNode>,
        edges: Vec<SetupCoverageEdge>,
        root: u32,
    ) -> Result<Self, WasmExactSearchError> {
        if root as usize >= nodes.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_wire_root_out_of_range",
            ));
        }
        for node in &nodes {
            let start = node.edge_start as usize;
            let end = start.checked_add(node.edge_count as usize).ok_or(
                WasmExactSearchError::InvalidProblem(
                    "setup_coverage_graph_wire_edge_range_overflow",
                ),
            )?;
            if end > edges.len() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_coverage_graph_wire_edge_range_out_of_bounds",
                ));
            }
        }
        if edges
            .iter()
            .any(|edge| edge.child() as usize >= nodes.len())
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_wire_child_out_of_range",
            ));
        }
        Ok(Self {
            nodes,
            edges,
            root,
            source_classes: Vec::new(),
        })
    }

    pub(super) fn source_class(&self, source_node: u32) -> Option<u32> {
        self.source_classes.get(source_node as usize).copied()
    }

    pub(super) fn compile_from_suffix(
        source: &PartialBuildGraph,
        terminal_classes: Vec<u32>,
        interner: SetupCoverageInterner,
    ) -> Result<Self, WasmExactSearchError> {
        if terminal_classes.len() != source.nodes.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_suffix_terminal_class_count_mismatch",
            ));
        }
        SetupCoverageGraphCompiler::from_interner(interner, terminal_classes)?
            .compile_prefix(source)
    }
}

pub(super) struct SetupTargetBuildLanguage<'a> {
    graph: &'a SetupCoverageGraph,
    target_shape: u32,
}

impl<'a> SetupTargetBuildLanguage<'a> {
    pub const fn new(graph: &'a SetupCoverageGraph, target_shape: u32) -> Self {
        Self {
            graph,
            target_shape,
        }
    }

    fn is_target(&self, node: SetupCoverageNode) -> bool {
        node.shape_index() == Some(self.target_shape)
    }
}

impl ObservationPieceLanguage for SetupTargetBuildLanguage<'_> {
    fn root(&self) -> u32 {
        self.graph.root
    }

    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        self.graph
            .nodes
            .get(node as usize)
            .copied()
            .map(|node| ObservationLanguageNode {
                accepting: self.is_target(node),
                depth: node.depth,
            })
    }

    fn edge_count(&self, node: u32) -> Option<usize> {
        let node = self.graph.nodes.get(node as usize).copied()?;
        Some(if self.is_target(node) {
            0
        } else {
            usize::from(node.edge_count)
        })
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        let node = self.graph.nodes.get(node as usize).copied()?;
        if self.is_target(node) || index >= usize::from(node.edge_count) {
            return None;
        }
        let edge = self.graph.edges[node.edge_start as usize + index];
        Some((edge.piece_code(), edge.child()))
    }
}

pub(super) struct SetupTargetJointLanguage<'a> {
    graph: &'a SetupCoverageGraph,
    target_shape: u32,
}

impl<'a> SetupTargetJointLanguage<'a> {
    pub const fn new(graph: &'a SetupCoverageGraph, target_shape: u32) -> Self {
        Self {
            graph,
            target_shape,
        }
    }

    fn encode(&self, node: u32, seen_target: bool) -> Option<u32> {
        node.checked_mul(2)?.checked_add(u32::from(seen_target))
    }

    fn decode(&self, state: u32) -> Option<(u32, bool)> {
        let node = state / 2;
        ((node as usize) < self.graph.nodes.len()).then_some((node, state & 1 != 0))
    }
}

impl ObservationPieceLanguage for SetupTargetJointLanguage<'_> {
    fn root(&self) -> u32 {
        let root = self.graph.root;
        let seen = self.graph.nodes[root as usize].shape_index() == Some(self.target_shape);
        self.encode(root, seen)
            .expect("setup coverage graph identity fits u32")
    }

    fn node(&self, state: u32) -> Option<ObservationLanguageNode> {
        let (node_index, seen_target) = self.decode(state)?;
        let node = self.graph.nodes[node_index as usize];
        Some(ObservationLanguageNode {
            accepting: node.accepting() && seen_target,
            depth: node.depth,
        })
    }

    fn edge_count(&self, state: u32) -> Option<usize> {
        let (node_index, _) = self.decode(state)?;
        Some(usize::from(
            self.graph.nodes[node_index as usize].edge_count,
        ))
    }

    fn edge(&self, state: u32, index: usize) -> Option<(u8, u32)> {
        let (node_index, seen_target) = self.decode(state)?;
        let node = self.graph.nodes[node_index as usize];
        if index >= usize::from(node.edge_count) {
            return None;
        }
        let edge = self.graph.edges[node.edge_start as usize + index];
        let child = edge.child();
        let child_seen = seen_target
            || self.graph.nodes[child as usize].shape_index() == Some(self.target_shape);
        Some((edge.piece_code(), self.encode(child, child_seen)?))
    }
}

pub(super) struct SetupCoverageInterner {
    nodes: Vec<SetupCoverageNode>,
    edges: Vec<SetupCoverageEdge>,
    bucket_heads: Vec<u32>,
    interning_disabled: bool,
}

impl SetupCoverageInterner {
    pub(super) const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            bucket_heads: Vec::new(),
            interning_disabled: false,
        }
    }

    pub(super) fn intern_language_node(
        &mut self,
        depth: u8,
        accepting: bool,
        edges: &mut Vec<SetupCoverageEdge>,
    ) -> Result<u32, WasmExactSearchError> {
        edges.sort_unstable();
        edges.dedup();
        self.intern_parts(depth, NO_SHAPE_INDEX, accepting, edges)
    }

    fn intern_node(
        &mut self,
        source: PartialBuildNode,
        edges: &[SetupCoverageEdge],
    ) -> Result<u32, WasmExactSearchError> {
        self.intern_parts(
            source.depth,
            source.shape_index().unwrap_or(NO_SHAPE_INDEX),
            source.accepting(),
            edges,
        )
    }

    fn intern_shape_alias(
        &mut self,
        depth: u8,
        shape_index: u32,
        source_class: u32,
    ) -> Result<u32, WasmExactSearchError> {
        let source =
            *self
                .nodes
                .get(source_class as usize)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_coverage_graph_shape_source_out_of_range",
                ))?;
        if source.depth != depth {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_shape_source_depth_mismatch",
            ));
        }
        let edge_start = source.edge_start as usize;
        let edge_end = edge_start + source.edge_count as usize;
        let hash = coverage_node_hash(
            depth,
            shape_index,
            source.accepting(),
            &self.edges[edge_start..edge_end],
        );
        self.ensure_buckets();
        if !self.interning_disabled {
            let bucket = hash as usize & (self.bucket_heads.len() - 1);
            let mut reference = self.bucket_heads[bucket];
            while reference != EMPTY_COVERAGE_REFERENCE {
                let node = self.nodes[reference as usize];
                if node.depth == depth
                    && node.shape_index == shape_index
                    && node.flags == source.flags
                    && self.node_edges(node) == &self.edges[edge_start..edge_end]
                {
                    return Ok(reference);
                }
                reference = node.next_same_bucket;
            }
        }

        let reference = u32::try_from(self.nodes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_coverage_graph_node_index_overflow")
        })?;
        if reference > SetupCoverageEdge::CHILD_MASK {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_node_index_overflow",
            ));
        }
        self.nodes.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_coverage_graph_node_storage_unavailable")
        })?;
        let next_same_bucket = if self.interning_disabled {
            EMPTY_COVERAGE_REFERENCE
        } else {
            self.bucket_heads[hash as usize & (self.bucket_heads.len() - 1)]
        };
        self.nodes.push(SetupCoverageNode {
            edge_start: source.edge_start,
            next_same_bucket,
            shape_index,
            edge_count: source.edge_count,
            depth,
            flags: source.flags,
        });
        if !self.interning_disabled {
            let bucket = hash as usize & (self.bucket_heads.len() - 1);
            self.bucket_heads[bucket] = reference;
            self.grow_buckets_if_needed();
        }
        Ok(reference)
    }

    fn intern_parts(
        &mut self,
        depth: u8,
        shape_index: u32,
        accepting: bool,
        edges: &[SetupCoverageEdge],
    ) -> Result<u32, WasmExactSearchError> {
        self.ensure_buckets();
        let flags = u8::from(accepting) * NODE_ACCEPTING;
        let hash = coverage_node_hash(depth, shape_index, accepting, edges);
        if !self.interning_disabled {
            let bucket = hash as usize & (self.bucket_heads.len() - 1);
            let mut reference = self.bucket_heads[bucket];
            while reference != EMPTY_COVERAGE_REFERENCE {
                let node = self.nodes[reference as usize];
                if node.depth == depth
                    && node.shape_index == shape_index
                    && node.flags == flags
                    && self.node_edges(node) == edges
                {
                    return Ok(reference);
                }
                reference = node.next_same_bucket;
            }
        }

        let reference = u32::try_from(self.nodes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_coverage_graph_node_index_overflow")
        })?;
        if reference > SetupCoverageEdge::CHILD_MASK {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_node_index_overflow",
            ));
        }
        let edge_start = u32::try_from(self.edges.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_coverage_graph_edge_index_overflow")
        })?;
        let edge_count = u16::try_from(edges.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_coverage_graph_edge_count_overflow")
        })?;
        self.nodes.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_coverage_graph_node_storage_unavailable")
        })?;
        self.edges.try_reserve_exact(edges.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_coverage_graph_edge_storage_unavailable")
        })?;
        let next_same_bucket = if self.interning_disabled {
            EMPTY_COVERAGE_REFERENCE
        } else {
            self.bucket_heads[hash as usize & (self.bucket_heads.len() - 1)]
        };
        self.edges.extend_from_slice(edges);
        self.nodes.push(SetupCoverageNode {
            edge_start,
            next_same_bucket,
            shape_index,
            edge_count,
            depth,
            flags,
        });
        if !self.interning_disabled {
            let bucket = hash as usize & (self.bucket_heads.len() - 1);
            self.bucket_heads[bucket] = reference;
            self.grow_buckets_if_needed();
        }
        Ok(reference)
    }

    fn node_edges(&self, node: SetupCoverageNode) -> &[SetupCoverageEdge] {
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
            .resize(INITIAL_BUCKET_COUNT, EMPTY_COVERAGE_REFERENCE);
    }

    fn grow_buckets_if_needed(&mut self) {
        if self.interning_disabled
            || self.nodes.len().saturating_mul(4) < self.bucket_heads.len().saturating_mul(3)
        {
            return;
        }
        let Some(new_count) = self.bucket_heads.len().checked_mul(2) else {
            self.interning_disabled = true;
            return;
        };
        let mut replacement = Vec::new();
        if replacement.try_reserve_exact(new_count).is_err() {
            self.interning_disabled = true;
            return;
        }
        replacement.resize(new_count, EMPTY_COVERAGE_REFERENCE);
        for reference in 0..self.nodes.len() {
            let node = self.nodes[reference];
            let hash = coverage_node_hash(
                node.depth,
                node.shape_index,
                node.accepting(),
                self.node_edges(node),
            );
            let bucket = hash as usize & (new_count - 1);
            self.nodes[reference].next_same_bucket = replacement[bucket];
            replacement[bucket] = reference as u32;
        }
        self.bucket_heads = replacement;
    }
}

struct SetupCoverageGraphCompiler {
    interner: SetupCoverageInterner,
    source_classes: Vec<u32>,
    edge_scratch: Vec<SetupCoverageEdge>,
}

impl SetupCoverageGraphCompiler {
    fn new(source_node_count: usize) -> Result<Self, WasmExactSearchError> {
        Self::from_interner(
            SetupCoverageInterner::new(),
            vec![EMPTY_COVERAGE_REFERENCE; source_node_count],
        )
    }

    fn from_interner(
        interner: SetupCoverageInterner,
        source_classes: Vec<u32>,
    ) -> Result<Self, WasmExactSearchError> {
        Ok(Self {
            interner,
            source_classes,
            edge_scratch: Vec::new(),
        })
    }

    fn compile_prefix(
        mut self,
        source: &PartialBuildGraph,
    ) -> Result<SetupCoverageGraph, WasmExactSearchError> {
        if self.source_classes.len() != source.nodes.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_class_count_mismatch",
            ));
        }
        for source_index in (0..source.nodes.len()).rev() {
            let node = source.nodes[source_index];
            if !node.live() {
                continue;
            }
            let existing_class = self.source_classes[source_index];
            if existing_class != EMPTY_COVERAGE_REFERENCE {
                if let Some(shape_index) = node.shape_index() {
                    self.source_classes[source_index] = self.interner.intern_shape_alias(
                        node.depth,
                        shape_index,
                        existing_class,
                    )?;
                }
                continue;
            }
            self.edge_scratch.clear();
            let edge_start = node.edge_start as usize;
            let edge_end = edge_start + node.edge_count as usize;
            self.edge_scratch
                .try_reserve(edge_end.saturating_sub(edge_start))
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "setup_coverage_graph_edge_scratch_unavailable",
                    )
                })?;
            for edge in &source.edges[edge_start..edge_end] {
                if !source.nodes[edge.to as usize].live() {
                    continue;
                }
                let child = self.source_classes[edge.to as usize];
                if child == EMPTY_COVERAGE_REFERENCE {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "setup_coverage_graph_not_topological",
                    ));
                }
                self.edge_scratch
                    .push(SetupCoverageEdge::new(child, edge.piece)?);
            }
            self.edge_scratch.sort_unstable();
            self.edge_scratch.dedup();
            self.source_classes[source_index] =
                self.interner.intern_node(node, &self.edge_scratch)?;
        }
        let root = self
            .source_classes
            .get(source.root as usize)
            .copied()
            .filter(|reference| *reference != EMPTY_COVERAGE_REFERENCE)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_coverage_graph_root_missing",
            ))?;
        Ok(SetupCoverageGraph {
            nodes: self.interner.nodes,
            edges: self.interner.edges,
            root,
            source_classes: self.source_classes,
        })
    }
}

fn coverage_node_hash(
    depth: u8,
    shape_index: u32,
    accepting: bool,
    edges: &[SetupCoverageEdge],
) -> u64 {
    let mut hash = mix_digest(0, u64::from(depth));
    hash = mix_digest(hash, u64::from(shape_index));
    hash = mix_digest(hash, u64::from(accepting));
    hash = mix_digest(hash, edges.len() as u64);
    for edge in edges {
        hash = mix_digest(hash, u64::from(edge.0));
    }
    hash
}

const fn piece_index(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

// This module owns one exact responsibility: removing language-equivalent
// setup states from the coverage hot path without changing the evidence graph.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_class_shape_alias_preserves_language_and_candidate_identity() {
        let mut interner = SetupCoverageInterner::new();
        let mut edges = Vec::new();
        let suffix = interner
            .intern_language_node(4, true, &mut edges)
            .expect("suffix");
        let alias = interner
            .intern_shape_alias(4, 17, suffix)
            .expect("shape alias");

        assert_ne!(alias, suffix);
        assert_eq!(interner.nodes[alias as usize].shape_index(), Some(17));
        assert!(interner.nodes[alias as usize].accepting());
        assert_eq!(
            interner.node_edges(interner.nodes[alias as usize]),
            interner.node_edges(interner.nodes[suffix as usize])
        );
    }
}
