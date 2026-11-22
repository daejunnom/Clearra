//! Complete source-relative closure of certified physical board transitions.
//!
//! The request capability is issued only for a completed, unrestricted PC
//! candidate family over one original source universe. This is not equality
//! of candidate residuals: it joins their proved transitions at the sufficient
//! physical state (board, remaining locks). Every joined edge is still one
//! actual lock followed by its deterministic line clear. The parent supplies
//! the whole-family completeness authority before requesting a frozen view.
//! Kind suffix roots remain dormant until the observation evaluator grants
//! the original-source reveal certificate.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use crate::backend::wasm_cpu::{
    buildup::place_and_clear,
    catalog::GeometryCatalog,
    piece_order_language::PieceOrderLanguageCache,
    placement_poset::CandidateGeometry,
    queue_observation_policy::{
        kind_union::{KindUnionArena, KindUnionResources, KnownSupplyQuery, KIND_SOURCE_TAG},
        ObservationLanguageNode, ObservationPieceLanguage,
    },
    reachability::{harddrop_path_mask, ReachabilityWorkspace, ReachableLocks},
    symbolic_prefix_domain::MAX_PREFIX_WORDS,
    symbolic_prefix_orders::{
        ExactPrefixRelation, SourceQualifiedPrefixOrders, SymbolicPrefixOrders,
    },
    WasmExactSearchError, MAX_BOARD64_PIECES,
};

use super::{
    invalid, piece_code, storage, MAX_EDGES, MAX_NODES, MAX_UNION_MEMBERS, MAX_WORK, NO_NODE,
};

#[cfg(any(test, feature = "local-search-ab"))]
#[path = "queue_observation_prefix_masks.rs"]
mod prefix_masks;
#[cfg(any(test, feature = "local-search-ab"))]
use prefix_masks::{PrefixMaskCache, PrefixMaskPlan, MAX_MASK_OPERATIONS, RUNTIME_WORD_WORK};

#[cfg(any(test, feature = "parallel"))]
#[path = "queue_observation_completed_boundary_quotient.rs"]
mod completed_boundary_quotient;

#[cfg(any(test, feature = "parallel"))]
#[path = "queue_observation_shared_producer.rs"]
pub(in crate::backend::wasm_cpu) mod shared_producer;

#[path = "queue_observation_ordered_policy_quotient.rs"]
mod ordered_policy_quotient;
use ordered_policy_quotient::OrderedPolicyQuotient;

const REVEALED_BOARD_TAG: u32 = 1 << 30;
const KIND_TAG: u32 = KIND_SOURCE_TAG;
const KIND_UNION_TAG: u32 = 1 << 29;
const SMALL_ROOTS: usize = 32;
const PREFIX_CONE_SLOTS: usize = 65_536;
const EXACT_CONE_LIMIT: usize = MAX_NODES;
const EXACT_CONE_CHUNK: usize = 4_096;
const EXACT_CONE_FIRST_BUCKETS: usize = 1_024;
const EXACT_CONE_MAX_BUCKETS: usize = 524_288;
const CONE_FRONTIER_WORDS: [[u64; MAX_PREFIX_WORDS]; 6] = cone_frontier_words();
const _: () = assert!(MAX_NODES < KIND_UNION_TAG as usize);
const _: () = assert!(PREFIX_CONE_SLOTS.is_power_of_two());

const fn cone_frontier_words() -> [[u64; MAX_PREFIX_WORDS]; 6] {
    let mut words = [[0; MAX_PREFIX_WORDS]; 6];
    let mut prefix = 0_usize;
    while prefix < MAX_PREFIX_WORDS * 64 {
        let depth = prefix.count_ones() as usize;
        if depth < words.len() {
            words[depth][prefix / 64] |= 1_u64 << (prefix % 64);
        }
        prefix += 1;
    }
    words
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EdgeClosureMode {
    Off,
    #[cfg(any(test, feature = "local-search-ab"))]
    Full,
    #[cfg(any(test, feature = "local-search-ab"))]
    BeforeLastPhysicalLayer,
}

#[derive(Debug)]
pub(in crate::backend::wasm_cpu) struct BoardObservationPlan {
    width: u8,
    height: u8,
    initial_board: u64,
    total_depth: u8,
    hidden_horizon: u8,
    reveal_floor: u8,
    boundary_suffixes_only: bool,
    // Issued once by the request factory after checking the complete original
    // fresh-bag/hold/projection contract. Candidate-local data cannot grant it.
    fresh_bag_observation: bool,
    source_filter: bool,
    prefix_cones_enabled: bool,
    prefix_cones_exact: bool,
    prefix_cones_ancestors_only: bool,
    boundary_classes_enabled: bool,
    ordered_classes_enabled: bool,
    edge_closure: EdgeClosureMode,
    #[cfg(any(test, feature = "local-search-ab"))]
    prefix_masks_enabled: bool,
    // A bound certified from the original source, valid through H. This
    // capability is not inferred from a candidate or its surviving paths.
    prefix_kind_limits: Option<[u8; 7]>,
    board_mask: u64,
}

impl BoardObservationPlan {
    pub(in crate::backend::wasm_cpu) fn new(
        width: u8,
        height: u8,
        initial_board: u64,
        total_depth: u8,
        hidden_horizon: u8,
        reveal_floor: u8,
        prefix_kind_limits: Option<[u8; 7]>,
    ) -> Result<Arc<Self>, WasmExactSearchError> {
        let cells = usize::from(width) * usize::from(height);
        if width == 0
            || height == 0
            || height > 6
            || cells > 64
            || total_depth == 0
            || usize::from(total_depth) > MAX_BOARD64_PIECES
            || hidden_horizon > total_depth
            || reveal_floor > hidden_horizon
        {
            return Err(invalid("wasm_observation_board_plan_invalid"));
        }
        let board_mask = if cells == 64 {
            u64::MAX
        } else {
            (1_u64 << cells) - 1
        };
        if initial_board & !board_mask != 0
            || place_and_clear(width, height, initial_board).0 != initial_board
        {
            return Err(invalid("wasm_observation_board_plan_invalid"));
        }
        let cone_mode = prefix_cone_mode()?;
        Ok(Arc::new(Self {
            width,
            height,
            initial_board,
            total_depth,
            hidden_horizon,
            reveal_floor,
            boundary_suffixes_only: boundary_suffixes_enabled()?,
            fresh_bag_observation: false,
            source_filter: false,
            prefix_cones_enabled: cone_mode != 0,
            prefix_cones_exact: cone_mode >= 2,
            prefix_cones_ancestors_only: cone_mode == 3,
            boundary_classes_enabled: boundary_classes_enabled()?,
            ordered_classes_enabled: ordered_classes_enabled()?,
            edge_closure: edge_closure_mode()?,
            #[cfg(any(test, feature = "local-search-ab"))]
            prefix_masks_enabled: prefix_masks_enabled()?,
            prefix_kind_limits,
            board_mask,
        }))
    }

    #[cfg(test)]
    pub(in crate::backend::wasm_cpu) fn initial_board(&self) -> u64 {
        self.initial_board
    }
    #[cfg(test)]
    pub(in crate::backend::wasm_cpu) fn prefix_kind_limits(&self) -> Option<[u8; 7]> {
        self.prefix_kind_limits
    }
    pub(in crate::backend::wasm_cpu) fn total_depth(&self) -> u8 {
        self.total_depth
    }
    pub(in crate::backend::wasm_cpu) fn hidden_horizon(&self) -> u8 {
        self.hidden_horizon
    }
    pub(in crate::backend::wasm_cpu) fn reveal_floor(&self) -> u8 {
        self.reveal_floor
    }
    pub(in crate::backend::wasm_cpu) fn boundary_suffixes_only(&self) -> bool {
        self.boundary_suffixes_only
    }
    pub(in crate::backend::wasm_cpu) fn fresh_bag_observation(&self) -> bool {
        self.fresh_bag_observation
    }
    pub(in crate::backend::wasm_cpu) fn enable_fresh_bag_observation(&mut self) {
        self.fresh_bag_observation = true;
    }
    pub(in crate::backend::wasm_cpu) fn source_filter(&self) -> bool {
        self.source_filter
    }
    pub(in crate::backend::wasm_cpu) fn enable_source_filter(&mut self) {
        self.source_filter = true;
    }

    pub(in crate::backend::wasm_cpu) fn boundary_classes_selected(&self) -> bool {
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            self.boundary_classes_enabled
                && self.fresh_bag_observation
                && self.source_filter
                && self.boundary_suffixes_only
                && (1..=5).contains(&self.hidden_horizon)
                && self.hidden_horizon < self.total_depth
                && self.edge_closure == EdgeClosureMode::BeforeLastPhysicalLayer
        }
        #[cfg(not(any(test, feature = "local-search-ab")))]
        {
            false
        }
    }

    fn requires_suffix(&self, depth: u8) -> bool {
        if self.boundary_suffixes_only {
            depth == self.hidden_horizon
        } else {
            depth >= self.reveal_floor
        }
    }

    fn should_rebuild(&self, depth: u8) -> bool {
        depth < self.hidden_horizon
            && match self.edge_closure {
                EdgeClosureMode::Off => false,
                #[cfg(any(test, feature = "local-search-ab"))]
                EdgeClosureMode::Full => true,
                #[cfg(any(test, feature = "local-search-ab"))]
                EdgeClosureMode::BeforeLastPhysicalLayer => depth + 1 < self.hidden_horizon,
            }
    }

    /// Shared once by the request and its workers, excluded from family bytes.
    pub(in crate::backend::wasm_cpu) fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>() + 2 * core::mem::size_of::<usize>()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct BoardKey {
    board: u64,
    remaining: u8,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BoardEdge {
    piece: u8,
    cells: u64,
    child: u32,
}

/// Registration handles are never policy node IDs. A pending boundary names
/// one still-growing H board group; it becomes an executable class only after
/// the complete native family has closed production.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct ProducerNode(u32);

impl ProducerNode {
    const BOUNDARY_TAG: u32 = 1 << 28;
    const MISSING: Self = Self(NO_NODE);

    fn physical(node: u32) -> Self {
        debug_assert!(node < Self::BOUNDARY_TAG);
        Self(node)
    }

    fn pending(group: u32) -> Result<Self, WasmExactSearchError> {
        (group < Self::BOUNDARY_TAG)
            .then_some(Self(group | Self::BOUNDARY_TAG))
            .ok_or_else(|| invalid("wasm_observation_boundary_group_limit"))
    }

    fn physical_node(self) -> Option<u32> {
        (self.0 < Self::BOUNDARY_TAG).then_some(self.0)
    }

    fn boundary_group(self) -> Option<u32> {
        (self.0 != NO_NODE && self.0 & Self::BOUNDARY_TAG != 0)
            .then_some(self.0 & !Self::BOUNDARY_TAG)
    }
}

#[derive(Clone, Copy)]
struct ProducerEdge {
    piece: u8,
    cells: u64,
    child: ProducerNode,
}

#[derive(Clone, Copy)]
struct PendingBoundaryEdge {
    piece: u8,
    cells: u64,
    group: u32,
}

struct PendingBoundaryGroup {
    key: BoardKey,
    roots: SuffixRoots,
}

#[derive(Default)]
struct PendingBoundaries {
    groups: Vec<PendingBoundaryGroup>,
    index: HashMap<BoardKey, u32>,
    edges: HashMap<u32, Vec<PendingBoundaryEdge>>,
    root_capacity_bytes: usize,
    edge_capacity_bytes: usize,
}

impl PendingBoundaries {
    fn retained_bytes(&self) -> usize {
        self.groups.capacity() * core::mem::size_of::<PendingBoundaryGroup>()
            + self.index.capacity() * (core::mem::size_of::<(BoardKey, u32)>() + 1)
            + self.edges.capacity() * (core::mem::size_of::<(u32, Vec<PendingBoundaryEdge>)>() + 1)
            + self.root_capacity_bytes
            + self.edge_capacity_bytes
    }
}

#[derive(Clone, Copy)]
struct ClosureLock {
    cells: u64,
    corridor: u64,
    floor: bool,
    // Different rotations may denote the same physical four cells. Any one
    // exact reachable pose authorizes this action; all aliases remain here.
    poses: ReachableLocks,
}

#[derive(Clone, Copy)]
struct ClosureTarget {
    lock: usize,
    child: u32,
}

enum SuffixRoots {
    // Mutable Small remains sorted for binary-search insertion.
    Small(Vec<u32>),
    Large(HashSet<u32>),
    // Issued only for a completed Boundary class. Its exact unique set is
    // immutable; neither class equality nor revealed membership needs order.
    #[cfg_attr(not(any(test, feature = "parallel")), allow(dead_code))]
    FrozenUniqueUnordered(Vec<u32>),
}

impl Default for SuffixRoots {
    fn default() -> Self {
        Self::Small(Vec::new())
    }
}

impl SuffixRoots {
    fn len(&self) -> usize {
        match self {
            Self::Small(roots) | Self::FrozenUniqueUnordered(roots) => roots.len(),
            Self::Large(roots) => roots.len(),
        }
    }

    fn retained_bytes(&self) -> usize {
        match self {
            Self::Small(roots) | Self::FrozenUniqueUnordered(roots) => {
                roots.capacity() * core::mem::size_of::<u32>()
            }
            Self::Large(roots) => roots.capacity() * (core::mem::size_of::<u32>() + 1),
        }
    }

    /// Returns (new member, temporary old-buffer bytes). Small vectors stay
    /// sorted; promotion avoids quadratic insertion into a large root union.
    fn insert(
        &mut self,
        root: u32,
        allow_new: bool,
    ) -> Result<(bool, usize), WasmExactSearchError> {
        let old_bytes = self.retained_bytes();
        match self {
            Self::Small(roots) => match roots.binary_search(&root) {
                Ok(_) => Ok((false, 0)),
                Err(index) => {
                    if !allow_new {
                        return Err(invalid("wasm_observation_physical_union_limit"));
                    }
                    if roots.len() < SMALL_ROOTS {
                        let old_capacity = roots.capacity();
                        roots.try_reserve(1).map_err(|_| storage())?;
                        let extra = if roots.capacity() != old_capacity {
                            old_bytes
                        } else {
                            0
                        };
                        roots.insert(index, root);
                        Ok((true, extra))
                    } else {
                        let mut large = HashSet::new();
                        large.try_reserve(roots.len() + 1).map_err(|_| storage())?;
                        large.extend(roots.iter().copied());
                        large.insert(root);
                        *self = Self::Large(large);
                        Ok((true, old_bytes))
                    }
                }
            },
            Self::Large(roots) => {
                if roots.contains(&root) {
                    return Ok((false, 0));
                }
                if !allow_new {
                    return Err(invalid("wasm_observation_physical_union_limit"));
                }
                let old_capacity = roots.capacity();
                roots.try_reserve(1).map_err(|_| storage())?;
                let extra = if roots.capacity() != old_capacity {
                    old_bytes
                } else {
                    0
                };
                roots.insert(root);
                Ok((true, extra))
            }
            Self::FrozenUniqueUnordered(_) => {
                Err(invalid("wasm_observation_boundary_producer_closed"))
            }
        }
    }

    fn freeze(&mut self) -> Result<usize, WasmExactSearchError> {
        let Self::Large(roots) = self else {
            return Ok(0);
        };
        let old_bytes = roots.capacity() * (core::mem::size_of::<u32>() + 1);
        let mut sorted = Vec::new();
        sorted
            .try_reserve_exact(roots.len())
            .map_err(|_| storage())?;
        sorted.extend(roots.iter().copied());
        sorted.sort_unstable();
        *self = Self::Small(sorted);
        Ok(old_bytes)
    }

    fn frozen(&self) -> Option<&[u32]> {
        match self {
            Self::Small(roots) | Self::FrozenUniqueUnordered(roots) => Some(roots),
            Self::Large(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum BoardState {
    Physical(BoardKey),
    // Only complete H<n root sets may enter this state. No representative
    // board authorizes an incoming physical transition after quotienting.
    Boundary { remaining: u8 },
}

struct BoardNode {
    state: BoardState,
    edges: Vec<BoardEdge>,
    suffixes: SuffixRoots,
}

impl BoardNode {
    fn physical_key(&self) -> Option<BoardKey> {
        match self.state {
            BoardState::Physical(key) => Some(key),
            BoardState::Boundary { .. } => None,
        }
    }

    fn remaining(&self) -> u8 {
        match self.state {
            BoardState::Physical(key) => key.remaining,
            BoardState::Boundary { remaining } => remaining,
        }
    }

    fn accepting(&self) -> bool {
        matches!(
            self.state,
            BoardState::Physical(BoardKey {
                board: 0,
                remaining: 0
            })
        )
    }
}

#[derive(Clone, Copy)]
struct Frame {
    initial: u64,
    placements: [u64; MAX_BOARD64_PIECES],
}

/// A certificate for a complete physical predecessor cone, local to exactly
/// this family. The key uses original cells, before any row deletion. Each
/// present kind occurs once, so neither operation indices nor future tiling
/// choices are part of the key. The current candidate's H suffix is separate.
#[derive(Clone, Copy)]
struct PrefixConeEntry {
    cells: [u64; 7],
    node: ProducerNode,
}

impl PrefixConeEntry {
    const EMPTY: Self = Self {
        cells: [0; 7],
        node: ProducerNode::MISSING,
    };
}

/// Optional exact certificates, separate from semantic graph nodes. Dense
/// chunks never move or initialize spare key payload. Bucket growth explicitly
/// initializes u32 indices and reindexes each retained certificate, so neither
/// allocator-dependent rehash work nor a full-table initialization is hidden.
#[derive(Clone, Copy)]
struct ExactConeEntry {
    cells: [u64; 7],
    node: ProducerNode,
    next: u32,
}

struct ExactConeCache {
    chunks: Vec<Vec<ExactConeEntry>>,
    buckets: Vec<u32>,
    len: usize,
    capacity: usize,
    limit: usize,
    allocation_refused: bool,
}

impl Default for ExactConeCache {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            buckets: Vec::new(),
            len: 0,
            capacity: 0,
            limit: EXACT_CONE_LIMIT,
            allocation_refused: false,
        }
    }
}

impl ExactConeCache {
    fn entry(&self, index: usize) -> &ExactConeEntry {
        &self.chunks[index / EXACT_CONE_CHUNK][index % EXACT_CONE_CHUNK]
    }

    fn entry_mut(&mut self, index: usize) -> &mut ExactConeEntry {
        &mut self.chunks[index / EXACT_CONE_CHUNK][index % EXACT_CONE_CHUNK]
    }

    fn can_publish(&self) -> bool {
        !self.allocation_refused && self.len < self.limit
    }

    fn retained_bytes(&self) -> usize {
        self.chunks.capacity() * core::mem::size_of::<Vec<ExactConeEntry>>()
            + self.capacity * core::mem::size_of::<ExactConeEntry>()
            + self.buckets.capacity() * core::mem::size_of::<u32>()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::backend::wasm_cpu) struct BoardActionMetrics {
    pub candidate_count: usize,
    pub nodes: usize,
    pub edges: usize,
    pub suffix_roots: usize,
    pub prefix_visits: usize,
    pub edge_attempts: usize,
    pub work: usize,
    pub source_pruned_prefixes: usize,
    pub dead_source_prefixes: usize,
}

pub(in crate::backend::wasm_cpu) struct BoardActionFamily {
    plan: Arc<BoardObservationPlan>,
    nodes: Vec<BoardNode>,
    index: HashMap<BoardKey, u32>,
    root: Option<u32>,
    prefixes: Vec<ProducerNode>,
    frames: Vec<Option<Frame>>,
    merge_nodes: Vec<u32>,
    edge_capacity_bytes: usize,
    suffix_capacity_bytes: usize,
    peak_bytes: usize,
    merge_peak_bytes: usize,
    metrics: BoardActionMetrics,
    frozen: bool,
    producer_closed: bool,
    boundary_class_metrics: [usize; 7],
    ordered_quotient: Option<OrderedPolicyQuotient>,
    ordered_class_metrics: [usize; 8],
    failed: bool,
    kind_arena: Option<Arc<()>>,
    closure_locks: [Vec<ClosureLock>; 7],
    closure_targets: Vec<ClosureTarget>,
    closure_pose_capacity_bytes: usize,
    closure_locks_ready: bool,
    edge_closure_complete: bool,
    closure_metrics: [usize; 5],
    #[cfg(any(test, feature = "local-search-ab"))]
    prefix_masks: Vec<PrefixMaskCache>,
    prefix_mask_metrics: [usize; 6],
    // Direct mapping bounds both memory and replacement cost. Hash collisions
    // only evict an optional certificate; full key equality authorizes hits.
    prefix_cones: Vec<PrefixConeEntry>,
    prefix_cone_metrics: [usize; 5],
    exact_cones: ExactConeCache,
    // Final producer snapshots (entries/capacity), ancestor lookups/hits,
    // full fallbacks, grows, reindexed entries, initialized buckets, extra work.
    prefix_cone_exact_metrics: [usize; 9],
    cone_pending: Vec<u16>,
    cone_misses: Vec<PrefixConeEntry>,
    cone_exact_misses: Vec<PrefixConeEntry>,
    cone_deleted_rows: Vec<u16>,
    pending_boundaries: Option<PendingBoundaries>,
}

impl BoardActionFamily {
    pub(in crate::backend::wasm_cpu) fn new(plan: Arc<BoardObservationPlan>) -> Self {
        Self {
            plan,
            nodes: Vec::new(),
            index: HashMap::new(),
            root: None,
            prefixes: Vec::new(),
            frames: Vec::new(),
            merge_nodes: Vec::new(),
            edge_capacity_bytes: 0,
            suffix_capacity_bytes: 0,
            peak_bytes: 0,
            merge_peak_bytes: 0,
            metrics: BoardActionMetrics::default(),
            frozen: false,
            producer_closed: false,
            boundary_class_metrics: [0; 7],
            ordered_quotient: None,
            ordered_class_metrics: [0; 8],
            failed: false,
            kind_arena: None,
            closure_locks: std::array::from_fn(|_| Vec::new()),
            closure_targets: Vec::new(),
            closure_pose_capacity_bytes: 0,
            closure_locks_ready: false,
            edge_closure_complete: false,
            closure_metrics: [0; 5],
            #[cfg(any(test, feature = "local-search-ab"))]
            prefix_masks: Vec::new(),
            prefix_mask_metrics: [0; 6],
            prefix_cones: Vec::new(),
            prefix_cone_metrics: [0; 5],
            exact_cones: ExactConeCache::default(),
            prefix_cone_exact_metrics: [0; 9],
            cone_pending: Vec::new(),
            cone_misses: Vec::new(),
            cone_exact_misses: Vec::new(),
            cone_deleted_rows: Vec::new(),
            pending_boundaries: None,
        }
    }

    pub(in crate::backend::wasm_cpu) fn has_roots(&self) -> bool {
        self.root.is_some()
    }

    pub(in crate::backend::wasm_cpu) fn plan(&self) -> &BoardObservationPlan {
        &self.plan
    }

    pub(in crate::backend::wasm_cpu) fn requires_edge_closure(&self) -> bool {
        self.plan.edge_closure != EdgeClosureMode::Off
    }

    pub(in crate::backend::wasm_cpu) fn metrics(&self) -> BoardActionMetrics {
        self.metrics
    }

    pub(in crate::backend::wasm_cpu) fn prefix_cone_metrics(&self) -> [usize; 5] {
        self.prefix_cone_metrics
    }

    pub(in crate::backend::wasm_cpu) fn prefix_cone_exact_metrics(&self) -> [usize; 9] {
        let mut metrics = self.prefix_cone_exact_metrics;
        metrics[0] += self.exact_cones.len;
        metrics[1] += self.exact_cones.capacity;
        metrics
    }

    /// O(1), including reusable registration and merge scratch. Shared plan
    /// payload and the parent-owned immutable kind arena are counted elsewhere.
    pub(in crate::backend::wasm_cpu) fn retained_bytes(&self) -> usize {
        let base = self.nodes.capacity() * core::mem::size_of::<BoardNode>()
            + self.index.capacity() * (core::mem::size_of::<(BoardKey, u32)>() + 1)
            + self.prefixes.capacity() * core::mem::size_of::<ProducerNode>()
            + self.frames.capacity() * core::mem::size_of::<Option<Frame>>()
            + self.merge_nodes.capacity() * core::mem::size_of::<u32>()
            + self.edge_capacity_bytes
            + self.suffix_capacity_bytes
            + self.closure_pose_capacity_bytes
            + self.closure_targets.capacity() * core::mem::size_of::<ClosureTarget>()
            + self.prefix_cones.capacity() * core::mem::size_of::<PrefixConeEntry>()
            + self.exact_cones.retained_bytes()
            + self.cone_pending.capacity() * core::mem::size_of::<u16>()
            + self.cone_misses.capacity() * core::mem::size_of::<PrefixConeEntry>()
            + self.cone_exact_misses.capacity() * core::mem::size_of::<PrefixConeEntry>()
            + self.cone_deleted_rows.capacity() * core::mem::size_of::<u16>()
            + self
                .pending_boundaries
                .as_ref()
                .map_or(0, PendingBoundaries::retained_bytes)
            + self
                .ordered_quotient
                .as_ref()
                .map_or(0, OrderedPolicyQuotient::retained_bytes);
        #[cfg(any(test, feature = "local-search-ab"))]
        let base = base
            + self.prefix_masks.capacity() * core::mem::size_of::<PrefixMaskCache>()
            + self
                .prefix_masks
                .first()
                .map_or(0, PrefixMaskCache::retained_bytes);
        base
    }

    /// Allocation peak upper estimate also covers old/new buffer coexistence
    /// during growth, set promotion, freeze, and incoming-family merge.
    pub(in crate::backend::wasm_cpu) fn peak_bytes(&self) -> usize {
        self.peak_bytes
    }

    fn observe_peak(&mut self, extra: usize) {
        self.peak_bytes = self.peak_bytes.max(
            self.retained_bytes()
                .saturating_add(extra)
                .saturating_add(self.merge_peak_bytes),
        );
    }

    pub(super) fn mutable(&self, control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if self.failed {
            return Err(invalid("wasm_observation_board_family_failed"));
        }
        if self.frozen {
            return Err(invalid("wasm_observation_board_family_frozen"));
        }
        Ok(())
    }

    fn producing(&self, control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
        self.mutable(control)?;
        if self.producer_closed {
            return Err(invalid("wasm_observation_boundary_producer_closed"));
        }
        Ok(())
    }

    pub(in crate::backend::wasm_cpu) fn boundary_class_metrics(&self) -> [usize; 7] {
        self.boundary_class_metrics
    }

    pub(in crate::backend::wasm_cpu) fn ordered_class_metrics(&self) -> [usize; 8] {
        self.ordered_class_metrics
    }

    pub(in crate::backend::wasm_cpu) fn fail_completed_boundary_batch(&mut self) {
        self.failed = true;
    }

    pub(in crate::backend::wasm_cpu) fn observe_boundary_owner_bytes(&mut self, bytes: usize) {
        self.boundary_class_metrics[5] = self.boundary_class_metrics[5].max(bytes);
        self.observe_peak(bytes);
    }

    pub(in crate::backend::wasm_cpu) fn register(
        &mut self,
        orders: SymbolicPrefixOrders,
        suffixes: &[u32],
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let mut root = |prefix: usize, _: u8, _: &ExecutionControl| Ok(suffixes[prefix]);
        let result = self.register_inner(&orders, None, suffixes.len(), &mut root, control);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    /// The source view binds the narrowed active mask to this candidate's
    /// original physical proof and geometry. Only this typed entry point
    /// may reuse its stronger first-six certificate instead of q5/q11 caps.
    pub(super) fn register_source_qualified(
        &mut self,
        orders: &SourceQualifiedPrefixOrders<'_>,
        suffixes: &[u32],
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let mut root = |prefix: usize, _: u8, _: &ExecutionControl| Ok(suffixes[prefix]);
        self.register_source_qualified_with_suffixes(orders, suffixes.len(), &mut root, control)
    }

    fn register_source_qualified_with_suffixes<R>(
        &mut self,
        orders: &SourceQualifiedPrefixOrders<'_>,
        prefix_count: usize,
        root: &mut R,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError>
    where
        R: FnMut(usize, u8, &ExecutionControl) -> Result<u32, WasmExactSearchError>,
    {
        let result =
            self.register_inner(orders.original(), Some(orders), prefix_count, root, control);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn register_inner<R>(
        &mut self,
        orders: &SymbolicPrefixOrders,
        source: Option<&SourceQualifiedPrefixOrders<'_>>,
        suffix_count: usize,
        suffix_root: &mut R,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError>
    where
        R: FnMut(usize, u8, &ExecutionControl) -> Result<u32, WasmExactSearchError>,
    {
        self.producing(control)?;
        if self.pending_boundaries.is_some() && source.is_none() {
            return Err(invalid("wasm_observation_shared_source_authority_missing"));
        }
        self.charge(1, control)?;
        let proof = orders.observation_relation()?;
        let geometry = orders.geometry();
        if source.is_some_and(|source| {
            !source.matches_original(orders)
                || !self.plan.fresh_bag_observation()
                || !self.plan.source_filter()
                || !self.plan.boundary_suffixes_only()
                || self.plan.hidden_horizon > 5
                || self
                    .plan
                    .prefix_kind_limits
                    .is_some_and(|limits| limits.contains(&0))
        }) {
            return Err(invalid("wasm_observation_source_filter_authority_missing"));
        }
        if proof.operation_count() != usize::from(self.plan.total_depth)
            || geometry.symbolic_board_scope()
                != (self.plan.width, self.plan.height, self.plan.initial_board)
            || geometry.pieces() != proof.pieces()
        {
            return Err(invalid("wasm_observation_board_candidate_scope_changed"));
        }
        let active_words =
            source.map_or_else(|| proof.active_words(), |source| source.active_words());
        if active_words[0] & 1 == 0 {
            return Ok(());
        }
        let prefix_count = usize::from(proof.terminal_prefix()) + 1;
        if suffix_count != prefix_count {
            return Err(invalid("wasm_observation_suffix_language_missing"));
        }
        self.prepare_scratch(prefix_count)?;
        let full = proof.terminal_prefix();
        let deleted = geometry.symbolic_deleted_rows(full);
        self.prepare_frame(geometry, deleted)?;
        if frame_board(self.frames[usize::from(deleted)].as_ref().unwrap(), full) != 0 {
            return Err(invalid("wasm_observation_board_terminal_not_empty"));
        }

        if source.is_some()
            && self.plan.prefix_cones_enabled
            && (1..=5).contains(&self.plan.hidden_horizon)
        {
            self.register_prefix_cones(proof, geometry, active_words, suffix_root, control)?;
            self.finish_candidate_registration();
            return Ok(());
        }

        #[cfg(any(test, feature = "local-search-ab"))]
        let mask_plan = if source.is_some() {
            None
        } else {
            self.prefix_mask_plan(proof.pieces(), control)?
        };
        #[cfg(any(test, feature = "local-search-ab"))]
        let use_word_masks = mask_plan.is_some();
        #[cfg(not(any(test, feature = "local-search-ab")))]
        let use_word_masks = false;

        let mut limited_groups = [(0_u16, 0_u8); 7];
        let mut limited_group_count = 0;
        if let Some(limits) = self
            .plan
            .prefix_kind_limits
            .filter(|_| source.is_none() && !use_word_masks)
        {
            let mut groups = [0_u16; 7];
            for (operation, &piece) in proof.pieces().iter().enumerate() {
                groups[usize::from(piece_code(piece) - 1)] |= 1_u16 << operation;
            }
            for (operations, limit) in groups.into_iter().zip(limits) {
                // A group already below its bound can never invalidate any
                // subset, so it adds no check to the prefix loop.
                if operations.count_ones() > u32::from(limit) {
                    limited_groups[limited_group_count] = (operations, limit);
                    limited_group_count += 1;
                }
            }
        }
        let limited_groups = &limited_groups[..limited_group_count];

        // Supersets have greater numeric masks, so every retained child is
        // available before its parent. Keep precisely the cap-valid prefixes
        // that reach H. The producer's active relation is root-reachable and
        // terminal-productive, and a kind-count upper bound is downward-closed:
        // any retained H-prefix has a fully cap-valid original root path.
        // Consequently this single sweep cannot leave an orphan contribution
        // when its root is absent, even if the cap removes every H-prefix.
        // A source-qualified view is already reachable/productive through a
        // distinct six-kind boundary. Since H<=5, it entails the original
        // fresh-bag cap at every visited prefix: neither scalar nor cached
        // cap calculation is repeated, and no unexecuted helper work is charged.
        for (word, &active) in active_words.iter().enumerate().rev() {
            let mut active = active;
            #[cfg(any(test, feature = "local-search-ab"))]
            if let Some(plan) = mask_plan.filter(|plan| plan.needs_filter(word, active)) {
                self.charge(RUNTIME_WORD_WORK, control)?;
                self.prefix_mask_metrics[4] += 1;
                self.prefix_mask_metrics[5] += RUNTIME_WORD_WORK;
                let (retained, rejected) = self.prefix_masks[0].word(plan, word)?.filter(active);
                active = retained;
                self.metrics.source_pruned_prefixes += rejected;
            }
            while active != 0 {
                let lane = 63 - active.leading_zeros() as usize;
                active &= !(1_u64 << lane);
                let prefix = (word * 64 + lane) as u16;
                let depth = prefix.count_ones() as u8;
                if depth > self.plan.hidden_horizon {
                    continue;
                }
                self.charge(1, control)?;
                self.metrics.prefix_visits += 1;
                if limited_groups.iter().any(|&(operations, limit)| {
                    (prefix & operations).count_ones() > u32::from(limit)
                }) {
                    self.metrics.source_pruned_prefixes += 1;
                    continue;
                }
                let mut operations = proof.live_operations_in(active_words, prefix);
                let first_child = if depth < self.plan.hidden_horizon {
                    let first = operations.find(|&(operation, _)| {
                        let child = prefix | (1_u16 << operation);
                        self.prefixes[usize::from(child)] != ProducerNode::MISSING
                    });
                    if first.is_none() {
                        self.metrics.dead_source_prefixes += 1;
                        continue;
                    }
                    first
                } else {
                    None
                };
                let (node, deleted) = self.register_prefix_node(geometry, prefix)?;
                // Before H the retained physical graph already supplies every
                // source-compatible continuation. Even after an earlier reveal
                // it can be consumed with known information until H, so only
                // the boundary needs dormant suffix bindings. The legacy AB
                // mode retains L..H. Oracle kind roots stay in the shared cache.
                if self.plan.requires_suffix(depth) {
                    let root =
                        suffix_root(usize::from(prefix), self.plan.total_depth - depth, control)?;
                    self.insert_producer_suffix(node, root)?;
                }
                if depth < self.plan.hidden_horizon && !self.plan.should_rebuild(depth) {
                    // Resume the same derivative iterator after its first live
                    // child. No second operation mask or edge scan is needed.
                    // Hybrid closure preserves the already-proved last layer:
                    // its many distinct boards need no repeated movement work.
                    for (operation, piece) in first_child.into_iter().chain(operations) {
                        let child = self.prefixes[usize::from(prefix | (1_u16 << operation))];
                        if child == ProducerNode::MISSING {
                            continue;
                        }
                        self.charge(1, control)?;
                        self.metrics.edge_attempts += 1;
                        let cells = self.frames[usize::from(deleted)]
                            .as_ref()
                            .unwrap()
                            .placements[operation];
                        self.insert_producer_edge(
                            node,
                            ProducerEdge {
                                piece: piece_code(piece),
                                cells,
                                child,
                            },
                        )?;
                    }
                }
            }
        }
        self.finish_candidate_registration();
        Ok(())
    }

    fn finish_candidate_registration(&mut self) {
        self.metrics.candidate_count += 1;
        #[cfg(feature = "local-search-ab")]
        if self.metrics.candidate_count == 1 || self.metrics.candidate_count & 0xffff == 0 {
            eprintln!(
                "clearra_observation_board_producer_progress metrics={:?} prefix_cones={:?}",
                self.metrics, self.prefix_cone_metrics
            );
        }
        self.observe_peak(0);
    }

    /// Shared by scalar registration and cone misses. The caller meters the
    /// actual prefix visit; all paths use the same frame/board/root checks.
    fn register_prefix_node(
        &mut self,
        geometry: &CandidateGeometry,
        prefix: u16,
    ) -> Result<(ProducerNode, u16), WasmExactSearchError> {
        let deleted = geometry.symbolic_deleted_rows(prefix);
        self.prepare_frame(geometry, deleted)?;
        let board = frame_board(self.frames[usize::from(deleted)].as_ref().unwrap(), prefix);
        let node = self.intern_producer_node(BoardKey {
            board,
            remaining: self.plan.total_depth - prefix.count_ones() as u8,
        })?;
        self.prefixes[usize::from(prefix)] = node;
        if prefix == 0 {
            let root_node = node
                .physical_node()
                .ok_or_else(|| invalid("wasm_observation_board_root_changed"))?;
            if board != self.plan.initial_board || self.root.is_some_and(|root| root != root_node) {
                return Err(invalid("wasm_observation_board_root_changed"));
            }
            self.root = Some(root_node);
        }
        Ok((node, deleted))
    }

    fn prepare_prefix_cones(
        &mut self,
        prefix_count: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if !self.plan.prefix_cones_exact || self.plan.prefix_cones_ancestors_only {
            self.prepare_direct_cones(control)?;
        }
        self.cone_exact_misses.clear();
        self.cone_pending.clear();
        let old = self.cone_pending.capacity() * core::mem::size_of::<u16>();
        self.cone_pending
            .try_reserve_exact(prefix_count)
            .map_err(|_| storage())?;
        self.observe_peak(
            if self.cone_pending.capacity() * core::mem::size_of::<u16>() != old {
                old
            } else {
                0
            },
        );
        self.cone_misses.clear();
        self.cone_deleted_rows.clear();
        let old = self.cone_deleted_rows.capacity() * core::mem::size_of::<u16>();
        self.cone_deleted_rows
            .try_reserve_exact(prefix_count)
            .map_err(|_| storage())?;
        self.cone_deleted_rows.resize(prefix_count, 0);
        self.observe_peak(
            if self.cone_deleted_rows.capacity() * core::mem::size_of::<u16>() != old {
                old
            } else {
                0
            },
        );
        Ok(())
    }

    fn prepare_direct_cones(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.prefix_cones.is_empty() {
            // This is actual one-time initialization, not free cache space.
            // The same cumulative physical budget covers it and publication.
            self.charge(PREFIX_CONE_SLOTS, control)?;
            self.prefix_cones
                .try_reserve_exact(PREFIX_CONE_SLOTS)
                .map_err(|_| storage())?;
            self.prefix_cones
                .resize(PREFIX_CONE_SLOTS, PrefixConeEntry::EMPTY);
            self.observe_peak(0);
        }
        Ok(())
    }

    fn cone_cache_charge(
        &mut self,
        steps: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.charge(steps, control)?;
        self.prefix_cone_exact_metrics[8] += steps;
        Ok(())
    }

    /// The caller's prefix/publication visit pays for the first bucket probe.
    /// Every further collision comparison is an additional actual operation.
    fn exact_cone_lookup(
        &mut self,
        cells: &[u64; 7],
        control: &ExecutionControl,
    ) -> Result<Option<ProducerNode>, WasmExactSearchError> {
        if self.exact_cones.buckets.is_empty() {
            return Ok(None);
        }
        let slot = prefix_cone_hash(cells) as usize & (self.exact_cones.buckets.len() - 1);
        let mut index = self.exact_cones.buckets[slot];
        let mut first = true;
        while index != NO_NODE {
            if !first {
                self.cone_cache_charge(1, control)?;
            }
            first = false;
            if index as usize >= self.exact_cones.len {
                return Err(invalid("wasm_observation_prefix_cone_invalid"));
            }
            let entry = *self.exact_cones.entry(index as usize);
            if entry.cells == *cells {
                return Ok(Some(entry.node));
            }
            index = entry.next;
        }
        Ok(None)
    }

    fn prepare_exact_cone_capacity(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        if !self.exact_cones.can_publish() {
            return Ok(false);
        }
        if self.exact_cones.len == self.exact_cones.capacity {
            if self.exact_cones.chunks.is_empty() {
                let chunk_count = self.exact_cones.limit.div_ceil(EXACT_CONE_CHUNK);
                if self
                    .exact_cones
                    .chunks
                    .try_reserve_exact(chunk_count)
                    .is_err()
                {
                    self.exact_cones.allocation_refused = true;
                    return Ok(false);
                }
                self.observe_peak(0);
            }
            let requested = (self.exact_cones.limit - self.exact_cones.len).min(EXACT_CONE_CHUNK);
            let mut chunk = Vec::new();
            if chunk.try_reserve_exact(requested).is_err() {
                self.exact_cones.allocation_refused = true;
                return Ok(false);
            }
            let bytes = chunk.capacity() * core::mem::size_of::<ExactConeEntry>();
            self.observe_peak(bytes);
            if chunk.capacity() > requested {
                // An allocator may over-reserve. Keep the stated optional
                // entry-capacity bound and fall back without touching old IDs.
                self.exact_cones.allocation_refused = true;
                return Ok(false);
            }
            self.exact_cones.capacity += chunk.capacity();
            self.exact_cones.chunks.push(chunk);
            self.prefix_cone_exact_metrics[5] += 1;
            self.observe_peak(0);
        }
        let old_slots = self.exact_cones.buckets.len();
        if old_slots == 0
            || (self.exact_cones.len >= old_slots && old_slots < EXACT_CONE_MAX_BUCKETS)
        {
            let slots = if old_slots == 0 {
                EXACT_CONE_FIRST_BUCKETS
            } else {
                (old_slots * 8).min(EXACT_CONE_MAX_BUCKETS)
            };
            let mut buckets = Vec::new();
            if buckets.try_reserve_exact(slots).is_err() {
                self.exact_cones.allocation_refused = true;
                return Ok(false);
            }
            self.observe_peak(buckets.capacity() * core::mem::size_of::<u32>());
            if buckets.capacity() > EXACT_CONE_MAX_BUCKETS {
                self.exact_cones.allocation_refused = true;
                return Ok(false);
            }
            let entries = self.exact_cones.len;
            // All fallible allocation and the whole bounded rebuild charge
            // precede the first link mutation. The following loops cannot
            // fail or publish a partial index. A cancellation observed after
            // them still poisons the enclosing candidate registration.
            self.cone_cache_charge(slots + entries, control)?;
            buckets.resize(slots, NO_NODE);
            for index in 0..entries {
                let entry = self.exact_cones.entry_mut(index);
                let slot = prefix_cone_hash(&entry.cells) as usize & (slots - 1);
                entry.next = buckets[slot];
                buckets[slot] = index as u32;
            }
            self.exact_cones.buckets = buckets;
            self.prefix_cone_exact_metrics[5] += 1;
            self.prefix_cone_exact_metrics[6] += entries;
            self.prefix_cone_exact_metrics[7] += slots;
            self.observe_peak(0);
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
        }
        Ok(true)
    }

    fn publish_exact_cone(
        &mut self,
        entry: PrefixConeEntry,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        self.cone_cache_charge(1, control)?;
        if let Some(node) = self.exact_cone_lookup(&entry.cells, control)? {
            if node != entry.node {
                return Err(invalid("wasm_observation_prefix_cone_invalid"));
            }
            return Ok(true);
        }
        if !self.prepare_exact_cone_capacity(control)? {
            self.prefix_cone_exact_metrics[4] += 1;
            return Ok(false);
        }
        let slot = prefix_cone_hash(&entry.cells) as usize & (self.exact_cones.buckets.len() - 1);
        let index = self.exact_cones.len;
        let next = self.exact_cones.buckets[slot];
        self.exact_cones.chunks[index / EXACT_CONE_CHUNK].push(ExactConeEntry {
            cells: entry.cells,
            node: entry.node,
            next,
        });
        self.exact_cones.buckets[slot] = index as u32;
        self.exact_cones.len += 1;
        Ok(true)
    }

    fn remember_exact_cone_miss(
        &mut self,
        entry: PrefixConeEntry,
    ) -> Result<(), WasmExactSearchError> {
        let old = self.cone_exact_misses.capacity() * core::mem::size_of::<PrefixConeEntry>();
        self.cone_exact_misses
            .try_reserve(1)
            .map_err(|_| storage())?;
        self.cone_exact_misses.push(entry);
        self.observe_peak(
            if self.cone_exact_misses.capacity() * core::mem::size_of::<PrefixConeEntry>() != old {
                old
            } else {
                0
            },
        );
        Ok(())
    }

    fn remember_cone_miss(&mut self, entry: PrefixConeEntry) -> Result<(), WasmExactSearchError> {
        let old = self.cone_misses.capacity() * core::mem::size_of::<PrefixConeEntry>();
        self.cone_misses.try_reserve(1).map_err(|_| storage())?;
        self.cone_misses.push(entry);
        self.observe_peak(
            if self.cone_misses.capacity() * core::mem::size_of::<PrefixConeEntry>() != old {
                old
            } else {
                0
            },
        );
        Ok(())
    }

    fn install_cone_hit(
        &mut self,
        node: ProducerNode,
        prefix: u16,
        geometry: &CandidateGeometry,
        needs_parent_frame: bool,
    ) -> Result<(), WasmExactSearchError> {
        let existing = self
            .producer_key(node)
            .ok_or_else(|| invalid("wasm_observation_prefix_cone_invalid"))?;
        if existing.remaining != self.plan.total_depth - prefix.count_ones() as u8 {
            return Err(invalid("wasm_observation_prefix_cone_invalid"));
        }
        self.prefixes[usize::from(prefix)] = node;
        if needs_parent_frame {
            // Equal original placements fix the deleted rows, but the frame
            // contains every placement of the CURRENT candidate. Never reuse
            // a former candidate's tail placements through a cone certificate.
            let deleted = geometry.symbolic_deleted_rows(prefix);
            self.cone_deleted_rows[usize::from(prefix)] = deleted;
            self.prepare_frame(geometry, deleted)?;
        }
        Ok(())
    }

    fn publish_direct_cone(
        &mut self,
        entry: PrefixConeEntry,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.prepare_direct_cones(control)?;
        self.charge(1, control)?;
        let slot = prefix_cone_slot(&entry.cells);
        let old = self.prefix_cones[slot];
        if old.node != ProducerNode::MISSING && old.cells != entry.cells {
            self.prefix_cone_metrics[3] += 1;
        }
        self.prefix_cones[slot] = entry;
        Ok(())
    }

    /// For H<=5 distinct kinds, a source-qualified H-prefix certifies a
    /// continuation through the distinct six-kind boundary. Every legal
    /// ordering of these same original placements has the same kind set and
    /// can use that continuation. Its entire predecessor cone is therefore
    /// independent of the rest of this candidate's tiling. Catalog rows join
    /// every realization of one (kind, original cells), so rotation choices
    /// are already existential in the exact physical proof.
    ///
    /// A hit reuses only this physical cone. It never reuses, replaces, or
    /// suppresses the current candidate's distinct suffix language at H.
    fn register_prefix_cones<R>(
        &mut self,
        proof: &ExactPrefixRelation,
        geometry: &CandidateGeometry,
        active_words: &[u64],
        suffix_root: &mut R,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError>
    where
        R: FnMut(usize, u8, &ExecutionControl) -> Result<u32, WasmExactSearchError>,
    {
        self.prepare_prefix_cones(self.prefixes.len(), control)?;
        self.prepare_frame(geometry, 0)?;
        let original = *self.frames[0].as_ref().unwrap();
        if original.initial != self.plan.initial_board {
            return Err(invalid("wasm_observation_board_root_changed"));
        }
        let horizon = usize::from(self.plan.hidden_horizon);
        // Select H directly at word granularity: cache hits do not traverse
        // or charge the ancestor prefixes which their certificate replaces.
        // This is the existing prefix-visit accounting, not a reset or refund.
        for (word, &active) in active_words.iter().enumerate().rev() {
            let mut frontier = active & CONE_FRONTIER_WORDS[horizon][word];
            while frontier != 0 {
                let lane = 63 - frontier.leading_zeros() as usize;
                frontier &= !(1_u64 << lane);
                let prefix = (word * 64 + lane) as u16;
                self.charge(1, control)?;
                self.metrics.prefix_visits += 1;
                self.prefix_cone_metrics[0] += 1;
                let cells = prefix_cone_key(proof.pieces(), &original, prefix)?;
                let exact_h =
                    self.plan.prefix_cones_exact && !self.plan.prefix_cones_ancestors_only;
                let mut cached = if exact_h {
                    self.exact_cone_lookup(&cells, control)?
                } else {
                    None
                };
                if cached.is_none() && !self.prefix_cones.is_empty() {
                    if exact_h {
                        self.cone_cache_charge(1, control)?;
                    }
                    let entry = self.prefix_cones[prefix_cone_slot(&cells)];
                    if entry.node != ProducerNode::MISSING && entry.cells == cells {
                        cached = Some(entry.node);
                    }
                }
                let node = if let Some(node) = cached {
                    self.install_cone_hit(node, prefix, geometry, false)?;
                    self.prefix_cone_metrics[1] += 1;
                    node
                } else {
                    self.prefix_cone_metrics[2] += 1;
                    let (node, deleted) = self.register_prefix_node(geometry, prefix)?;
                    self.cone_deleted_rows[usize::from(prefix)] = deleted;
                    self.cone_pending.push(prefix);
                    if exact_h {
                        self.remember_exact_cone_miss(PrefixConeEntry { cells, node })?;
                    } else {
                        self.remember_cone_miss(PrefixConeEntry { cells, node })?;
                    }
                    node
                };
                let root = suffix_root(
                    usize::from(prefix),
                    self.plan.total_depth - self.plan.hidden_horizon,
                    control,
                )?;
                self.insert_producer_suffix(node, root)?;
            }
        }

        // Expand each child once, but always insert ALL of its exact incoming
        // edges. A shared predecessor may already have been expanded for a
        // different frontier; that never suppresses its new outgoing edge to
        // this child. Thus overlaps between several misses are reused without
        // losing a branch, and every published cone includes every root path.
        while let Some(child_prefix) = self.cone_pending.pop() {
            if child_prefix == 0 {
                continue;
            }
            let child = self.prefixes[usize::from(child_prefix)];
            let mut operations = child_prefix;
            let mut has_predecessor = false;
            while operations != 0 {
                let operation = operations.trailing_zeros() as usize;
                operations &= operations - 1;
                let prefix = child_prefix & !(1_u16 << operation);
                let word = usize::from(prefix) / 64;
                let lane = 1_u64 << (usize::from(prefix) % 64);
                if active_words[word] & proof.parent_words(operation)[word] & lane == 0 {
                    continue;
                }
                has_predecessor = true;
                let parent = if self.prefixes[usize::from(prefix)] == ProducerNode::MISSING {
                    self.charge(1, control)?;
                    self.metrics.prefix_visits += 1;
                    let cells = if self.plan.prefix_cones_exact && prefix != 0 {
                        Some(prefix_cone_key(proof.pieces(), &original, prefix)?)
                    } else {
                        None
                    };
                    let cached = if let Some(cells) = cells.as_ref() {
                        self.prefix_cone_exact_metrics[2] += 1;
                        self.exact_cone_lookup(cells, control)?
                    } else {
                        None
                    };
                    if let Some(node) = cached {
                        self.prefix_cone_exact_metrics[3] += 1;
                        self.install_cone_hit(
                            node,
                            prefix,
                            geometry,
                            !self.plan.should_rebuild(prefix.count_ones() as u8),
                        )?;
                        node
                    } else {
                        let (node, deleted) = self.register_prefix_node(geometry, prefix)?;
                        self.cone_deleted_rows[usize::from(prefix)] = deleted;
                        self.cone_pending.push(prefix);
                        if let Some(cells) = cells {
                            self.remember_exact_cone_miss(PrefixConeEntry { cells, node })?;
                        }
                        node
                    }
                } else {
                    self.prefixes[usize::from(prefix)]
                };
                if !self.plan.should_rebuild(prefix.count_ones() as u8) {
                    self.charge(1, control)?;
                    self.metrics.edge_attempts += 1;
                    let deleted = self.cone_deleted_rows[usize::from(prefix)];
                    let cells = self.frames[usize::from(deleted)]
                        .as_ref()
                        .unwrap()
                        .placements[operation];
                    self.insert_producer_edge(
                        parent,
                        ProducerEdge {
                            piece: piece_code(proof.pieces()[operation]),
                            cells,
                            child,
                        },
                    )?;
                }
            }
            if !has_predecessor {
                return Err(invalid("wasm_observation_prefix_cone_unreachable"));
            }
        }
        // Hit-only candidates still validate their root and the terminal
        // geometry/suffix scope checked by register_inner. Cache certificates
        // never grant a new request or an empty successful contribution.
        let root = self
            .root
            .and_then(|root| self.nodes.get(root as usize))
            .ok_or_else(|| invalid("wasm_observation_board_root_changed"))?;
        if root.physical_key()
            != Some(BoardKey {
                board: self.plan.initial_board,
                remaining: self.plan.total_depth,
            })
        {
            return Err(invalid("wasm_observation_board_root_changed"));
        }
        // No certificate is visible until every missed cone above completed.
        // A later error poisons the owner; it never publishes a partial cone.
        if self.plan.prefix_cones_exact {
            // The same independence lemma holds for each distinct-kind T
            // below H. A hit suppresses only T's ancestor expansion: its new
            // outgoing edge to the current child was still inserted above.
            // Publish ALL T certificates only after the whole pending cone
            // traversal completed, never merely when T was popped.
            for index in 0..self.cone_exact_misses.len() {
                let entry = self.cone_exact_misses[index];
                let at_horizon = entry.cells.iter().filter(|&&cells| cells != 0).count() == horizon;
                if !self.publish_exact_cone(entry, control)? && at_horizon {
                    self.publish_direct_cone(entry, control)?;
                }
                if at_horizon {
                    self.prefix_cone_metrics[4] += 1;
                }
            }
        }
        if !self.plan.prefix_cones_exact || self.plan.prefix_cones_ancestors_only {
            for index in 0..self.cone_misses.len() {
                self.publish_direct_cone(self.cone_misses[index], control)?;
                self.prefix_cone_metrics[4] += 1;
            }
        }
        Ok(())
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    fn prefix_mask_plan(
        &mut self,
        pieces: &[PieceKind],
        control: &ExecutionControl,
    ) -> Result<Option<PrefixMaskPlan>, WasmExactSearchError> {
        if !self.plan.prefix_masks_enabled || self.plan.total_depth > MAX_MASK_OPERATIONS {
            return Ok(None);
        }
        let Some(limits) = self.plan.prefix_kind_limits else {
            return Ok(None);
        };
        if self.prefix_masks.is_empty() {
            self.prefix_masks
                .try_reserve_exact(1)
                .map_err(|_| storage())?;
            self.prefix_masks.push(PrefixMaskCache::new(
                self.plan.total_depth,
                self.plan.hidden_horizon,
                limits,
            )?);
            self.observe_peak(0);
        }
        let cache = &mut self.prefix_masks[0];
        let before = cache.metrics();
        let result = cache.query(pieces, MAX_WORK.saturating_sub(self.metrics.work), control);
        let after = cache.metrics();
        let work = after.work - before.work;
        self.prefix_mask_metrics[0] += after.lookups - before.lookups;
        self.prefix_mask_metrics[1] += after.hits - before.hits;
        self.prefix_mask_metrics[2] += after.misses - before.misses;
        self.prefix_mask_metrics[3] += after.full_fallbacks - before.full_fallbacks;
        self.prefix_mask_metrics[5] += work;
        let extra_peak = cache.peak_bytes().saturating_sub(cache.retained_bytes());
        self.observe_peak(extra_peak);
        // The helper checks this same remaining budget before each operation.
        // Even on failure, retain all work already performed, then poison the
        // owning registration rather than turning the failure into no paths.
        self.metrics.work = self
            .metrics
            .work
            .checked_add(work)
            .filter(|&total| total <= MAX_WORK)
            .ok_or_else(|| invalid("wasm_observation_physical_work_limit"))?;
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let query = result?;
        debug_assert_eq!(query.work, work);
        debug_assert_eq!(usize::from(query.cache_hit), after.hits - before.hits);
        Ok(query.plan)
    }

    fn prepare_scratch(&mut self, prefixes: usize) -> Result<(), WasmExactSearchError> {
        self.prefixes.clear();
        let old = self.prefixes.capacity() * core::mem::size_of::<ProducerNode>();
        self.prefixes
            .try_reserve_exact(prefixes)
            .map_err(|_| storage())?;
        self.prefixes.resize(prefixes, ProducerNode::MISSING);
        self.observe_peak(
            if self.prefixes.capacity() * core::mem::size_of::<ProducerNode>() != old {
                old
            } else {
                0
            },
        );
        self.frames.clear();
        let old = self.frames.capacity() * core::mem::size_of::<Option<Frame>>();
        self.frames
            .try_reserve_exact(1_usize << self.plan.height)
            .map_err(|_| storage())?;
        self.frames.resize(1_usize << self.plan.height, None);
        self.observe_peak(
            if self.frames.capacity() * core::mem::size_of::<Option<Frame>>() != old {
                old
            } else {
                0
            },
        );
        Ok(())
    }

    fn prepare_frame(
        &mut self,
        geometry: &CandidateGeometry,
        deleted: u16,
    ) -> Result<(), WasmExactSearchError> {
        let frame = self
            .frames
            .get_mut(usize::from(deleted))
            .ok_or_else(|| invalid("wasm_observation_board_frame_invalid"))?;
        if frame.is_none() {
            let (initial, placements) = geometry.symbolic_frame(deleted);
            *frame = Some(Frame {
                initial,
                placements,
            });
        }
        Ok(())
    }

    fn producer_key(&self, node: ProducerNode) -> Option<BoardKey> {
        if let Some(node) = node.physical_node() {
            self.nodes.get(node as usize)?.physical_key()
        } else {
            self.pending_boundaries
                .as_ref()?
                .groups
                .get(node.boundary_group()? as usize)
                .map(|group| group.key)
        }
    }

    fn intern_producer_node(
        &mut self,
        key: BoardKey,
    ) -> Result<ProducerNode, WasmExactSearchError> {
        let depth = self
            .plan
            .total_depth
            .checked_sub(key.remaining)
            .ok_or_else(|| invalid("wasm_observation_board_state_invalid"))?;
        if self.pending_boundaries.is_none() || depth != self.plan.hidden_horizon {
            return self.intern_board(key).map(ProducerNode::physical);
        }
        if key.board & !self.plan.board_mask != 0 || key.remaining == 0 {
            return Err(invalid("wasm_observation_board_state_invalid"));
        }
        if let Some(&group) = self.pending_boundaries.as_ref().unwrap().index.get(&key) {
            return ProducerNode::pending(group);
        }
        // Every retained pending group has at least one root. Both the group
        // allocation and its roots remain subject to the unchanged 8M member
        // limit; naming a value staging never exempts retained H bindings.
        if self.pending_boundaries.as_ref().unwrap().groups.len() >= MAX_UNION_MEMBERS {
            return Err(invalid("wasm_observation_physical_union_limit"));
        }
        let old = self.pending_boundaries.as_ref().unwrap().groups.capacity()
            * core::mem::size_of::<PendingBoundaryGroup>();
        self.pending_boundaries
            .as_mut()
            .unwrap()
            .groups
            .try_reserve(1)
            .map_err(|_| storage())?;
        let new = self.pending_boundaries.as_ref().unwrap().groups.capacity()
            * core::mem::size_of::<PendingBoundaryGroup>();
        self.observe_peak(if new != old { old } else { 0 });
        let old = self.pending_boundaries.as_ref().unwrap().index.capacity()
            * (core::mem::size_of::<(BoardKey, u32)>() + 1);
        self.pending_boundaries
            .as_mut()
            .unwrap()
            .index
            .try_reserve(1)
            .map_err(|_| storage())?;
        let new = self.pending_boundaries.as_ref().unwrap().index.capacity()
            * (core::mem::size_of::<(BoardKey, u32)>() + 1);
        self.observe_peak(if new != old { old } else { 0 });
        let pending = self.pending_boundaries.as_mut().unwrap();
        let group = u32::try_from(pending.groups.len())
            .map_err(|_| invalid("wasm_observation_boundary_group_limit"))?;
        pending.groups.push(PendingBoundaryGroup {
            key,
            roots: SuffixRoots::default(),
        });
        pending.index.insert(key, group);
        ProducerNode::pending(group)
    }

    fn insert_producer_edge(
        &mut self,
        parent: ProducerNode,
        edge: ProducerEdge,
    ) -> Result<(), WasmExactSearchError> {
        let parent = parent
            .physical_node()
            .ok_or_else(|| invalid("wasm_observation_boundary_requires_proved_edge"))?;
        if let Some(child) = edge.child.physical_node() {
            return self.insert_edge(
                parent,
                BoardEdge {
                    piece: edge.piece,
                    cells: edge.cells,
                    child,
                },
            );
        }
        let group = edge
            .child
            .boundary_group()
            .ok_or_else(|| invalid("wasm_observation_physical_child_invalid"))?;
        let pending = self
            .pending_boundaries
            .as_ref()
            .ok_or_else(|| invalid("wasm_observation_shared_producer_scope_invalid"))?;
        let existing = pending.edges.get(&parent).map_or(Err(0), |edges| {
            edges.binary_search_by_key(&(edge.piece, edge.cells), |edge| (edge.piece, edge.cells))
        });
        if let Ok(index) = existing {
            if pending.edges[&parent][index].group != group {
                return Err(invalid("wasm_observation_board_action_not_deterministic"));
            }
            return Ok(());
        }
        let child = pending
            .groups
            .get(group as usize)
            .ok_or_else(|| invalid("wasm_observation_physical_child_invalid"))?
            .key;
        let parent_key = self
            .nodes
            .get(parent as usize)
            .and_then(BoardNode::physical_key)
            .ok_or_else(|| invalid("wasm_observation_boundary_requires_proved_edge"))?;
        self.validate_edge_keys(
            parent_key,
            BoardEdge {
                piece: edge.piece,
                cells: edge.cells,
                child: 0,
            },
            child,
        )?;
        if self.metrics.edges >= MAX_EDGES {
            return Err(invalid("wasm_observation_physical_edge_limit"));
        }
        let old = pending.edges.capacity()
            * (core::mem::size_of::<(u32, Vec<PendingBoundaryEdge>)>() + 1);
        self.pending_boundaries
            .as_mut()
            .unwrap()
            .edges
            .try_reserve(1)
            .map_err(|_| storage())?;
        let new = self.pending_boundaries.as_ref().unwrap().edges.capacity()
            * (core::mem::size_of::<(u32, Vec<PendingBoundaryEdge>)>() + 1);
        self.observe_peak(if new != old { old } else { 0 });
        let pending = self.pending_boundaries.as_mut().unwrap();
        let edges = pending.edges.entry(parent).or_default();
        let old = edges.capacity() * core::mem::size_of::<PendingBoundaryEdge>();
        edges.try_reserve(1).map_err(|_| storage())?;
        let new = edges.capacity() * core::mem::size_of::<PendingBoundaryEdge>();
        edges.insert(
            existing.unwrap_err(),
            PendingBoundaryEdge {
                piece: edge.piece,
                cells: edge.cells,
                group,
            },
        );
        pending.edge_capacity_bytes += new - old;
        self.metrics.edges += 1;
        self.observe_peak(if new != old { old } else { 0 });
        Ok(())
    }

    fn insert_producer_suffix(
        &mut self,
        node: ProducerNode,
        root: u32,
    ) -> Result<(), WasmExactSearchError> {
        if let Some(node) = node.physical_node() {
            return self.insert_suffix(node, root);
        }
        if root >= KIND_TAG - 1 {
            return Err(invalid("wasm_observation_suffix_language_missing"));
        }
        let group = node
            .boundary_group()
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        let pending = self
            .pending_boundaries
            .as_mut()
            .ok_or_else(|| invalid("wasm_observation_shared_producer_scope_invalid"))?;
        let roots = &mut pending
            .groups
            .get_mut(group as usize)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?
            .roots;
        let old = roots.retained_bytes();
        let inserted = roots.insert(root, self.metrics.suffix_roots < MAX_UNION_MEMBERS);
        let new = roots.retained_bytes();
        pending.root_capacity_bytes = pending.root_capacity_bytes - old + new;
        let (inserted, extra) = inserted?;
        self.metrics.suffix_roots += usize::from(inserted);
        self.observe_peak(extra);
        Ok(())
    }

    fn intern_board(&mut self, key: BoardKey) -> Result<u32, WasmExactSearchError> {
        if key.remaining > self.plan.total_depth
            || key.board & !self.plan.board_mask != 0
            || (key.remaining == 0 && key.board != 0)
        {
            return Err(invalid("wasm_observation_board_state_invalid"));
        }
        if let Some(&node) = self.index.get(&key) {
            return Ok(node);
        }
        if self.nodes.len() >= MAX_NODES {
            return Err(invalid("wasm_observation_physical_node_limit"));
        }
        let old = self.nodes.capacity() * core::mem::size_of::<BoardNode>();
        self.nodes.try_reserve(1).map_err(|_| storage())?;
        self.observe_peak(
            if self.nodes.capacity() * core::mem::size_of::<BoardNode>() != old {
                old
            } else {
                0
            },
        );
        let old = self.index.capacity() * (core::mem::size_of::<(BoardKey, u32)>() + 1);
        self.index.try_reserve(1).map_err(|_| storage())?;
        self.observe_peak(
            if self.index.capacity() * (core::mem::size_of::<(BoardKey, u32)>() + 1) != old {
                old
            } else {
                0
            },
        );
        let node = self.nodes.len() as u32;
        self.nodes.push(BoardNode {
            state: BoardState::Physical(key),
            edges: Vec::new(),
            suffixes: SuffixRoots::default(),
        });
        self.index.insert(key, node);
        self.metrics.nodes += 1;
        Ok(node)
    }

    fn validate_edge_keys(
        &self,
        parent: BoardKey,
        edge: BoardEdge,
        child: BoardKey,
    ) -> Result<(), WasmExactSearchError> {
        if edge.piece == 0
            || edge.piece > 7
            || edge.cells.count_ones() != 4
            || edge.cells & !self.plan.board_mask != 0
            || edge.cells & parent.board != 0
            || child.remaining.checked_add(1) != Some(parent.remaining)
            || place_and_clear(self.plan.width, self.plan.height, parent.board | edge.cells).0
                != child.board
        {
            return Err(invalid("wasm_observation_board_transition_invalid"));
        }
        Ok(())
    }

    fn insert_edge(&mut self, parent: u32, edge: BoardEdge) -> Result<(), WasmExactSearchError> {
        let node = self
            .nodes
            .get(parent as usize)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        let existing = node
            .edges
            .binary_search_by_key(&(edge.piece, edge.cells), |edge| (edge.piece, edge.cells));
        if let Ok(index) = existing {
            // Preserve the ordinary producer's verified deterministic-action hit.
            if node.edges[index].child != edge.child {
                return Err(invalid("wasm_observation_board_action_not_deterministic"));
            }
            return Ok(());
        }
        let parent_key = node
            .physical_key()
            .ok_or_else(|| invalid("wasm_observation_boundary_requires_proved_edge"))?;
        let child = self
            .nodes
            .get(edge.child as usize)
            .and_then(BoardNode::physical_key)
            .ok_or_else(|| invalid("wasm_observation_physical_child_invalid"))?;
        self.validate_edge_keys(parent_key, edge, child)?;
        self.insert_new_edge(parent, edge, existing.unwrap_err())
    }

    fn insert_new_edge(
        &mut self,
        parent: u32,
        edge: BoardEdge,
        index: usize,
    ) -> Result<(), WasmExactSearchError> {
        if self.metrics.edges >= MAX_EDGES {
            return Err(invalid("wasm_observation_physical_edge_limit"));
        }
        let node = &mut self.nodes[parent as usize];
        let old = node.edges.capacity() * core::mem::size_of::<BoardEdge>();
        node.edges.try_reserve(1).map_err(|_| storage())?;
        let new = node.edges.capacity() * core::mem::size_of::<BoardEdge>();
        node.edges.insert(index, edge);
        self.edge_capacity_bytes += new - old;
        self.metrics.edges += 1;
        self.observe_peak(if new != old { old } else { 0 });
        Ok(())
    }

    fn insert_suffix(&mut self, node: u32, root: u32) -> Result<(), WasmExactSearchError> {
        if root >= KIND_TAG - 1 {
            return Err(invalid("wasm_observation_suffix_language_missing"));
        }
        let suffixes = &mut self.nodes[node as usize].suffixes;
        let old = suffixes.retained_bytes();
        let inserted = suffixes.insert(root, self.metrics.suffix_roots < MAX_UNION_MEMBERS);
        let new = suffixes.retained_bytes();
        #[cfg(feature = "local-search-ab")]
        if matches!(
            inserted,
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_union_limit"
            ))
        ) {
            let root_suffixes = self
                .root
                .map_or(0, |root| self.nodes[root as usize].suffixes.len());
            eprintln!("clearra_observation_board_suffix_limit metrics={:?} node={node} root_suffixes={root_suffixes} retained_bytes={}", self.metrics, self.retained_bytes());
        }
        let (inserted, extra) = inserted?;
        self.suffix_capacity_bytes = self.suffix_capacity_bytes - old + new;
        self.metrics.suffix_roots += usize::from(inserted);
        self.observe_peak(extra);
        Ok(())
    }

    #[cfg(any(test, feature = "parallel"))]
    pub(in crate::backend::wasm_cpu) fn merge(
        &mut self,
        source: Self,
        rebound: &[u32],
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let result = self.merge_inner(source, rebound, control);
        self.merge_peak_bytes = 0;
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    #[cfg(any(test, feature = "parallel"))]
    fn merge_inner(
        &mut self,
        source: Self,
        rebound: &[u32],
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.producing(control)?;
        source.producing(control)?;
        if self.pending_boundaries.is_some() || source.pending_boundaries.is_some() {
            return Err(invalid("wasm_observation_shared_producer_not_closed"));
        }
        if !Arc::ptr_eq(&self.plan, &source.plan) {
            return Err(invalid("wasm_observation_board_request_scope_changed"));
        }
        self.charge(source.metrics.work.saturating_add(1), control)?;
        let source_exact_metrics = source.prefix_cone_exact_metrics();
        let incoming_bytes = source.retained_bytes();
        self.merge_peak_bytes = incoming_bytes;
        self.observe_peak(0);
        self.merge_nodes.clear();
        let old = self.merge_nodes.capacity() * core::mem::size_of::<u32>();
        self.merge_nodes
            .try_reserve_exact(source.nodes.len())
            .map_err(|_| storage())?;
        self.observe_peak(
            if self.merge_nodes.capacity() * core::mem::size_of::<u32>() != old {
                old
            } else {
                0
            },
        );
        for node in &source.nodes {
            self.charge(1, control)?;
            let imported = self.intern_board(
                node.physical_key()
                    .ok_or_else(|| invalid("wasm_observation_boundary_source_already_closed"))?,
            )?;
            self.merge_nodes.push(imported);
        }
        if let Some(root) = source.root {
            let imported = self.merge_nodes[root as usize];
            if self.root.is_some_and(|root| root != imported) {
                return Err(invalid("wasm_observation_board_root_changed"));
            }
            self.root = Some(imported);
        }
        for (index, node) in source.nodes.into_iter().enumerate() {
            let target = self.merge_nodes[index];
            for edge in node.edges {
                self.charge(1, control)?;
                let child = *self
                    .merge_nodes
                    .get(edge.child as usize)
                    .ok_or_else(|| invalid("wasm_observation_physical_child_invalid"))?;
                self.insert_edge(target, BoardEdge { child, ..edge })?;
            }
            let mut import = |root: u32| -> Result<(), WasmExactSearchError> {
                self.charge(1, control)?;
                let root = rebound
                    .get(root as usize)
                    .copied()
                    .filter(|&root| root != NO_NODE)
                    .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                self.insert_suffix(target, root)
            };
            match node.suffixes {
                SuffixRoots::Small(roots) => {
                    for root in roots {
                        import(root)?;
                    }
                }
                SuffixRoots::Large(roots) => {
                    for root in roots {
                        import(root)?;
                    }
                }
                SuffixRoots::FrozenUniqueUnordered(_) => {
                    return Err(invalid("wasm_observation_boundary_source_already_closed"));
                }
            }
        }
        self.metrics.candidate_count += source.metrics.candidate_count;
        self.metrics.prefix_visits += source.metrics.prefix_visits;
        self.metrics.edge_attempts += source.metrics.edge_attempts;
        self.metrics.source_pruned_prefixes += source.metrics.source_pruned_prefixes;
        self.metrics.dead_source_prefixes += source.metrics.dead_source_prefixes;
        for (destination, incoming) in self
            .prefix_mask_metrics
            .iter_mut()
            .zip(source.prefix_mask_metrics)
        {
            *destination += incoming;
        }
        for (destination, incoming) in self
            .prefix_cone_metrics
            .iter_mut()
            .zip(source.prefix_cone_metrics)
        {
            *destination += incoming;
        }
        for (destination, incoming) in self
            .prefix_cone_exact_metrics
            .iter_mut()
            .zip(source_exact_metrics)
        {
            *destination += incoming;
        }
        // Source cone certificates contain source-local board IDs. The actual
        // graph and suffixes were imported above, but certificates are neither
        // copied nor inferred from that import. Destination certificates keep
        // their original IDs because its board arena is append-only.
        self.observe_peak(0);
        Ok(())
    }

    fn prepare_closure_locks(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.closure_locks_ready {
            return Ok(());
        }
        let registry = standard_tetromino_registry();
        for piece in PieceKind::STANDARD_TETROMINOES {
            let index = usize::from(piece_code(piece) - 1);
            let definition = registry
                .get(piece)
                .ok_or_else(|| invalid("wasm_observation_piece_definition_missing"))?;
            for rotation in RotationState::ALL {
                let shape = definition.shape(rotation);
                if shape.width() > self.plan.width || shape.height() > self.plan.height {
                    continue;
                }
                for y in 0..=self.plan.height - shape.height() {
                    for x in 0..=self.plan.width - shape.width() {
                        self.charge(1, control)?;
                        let mut cells = 0_u64;
                        for cell in shape.cells() {
                            let x = u32::from(x) + cell.x() as u32;
                            let y = u32::from(y) + cell.y() as u32;
                            cells |= 1_u64 << (y * u32::from(self.plan.width) + x);
                        }
                        let mut poses = ReachableLocks::default();
                        poses.insert(self.plan.width, rotation, x as i8, y as i8);
                        let locks = &mut self.closure_locks[index];
                        let old = locks.capacity() * core::mem::size_of::<ClosureLock>();
                        locks.try_reserve(1).map_err(|_| storage())?;
                        let new = locks.capacity() * core::mem::size_of::<ClosureLock>();
                        locks.push(ClosureLock {
                            cells,
                            corridor: harddrop_path_mask(cells, self.plan.width),
                            floor: y == 0,
                            poses,
                        });
                        self.closure_pose_capacity_bytes += new - old;
                        self.observe_peak(if new != old { old } else { 0 });
                    }
                }
            }
            let locks = &mut self.closure_locks[index];
            locks.sort_unstable_by_key(|lock| lock.cells);
            locks.dedup_by(|next, previous| {
                if next.cells != previous.cells {
                    return false;
                }
                previous.poses.union_with(next.poses);
                true
            });
        }
        self.closure_locks_ready = true;
        Ok(())
    }

    /// Complete the retained board set once, after the original candidate
    /// family is complete. Every generated edge is an actual grounded lock;
    /// its child must already belong to that family's productive prefix set.
    /// Thus no candidate-specific movement permission is reconstructed, and
    /// all-sky reachability uses the existing request-bound workspace owner.
    fn complete_physical_edges(
        &mut self,
        catalog: &GeometryCatalog,
        reachability: &mut ReachabilityWorkspace,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.pending_boundaries.is_some() {
            return Err(invalid("wasm_observation_shared_producer_not_closed"));
        }
        if !self.requires_edge_closure() || self.edge_closure_complete {
            return Ok(());
        }
        self.mutable(control)?;
        for parent in 0..self.nodes.len() {
            let Some(key) = self.nodes[parent].physical_key() else {
                continue;
            };
            if !self
                .plan
                .should_rebuild(self.plan.total_depth - key.remaining)
            {
                continue;
            }
            // In hybrid mode H=0 and H=1 retain every proof edge. Do not
            // compile poses or query movement when no layer needs rebuilding.
            self.prepare_closure_locks(control)?;
            self.charge(1, control)?;
            self.closure_metrics[4] += 1;
            for piece in PieceKind::STANDARD_TETROMINOES {
                let index = usize::from(piece_code(piece) - 1);
                self.closure_targets.clear();
                let mut desired = ReachableLocks::default();
                for lock_index in 0..self.closure_locks[index].len() {
                    let lock = self.closure_locks[index][lock_index];
                    self.charge(1, control)?;
                    self.closure_metrics[0] += 1;
                    if lock.cells & key.board != 0
                        || (!lock.floor && key.board & (lock.cells >> self.plan.width) == 0)
                    {
                        continue;
                    }
                    // This lookup is genuine work even when the target was
                    // removed by the original-source prefix cap.
                    self.charge(1, control)?;
                    let board =
                        place_and_clear(self.plan.width, self.plan.height, key.board | lock.cells)
                            .0;
                    let Some(child) = self
                        .index
                        .get(&BoardKey {
                            board,
                            remaining: key.remaining - 1,
                        })
                        .copied()
                    else {
                        continue;
                    };
                    if key.board & lock.corridor == 0 {
                        self.closure_metrics[1] += 1;
                        self.charge(1, control)?;
                        self.metrics.edge_attempts += 1;
                        self.insert_edge(
                            parent as u32,
                            BoardEdge {
                                piece: piece_code(piece),
                                cells: lock.cells,
                                child,
                            },
                        )?;
                        continue;
                    }
                    let old =
                        self.closure_targets.capacity() * core::mem::size_of::<ClosureTarget>();
                    self.closure_targets.try_reserve(1).map_err(|_| storage())?;
                    self.closure_targets.push(ClosureTarget {
                        lock: lock_index,
                        child,
                    });
                    self.observe_peak(
                        if self.closure_targets.capacity() * core::mem::size_of::<ClosureTarget>()
                            != old
                        {
                            old
                        } else {
                            0
                        },
                    );
                    desired.union_with(lock.poses);
                }
                if self.closure_targets.is_empty() {
                    continue;
                }
                self.charge(1, control)?;
                self.closure_metrics[2] += 1;
                // Some(desired) may return before exhaustiveness once every
                // requested pose has been proved. Otherwise it exhausts the
                // same primitive. A physical lock needs any of its aliases.
                let found = reachability
                    .reachable_locks_for_board(catalog, key.board, piece, desired, control)?;
                self.closure_metrics[3] += found.visited_state_count;
                self.charge(found.visited_state_count, control)?;
                for target_index in 0..self.closure_targets.len() {
                    let target = self.closure_targets[target_index];
                    let lock = self.closure_locks[index][target.lock];
                    self.charge(1, control)?;
                    if !found.locks.intersects(lock.poses) {
                        if !found.exhaustive {
                            return Err(invalid("wasm_observation_reachability_incomplete"));
                        }
                        continue;
                    }
                    self.charge(1, control)?;
                    self.metrics.edge_attempts += 1;
                    self.insert_edge(
                        parent as u32,
                        BoardEdge {
                            piece: piece_code(piece),
                            cells: lock.cells,
                            child: target.child,
                        },
                    )?;
                }
            }
        }
        self.edge_closure_complete = true;
        self.observe_peak(0);
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::backend::wasm_cpu) fn view<'a>(
        &'a mut self,
        kinds: &'a PieceOrderLanguageCache,
        control: &ExecutionControl,
    ) -> Result<BoardActionLanguageView<'a>, WasmExactSearchError> {
        self.view_with_kind_union(kinds, control, kind_union_enabled()?)
    }

    pub(in crate::backend::wasm_cpu) fn view_with_reachability<'a>(
        &'a mut self,
        kinds: &'a PieceOrderLanguageCache,
        catalog: &GeometryCatalog,
        reachability: &mut ReachabilityWorkspace,
        control: &ExecutionControl,
    ) -> Result<BoardActionLanguageView<'a>, WasmExactSearchError> {
        if (catalog.width(), catalog.height(), catalog.initial_board())
            != (self.plan.width, self.plan.height, self.plan.initial_board)
        {
            return Err(invalid("wasm_observation_board_candidate_scope_changed"));
        }
        let result = self.complete_physical_edges(catalog, reachability, control);
        if let Err(error) = result {
            self.failed = true;
            return Err(error);
        }
        self.view_with_kind_union(kinds, control, kind_union_enabled()?)
    }

    fn view_with_kind_union<'a>(
        &'a mut self,
        kinds: &'a PieceOrderLanguageCache,
        control: &ExecutionControl,
        union_enabled: bool,
    ) -> Result<BoardActionLanguageView<'a>, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if self.failed {
            return Err(invalid("wasm_observation_board_family_failed"));
        }
        if self.plan.ordered_classes_enabled && !self.plan.boundary_classes_selected() {
            self.failed = true;
            return Err(invalid("wasm_observation_ordered_class_scope_invalid"));
        }
        if self
            .kind_arena
            .as_ref()
            .is_some_and(|identity| !Arc::ptr_eq(identity, kinds.arena_identity()))
        {
            return Err(invalid("wasm_observation_suffix_arena_changed"));
        }
        let root = self
            .root
            .ok_or_else(|| invalid("wasm_observation_physical_root_missing"))?;
        if kinds.node_count() >= KIND_TAG as usize {
            return Err(invalid("wasm_observation_kind_namespace_exhausted"));
        }
        let result = self.freeze(kinds, control);
        if let Err(error) = result {
            self.failed = true;
            return Err(error);
        }
        if self.plan.ordered_classes_enabled {
            self.prepare_ordered_policy_quotient(control)?;
        }
        self.kind_arena = Some(Arc::clone(kinds.arena_identity()));
        let mut union = KindUnionArena::new(
            KindUnionResources {
                nodes: self.metrics.nodes,
                edges: self.metrics.edges,
                // Retained original edge-index choices are extra u32
                // references. Identity slices need no additional member.
                members: self.metrics.suffix_roots.saturating_add(
                    self.ordered_quotient
                        .as_ref()
                        .map_or(0, OrderedPolicyQuotient::member_count),
                ),
                work: self.metrics.work,
            },
            KindUnionResources {
                nodes: MAX_NODES,
                edges: MAX_EDGES,
                members: MAX_UNION_MEMBERS,
                work: MAX_WORK,
            },
        )?;
        let supply_multiset_enabled = supply_multiset_enabled()? && union_enabled;
        let groups_enabled = multiset_groups_enabled()? && supply_multiset_enabled;
        union.set_multiset_grouping_enabled(groups_enabled);
        let full_multiset_enabled = full_multiset_enabled()?;
        if full_multiset_enabled
            && (!groups_enabled
                || !self.plan.fresh_bag_observation
                || !self.plan.source_filter
                || !self.plan.boundary_suffixes_only
                || !(1..=6).contains(&(self.plan.total_depth - self.plan.hidden_horizon)))
        {
            return Err(invalid("wasm_observation_full_multiset_scope_invalid"));
        }
        union.set_full_multiset_enabled(full_multiset_enabled);
        let mut revealed = Vec::new();
        if union_enabled {
            revealed
                .try_reserve_exact(self.nodes.len())
                .map_err(|_| storage())?;
            revealed.resize(self.nodes.len(), NO_NODE);
        }
        Ok(BoardActionLanguageView {
            owner: self,
            kinds,
            root,
            union_enabled,
            // The alternative-root AB mode keeps its established unfiltered
            // representation. Supply filtering specializes only lazy unions.
            supply_multiset_enabled,
            word_bitmap_enabled: word_bitmap_enabled()?,
            full_multiset_enabled,
            union: RefCell::new(RevealedKindUnion {
                arena: union,
                revealed,
            }),
        })
    }

    fn freeze(
        &mut self,
        kinds: &PieceOrderLanguageCache,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.frozen {
            return Ok(());
        }
        if self.pending_boundaries.is_some() {
            return Err(invalid("wasm_observation_shared_producer_not_closed"));
        }
        if self.requires_edge_closure() && !self.edge_closure_complete {
            return Err(invalid("wasm_observation_board_edges_not_finalized"));
        }
        for index in 0..self.nodes.len() {
            self.charge(self.nodes[index].suffixes.len().saturating_add(1), control)?;
            if matches!(
                self.nodes[index].suffixes,
                SuffixRoots::FrozenUniqueUnordered(_)
            ) && (!self.producer_closed || self.nodes[index].physical_key().is_some())
            {
                return Err(invalid("wasm_observation_boundary_batch_scope_invalid"));
            }
            let suffixes = &mut self.nodes[index].suffixes;
            let old = suffixes.retained_bytes();
            let extra = suffixes.freeze()?;
            self.suffix_capacity_bytes =
                self.suffix_capacity_bytes - old + suffixes.retained_bytes();
            self.observe_peak(extra);
            let node = &self.nodes[index];
            let depth = self.plan.total_depth - node.remaining();
            if (depth < self.plan.hidden_horizon && node.edges.is_empty())
                || (self.plan.requires_suffix(depth) && node.suffixes.len() == 0)
            {
                return Err(invalid("wasm_observation_board_family_incomplete"));
            }
            for &root in node.suffixes.frozen().unwrap() {
                let kind = kinds
                    .node(root)
                    .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                if kind.remaining_depth != node.remaining()
                    || kind.accepting != (node.remaining() == 0)
                {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
            }
        }
        self.frozen = true;
        Ok(())
    }

    fn charge(
        &mut self,
        steps: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let Some(work) = self
            .metrics
            .work
            .checked_add(steps)
            .filter(|&work| work <= MAX_WORK)
        else {
            #[cfg(feature = "local-search-ab")]
            {
                let mut depths = [[0_usize; 3]; MAX_BOARD64_PIECES + 1];
                for node in &self.nodes {
                    let depth = usize::from(self.plan.total_depth - node.remaining());
                    depths[depth][0] += 1;
                    depths[depth][1] += node.edges.len();
                    depths[depth][2] += node.suffixes.len();
                }
                eprintln!("clearra_observation_board_work_limit metrics={:?} closure={:?} prefix_cones={:?} ordered_classes={:?} depths_nodes_edges_roots={depths:?} attempted_steps={steps} retained_bytes={} peak_bytes={}", self.metrics, self.closure_metrics, self.prefix_cone_metrics, self.ordered_class_metrics, self.retained_bytes(), self.peak_bytes);
            }
            return Err(invalid("wasm_observation_physical_work_limit"));
        };
        self.metrics.work = work;
        Ok(())
    }
}

#[cfg(any(test, feature = "parallel"))]
enum RetainedSuffixRoots<'a> {
    Small(std::slice::Iter<'a, u32>),
    Large(std::collections::hash_set::Iter<'a, u32>),
}

#[cfg(any(test, feature = "parallel"))]
impl Iterator for RetainedSuffixRoots<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Small(roots) => roots.next().copied(),
            Self::Large(roots) => roots.next().copied(),
        }
    }
}

#[cfg(any(test, feature = "parallel"))]
impl BoardActionFamily {
    /// Only the still-mutable producer owns these root bindings. A worker
    /// shard imports this exact closure before its ordinary board merge;
    /// the merge itself still performs the sole physical root rebind/charge.
    /// Both small and large sets are traversed without another root array.
    pub(super) fn retained_suffix_roots(
        &self,
        kinds: &PieceOrderLanguageCache,
        control: &ExecutionControl,
    ) -> Result<impl Iterator<Item = (u32, u8)> + '_, WasmExactSearchError> {
        self.producing(control)?;
        if self.pending_boundaries.is_some() {
            return Err(invalid("wasm_observation_shared_producer_not_closed"));
        }
        if self
            .kind_arena
            .as_ref()
            .is_some_and(|identity| !Arc::ptr_eq(identity, kinds.arena_identity()))
        {
            return Err(invalid("wasm_observation_suffix_arena_changed"));
        }
        Ok(self.nodes.iter().flat_map(|node| {
            let roots = match &node.suffixes {
                SuffixRoots::Small(roots) | SuffixRoots::FrozenUniqueUnordered(roots) => {
                    RetainedSuffixRoots::Small(roots.iter())
                }
                SuffixRoots::Large(roots) => RetainedSuffixRoots::Large(roots.iter()),
            };
            roots.map(move |root| (root, node.remaining()))
        }))
    }
}

fn frame_board(frame: &Frame, mut prefix: u16) -> u64 {
    let mut board = frame.initial;
    while prefix != 0 {
        let operation = prefix.trailing_zeros() as usize;
        prefix &= prefix - 1;
        board |= frame.placements[operation];
    }
    board
}

fn prefix_cone_key(
    pieces: &[PieceKind],
    original: &Frame,
    mut prefix: u16,
) -> Result<[u64; 7], WasmExactSearchError> {
    let mut key = [0; 7];
    while prefix != 0 {
        let operation = prefix.trailing_zeros() as usize;
        prefix &= prefix - 1;
        let piece = pieces
            .get(operation)
            .copied()
            .ok_or_else(|| invalid("wasm_observation_prefix_cone_invalid"))?;
        let kind = usize::from(piece_code(piece) - 1);
        let cells = original.placements[operation];
        if key[kind] != 0 || cells.count_ones() != 4 {
            // A repeated kind cannot be collapsed by OR-ing its placements:
            // its order would no longer be certified by the first-six proof.
            return Err(invalid("wasm_observation_prefix_cone_invalid"));
        }
        key[kind] = cells;
    }
    Ok(key)
}

fn prefix_cone_slot(cells: &[u64; 7]) -> usize {
    prefix_cone_hash(cells) as usize & (PREFIX_CONE_SLOTS - 1)
}

fn prefix_cone_hash(cells: &[u64; 7]) -> u64 {
    let mut hash = 0x9e37_79b9_7f4a_7c15_u64;
    for &cells in cells {
        hash = (hash ^ cells)
            .rotate_left(23)
            .wrapping_mul(0x9ddf_ea08_eb38_2d69);
    }
    hash ^= hash >> 32;
    hash = hash.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    hash ^ (hash >> 32)
}

fn boundary_classes_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_BOUNDARY_CLASSES") {
            Ok(mode) if mode == "0" => Ok(false),
            Ok(mode) if mode == "1" => Ok(true),
            Err(std::env::VarError::NotPresent) => Ok(false),
            _ => Err(invalid("wasm_observation_boundary_classes_mode_invalid")),
        }
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        Ok(false)
    }
}

fn ordered_classes_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    match std::env::var("CLEARRA_OBSERVATION_ORDERED_CLASSES") {
        Ok(mode) if mode == "0" => return Ok(false),
        Ok(mode) if mode == "1" => return Ok(true),
        Err(std::env::VarError::NotPresent) => (),
        _ => return Err(invalid("wasm_observation_ordered_class_mode_invalid")),
    }
    Ok(false)
}

fn prefix_cone_mode() -> Result<u8, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_PREFIX_CONES") {
            Ok(value) if value == "0" => return Ok(0),
            Ok(value) if value == "1" => return Ok(1),
            Ok(value) if value == "2" => return Ok(2),
            Ok(value) if value == "3" => return Ok(3),
            Err(std::env::VarError::NotPresent) => {}
            _ => return Err(invalid("wasm_observation_prefix_cone_mode_invalid")),
        }
    }
    Ok(0)
}

fn edge_closure_mode() -> Result<EdgeClosureMode, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_EDGE_CLOSURE") {
            Ok(value) if value == "0" => return Ok(EdgeClosureMode::Off),
            Ok(value) if value == "1" => return Ok(EdgeClosureMode::Full),
            Ok(value) if value == "2" => return Ok(EdgeClosureMode::BeforeLastPhysicalLayer),
            Err(std::env::VarError::NotPresent) => {}
            _ => return Err(invalid("wasm_observation_edge_closure_mode_invalid")),
        }
    }
    Ok(EdgeClosureMode::Off)
}

fn boundary_suffixes_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_BOUNDARY_SUFFIX") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => return Ok(true),
            Err(std::env::VarError::NotPresent) => {}
            _ => return Err(invalid("wasm_observation_boundary_suffix_mode_invalid")),
        }
    }
    Ok(true)
}

#[cfg(any(test, feature = "local-search-ab"))]
fn prefix_masks_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_PREFIX_MASK") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => return Ok(true),
            Err(std::env::VarError::NotPresent) => {}
            _ => return Err(invalid("wasm_observation_prefix_mask_mode_invalid")),
        }
    }
    Ok(false)
}

fn kind_union_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_KIND_UNION") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => return Ok(true),
            Err(std::env::VarError::NotPresent) => {}
            _ => return Err(invalid("wasm_observation_kind_union_mode_invalid")),
        }
    }
    Ok(true)
}

fn supply_multiset_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_SUPPLY_MULTISET") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => return Ok(true),
            Err(std::env::VarError::NotPresent) => {}
            _ => return Err(invalid("wasm_observation_supply_multiset_mode_invalid")),
        }
    }
    Ok(false)
}

fn multiset_groups_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_MULTISET_GROUPS") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => return Ok(true),
            Err(std::env::VarError::NotPresent) => {}
            _ => return Err(invalid("wasm_observation_multiset_groups_mode_invalid")),
        }
    }
    Ok(false)
}

fn word_bitmap_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_WORD_BITMAP") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => return Ok(true),
            Err(std::env::VarError::NotPresent) => {}
            _ => return Err(invalid("wasm_observation_word_bitmap_mode_invalid")),
        }
    }
    Ok(false)
}

fn full_multiset_enabled() -> Result<bool, WasmExactSearchError> {
    #[cfg(feature = "local-search-ab")]
    {
        match std::env::var("CLEARRA_OBSERVATION_FULL_MULTISET") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => return Ok(true),
            Err(std::env::VarError::NotPresent) => {}
            _ => return Err(invalid("wasm_observation_full_multiset_mode_invalid")),
        }
    }
    Ok(false)
}

fn is_kind_union(node: u32) -> bool {
    node & (KIND_TAG | REVEALED_BOARD_TAG | KIND_UNION_TAG) == KIND_UNION_TAG
}

fn union_reference(reference: u32) -> u32 {
    if reference & KIND_TAG != 0 {
        reference
    } else {
        reference | KIND_UNION_TAG
    }
}

struct RevealedKindUnion {
    arena: KindUnionArena,
    // Policy solve calls reveal before its own memo lookup. Cache this alias
    // so a repeated board does not copy, sort or hash its root set again.
    revealed: Vec<u32>,
}

pub(in crate::backend::wasm_cpu) struct BoardActionLanguageView<'a> {
    owner: &'a BoardActionFamily,
    kinds: &'a PieceOrderLanguageCache,
    root: u32,
    union_enabled: bool,
    supply_multiset_enabled: bool,
    word_bitmap_enabled: bool,
    full_multiset_enabled: bool,
    union: RefCell<RevealedKindUnion>,
}

impl BoardActionLanguageView<'_> {
    pub(in crate::backend::wasm_cpu) fn metrics(&self) -> BoardActionMetrics {
        self.owner.metrics()
    }

    pub(in crate::backend::wasm_cpu) fn kind_union_metrics(&self) -> [usize; 5] {
        self.union.borrow().arena.metrics()
    }

    pub(in crate::backend::wasm_cpu) fn supply_multiset_metrics(&self) -> [usize; 5] {
        self.union.borrow().arena.supply_multiset_metrics()
    }

    pub(in crate::backend::wasm_cpu) fn multiset_group_metrics(&self) -> [usize; 9] {
        self.union.borrow().arena.multiset_group_metrics()
    }

    pub(in crate::backend::wasm_cpu) fn word_bitmap_metrics(&self) -> [usize; 9] {
        self.union.borrow().arena.word_bitmap_metrics()
    }

    pub(in crate::backend::wasm_cpu) fn word_bitmap_board_metrics(&self) -> [usize; 6] {
        self.union.borrow().arena.word_bitmap_board_metrics()
    }

    pub(in crate::backend::wasm_cpu) fn full_multiset_metrics(&self) -> [usize; 6] {
        self.union.borrow().arena.full_multiset_metrics()
    }

    pub(in crate::backend::wasm_cpu) fn edge_closure_metrics(&self) -> [usize; 5] {
        self.owner.closure_metrics
    }

    pub(in crate::backend::wasm_cpu) fn prefix_mask_metrics(&self) -> [usize; 6] {
        self.owner.prefix_mask_metrics
    }

    pub(in crate::backend::wasm_cpu) fn prefix_cone_metrics(&self) -> [usize; 5] {
        self.owner.prefix_cone_metrics()
    }

    pub(in crate::backend::wasm_cpu) fn prefix_cone_exact_metrics(&self) -> [usize; 9] {
        self.owner.prefix_cone_exact_metrics()
    }

    pub(in crate::backend::wasm_cpu) fn boundary_class_metrics(&self) -> [usize; 7] {
        self.owner.boundary_class_metrics()
    }

    pub(in crate::backend::wasm_cpu) fn ordered_class_metrics(&self) -> [usize; 8] {
        self.owner.ordered_class_metrics()
    }

    pub(in crate::backend::wasm_cpu) fn retained_bytes(&self) -> usize {
        let union = self.union.borrow();
        union.arena.retained_bytes() + union.revealed.capacity() * core::mem::size_of::<u32>()
    }

    /// Only view-owned allocation peak; the family and shared kind cache
    /// already belong to the workspace's separate accounting.
    pub(in crate::backend::wasm_cpu) fn peak_bytes(&self) -> usize {
        let union = self.union.borrow();
        union.arena.peak_bytes() + union.revealed.capacity() * core::mem::size_of::<u32>()
    }

    fn board_node(&self, node: u32) -> Option<&BoardNode> {
        if node & (KIND_TAG | KIND_UNION_TAG) != 0 {
            return None;
        }
        self.owner.nodes.get((node & !REVEALED_BOARD_TAG) as usize)
    }
}

impl ObservationPieceLanguage for BoardActionLanguageView<'_> {
    fn root(&self) -> u32 {
        self.root
    }
    fn fixed_horizon(&self) -> Option<u8> {
        Some(self.owner.plan.total_depth)
    }

    fn node(&self, node: u32) -> Option<ObservationLanguageNode> {
        if node & KIND_TAG != 0 {
            let node = self.kinds.node(node & !KIND_TAG)?;
            return Some(ObservationLanguageNode {
                accepting: node.accepting,
                depth: self
                    .owner
                    .plan
                    .total_depth
                    .checked_sub(node.remaining_depth)?,
            });
        }
        if is_kind_union(node) {
            let (remaining, accepting) = self.union.borrow().arena.node(node & !KIND_UNION_TAG)?;
            return Some(ObservationLanguageNode {
                accepting,
                depth: self.owner.plan.total_depth.checked_sub(remaining)?,
            });
        }
        let node = self.board_node(node)?;
        Some(ObservationLanguageNode {
            accepting: node.accepting(),
            depth: self.owner.plan.total_depth.checked_sub(node.remaining())?,
        })
    }

    fn edge_count(&self, node: u32) -> Option<usize> {
        if node & KIND_TAG != 0 {
            return self.kinds.node_edge_count(node & !KIND_TAG);
        }
        if is_kind_union(node) {
            return self.union.borrow().arena.edge_count(node & !KIND_UNION_TAG);
        }
        let board = self.board_node(node)?;
        Some(if node & REVEALED_BOARD_TAG != 0 {
            0
        } else if let Some(quotient) = &self.owner.ordered_quotient {
            quotient.edge_count(node)?
        } else {
            board.edges.len()
        })
    }

    fn edge(&self, node: u32, index: usize) -> Option<(u8, u32)> {
        if node & KIND_TAG != 0 {
            let edge = self.kinds.edge_at(node & !KIND_TAG, index)?;
            return Some((edge.piece_code(), edge.child() | KIND_TAG));
        }
        if is_kind_union(node) {
            let (piece, child) = self
                .union
                .borrow()
                .arena
                .edge(node & !KIND_UNION_TAG, index)?;
            return Some((piece, union_reference(child)));
        }
        if node & REVEALED_BOARD_TAG != 0 {
            return None;
        }
        let index = match &self.owner.ordered_quotient {
            Some(quotient) => quotient.original_edge_index(node, index)?,
            None => index,
        };
        let edge = self.board_node(node)?.edges.get(index)?;
        Some((edge.piece, edge.child))
    }

    fn memo_class(&self, node: u32) -> Option<u32> {
        self.node(node)?;
        if node & (REVEALED_BOARD_TAG | KIND_TAG | KIND_UNION_TAG) == 0 {
            if let Some(quotient) = &self.owner.ordered_quotient {
                return quotient.representative(node);
            }
        }
        Some(node)
    }

    fn has_physical_actions(&self, node: u32) -> bool {
        node & (REVEALED_BOARD_TAG | KIND_TAG | KIND_UNION_TAG) == 0
            && self.board_node(node).is_some_and(|board| {
                board.physical_key().is_some()
                    && board.remaining() != 0
                    && self.owner.plan.total_depth - board.remaining()
                        < self.owner.plan.hidden_horizon
            })
    }

    fn physical_action_cells(&self, node: u32, index: usize) -> Option<u64> {
        if !self.has_physical_actions(node) {
            return None;
        }
        let index = match &self.owner.ordered_quotient {
            Some(quotient) => quotient.original_edge_index(node, index)?,
            None => index,
        };
        Some(self.board_node(node)?.edges.get(index)?.cells)
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
        let metadata = self
            .node(node)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        if metadata.accepting || node & KIND_TAG != 0 {
            return Ok(());
        }
        if is_kind_union(node) {
            return self.union.borrow_mut().arena.prepare_edges(
                node & !KIND_UNION_TAG,
                piece_mask,
                self.kinds,
                control,
            );
        }
        if node & REVEALED_BOARD_TAG != 0 {
            return Err(invalid(
                "wasm_observation_board_revealed_union_requires_membership",
            ));
        }
        if metadata.depth >= self.owner.plan.hidden_horizon {
            return Err(invalid("wasm_observation_physical_hidden_horizon_exceeded"));
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
        let metadata = self
            .node(node)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        if metadata.accepting || node & KIND_TAG != 0 || is_kind_union(node) {
            return Ok(node);
        }
        if self.owner.plan.boundary_suffixes_only && metadata.depth < self.owner.plan.hidden_horizon
        {
            // The original-source reveal certificate is owned by the evaluator.
            // Keeping the physical node here does not hide information again:
            // every later decision uses that same now-known source suffix.
            // The completed source-relative closure preserves all successful
            // continuations, including those retained by the source cap. At H
            // the existing exact kind union still replaces physical execution.
            return Ok(node & !REVEALED_BOARD_TAG);
        }
        if self
            .board_node(node)
            .is_none_or(|board| board.suffixes.len() == 0)
        {
            return Err(invalid("wasm_observation_suffix_language_missing"));
        }
        if self.union_enabled {
            let board_id = node & !REVEALED_BOARD_TAG;
            let mut union = self.union.borrow_mut();
            let cached = *union
                .revealed
                .get(board_id as usize)
                .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
            if cached != NO_NODE {
                return Ok(cached);
            }
            let board = self
                .board_node(node)
                .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
            let roots = board
                .suffixes
                .frozen()
                .ok_or_else(|| invalid("wasm_observation_board_family_not_frozen"))?;
            let reference = union
                .arena
                .intern(board.remaining(), roots, self.kinds, control)?;
            let reference = union_reference(reference);
            union.revealed[board_id as usize] = reference;
            return Ok(reference);
        }
        Ok(node | REVEALED_BOARD_TAG)
    }

    fn wants_revealed_supply(&self, node: u32) -> bool {
        self.supply_multiset_enabled
            && self.union_enabled
            && self.board_node(node).is_some_and(|board| {
                board.remaining() != 0
                    && self.owner.plan.total_depth - board.remaining()
                        == self.owner.plan.hidden_horizon
            })
    }

    fn supports_revealed_word_membership(&self, node: u32) -> bool {
        (self.word_bitmap_enabled || self.full_multiset_enabled)
            && self.union_enabled
            && self.owner.plan.fresh_bag_observation
            && self.owner.plan.source_filter
            && self.owner.plan.boundary_suffixes_only
            && self.board_node(node).is_some_and(|board| {
                (1..=6).contains(&board.remaining())
                    && self.owner.plan.total_depth.checked_sub(board.remaining())
                        == Some(self.owner.plan.hidden_horizon)
            })
    }

    fn revealed_word_membership(
        &self,
        node: u32,
        query: &KnownSupplyQuery,
        control: &ExecutionControl,
    ) -> Result<Option<bool>, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if !self.supports_revealed_word_membership(node) {
            return Ok(None);
        }
        let board = self
            .board_node(node)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        let roots = board
            .suffixes
            .frozen()
            .ok_or_else(|| invalid("wasm_observation_board_family_not_frozen"))?;
        // This frozen view pins both the board/class ID and its exact unique
        // root set for the arena lifetime. Root order is not part of that
        // identity; the evaluator alone grants actual-observed supply access.
        let mut union = self.union.borrow_mut();
        if self.full_multiset_enabled {
            match union.arena.full_multiset_membership(
                node & !REVEALED_BOARD_TAG,
                board.remaining(),
                roots,
                query,
                self.kinds,
                control,
            )? {
                Some(true) => return Ok(Some(true)),
                Some(false) => {
                    return Err(invalid("wasm_observation_full_multiset_negative_invalid"));
                }
                None => {}
            }
        }
        if !self.word_bitmap_enabled {
            return Ok(None);
        }
        union.arena.word_bitmap_membership_for_board(
            node & !REVEALED_BOARD_TAG,
            board.remaining(),
            roots,
            query,
            self.kinds,
            control,
        )
    }

    fn fully_revealed_node_with_supply(
        &self,
        node: u32,
        available: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if !self.wants_revealed_supply(node) {
            return self.fully_revealed_node(node, control);
        }
        let board = self
            .board_node(node)
            .ok_or_else(|| invalid("wasm_observation_physical_root_invalid"))?;
        let roots = board
            .suffixes
            .frozen()
            .ok_or_else(|| invalid("wasm_observation_board_family_not_frozen"))?;
        let reference = self.union.borrow_mut().arena.intern_for_supply(
            node & !REVEALED_BOARD_TAG,
            board.remaining(),
            roots,
            available,
            self.kinds,
            control,
        )?;
        if let Some(reference) = reference {
            return Ok(union_reference(reference));
        }
        // Overflow or an optional alias-cache bound preserves the full
        // language. It does not populate the filtered or unfiltered alias
        // with an empty result.
        self.fully_revealed_node(node, control)
    }

    fn revealed_alternatives(
        &self,
        node: u32,
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u32>>, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if self.node(node).is_none() {
            return Err(invalid("wasm_observation_physical_root_invalid"));
        }
        if node & KIND_TAG != 0 || is_kind_union(node) || node & REVEALED_BOARD_TAG == 0 {
            return Ok(None);
        }
        let roots = self
            .board_node(node)
            .and_then(|board| board.suffixes.frozen())
            .ok_or_else(|| invalid("wasm_observation_board_family_not_frozen"))?;
        let mut alternatives = Vec::new();
        alternatives
            .try_reserve_exact(roots.len())
            .map_err(|_| storage())?;
        alternatives.extend(roots.iter().map(|&root| root | KIND_TAG));
        Ok(Some(alternatives))
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
    };
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };
    use clearra_rules::kicks::KickTableProfileId;

    use crate::backend::wasm_cpu::{
        catalog::GeometryCatalog, geometry::GeometryCandidate,
        queue_observation_policy::QueueObservationPolicyEvaluator,
        reachability::ReachabilityWorkspace, symbolic_prefix_domain::MAX_PREFIX_WORDS,
        symbolic_prefix_orders::SymbolicPrefixWork,
    };
    use clearra_supply::{pattern_universe::MaterializedPatternUniverse, QueueObservationPolicy};

    use super::super::PhysicalActionLanguage;
    use super::*;

    fn boundary_mode(
        mut plan: Arc<BoardObservationPlan>,
        enabled: bool,
    ) -> Arc<BoardObservationPlan> {
        Arc::get_mut(&mut plan).unwrap().boundary_suffixes_only = enabled;
        plan
    }

    fn edge_mode(
        mut plan: Arc<BoardObservationPlan>,
        mode: EdgeClosureMode,
    ) -> Arc<BoardObservationPlan> {
        Arc::get_mut(&mut plan).unwrap().edge_closure = mode;
        plan
    }

    pub(super) fn initial() -> u64 {
        let left = 3 | (3 << 10);
        ((1_u64 << 20) - 1) & !(left | (left << 2))
    }

    pub(super) fn two_os() -> SymbolicPrefixOrders {
        let left = 3 | (3 << 10);
        let required = left | (left << 2);
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 2, initial(), required)
                .unwrap();
        let rows = [left, left << 2].map(|cells| catalog.skeleton_id(PieceKind::O, cells).unwrap());
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

    pub(super) fn two_is() -> SymbolicPrefixOrders {
        let required = 15 | (15 << 10);
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 2, initial(), required)
                .unwrap();
        let rows = [15, 15 << 10].map(|cells| catalog.skeleton_id(PieceKind::I, cells).unwrap());
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

    fn register(family: &mut BoardActionFamily, kinds: &mut PieceOrderLanguageCache) {
        register_orders(family, kinds, two_os());
    }

    pub(super) fn register_orders(
        family: &mut BoardActionFamily,
        kinds: &mut PieceOrderLanguageCache,
        orders: SymbolicPrefixOrders,
    ) {
        let mut suffixes = Vec::new();
        orders
            .compile_product_language_with_residuals(
                kinds,
                &mut SymbolicPrefixWork::default(),
                &mut suffixes,
            )
            .unwrap();
        family
            .register(orders, &suffixes, &ExecutionControl::default())
            .unwrap();
    }

    fn two_os_and_i() -> (u64, SymbolicPrefixOrders) {
        let left = 3 | (3 << 10);
        let upper_i = 15 << 14;
        let required = left | (left << 2) | upper_i;
        let initial = ((1_u64 << 20) - 1) & !required;
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 2, initial, required)
                .unwrap();
        let rows = [
            (PieceKind::O, left),
            (PieceKind::O, left << 2),
            (PieceKind::I, upper_i),
        ]
        .map(|(piece, cells)| catalog.skeleton_id(piece, cells).unwrap());
        let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
        let orders = SymbolicPrefixOrders::compile_product_with_workspace(
            &catalog,
            &candidate,
            KickTableProfileId::SrsPlus,
            &ExecutionControl::default(),
            &mut ReachabilityWorkspace::default(),
            &mut SymbolicPrefixWork::default(),
            0,
        )
        .unwrap();
        (initial, orders)
    }

    fn physical_paths<G: ObservationPieceLanguage>(language: &G, node: u32) -> Vec<Vec<(u8, u64)>> {
        physical_prefixes(language, node, u8::MAX)
    }

    pub(super) fn physical_prefixes<G: ObservationPieceLanguage>(
        language: &G,
        node: u32,
        until_depth: u8,
    ) -> Vec<Vec<(u8, u64)>> {
        let metadata = language.node(node).unwrap();
        if metadata.accepting || metadata.depth == until_depth {
            return vec![Vec::new()];
        }
        language
            .prepare_edges(node, 0x7f, &ExecutionControl::default())
            .unwrap();
        assert!(language.has_physical_actions(node));
        let mut paths = Vec::new();
        for index in 0..language.edge_count(node).unwrap() {
            let (piece, child) = language.edge(node, index).unwrap();
            let cells = language.physical_action_cells(node, index).unwrap();
            for suffix in physical_prefixes(language, child, until_depth) {
                let mut path = vec![(piece, cells)];
                path.extend(suffix);
                paths.push(path);
            }
        }
        paths.sort_unstable();
        paths
    }

    fn fresh_bag_orders() -> SymbolicPrefixOrders {
        fresh_bag_orders_for_tail(false)
    }

    pub(super) fn fresh_bag_orders_for_tail(alternate: bool) -> SymbolicPrefixOrders {
        use PieceKind::{I, J, L, O, S, T, Z};

        let mut placements = [
            (I, 0xf),
            (I, 0xf0),
            (O, 0xc0300),
            (T, 0x201c00),
            (Z, 0xc06000),
            (Z, 0x1006008000),
            (S, 0x18030000),
            (L, 0x1c0100000),
            (T, 0xe01000000),
            (J, 0xe020000000),
        ];
        if alternate {
            // Retile the left-middle eight original cells from T+Z to S+T.
            // I(left), O, S(right), J form a common H-prefix; the new T
            // and the vertical Z can extend it through six distinct kinds.
            placements[3] = (S, 0x600c00);
            placements[4] = (T, 0x807000);
        }
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, (1_u64 << 40) - 1)
                .unwrap();
        let rows = placements.map(|(piece, cells)| catalog.skeleton_id(piece, cells).unwrap());
        let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
        SymbolicPrefixOrders::compile_product_with_workspace(
            &catalog,
            &candidate,
            KickTableProfileId::Jstris180,
            &ExecutionControl::default(),
            &mut ReachabilityWorkspace::default(),
            &mut SymbolicPrefixWork::default(),
            0,
        )
        .unwrap()
    }

    pub(super) fn source_filter_plan(enabled: bool) -> Arc<BoardObservationPlan> {
        let mut plan = edge_mode(
            boundary_mode(
                BoardObservationPlan::new(10, 4, 0, 10, 4, 3, Some([1; 7])).unwrap(),
                true,
            ),
            EdgeClosureMode::Off,
        );
        let private = Arc::get_mut(&mut plan).unwrap();
        private.enable_fresh_bag_observation();
        private.prefix_masks_enabled = true;
        if enabled {
            private.enable_source_filter();
        }
        plan
    }

    mod prefix_cone_tests {
        use super::*;
        use crate::backend::wasm_cpu::symbolic_prefix_orders::SourceFilterMetrics;

        type NodeSnapshot = (u8, u64, Vec<(u8, u64, u8, u64)>, Vec<u32>);

        fn plan(enabled: bool, horizon: u8, mode: EdgeClosureMode) -> Arc<BoardObservationPlan> {
            let mut plan = source_filter_plan(true);
            let private = Arc::get_mut(&mut plan).unwrap();
            private.hidden_horizon = horizon;
            private.reveal_floor = private.reveal_floor.min(horizon);
            private.edge_closure = mode;
            private.prefix_cones_enabled = enabled;
            private.prefix_cones_exact = false;
            private.prefix_cones_ancestors_only = false;
            plan
        }

        fn exact_plan(horizon: u8, mode: EdgeClosureMode) -> Arc<BoardObservationPlan> {
            let mut plan = plan(true, horizon, mode);
            Arc::get_mut(&mut plan).unwrap().prefix_cones_exact = true;
            plan
        }

        fn ancestor_plan(horizon: u8, mode: EdgeClosureMode) -> Arc<BoardObservationPlan> {
            let mut plan = exact_plan(horizon, mode);
            Arc::get_mut(&mut plan).unwrap().prefix_cones_ancestors_only = true;
            plan
        }

        fn suffixes(
            orders: &SourceQualifiedPrefixOrders<'_>,
            kinds: &mut PieceOrderLanguageCache,
            horizon: u8,
        ) -> Vec<u32> {
            let mut suffixes = Vec::new();
            orders
                .compile_product_suffixes_with_residuals(
                    kinds,
                    horizon,
                    &mut SymbolicPrefixWork::default(),
                    &mut suffixes,
                )
                .unwrap();
            suffixes
        }

        fn graph(family: &BoardActionFamily) -> Vec<NodeSnapshot> {
            let mut graph = Vec::new();
            for node in &family.nodes {
                let mut edges: Vec<_> = node
                    .edges
                    .iter()
                    .map(|edge| {
                        let child = family.nodes[edge.child as usize].physical_key().unwrap();
                        (edge.piece, edge.cells, child.remaining, child.board)
                    })
                    .collect();
                edges.sort_unstable();
                let mut roots: Vec<_> = match &node.suffixes {
                    SuffixRoots::Small(roots) | SuffixRoots::FrozenUniqueUnordered(roots) => {
                        roots.clone()
                    }
                    SuffixRoots::Large(roots) => roots.iter().copied().collect(),
                };
                roots.sort_unstable();
                graph.push((
                    node.remaining(),
                    node.physical_key().unwrap().board,
                    edges,
                    roots,
                ));
            }
            graph.sort_unstable();
            graph
        }

        #[test]
        fn prefix_cones_reuse_all_predecessor_edges_with_exact_graph_and_work() {
            let control = ExecutionControl::default();
            let original = fresh_bag_orders();
            let qualified = SourceQualifiedPrefixOrders::new(
                &original,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            for (horizon, mode) in [
                (4, EdgeClosureMode::Off),
                (4, EdgeClosureMode::Full),
                (4, EdgeClosureMode::BeforeLastPhysicalLayer),
                (1, EdgeClosureMode::BeforeLastPhysicalLayer),
                (5, EdgeClosureMode::BeforeLastPhysicalLayer),
            ] {
                let mut kinds = PieceOrderLanguageCache::default();
                let suffixes = suffixes(&qualified, &mut kinds, horizon);
                let mut plain = BoardActionFamily::new(plan(false, horizon, mode));
                let mut cached = BoardActionFamily::new(plan(true, horizon, mode));
                for family in [&mut plain, &mut cached] {
                    family
                        .register_source_qualified(&qualified, &suffixes, &control)
                        .unwrap();
                }
                assert_eq!(graph(&cached), graph(&plain));
                assert_eq!(cached.metrics.prefix_visits, plain.metrics.prefix_visits);
                assert_eq!(cached.metrics.edge_attempts, plain.metrics.edge_attempts);
                let before = cached.metrics;
                let cones = cached.prefix_cone_metrics();
                assert!(cones[0] > 0);
                assert_eq!(cones[1], 0);
                assert_eq!(cones[0], cones[2]);
                assert_eq!(cones[2], cones[4]);
                assert_eq!(
                    before.work,
                    plain.metrics.work + PREFIX_CONE_SLOTS + cones[4]
                );
                assert_eq!(cached.prefix_cones.len(), PREFIX_CONE_SLOTS);
                assert!(
                    cached.retained_bytes()
                        >= PREFIX_CONE_SLOTS * core::mem::size_of::<PrefixConeEntry>()
                );
                assert!(cached.peak_bytes() >= cached.retained_bytes());

                cached
                    .register_source_qualified(&qualified, &suffixes, &control)
                    .unwrap();
                assert_eq!(graph(&cached), graph(&plain));
                let after = cached.prefix_cone_metrics();
                assert!(after[1] > cones[1]);
                assert!(
                    cached.metrics.edge_attempts - before.edge_attempts
                        <= plain.metrics.edge_attempts
                );
                if after[2] == cones[2] {
                    assert_eq!(
                        cached.metrics.prefix_visits - before.prefix_visits,
                        cones[0]
                    );
                    assert_eq!(cached.metrics.edge_attempts, before.edge_attempts);
                    assert_eq!(cached.metrics.work - before.work, cones[0] + 1);
                    assert!(cached.metrics.work - before.work < plain.metrics.work);
                }
                assert_eq!(cached.metrics.candidate_count, 2);
                assert_eq!(cached.metrics.source_pruned_prefixes, 0);
                assert_eq!(cached.metrics.dead_source_prefixes, 0);
                assert_eq!(cached.prefix_mask_metrics, [0; 6]);
            }
        }

        #[test]
        fn prefix_cones_hits_add_current_suffix_and_preserve_weighted_original_ids() {
            use PieceKind::{I, J, L, O, S, T, Z};
            let control = ExecutionControl::default();
            let original = fresh_bag_orders();
            let qualified = SourceQualifiedPrefixOrders::new(
                &original,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            let alternate_original = fresh_bag_orders_for_tail(true);
            let alternate = SourceQualifiedPrefixOrders::new(
                &alternate_original,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            assert!(alternate.has_witness());
            let first = vec![J, I, O, T, Z, S, L, Z, T, I, O];
            let second = vec![J, I, O, T, Z, S, L, Z, T, I, S];
            let universe = MaterializedPatternUniverse::from_sequences(
                PatternUniverseId::new(995),
                PatternWeightModelId::new(996),
                vec![first.clone(), second, first],
                vec![
                    ProbabilityValue::new(0.2).unwrap(),
                    ProbabilityValue::new(0.3).unwrap(),
                    ProbabilityValue::new(0.5).unwrap(),
                ],
                3,
                true,
                None,
            )
            .unwrap();
            let mut expected = None;
            for cone_mode in [0, 1, 2, 3] {
                let enabled = cone_mode != 0;
                let mut kinds = PieceOrderLanguageCache::default();
                let first_suffixes = suffixes(&qualified, &mut kinds, 4);
                let alternate_suffixes = suffixes(&alternate, &mut kinds, 4);
                let mut family = BoardActionFamily::new(if cone_mode == 3 {
                    ancestor_plan(4, EdgeClosureMode::Off)
                } else if cone_mode == 2 {
                    exact_plan(4, EdgeClosureMode::Off)
                } else {
                    plan(enabled, 4, EdgeClosureMode::Off)
                });
                family
                    .register_source_qualified(&qualified, &first_suffixes, &control)
                    .unwrap();
                let mut new_bindings = Vec::new();
                let hits = family.prefix_cone_metrics()[1];
                if enabled {
                    let proof = alternate_original.observation_relation().unwrap();
                    let (initial, placements) = alternate_original.geometry().symbolic_frame(0);
                    let original_frame = Frame {
                        initial,
                        placements,
                    };
                    for (prefix, &root) in alternate_suffixes.iter().enumerate() {
                        if prefix.count_ones() != 4 || !alternate.contains_prefix(prefix as u16) {
                            continue;
                        }
                        let key = prefix_cone_key(proof.pieces(), &original_frame, prefix as u16)
                            .unwrap();
                        let node = if cone_mode == 2 {
                            family.exact_cone_lookup(&key, &control).unwrap()
                        } else {
                            let entry = family.prefix_cones[prefix_cone_slot(&key)];
                            (entry.node != ProducerNode::MISSING && entry.cells == key)
                                .then_some(entry.node)
                        };
                        if let Some(node) = node {
                            let contains = match &family.nodes
                                [node.physical_node().unwrap() as usize]
                                .suffixes
                            {
                                SuffixRoots::Small(roots)
                                | SuffixRoots::FrozenUniqueUnordered(roots) => {
                                    roots.contains(&root)
                                }
                                SuffixRoots::Large(roots) => roots.contains(&root),
                            };
                            assert!(
                                !contains,
                                "different future kind multisets cannot share a suffix root"
                            );
                            new_bindings.push((node, root));
                        }
                    }
                    assert!(
                        !new_bindings.is_empty(),
                        "the actual candidates share a certified H cone"
                    );
                }
                family
                    .register_source_qualified(&alternate, &alternate_suffixes, &control)
                    .unwrap();
                if enabled {
                    assert!(family.prefix_cone_metrics()[1] > hits);
                    for (node, root) in new_bindings {
                        let contains = match &family.nodes[node.physical_node().unwrap() as usize]
                            .suffixes
                        {
                            SuffixRoots::Small(roots)
                            | SuffixRoots::FrozenUniqueUnordered(roots) => roots.contains(&root),
                            SuffixRoots::Large(roots) => roots.contains(&root),
                        };
                        assert!(
                            contains,
                            "a physical cache hit must add the new actual suffix"
                        );
                    }
                }
                let combined_graph = graph(&family);
                let view = family.view_with_kind_union(&kinds, &control, true).unwrap();
                assert_eq!(view.prefix_cone_metrics()[0] > 0, enabled);
                let paths = physical_prefixes(&view, view.root(), 4);
                let coverage = QueueObservationPolicyEvaluator::for_original_universe(
                    &universe,
                    QueueObservationPolicy::VisibleSeven,
                    0,
                    None,
                    true,
                    false,
                    false,
                    None,
                )
                .unwrap()
                .evaluate(&view, &control)
                .unwrap();
                assert_eq!(coverage.covered_pattern_count, 3);
                let actual = (
                    combined_graph,
                    paths,
                    coverage.covered_patterns.words().to_vec(),
                    coverage.covered_weight.to_bits(),
                    coverage.min_accepted_depth,
                    coverage.max_accepted_depth,
                );
                if let Some(expected) = &expected {
                    assert_eq!(&actual, expected);
                } else {
                    expected = Some(actual);
                }
            }
        }

        #[test]
        fn prefix_cones_compare_full_keys_and_replace_collisions_without_aliasing() {
            let control = ExecutionControl::default();
            let original = fresh_bag_orders();
            let qualified = SourceQualifiedPrefixOrders::new(
                &original,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            let mut kinds = PieceOrderLanguageCache::default();
            let suffixes = suffixes(&qualified, &mut kinds, 4);
            let mut family = BoardActionFamily::new(plan(true, 4, EdgeClosureMode::Off));
            family
                .register_source_qualified(&qualified, &suffixes, &control)
                .unwrap();
            let expected = graph(&family);
            let entry = *family
                .prefix_cones
                .iter()
                .find(|entry| entry.node != ProducerNode::MISSING)
                .unwrap();
            let slot = prefix_cone_slot(&entry.cells);
            let mut collision = entry;
            collision.cells[0] ^= 1;
            // Force an occupied hash slot with a different full key. The
            // lookup must miss even though the cached node itself is valid.
            family.prefix_cones[slot] = collision;
            let before = family.prefix_cone_metrics();
            family
                .register_source_qualified(&qualified, &suffixes, &control)
                .unwrap();
            let after = family.prefix_cone_metrics();
            assert!(after[2] > before[2]);
            assert!(after[3] > before[3]);
            assert!(after[4] > before[4]);
            assert_eq!(family.prefix_cones.len(), PREFIX_CONE_SLOTS);
            assert_eq!(graph(&family), expected);

            let frame = Frame {
                initial: 0,
                placements: [0xf; MAX_BOARD64_PIECES],
            };
            assert!(prefix_cone_key(&[PieceKind::I, PieceKind::I], &frame, 3).is_err());
            let mut first = frame;
            first.placements[1] = 0xf0;
            let mut reordered = first;
            reordered.placements.swap(0, 1);
            assert_eq!(
                prefix_cone_key(&[PieceKind::I, PieceKind::T], &first, 3).unwrap(),
                prefix_cone_key(&[PieceKind::T, PieceKind::I], &reordered, 3).unwrap(),
            );
        }

        #[test]
        fn prefix_cones_merge_counts_without_importing_worker_local_certificates() {
            let control = ExecutionControl::default();
            let original = fresh_bag_orders();
            let qualified = SourceQualifiedPrefixOrders::new(
                &original,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            let mut kinds = PieceOrderLanguageCache::default();
            let suffixes = suffixes(&qualified, &mut kinds, 4);
            let request_plan = plan(true, 4, EdgeClosureMode::Off);
            let mut worker = BoardActionFamily::new(Arc::clone(&request_plan));
            worker
                .register_source_qualified(&qualified, &suffixes, &control)
                .unwrap();
            let expected = graph(&worker);
            let cones = worker.prefix_cone_metrics();
            let worker_bytes = worker.retained_bytes();
            let rebound: Vec<_> = (0..kinds.node_count() as u32).collect();
            let mut destination = BoardActionFamily::new(request_plan);
            destination.merge(worker, &rebound, &control).unwrap();
            assert_eq!(destination.prefix_cone_metrics(), cones);
            assert!(destination.prefix_cones.is_empty());
            assert_eq!(graph(&destination), expected);
            assert!(destination.peak_bytes() >= worker_bytes);
            destination
                .register_source_qualified(&qualified, &suffixes, &control)
                .unwrap();
            assert_eq!(destination.prefix_cone_metrics()[1], 0);
            assert_eq!(destination.prefix_cone_metrics()[2], cones[2] * 2);
            assert_eq!(graph(&destination), expected);
            assert_eq!(destination.metrics.candidate_count, 2);

            let other = BoardActionFamily::new(plan(true, 4, EdgeClosureMode::Off));
            assert!(matches!(
                destination.merge(other, &rebound, &control),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_board_request_scope_changed"
                ))
            ));
            assert!(destination.failed);
        }

        #[test]
        fn prefix_cones_publish_only_completed_cones_and_keep_failures_explicit() {
            let control = ExecutionControl::default();
            let original = fresh_bag_orders();
            let qualified = SourceQualifiedPrefixOrders::new(
                &original,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            let mut kinds = PieceOrderLanguageCache::default();
            let suffixes = suffixes(&qualified, &mut kinds, 4);
            let mut complete = BoardActionFamily::new(plan(true, 4, EdgeClosureMode::Off));
            complete
                .register_source_qualified(&qualified, &suffixes, &control)
                .unwrap();
            let before_publish = complete.metrics.work - complete.prefix_cone_metrics()[4];
            let mut limited = BoardActionFamily::new(plan(true, 4, EdgeClosureMode::Off));
            limited.metrics.work = MAX_WORK - before_publish;
            assert!(matches!(
                limited.register_source_qualified(&qualified, &suffixes, &control),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_physical_work_limit"
                ))
            ));
            assert_eq!(limited.metrics.work, MAX_WORK);
            assert_eq!(limited.metrics.candidate_count, 0);
            assert!(limited.failed);
            assert_eq!(limited.prefix_cone_metrics()[4], 0);
            assert!(limited
                .prefix_cones
                .iter()
                .all(|entry| entry.node == ProducerNode::MISSING));
            assert!(limited.view(&kinds, &control).is_err());

            let cancelled = ExecutionControl::default();
            cancelled.cancellation.handle().cancel();
            let before = complete.prefix_cone_metrics();
            assert!(matches!(
                complete.register_source_qualified(&qualified, &suffixes, &cancelled),
                Err(WasmExactSearchError::Cancelled)
            ));
            assert_eq!(complete.prefix_cone_metrics(), before);
            assert!(complete.failed);
        }

        #[test]
        fn prefix_cones_exact_reuse_ancestor_cones_without_losing_new_outgoing_edges() {
            let control = ExecutionControl::default();
            let first = fresh_bag_orders();
            let second = fresh_bag_orders_for_tail(true);
            let first = SourceQualifiedPrefixOrders::new(
                &first,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            let second = SourceQualifiedPrefixOrders::new(
                &second,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            for mode in [
                EdgeClosureMode::Off,
                EdgeClosureMode::BeforeLastPhysicalLayer,
            ] {
                let mut kinds = PieceOrderLanguageCache::default();
                let first_suffixes = suffixes(&first, &mut kinds, 5);
                let second_suffixes = suffixes(&second, &mut kinds, 5);
                let mut plain = BoardActionFamily::new(plan(false, 5, mode));
                let mut exact = BoardActionFamily::new(exact_plan(5, mode));
                for family in [&mut plain, &mut exact] {
                    family
                        .register_source_qualified(&first, &first_suffixes, &control)
                        .unwrap();
                }
                assert_eq!(graph(&exact), graph(&plain));
                let before = exact.prefix_cone_exact_metrics();
                let old_edges = exact.metrics.edges;
                for family in [&mut plain, &mut exact] {
                    family
                        .register_source_qualified(&second, &second_suffixes, &control)
                        .unwrap();
                }
                assert_eq!(graph(&exact), graph(&plain));
                assert!(exact.prefix_cone_exact_metrics()[3] > before[3]);
                assert!(exact.metrics.edges > old_edges);
                assert!(exact.metrics.prefix_visits < plain.metrics.prefix_visits);
                assert!(exact.prefix_cones.is_empty());
                assert!(exact.exact_cones.len > exact.prefix_cone_metrics()[4]);
                assert!(exact.peak_bytes() >= exact.retained_bytes());
            }
        }

        #[test]
        fn prefix_cones_ancestor_only_preserves_direct_h_cache_and_bounded_fallback() {
            let control = ExecutionControl::default();
            let original = fresh_bag_orders();
            let alternate = fresh_bag_orders_for_tail(true);
            let first = SourceQualifiedPrefixOrders::new(
                &original,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            let second = SourceQualifiedPrefixOrders::new(
                &alternate,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            let mut kinds = PieceOrderLanguageCache::default();
            let first_suffixes = suffixes(&first, &mut kinds, 5);
            let second_suffixes = suffixes(&second, &mut kinds, 5);
            let mode = EdgeClosureMode::BeforeLastPhysicalLayer;
            let mut direct = BoardActionFamily::new(plan(true, 5, mode));
            let mut ancestors = BoardActionFamily::new(ancestor_plan(5, mode));
            let mut bounded = BoardActionFamily::new(ancestor_plan(5, mode));
            bounded.exact_cones.limit = 2;
            for (orders, roots) in [(&first, &first_suffixes), (&second, &second_suffixes)] {
                for family in [&mut direct, &mut ancestors, &mut bounded] {
                    family
                        .register_source_qualified(orders, roots, &control)
                        .unwrap();
                }
                // Mode 3 makes exactly the same H lookups/publications as
                // mode 1. Its separate exact table only removes repeated
                // predecessor expansion, never the new edge into a child.
                assert_eq!(
                    ancestors.prefix_cone_metrics(),
                    direct.prefix_cone_metrics()
                );
                assert_eq!(bounded.prefix_cone_metrics(), direct.prefix_cone_metrics());
                assert_eq!(graph(&ancestors), graph(&direct));
                assert_eq!(graph(&bounded), graph(&direct));
            }
            assert!(ancestors.prefix_cone_exact_metrics()[3] > 0);
            assert!(ancestors.metrics.prefix_visits < direct.metrics.prefix_visits);
            assert_eq!(ancestors.prefix_cones.len(), PREFIX_CONE_SLOTS);
            for index in 0..ancestors.exact_cones.len {
                let entry = ancestors.exact_cones.entry(index);
                let depth = entry.cells.iter().filter(|&&cells| cells != 0).count();
                assert!((1..5).contains(&depth));
                assert_eq!(
                    ancestors.nodes[entry.node.physical_node().unwrap() as usize].remaining(),
                    10 - depth as u8
                );
            }
            assert_eq!(bounded.exact_cones.len, 2);
            assert!(bounded.prefix_cone_exact_metrics()[4] > 0);
            assert!(bounded.peak_bytes() >= bounded.retained_bytes());
        }

        #[test]
        fn prefix_cones_exact_cap_keeps_old_certificates_and_direct_h_fallback() {
            let control = ExecutionControl::default();
            let original = fresh_bag_orders();
            let qualified = SourceQualifiedPrefixOrders::new(
                &original,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            let mut kinds = PieceOrderLanguageCache::default();
            let suffixes = suffixes(&qualified, &mut kinds, 4);
            let mut plain = BoardActionFamily::new(plan(false, 4, EdgeClosureMode::Off));
            let mut exact = BoardActionFamily::new(exact_plan(4, EdgeClosureMode::Off));
            exact.exact_cones.limit = 2;
            for family in [&mut plain, &mut exact] {
                family
                    .register_source_qualified(&qualified, &suffixes, &control)
                    .unwrap();
            }
            assert_eq!(graph(&exact), graph(&plain));
            assert_eq!(exact.exact_cones.len, 2);
            assert_eq!(exact.exact_cones.capacity, 2);
            assert!(exact.prefix_cone_exact_metrics()[4] > 0);
            assert_eq!(exact.prefix_cones.len(), PREFIX_CONE_SLOTS);
            let old = *exact.exact_cones.entry(0);
            assert_eq!(
                exact.exact_cone_lookup(&old.cells, &control).unwrap(),
                Some(old.node)
            );
            let before = exact.prefix_cone_metrics();
            exact
                .register_source_qualified(&qualified, &suffixes, &control)
                .unwrap();
            assert!(exact.prefix_cone_metrics()[1] > before[1]);
            assert_eq!(exact.exact_cones.len, 2);
            assert_eq!(graph(&exact), graph(&plain));

            // Allocation refusal is also an optional-cache miss, not an
            // empty language. Existing exact certificates remain readable.
            let mut refused = BoardActionFamily::new(exact_plan(4, EdgeClosureMode::Off));
            refused.exact_cones.allocation_refused = true;
            refused
                .register_source_qualified(&qualified, &suffixes, &control)
                .unwrap();
            assert_eq!(refused.exact_cones.len, 0);
            assert_eq!(refused.prefix_cones.len(), PREFIX_CONE_SLOTS);
            assert_eq!(graph(&refused), graph(&plain));
        }

        #[test]
        fn prefix_cones_exact_growth_accounts_index_work_and_preserves_colliding_keys() {
            let control = ExecutionControl::default();
            let mut family = BoardActionFamily::new(exact_plan(4, EdgeClosureMode::Off));
            family.exact_cones.limit = EXACT_CONE_FIRST_BUCKETS + 1;
            for index in 0..EXACT_CONE_FIRST_BUCKETS {
                let mut cells = [0; 7];
                cells[0] = index as u64 + 1;
                assert!(family
                    .publish_exact_cone(
                        PrefixConeEntry {
                            cells,
                            node: ProducerNode::physical(index as u32)
                        },
                        &control
                    )
                    .unwrap());
            }
            assert_eq!(family.exact_cones.buckets.len(), EXACT_CONE_FIRST_BUCKETS);
            assert!(family.exact_cones.chunks[0]
                .iter()
                .any(|entry| entry.next != NO_NODE));
            let old_links: Vec<_> = family.exact_cones.chunks[0]
                .iter()
                .map(|entry| entry.next)
                .collect();
            let old_buckets = family.exact_cones.buckets.clone();
            let old_work = family.metrics.work;
            let rebuild_work = EXACT_CONE_FIRST_BUCKETS * 9;
            family.metrics.work = MAX_WORK - rebuild_work + 1;
            let next = PrefixConeEntry {
                cells: [EXACT_CONE_FIRST_BUCKETS as u64 + 1, 0, 0, 0, 0, 0, 0],
                node: ProducerNode::physical(EXACT_CONE_FIRST_BUCKETS as u32),
            };
            assert!(matches!(
                family.prepare_exact_cone_capacity(&control),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_physical_work_limit"
                ))
            ));
            assert_eq!(family.exact_cones.buckets, old_buckets);
            assert_eq!(
                family.exact_cones.chunks[0]
                    .iter()
                    .map(|entry| entry.next)
                    .collect::<Vec<_>>(),
                old_links
            );
            assert_eq!(family.exact_cones.len, EXACT_CONE_FIRST_BUCKETS);
            assert!(
                family.peak_bytes()
                    >= family.retained_bytes()
                        + EXACT_CONE_FIRST_BUCKETS * 8 * core::mem::size_of::<u32>()
            );
            // Test-only restoration lets the same immutable old index prove
            // that a later successful grow retains every colliding full key.
            family.metrics.work = old_work;
            assert!(family.publish_exact_cone(next, &control).unwrap());
            assert_eq!(
                family.exact_cones.buckets.len(),
                EXACT_CONE_FIRST_BUCKETS * 8
            );
            assert_eq!(
                family.prefix_cone_exact_metrics()[6],
                EXACT_CONE_FIRST_BUCKETS
            );
            assert_eq!(
                family.prefix_cone_exact_metrics()[7],
                EXACT_CONE_FIRST_BUCKETS * 9
            );
            assert_eq!(family.exact_cones.chunks.len(), 1);
            for index in 0..=EXACT_CONE_FIRST_BUCKETS {
                let mut cells = [0; 7];
                cells[0] = index as u64 + 1;
                family.cone_cache_charge(1, &control).unwrap();
                assert_eq!(
                    family.exact_cone_lookup(&cells, &control).unwrap(),
                    Some(ProducerNode::physical(index as u32))
                );
            }
            assert!(family.peak_bytes() >= family.retained_bytes());
        }

        #[test]
        fn prefix_cones_exact_merge_drops_local_ids_and_failures_publish_no_partial_cone() {
            let control = ExecutionControl::default();
            let original = fresh_bag_orders();
            let qualified = SourceQualifiedPrefixOrders::new(
                &original,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            let mut kinds = PieceOrderLanguageCache::default();
            let suffixes = suffixes(&qualified, &mut kinds, 4);
            let scope = exact_plan(4, EdgeClosureMode::Off);
            let mut worker = BoardActionFamily::new(Arc::clone(&scope));
            worker
                .register_source_qualified(&qualified, &suffixes, &control)
                .unwrap();
            let expected = graph(&worker);
            let snapshot = worker.prefix_cone_exact_metrics();
            let before_publish = worker.metrics.work - snapshot[8];
            let rebound: Vec<_> = (0..kinds.node_count() as u32).collect();
            let mut destination = BoardActionFamily::new(Arc::clone(&scope));
            destination.merge(worker, &rebound, &control).unwrap();
            assert_eq!(destination.prefix_cone_exact_metrics(), snapshot);
            assert_eq!(destination.exact_cones.len, 0);
            assert!(destination.exact_cones.buckets.is_empty());
            assert_eq!(graph(&destination), expected);
            destination
                .register_source_qualified(&qualified, &suffixes, &control)
                .unwrap();
            assert_eq!(destination.prefix_cone_exact_metrics()[3], 0);
            assert_eq!(graph(&destination), expected);

            let mut limited = BoardActionFamily::new(scope);
            limited.metrics.work = MAX_WORK - before_publish;
            assert!(matches!(
                limited.register_source_qualified(&qualified, &suffixes, &control),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_physical_work_limit"
                ))
            ));
            assert!(limited.failed);
            assert_eq!(limited.exact_cones.len, 0);
            assert_eq!(limited.prefix_cone_metrics()[4], 0);
            assert_eq!(limited.metrics.candidate_count, 0);
            let cancelled = ExecutionControl::default();
            cancelled.cancellation.handle().cancel();
            assert!(matches!(
                destination.register_source_qualified(&qualified, &suffixes, &cancelled),
                Err(WasmExactSearchError::Cancelled)
            ));
            assert!(destination.failed);
        }

        #[test]
        fn prefix_cones_unqualified_and_zero_horizon_keep_the_existing_route() {
            let control = ExecutionControl::default();
            let original = fresh_bag_orders();
            let qualified = SourceQualifiedPrefixOrders::new(
                &original,
                &control,
                &mut SourceFilterMetrics::default(),
            )
            .unwrap();
            let mut kinds = PieceOrderLanguageCache::default();
            let suffixes = suffixes(&qualified, &mut kinds, 0);
            let mut zero = BoardActionFamily::new(plan(true, 0, EdgeClosureMode::Off));
            zero.register_source_qualified(&qualified, &suffixes, &control)
                .unwrap();
            assert_eq!(zero.prefix_cone_metrics(), [0; 5]);
            assert!(zero.prefix_cones.is_empty());
            assert_eq!(zero.metrics.nodes, 1);
            let mut expected = None;
            for enabled in [false, true] {
                let mut family = BoardActionFamily::new(plan(enabled, 4, EdgeClosureMode::Off));
                let mut kinds = PieceOrderLanguageCache::default();
                register_orders(&mut family, &mut kinds, fresh_bag_orders());
                assert_eq!(family.prefix_cone_metrics(), [0; 5]);
                assert!(family.prefix_cones.is_empty());
                assert!(family.prefix_mask_metrics[5] > 0);
                let actual = (
                    graph(&family),
                    family.metrics.work,
                    family.prefix_mask_metrics,
                );
                if let Some(expected) = &expected {
                    assert_eq!(&actual, expected);
                } else {
                    expected = Some(actual);
                }
            }
        }
    }

    #[test]
    fn source_qualified_registration_preserves_known_words_and_skips_duplicate_caps() {
        use crate::backend::wasm_cpu::piece_order_language::PrefixSuffixCompilation;
        use PieceKind::{I, J, L, O, S, T, Z};

        let control = ExecutionControl::default();
        // Store J, execute IOTZSL, then swap JZTI. The final filler remains
        // held; either filler is an actual fresh-bag draw. The first seven
        // observations are identical, with distinct hidden continuations.
        let first = vec![J, I, O, T, Z, S, L, Z, T, I, O];
        let second = vec![J, I, O, T, Z, S, L, Z, T, I, S];
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(983),
            PatternWeightModelId::new(984),
            vec![first.clone(), second, first],
            vec![
                ProbabilityValue::new(0.2).unwrap(),
                ProbabilityValue::new(0.3).unwrap(),
                ProbabilityValue::new(0.5).unwrap(),
            ],
            3,
            true,
            None,
        )
        .unwrap();
        let mut expected_coverage = None;
        let mut raw_prefixes: Option<Vec<Vec<(u8, u64)>>> = None;
        for enabled in [false, true] {
            let mut owner = PhysicalActionLanguage::default();
            owner.set_board_plan(source_filter_plan(enabled)).unwrap();
            let mut kinds = PieceOrderLanguageCache::default();
            assert_eq!(
                owner
                    .produce_board_relation(
                        fresh_bag_orders(),
                        &mut kinds,
                        &mut SymbolicPrefixWork::default(),
                        &control,
                        0,
                    )
                    .unwrap(),
                PrefixSuffixCompilation::Compiled
            );
            assert!(owner.prefix_suffix_scratch.is_empty());
            let source_metrics = owner.source_filter_metrics();
            let family = owner.board.as_mut().unwrap();
            if enabled {
                assert_eq!(source_metrics[0], 1);
                assert!(source_metrics[1..].iter().all(|&count| count > 0));
                assert!(family.prefix_masks.is_empty());
                assert_eq!(family.prefix_mask_metrics, [0; 6]);
                assert_eq!(family.metrics().source_pruned_prefixes, 0);
                assert_eq!(family.metrics().dead_source_prefixes, 0);
            } else {
                assert_eq!(source_metrics, [0; 4]);
                assert_eq!(family.prefix_mask_metrics[0], 1);
                assert!(family.prefix_mask_metrics[5] > 0);
            }
            let view = family.view_with_kind_union(&kinds, &control, true).unwrap();
            let prefixes = physical_prefixes(&view, view.root(), 4);
            if let Some(raw) = &raw_prefixes {
                assert!(prefixes
                    .iter()
                    .all(|prefix| raw.binary_search(prefix).is_ok()));
            } else {
                raw_prefixes = Some(prefixes);
            }
            let coverage = QueueObservationPolicyEvaluator::for_original_universe(
                &universe,
                QueueObservationPolicy::VisibleSeven,
                0,
                None,
                true,
                false,
                false,
                None,
            )
            .unwrap()
            .evaluate(&view, &control)
            .unwrap();
            assert_eq!(coverage.covered_pattern_count, 3);
            assert_eq!(coverage.covered_patterns.words()[0], 0b111);
            let result = (
                coverage.covered_patterns.words().to_vec(),
                coverage.covered_weight.to_bits(),
                coverage.min_accepted_depth,
                coverage.max_accepted_depth,
            );
            if let Some(expected) = &expected_coverage {
                assert_eq!(&result, expected);
            } else {
                expected_coverage = Some(result);
            }
        }
    }

    #[test]
    fn source_qualified_worker_merge_retains_metrics_and_filtered_actions() {
        let control = ExecutionControl::default();
        let plan = source_filter_plan(true);
        let mut destination = PhysicalActionLanguage::default();
        destination.set_board_plan(Arc::clone(&plan)).unwrap();
        let mut kinds = PieceOrderLanguageCache::default();
        let mut worker = PhysicalActionLanguage::default();
        worker.set_board_plan(plan).unwrap();
        let mut worker_kinds = PieceOrderLanguageCache::default();
        for (owner, kinds) in [
            (&mut destination, &mut kinds),
            (&mut worker, &mut worker_kinds),
        ] {
            owner
                .produce_board_relation(
                    fresh_bag_orders(),
                    kinds,
                    &mut SymbolicPrefixWork::default(),
                    &control,
                    0,
                )
                .unwrap();
        }
        let before = destination.source_filter_metrics();
        assert_eq!(before, worker.source_filter_metrics());
        let original_nodes = destination.board.as_ref().unwrap().metrics().nodes;
        let original_edges = destination.board.as_ref().unwrap().metrics().edges;
        let imported = worker
            .import_board_suffixes(&worker_kinds, &mut kinds, &control)
            .unwrap()
            .unwrap();
        let rebound = imported.rebound_for(&worker_kinds, &kinds).unwrap();
        destination
            .merge_factored(worker, rebound, &control, &kinds)
            .unwrap();
        assert_eq!(
            destination.source_filter_metrics(),
            before.map(|value| value * 2)
        );
        let family = destination.board.as_mut().unwrap();
        assert_eq!(family.metrics().candidate_count, 2);
        assert_eq!(family.metrics().nodes, original_nodes);
        assert_eq!(family.metrics().edges, original_edges);
        assert_eq!(family.prefix_mask_metrics, [0; 6]);
        let view = family.view(&kinds, &control).unwrap();
        assert!(!physical_prefixes(&view, view.root(), 4).is_empty());
    }

    #[test]
    fn source_filter_plan_preserves_caps_on_the_raw_registration_route() {
        let control = ExecutionControl::default();
        let mut expected = None;
        for enabled in [false, true] {
            let mut family = BoardActionFamily::new(source_filter_plan(enabled));
            let mut kinds = PieceOrderLanguageCache::default();
            // Explicit reference/fallback callers retain the raw route even
            // when this request also owns a source-filter capability.
            register_orders(&mut family, &mut kinds, fresh_bag_orders());
            assert_eq!(family.prefix_mask_metrics[0], 1);
            assert!(family.prefix_mask_metrics[5] > 0);
            let metrics = family.metrics();
            let masks = family.prefix_mask_metrics;
            let view = family.view(&kinds, &control).unwrap();
            let actual = (
                physical_prefixes(&view, view.root(), 4),
                metrics.nodes,
                metrics.edges,
                metrics.suffix_roots,
                metrics.work,
                masks,
            );
            if let Some(expected) = &expected {
                assert_eq!(&actual, expected);
            } else {
                expected = Some(actual);
            }
        }
    }

    #[test]
    fn source_qualified_production_refuses_missing_authority_and_keeps_failed_work() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let mut plan = source_filter_plan(true);
        Arc::get_mut(&mut plan).unwrap().fresh_bag_observation = false;
        let mut missing = PhysicalActionLanguage::default();
        missing.set_board_plan(plan).unwrap();
        assert!(matches!(
            missing.produce_board_relation(
                fresh_bag_orders(),
                &mut kinds,
                &mut SymbolicPrefixWork::default(),
                &control,
                0,
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_source_filter_authority_missing"
            ))
        ));
        assert_eq!(missing.source_filter_metrics(), [0; 4]);
        assert!(!missing.has_roots());

        let mut owner = PhysicalActionLanguage::default();
        owner.set_board_plan(source_filter_plan(true)).unwrap();
        owner.board.as_mut().unwrap().metrics.work = MAX_WORK;
        assert!(matches!(
            owner.produce_board_relation(
                fresh_bag_orders(),
                &mut kinds,
                &mut SymbolicPrefixWork::default(),
                &control,
                0,
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        let metrics = owner.source_filter_metrics();
        assert_eq!(metrics[0], 1);
        assert!(metrics[1..].iter().all(|&count| count > 0));
        assert!(owner.prefix_suffix_scratch.is_empty());
        let family = owner.board.as_mut().unwrap();
        assert_eq!(family.metrics().work, MAX_WORK);
        assert!(family.failed);
        assert!(family.view(&kinds, &control).is_err());
    }

    #[test]
    fn boundary_suffix_compilation_matches_full_production_at_every_horizon() {
        use crate::backend::wasm_cpu::piece_order_language::PrefixSuffixCompilation;

        let control = ExecutionControl::default();
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(981),
            PatternWeightModelId::new(982),
            vec![
                vec![PieceKind::O, PieceKind::O],
                vec![PieceKind::I, PieceKind::I],
                vec![PieceKind::O, PieceKind::O],
            ],
            vec![
                ProbabilityValue::new(0.2).unwrap(),
                ProbabilityValue::new(0.3).unwrap(),
                ProbabilityValue::new(0.5).unwrap(),
            ],
            3,
            true,
            None,
        )
        .unwrap();
        for horizon in 0..=2 {
            let mut plan = edge_mode(
                boundary_mode(
                    BoardObservationPlan::new(10, 2, initial(), 2, horizon, 0, None).unwrap(),
                    true,
                ),
                EdgeClosureMode::Off,
            );
            // This fixture tests preparation/transfer, not issuance of the
            // fresh-bag theorem capability by the production request factory.
            Arc::get_mut(&mut plan)
                .unwrap()
                .enable_fresh_bag_observation();
            let mut expected = None;
            for suffix_only in [false, true] {
                let mut owner = PhysicalActionLanguage::default();
                owner.set_board_plan(Arc::clone(&plan)).unwrap();
                assert!(owner.board_fresh_bag_observation_enabled());
                let mut kinds = PieceOrderLanguageCache::default();
                let orders = two_os();
                let mut work = SymbolicPrefixWork::default();
                if suffix_only {
                    assert_eq!(
                        owner
                            .prepare_board_suffixes(&orders, &mut kinds, &mut work, &control)
                            .unwrap(),
                        PrefixSuffixCompilation::Compiled
                    );
                    assert_eq!(owner.prefix_suffix_scratch.len(), 4);
                    assert_eq!(owner.prefix_suffix_scratch[0] == NO_NODE, horizon > 0);
                    assert_eq!(kinds.node_count(), 3 - usize::from(horizon));
                    owner
                        .register_prepared_board_relation(orders, &control)
                        .unwrap();
                    assert!(owner.prefix_suffix_scratch.is_empty());
                } else {
                    assert!(owner
                        .compile_relation_language(&orders, &mut kinds, &mut work, &control)
                        .unwrap()
                        .is_some());
                    owner
                        .register_relation_with_reveal_floor(orders, horizon, 0, &control)
                        .unwrap();
                }
                let view = owner
                    .board
                    .as_mut()
                    .unwrap()
                    .view_with_kind_union(&kinds, &control, true)
                    .unwrap();
                let coverage = QueueObservationPolicyEvaluator::for_original_universe(
                    &universe,
                    QueueObservationPolicy::VisibleSeven,
                    0,
                    None,
                    false,
                    false,
                    false,
                    None,
                )
                .unwrap()
                .evaluate(&view, &control)
                .unwrap();
                assert_eq!(coverage.covered_pattern_count, 2);
                assert_eq!(coverage.covered_patterns.words()[0], 0b101);
                let actual = (
                    physical_prefixes(&view, view.root(), horizon),
                    coverage.covered_patterns.words().to_vec(),
                    coverage.covered_weight.to_bits(),
                    coverage.min_accepted_depth,
                    coverage.max_accepted_depth,
                    view.metrics().work,
                );
                if let Some(expected) = &expected {
                    assert_eq!(&actual, expected);
                } else {
                    expected = Some(actual);
                }
            }
        }
    }

    #[test]
    fn board_suffix_preparation_checks_scope_phase_and_pinned_arena() {
        let control = ExecutionControl::default();
        let orders = two_os();
        let mut kinds = PieceOrderLanguageCache::default();
        let mut work = SymbolicPrefixWork::default();
        let mut missing = PhysicalActionLanguage::default();
        assert!(matches!(
            missing.prepare_board_suffixes(&orders, &mut kinds, &mut work, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_board_plan_missing"
            ))
        ));
        for (total, boundary, expected) in [
            (3, true, "wasm_observation_board_candidate_scope_changed"),
            (
                2,
                false,
                "wasm_observation_board_boundary_suffixes_required",
            ),
        ] {
            let plan = boundary_mode(
                BoardObservationPlan::new(10, 2, initial(), total, 1, 0, None).unwrap(),
                boundary,
            );
            let mut owner = PhysicalActionLanguage::default();
            owner.set_board_plan(plan).unwrap();
            owner.prefix_suffix_scratch.push(17);
            assert!(matches!(
                owner.prepare_board_suffixes(&orders, &mut kinds, &mut work, &control),
                Err(WasmExactSearchError::InvalidProblem(reason)) if reason == expected
            ));
            assert!(owner.prefix_suffix_scratch.is_empty());
        }

        let mut plan = edge_mode(
            boundary_mode(
                BoardObservationPlan::new(10, 2, initial(), 2, 1, 0, None).unwrap(),
                true,
            ),
            EdgeClosureMode::Off,
        );
        Arc::get_mut(&mut plan)
            .unwrap()
            .enable_fresh_bag_observation();
        let mut owner = PhysicalActionLanguage::default();
        owner.set_board_plan(plan).unwrap();
        owner
            .prepare_board_suffixes(&orders, &mut kinds, &mut work, &control)
            .unwrap();
        owner
            .register_prepared_board_relation(orders, &control)
            .unwrap();
        let mut replacement = PieceOrderLanguageCache::default();
        let orders = two_os();
        assert!(matches!(
            owner.prepare_board_suffixes(&orders, &mut replacement, &mut work, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_arena_changed"
            ))
        ));
        assert!(owner.prefix_suffix_scratch.is_empty());
        assert_eq!(replacement.node_count(), 0);
        owner
            .board
            .as_mut()
            .unwrap()
            .view(&kinds, &control)
            .unwrap();
        assert!(matches!(
            owner.prepare_board_suffixes(&orders, &mut kinds, &mut work, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_board_family_frozen"
            ))
        ));
        assert!(owner.prefix_suffix_scratch.is_empty());
    }

    #[test]
    fn prepared_board_registration_requires_capability_and_consumes_attempts() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let mut work = SymbolicPrefixWork::default();
        for enabled in [false, true] {
            let mut plan = boundary_mode(
                BoardObservationPlan::new(10, 2, initial(), 2, 1, 0, None).unwrap(),
                true,
            );
            assert!(!plan.fresh_bag_observation());
            if enabled {
                Arc::get_mut(&mut plan)
                    .unwrap()
                    .enable_fresh_bag_observation();
            }
            let mut owner = PhysicalActionLanguage::default();
            owner.set_board_plan(plan).unwrap();
            assert_eq!(owner.board_fresh_bag_observation_enabled(), enabled);
            assert!(owner
                .register_prepared_board_relation(two_os(), &control)
                .is_err());
            let orders = two_os();
            owner
                .prepare_board_suffixes(&orders, &mut kinds, &mut work, &control)
                .unwrap();
            let result = owner.register_prepared_board_relation(orders, &control);
            if enabled {
                result.unwrap();
                assert!(owner.has_board_roots());
            } else {
                assert!(matches!(
                    result,
                    Err(WasmExactSearchError::InvalidProblem(
                        "wasm_observation_fresh_bag_authority_missing"
                    ))
                ));
                assert!(!owner.has_roots());
            }
            assert!(owner.prefix_suffix_scratch.is_empty());
            assert!(owner
                .register_prepared_board_relation(two_os(), &control)
                .is_err());
        }

        let plan = boundary_mode(
            BoardObservationPlan::new(10, 2, initial(), 2, 1, 0, None).unwrap(),
            true,
        );
        let mut owner = PhysicalActionLanguage::default();
        owner.set_board_plan(plan).unwrap();
        owner
            .prepare_board_suffixes(&two_os(), &mut kinds, &mut work, &control)
            .unwrap();
        let before = kinds.node_count();
        control.cancellation.handle().cancel();
        assert!(matches!(
            owner.prepare_board_suffixes(&two_os(), &mut kinds, &mut work, &control),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert!(owner.prefix_suffix_scratch.is_empty());
        assert_eq!(kinds.node_count(), before);
        assert!(!owner.has_roots());
    }

    #[test]
    fn global_edge_closure_reuses_retained_boards_and_preserves_physical_actions() {
        let control = ExecutionControl::default();
        let required = ((1_u64 << 20) - 1) & !initial();
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 2, initial(), required)
                .unwrap();
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::NoKick,
        ] {
            let mut expected = None;
            for closure in [
                EdgeClosureMode::Off,
                EdgeClosureMode::Full,
                EdgeClosureMode::BeforeLastPhysicalLayer,
            ] {
                let plan = edge_mode(
                    BoardObservationPlan::new(10, 2, initial(), 2, 2, 0, None).unwrap(),
                    closure,
                );
                let mut family = BoardActionFamily::new(plan);
                let mut kinds = PieceOrderLanguageCache::default();
                for _ in 0..8 {
                    register(&mut family, &mut kinds);
                }
                let (registered_edges, registered_attempts) = match closure {
                    EdgeClosureMode::Off => (4, 32),
                    EdgeClosureMode::Full => (0, 0),
                    EdgeClosureMode::BeforeLastPhysicalLayer => (2, 16),
                };
                assert_eq!(family.metrics().edges, registered_edges);
                assert_eq!(family.metrics().edge_attempts, registered_attempts);
                let mut reachability = ReachabilityWorkspace::default();
                reachability.configure_kick_profile(profile, false);
                let view = family
                    .view_with_reachability(&kinds, &catalog, &mut reachability, &control)
                    .unwrap();
                let paths = physical_paths(&view, view.root());
                assert_eq!(paths.len(), 2);
                if let Some(expected) = &expected {
                    assert_eq!(&paths, expected);
                } else {
                    expected = Some(paths);
                }
                if closure != EdgeClosureMode::Off {
                    let (rebuilt_edges, rebuilt_parents) = match closure {
                        EdgeClosureMode::Full => (4, 3),
                        EdgeClosureMode::BeforeLastPhysicalLayer => (2, 1),
                        EdgeClosureMode::Off => unreachable!(),
                    };
                    let metrics = view.edge_closure_metrics();
                    assert!(metrics[0] > 0);
                    assert_eq!(&metrics[1..], &[rebuilt_edges, 0, 0, rebuilt_parents]);
                    assert_eq!(
                        view.metrics().edge_attempts,
                        registered_attempts + rebuilt_edges
                    );
                    assert_eq!(view.metrics().edges, 4);
                    let work = view.metrics().work;
                    drop(view);
                    assert!(family.peak_bytes() >= family.retained_bytes());
                    let repeated = family
                        .view_with_reachability(&kinds, &catalog, &mut reachability, &control)
                        .unwrap();
                    assert_eq!(repeated.edge_closure_metrics(), metrics);
                    assert_eq!(repeated.metrics().work, work);
                }
            }
        }
    }

    #[test]
    fn hybrid_closure_preserves_merged_last_layer_and_requires_finalization() {
        let control = ExecutionControl::default();
        let plan = edge_mode(
            BoardObservationPlan::new(10, 2, initial(), 2, 2, 0, None).unwrap(),
            EdgeClosureMode::BeforeLastPhysicalLayer,
        );
        let required = ((1_u64 << 20) - 1) & !initial();
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 2, initial(), required)
                .unwrap();
        let mut kinds = PieceOrderLanguageCache::default();
        let mut blocked = BoardActionFamily::new(Arc::clone(&plan));
        register(&mut blocked, &mut kinds);
        assert!(matches!(
            blocked.view(&kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_board_edges_not_finalized"
            ))
        ));
        assert!(blocked.failed);

        let mut family = BoardActionFamily::new(Arc::clone(&plan));
        register(&mut family, &mut kinds);
        let mut worker = BoardActionFamily::new(plan);
        let mut worker_kinds = PieceOrderLanguageCache::default();
        register(&mut worker, &mut worker_kinds);
        let rebound = kinds.import_node_map(&worker_kinds).unwrap();
        family.merge(worker, &rebound, &control).unwrap();
        assert_eq!(family.metrics().candidate_count, 2);
        assert_eq!(family.metrics().edges, 2);
        assert_eq!(family.metrics().edge_attempts, 4);
        assert!(family.nodes[family.root.unwrap() as usize].edges.is_empty());
        let last_edges: Vec<_> = family
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.remaining() == 1)
            .map(|(index, node)| (index, node.edges.clone()))
            .collect();
        assert_eq!(last_edges.len(), 2);

        let mut reachability = ReachabilityWorkspace::default();
        reachability.configure_kick_profile(KickTableProfileId::SrsPlus, false);
        let view = family
            .view_with_reachability(&kinds, &catalog, &mut reachability, &control)
            .unwrap();
        assert_eq!(physical_paths(&view, view.root()).len(), 2);
        assert_eq!(view.metrics().edge_attempts, 6);
        assert_eq!(view.metrics().edges, 4);
        assert_eq!(&view.edge_closure_metrics()[1..], &[2, 0, 0, 1]);
        drop(view);
        for (index, edges) in last_edges {
            assert_eq!(family.nodes[index].edges, edges);
        }
        assert!(family.frozen);
        assert!(family.edge_closure_complete);
        assert!(family.peak_bytes() >= family.retained_bytes());
    }

    #[test]
    fn hybrid_closure_has_no_reconstruction_at_zero_or_one_hidden_lock() {
        let control = ExecutionControl::default();
        let required = ((1_u64 << 20) - 1) & !initial();
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 2, initial(), required)
                .unwrap();
        for horizon in [0, 1] {
            let mut expected = None;
            for closure in [
                EdgeClosureMode::Off,
                EdgeClosureMode::BeforeLastPhysicalLayer,
            ] {
                let plan = edge_mode(
                    BoardObservationPlan::new(10, 2, initial(), 2, horizon, horizon, None).unwrap(),
                    closure,
                );
                let mut family = BoardActionFamily::new(plan);
                let mut kinds = PieceOrderLanguageCache::default();
                register(&mut family, &mut kinds);
                let registered_edges = family.metrics().edges;
                assert_eq!(registered_edges, usize::from(horizon) * 2);
                let mut reachability = ReachabilityWorkspace::default();
                let view = family
                    .view_with_reachability(&kinds, &catalog, &mut reachability, &control)
                    .unwrap();
                let paths = physical_prefixes(&view, view.root(), horizon);
                if let Some(expected) = &expected {
                    assert_eq!(&paths, expected);
                } else {
                    expected = Some(paths);
                }
                assert_eq!(view.edge_closure_metrics(), [0; 5]);
                assert_eq!(view.metrics().edges, registered_edges);
                assert_eq!(view.metrics().edge_attempts, registered_edges);
                let frontier = if horizon == 0 {
                    view.root()
                } else {
                    view.edge(view.root(), 0).unwrap().1
                };
                let revealed = view.fully_revealed_node(frontier, &control).unwrap();
                assert!(!view.has_physical_actions(revealed));
                assert_eq!(view.node(revealed).unwrap().depth, horizon);
                drop(view);
                assert!(!family.closure_locks_ready);
                assert_eq!(family.closure_pose_capacity_bytes, 0);
                assert!(family.frozen);
            }
        }
    }

    #[test]
    fn board_supply_aliases_preserve_unfiltered_reveal_and_explicit_reject_depth() {
        use PieceKind::{I, O};
        let control = ExecutionControl::default();
        let plan = BoardObservationPlan::new(10, 2, initial(), 2, 0, 0, None).unwrap();
        let mut family = BoardActionFamily::new(plan);
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut family, &mut kinds);
        register_orders(&mut family, &mut kinds, two_is());
        let mut view = family.view_with_kind_union(&kinds, &control, true).unwrap();
        view.supply_multiset_enabled = true;
        let root = view.root();
        assert!(view.wants_revealed_supply(root));
        let only_i = view
            .fully_revealed_node_with_supply(root, [2, 0, 0, 0, 0, 0, 0], &control)
            .unwrap();
        let only_o = view
            .fully_revealed_node_with_supply(root, [0, 2, 0, 0, 0, 0, 0], &control)
            .unwrap();
        let reject = view
            .fully_revealed_node_with_supply(root, [1, 1, 0, 0, 0, 0, 0], &control)
            .unwrap();
        assert_ne!(only_i, only_o);
        assert!(is_kind_union(reject));
        assert_eq!(
            view.node(reject),
            Some(ObservationLanguageNode {
                accepting: false,
                depth: 0
            })
        );
        assert_eq!(view.edge_count(reject), Some(0));
        view.prepare_edges(reject, 0x7f, &control).unwrap();
        for filtered in [only_i, only_o, reject] {
            assert!(!view.wants_revealed_supply(filtered));
            assert_eq!(
                view.fully_revealed_node(filtered, &control).unwrap(),
                filtered
            );
            assert_eq!(view.node(filtered).unwrap().depth, 0);
        }
        assert_eq!(view.supply_multiset_metrics()[4], 3);
        let unfiltered = view.fully_revealed_node(root, &control).unwrap();
        assert_ne!(unfiltered, reject);
        view.prepare_edges(unfiltered, 0x7f, &control).unwrap();
        assert_eq!(view.edge_count(unfiltered), Some(2));
        assert_eq!(
            view.fully_revealed_node_with_supply(root, [16, 0, 0, 0, 0, 0, 0], &control)
                .unwrap(),
            unfiltered
        );
        assert_eq!(view.supply_multiset_metrics()[4], 3);

        let source = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(961),
            PatternWeightModelId::new(962),
            vec![vec![I, O]],
            vec![ProbabilityValue::new(1.0).unwrap()],
            1,
            true,
            None,
        )
        .unwrap();
        let coverage = QueueObservationPolicyEvaluator::for_original_universe(
            &source,
            QueueObservationPolicy::VisibleSeven,
            0,
            None,
            false,
            false,
            false,
            None,
        )
        .unwrap()
        .evaluate(&view, &control)
        .unwrap();
        assert_eq!(coverage.covered_pattern_count, 0);
        assert_eq!(coverage.covered_weight, 0.0);
        assert!(coverage
            .covered_patterns
            .words()
            .iter()
            .all(|word| *word == 0));
        assert_eq!(coverage.min_accepted_depth, None);
        assert_eq!(coverage.max_accepted_depth, None);
        assert_eq!(coverage.metrics.collection[1], 0);
        assert!(view.peak_bytes() >= view.retained_bytes());
    }

    #[test]
    fn board_supply_filter_on_off_preserves_original_coverage_and_alternative_fallback() {
        use PieceKind::{I, O};
        let control = ExecutionControl::default();
        let plan = BoardObservationPlan::new(10, 2, initial(), 2, 0, 0, None).unwrap();
        let mut family = BoardActionFamily::new(plan);
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut family, &mut kinds);
        register_orders(&mut family, &mut kinds, two_is());
        let source = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(963),
            PatternWeightModelId::new(964),
            vec![vec![I, I], vec![O, O], vec![I, O], vec![O, O]],
            vec![ProbabilityValue::new(0.25).unwrap(); 4],
            4,
            true,
            None,
        )
        .unwrap();
        let mut baseline = None;
        for (enabled, grouped) in [(false, false), (true, false), (true, true)] {
            let mut view = family.view_with_kind_union(&kinds, &control, true).unwrap();
            view.supply_multiset_enabled = enabled;
            view.union
                .borrow_mut()
                .arena
                .set_multiset_grouping_enabled(grouped);
            let coverage = QueueObservationPolicyEvaluator::for_original_universe(
                &source,
                QueueObservationPolicy::VisibleSeven,
                0,
                None,
                false,
                false,
                false,
                None,
            )
            .unwrap()
            .evaluate(&view, &control)
            .unwrap();
            let summary = (
                coverage.covered_patterns.words().to_vec(),
                coverage.covered_pattern_count,
                coverage.covered_weight.to_bits(),
                coverage.min_accepted_depth,
                coverage.max_accepted_depth,
            );
            assert_eq!(coverage.covered_pattern_count, 3);
            assert_eq!(coverage.covered_patterns.words()[0], 0b1011);
            if let Some(expected) = &baseline {
                assert_eq!(&summary, expected);
                if grouped {
                    assert!(view.multiset_group_metrics()[0] > 0);
                    assert!(view.multiset_group_metrics()[7] > 0);
                    assert!(view.peak_bytes() >= view.retained_bytes());
                } else {
                    assert!(view.supply_multiset_metrics()[1] > 0);
                    assert_eq!(view.multiset_group_metrics(), [0; 9]);
                }
            } else {
                baseline = Some(summary);
                assert_eq!(view.supply_multiset_metrics(), [0; 5]);
                assert_eq!(view.multiset_group_metrics(), [0; 9]);
            }
        }

        let mut alternatives = family
            .view_with_kind_union(&kinds, &control, false)
            .unwrap();
        alternatives.supply_multiset_enabled = true;
        let root = alternatives.root();
        assert!(!alternatives.wants_revealed_supply(root));
        let revealed = alternatives
            .fully_revealed_node_with_supply(root, [1, 1, 0, 0, 0, 0, 0], &control)
            .unwrap();
        assert_eq!(revealed, root | REVEALED_BOARD_TAG);
        assert_eq!(
            alternatives
                .revealed_alternatives(revealed, &control)
                .unwrap()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(alternatives.supply_multiset_metrics(), [0; 5]);
    }

    #[test]
    fn boundary_only_suffixes_preserve_early_reveal_coverage_and_hold_release() {
        use PieceKind::{I, O, T};
        let control = ExecutionControl::default();
        let mut held_limits = [1; 7];
        held_limits[1] = 2;
        // Each small original universe satisfies its supplied prefix cap.
        // Production issuance additionally requires the full fresh-bag gate.
        for (held, hold_enabled, limits, sequences, expected_count) in [
            (
                None,
                false,
                Some([1; 7]),
                vec![vec![O, I, O], vec![I, O, O], vec![I, T, O]],
                2,
            ),
            (
                None,
                true,
                None,
                vec![vec![O, O, I], vec![O, I, O], vec![I, O, O], vec![I, T, O]],
                3,
            ),
            (
                Some(O),
                true,
                Some(held_limits),
                vec![vec![O, I], vec![I, O], vec![I, T]],
                2,
            ),
        ] {
            let count = sequences.len();
            let universe = MaterializedPatternUniverse::from_sequences(
                PatternUniverseId::new(946),
                PatternWeightModelId::new(947),
                sequences,
                vec![ProbabilityValue::new(1.0 / count as f64).unwrap(); count],
                count as u128,
                true,
                None,
            )
            .unwrap();
            let mut expected = None;
            let mut suffix_counts = [0; 2];
            for boundary_only in [false, true] {
                let (initial, orders) = two_os_and_i();
                let plan = boundary_mode(
                    BoardObservationPlan::new(10, 2, initial, 3, 2, 0, limits).unwrap(),
                    boundary_only,
                );
                let mut family = BoardActionFamily::new(plan);
                let mut kinds = PieceOrderLanguageCache::default();
                register_orders(&mut family, &mut kinds, orders);
                suffix_counts[usize::from(boundary_only)] = family.metrics().suffix_roots;
                for supply_filter in [false, true] {
                    let mut view = family.view_with_kind_union(&kinds, &control, true).unwrap();
                    view.supply_multiset_enabled = supply_filter;
                    let revealed = view.fully_revealed_node(view.root(), &control).unwrap();
                    assert_eq!(revealed == view.root(), boundary_only);
                    assert_eq!(view.has_physical_actions(revealed), boundary_only);
                    let coverage = QueueObservationPolicyEvaluator::for_original_universe(
                        &universe,
                        QueueObservationPolicy::VisibleSeven,
                        0,
                        held,
                        hold_enabled,
                        held.is_some(),
                        false,
                        None,
                    )
                    .unwrap()
                    .evaluate(&view, &control)
                    .unwrap();
                    assert_eq!(coverage.covered_pattern_count, expected_count);
                    let result = (
                        coverage.covered_patterns.words().to_vec(),
                        coverage.covered_weight.to_bits(),
                    );
                    if let Some(expected) = &expected {
                        assert_eq!(&result, expected);
                    } else {
                        expected = Some(result);
                    }
                }
            }
            assert!(suffix_counts[1] < suffix_counts[0]);
        }
    }

    #[test]
    fn boundary_only_suffixes_keep_zero_horizon_and_terminal_acceptance() {
        let control = ExecutionControl::default();
        for horizon in [0, 2] {
            let plan = boundary_mode(
                BoardObservationPlan::new(10, 2, initial(), 2, horizon, 0, None).unwrap(),
                true,
            );
            let mut family = BoardActionFamily::new(plan);
            let mut kinds = PieceOrderLanguageCache::default();
            register(&mut family, &mut kinds);
            assert_eq!(family.metrics().suffix_roots, 1);
            let view = family.view_with_kind_union(&kinds, &control, true).unwrap();
            let root = view.root();
            let revealed = view.fully_revealed_node(root, &control).unwrap();
            assert_eq!(revealed == root, horizon != 0);
            let (_, child) = view.edge(revealed, 0).unwrap();
            let child = view.fully_revealed_node(child, &control).unwrap();
            let (_, terminal) = view.edge(child, 0).unwrap();
            assert!(view.node(terminal).unwrap().accepting);
            assert_eq!(
                view.fully_revealed_node(terminal, &control).unwrap(),
                terminal
            );
        }
    }

    #[test]
    fn source_prefix_caps_preserve_exactly_the_admissible_physical_prefixes() {
        let control = ExecutionControl::default();
        let (initial, orders) = two_os_and_i();
        let plain_plan = BoardObservationPlan::new(10, 2, initial, 3, 2, 2, None).unwrap();
        let capped_plan = BoardObservationPlan::new(10, 2, initial, 3, 2, 2, Some([1; 7])).unwrap();
        let mut kinds = PieceOrderLanguageCache::default();
        let mut plain = BoardActionFamily::new(plain_plan);
        register_orders(&mut plain, &mut kinds, orders);
        let view = plain.view(&kinds, &control).unwrap();
        let mut expected = physical_prefixes(&view, view.root(), 2);
        let original_count = expected.len();
        expected.retain(|prefix| prefix[0].0 != prefix[1].0);
        assert!(original_count > expected.len());
        assert!(!expected.is_empty());
        drop(view);

        let mut capped = BoardActionFamily::new(Arc::clone(&capped_plan));
        register_orders(&mut capped, &mut kinds, two_os_and_i().1);
        assert_eq!(capped.metrics().source_pruned_prefixes, 1);
        assert_eq!(capped.metrics().dead_source_prefixes, 0);
        assert_eq!(
            capped.metrics().prefix_visits,
            plain.metrics().prefix_visits
        );
        assert!(capped.metrics().edges < plain.metrics().edges);
        assert!(capped.metrics().suffix_roots < plain.metrics().suffix_roots);

        let mut worker_kinds = PieceOrderLanguageCache::default();
        let mut worker = BoardActionFamily::new(capped_plan);
        register_orders(&mut worker, &mut worker_kinds, two_os_and_i().1);
        let rebound = kinds.import_node_map(&worker_kinds).unwrap();
        capped.merge(worker, &rebound, &control).unwrap();
        assert_eq!(capped.metrics().source_pruned_prefixes, 2);
        assert_eq!(capped.metrics().candidate_count, 2);
        let view = capped.view(&kinds, &control).unwrap();
        assert_eq!(physical_prefixes(&view, view.root(), 2), expected);
    }

    #[test]
    fn prefix_word_masks_preserve_merged_physical_prefixes_and_charge_actual_work() {
        let control = ExecutionControl::default();
        let mut snapshots = Vec::new();
        for enabled in [false, true] {
            let initial = two_os_and_i().0;
            let mut plan =
                BoardObservationPlan::new(10, 2, initial, 3, 2, 2, Some([1; 7])).unwrap();
            Arc::get_mut(&mut plan).unwrap().prefix_masks_enabled = enabled;
            let mut family = BoardActionFamily::new(Arc::clone(&plan));
            let mut kinds = PieceOrderLanguageCache::default();
            register_orders(&mut family, &mut kinds, two_os_and_i().1);
            register_orders(&mut family, &mut kinds, two_os_and_i().1);
            let mut worker = BoardActionFamily::new(plan);
            let mut worker_kinds = PieceOrderLanguageCache::default();
            register_orders(&mut worker, &mut worker_kinds, two_os_and_i().1);
            let rebound = kinds.import_node_map(&worker_kinds).unwrap();
            family.merge(worker, &rebound, &control).unwrap();
            assert!(family.peak_bytes() >= family.retained_bytes());
            let masks = family.prefix_mask_metrics;
            if enabled {
                assert_eq!(&masks[..4], &[3, 1, 2, 0]);
                assert_eq!(masks[4], 3);
                assert!(masks[5] > masks[4]);
            } else {
                assert_eq!(masks, [0; 6]);
            }
            let view = family.view(&kinds, &control).unwrap();
            snapshots.push((
                physical_prefixes(&view, view.root(), 2),
                view.metrics(),
                masks,
            ));
        }
        let (scalar_paths, scalar, _) = &snapshots[0];
        let (masked_paths, masked, extra) = &snapshots[1];
        assert_eq!(scalar_paths, masked_paths);
        assert_eq!(scalar.nodes, masked.nodes);
        assert_eq!(scalar.edges, masked.edges);
        assert_eq!(scalar.suffix_roots, masked.suffix_roots);
        assert_eq!(scalar.source_pruned_prefixes, masked.source_pruned_prefixes);
        assert_eq!(scalar.dead_source_prefixes, masked.dead_source_prefixes);
        let skipped = scalar.prefix_visits - masked.prefix_visits;
        assert_eq!(skipped, 3);
        assert_eq!(masked.work + skipped, scalar.work + extra[5]);
    }

    #[test]
    fn prefix_mask_budget_failure_poisoning_cannot_report_an_empty_family() {
        let control = ExecutionControl::default();
        let (initial, orders) = two_os_and_i();
        let mut plan = BoardObservationPlan::new(10, 2, initial, 3, 2, 2, Some([1; 7])).unwrap();
        Arc::get_mut(&mut plan).unwrap().prefix_masks_enabled = true;
        let mut family = BoardActionFamily::new(plan);
        let mut kinds = PieceOrderLanguageCache::default();
        let mut suffixes = Vec::new();
        orders
            .compile_product_language_with_residuals(
                &mut kinds,
                &mut SymbolicPrefixWork::default(),
                &mut suffixes,
            )
            .unwrap();
        family.metrics.work = MAX_WORK - 1;
        assert!(matches!(
            family.register(orders, &suffixes, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        assert_eq!(family.metrics.work, MAX_WORK);
        assert!(family.failed);
        assert!(!family.has_roots());
        assert!(family.view(&kinds, &control).is_err());
    }

    #[test]
    fn source_dead_candidates_leave_no_orphans_and_held_allowance_restores_paths() {
        let control = ExecutionControl::default();
        let plan = BoardObservationPlan::new(10, 2, initial(), 2, 2, 0, Some([1; 7])).unwrap();
        let mut family = BoardActionFamily::new(Arc::clone(&plan));
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut family, &mut kinds);
        assert!(!family.has_roots());
        assert!(family.nodes.is_empty());
        assert!(family.index.is_empty());
        assert!(family
            .prefixes
            .iter()
            .all(|&node| node == ProducerNode::MISSING));
        assert_eq!(family.metrics().source_pruned_prefixes, 1);
        assert_eq!(family.metrics().dead_source_prefixes, 3);
        assert_eq!(family.metrics().prefix_visits, 4);
        assert_eq!(family.metrics().edge_attempts, 0);
        assert_eq!(family.metrics().suffix_roots, 0);
        assert!(
            kinds.node_count() > 0,
            "the oracle language was still compiled"
        );

        let mut worker = BoardActionFamily::new(plan);
        let mut worker_kinds = PieceOrderLanguageCache::default();
        register(&mut worker, &mut worker_kinds);
        let rebound = kinds.import_node_map(&worker_kinds).unwrap();
        family.merge(worker, &rebound, &control).unwrap();
        assert!(!family.has_roots());
        assert!(family.nodes.is_empty());
        assert_eq!(family.metrics().source_pruned_prefixes, 2);
        assert_eq!(family.metrics().dead_source_prefixes, 6);

        let mut limits = [1; 7];
        limits[1] = 2;
        let plan = BoardObservationPlan::new(10, 2, initial(), 2, 2, 0, Some(limits)).unwrap();
        let mut allowed = BoardActionFamily::new(plan);
        register(&mut allowed, &mut kinds);
        assert_eq!(allowed.metrics().source_pruned_prefixes, 0);
        assert_eq!(allowed.metrics().dead_source_prefixes, 0);
        let view = allowed.view(&kinds, &control).unwrap();
        assert_eq!(physical_paths(&view, view.root()).len(), 2);
    }

    #[test]
    fn repeated_board_proofs_share_states_and_preserve_eager_physical_paths() {
        let control = ExecutionControl::default();
        let plan = boundary_mode(
            BoardObservationPlan::new(10, 2, initial(), 2, 2, 0, None).unwrap(),
            false,
        );
        let mut family = BoardActionFamily::new(plan);
        assert!(!family.has_roots());
        let mut kinds = PieceOrderLanguageCache::default();
        for _ in 0..32 {
            register(&mut family, &mut kinds);
        }
        assert_eq!(family.metrics().candidate_count, 32);
        assert_eq!(family.metrics().nodes, 4);
        assert_eq!(family.metrics().edges, 4);
        assert_eq!(family.metrics().suffix_roots, 4);
        assert_eq!(family.metrics().prefix_visits, 32 * 4);
        assert_eq!(family.metrics().edge_attempts, 32 * 4);
        assert_eq!(
            family.edge_capacity_bytes,
            family
                .nodes
                .iter()
                .map(|node| { node.edges.capacity() * core::mem::size_of::<BoardEdge>() })
                .sum::<usize>()
        );
        assert_eq!(
            family.suffix_capacity_bytes,
            family
                .nodes
                .iter()
                .map(|node| { node.suffixes.retained_bytes() })
                .sum::<usize>()
        );
        assert!(family.peak_bytes() >= family.retained_bytes());

        let mut eager = PhysicalActionLanguage::default();
        two_os()
            .compile_observation_language(&mut eager, &control)
            .unwrap();
        let eager_view = eager.view(&control).unwrap().unwrap();
        let view = family.view(&kinds, &control).unwrap();
        let paths = physical_paths(&view, view.root());
        assert_eq!(paths.len(), 2);
        assert_eq!(paths, physical_paths(&eager_view, eager_view.root()));
        assert!(
            view.physical_action_cells(view.root(), 0).unwrap()
                < view.physical_action_cells(view.root(), 1).unwrap()
        );
        let frozen_work = view.metrics().work;
        drop(view);
        assert_eq!(
            family.view(&kinds, &control).unwrap().metrics().work,
            frozen_work
        );

        let mut roots = SuffixRoots::default();
        for root in (0..96).rev() {
            assert!(roots.insert(root, true).unwrap().0);
        }
        assert!(matches!(roots, SuffixRoots::Large(_)));
        assert_eq!(roots.insert(7, false).unwrap(), (false, 0));
        assert!(roots.freeze().unwrap() > 0);
        assert_eq!(roots.frozen().unwrap(), (0..96).collect::<Vec<_>>());
    }

    #[test]
    fn board_boundary_requires_reveal_and_reads_only_pinned_kind_suffixes() {
        let control = ExecutionControl::default();
        let plan = BoardObservationPlan::new(10, 2, initial(), 2, 1, 1, None).unwrap();
        let mut family = BoardActionFamily::new(plan);
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut family, &mut kinds);
        assert_eq!(
            family.nodes[family.root.unwrap() as usize].suffixes.len(),
            0
        );
        assert_eq!(family.metrics().suffix_roots, 2);
        let view = family
            .view_with_kind_union(&kinds, &control, false)
            .unwrap();
        assert_eq!(
            view.revealed_alternatives(view.root(), &control).unwrap(),
            None
        );
        let (_, child) = view.edge(view.root(), 0).unwrap();
        assert!(!view.node(child).unwrap().accepting);
        assert!(!view.has_physical_actions(child));
        assert!(matches!(
            view.prepare_edges(child, 1 << 1, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_hidden_horizon_exceeded"
            ))
        ));
        assert_eq!(view.revealed_alternatives(child, &control).unwrap(), None);
        let revealed = view.fully_revealed_node(child, &control).unwrap();
        assert_ne!(revealed, child);
        assert_eq!(view.node(revealed).unwrap().depth, 1);
        assert!(!view.has_physical_actions(revealed));
        let alternatives = view
            .revealed_alternatives(revealed, &control)
            .unwrap()
            .unwrap();
        assert_eq!(alternatives.len(), 1);
        let kind = alternatives[0];
        assert_ne!(kind & KIND_TAG, 0);
        view.prepare_edges(kind, 1 << 1, &control).unwrap();
        let (_, terminal) = view.edge(kind, 0).unwrap();
        assert!(view.node(terminal).unwrap().accepting);
        drop(view);
        let mut replacement = PieceOrderLanguageCache::default();
        let orders = two_os();
        orders
            .compile_product_language(&mut replacement, &mut SymbolicPrefixWork::default())
            .unwrap();
        assert!(matches!(
            family.view(&replacement, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_arena_changed"
            ))
        ));
        assert!(matches!(
            family.register(two_os(), &[], &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_board_family_frozen"
            ))
        ));
    }

    #[test]
    fn revealed_singletons_reuse_kind_nodes_and_cache_board_aliases() {
        let control = ExecutionControl::default();
        let plan = BoardObservationPlan::new(10, 2, initial(), 2, 1, 1, None).unwrap();
        let mut family = BoardActionFamily::new(plan);
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut family, &mut kinds);
        let view = family.view_with_kind_union(&kinds, &control, true).unwrap();
        let (_, child) = view.edge(view.root(), 0).unwrap();
        assert!(view.prepare_edges(child, 0x7f, &control).is_err());
        assert_eq!(view.revealed_alternatives(child, &control).unwrap(), None);
        let revealed = view.fully_revealed_node(child, &control).unwrap();
        assert_ne!(revealed & KIND_TAG, 0);
        assert!(!is_kind_union(revealed));
        assert_eq!(
            view.revealed_alternatives(revealed, &control).unwrap(),
            None
        );
        let metrics = view.kind_union_metrics();
        assert_eq!(&metrics[..4], &[0, 0, 0, 0]);
        assert!(metrics[4] > 0);
        for _ in 0..8 {
            assert_eq!(view.fully_revealed_node(child, &control).unwrap(), revealed);
        }
        assert_eq!(view.kind_union_metrics(), metrics);
        assert!(view.peak_bytes() >= view.retained_bytes());
        view.prepare_edges(revealed, 1 << 1, &control).unwrap();
        let (piece, terminal) = view.edge(revealed, 0).unwrap();
        assert_eq!(piece, 2);
        assert!(view.node(terminal).unwrap().accepting);
        assert_eq!(view.node(terminal).unwrap().depth, 2);
    }

    #[test]
    fn board_worker_merge_rebinds_roots_and_rejects_distinct_request_capabilities() {
        let control = ExecutionControl::default();
        let plan = boundary_mode(
            BoardObservationPlan::new(10, 2, initial(), 2, 2, 0, None).unwrap(),
            false,
        );
        let mut destination = BoardActionFamily::new(Arc::clone(&plan));
        let mut kinds = PieceOrderLanguageCache::default();
        register(&mut destination, &mut kinds);
        let mut worker = BoardActionFamily::new(Arc::clone(&plan));
        let mut worker_kinds = PieceOrderLanguageCache::default();
        let mut parents = [[0; MAX_PREFIX_WORDS]; 1];
        parents[0][0] = 1;
        let mut active = [0; MAX_PREFIX_WORDS];
        active[0] = 3;
        worker_kinds
            .compile_product_closed_prefix_language(&[PieceKind::I], &parents, &active, &mut 0)
            .unwrap();
        register(&mut worker, &mut worker_kinds);
        let rebound = kinds.import_node_map(&worker_kinds).unwrap();
        assert!(rebound
            .iter()
            .enumerate()
            .any(|(old, &new)| old != new as usize));
        destination.merge(worker, &rebound, &control).unwrap();
        assert_eq!(destination.metrics().candidate_count, 2);
        assert_eq!(destination.metrics().nodes, 4);
        assert_eq!(destination.metrics().edges, 4);
        assert_eq!(destination.metrics().suffix_roots, 4);
        assert!(destination.peak_bytes() >= destination.retained_bytes());

        let other_plan = BoardObservationPlan::new(10, 2, initial(), 2, 2, 0, None).unwrap();
        let mut other = BoardActionFamily::new(other_plan);
        register(&mut other, &mut worker_kinds);
        assert!(matches!(
            destination.merge(other, &rebound, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_board_request_scope_changed"
            ))
        ));
        assert!(matches!(
            destination.view(&kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_board_family_failed"
            ))
        ));

        let mut destination = BoardActionFamily::new(Arc::clone(&plan));
        let mut worker = BoardActionFamily::new(plan);
        register(&mut worker, &mut worker_kinds);
        assert!(matches!(
            destination.merge(worker, &[], &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_language_missing"
            ))
        ));
        assert!(destination.view(&kinds, &control).is_err());
    }

    fn suffix_import_worker(
        plan: Arc<BoardObservationPlan>,
        control: &ExecutionControl,
    ) -> (PhysicalActionLanguage, PieceOrderLanguageCache) {
        let horizon = plan.hidden_horizon();
        let floor = plan.reveal_floor();
        let mut worker = PhysicalActionLanguage::default();
        worker.set_board_plan(plan).unwrap();
        let mut kinds = PieceOrderLanguageCache::default();
        let orders = two_os();
        worker
            .compile_relation_language(
                &orders,
                &mut kinds,
                &mut SymbolicPrefixWork::default(),
                control,
            )
            .unwrap();
        worker
            .register_relation_with_reveal_floor(orders, horizon, floor, control)
            .unwrap();
        // This candidate's oracle graph was compiled but never retained by
        // the board producer. It must not enter the coordinator kind arena.
        two_is()
            .compile_product_language(&mut kinds, &mut SymbolicPrefixWork::default())
            .unwrap();
        (worker, kinds)
    }

    #[test]
    fn board_subset_import_preserves_merge_actions_coverage_and_physical_work() {
        let control = ExecutionControl::default();
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(974),
            PatternWeightModelId::new(975),
            vec![
                vec![PieceKind::O, PieceKind::O],
                vec![PieceKind::I, PieceKind::I],
                vec![PieceKind::O, PieceKind::O],
            ],
            vec![
                ProbabilityValue::new(0.2).unwrap(),
                ProbabilityValue::new(0.3).unwrap(),
                ProbabilityValue::new(0.5).unwrap(),
            ],
            3,
            true,
            None,
        )
        .unwrap();
        for boundary_only in [false, true] {
            let plan = boundary_mode(
                BoardObservationPlan::new(10, 2, initial(), 2, 1, 0, None).unwrap(),
                boundary_only,
            );
            let mut baseline = None;
            for subset in [false, true] {
                let (worker, worker_kinds) = suffix_import_worker(Arc::clone(&plan), &control);
                let worker_work = worker.board.as_ref().unwrap().metrics().work;
                let mut merged = PhysicalActionLanguage::default();
                merged.set_board_plan(Arc::clone(&plan)).unwrap();
                let mut kinds = PieceOrderLanguageCache::default();
                if subset {
                    let imported = worker
                        .import_board_suffixes(&worker_kinds, &mut kinds, &control)
                        .unwrap()
                        .unwrap();
                    assert_eq!(worker.board.as_ref().unwrap().metrics().work, worker_work);
                    assert!(imported.metrics().reachable_nodes < worker_kinds.node_count());
                    let rebound = imported.rebound_for(&worker_kinds, &kinds).unwrap();
                    merged
                        .merge_factored(worker, rebound, &control, &kinds)
                        .unwrap();
                } else {
                    let rebound = kinds.import_node_map(&worker_kinds).unwrap();
                    merged
                        .merge_factored(worker, &rebound, &control, &kinds)
                        .unwrap();
                }
                let view = merged
                    .board
                    .as_mut()
                    .unwrap()
                    .view_with_kind_union(&kinds, &control, true)
                    .unwrap();
                let prefixes = physical_prefixes(&view, view.root(), 1);
                let coverage = QueueObservationPolicyEvaluator::for_original_universe(
                    &universe,
                    QueueObservationPolicy::VisibleSeven,
                    0,
                    None,
                    false,
                    false,
                    false,
                    None,
                )
                .unwrap()
                .evaluate(&view, &control)
                .unwrap();
                assert_eq!(coverage.covered_pattern_count, 2);
                assert_eq!(coverage.covered_patterns.words()[0], 0b101);
                let result = (
                    prefixes,
                    coverage.covered_patterns.words().to_vec(),
                    coverage.covered_weight.to_bits(),
                    coverage.min_accepted_depth,
                    coverage.max_accepted_depth,
                    view.metrics().work,
                );
                if let Some(expected) = &baseline {
                    assert_eq!(&result, expected);
                } else {
                    baseline = Some(result);
                }
            }
        }
    }

    #[test]
    fn board_subset_import_validates_both_owner_layers_and_the_producer_phase() {
        let control = ExecutionControl::default();
        let plan = BoardObservationPlan::new(10, 2, initial(), 2, 1, 1, None).unwrap();
        let (mut worker, kinds) = suffix_import_worker(plan, &control);
        let mut destination = PieceOrderLanguageCache::default();
        let (_, other_kinds) = suffix_import_worker(
            BoardObservationPlan::new(10, 2, initial(), 2, 1, 1, None).unwrap(),
            &control,
        );
        assert!(matches!(
            worker.import_board_suffixes(&other_kinds, &mut destination, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_arena_changed"
            ))
        ));
        assert_eq!(destination.node_count(), 0);
        worker.board.as_mut().unwrap().kind_arena = Some(Arc::clone(other_kinds.arena_identity()));
        assert!(matches!(
            worker.import_board_suffixes(&kinds, &mut destination, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_arena_changed"
            ))
        ));
        worker.board.as_mut().unwrap().kind_arena = None;

        // Exercise the large-set iterator without changing a stored member.
        let family = worker.board.as_mut().unwrap();
        for node in &mut family.nodes {
            if let SuffixRoots::Small(roots) = &node.suffixes {
                node.suffixes = SuffixRoots::Large(roots.iter().copied().collect());
            }
        }
        family.suffix_capacity_bytes = family
            .nodes
            .iter()
            .map(|node| node.suffixes.retained_bytes())
            .sum();
        family.observe_peak(0);
        let imported = worker
            .import_board_suffixes(&kinds, &mut destination, &control)
            .unwrap()
            .unwrap();
        assert_eq!(
            imported.metrics().root_visits,
            worker.board.as_ref().unwrap().metrics().suffix_roots
        );
        assert!(imported.rebound_for(&kinds, &destination).is_ok());

        drop(
            worker
                .board
                .as_mut()
                .unwrap()
                .view(&kinds, &control)
                .unwrap(),
        );
        assert!(matches!(
            worker.import_board_suffixes(&kinds, &mut destination, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_board_family_frozen"
            ))
        ));
    }
}
