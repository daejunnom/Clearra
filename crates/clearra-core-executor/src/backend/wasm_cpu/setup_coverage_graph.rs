use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};

use super::{
    mix_digest,
    queue_observation_policy::{ObservationLanguageNode, ObservationPieceLanguage},
    setup_partial_build::{PartialBuildGraph, PartialBuildNode},
    WasmExactSearchError,
};

pub(super) const EMPTY_COVERAGE_REFERENCE: u32 = u32::MAX;
const NO_SHAPE_INDEX: u32 = u32::MAX;
const INITIAL_BUCKET_COUNT: usize = 1024;
const SETUP_JOINT_SHARED_MEMO_MIN_DEPTH: u8 = 7;

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

    fn edges_for_node(&self, node: SetupCoverageNode) -> &[SetupCoverageEdge] {
        let start = node.edge_start as usize;
        &self.edges[start..start + usize::from(node.edge_count)]
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
    reachability: &'a SetupTargetLanguageReachability,
    memo_classes: Vec<u32>,
}

impl<'a> SetupTargetBuildLanguage<'a> {
    pub fn compile(
        graph: &'a SetupCoverageGraph,
        target_shape: u32,
        reachability: &'a SetupTargetLanguageReachability,
        control: &ExecutionControl,
    ) -> Result<Self, WasmExactSearchError> {
        let mut interner = SetupCoverageInterner::new();
        let mut classes = target_language_class_storage(graph.nodes.len())?;
        let mut edge_scratch = Vec::new();
        let max_depth = graph.nodes.iter().map(|node| node.depth).max().unwrap_or(0);
        let mut work = 0_usize;
        for depth in (0..=max_depth).rev() {
            for (node_index, node) in graph.nodes.iter().copied().enumerate() {
                if node.depth != depth {
                    continue;
                }
                poll_target_reachability_cancellation(control, &mut work)?;
                edge_scratch.clear();
                let is_target = node.shape_index() == Some(target_shape);
                if !is_target {
                    for edge in graph.edges_for_node(node).iter().copied() {
                        poll_target_reachability_cancellation(control, &mut work)?;
                        let child = edge.child() as usize;
                        if !reachability.can_build_from(child) {
                            continue;
                        }
                        edge_scratch
                            .push(edge.with_child(target_language_child_class(&classes, child)?)?);
                    }
                }
                classes[node_index] =
                    interner.intern_language_node(node.depth, is_target, &mut edge_scratch)?;
            }
        }
        drop(interner);
        Ok(Self {
            graph,
            target_shape,
            reachability,
            memo_classes: classes,
        })
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
            self.graph
                .edges_for_node(node)
                .iter()
                .filter(|edge| self.reachability.can_build_from(edge.child() as usize))
                .count()
        })
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        let node = self.graph.nodes.get(node as usize).copied()?;
        if self.is_target(node) {
            return None;
        }
        let edge = self
            .graph
            .edges_for_node(node)
            .iter()
            .copied()
            .filter(|edge| self.reachability.can_build_from(edge.child() as usize))
            .nth(index)?;
        Some((edge.piece_code(), edge.child()))
    }

    fn memo_class(&self, node: u32) -> Option<u32> {
        self.memo_classes.get(node as usize).copied()
    }
}

const CAN_BUILD_FROM: u8 = 1 << 0;
const CAN_ACCEPT_FROM: u8 = 1 << 1;
const CAN_JOINT_FROM: u8 = 1 << 2;
const TARGET_REACHABILITY_CANCEL_MASK: usize = 0xfff;

/// Exact target-specific liveness for the two observation languages.
///
/// The coverage graph is a depth-increasing DAG. A branch which cannot reach
/// the relevant accepting condition has zero value for every possible queue,
/// so hiding that branch preserves the policy oracle while avoiding its
/// product with the observation trie.
pub(super) struct SetupTargetLanguageReachability {
    flags: Vec<u8>,
}

impl SetupTargetLanguageReachability {
    pub fn compile(
        graph: &SetupCoverageGraph,
        target_shape: u32,
        control: &ExecutionControl,
    ) -> Result<Self, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let mut flags = Vec::new();
        flags.try_reserve_exact(graph.nodes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_target_reachability_storage_unavailable")
        })?;
        flags.resize(graph.nodes.len(), 0);
        let max_depth = graph.nodes.iter().map(|node| node.depth).max().unwrap_or(0);
        let mut work = 0_usize;

        // Node references are interned and need not be ordered by index. Depth
        // is the stable topological rank, so descendants are complete before
        // their parents regardless of compilation or wire origin.
        for depth in (0..=max_depth).rev() {
            for (node_index, node) in graph.nodes.iter().copied().enumerate() {
                if node.depth != depth {
                    continue;
                }
                poll_target_reachability_cancellation(control, &mut work)?;
                let mut descendant_flags = 0_u8;
                for edge in graph.edges_for_node(node) {
                    poll_target_reachability_cancellation(control, &mut work)?;
                    let child_index = edge.child() as usize;
                    let child = graph.nodes.get(child_index).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "setup_target_reachability_child_out_of_range",
                        ),
                    )?;
                    if child.depth <= node.depth {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "setup_target_reachability_graph_not_depth_increasing",
                        ));
                    }
                    descendant_flags |= flags[child_index];
                }

                let is_target = node.shape_index() == Some(target_shape);
                let mut node_flags = descendant_flags;
                if is_target {
                    node_flags |= CAN_BUILD_FROM;
                }
                if node.accepting() {
                    node_flags |= CAN_ACCEPT_FROM;
                }
                if is_target && node_flags & CAN_ACCEPT_FROM != 0 {
                    node_flags |= CAN_JOINT_FROM;
                }
                flags[node_index] = node_flags;
            }
        }
        Ok(Self { flags })
    }

    fn has(&self, node: usize, flag: u8) -> bool {
        self.flags
            .get(node)
            .is_some_and(|node_flags| node_flags & flag != 0)
    }

    fn can_build_from(&self, node: usize) -> bool {
        self.has(node, CAN_BUILD_FROM)
    }

    fn can_joint_from(&self, node: usize) -> bool {
        self.has(node, CAN_JOINT_FROM)
    }
}

fn poll_target_reachability_cancellation(
    control: &ExecutionControl,
    work: &mut usize,
) -> Result<(), WasmExactSearchError> {
    *work = work.wrapping_add(1);
    if *work & TARGET_REACHABILITY_CANCEL_MASK == 0 && control.is_cancelled() {
        Err(WasmExactSearchError::Cancelled)
    } else {
        Ok(())
    }
}

/// Target-independent suffix of every setup joint language after the target
/// shape has been observed. Its classes are compiled once so observation
/// policy scores for accepting continuations can be reused across targets.
pub(super) struct SetupJointSeenLanguageClasses {
    can_accept_from: Vec<bool>,
    memo_classes: Vec<u32>,
    class_count: u32,
}

impl SetupJointSeenLanguageClasses {
    pub fn compile(
        graph: &SetupCoverageGraph,
        control: &ExecutionControl,
    ) -> Result<Self, WasmExactSearchError> {
        let mut interner = SetupCoverageInterner::new();
        let mut classes = target_language_class_storage(graph.nodes.len())?;
        let mut can_accept_from = Vec::new();
        can_accept_from
            .try_reserve_exact(graph.nodes.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_joint_seen_reachability_storage_unavailable",
                )
            })?;
        can_accept_from.resize(graph.nodes.len(), false);
        let mut edge_scratch = Vec::new();
        let max_depth = graph.nodes.iter().map(|node| node.depth).max().unwrap_or(0);
        let mut work = 0_usize;
        for depth in (0..=max_depth).rev() {
            for (node_index, node) in graph.nodes.iter().copied().enumerate() {
                if node.depth != depth {
                    continue;
                }
                poll_target_reachability_cancellation(control, &mut work)?;
                edge_scratch.clear();
                let mut can_accept = node.accepting();
                for edge in graph.edges_for_node(node).iter().copied() {
                    poll_target_reachability_cancellation(control, &mut work)?;
                    let child = edge.child() as usize;
                    let child_node =
                        graph
                            .nodes
                            .get(child)
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "setup_joint_seen_child_out_of_range",
                            ))?;
                    if child_node.depth <= node.depth {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "setup_joint_seen_graph_not_depth_increasing",
                        ));
                    }
                    if !can_accept_from[child] {
                        continue;
                    }
                    can_accept = true;
                    edge_scratch
                        .push(edge.with_child(target_language_child_class(&classes, child)?)?);
                }
                can_accept_from[node_index] = can_accept;
                classes[node_index] = interner.intern_language_node(
                    node.depth,
                    node.accepting(),
                    &mut edge_scratch,
                )?;
            }
        }
        let class_count = u32::try_from(interner.nodes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_joint_seen_class_count_overflow")
        })?;
        if class_count > SetupCoverageEdge::CHILD_MASK {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_joint_seen_class_count_overflow",
            ));
        }
        Ok(Self {
            can_accept_from,
            memo_classes: classes,
            class_count,
        })
    }

    fn can_accept_from(&self, node: usize) -> bool {
        self.can_accept_from.get(node).copied().unwrap_or(false)
    }

    fn memo_class(&self, node: usize) -> Option<u32> {
        self.memo_classes.get(node).copied()
    }

    fn unseen_class(&self, local_class: u32) -> Result<u32, WasmExactSearchError> {
        self.class_count
            .checked_add(local_class)
            .filter(|class| *class <= SetupCoverageEdge::CHILD_MASK)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_joint_unseen_class_count_overflow",
            ))
    }
}

pub(super) struct SetupTargetJointLanguage<'a> {
    graph: &'a SetupCoverageGraph,
    target_shape: u32,
    reachability: &'a SetupTargetLanguageReachability,
    seen: &'a SetupJointSeenLanguageClasses,
    unseen_memo_classes: Vec<u32>,
    memo_domain: u64,
}

impl<'a> SetupTargetJointLanguage<'a> {
    pub fn compile(
        graph: &'a SetupCoverageGraph,
        target_shape: u32,
        reachability: &'a SetupTargetLanguageReachability,
        seen: &'a SetupJointSeenLanguageClasses,
        memo_domain: u64,
        control: &ExecutionControl,
    ) -> Result<Self, WasmExactSearchError> {
        let mut interner = SetupCoverageInterner::new();
        let mut unseen_classes = target_language_class_storage(graph.nodes.len())?;
        let mut edge_scratch = Vec::new();
        let max_depth = graph.nodes.iter().map(|node| node.depth).max().unwrap_or(0);
        let mut work = 0_usize;
        for depth in (0..=max_depth).rev() {
            for (node_index, node) in graph.nodes.iter().copied().enumerate() {
                if node.depth != depth {
                    continue;
                }
                poll_target_reachability_cancellation(control, &mut work)?;

                edge_scratch.clear();
                for edge in graph.edges_for_node(node).iter().copied() {
                    poll_target_reachability_cancellation(control, &mut work)?;
                    let child = edge.child() as usize;
                    if !reachability.can_joint_from(child) {
                        continue;
                    }
                    let child_class = if graph.nodes[child].shape_index() == Some(target_shape) {
                        seen.memo_class(child)
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "setup_joint_seen_class_missing",
                            ))?
                    } else {
                        target_language_child_class(&unseen_classes, child)?
                    };
                    edge_scratch.push(edge.with_child(child_class)?);
                }
                let local_class =
                    interner.intern_language_node(node.depth, false, &mut edge_scratch)?;
                unseen_classes[node_index] = seen.unseen_class(local_class)?;
            }
        }
        drop(interner);
        Ok(Self {
            graph,
            target_shape,
            reachability,
            seen,
            unseen_memo_classes: unseen_classes,
            memo_domain,
        })
    }

    fn encode(&self, node: u32, seen_target: bool) -> Option<u32> {
        node.checked_mul(2)?.checked_add(u32::from(seen_target))
    }

    fn decode(&self, state: u32) -> Option<(u32, bool)> {
        let node = state / 2;
        ((node as usize) < self.graph.nodes.len()).then_some((node, state & 1 != 0))
    }

    fn keeps_child(&self, child: u32, seen_target: bool) -> bool {
        if seen_target {
            self.seen.can_accept_from(child as usize)
        } else {
            self.reachability.can_joint_from(child as usize)
        }
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
        let (node_index, seen_target) = self.decode(state)?;
        let node = self.graph.nodes[node_index as usize];
        Some(
            self.graph
                .edges_for_node(node)
                .iter()
                .filter(|edge| self.keeps_child(edge.child(), seen_target))
                .count(),
        )
    }

    fn edge(&self, state: u32, index: usize) -> Option<(u8, u32)> {
        let (node_index, seen_target) = self.decode(state)?;
        let node = self.graph.nodes[node_index as usize];
        let edge = self
            .graph
            .edges_for_node(node)
            .iter()
            .copied()
            .filter(|edge| self.keeps_child(edge.child(), seen_target))
            .nth(index)?;
        let child = edge.child();
        let child_seen = seen_target
            || self.graph.nodes[child as usize].shape_index() == Some(self.target_shape);
        Some((edge.piece_code(), self.encode(child, child_seen)?))
    }

    fn memo_class(&self, state: u32) -> Option<u32> {
        let (node, seen_target) = self.decode(state)?;
        if seen_target {
            self.seen.memo_class(node as usize)
        } else {
            self.unseen_memo_classes.get(node as usize).copied()
        }
    }

    fn memo_reusable(&self, state: u32) -> bool {
        self.decode(state).is_some_and(|(node, seen_target)| {
            seen_target
                && self.graph.nodes[node as usize].depth >= SETUP_JOINT_SHARED_MEMO_MIN_DEPTH
        })
    }

    fn reusable_memo_domain(&self) -> Option<u64> {
        Some(self.memo_domain)
    }
}

fn target_language_class_storage(node_count: usize) -> Result<Vec<u32>, WasmExactSearchError> {
    let mut classes = Vec::new();
    classes.try_reserve_exact(node_count).map_err(|_| {
        WasmExactSearchError::InvalidProblem("setup_target_language_class_storage_unavailable")
    })?;
    classes.resize(node_count, EMPTY_COVERAGE_REFERENCE);
    Ok(classes)
}

fn target_language_child_class(classes: &[u32], child: usize) -> Result<u32, WasmExactSearchError> {
    classes
        .get(child)
        .copied()
        .filter(|class| *class != EMPTY_COVERAGE_REFERENCE)
        .ok_or(WasmExactSearchError::InvalidProblem(
            "setup_target_language_class_not_topological",
        ))
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
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, probability::probability_value::ProbabilityValue,
    };
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };
    use clearra_supply::{
        pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
        QueueObservationPolicy,
    };

    use crate::backend::wasm_cpu::queue_observation_policy::QueueObservationPolicyEvaluator;

    use super::*;

    fn target_pruning_graph() -> SetupCoverageGraph {
        let edges = vec![
            SetupCoverageEdge::new(1, PieceKind::I).expect("root target edge"),
            SetupCoverageEdge::new(2, PieceKind::O).expect("root unrelated edge"),
            SetupCoverageEdge::new(4, PieceKind::L).expect("root dead-target edge"),
            SetupCoverageEdge::new(3, PieceKind::T).expect("target accepting edge"),
            SetupCoverageEdge::new(4, PieceKind::S).expect("target dead edge"),
            SetupCoverageEdge::new(3, PieceKind::Z).expect("unrelated accepting edge"),
        ];
        let nodes = vec![
            SetupCoverageNode::from_wire(0, 3, NO_SHAPE_INDEX, 0, 0).expect("root"),
            SetupCoverageNode::from_wire(3, 2, 7, 1, 0).expect("target"),
            SetupCoverageNode::from_wire(5, 1, NO_SHAPE_INDEX, 1, 0).expect("unrelated"),
            SetupCoverageNode::from_wire(6, 0, 9, 2, NODE_ACCEPTING).expect("accepting target"),
            SetupCoverageNode::from_wire(6, 0, 8, 2, 0).expect("dead target"),
        ];
        SetupCoverageGraph::from_wire_parts(nodes, edges, 0).expect("coverage graph")
    }

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

    #[test]
    fn target_languages_hide_only_branches_with_no_accepting_completion() {
        let graph = target_pruning_graph();
        let control = ExecutionControl::default();
        let reachability =
            SetupTargetLanguageReachability::compile(&graph, 7, &control).expect("reachability");
        let build = SetupTargetBuildLanguage::compile(&graph, 7, &reachability, &control)
            .expect("build language");
        let seen = SetupJointSeenLanguageClasses::compile(&graph, &control).expect("seen classes");
        let joint = SetupTargetJointLanguage::compile(&graph, 7, &reachability, &seen, 0, &control)
            .expect("joint language");

        assert_eq!(build.edge_count(build.root()), Some(1));
        assert_eq!(build.edge(build.root(), 0), Some((1, 1)));
        assert!(build.node(1).is_some_and(|node| node.accepting));
        assert_eq!(build.edge_count(1), Some(0));

        let joint_root = joint.root();
        assert_eq!(joint.edge_count(joint_root), Some(1));
        let (piece, seen_target_state) = joint.edge(joint_root, 0).expect("target edge");
        assert_eq!(piece, 1);
        assert_eq!(joint.decode(seen_target_state), Some((1, true)));
        assert_eq!(joint.edge_count(seen_target_state), Some(1));
        let (piece, accepting_state) = joint.edge(seen_target_state, 0).expect("accept edge");
        assert_eq!(piece, 3);
        assert!(joint
            .node(accepting_state)
            .is_some_and(|node| node.accepting));
    }

    #[test]
    fn build_keeps_a_dead_target_but_joint_rejects_it() {
        let graph = target_pruning_graph();
        let control = ExecutionControl::default();
        let reachability =
            SetupTargetLanguageReachability::compile(&graph, 8, &control).expect("reachability");
        let build = SetupTargetBuildLanguage::compile(&graph, 8, &reachability, &control)
            .expect("build language");
        let seen = SetupJointSeenLanguageClasses::compile(&graph, &control).expect("seen classes");
        let joint = SetupTargetJointLanguage::compile(&graph, 8, &reachability, &seen, 0, &control)
            .expect("joint language");

        assert_eq!(build.edge_count(build.root()), Some(2));
        assert_eq!(build.edge(build.root(), 0), Some((1, 1)));
        assert_eq!(build.edge(build.root(), 1), Some((7, 4)));
        assert_eq!(joint.edge_count(joint.root()), Some(0));
        assert_eq!(joint.edge(joint.root(), 0), None);
    }

    #[test]
    fn joint_seen_classes_collapse_aliases_without_renumbering_raw_edges() {
        let graph = SetupCoverageGraph::from_wire_parts(
            vec![
                SetupCoverageNode::from_wire(0, 2, NO_SHAPE_INDEX, 6, 0).expect("root"),
                SetupCoverageNode::from_wire(2, 1, 7, 7, 0).expect("first target"),
                SetupCoverageNode::from_wire(3, 1, 8, 7, 0).expect("second target"),
                SetupCoverageNode::from_wire(4, 0, 9, 8, NODE_ACCEPTING).expect("first accepting"),
                SetupCoverageNode::from_wire(4, 0, 10, 8, NODE_ACCEPTING)
                    .expect("second accepting"),
            ],
            vec![
                SetupCoverageEdge::new(1, PieceKind::I).expect("first target edge"),
                SetupCoverageEdge::new(2, PieceKind::O).expect("second target edge"),
                SetupCoverageEdge::new(3, PieceKind::T).expect("first accepting edge"),
                SetupCoverageEdge::new(4, PieceKind::T).expect("second accepting edge"),
            ],
            0,
        )
        .expect("coverage graph");
        let control = ExecutionControl::default();
        let seen = SetupJointSeenLanguageClasses::compile(&graph, &control).expect("seen classes");
        let first_reachability = SetupTargetLanguageReachability::compile(&graph, 7, &control)
            .expect("first reachability");
        let second_reachability = SetupTargetLanguageReachability::compile(&graph, 8, &control)
            .expect("second reachability");
        let first =
            SetupTargetJointLanguage::compile(&graph, 7, &first_reachability, &seen, 0, &control)
                .expect("first language");
        let second =
            SetupTargetJointLanguage::compile(&graph, 8, &second_reachability, &seen, 0, &control)
                .expect("second language");

        let (_, first_target) = first.edge(first.root(), 0).expect("first raw edge");
        let (_, second_target) = second.edge(second.root(), 0).expect("second raw edge");
        assert_eq!(first.decode(first_target), Some((1, true)));
        assert_eq!(second.decode(second_target), Some((2, true)));
        assert_ne!(first_target, second_target);
        assert_eq!(
            first.memo_class(first_target),
            second.memo_class(second_target)
        );
        assert!(first.memo_class(first_target).unwrap() < seen.class_count);
        assert!(second.memo_class(second_target).unwrap() < seen.class_count);
        assert!(first.memo_class(first.root()).unwrap() >= seen.class_count);
        assert!(second.memo_class(second.root()).unwrap() >= seen.class_count);

        let (_, first_accepting) = first.edge(first_target, 0).expect("first suffix edge");
        let (_, second_accepting) = second.edge(second_target, 0).expect("second suffix edge");
        assert_eq!(first.decode(first_accepting), Some((3, true)));
        assert_eq!(second.decode(second_accepting), Some((4, true)));
        assert_ne!(first_accepting, second_accepting);
        assert_eq!(
            first.memo_class(first_accepting),
            second.memo_class(second_accepting)
        );
    }

    #[test]
    fn joint_seen_cache_reuse_matches_a_fresh_second_target_exactly() {
        let graph = SetupCoverageGraph::from_wire_parts(
            vec![
                SetupCoverageNode::from_wire(0, 2, NO_SHAPE_INDEX, 6, 0).expect("root"),
                SetupCoverageNode::from_wire(2, 1, 7, 7, 0).expect("first target"),
                SetupCoverageNode::from_wire(3, 1, 8, 7, 0).expect("second target"),
                SetupCoverageNode::from_wire(4, 0, 9, 8, NODE_ACCEPTING).expect("first accepting"),
                SetupCoverageNode::from_wire(4, 0, 10, 8, NODE_ACCEPTING)
                    .expect("second accepting"),
            ],
            vec![
                SetupCoverageEdge::new(1, PieceKind::I).expect("first target edge"),
                SetupCoverageEdge::new(2, PieceKind::O).expect("second target edge"),
                SetupCoverageEdge::new(3, PieceKind::T).expect("first accepting edge"),
                SetupCoverageEdge::new(4, PieceKind::T).expect("second accepting edge"),
            ],
            0,
        )
        .expect("coverage graph");
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(91),
            PatternWeightModelId::new(92),
            vec![
                vec![PieceKind::I, PieceKind::T],
                vec![PieceKind::O, PieceKind::T],
            ],
            vec![
                ProbabilityValue::new(0.5).expect("weight"),
                ProbabilityValue::new(0.5).expect("weight"),
            ],
            2,
            true,
            None,
        )
        .expect("universe");
        let index = PatternPiecePositionIndex::compile(&universe).expect("pattern index");
        let control = ExecutionControl::default();
        let seen = SetupJointSeenLanguageClasses::compile(&graph, &control).expect("seen classes");
        let first_reachability = SetupTargetLanguageReachability::compile(&graph, 7, &control)
            .expect("first reachability");
        let second_reachability = SetupTargetLanguageReachability::compile(&graph, 8, &control)
            .expect("second reachability");
        let mut shared = QueueObservationPolicyEvaluator::new(
            &universe,
            &index,
            QueueObservationPolicy::VisibleSeven,
            0,
            None,
            false,
            false,
            false,
            None,
        )
        .expect("shared evaluator");
        let domain = shared.begin_reusable_memo_domain();
        let first = SetupTargetJointLanguage::compile(
            &graph,
            7,
            &first_reachability,
            &seen,
            domain,
            &control,
        )
        .expect("first language");
        let second = SetupTargetJointLanguage::compile(
            &graph,
            8,
            &second_reachability,
            &seen,
            domain,
            &control,
        )
        .expect("second language");
        shared.evaluate(&first, &control).expect("first coverage");
        let reused = shared.evaluate(&second, &control).expect("reused coverage");

        let mut fresh = QueueObservationPolicyEvaluator::new(
            &universe,
            &index,
            QueueObservationPolicy::VisibleSeven,
            0,
            None,
            false,
            false,
            false,
            None,
        )
        .expect("fresh evaluator");
        let fresh_domain = fresh.begin_reusable_memo_domain();
        let fresh_second = SetupTargetJointLanguage::compile(
            &graph,
            8,
            &second_reachability,
            &seen,
            fresh_domain,
            &control,
        )
        .expect("fresh second language");
        let expected = fresh
            .evaluate(&fresh_second, &control)
            .expect("fresh second coverage");

        assert_eq!(
            reused.covered_patterns.covered_patterns(),
            expected.covered_patterns.covered_patterns()
        );
        assert_eq!(reused.covered_pattern_count, expected.covered_pattern_count);
        assert_eq!(reused.covered_weight, expected.covered_weight);
        assert_eq!(reused.min_accepted_depth, expected.min_accepted_depth);
        assert_eq!(reused.max_accepted_depth, expected.max_accepted_depth);
        assert!(reused.metrics.action_checks < expected.metrics.action_checks);
    }

    #[test]
    fn target_reachability_honors_cancellation_and_rejects_cycles() {
        let graph = target_pruning_graph();
        let control = ExecutionControl::default();
        control.cancellation.handle().cancel();
        assert!(matches!(
            SetupTargetLanguageReachability::compile(&graph, 7, &control),
            Err(WasmExactSearchError::Cancelled)
        ));

        let cyclic = SetupCoverageGraph::from_wire_parts(
            vec![
                SetupCoverageNode::from_wire(0, 1, NO_SHAPE_INDEX, 0, 0).expect("root"),
                SetupCoverageNode::from_wire(1, 1, 7, 1, 0).expect("target"),
            ],
            vec![
                SetupCoverageEdge::new(1, PieceKind::I).expect("forward edge"),
                SetupCoverageEdge::new(1, PieceKind::O).expect("self edge"),
            ],
            0,
        )
        .expect("wire graph");
        assert!(matches!(
            SetupTargetLanguageReachability::compile(&cyclic, 7, &ExecutionControl::default()),
            Err(WasmExactSearchError::InvalidProblem(
                "setup_target_reachability_graph_not_depth_increasing"
            ))
        ));
    }
}
