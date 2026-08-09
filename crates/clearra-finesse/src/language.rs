//! SRP rationale: this module has one behavior-level change reason: evaluating queue and Hold policies over an immutable costed geometry language.

use std::{collections::BTreeMap, error::Error, fmt};

use clearra_core_domain::{
    piece::{piece_kind::PieceKind, rotation::RotationState},
    probability::probability_value::ProbabilityValue,
};
use clearra_coverage::pattern::pattern_id::PatternId;
use clearra_rules::{kicks::KickTableProfile, spawn::SpawnProfile};

use crate::{
    ClassicInputAction, FinesseBoard, FinesseError, FinesseTarget, FrozenFinesseQuery,
    TerminalEvidenceLabel,
};

const UNREACHABLE: u32 = u32::MAX;
const VISIBLE_COUNT: usize = 7;
const DENSE_LANES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeometryNodeId(u32);

impl GeometryNodeId {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CostedGeometryEdge {
    piece: PieceKind,
    child: GeometryNodeId,
    input_cost: u32,
    transition_order: u32,
    action_key: Option<GeometryActionKey>,
    terminal_evidence: Option<TerminalEvidenceLabel>,
}

/// Stable identity of one placement action at a geometry-language state.
///
/// This is deliberately independent of a solution DAG's node and edge
/// numbering so equivalent actions from multiple solution or symmetry passes
/// can be determinized into one online policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeometryActionKey {
    piece: PieceKind,
    rotation: RotationState,
    x: i16,
    y: i16,
}

impl GeometryActionKey {
    pub const fn new(piece: PieceKind, rotation: RotationState, x: i16, y: i16) -> Self {
        Self {
            piece,
            rotation,
            x,
            y,
        }
    }

    pub const fn piece(self) -> PieceKind {
        self.piece
    }

    pub const fn rotation(self) -> RotationState {
        self.rotation
    }

    pub const fn x(self) -> i16 {
        self.x
    }

    pub const fn y(self) -> i16 {
        self.y
    }
}

impl CostedGeometryEdge {
    pub const fn new(
        piece: PieceKind,
        child: GeometryNodeId,
        input_cost: u32,
        transition_order: u32,
    ) -> Self {
        Self {
            piece,
            child,
            input_cost,
            transition_order,
            action_key: None,
            terminal_evidence: None,
        }
    }

    pub const fn with_action_key(mut self, action_key: GeometryActionKey) -> Self {
        self.action_key = Some(action_key);
        self
    }

    pub const fn with_terminal_evidence(mut self, evidence: TerminalEvidenceLabel) -> Self {
        self.terminal_evidence = Some(evidence);
        self
    }

    pub const fn piece(self) -> PieceKind {
        self.piece
    }

    pub const fn child(self) -> GeometryNodeId {
        self.child
    }

    pub const fn input_cost(self) -> u32 {
        self.input_cost
    }

    pub const fn transition_order(self) -> u32 {
        self.transition_order
    }

    pub const fn action_key(self) -> Option<GeometryActionKey> {
        self.action_key
    }

    pub const fn terminal_evidence(self) -> Option<TerminalEvidenceLabel> {
        self.terminal_evidence
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeometryLanguageNode {
    depth: u16,
    accepting: bool,
    edges: Box<[CostedGeometryEdge]>,
    source_board: Option<FinesseBoard>,
}

impl GeometryLanguageNode {
    pub fn new(depth: u16, accepting: bool, edges: impl Into<Box<[CostedGeometryEdge]>>) -> Self {
        Self {
            depth,
            accepting,
            edges: edges.into(),
            source_board: None,
        }
    }

    pub const fn with_source_board(mut self, source_board: FinesseBoard) -> Self {
        self.source_board = Some(source_board);
        self
    }

    pub const fn depth(&self) -> u16 {
        self.depth
    }

    pub const fn accepting(&self) -> bool {
        self.accepting
    }

    pub fn edges(&self) -> &[CostedGeometryEdge] {
        &self.edges
    }

    pub const fn source_board(&self) -> Option<FinesseBoard> {
        self.source_board
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostedGeometryLanguage {
    root: GeometryNodeId,
    nodes: Box<[GeometryLanguageNode]>,
}

impl CostedGeometryLanguage {
    pub fn new(
        root: GeometryNodeId,
        nodes: impl Into<Box<[GeometryLanguageNode]>>,
    ) -> Result<Self, GeometryLanguageError> {
        let nodes = nodes.into();
        if root.index() >= nodes.len() {
            return Err(GeometryLanguageError::RootOutOfRange(root));
        }
        if nodes[root.index()].depth != 0 {
            return Err(GeometryLanguageError::RootDepthNotZero {
                root,
                depth: nodes[root.index()].depth,
            });
        }
        for (source_index, node) in nodes.iter().enumerate() {
            if node.accepting && !node.edges.is_empty() {
                return Err(GeometryLanguageError::AcceptingNodeHasEdges {
                    node: GeometryNodeId::new(source_index as u32),
                });
            }
            let mut prior_order = None;
            for edge in node.edges.iter().copied() {
                let expected_child_depth =
                    node.depth
                        .checked_add(1)
                        .ok_or(GeometryLanguageError::DepthOverflow {
                            source: GeometryNodeId::new(source_index as u32),
                        })?;
                let child = nodes.get(edge.child.index()).ok_or(
                    GeometryLanguageError::ChildOutOfRange {
                        source: GeometryNodeId::new(source_index as u32),
                        child: edge.child,
                    },
                )?;
                if child.depth != expected_child_depth {
                    return Err(GeometryLanguageError::DepthMismatch {
                        source: GeometryNodeId::new(source_index as u32),
                        source_depth: node.depth,
                        child: edge.child,
                        child_depth: child.depth,
                    });
                }
                if prior_order.is_some_and(|prior| edge.transition_order < prior) {
                    return Err(GeometryLanguageError::TransitionOrderNotStable {
                        node: GeometryNodeId::new(source_index as u32),
                    });
                }
                prior_order = Some(edge.transition_order);
            }
        }
        Ok(Self { root, nodes })
    }

    pub const fn root(&self) -> GeometryNodeId {
        self.root
    }

    pub fn node(&self, id: GeometryNodeId) -> Option<&GeometryLanguageNode> {
        self.nodes.get(id.index())
    }

    pub fn nodes(&self) -> &[GeometryLanguageNode] {
        &self.nodes
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct UnionState(Box<[(usize, GeometryNodeId)]>);

struct UnionEdgeGroup {
    piece: PieceKind,
    input_cost: u32,
    transition_order: u32,
    children: Vec<(usize, GeometryNodeId)>,
}

/// Determinize the union of solution DAGs by semantic placement action.
///
/// Nodes reached by an identical action history represent the same board.
/// Outgoing edges with the same [`GeometryActionKey`] therefore remain one
/// online action while retaining every compatible solution continuation.
pub fn union_costed_geometry_languages(
    languages: &[&CostedGeometryLanguage],
) -> Result<CostedGeometryLanguage, GeometryLanguageError> {
    if languages.is_empty() {
        return Err(GeometryLanguageError::EmptyLanguageUnion);
    }
    let root = UnionState(
        languages
            .iter()
            .enumerate()
            .map(|(language_index, language)| (language_index, language.root))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let mut state_ids = BTreeMap::new();
    state_ids.insert(root.clone(), GeometryNodeId::new(0));
    let mut states = vec![root];
    let mut nodes = Vec::new();
    let mut cursor = 0usize;

    while cursor < states.len() {
        let state = states[cursor].clone();
        let mut depth = None;
        let mut source_board = None;
        let mut source_board_initialized = false;
        let mut accepting = false;
        for &(language_index, node_id) in state.0.iter() {
            let node = &languages[language_index].nodes[node_id.index()];
            if depth
                .replace(node.depth)
                .is_some_and(|value| value != node.depth)
            {
                return Err(GeometryLanguageError::UnionStateDepthMismatch);
            }
            if source_board_initialized {
                if source_board != node.source_board {
                    return Err(GeometryLanguageError::UnionSourceBoardMismatch);
                }
            } else {
                source_board = node.source_board;
                source_board_initialized = true;
            }
            accepting |= node.accepting;
        }
        let depth = depth.expect("a union state always contains at least one language node");
        if accepting {
            let mut node = GeometryLanguageNode::new(depth, true, Vec::<CostedGeometryEdge>::new());
            if let Some(board) = source_board {
                node = node.with_source_board(board);
            }
            nodes.push(node);
            cursor += 1;
            continue;
        }

        let mut groups =
            BTreeMap::<(GeometryActionKey, Option<TerminalEvidenceLabel>), UnionEdgeGroup>::new();
        for &(language_index, node_id) in state.0.iter() {
            let node = &languages[language_index].nodes[node_id.index()];
            for edge in node.edges.iter().copied() {
                let action = edge
                    .action_key
                    .ok_or(GeometryLanguageError::MissingActionKey { node: node_id })?;
                if action.piece != edge.piece {
                    return Err(GeometryLanguageError::ActionPieceMismatch { node: node_id });
                }
                let group = groups
                    .entry((action, edge.terminal_evidence))
                    .or_insert_with(|| UnionEdgeGroup {
                        piece: edge.piece,
                        input_cost: edge.input_cost,
                        transition_order: edge.transition_order,
                        children: Vec::new(),
                    });
                if group.input_cost != edge.input_cost {
                    return Err(GeometryLanguageError::UnionActionCostMismatch { action });
                }
                group.transition_order = group.transition_order.min(edge.transition_order);
                group.children.push((language_index, edge.child));
            }
        }
        let mut ordered_groups = groups.into_iter().collect::<Vec<_>>();
        ordered_groups.sort_unstable_by_key(|(action, group)| (group.transition_order, *action));
        let mut edges = Vec::with_capacity(ordered_groups.len());
        for ((action, terminal_evidence), mut group) in ordered_groups {
            group.children.sort_unstable();
            group.children.dedup();
            let child_state = UnionState(group.children.into_boxed_slice());
            let child = if let Some(child) = state_ids.get(&child_state).copied() {
                child
            } else {
                let index = u32::try_from(states.len())
                    .map_err(|_| GeometryLanguageError::LanguageUnionTooLarge)?;
                let child = GeometryNodeId::new(index);
                state_ids.insert(child_state.clone(), child);
                states.push(child_state);
                child
            };
            let mut edge = CostedGeometryEdge::new(
                group.piece,
                child,
                group.input_cost,
                group.transition_order,
            )
            .with_action_key(action);
            if let Some(evidence) = terminal_evidence {
                edge = edge.with_terminal_evidence(evidence);
            }
            edges.push(edge);
        }
        let mut node = GeometryLanguageNode::new(depth, false, edges);
        if let Some(board) = source_board {
            node = node.with_source_board(board);
        }
        nodes.push(node);
        cursor += 1;
    }

    CostedGeometryLanguage::new(GeometryNodeId::new(0), nodes)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryLanguageError {
    RootOutOfRange(GeometryNodeId),
    RootDepthNotZero {
        root: GeometryNodeId,
        depth: u16,
    },
    ChildOutOfRange {
        source: GeometryNodeId,
        child: GeometryNodeId,
    },
    AcceptingNodeHasEdges {
        node: GeometryNodeId,
    },
    DepthMismatch {
        source: GeometryNodeId,
        source_depth: u16,
        child: GeometryNodeId,
        child_depth: u16,
    },
    DepthOverflow {
        source: GeometryNodeId,
    },
    TransitionOrderNotStable {
        node: GeometryNodeId,
    },
    DuplicatePatternId(PatternId),
    ProbabilityMassOutOfRange,
    CostVectorTooWide {
        lanes: usize,
    },
    CostVectorLaneOutOfRange {
        lane: usize,
        lanes: usize,
    },
    CostOverflow,
    CostTableLengthMismatch {
        expected: usize,
        actual: usize,
    },
    EmptyLanguageUnion,
    MissingActionKey {
        node: GeometryNodeId,
    },
    ActionPieceMismatch {
        node: GeometryNodeId,
    },
    UnionStateDepthMismatch,
    UnionActionCostMismatch {
        action: GeometryActionKey,
    },
    UnionSourceBoardMismatch,
    LanguageUnionTooLarge,
    WitnessReconstructionMismatch,
    Cancelled,
}

impl fmt::Display for GeometryLanguageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for GeometryLanguageError {}

#[derive(Clone, Debug, PartialEq)]
pub struct QueuePattern {
    pattern_id: PatternId,
    queue: Box<[PieceKind]>,
    probability: ProbabilityValue,
}

impl QueuePattern {
    pub fn new(
        pattern_id: PatternId,
        queue: impl Into<Box<[PieceKind]>>,
        probability: ProbabilityValue,
    ) -> Self {
        Self {
            pattern_id,
            queue: queue.into(),
            probability,
        }
    }

    pub const fn pattern_id(&self) -> PatternId {
        self.pattern_id
    }

    pub fn queue(&self) -> &[PieceKind] {
        &self.queue
    }

    pub const fn probability(&self) -> ProbabilityValue {
        self.probability
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct QueueClassId(u32);

impl QueueClassId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueueClass {
    id: QueueClassId,
    queue: Box<[PieceKind]>,
    pattern_ids: Box<[PatternId]>,
    probability_mass: f64,
}

impl QueueClass {
    pub const fn id(&self) -> QueueClassId {
        self.id
    }

    pub fn queue(&self) -> &[PieceKind] {
        &self.queue
    }

    pub fn pattern_ids(&self) -> &[PatternId] {
        &self.pattern_ids
    }

    pub const fn probability_mass(&self) -> f64 {
        self.probability_mass
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QueueUniverseMetadata {
    pub materialized_probability_mass: f64,
    pub complete: bool,
    pub pattern_count: usize,
    pub unique_queue_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueueClassSet {
    classes: Box<[QueueClass]>,
    metadata: QueueUniverseMetadata,
}

impl QueueClassSet {
    /// Group identical complete queue strings in first-occurrence order.
    /// Probability mass is summed, never normalized.
    pub fn group(
        patterns: &[QueuePattern],
        universe_complete: bool,
    ) -> Result<Self, GeometryLanguageError> {
        let mut class_by_queue = BTreeMap::<Box<[PieceKind]>, usize>::new();
        let mut queues = Vec::<Box<[PieceKind]>>::new();
        let mut ids = Vec::<Vec<PatternId>>::new();
        let mut masses = Vec::<f64>::new();
        let mut seen_ids = BTreeMap::<PatternId, ()>::new();
        let mut total_mass = 0.0;

        for pattern in patterns {
            if seen_ids.insert(pattern.pattern_id, ()).is_some() {
                return Err(GeometryLanguageError::DuplicatePatternId(
                    pattern.pattern_id,
                ));
            }
            let mass = pattern.probability.get();
            total_mass += mass;
            let class_index = if let Some(index) = class_by_queue.get(pattern.queue()).copied() {
                index
            } else {
                let index = queues.len();
                let queue = pattern.queue.clone();
                class_by_queue.insert(queue.clone(), index);
                queues.push(queue);
                ids.push(Vec::new());
                masses.push(0.0);
                index
            };
            ids[class_index].push(pattern.pattern_id);
            masses[class_index] += mass;
        }
        if !total_mass.is_finite() || total_mass > 1.0 + 1e-12 {
            return Err(GeometryLanguageError::ProbabilityMassOutOfRange);
        }

        let classes = queues
            .into_iter()
            .zip(ids)
            .zip(masses)
            .enumerate()
            .map(
                |(index, ((queue, pattern_ids), probability_mass))| QueueClass {
                    id: QueueClassId(index as u32),
                    queue,
                    pattern_ids: pattern_ids.into_boxed_slice(),
                    probability_mass,
                },
            )
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let metadata = QueueUniverseMetadata {
            materialized_probability_mass: total_mass,
            complete: universe_complete,
            pattern_count: patterns.len(),
            unique_queue_count: classes.len(),
        };
        Ok(Self { classes, metadata })
    }

    pub fn classes(&self) -> &[QueueClass] {
        &self.classes
    }

    pub const fn metadata(&self) -> QueueUniverseMetadata {
        self.metadata
    }

    pub fn with_complete(mut self, complete: bool) -> Self {
        self.metadata.complete &= complete;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CostVectorStorageKind {
    Single,
    Inline,
    Dense64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CostVectorStorage {
    Single {
        lanes: u8,
        cost: u32,
    },
    Inline {
        lanes: u8,
        bucket_count: u8,
        costs: [u32; 4],
        lane_masks: [u64; 4],
    },
    Dense64 {
        lanes: u8,
        costs: Box<[u32; 64]>,
    },
}

/// One compact cost bucket with at most 64 lanes.
///
/// Storage follows the number of distinct costs, including `unreachable`:
/// one shared value, up to four `(cost, lane-mask)` partitions, then a dense
/// 64-lane array. It can compact again after strict-min updates converge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostVector64 {
    storage: CostVectorStorage,
}

impl CostVector64 {
    pub fn unreachable(lanes: usize) -> Result<Self, GeometryLanguageError> {
        if lanes > DENSE_LANES {
            return Err(GeometryLanguageError::CostVectorTooWide { lanes });
        }
        let storage = CostVectorStorage::Single {
            lanes: lanes as u8,
            cost: UNREACHABLE,
        };
        Ok(Self { storage })
    }

    fn filled(lanes: usize, cost: u32) -> Result<Self, GeometryLanguageError> {
        if lanes > DENSE_LANES {
            return Err(GeometryLanguageError::CostVectorTooWide { lanes });
        }
        if cost == UNREACHABLE {
            return Err(GeometryLanguageError::CostOverflow);
        }
        Ok(Self {
            storage: CostVectorStorage::Single {
                lanes: lanes as u8,
                cost,
            },
        })
    }

    pub const fn len(&self) -> usize {
        match &self.storage {
            CostVectorStorage::Single { lanes, .. } => *lanes as usize,
            CostVectorStorage::Inline { lanes, .. } | CostVectorStorage::Dense64 { lanes, .. } => {
                *lanes as usize
            }
        }
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub const fn storage_kind(&self) -> CostVectorStorageKind {
        match self.storage {
            CostVectorStorage::Single { .. } => CostVectorStorageKind::Single,
            CostVectorStorage::Inline { .. } => CostVectorStorageKind::Inline,
            CostVectorStorage::Dense64 { .. } => CostVectorStorageKind::Dense64,
        }
    }

    pub fn get(&self, lane: usize) -> Option<Option<u32>> {
        if lane >= self.len() {
            return None;
        }
        let value = match &self.storage {
            CostVectorStorage::Single { cost, .. } => *cost,
            CostVectorStorage::Inline {
                bucket_count,
                costs,
                lane_masks,
                ..
            } => {
                let lane_bit = 1_u64 << lane;
                (0..usize::from(*bucket_count))
                    .find_map(|bucket| {
                        (lane_masks[bucket] & lane_bit != 0).then_some(costs[bucket])
                    })
                    .expect("inline cost partitions cover every active lane")
            }
            CostVectorStorage::Dense64 { costs, .. } => costs[lane],
        };
        Some((value != UNREACHABLE).then_some(value))
    }

    /// Strict min update. Equal routes never replace the existing lane.
    pub fn set_min(&mut self, lane: usize, cost: u32) -> Result<bool, GeometryLanguageError> {
        if cost == UNREACHABLE {
            return Err(GeometryLanguageError::CostOverflow);
        }
        let lanes = self.len();
        if lane >= lanes {
            return Err(GeometryLanguageError::CostVectorLaneOutOfRange { lane, lanes });
        }
        let current = self
            .get(lane)
            .expect("validated lane belongs to the cost vector")
            .unwrap_or(UNREACHABLE);
        if cost >= current {
            return Ok(false);
        }

        let lane_bit = 1_u64 << lane;
        let replacement = match &mut self.storage {
            CostVectorStorage::Single {
                lanes: stored_lanes,
                cost: shared_cost,
            } => {
                if *stored_lanes <= 1 {
                    *shared_cost = cost;
                    None
                } else {
                    let mut costs = [UNREACHABLE; 4];
                    let mut lane_masks = [0; 4];
                    costs[0] = cost;
                    lane_masks[0] = lane_bit;
                    costs[1] = *shared_cost;
                    lane_masks[1] = active_lane_mask(usize::from(*stored_lanes)) & !lane_bit;
                    Some(CostVectorStorage::Inline {
                        lanes: *stored_lanes,
                        bucket_count: 2,
                        costs,
                        lane_masks,
                    })
                }
            }
            CostVectorStorage::Inline {
                lanes: stored_lanes,
                bucket_count,
                costs,
                lane_masks,
            } => {
                let count = usize::from(*bucket_count);
                let source_bucket = (0..count)
                    .find(|bucket| lane_masks[*bucket] & lane_bit != 0)
                    .expect("inline cost partitions cover every active lane");
                let target_bucket = (0..count).find(|bucket| costs[*bucket] == cost);
                if let Some(target_bucket) = target_bucket {
                    lane_masks[source_bucket] &= !lane_bit;
                    lane_masks[target_bucket] |= lane_bit;
                    Some(compact_inline_storage(
                        *stored_lanes,
                        *bucket_count,
                        *costs,
                        *lane_masks,
                    ))
                } else if count < 4 || lane_masks[source_bucket] == lane_bit {
                    lane_masks[source_bucket] &= !lane_bit;
                    let mut compacted = compact_inline_parts(*bucket_count, *costs, *lane_masks);
                    let next = usize::from(compacted.0);
                    compacted.1[next] = cost;
                    compacted.2[next] = lane_bit;
                    compacted.0 += 1;
                    Some(inline_storage_from_parts(
                        *stored_lanes,
                        compacted.0,
                        compacted.1,
                        compacted.2,
                    ))
                } else {
                    let mut dense = Box::new([UNREACHABLE; DENSE_LANES]);
                    for bucket in 0..count {
                        let mut mask = lane_masks[bucket];
                        while mask != 0 {
                            let dense_lane = mask.trailing_zeros() as usize;
                            mask &= mask - 1;
                            dense[dense_lane] = costs[bucket];
                        }
                    }
                    dense[lane] = cost;
                    Some(CostVectorStorage::Dense64 {
                        lanes: *stored_lanes,
                        costs: dense,
                    })
                }
            }
            CostVectorStorage::Dense64 {
                lanes: stored_lanes,
                costs,
            } => {
                costs[lane] = cost;
                compact_dense_storage(*stored_lanes, costs)
            }
        };
        if let Some(replacement) = replacement {
            self.storage = replacement;
        }
        Ok(true)
    }

    fn reachable_lane_mask(&self) -> u64 {
        match &self.storage {
            CostVectorStorage::Single { lanes, cost } => {
                if *cost == UNREACHABLE {
                    0
                } else {
                    active_lane_mask(usize::from(*lanes))
                }
            }
            CostVectorStorage::Inline {
                bucket_count,
                costs,
                lane_masks,
                ..
            } => (0..usize::from(*bucket_count))
                .filter(|bucket| costs[*bucket] != UNREACHABLE)
                .fold(0, |mask, bucket| mask | lane_masks[bucket]),
            CostVectorStorage::Dense64 { lanes, costs } => {
                let mut mask = 0;
                for (lane, cost) in costs.iter().take(usize::from(*lanes)).enumerate() {
                    if *cost != UNREACHABLE {
                        mask |= 1_u64 << lane;
                    }
                }
                mask
            }
        }
    }

    /// Apply one masked min-plus transition without materializing a per-class
    /// state table. Cost partitions remain compact while lanes agree and
    /// expand only when the product actually diverges.
    fn relax_masked_from(
        &mut self,
        source: &Self,
        lane_mask: u64,
        increment: u32,
    ) -> Result<(), GeometryLanguageError> {
        debug_assert_eq!(self.len(), source.len());
        let lane_mask = lane_mask & active_lane_mask(source.len()) & source.reachable_lane_mask();
        match &source.storage {
            CostVectorStorage::Single { cost, .. } => {
                if *cost != UNREACHABLE {
                    self.set_min_mask(lane_mask, checked_product_cost(*cost, increment)?)?;
                }
            }
            CostVectorStorage::Inline {
                bucket_count,
                costs,
                lane_masks,
                ..
            } => {
                for bucket in 0..usize::from(*bucket_count) {
                    if costs[bucket] == UNREACHABLE {
                        continue;
                    }
                    self.set_min_mask(
                        lane_mask & lane_masks[bucket],
                        checked_product_cost(costs[bucket], increment)?,
                    )?;
                }
            }
            CostVectorStorage::Dense64 { lanes, costs } => {
                let mut remaining = lane_mask & active_lane_mask(usize::from(*lanes));
                while remaining != 0 {
                    let lane = remaining.trailing_zeros() as usize;
                    remaining &= remaining - 1;
                    self.set_min(lane, checked_product_cost(costs[lane], increment)?)?;
                }
            }
        }
        Ok(())
    }

    fn set_min_mask(&mut self, mut lane_mask: u64, cost: u32) -> Result<(), GeometryLanguageError> {
        lane_mask &= active_lane_mask(self.len());
        if lane_mask == 0 {
            return Ok(());
        }
        if lane_mask == active_lane_mask(self.len()) {
            if let CostVectorStorage::Single {
                cost: shared_cost, ..
            } = &mut self.storage
            {
                if cost < *shared_cost {
                    *shared_cost = cost;
                }
                return Ok(());
            }
        }
        while lane_mask != 0 {
            let lane = lane_mask.trailing_zeros() as usize;
            lane_mask &= lane_mask - 1;
            self.set_min(lane, cost)?;
        }
        Ok(())
    }
}

fn checked_product_cost(base: u32, increment: u32) -> Result<u32, GeometryLanguageError> {
    let cost = base
        .checked_add(increment)
        .ok_or(GeometryLanguageError::CostOverflow)?;
    if cost == UNREACHABLE {
        return Err(GeometryLanguageError::CostOverflow);
    }
    Ok(cost)
}

const fn active_lane_mask(lanes: usize) -> u64 {
    if lanes == DENSE_LANES {
        u64::MAX
    } else if lanes == 0 {
        0
    } else {
        (1_u64 << lanes) - 1
    }
}

fn compact_inline_parts(
    bucket_count: u8,
    costs: [u32; 4],
    lane_masks: [u64; 4],
) -> (u8, [u32; 4], [u64; 4]) {
    let mut compacted_costs = [UNREACHABLE; 4];
    let mut compacted_masks = [0; 4];
    let mut compacted_count = 0;
    for bucket in 0..usize::from(bucket_count) {
        if lane_masks[bucket] == 0 {
            continue;
        }
        compacted_costs[compacted_count] = costs[bucket];
        compacted_masks[compacted_count] = lane_masks[bucket];
        compacted_count += 1;
    }
    (compacted_count as u8, compacted_costs, compacted_masks)
}

fn compact_inline_storage(
    lanes: u8,
    bucket_count: u8,
    costs: [u32; 4],
    lane_masks: [u64; 4],
) -> CostVectorStorage {
    let (bucket_count, costs, lane_masks) = compact_inline_parts(bucket_count, costs, lane_masks);
    inline_storage_from_parts(lanes, bucket_count, costs, lane_masks)
}

fn inline_storage_from_parts(
    lanes: u8,
    bucket_count: u8,
    costs: [u32; 4],
    lane_masks: [u64; 4],
) -> CostVectorStorage {
    if bucket_count == 1 {
        CostVectorStorage::Single {
            lanes,
            cost: costs[0],
        }
    } else {
        CostVectorStorage::Inline {
            lanes,
            bucket_count,
            costs,
            lane_masks,
        }
    }
}

fn compact_dense_storage(lanes: u8, dense: &[u32; DENSE_LANES]) -> Option<CostVectorStorage> {
    let mut costs = [UNREACHABLE; 4];
    let mut lane_masks = [0_u64; 4];
    let mut bucket_count = 0_usize;
    for (lane, cost) in dense.iter().copied().enumerate().take(usize::from(lanes)) {
        let bucket = if let Some(bucket) = (0..bucket_count).find(|bucket| costs[*bucket] == cost) {
            bucket
        } else {
            if bucket_count == 4 {
                return None;
            }
            costs[bucket_count] = cost;
            bucket_count += 1;
            bucket_count - 1
        };
        lane_masks[bucket] |= 1_u64 << lane;
    }
    Some(inline_storage_from_parts(
        lanes,
        bucket_count as u8,
        costs,
        lane_masks,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueCostTable {
    len: usize,
    buckets: Box<[CostVector64]>,
}

impl QueueCostTable {
    pub fn unreachable(len: usize) -> Result<Self, GeometryLanguageError> {
        let mut buckets = Vec::new();
        let mut remaining = len;
        while remaining != 0 {
            let lanes = remaining.min(DENSE_LANES);
            buckets.push(CostVector64::unreachable(lanes)?);
            remaining -= lanes;
        }
        Ok(Self {
            len,
            buckets: buckets.into_boxed_slice(),
        })
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, queue_class_index: usize) -> Option<Option<u32>> {
        if queue_class_index >= self.len {
            return None;
        }
        let bucket = queue_class_index / DENSE_LANES;
        let lane = queue_class_index % DENSE_LANES;
        self.buckets.get(bucket)?.get(lane)
    }

    pub fn set_min(
        &mut self,
        queue_class_index: usize,
        cost: u32,
    ) -> Result<bool, GeometryLanguageError> {
        if queue_class_index >= self.len {
            return Err(GeometryLanguageError::CostVectorLaneOutOfRange {
                lane: queue_class_index,
                lanes: self.len,
            });
        }
        self.buckets[queue_class_index / DENSE_LANES].set_min(queue_class_index % DENSE_LANES, cost)
    }

    pub fn storage_kinds(&self) -> impl Iterator<Item = CostVectorStorageKind> + '_ {
        self.buckets.iter().map(CostVector64::storage_kind)
    }

    pub fn to_vec(&self) -> Vec<Option<u32>> {
        (0..self.len)
            .map(|index| self.get(index).expect("index belongs to cost table"))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OracleQueueEvaluation {
    pub costs: QueueCostTable,
    pub universe: QueueUniverseMetadata,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisibleSevenEvaluation {
    pub costs: QueueCostTable,
    pub successful_probability_mass: f64,
    pub successful_unique_queue_count: usize,
    pub total_inputs: u64,
    pub universe: QueueUniverseMetadata,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum QueueSupplyAction {
    UseCurrent,
    SwapHeld,
    StoreCurrentUseNext,
    ReleaseHeld,
}

impl QueueSupplyAction {
    pub const fn input_cost(self) -> u32 {
        match self {
            Self::UseCurrent => 0,
            Self::SwapHeld | Self::StoreCurrentUseNext | Self::ReleaseHeld => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedQueueRouteStep {
    source: GeometryNodeId,
    supply_action: QueueSupplyAction,
    edge: CostedGeometryEdge,
}

impl FixedQueueRouteStep {
    pub const fn source(self) -> GeometryNodeId {
        self.source
    }

    pub const fn supply_action(self) -> QueueSupplyAction {
        self.supply_action
    }

    pub const fn edge(self) -> CostedGeometryEdge {
        self.edge
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedQueueWitness {
    total_cost: u32,
    steps: Box<[FixedQueueRouteStep]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinesseSequenceInput {
    Hold,
    Movement(ClassicInputAction),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayedFixedQueueWitness {
    total_cost: u32,
    inputs: Box<[FinesseSequenceInput]>,
    placements: Box<[GeometryActionKey]>,
}

impl ReplayedFixedQueueWitness {
    pub const fn total_cost(&self) -> u32 {
        self.total_cost
    }

    pub fn inputs(&self) -> &[FinesseSequenceInput] {
        &self.inputs
    }

    /// Exact placement actions selected by the queue-class product replay, in
    /// lock order. Hold inputs remain in [`Self::inputs`] and never create a
    /// placement entry.
    pub fn placements(&self) -> &[GeometryActionKey] {
        &self.placements
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinesseRouteWitnessError {
    Geometry(GeometryLanguageError),
    Movement(FinesseError),
    MissingSourceBoard { node: GeometryNodeId },
    MissingActionKey { node: GeometryNodeId },
    MovementRouteMissing { node: GeometryNodeId },
    CostMismatch { node: GeometryNodeId },
}

impl fmt::Display for FinesseRouteWitnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for FinesseRouteWitnessError {}

impl From<GeometryLanguageError> for FinesseRouteWitnessError {
    fn from(value: GeometryLanguageError) -> Self {
        Self::Geometry(value)
    }
}

impl From<FinesseError> for FinesseRouteWitnessError {
    fn from(value: FinesseError) -> Self {
        Self::Movement(value)
    }
}

impl FixedQueueWitness {
    pub const fn total_cost(&self) -> u32 {
        self.total_cost
    }

    pub fn steps(&self) -> &[FixedQueueRouteStep] {
        &self.steps
    }
}

pub struct QueueClassProductEvaluator<'a> {
    language: &'a CostedGeometryLanguage,
    hold_enabled: bool,
    terminal_hold_release_enabled: bool,
}

impl<'a> QueueClassProductEvaluator<'a> {
    pub const fn new(language: &'a CostedGeometryLanguage) -> Self {
        Self {
            language,
            hold_enabled: true,
            terminal_hold_release_enabled: true,
        }
    }

    pub const fn new_with_hold_enabled(
        language: &'a CostedGeometryLanguage,
        hold_enabled: bool,
    ) -> Self {
        Self::new(language).with_hold_enabled(hold_enabled)
    }

    pub const fn with_hold_enabled(mut self, hold_enabled: bool) -> Self {
        self.hold_enabled = hold_enabled;
        self
    }

    pub const fn with_terminal_hold_release_enabled(mut self, enabled: bool) -> Self {
        self.terminal_hold_release_enabled = enabled;
        self
    }

    /// Scalar min-plus cost for one known full queue. Every hold action costs
    /// one input in addition to the selected geometry edge.
    pub fn fixed_queue_cost(
        &self,
        queue: &[PieceKind],
        initial_hold: Option<PieceKind>,
    ) -> Result<Option<u32>, GeometryLanguageError> {
        self.fixed_queue_cost_with_cancel(queue, initial_hold, || false)
    }

    /// Scalar min-plus cost for one known full queue with exact-state
    /// cancellation boundaries. Pattern Oracle callers should use
    /// [`Self::oracle_with_cancel`] so queue classes can share the vector
    /// product instead.
    pub fn fixed_queue_cost_with_cancel(
        &self,
        queue: &[PieceKind],
        initial_hold: Option<PieceKind>,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<Option<u32>, GeometryLanguageError> {
        self.scalar_cost_with_cancel(
            queue,
            ScalarState {
                node: self.language.root,
                cursor: 0,
                hold: initial_hold,
            },
            &mut BTreeMap::new(),
            &mut is_cancelled,
        )
    }

    /// Recompute one fixed-queue minimum and restore its strict first-tie
    /// supply/geometry edge sequence without retaining parents in cost-only
    /// evaluation.
    pub fn fixed_queue_witness(
        &self,
        queue: &[PieceKind],
        initial_hold: Option<PieceKind>,
    ) -> Result<Option<FixedQueueWitness>, GeometryLanguageError> {
        self.fixed_queue_witness_with_cancel(queue, initial_hold, || false)
    }

    /// Recompute one fixed-queue route while allowing cancellation throughout
    /// both the scalar DP rerun and strict first-tie reconstruction.
    pub fn fixed_queue_witness_with_cancel(
        &self,
        queue: &[PieceKind],
        initial_hold: Option<PieceKind>,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<Option<FixedQueueWitness>, GeometryLanguageError> {
        let mut memo = BTreeMap::new();
        let mut state = ScalarState {
            node: self.language.root,
            cursor: 0,
            hold: initial_hold,
        };
        let Some(total_cost) =
            self.scalar_cost_with_cancel(queue, state, &mut memo, &mut is_cancelled)?
        else {
            return Ok(None);
        };
        let mut remaining_cost = total_cost;
        let mut steps = Vec::new();
        while !self.language.nodes[state.node.index()].accepting {
            if is_cancelled() {
                return Err(GeometryLanguageError::Cancelled);
            }
            let node = &self.language.nodes[state.node.index()];
            let mut selected = None;
            'candidate: for supply in supply_transitions(
                queue,
                state.cursor,
                state.hold,
                self.hold_enabled,
                self.terminal_hold_release_enabled,
            ) {
                for edge in node.edges.iter().copied() {
                    if edge.piece != supply.piece {
                        continue;
                    }
                    let child = ScalarState {
                        node: edge.child,
                        cursor: supply.cursor,
                        hold: supply.hold,
                    };
                    let Some(suffix) =
                        self.scalar_cost_with_cancel(queue, child, &mut memo, &mut is_cancelled)?
                    else {
                        continue;
                    };
                    let candidate_cost = edge
                        .input_cost
                        .checked_add(supply.hold_cost)
                        .and_then(|cost| cost.checked_add(suffix))
                        .ok_or(GeometryLanguageError::CostOverflow)?;
                    if candidate_cost == remaining_cost {
                        selected = Some((supply, edge, child, suffix));
                        break 'candidate;
                    }
                }
            }
            let Some((supply, edge, child, suffix)) = selected else {
                return Err(GeometryLanguageError::WitnessReconstructionMismatch);
            };
            steps.push(FixedQueueRouteStep {
                source: state.node,
                supply_action: supply.action,
                edge,
            });
            state = child;
            remaining_cost = suffix;
        }
        if remaining_cost != 0 {
            return Err(GeometryLanguageError::WitnessReconstructionMismatch);
        }
        Ok(Some(FixedQueueWitness {
            total_cost,
            steps: steps.into_boxed_slice(),
        }))
    }

    /// Rerun movement BFS only for the selected fixed route and verify that
    /// its concrete input sequence has exactly the authoritative DP cost.
    pub fn replay_fixed_queue_witness(
        &self,
        queue: &[PieceKind],
        initial_hold: Option<PieceKind>,
        spawn: SpawnProfile,
        kicks: &KickTableProfile,
    ) -> Result<Option<ReplayedFixedQueueWitness>, FinesseRouteWitnessError> {
        self.replay_fixed_queue_witness_with_cancel(queue, initial_hold, spawn, kicks, || false)
    }

    /// Restore a fixed-queue movement witness with cancellation spanning the
    /// cost DP, route selection, and each per-placement movement BFS.
    pub fn replay_fixed_queue_witness_with_cancel(
        &self,
        queue: &[PieceKind],
        initial_hold: Option<PieceKind>,
        spawn: SpawnProfile,
        kicks: &KickTableProfile,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<Option<ReplayedFixedQueueWitness>, FinesseRouteWitnessError> {
        let Some(route) =
            self.fixed_queue_witness_with_cancel(queue, initial_hold, &mut is_cancelled)?
        else {
            return Ok(None);
        };
        self.replay_route_witness_with_cancel(&route, spawn, kicks, &mut is_cancelled)
            .map(Some)
    }

    /// Recompute the shared visible-seven policy and restore the route taken
    /// by one successful queue class. The policy rerun retains no parent map:
    /// each selected action is recovered from the same strict policy ordering
    /// and then only the selected observation branch is followed.
    pub fn visible_seven_class_witness(
        &self,
        classes: &QueueClassSet,
        initial_hold: Option<PieceKind>,
        class_index: usize,
    ) -> Result<Option<FixedQueueWitness>, GeometryLanguageError> {
        self.visible_seven_class_witness_with_cancel(classes, initial_hold, class_index, || false)
    }

    /// Recompute one selected branch of the shared visible-seven policy while
    /// retaining cancellation at every product state and reconstruction loop.
    pub fn visible_seven_class_witness_with_cancel(
        &self,
        classes: &QueueClassSet,
        initial_hold: Option<PieceKind>,
        class_index: usize,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<Option<FixedQueueWitness>, GeometryLanguageError> {
        if is_cancelled() {
            return Err(GeometryLanguageError::Cancelled);
        }
        if class_index >= classes.classes.len() {
            return Err(GeometryLanguageError::CostVectorLaneOutOfRange {
                lane: class_index,
                lanes: classes.classes.len(),
            });
        }
        let all = (0..classes.classes.len()).collect::<Vec<_>>();
        let Some(members) = split_observations(&all, classes, 0, initial_hold)
            .into_iter()
            .find(|members| members.contains(&class_index))
        else {
            return Ok(None);
        };
        let mut state = PolicyState {
            node: self.language.root,
            cursor: 0,
            hold: initial_hold,
            members: members.into_boxed_slice(),
        };
        let mut memo = BTreeMap::new();
        let value =
            self.policy_value_with_cancel(classes, state.clone(), &mut memo, &mut is_cancelled)?;
        let Some(total_cost) = value.outcomes.get(class_index) else {
            return Ok(None);
        };
        let mut remaining_cost = total_cost;
        let mut steps = Vec::new();

        while !self.language.nodes[state.node.index()].accepting {
            if is_cancelled() {
                return Err(GeometryLanguageError::Cancelled);
            }
            let selected_value = self.policy_value_with_cancel(
                classes,
                state.clone(),
                &mut memo,
                &mut is_cancelled,
            )?;
            if selected_value.outcomes.get(class_index) != Some(remaining_cost) {
                return Err(GeometryLanguageError::WitnessReconstructionMismatch);
            }
            let Some(&representative) = state.members.first() else {
                return Err(GeometryLanguageError::WitnessReconstructionMismatch);
            };
            let queue = classes.classes[representative].queue();
            let node = &self.language.nodes[state.node.index()];
            let mut selected = None;

            'candidate: for supply in supply_transitions(
                queue,
                state.cursor,
                state.hold,
                self.hold_enabled,
                self.terminal_hold_release_enabled,
            ) {
                if !state.members.iter().copied().all(|member| {
                    supply_transitions(
                        classes.classes[member].queue(),
                        state.cursor,
                        state.hold,
                        self.hold_enabled,
                        self.terminal_hold_release_enabled,
                    )
                    .into_iter()
                    .any(|candidate| candidate == supply)
                }) {
                    continue;
                }
                for edge in node.edges.iter().copied() {
                    if edge.piece != supply.piece {
                        continue;
                    }
                    let action_cost = edge
                        .input_cost
                        .checked_add(supply.hold_cost)
                        .ok_or(GeometryLanguageError::CostOverflow)?;
                    let groups =
                        split_observations(&state.members, classes, supply.cursor, supply.hold);
                    let mut outcomes = PolicyOutcomes::new(classes.classes.len());
                    for members in &groups {
                        let child = self.policy_value_with_cancel(
                            classes,
                            PolicyState {
                                node: edge.child,
                                cursor: supply.cursor,
                                hold: supply.hold,
                                members: members.clone().into_boxed_slice(),
                            },
                            &mut memo,
                            &mut is_cancelled,
                        )?;
                        for outcome in child.outcomes.iter() {
                            outcomes.set_min(
                                outcome.class_index,
                                outcome
                                    .cost
                                    .checked_add(action_cost)
                                    .ok_or(GeometryLanguageError::CostOverflow)?,
                            )?;
                        }
                    }
                    let candidate = PolicyValue::new(classes, outcomes, edge.transition_order);
                    if candidate != selected_value {
                        continue;
                    }
                    let Some(child_members) = groups
                        .into_iter()
                        .find(|members| members.contains(&class_index))
                    else {
                        return Err(GeometryLanguageError::WitnessReconstructionMismatch);
                    };
                    let child = PolicyState {
                        node: edge.child,
                        cursor: supply.cursor,
                        hold: supply.hold,
                        members: child_members.into_boxed_slice(),
                    };
                    let child_value = self.policy_value_with_cancel(
                        classes,
                        child.clone(),
                        &mut memo,
                        &mut is_cancelled,
                    )?;
                    let Some(suffix) = child_value.outcomes.get(class_index) else {
                        return Err(GeometryLanguageError::WitnessReconstructionMismatch);
                    };
                    if action_cost.checked_add(suffix) != Some(remaining_cost) {
                        return Err(GeometryLanguageError::WitnessReconstructionMismatch);
                    }
                    selected = Some((supply, edge, child, suffix));
                    break 'candidate;
                }
            }

            let Some((supply, edge, child, suffix)) = selected else {
                return Err(GeometryLanguageError::WitnessReconstructionMismatch);
            };
            steps.push(FixedQueueRouteStep {
                source: state.node,
                supply_action: supply.action,
                edge,
            });
            state = child;
            remaining_cost = suffix;
        }
        if remaining_cost != 0 {
            return Err(GeometryLanguageError::WitnessReconstructionMismatch);
        }
        Ok(Some(FixedQueueWitness {
            total_cost,
            steps: steps.into_boxed_slice(),
        }))
    }

    /// Restore concrete movement inputs for one selected visible-seven class
    /// after the aggregate policy computation has completed.
    pub fn replay_visible_seven_class_witness(
        &self,
        classes: &QueueClassSet,
        initial_hold: Option<PieceKind>,
        class_index: usize,
        spawn: SpawnProfile,
        kicks: &KickTableProfile,
    ) -> Result<Option<ReplayedFixedQueueWitness>, FinesseRouteWitnessError> {
        self.replay_visible_seven_class_witness_with_cancel(
            classes,
            initial_hold,
            class_index,
            spawn,
            kicks,
            || false,
        )
    }

    /// Restore one visible-seven class movement witness while allowing the
    /// caller to stop policy reconstruction and movement BFS reruns.
    pub fn replay_visible_seven_class_witness_with_cancel(
        &self,
        classes: &QueueClassSet,
        initial_hold: Option<PieceKind>,
        class_index: usize,
        spawn: SpawnProfile,
        kicks: &KickTableProfile,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<Option<ReplayedFixedQueueWitness>, FinesseRouteWitnessError> {
        let Some(route) = self.visible_seven_class_witness_with_cancel(
            classes,
            initial_hold,
            class_index,
            &mut is_cancelled,
        )?
        else {
            return Ok(None);
        };
        self.replay_route_witness_with_cancel(&route, spawn, kicks, &mut is_cancelled)
            .map(Some)
    }

    fn replay_route_witness_with_cancel(
        &self,
        route: &FixedQueueWitness,
        spawn: SpawnProfile,
        kicks: &KickTableProfile,
        is_cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ReplayedFixedQueueWitness, FinesseRouteWitnessError> {
        let mut inputs = Vec::new();
        let mut placements = Vec::with_capacity(route.steps.len());
        for step in route.steps.iter().copied() {
            if is_cancelled() {
                return Err(GeometryLanguageError::Cancelled.into());
            }
            if step.supply_action != QueueSupplyAction::UseCurrent {
                inputs.push(FinesseSequenceInput::Hold);
            }
            let node = &self.language.nodes[step.source.index()];
            let board = node
                .source_board
                .ok_or(FinesseRouteWitnessError::MissingSourceBoard { node: step.source })?;
            let action = step
                .edge
                .action_key
                .ok_or(FinesseRouteWitnessError::MissingActionKey { node: step.source })?;
            placements.push(action);
            let query = FrozenFinesseQuery::new(
                board,
                step.edge.piece,
                spawn,
                kicks.clone(),
                [FinesseTarget::new(action.rotation, action.x, action.y)],
            );
            let witness = match step.edge.terminal_evidence {
                Some(evidence) => {
                    query.witness_for_terminal_evidence_with_cancel(0, evidence, &mut *is_cancelled)
                }
                None => query.witness_with_cancel(0, &mut *is_cancelled),
            }?
            .ok_or(FinesseRouteWitnessError::MovementRouteMissing { node: step.source })?;
            if witness.cost != step.edge.input_cost {
                return Err(FinesseRouteWitnessError::CostMismatch { node: step.source });
            }
            inputs.extend(
                witness
                    .actions
                    .iter()
                    .copied()
                    .map(FinesseSequenceInput::Movement),
            );
        }
        if u32::try_from(inputs.len()).ok() != Some(route.total_cost) {
            return Err(FinesseRouteWitnessError::CostMismatch {
                node: self.language.root,
            });
        }
        Ok(ReplayedFixedQueueWitness {
            total_cost: route.total_cost,
            inputs: inputs.into_boxed_slice(),
            placements: placements.into_boxed_slice(),
        })
    }

    /// Evaluate unique full queues in 64-class chunks. Fixed-queue queries keep
    /// the scalar DP above; pattern Oracle evaluation shares exact product
    /// states and stores intermediate lane costs in adaptive partitions.
    pub fn oracle(
        &self,
        classes: &QueueClassSet,
        initial_hold: Option<PieceKind>,
    ) -> Result<OracleQueueEvaluation, GeometryLanguageError> {
        self.oracle_with_cancel(classes, initial_hold, || false)
    }

    /// Evaluate every unique queue while allowing the caller to stop the
    /// product at class and scalar-state boundaries. The callback stays
    /// generic so this crate does not depend on a runtime cancellation type.
    pub fn oracle_with_cancel(
        &self,
        classes: &QueueClassSet,
        initial_hold: Option<PieceKind>,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<OracleQueueEvaluation, GeometryLanguageError> {
        let mut costs = QueueCostTable::unreachable(classes.classes.len())?;
        for chunk_start in (0..classes.classes.len()).step_by(DENSE_LANES) {
            if is_cancelled() {
                return Err(GeometryLanguageError::Cancelled);
            }
            let chunk_end = (chunk_start + DENSE_LANES).min(classes.classes.len());
            let chunk = &classes.classes[chunk_start..chunk_end];
            let chunk_costs = self.oracle_product_chunk_with_cancel_and_observer(
                chunk,
                initial_hold,
                &mut is_cancelled,
                &mut |_| {},
            )?;
            for lane in 0..chunk.len() {
                if let Some(cost) = chunk_costs.get(lane).flatten() {
                    costs.set_min(chunk_start + lane, cost)?;
                }
            }
        }
        Ok(OracleQueueEvaluation {
            costs,
            universe: classes.metadata,
        })
    }

    /// Optimize one shared action for each visible-seven observation state.
    pub fn visible_seven(
        &self,
        classes: &QueueClassSet,
        initial_hold: Option<PieceKind>,
    ) -> Result<VisibleSevenEvaluation, GeometryLanguageError> {
        self.visible_seven_with_cancel(classes, initial_hold, || false)
    }

    /// Optimize the shared visible-seven policy with cancellation checks at
    /// every observation/product state.
    pub fn visible_seven_with_cancel(
        &self,
        classes: &QueueClassSet,
        initial_hold: Option<PieceKind>,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<VisibleSevenEvaluation, GeometryLanguageError> {
        let mut costs = QueueCostTable::unreachable(classes.classes.len())?;
        let all = (0..classes.classes.len()).collect::<Vec<_>>();
        let initial_groups = split_observations(&all, classes, 0, initial_hold);
        let mut memo = BTreeMap::new();
        for members in initial_groups {
            if is_cancelled() {
                return Err(GeometryLanguageError::Cancelled);
            }
            let value = self.policy_value_with_cancel(
                classes,
                PolicyState {
                    node: self.language.root,
                    cursor: 0,
                    hold: initial_hold,
                    members: members.into_boxed_slice(),
                },
                &mut memo,
                &mut is_cancelled,
            )?;
            for outcome in value.outcomes.iter() {
                costs.set_min(outcome.class_index, outcome.cost)?;
            }
        }
        let score = score_costs(classes, &costs);
        Ok(VisibleSevenEvaluation {
            costs,
            successful_probability_mass: score.probability_mass,
            successful_unique_queue_count: score.unique_queue_count,
            total_inputs: score.total_inputs,
            universe: classes.metadata,
        })
    }

    fn oracle_product_chunk_with_cancel_and_observer(
        &self,
        classes: &[QueueClass],
        initial_hold: Option<PieceKind>,
        is_cancelled: &mut impl FnMut() -> bool,
        observe_storage: &mut impl FnMut(CostVectorStorageKind),
    ) -> Result<CostVector64, GeometryLanguageError> {
        debug_assert!(classes.len() <= DENSE_LANES);
        let lane_count = classes.len();
        let mut result = CostVector64::unreachable(lane_count)?;
        if classes.is_empty() {
            return Ok(result);
        }

        // Every geometry edge advances depth by exactly one, so a pair of
        // exact BTreeMap layers is sufficient. Each semantic product state is
        // expanded once after every predecessor has contributed its strict
        // min cost. This preserves deterministic traversal without retaining
        // route parents or hashes.
        let mut current = BTreeMap::new();
        current.insert(
            ScalarState {
                node: self.language.root,
                cursor: 0,
                hold: initial_hold,
            },
            CostVector64::filled(lane_count, 0)?,
        );

        while !current.is_empty() {
            let mut next = BTreeMap::<ScalarState, CostVector64>::new();
            for (state, state_costs) in current {
                if is_cancelled() {
                    return Err(GeometryLanguageError::Cancelled);
                }
                observe_storage(state_costs.storage_kind());
                let node = &self.language.nodes[state.node.index()];
                if node.accepting {
                    result.relax_masked_from(&state_costs, u64::MAX, 0)?;
                    continue;
                }

                // Queue-dependent supply choices are partitioned once per
                // shared product state. The geometry edge scan and all prefix
                // costs are then reused by every lane in that partition.
                let mut supply_masks = BTreeMap::<SupplyTransition, u64>::new();
                let mut reachable_lanes = state_costs.reachable_lane_mask();
                while reachable_lanes != 0 {
                    let lane = reachable_lanes.trailing_zeros() as usize;
                    reachable_lanes &= reachable_lanes - 1;
                    for supply in supply_transitions(
                        classes[lane].queue(),
                        state.cursor,
                        state.hold,
                        self.hold_enabled,
                        self.terminal_hold_release_enabled,
                    ) {
                        *supply_masks.entry(supply).or_default() |= 1_u64 << lane;
                    }
                }

                for (supply, lane_mask) in supply_masks {
                    let hold_and_edge_cost = |edge: CostedGeometryEdge| {
                        edge.input_cost
                            .checked_add(supply.hold_cost)
                            .filter(|cost| *cost != UNREACHABLE)
                            .ok_or(GeometryLanguageError::CostOverflow)
                    };
                    for edge in node.edges.iter().copied() {
                        if edge.piece != supply.piece {
                            continue;
                        }
                        let child = ScalarState {
                            node: edge.child,
                            cursor: supply.cursor,
                            hold: supply.hold,
                        };
                        let child_costs = match next.entry(child) {
                            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                            std::collections::btree_map::Entry::Vacant(entry) => {
                                entry.insert(CostVector64::unreachable(lane_count)?)
                            }
                        };
                        child_costs.relax_masked_from(
                            &state_costs,
                            lane_mask,
                            hold_and_edge_cost(edge)?,
                        )?;
                    }
                }
            }
            current = next;
        }
        Ok(result)
    }

    fn scalar_cost_with_cancel(
        &self,
        queue: &[PieceKind],
        state: ScalarState,
        memo: &mut BTreeMap<ScalarState, Option<u32>>,
        is_cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<Option<u32>, GeometryLanguageError> {
        if is_cancelled() {
            return Err(GeometryLanguageError::Cancelled);
        }
        if let Some(cached) = memo.get(&state).copied() {
            return Ok(cached);
        }
        let node = &self.language.nodes[state.node.index()];
        if node.accepting {
            memo.insert(state, Some(0));
            return Ok(Some(0));
        }

        let mut best = None;
        for supply in supply_transitions(
            queue,
            state.cursor,
            state.hold,
            self.hold_enabled,
            self.terminal_hold_release_enabled,
        ) {
            for edge in node.edges.iter().copied() {
                if edge.piece != supply.piece {
                    continue;
                }
                let Some(suffix) = self.scalar_cost_with_cancel(
                    queue,
                    ScalarState {
                        node: edge.child,
                        cursor: supply.cursor,
                        hold: supply.hold,
                    },
                    memo,
                    is_cancelled,
                )?
                else {
                    continue;
                };
                let cost = edge
                    .input_cost
                    .checked_add(supply.hold_cost)
                    .and_then(|cost| cost.checked_add(suffix))
                    .ok_or(GeometryLanguageError::CostOverflow)?;
                if cost == UNREACHABLE {
                    return Err(GeometryLanguageError::CostOverflow);
                }
                if best.is_none_or(|current| cost < current) {
                    best = Some(cost);
                }
            }
        }
        memo.insert(state, best);
        Ok(best)
    }

    fn policy_value_with_cancel(
        &self,
        classes: &QueueClassSet,
        state: PolicyState,
        memo: &mut BTreeMap<PolicyState, PolicyValue>,
        is_cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<PolicyValue, GeometryLanguageError> {
        if is_cancelled() {
            return Err(GeometryLanguageError::Cancelled);
        }
        if let Some(cached) = memo.get(&state).cloned() {
            return Ok(cached);
        }
        let node = &self.language.nodes[state.node.index()];
        if node.accepting {
            let mut outcomes = PolicyOutcomes::new(classes.classes.len());
            for class_index in state.members.iter().copied() {
                outcomes.set_min(class_index, 0)?;
            }
            let value = PolicyValue::new(classes, outcomes, 0);
            memo.insert(state, value.clone());
            return Ok(value);
        }
        let Some(&representative) = state.members.first() else {
            let value = PolicyValue::reject(classes.classes.len());
            memo.insert(state, value.clone());
            return Ok(value);
        };
        let queue = classes.classes[representative].queue();
        let mut best: Option<PolicyValue> = None;
        for supply in supply_transitions(
            queue,
            state.cursor,
            state.hold,
            self.hold_enabled,
            self.terminal_hold_release_enabled,
        ) {
            if !state.members.iter().copied().all(|class_index| {
                supply_transitions(
                    classes.classes[class_index].queue(),
                    state.cursor,
                    state.hold,
                    self.hold_enabled,
                    self.terminal_hold_release_enabled,
                )
                .into_iter()
                .any(|candidate| candidate == supply)
            }) {
                continue;
            }
            for edge in node.edges.iter().copied() {
                if edge.piece != supply.piece {
                    continue;
                }
                let action_cost = edge
                    .input_cost
                    .checked_add(supply.hold_cost)
                    .ok_or(GeometryLanguageError::CostOverflow)?;
                if action_cost == UNREACHABLE {
                    return Err(GeometryLanguageError::CostOverflow);
                }
                let groups =
                    split_observations(&state.members, classes, supply.cursor, supply.hold);
                let mut outcomes = PolicyOutcomes::new(classes.classes.len());
                for members in groups {
                    let child = self.policy_value_with_cancel(
                        classes,
                        PolicyState {
                            node: edge.child,
                            cursor: supply.cursor,
                            hold: supply.hold,
                            members: members.into_boxed_slice(),
                        },
                        memo,
                        is_cancelled,
                    )?;
                    for outcome in child.outcomes.iter() {
                        let cost = outcome
                            .cost
                            .checked_add(action_cost)
                            .ok_or(GeometryLanguageError::CostOverflow)?;
                        if cost == UNREACHABLE {
                            return Err(GeometryLanguageError::CostOverflow);
                        }
                        outcomes.set_min(outcome.class_index, cost)?;
                    }
                }
                let candidate = PolicyValue::new(classes, outcomes, edge.transition_order);
                if best
                    .as_ref()
                    .is_none_or(|current| candidate.better_than(current))
                {
                    best = Some(candidate);
                }
            }
        }
        let value = best.unwrap_or_else(|| PolicyValue::reject(classes.classes.len()));
        memo.insert(state, value.clone());
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ScalarState {
    node: GeometryNodeId,
    cursor: usize,
    hold: Option<PieceKind>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SupplyTransition {
    piece: PieceKind,
    cursor: usize,
    hold: Option<PieceKind>,
    hold_cost: u32,
    action: QueueSupplyAction,
}

fn supply_transitions(
    queue: &[PieceKind],
    cursor: usize,
    hold: Option<PieceKind>,
    hold_enabled: bool,
    terminal_hold_release_enabled: bool,
) -> Vec<SupplyTransition> {
    let mut transitions = Vec::with_capacity(2);
    let current = queue.get(cursor).copied();
    if let Some(current) = current {
        transitions.push(SupplyTransition {
            piece: current,
            cursor: cursor + 1,
            hold,
            hold_cost: 0,
            action: QueueSupplyAction::UseCurrent,
        });
        if hold_enabled {
            if let Some(held) = hold {
                transitions.push(SupplyTransition {
                    piece: held,
                    cursor: cursor + 1,
                    hold: Some(current),
                    hold_cost: 1,
                    action: QueueSupplyAction::SwapHeld,
                });
            } else if let Some(next) = queue.get(cursor + 1).copied() {
                transitions.push(SupplyTransition {
                    piece: next,
                    cursor: cursor + 2,
                    hold: Some(current),
                    hold_cost: 1,
                    action: QueueSupplyAction::StoreCurrentUseNext,
                });
            }
        }
    } else if hold_enabled && terminal_hold_release_enabled {
        if let Some(held) = hold {
            transitions.push(SupplyTransition {
                piece: held,
                cursor,
                hold: None,
                hold_cost: 1,
                action: QueueSupplyAction::ReleaseHeld,
            });
        }
    }
    transitions
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ObservationKey {
    cursor: usize,
    hold: Option<PieceKind>,
    visible: Box<[PieceKind]>,
}

fn split_observations(
    members: &[usize],
    classes: &QueueClassSet,
    cursor: usize,
    hold: Option<PieceKind>,
) -> Vec<Vec<usize>> {
    let mut indices = BTreeMap::<ObservationKey, usize>::new();
    let mut groups = Vec::<Vec<usize>>::new();
    for class_index in members.iter().copied() {
        let queue = classes.classes[class_index].queue();
        let visible = queue
            .get(cursor..)
            .unwrap_or_default()
            .iter()
            .take(VISIBLE_COUNT)
            .copied()
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let key = ObservationKey {
            cursor,
            hold,
            visible,
        };
        let group_index = if let Some(index) = indices.get(&key).copied() {
            index
        } else {
            let index = groups.len();
            indices.insert(key, index);
            groups.push(Vec::new());
            index
        };
        groups[group_index].push(class_index);
    }
    groups
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PolicyState {
    node: GeometryNodeId,
    cursor: usize,
    hold: Option<PieceKind>,
    members: Box<[usize]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ClassOutcome {
    class_index: usize,
    cost: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PolicyOutcomeBucket {
    bucket_index: usize,
    costs: CostVector64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PolicyOutcomes {
    class_count: usize,
    buckets: Vec<PolicyOutcomeBucket>,
}

impl PolicyOutcomes {
    fn new(class_count: usize) -> Self {
        Self {
            class_count,
            buckets: Vec::new(),
        }
    }

    fn set_min(&mut self, class_index: usize, cost: u32) -> Result<(), GeometryLanguageError> {
        if class_index >= self.class_count {
            return Err(GeometryLanguageError::CostVectorLaneOutOfRange {
                lane: class_index,
                lanes: self.class_count,
            });
        }
        let bucket_index = class_index / DENSE_LANES;
        let local_lane = class_index % DENSE_LANES;
        let position = match self
            .buckets
            .binary_search_by_key(&bucket_index, |bucket| bucket.bucket_index)
        {
            Ok(position) => position,
            Err(position) => {
                let bucket_start = bucket_index * DENSE_LANES;
                let lane_count = self
                    .class_count
                    .saturating_sub(bucket_start)
                    .min(DENSE_LANES);
                self.buckets.insert(
                    position,
                    PolicyOutcomeBucket {
                        bucket_index,
                        costs: CostVector64::unreachable(lane_count)?,
                    },
                );
                position
            }
        };
        self.buckets[position].costs.set_min(local_lane, cost)?;
        Ok(())
    }

    fn iter(&self) -> impl Iterator<Item = ClassOutcome> + '_ {
        self.buckets.iter().flat_map(|bucket| {
            let base = bucket.bucket_index * DENSE_LANES;
            (0..bucket.costs.len()).filter_map(move |lane| {
                bucket.costs.get(lane).flatten().map(|cost| ClassOutcome {
                    class_index: base + lane,
                    cost,
                })
            })
        })
    }

    fn get(&self, class_index: usize) -> Option<u32> {
        if class_index >= self.class_count {
            return None;
        }
        let bucket_index = class_index / DENSE_LANES;
        let local_lane = class_index % DENSE_LANES;
        self.buckets
            .binary_search_by_key(&bucket_index, |bucket| bucket.bucket_index)
            .ok()
            .and_then(|position| self.buckets[position].costs.get(local_lane))
            .flatten()
    }

    #[cfg(test)]
    fn storage_kinds(&self) -> impl Iterator<Item = CostVectorStorageKind> + '_ {
        self.buckets
            .iter()
            .map(|bucket| bucket.costs.storage_kind())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PolicyScore {
    probability_mass: f64,
    unique_queue_count: usize,
    total_inputs: u64,
    transition_order: u32,
}

#[derive(Clone, Debug, PartialEq)]
struct PolicyValue {
    outcomes: PolicyOutcomes,
    score: PolicyScore,
}

impl PolicyValue {
    fn new(classes: &QueueClassSet, outcomes: PolicyOutcomes, transition_order: u32) -> Self {
        let mut probability_mass = 0.0;
        let mut unique_queue_count = 0;
        let mut total_inputs = 0_u64;
        for outcome in outcomes.iter() {
            probability_mass += classes.classes[outcome.class_index].probability_mass;
            unique_queue_count += 1;
            total_inputs += u64::from(outcome.cost);
        }
        Self {
            score: PolicyScore {
                probability_mass,
                unique_queue_count,
                total_inputs,
                transition_order,
            },
            outcomes,
        }
    }

    fn reject(class_count: usize) -> Self {
        Self {
            outcomes: PolicyOutcomes::new(class_count),
            score: PolicyScore {
                probability_mass: 0.0,
                unique_queue_count: 0,
                total_inputs: 0,
                transition_order: u32::MAX,
            },
        }
    }

    fn better_than(&self, other: &Self) -> bool {
        self.score
            .probability_mass
            .total_cmp(&other.score.probability_mass)
            .is_gt()
            || (self
                .score
                .probability_mass
                .total_cmp(&other.score.probability_mass)
                .is_eq()
                && (self.score.unique_queue_count > other.score.unique_queue_count
                    || (self.score.unique_queue_count == other.score.unique_queue_count
                        && (self.score.total_inputs < other.score.total_inputs
                            || (self.score.total_inputs == other.score.total_inputs
                                && self.score.transition_order < other.score.transition_order)))))
    }
}

fn score_costs(classes: &QueueClassSet, costs: &QueueCostTable) -> PolicyScore {
    let mut score = PolicyScore {
        probability_mass: 0.0,
        unique_queue_count: 0,
        total_inputs: 0,
        transition_order: 0,
    };
    for (class_index, class) in classes.classes.iter().enumerate() {
        if let Some(cost) = costs.get(class_index).flatten() {
            score.probability_mass += class.probability_mass;
            score.unique_queue_count += 1;
            score.total_inputs += u64::from(cost);
        }
    }
    score
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QueueCostAggregation {
    pub successful_unique_queue_count: usize,
    pub total_unique_queue_count: usize,
    pub conditional_mean_inputs: Option<f64>,
    pub successful_probability_mass: f64,
    pub materialized_queue_coverage_complete: bool,
    pub complete: bool,
    pub universe: QueueUniverseMetadata,
}

/// Aggregate one solution's costs using an unweighted unique-queue mean.
pub fn aggregate_unique_queue_costs(
    classes: &QueueClassSet,
    costs: &QueueCostTable,
) -> Result<QueueCostAggregation, GeometryLanguageError> {
    if costs.len() != classes.classes.len() {
        return Err(GeometryLanguageError::CostTableLengthMismatch {
            expected: classes.classes.len(),
            actual: costs.len(),
        });
    }
    let score = score_costs(classes, costs);
    let conditional_mean_inputs = (score.unique_queue_count != 0)
        .then_some(score.total_inputs as f64 / score.unique_queue_count as f64);
    let materialized_queue_coverage_complete = score.unique_queue_count == classes.classes.len();
    Ok(QueueCostAggregation {
        successful_unique_queue_count: score.unique_queue_count,
        total_unique_queue_count: classes.classes.len(),
        conditional_mean_inputs,
        successful_probability_mass: score.probability_mass,
        materialized_queue_coverage_complete,
        // Completeness describes whether the declared universe was evaluated,
        // not whether every queue succeeds in this language.
        complete: classes.metadata.complete,
        universe: classes.metadata,
    })
}

/// Take the strict per-queue minimum across solutions, then aggregate it.
pub fn aggregate_overall_costs(
    classes: &QueueClassSet,
    solution_costs: &[&QueueCostTable],
) -> Result<(QueueCostTable, QueueCostAggregation), GeometryLanguageError> {
    let mut overall = QueueCostTable::unreachable(classes.classes.len())?;
    for costs in solution_costs {
        if costs.len() != classes.classes.len() {
            return Err(GeometryLanguageError::CostTableLengthMismatch {
                expected: classes.classes.len(),
                actual: costs.len(),
            });
        }
        for class_index in 0..classes.classes.len() {
            if let Some(cost) = costs.get(class_index).flatten() {
                overall.set_min(class_index, cost)?;
            }
        }
    }
    let aggregate = aggregate_unique_queue_costs(classes, &overall)?;
    Ok((overall, aggregate))
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        board::board_size::BoardSize, probability::probability_value::ProbabilityValue,
    };
    use clearra_geometry::layout::board64_layout::Board64Layout;
    use clearra_rules::kicks::NoKick;

    use super::*;

    fn probability(value: f64) -> ProbabilityValue {
        ProbabilityValue::new(value).unwrap()
    }

    fn linear_language(pieces: &[PieceKind], costs: &[u32]) -> CostedGeometryLanguage {
        assert_eq!(pieces.len(), costs.len());
        let mut nodes = Vec::new();
        for depth in 0..=pieces.len() {
            let edges = if depth == pieces.len() {
                Vec::new()
            } else {
                vec![CostedGeometryEdge::new(
                    pieces[depth],
                    GeometryNodeId::new(depth as u32 + 1),
                    costs[depth],
                    depth as u32,
                )]
            };
            nodes.push(GeometryLanguageNode::new(
                depth as u16,
                depth == pieces.len(),
                edges,
            ));
        }
        CostedGeometryLanguage::new(GeometryNodeId::new(0), nodes).unwrap()
    }

    fn all_piece_language(depth: usize) -> CostedGeometryLanguage {
        let mut nodes = Vec::with_capacity(depth + 1);
        for layer in 0..=depth {
            let edges = if layer == depth {
                Vec::new()
            } else {
                PieceKind::STANDARD_TETROMINOES
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(piece_index, piece)| {
                        CostedGeometryEdge::new(
                            piece,
                            GeometryNodeId::new(layer as u32 + 1),
                            (piece_index + layer + 1) as u32,
                            piece_index as u32,
                        )
                    })
                    .collect()
            };
            nodes.push(GeometryLanguageNode::new(
                layer as u16,
                layer == depth,
                edges,
            ));
        }
        CostedGeometryLanguage::new(GeometryNodeId::new(0), nodes).unwrap()
    }

    fn encoded_suffix(mut value: usize, digits: usize) -> Vec<PieceKind> {
        (0..digits)
            .map(|_| {
                let piece = PieceKind::STANDARD_TETROMINOES[value % 7];
                value /= 7;
                piece
            })
            .collect()
    }

    #[test]
    fn cost_vectors_select_single_inline_and_dense_storage() {
        let mut vector = CostVector64::unreachable(64).unwrap();
        assert_eq!(vector.storage_kind(), CostVectorStorageKind::Single);

        assert!(vector.set_min(0, 10).unwrap());
        assert_eq!(vector.storage_kind(), CostVectorStorageKind::Inline);
        assert!(vector.set_min(1, 20).unwrap());
        assert!(vector.set_min(2, 30).unwrap());
        assert_eq!(vector.storage_kind(), CostVectorStorageKind::Inline);
        assert!(vector.set_min(3, 40).unwrap());
        assert_eq!(vector.storage_kind(), CostVectorStorageKind::Dense64);
        assert_eq!(vector.get(0), Some(Some(10)));
        assert_eq!(vector.get(4), Some(None));

        for lane in 0..64 {
            assert!(vector.set_min(lane, 0).unwrap());
        }
        assert_eq!(vector.storage_kind(), CostVectorStorageKind::Single);
        assert!((0..64).all(|lane| vector.get(lane) == Some(Some(0))));
        assert!(CostVector64::unreachable(65).is_err());

        let table = QueueCostTable::unreachable(65).unwrap();
        assert_eq!(
            table.storage_kinds().collect::<Vec<_>>(),
            [CostVectorStorageKind::Single, CostVectorStorageKind::Single]
        );
    }

    #[test]
    fn equal_cost_does_not_replace_a_cost_lane() {
        let mut vector = CostVector64::unreachable(1).unwrap();
        assert!(vector.set_min(0, 7).unwrap());
        assert!(!vector.set_min(0, 7).unwrap());
        assert!(!vector.set_min(0, 8).unwrap());
        assert!(vector.set_min(0, 6).unwrap());
        assert_eq!(vector.get(0), Some(Some(6)));
    }

    #[test]
    fn visible_policy_outcomes_use_adaptive_cost_partitions() {
        let mut outcomes = PolicyOutcomes::new(64);
        assert!(outcomes.storage_kinds().next().is_none());

        outcomes.set_min(0, 10).unwrap();
        outcomes.set_min(1, 20).unwrap();
        outcomes.set_min(2, 30).unwrap();
        assert_eq!(
            outcomes.storage_kinds().collect::<Vec<_>>(),
            [CostVectorStorageKind::Inline]
        );

        outcomes.set_min(3, 40).unwrap();
        assert_eq!(
            outcomes.storage_kinds().collect::<Vec<_>>(),
            [CostVectorStorageKind::Dense64]
        );

        for class_index in 0..64 {
            outcomes.set_min(class_index, 0).unwrap();
        }
        assert_eq!(
            outcomes.storage_kinds().collect::<Vec<_>>(),
            [CostVectorStorageKind::Single]
        );
        assert_eq!(outcomes.iter().count(), 64);
    }

    #[test]
    fn queue_classes_preserve_ids_and_raw_incomplete_mass() {
        let patterns = vec![
            QueuePattern::new(PatternId::new(2), vec![PieceKind::I], probability(0.2)),
            QueuePattern::new(PatternId::new(9), vec![PieceKind::I], probability(0.3)),
            QueuePattern::new(PatternId::new(4), vec![PieceKind::O], probability(0.1)),
        ];
        let classes = QueueClassSet::group(&patterns, false).unwrap();

        assert_eq!(classes.classes().len(), 2);
        assert_eq!(
            classes.classes()[0].pattern_ids(),
            [PatternId::new(2), PatternId::new(9)]
        );
        assert_eq!(classes.classes()[0].probability_mass(), 0.5);
        assert_eq!(classes.metadata().materialized_probability_mass, 0.6);
        assert!(!classes.metadata().complete);
    }

    #[test]
    fn queue_classes_reject_duplicate_pattern_ids_before_returning_a_partial_universe() {
        let duplicate = PatternId::new(17);
        let result = QueueClassSet::group(
            &[
                QueuePattern::new(duplicate, vec![PieceKind::I], probability(0.4)),
                QueuePattern::new(duplicate, vec![PieceKind::O], probability(0.6)),
            ],
            true,
        );

        assert_eq!(
            result,
            Err(GeometryLanguageError::DuplicatePatternId(duplicate))
        );
    }

    #[test]
    fn duplicate_queue_and_hold_states_do_not_weight_the_unique_queue_mean_twice() {
        let language = linear_language(&[PieceKind::O], &[4]);
        let classes = QueueClassSet::group(
            &[
                QueuePattern::new(PatternId::new(0), vec![PieceKind::I], probability(0.1)),
                QueuePattern::new(PatternId::new(1), vec![PieceKind::I], probability(0.8)),
                QueuePattern::new(PatternId::new(2), vec![PieceKind::O], probability(0.1)),
            ],
            true,
        )
        .unwrap();
        assert_eq!(classes.classes().len(), 2);
        assert_eq!(classes.classes()[0].pattern_ids().len(), 2);

        let evaluation = QueueClassProductEvaluator::new(&language)
            .oracle(&classes, Some(PieceKind::O))
            .unwrap();
        assert_eq!(evaluation.costs.to_vec(), [Some(5), Some(4)]);
        let aggregate = aggregate_unique_queue_costs(&classes, &evaluation.costs).unwrap();

        assert_eq!(aggregate.successful_unique_queue_count, 2);
        assert_eq!(aggregate.conditional_mean_inputs, Some(4.5));
        assert_eq!(aggregate.successful_probability_mass, 1.0);
    }

    #[test]
    fn scalar_oracle_charges_one_for_hold() {
        let language = linear_language(&[PieceKind::O, PieceKind::I], &[2, 3]);
        let classes = QueueClassSet::group(
            &[QueuePattern::new(
                PatternId::new(0),
                vec![PieceKind::I, PieceKind::O],
                ProbabilityValue::ONE,
            )],
            true,
        )
        .unwrap();
        let result = QueueClassProductEvaluator::new(&language)
            .oracle(&classes, None)
            .unwrap();

        assert_eq!(result.costs.get(0), Some(Some(7)));

        let disabled = QueueClassProductEvaluator::new(&language)
            .with_hold_enabled(false)
            .oracle(&classes, None)
            .unwrap();
        assert_eq!(disabled.costs.get(0), Some(None));
    }

    #[test]
    fn fixed_and_single_class_pattern_keep_explicit_scalar_and_vector_dispatch() {
        let language = linear_language(&[PieceKind::I], &[4]);
        let evaluator = QueueClassProductEvaluator::new(&language).with_hold_enabled(false);
        let mut fixed_state_checks = 0;
        let fixed = evaluator
            .fixed_queue_cost_with_cancel(&[PieceKind::I], None, || {
                fixed_state_checks += 1;
                false
            })
            .unwrap();

        let classes = QueueClassSet::group(
            &[QueuePattern::new(
                PatternId::new(0),
                vec![PieceKind::I],
                ProbabilityValue::ONE,
            )],
            false,
        )
        .unwrap();
        let mut pattern_boundaries = 0;
        let pattern = evaluator
            .oracle_with_cancel(&classes, None, || {
                pattern_boundaries += 1;
                false
            })
            .unwrap();

        assert_eq!(fixed, Some(4));
        assert_eq!(pattern.costs.get(0), Some(Some(4)));
        // The fixed entry point visits only its scalar root and terminal
        // states. Even a one-class pattern intentionally enters the vector
        // dispatcher, adding the explicit 64-class chunk boundary.
        assert_eq!(fixed_state_checks, 2);
        assert_eq!(pattern_boundaries, 3);
    }

    #[test]
    fn fixed_witness_recomputes_the_strict_first_hold_route() {
        let language = linear_language(&[PieceKind::O, PieceKind::I], &[2, 3]);
        let evaluator = QueueClassProductEvaluator::new(&language);
        let witness = evaluator
            .fixed_queue_witness(&[PieceKind::I, PieceKind::O], None)
            .unwrap()
            .unwrap();

        assert_eq!(witness.total_cost(), 7);
        assert_eq!(witness.steps().len(), 2);
        assert_eq!(
            witness.steps()[0].supply_action(),
            QueueSupplyAction::StoreCurrentUseNext
        );
        assert_eq!(
            witness.steps()[1].supply_action(),
            QueueSupplyAction::ReleaseHeld
        );
        assert_eq!(witness.steps()[0].edge().piece(), PieceKind::O);
        assert_eq!(witness.steps()[1].edge().piece(), PieceKind::I);
    }

    #[test]
    fn route_witness_product_reruns_honor_cancellation_inside_dp() {
        let language = linear_language(&[PieceKind::I, PieceKind::O, PieceKind::T], &[1, 1, 1]);
        let classes = QueueClassSet::group(
            &[QueuePattern::new(
                PatternId::new(0),
                vec![PieceKind::I, PieceKind::O, PieceKind::T],
                ProbabilityValue::ONE,
            )],
            true,
        )
        .unwrap();
        let evaluator = QueueClassProductEvaluator::new(&language).with_hold_enabled(false);

        let mut fixed_checks = 0;
        assert_eq!(
            evaluator.fixed_queue_witness_with_cancel(classes.classes()[0].queue(), None, || {
                fixed_checks += 1;
                fixed_checks == 3
            },),
            Err(GeometryLanguageError::Cancelled)
        );

        let mut visible_checks = 0;
        assert_eq!(
            evaluator.visible_seven_class_witness_with_cancel(&classes, None, 0, || {
                visible_checks += 1;
                visible_checks == 3
            }),
            Err(GeometryLanguageError::Cancelled)
        );
    }

    #[test]
    fn replayed_witness_verifies_edge_and_total_input_costs() {
        let layout = Board64Layout::new(BoardSize::new(4, 4).unwrap()).unwrap();
        let board = FinesseBoard::new(layout, 0).unwrap();
        let action = GeometryActionKey::new(PieceKind::O, RotationState::Zero, 1, 0);
        let language = CostedGeometryLanguage::new(
            GeometryNodeId::new(0),
            vec![
                GeometryLanguageNode::new(
                    0,
                    false,
                    vec![
                        CostedGeometryEdge::new(PieceKind::O, GeometryNodeId::new(1), 1, 0)
                            .with_action_key(action),
                    ],
                )
                .with_source_board(board),
                GeometryLanguageNode::new(1, true, Vec::<CostedGeometryEdge>::new()),
            ],
        )
        .unwrap();
        let witness = QueueClassProductEvaluator::new(&language)
            .replay_fixed_queue_witness(
                &[PieceKind::O],
                None,
                SpawnProfile::new(1, 4),
                &NoKick::profile(),
            )
            .unwrap()
            .unwrap();

        assert_eq!(witness.total_cost(), 1);
        assert_eq!(
            witness.inputs(),
            [FinesseSequenceInput::Movement(ClassicInputAction::HardDrop)]
        );
        assert_eq!(witness.placements(), [action]);

        // The seventh boundary is the first movement-BFS dequeue, after the
        // scalar DP and route reconstruction have already completed.
        let mut cancellation_checks = 0;
        let error = QueueClassProductEvaluator::new(&language)
            .replay_fixed_queue_witness_with_cancel(
                &[PieceKind::O],
                None,
                SpawnProfile::new(1, 4),
                &NoKick::profile(),
                || {
                    cancellation_checks += 1;
                    cancellation_checks == 7
                },
            )
            .unwrap_err();
        assert_eq!(
            error,
            FinesseRouteWitnessError::Movement(FinesseError::Cancelled)
        );
        assert_eq!(cancellation_checks, 7);
    }

    #[test]
    fn language_union_merges_the_same_action_before_solutions_diverge() {
        let common = GeometryActionKey::new(PieceKind::I, RotationState::Zero, 0, 0);
        let language = |suffix_piece, suffix_x| {
            CostedGeometryLanguage::new(
                GeometryNodeId::new(0),
                vec![
                    GeometryLanguageNode::new(
                        0,
                        false,
                        vec![
                            CostedGeometryEdge::new(PieceKind::I, GeometryNodeId::new(1), 2, 0)
                                .with_action_key(common),
                        ],
                    ),
                    GeometryLanguageNode::new(
                        1,
                        false,
                        vec![
                            CostedGeometryEdge::new(suffix_piece, GeometryNodeId::new(2), 3, 1)
                                .with_action_key(GeometryActionKey::new(
                                    suffix_piece,
                                    RotationState::Zero,
                                    suffix_x,
                                    0,
                                )),
                        ],
                    ),
                    GeometryLanguageNode::new(2, true, Vec::<CostedGeometryEdge>::new()),
                ],
            )
            .unwrap()
        };
        let left = language(PieceKind::O, -1);
        let right = language(PieceKind::T, 1);
        let union = union_costed_geometry_languages(&[&left, &right]).unwrap();

        let root_edges = union.node(union.root()).unwrap().edges();
        assert_eq!(root_edges.len(), 1);
        assert_eq!(root_edges[0].action_key(), Some(common));
        let suffix_edges = union.node(root_edges[0].child()).unwrap().edges();
        assert_eq!(suffix_edges.len(), 2);
        assert_eq!(
            suffix_edges
                .iter()
                .map(|edge| edge.piece())
                .collect::<Vec<_>>(),
            [PieceKind::O, PieceKind::T]
        );
    }

    #[test]
    fn oracle_evaluates_identical_full_queues_once_per_class() {
        let language = linear_language(&[PieceKind::I], &[4]);
        let classes = QueueClassSet::group(
            &[
                QueuePattern::new(PatternId::new(0), vec![PieceKind::I], probability(0.4)),
                QueuePattern::new(PatternId::new(1), vec![PieceKind::I], probability(0.6)),
            ],
            true,
        )
        .unwrap();
        let result = QueueClassProductEvaluator::new(&language)
            .oracle(&classes, None)
            .unwrap();

        assert_eq!(result.costs.len(), 1);
        assert_eq!(result.costs.get(0), Some(Some(4)));
    }

    #[test]
    fn oracle_vector_product_matches_scalar_reference_across_three_chunks_with_hold() {
        let language = all_piece_language(2);
        let unique_mass = 0.5 / 130.0;
        let mut patterns = (0..130)
            .map(|index| {
                QueuePattern::new(
                    PatternId::new(index),
                    encoded_suffix(index, 3),
                    probability(unique_mass),
                )
            })
            .collect::<Vec<_>>();
        patterns.push(QueuePattern::new(
            PatternId::new(130),
            encoded_suffix(0, 3),
            probability(0.2),
        ));
        patterns.push(QueuePattern::new(
            PatternId::new(131),
            encoded_suffix(1, 3),
            probability(0.1),
        ));
        let classes = QueueClassSet::group(&patterns, false).unwrap();
        assert_eq!(classes.classes().len(), 130);
        assert_eq!(
            classes.classes()[0].pattern_ids(),
            [PatternId::new(0), PatternId::new(130)]
        );
        assert!((classes.classes()[0].probability_mass() - (unique_mass + 0.2)).abs() < 1e-12);
        assert!((classes.metadata().materialized_probability_mass - 0.8).abs() < 1e-12);
        assert!(!classes.metadata().complete);

        let evaluator = QueueClassProductEvaluator::new(&language);
        let vector = evaluator.oracle(&classes, Some(PieceKind::I)).unwrap();
        for (class_index, class) in classes.classes().iter().enumerate() {
            assert_eq!(
                vector.costs.get(class_index).unwrap(),
                evaluator
                    .fixed_queue_cost(class.queue(), Some(PieceKind::I))
                    .unwrap(),
                "queue class {class_index} diverged from scalar min-plus"
            );
        }
    }

    #[test]
    fn oracle_product_intermediates_use_single_inline_and_dense_partitions() {
        let observed = |distinct_costs: usize| {
            let language = all_piece_language(1);
            let patterns = (0..64)
                .map(|index| {
                    let mut queue = vec![PieceKind::STANDARD_TETROMINOES[index % distinct_costs]];
                    queue.extend(encoded_suffix(index, 3));
                    QueuePattern::new(PatternId::new(index), queue, probability(1.0 / 64.0))
                })
                .collect::<Vec<_>>();
            let classes = QueueClassSet::group(&patterns, true).unwrap();
            let evaluator = QueueClassProductEvaluator::new(&language).with_hold_enabled(false);
            let mut kinds = Vec::new();
            let result = evaluator
                .oracle_product_chunk_with_cancel_and_observer(
                    classes.classes(),
                    None,
                    &mut || false,
                    &mut |kind| kinds.push(kind),
                )
                .unwrap();
            assert!((0..result.len()).all(|lane| result.get(lane).flatten().is_some()));
            kinds
        };

        assert_eq!(
            observed(1),
            [CostVectorStorageKind::Single, CostVectorStorageKind::Single]
        );
        assert_eq!(
            observed(4),
            [CostVectorStorageKind::Single, CostVectorStorageKind::Inline]
        );
        assert_eq!(
            observed(5),
            [
                CostVectorStorageKind::Single,
                CostVectorStorageKind::Dense64
            ]
        );
    }

    #[test]
    fn oracle_shares_one_prefix_product_expansion_across_sixty_four_classes() {
        let language = linear_language(&[PieceKind::I, PieceKind::O], &[2, 3]);
        let patterns = (0..64)
            .map(|index| {
                let mut queue = vec![PieceKind::I, PieceKind::O];
                queue.extend(encoded_suffix(index, 3));
                QueuePattern::new(PatternId::new(index), queue, probability(1.0 / 64.0))
            })
            .collect::<Vec<_>>();
        let classes = QueueClassSet::group(&patterns, true).unwrap();
        let evaluator = QueueClassProductEvaluator::new(&language).with_hold_enabled(false);
        let mut cancellation_boundaries = 0;
        let result = evaluator
            .oracle_with_cancel(&classes, None, || {
                cancellation_boundaries += 1;
                false
            })
            .unwrap();

        assert!(result.costs.to_vec().iter().all(|cost| *cost == Some(5)));
        // One chunk boundary plus root, shared prefix, and accepting product
        // states. Independent scalar evaluation would visit the three states
        // once for each of the 64 unique full queue strings.
        assert_eq!(cancellation_boundaries, 4);
        assert!(cancellation_boundaries < classes.classes().len() * 3);
    }

    #[test]
    fn product_evaluators_cancel_inside_recursive_states() {
        let language = linear_language(&[PieceKind::I, PieceKind::O, PieceKind::T], &[1, 1, 1]);
        let classes = QueueClassSet::group(
            &[QueuePattern::new(
                PatternId::new(0),
                vec![PieceKind::I, PieceKind::O, PieceKind::T],
                probability(1.0),
            )],
            true,
        )
        .unwrap();
        let evaluator = QueueClassProductEvaluator::new(&language);

        let mut oracle_checks = 0;
        assert_eq!(
            evaluator.oracle_with_cancel(&classes, None, || {
                oracle_checks += 1;
                oracle_checks == 3
            }),
            Err(GeometryLanguageError::Cancelled)
        );
        let mut visible_checks = 0;
        assert_eq!(
            evaluator.visible_seven_with_cancel(&classes, None, || {
                visible_checks += 1;
                visible_checks == 3
            }),
            Err(GeometryLanguageError::Cancelled)
        );
    }

    #[test]
    fn visible_seven_splits_only_after_a_new_piece_is_revealed() {
        let common = [
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ];
        let mut nodes = Vec::new();
        for (depth, piece) in common.iter().copied().enumerate() {
            nodes.push(GeometryLanguageNode::new(
                depth as u16,
                false,
                vec![CostedGeometryEdge::new(
                    piece,
                    GeometryNodeId::new(depth as u32 + 1),
                    1,
                    depth as u32,
                )],
            ));
        }
        nodes.push(GeometryLanguageNode::new(
            7,
            false,
            vec![
                CostedGeometryEdge::new(PieceKind::I, GeometryNodeId::new(8), 1, 7),
                CostedGeometryEdge::new(PieceKind::O, GeometryNodeId::new(8), 1, 8),
            ],
        ));
        nodes.push(GeometryLanguageNode::new(
            8,
            true,
            Vec::<CostedGeometryEdge>::new(),
        ));
        let language = CostedGeometryLanguage::new(GeometryNodeId::new(0), nodes).unwrap();
        let mut queue_i = common.to_vec();
        queue_i.push(PieceKind::I);
        let mut queue_o = common.to_vec();
        queue_o.push(PieceKind::O);
        let classes = QueueClassSet::group(
            &[
                QueuePattern::new(PatternId::new(0), queue_i, probability(0.5)),
                QueuePattern::new(PatternId::new(1), queue_o, probability(0.5)),
            ],
            true,
        )
        .unwrap();

        let result = QueueClassProductEvaluator::new(&language)
            .visible_seven(&classes, None)
            .unwrap();
        assert_eq!(result.successful_unique_queue_count, 2);
        assert_eq!(result.successful_probability_mass, 1.0);
    }

    #[test]
    fn visible_policy_prefers_mass_then_count_then_inputs_then_transition_order() {
        let language = CostedGeometryLanguage::new(
            GeometryNodeId::new(0),
            vec![
                GeometryLanguageNode::new(
                    0,
                    false,
                    vec![
                        CostedGeometryEdge::new(PieceKind::I, GeometryNodeId::new(1), 8, 0),
                        CostedGeometryEdge::new(PieceKind::I, GeometryNodeId::new(2), 2, 1),
                    ],
                ),
                GeometryLanguageNode::new(1, true, Vec::<CostedGeometryEdge>::new()),
                GeometryLanguageNode::new(1, true, Vec::<CostedGeometryEdge>::new()),
            ],
        )
        .unwrap();
        let classes = QueueClassSet::group(
            &[QueuePattern::new(
                PatternId::new(0),
                vec![PieceKind::I],
                ProbabilityValue::ONE,
            )],
            true,
        )
        .unwrap();

        let result = QueueClassProductEvaluator::new(&language)
            .visible_seven(&classes, None)
            .unwrap();
        assert_eq!(result.costs.get(0), Some(Some(2)));

        let witness = QueueClassProductEvaluator::new(&language)
            .visible_seven_class_witness(&classes, None, 0)
            .unwrap()
            .expect("the selected visible policy succeeds");
        assert_eq!(witness.total_cost(), 2);
        assert_eq!(witness.steps().len(), 1);
        assert_eq!(witness.steps()[0].edge().input_cost(), 2);
        assert_eq!(witness.steps()[0].edge().transition_order(), 1);
    }

    #[test]
    fn visible_witness_replays_the_shared_policy_instead_of_the_full_queue_minimum() {
        let common = [
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ];
        let mut nodes = Vec::with_capacity(17);
        nodes.push(GeometryLanguageNode::new(
            0,
            false,
            vec![
                CostedGeometryEdge::new(PieceKind::I, GeometryNodeId::new(1), 5, 0),
                CostedGeometryEdge::new(PieceKind::I, GeometryNodeId::new(9), 1, 1),
            ],
        ));
        for depth in 1..=8 {
            let edges = if depth == 8 {
                Vec::new()
            } else if depth == 7 {
                vec![
                    CostedGeometryEdge::new(PieceKind::I, GeometryNodeId::new(8), 1, 7),
                    CostedGeometryEdge::new(PieceKind::O, GeometryNodeId::new(8), 1, 8),
                ]
            } else {
                vec![CostedGeometryEdge::new(
                    common.get(depth).copied().expect("common prefix piece"),
                    GeometryNodeId::new(depth as u32 + 1),
                    1,
                    depth as u32,
                )]
            };
            nodes.push(GeometryLanguageNode::new(depth as u16, depth == 8, edges));
        }
        for depth in 1..=8 {
            let node_id = 8 + depth;
            let edges = if depth == 8 {
                Vec::new()
            } else if depth == 7 {
                vec![CostedGeometryEdge::new(
                    PieceKind::I,
                    GeometryNodeId::new(16),
                    1,
                    7,
                )]
            } else {
                vec![CostedGeometryEdge::new(
                    common.get(depth).copied().expect("common prefix piece"),
                    GeometryNodeId::new(node_id as u32 + 1),
                    1,
                    depth as u32,
                )]
            };
            nodes.push(GeometryLanguageNode::new(depth as u16, depth == 8, edges));
        }
        let language = CostedGeometryLanguage::new(GeometryNodeId::new(0), nodes).unwrap();
        let mut queue_i = common.to_vec();
        queue_i.push(PieceKind::I);
        let mut queue_o = common.to_vec();
        queue_o.push(PieceKind::O);
        let classes = QueueClassSet::group(
            &[
                QueuePattern::new(PatternId::new(0), queue_i, probability(0.4)),
                QueuePattern::new(PatternId::new(1), queue_o, probability(0.6)),
            ],
            true,
        )
        .unwrap();
        let evaluator = QueueClassProductEvaluator::new(&language).with_hold_enabled(false);
        let visible = evaluator.visible_seven(&classes, None).unwrap();
        let oracle_for_i = evaluator
            .fixed_queue_cost(classes.classes()[0].queue(), None)
            .unwrap();
        let witness = evaluator
            .visible_seven_class_witness(&classes, None, 0)
            .unwrap()
            .expect("the shared visible policy succeeds for the selected class");

        assert_eq!(oracle_for_i, Some(8));
        assert_eq!(visible.costs.get(0), Some(Some(12)));
        assert_eq!(witness.total_cost(), 12);
        assert_eq!(witness.steps()[0].edge().input_cost(), 5);
        assert_eq!(witness.steps()[0].edge().transition_order(), 0);
    }

    #[test]
    fn visible_policy_score_uses_the_declared_lexicographic_priority() {
        let value = |mass, count, inputs, order| PolicyValue {
            outcomes: PolicyOutcomes::new(0),
            score: PolicyScore {
                probability_mass: mass,
                unique_queue_count: count,
                total_inputs: inputs,
                transition_order: order,
            },
        };

        assert!(value(0.6, 1, 100, 9).better_than(&value(0.5, 9, 1, 0)));
        assert!(value(0.5, 2, 100, 9).better_than(&value(0.5, 1, 1, 0)));
        assert!(value(0.5, 2, 8, 9).better_than(&value(0.5, 2, 9, 0)));
        assert!(value(0.5, 2, 8, 3).better_than(&value(0.5, 2, 8, 4)));
        assert!(!value(0.5, 2, 8, 3).better_than(&value(0.5, 2, 8, 3)));
    }

    #[test]
    fn aggregation_uses_unweighted_unique_queue_conditional_mean() {
        let classes = QueueClassSet::group(
            &[
                QueuePattern::new(PatternId::new(0), vec![PieceKind::I], probability(0.9)),
                QueuePattern::new(PatternId::new(1), vec![PieceKind::O], probability(0.1)),
            ],
            false,
        )
        .unwrap();
        let mut costs = QueueCostTable::unreachable(2).unwrap();
        costs.set_min(0, 2).unwrap();
        costs.set_min(1, 8).unwrap();

        let aggregate = aggregate_unique_queue_costs(&classes, &costs).unwrap();
        assert_eq!(aggregate.conditional_mean_inputs, Some(5.0));
        assert!(aggregate.materialized_queue_coverage_complete);
        assert!(!aggregate.complete);
        assert_eq!(aggregate.successful_probability_mass, 1.0);
    }

    #[test]
    fn complete_universe_stays_complete_when_some_queues_fail() {
        let classes = QueueClassSet::group(
            &[
                QueuePattern::new(PatternId::new(0), vec![PieceKind::I], probability(0.5)),
                QueuePattern::new(PatternId::new(1), vec![PieceKind::O], probability(0.5)),
            ],
            true,
        )
        .unwrap();
        let mut costs = QueueCostTable::unreachable(2).unwrap();
        costs.set_min(0, 3).unwrap();

        let aggregate = aggregate_unique_queue_costs(&classes, &costs).unwrap();
        assert!(aggregate.complete);
        assert!(!aggregate.materialized_queue_coverage_complete);
        assert_eq!(aggregate.successful_unique_queue_count, 1);
        assert_eq!(aggregate.successful_probability_mass, 0.5);
    }
}
