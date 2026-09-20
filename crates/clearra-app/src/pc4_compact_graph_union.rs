//! SRP rationale: this module's single change reason is enumeration of the
//! complete canonical graph x compact-supply union without
//! reveal histories. I/O, observation probabilities and product reduction stay
//! outside. Missing records suspend individual work items, never mean no edge.
use core::{convert::Infallible, num::NonZeroUsize};
use std::collections::{HashMap, HashSet, VecDeque};

#[path = "pc4_frontier_retention.rs"]
mod retention;
use retention::{capacity_bytes, FrontierRetention, FrontierRetentionError};

use clearra_core_domain::{
    board::standard_pc_board::StandardPcBoard,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
};
use clearra_pc4_tablebase::{
    clearra_board64_mask_to_hydra_field_hash_v1, materialize_qualified_graph_edge,
    read_qualified_pc4_adjacency, ClearraPlacementIdentity, FixedQueueTraversalPageError,
    Pc4RowFrame, PlacementMaterializationError, QualifiedPc4GraphEdge, QualifiedPc4TargetIdentity,
};
use clearra_supply::pattern_universe::{
    CompactPatternUnionError, CompactPatternUnionFrontier, CompactPatternUnionLanguage,
    CompactPatternUnionLimits,
};

use super::{
    cooperative_canonicalizer::{
        CandidateCanonicalizationError, CooperativeCandidateCanonicalizer,
    },
    graph_candidate_adapter::Pc4GraphCandidateGuard,
    PcCandidateBoundaryError, PcCandidateCompletenessEvidence, PcCandidatePageGuard,
    PcCandidateReducerInput, PcCandidateSourceBinding, PcCandidateUniverseIdentity,
};
use crate::{
    pc4_input_disclosure_policy::{Pc4PreparedOnlineInput, Pc4PreparedQueueInput},
    pc4_lookup_graph_runtime_adapter::{
        Pc4LookupAdjacencyError, Pc4LookupGraphCache, Pc4LookupMaterializationError,
    },
};

/// Retained frontier bytes and independent work/result limits. Graph cache,
/// source owners and I/O have separate bounds; no limit authorizes truncation.
#[derive(Clone, Copy)]
pub(crate) struct CompactGraphUnionLimits {
    pub frontier_bytes: NonZeroUsize,
    /// Resident source/target dependencies, not all cold next-layer states.
    pub resident_work: NonZeroUsize,
    pub work: NonZeroUsize,
    pub candidates: NonZeroUsize,
    pub waiting_fields: NonZeroUsize,
    pub edge_placements: NonZeroUsize,
    /// Candidate buffers during collection/sorting only, not whole-search RSS.
    pub canonicalization_bytes: NonZeroUsize,
    pub language: CompactPatternUnionLimits,
}

#[derive(Debug)]
pub(crate) enum CompactGraphUnionError {
    Contract(&'static str),
    Supply(CompactPatternUnionError),
    Adjacency(FixedQueueTraversalPageError<Pc4LookupAdjacencyError, Infallible>),
    Placement(PlacementMaterializationError<Pc4LookupMaterializationError>),
    Boundary(PcCandidateBoundaryError),
    Canonicalization(CandidateCanonicalizationError),
    Retention(FrontierRetentionError),
    Limit {
        kind: &'static str,
        limit: usize,
        attempted: usize,
    },
}

impl CompactGraphUnionError {
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            Self::Contract(reason) => reason,
            Self::Supply(error) => error.reason(),
            Self::Adjacency(error) => error.reason(),
            Self::Placement(error) => error.reason(),
            Self::Boundary(error) => error.reason(),
            Self::Canonicalization(error) => error.reason(),
            Self::Retention(error) => error.reason(),
            Self::Limit { kind, .. } => kind,
        }
    }
}

impl From<FrontierRetentionError> for CompactGraphUnionError {
    fn from(error: FrontierRetentionError) -> Self {
        Self::Retention(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompactGraphUnionStep {
    Progress,
    Waiting,
    Complete,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CompactGraphUnionUsage {
    pub work: usize,
    pub generated_states: usize,
    pub merged_states: usize,
    pub promoted_states: usize,
    pub materialized_edges: usize,
    pub peak_waiting_fields: usize,
    pub canonicalization_work: usize,
    pub peak_canonicalization_buffer_bytes: usize,
    pub peak_frontier_bytes: usize,
    pub terminal_arrivals: usize,
    pub peak_resident_work: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct StateKey {
    layout: StandardBoard64TilingIdentity,
    field: u32,
    frame: Pc4RowFrame,
}

/// Moving a completed layer is cooperative too: a large retained frontier
/// must not become one uninterruptible O(layer size) queue conversion.
struct LayerPromotion {
    entries: std::collections::hash_map::IntoIter<StateKey, CompactPatternUnionFrontier>,
    table_capacity: usize,
}

struct Work {
    key: StateKey,
    supply: CompactPatternUnionFrontier,
    piece: usize,
    next_supply: Option<CompactPatternUnionFrontier>,
    targets: Option<Vec<u32>>,
    target_index: usize,
    placements: Option<Vec<ClearraPlacementIdentity>>,
    placement_index: usize,
    resident: bool,
    target_pin: Option<u32>,
}

impl Work {
    fn new(key: StateKey, supply: CompactPatternUnionFrontier) -> Self {
        Self {
            key,
            supply,
            piece: 0,
            next_supply: None,
            targets: None,
            target_index: 0,
            placements: None,
            placement_index: 0,
            resident: false,
            target_pin: None,
        }
    }

    fn supply_states(&self) -> usize {
        self.supply.state_count() + self.next_supply.as_ref().map_or(0, |s| s.state_count())
    }

    fn retained_payload_bytes(&self) -> Result<usize, CompactGraphUnionError> {
        let mut bytes = self.supply.retained_state_capacity_bytes();
        for payload in [
            self.next_supply
                .as_ref()
                .map_or(0, |s| s.retained_state_capacity_bytes()),
            capacity_bytes::<u32>(self.targets.as_ref().map_or(0, Vec::capacity))?,
            capacity_bytes::<ClearraPlacementIdentity>(
                self.placements.as_ref().map_or(0, Vec::capacity),
            )?,
        ] {
            bytes = bytes
                .checked_add(payload)
                .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        }
        Ok(bytes)
    }

    fn next_piece(&mut self) -> Result<(usize, usize), CompactGraphUnionError> {
        let payload = self
            .retained_payload_bytes()?
            .checked_sub(self.supply.retained_state_capacity_bytes())
            .ok_or(FrontierRetentionError::Underflow)?;
        let released = self.next_supply.take().map_or(0, |s| s.state_count());
        self.targets = None;
        self.placements = None;
        self.target_index = 0;
        self.placement_index = 0;
        self.piece += 1;
        Ok((released, payload))
    }
}

enum WorkStep {
    Continue,
    Done,
    Missing(u32),
}

/// Input is nominally source-bound. A layer is sealed before expanding its
/// successor, so all incoming supply frontiers are merged exactly once before
/// a state is expanded. Independent items within the layer may finish in any
/// I/O order. A future cross-layer scheduler must supply a delta/fixpoint proof;
/// removing this barrier by merely marking a node "visited" loses solutions.
pub(crate) struct Pc4CompactGraphUnion {
    source: PcCandidateSourceBinding,
    target: QualifiedPc4TargetIdentity,
    language: CompactPatternUnionLanguage,
    limits: CompactGraphUnionLimits,
    start_field: u32,
    start_hash: u64,
    start_verified: bool,
    placement_count: usize,
    full_mask: u64,
    ready: VecDeque<Work>,
    waiting: HashMap<u32, Vec<Work>>,
    waiting_count: usize,
    next_layer: HashMap<StateKey, CompactPatternUnionFrontier>,
    promotion: Option<LayerPromotion>,
    candidates: HashSet<StandardBoard64TilingIdentity>,
    canonicalizer: Option<CooperativeCandidateCanonicalizer>,
    retained_supply_states: usize,
    retention: FrontierRetention,
    residents: usize,
    pins: HashMap<u32, usize>,
    cache_revision_seen: u64,
    completed: bool,
    terminated: bool,
    usage: CompactGraphUnionUsage,
}

impl Pc4CompactGraphUnion {
    pub(crate) fn check_current<G: Pc4GraphCandidateGuard>(
        &self,
        guard: &G,
    ) -> Result<(), CompactGraphUnionError> {
        if self.terminated {
            return Err(contract("pc4_compact_union_terminated"));
        }
        check_guard(&self.source, &self.target, guard)
    }

    pub(crate) fn has_ready_work(&self) -> bool {
        !self.completed
            && !self.terminated
            && (self.canonicalizer.is_some()
                || self.promotion.is_some()
                || self.ready.front().is_some_and(|task| {
                    task.resident || self.residents < self.limits.resident_work.get()
                })
                || (self.ready.is_empty() && self.waiting.is_empty()))
    }

    pub(crate) fn protects_field(&self, field: u32) -> bool {
        self.pins.contains_key(&field) || (!self.start_verified && field == self.start_field)
    }

    /// Watermarks throttle new CPU demand before the hard retained-state
    /// limits are reached. They are counts, not whole-owner byte authority.
    #[cfg(test)]
    pub(crate) fn io_demand_is_full(&self, fields: usize, continuations: usize) -> bool {
        // Before its first verified record the root is a real pending demand,
        // but has not been parked in `waiting` yet. Match pending_fields rather
        // than silently reporting zero demand for that bootstrap phase.
        let root = usize::from(!self.start_verified && !self.completed && !self.terminated);
        self.waiting.len() + root >= fields || self.waiting_count + root >= continuations
    }

    #[cfg(test)]
    pub(crate) fn resident_capacity_blocks_cold_front(&self) -> bool {
        self.residents == self.limits.resident_work.get()
            && self.ready.front().is_some_and(|task| !task.resident)
    }

    pub(crate) fn prepare<G: Pc4GraphCandidateGuard>(
        source: &PcCandidateSourceBinding,
        prepared: &Pc4PreparedOnlineInput,
        cache: &Pc4LookupGraphCache,
        start_field: u32,
        limits: CompactGraphUnionLimits,
        guard: &G,
    ) -> Result<Option<Self>, CompactGraphUnionError> {
        let Pc4PreparedQueueInput::CompiledPattern(pattern) = prepared.queue() else {
            return Ok(None);
        };
        let target = prepared.target();
        if cache.target() != target || start_field >= cache.field_count() {
            return Err(contract("pc4_compact_union_target_mismatch"));
        }
        check_guard(source, target, guard)?;
        let problem = pattern.problem();
        crate::validate_pc4_search_problem_compatibility(target.profile(), problem)
            .map_err(|e| contract(e.reason()))?;
        let initial = problem.initial_board();
        let board = StandardPcBoard::from_words(
            target.target_lines().get(),
            [initial.occupied_mask(), 0, 0, 0],
        )
        .map_err(|_| contract("pc4_compact_union_initial_board_invalid"))?;
        let hold = crate::pc_candidate_execution_bridge::fixed_queue_hold_state(
            problem.core_query().allow_hold(),
            problem.core_query().hold_state(),
        );
        crate::online_pc4_fixed_queue_candidate_session::validate_prepared_candidate_source(
            source, prepared, board, hold,
        )
        .map_err(|e| contract(e.reason()))?;
        if initial.width() != 10 || initial.visible_height() != u16::from(board.lines()) {
            return Err(contract("pc4_compact_union_initial_board_mismatch"));
        }
        let free = u32::from(board.cell_count()) - board.occupied().count_ones();
        if !free.is_multiple_of(4) {
            return Err(contract("pc4_compact_union_invalid_area"));
        }
        let Some((language, supply)) = pattern
            .compact_union_language(limits.language, &|| {
                PcCandidatePageGuard::is_cancelled(guard)
            })
            .map_err(|e| contract(e.reason()))?
        else {
            return Ok(None);
        };
        let layout =
            StandardBoard64TilingIdentity::from_placements(source.initial_board_mask(), [])
                .map_err(|_| contract("pc4_compact_union_initial_layout_invalid"))?;
        let mut prefix = 0;
        while prefix < 4 && (source.initial_board_mask() >> (10 * prefix)) & 1023 == 1023 {
            prefix += 1;
        }
        let frame = Pc4RowFrame::new(prefix)
            .map_err(|_| contract("pc4_compact_union_row_frame_invalid"))?;
        let retained_supply_states = supply.state_count();
        let mut retention = FrontierRetention::new(limits.frontier_bytes);
        let initial_payload = supply.retained_state_capacity_bytes();
        retention.authorize(capacity_bytes::<Work>(1)?, initial_payload)?;
        let mut ready = VecDeque::new();
        ready.try_reserve(1).map_err(|_| allocation())?;
        retention.retain(capacity_bytes::<Work>(ready.capacity())?, initial_payload)?;
        ready.push_back(Work::new(
            StateKey {
                layout,
                field: start_field,
                frame,
            },
            supply,
        ));
        let start_hash = clearra_board64_mask_to_hydra_field_hash_v1(source.initial_board_mask())
            .map_err(|_| contract("pc4_compact_union_initial_hash_invalid"))?;
        Ok(Some(Self {
            source: source.clone(),
            target: target.clone(),
            language,
            limits,
            start_field,
            start_hash,
            start_verified: false,
            placement_count: (free / 4) as usize,
            full_mask: (1u64 << (10 * u32::from(board.lines()))) - 1,
            ready,
            waiting: HashMap::new(),
            waiting_count: 0,
            next_layer: HashMap::new(),
            promotion: None,
            candidates: HashSet::new(),
            canonicalizer: None,
            retained_supply_states,
            retention,
            residents: 0,
            pins: HashMap::new(),
            cache_revision_seen: 0,
            completed: false,
            terminated: false,
            usage: CompactGraphUnionUsage::default(),
        }))
    }

    /// Bounded demand list: duplicates share one cache record. Exposing demands
    /// while CPU progress remains lets the host start I/O without a global wait.
    pub(crate) fn pending_fields(
        &self,
        maximum: usize,
    ) -> Result<Vec<u32>, CompactGraphUnionError> {
        if self.terminated || self.completed || maximum == 0 {
            return Ok(Vec::new());
        }
        // The caller owns this small demand list. Never allocate every waiting
        // ID merely to return at most the caller's I/O window (normally 16).
        let mut ids = Vec::new();
        ids.try_reserve_exact(if self.start_verified {
            maximum.min(self.waiting.len())
        } else {
            1
        })
        .map_err(|_| allocation())?;
        if !self.start_verified {
            ids.push(self.start_field);
            return Ok(ids);
        }
        for &id in self.waiting.keys() {
            let index = ids.binary_search(&id).unwrap_err();
            if index < maximum {
                if ids.len() == maximum {
                    ids.pop();
                }
                ids.insert(index, id);
            }
        }
        Ok(ids)
    }

    pub(crate) const fn usage(&self) -> CompactGraphUnionUsage {
        self.usage
    }

    pub(crate) fn advance<G: Pc4GraphCandidateGuard>(
        &mut self,
        cache: &Pc4LookupGraphCache,
        work: NonZeroUsize,
        guard: &G,
    ) -> Result<CompactGraphUnionStep, CompactGraphUnionError> {
        if self.terminated {
            return Err(contract("pc4_compact_union_terminated"));
        }
        let result = self.advance_inner(cache, work, guard);
        if result.is_err() {
            self.terminated = true;
            self.completed = false;
            self.ready = VecDeque::new();
            self.waiting = HashMap::new();
            self.next_layer = HashMap::new();
            self.promotion = None;
            self.candidates = HashSet::new();
            self.canonicalizer = None;
            self.waiting_count = 0;
            self.retained_supply_states = 0;
            self.residents = 0;
            self.pins = HashMap::new();
        }
        self.usage.peak_frontier_bytes = self.retention.peak();
        result
    }

    fn advance_inner<G: Pc4GraphCandidateGuard>(
        &mut self,
        cache: &Pc4LookupGraphCache,
        work: NonZeroUsize,
        guard: &G,
    ) -> Result<CompactGraphUnionStep, CompactGraphUnionError> {
        check_guard(&self.source, &self.target, guard)?;
        if cache.target() != &self.target {
            return Err(contract("pc4_compact_union_cache_mismatch"));
        }
        if self.completed {
            return Ok(CompactGraphUnionStep::Complete);
        }
        if let Some(canonicalizer) = &mut self.canonicalizer {
            let consumed = self
                .usage
                .work
                .checked_add(self.usage.canonicalization_work)
                .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
            let remaining = self.limits.work.get().saturating_sub(consumed);
            let Some(remaining) = NonZeroUsize::new(remaining) else {
                return Err(CompactGraphUnionError::Limit {
                    kind: "pc4_compact_union_work_limit",
                    limit: self.limits.work.get(),
                    attempted: consumed.saturating_add(1),
                });
            };
            let complete = canonicalizer
                .advance(work.min(remaining), &|| {
                    PcCandidatePageGuard::is_cancelled(guard)
                })
                .map_err(CompactGraphUnionError::Canonicalization)?;
            self.usage.canonicalization_work = canonicalizer.work_done();
            self.usage.peak_canonicalization_buffer_bytes = canonicalizer.peak_buffer_bytes();
            check_guard(&self.source, &self.target, guard)?;
            self.completed = complete;
            return Ok(if complete {
                CompactGraphUnionStep::Complete
            } else {
                CompactGraphUnionStep::Progress
            });
        }
        if !self.start_verified {
            let Some(hash) = cache.field_hash(self.start_field) else {
                return Ok(CompactGraphUnionStep::Waiting);
            };
            if hash != self.start_hash {
                return Err(contract("pc4_compact_union_start_field_mismatch"));
            }
            self.start_verified = true;
        }
        if cache.admission_revision() != self.cache_revision_seen || self.ready.is_empty() {
            // The host bounds waiting fields with its I/O watermarks. Wake
            // known records without allocating a second list of all waiters.
            while let Some(id) = self
                .waiting
                .keys()
                .copied()
                .find(|id| cache.contains_field_id(*id))
            {
                let tasks = self.waiting.remove(&id).expect("collected waiting field");
                let old_buffer = capacity_bytes::<Work>(tasks.capacity())?;
                self.reserve_ready(tasks.len())?;
                self.waiting_count -= tasks.len();
                // Resumed residents precede cold work. Keep the same source
                // resident until done or blocked, instead of pinning an entire
                // large layer before any of its items can release a record.
                for task in tasks.into_iter().rev() {
                    self.ready.push_front(task);
                }
                self.retention.release(old_buffer)?;
            }
            self.cache_revision_seen = cache.admission_revision();
        }
        for _ in 0..work.get() {
            check_guard(&self.source, &self.target, guard)?;
            if self.promotion.is_none() && self.ready.is_empty() && !self.waiting.is_empty() {
                return Ok(CompactGraphUnionStep::Waiting);
            }
            if self.residents == self.limits.resident_work.get()
                && !self.ready.front().is_some_and(|task| task.resident)
            {
                // All resident continuations are waiting. Do not charge work
                // or promote another cold task while only I/O can unblock a
                // resident and return its dependency pins/credit.
                return Ok(CompactGraphUnionStep::Waiting);
            }
            self.usage.work = self
                .usage
                .work
                .checked_add(1)
                .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
            limit(
                "pc4_compact_union_work_limit",
                self.limits.work,
                self.usage.work,
            )?;
            if let Some(promotion) = &mut self.promotion {
                if let Some((key, supply)) = promotion.entries.next() {
                    // All ready slots were admitted before promotion. The
                    // supply payload changes owner, not allocation or credit.
                    self.ready.push_back(Work::new(key, supply));
                    self.usage.promoted_states += 1;
                } else {
                    self.promotion = None;
                }
                continue;
            }
            if self.ready.is_empty() {
                if !self.waiting.is_empty() {
                    return Ok(CompactGraphUnionStep::Waiting);
                }
                if self.next_layer.is_empty() {
                    if self.retained_supply_states != 0
                        || self.retention.nested() != 0
                        || self.residents != 0
                        || !self.pins.is_empty()
                    {
                        return Err(contract("pc4_compact_union_frontier_accounting_failed"));
                    }
                    // No histories remain. Release queue backing stores before
                    // retaining both candidate buffers during canonicalization.
                    self.ready = VecDeque::new();
                    self.waiting = HashMap::new();
                    self.next_layer = HashMap::new();
                    self.pins = HashMap::new();
                    self.canonicalizer = Some(
                        CooperativeCandidateCanonicalizer::begin(
                            core::mem::take(&mut self.candidates),
                            self.limits.canonicalization_bytes,
                        )
                        .map_err(CompactGraphUnionError::Canonicalization)?,
                    );
                    return Ok(CompactGraphUnionStep::Progress);
                }
                self.reserve_ready(self.next_layer.len())?;
                let old_layer = core::mem::take(&mut self.next_layer);
                self.promotion = Some(LayerPromotion {
                    table_capacity: old_layer.capacity(),
                    entries: old_layer.into_iter(),
                });
                continue;
            }
            let mut task = self.ready.pop_front().expect("ready layer");
            if !task.resident {
                self.pin_field(task.key.field)?;
                self.residents += 1;
                self.usage.peak_resident_work = self.usage.peak_resident_work.max(self.residents);
                task.resident = true;
            }
            match self.step_work(&mut task, cache, guard)? {
                WorkStep::Continue => self.ready.push_front(task),
                WorkStep::Done => {
                    self.unpin_field(task.key.field)?;
                    if let Some(field) = task.target_pin {
                        self.unpin_field(field)?;
                    }
                    self.residents -= 1;
                    let states = task.supply_states();
                    let bytes = task.retained_payload_bytes()?;
                    drop(task);
                    self.retained_supply_states -= states;
                    self.retention.release(bytes)?;
                }
                WorkStep::Missing(id) => {
                    if !self.waiting.contains_key(&id) {
                        limit(
                            "pc4_compact_union_waiting_field_limit",
                            self.limits.waiting_fields,
                            self.waiting.len() + 1,
                        )?;
                        self.reserve_waiting_field()?;
                    }
                    self.reserve_waiting_work(id)?;
                    self.waiting
                        .get_mut(&id)
                        .expect("reserved waiter")
                        .push(task);
                    self.waiting_count += 1;
                    self.usage.peak_waiting_fields =
                        self.usage.peak_waiting_fields.max(self.waiting.len());
                }
            }
        }
        check_guard(&self.source, &self.target, guard)?;
        Ok(CompactGraphUnionStep::Progress)
    }

    fn step_work<G: Pc4GraphCandidateGuard>(
        &mut self,
        task: &mut Work,
        cache: &Pc4LookupGraphCache,
        guard: &G,
    ) -> Result<WorkStep, CompactGraphUnionError> {
        if task.supply.is_empty() {
            return Ok(WorkStep::Done);
        }
        if task.key.layout.placement_count() == self.placement_count {
            self.accept_terminal(task.key.layout, task.key.field)?;
            return Ok(WorkStep::Done);
        }
        if task.piece == PieceKind::STANDARD_TETROMINOES.len() {
            return Ok(WorkStep::Done);
        }
        let piece = PieceKind::STANDARD_TETROMINOES[task.piece];
        if task.next_supply.is_none() {
            self.authorize_frontier(
                self.language
                    .maximum_frontier_capacity_bytes()
                    .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?,
            )?;
            let next = self
                .language
                .advance(&task.supply, piece, &|| {
                    PcCandidatePageGuard::is_cancelled(guard)
                })
                .map_err(CompactGraphUnionError::Supply)?;
            if next.is_empty() {
                task.piece += 1;
                return Ok(WorkStep::Continue);
            }
            self.retain_supply(next.state_count(), next.retained_state_capacity_bytes())?;
            task.next_supply = Some(next);
            return Ok(WorkStep::Continue);
        }
        if task.targets.is_none() {
            // Qualified v1 records use seven cumulative u8 degree endpoints.
            self.authorize_frontier(capacity_bytes::<u32>(usize::from(u8::MAX))?)?;
            let edges = match read_qualified_pc4_adjacency(
                &self.target,
                task.key.field,
                crate::pc_candidate_execution_bridge::core_piece_to_graph(piece),
                task.key.layout.placement_count(),
                &mut cache.adjacency_provider(),
                guard,
            ) {
                Ok(edges) => edges,
                Err(FixedQueueTraversalPageError::Provider(
                    Pc4LookupAdjacencyError::RecordRequired { field_id },
                )) => {
                    return Ok(WorkStep::Missing(field_id));
                }
                Err(error) => return Err(CompactGraphUnionError::Adjacency(error)),
            };
            let mut targets = Vec::new();
            targets
                .try_reserve_exact(edges.len())
                .map_err(|_| allocation())?;
            targets.extend(edges.into_iter().map(|edge| edge.target_field_id()));
            self.retain_frontier(capacity_bytes::<u32>(targets.capacity())?)?;
            task.targets = Some(targets);
            return Ok(WorkStep::Continue);
        }
        let targets = task.targets.as_ref().expect("prepared piece adjacency");
        let Some(&target_field) = targets.get(task.target_index) else {
            let (states, bytes) = task.next_piece()?;
            self.retained_supply_states -= states;
            self.retention.release(bytes)?;
            return Ok(WorkStep::Continue);
        };
        if task.placements.is_none() {
            if task.target_pin.is_none() {
                self.pin_field(target_field)?;
                task.target_pin = Some(target_field);
            }
            self.authorize_frontier(capacity_bytes::<ClearraPlacementIdentity>(
                self.limits.edge_placements.get(),
            )?)?;
            let edge = QualifiedPc4GraphEdge::from_qualified_record(
                &self.target,
                task.key.field,
                crate::pc_candidate_execution_bridge::core_piece_to_graph(piece),
                target_field,
            );
            let placements = match materialize_qualified_graph_edge(
                &edge,
                &mut cache.placement_materializer(),
                guard,
            ) {
                Ok(placements) => placements,
                Err(PlacementMaterializationError::Materializer(
                    Pc4LookupMaterializationError::SourceRecordRequired { field_id }
                    | Pc4LookupMaterializationError::TargetRecordRequired { field_id },
                )) => {
                    return Ok(WorkStep::Missing(field_id));
                }
                Err(error) => return Err(CompactGraphUnionError::Placement(error)),
            };
            limit(
                "pc4_compact_union_edge_placement_limit",
                self.limits.edge_placements,
                placements.len(),
            )?;
            self.usage.materialized_edges += 1;
            self.unpin_field(task.target_pin.take().expect("materialization target pin"))?;
            self.retain_frontier(capacity_bytes::<ClearraPlacementIdentity>(
                placements.capacity(),
            )?)?;
            task.placements = Some(placements);
            return Ok(WorkStep::Continue);
        }
        let Some(&placement) = task
            .placements
            .as_ref()
            .expect("materialized edge")
            .get(task.placement_index)
        else {
            let old = task.placements.take().expect("exhausted placements");
            let bytes = capacity_bytes::<ClearraPlacementIdentity>(old.capacity())?;
            drop(old);
            self.retention.release(bytes)?;
            task.placement_index = 0;
            task.target_index += 1;
            return Ok(WorkStep::Continue);
        };
        let (lifted, frame) = placement
            .rebase_in_frame(task.key.frame)
            .map_err(|e| contract(e.reason()))?;
        let cells = lifted.occupied_cells();
        // A graph edge can have several ILC realizations. A realization that
        // overlaps this original-frame history is not this history's child.
        let occupied = task
            .key
            .layout
            .placement_masks()
            .iter()
            .fold(self.source.initial_board_mask(), |all, mask| all | mask);
        if cells & !self.full_mask == 0 && occupied & cells == 0 {
            let layout = StandardBoard64TilingIdentity::from_placements(
                self.source.initial_board_mask(),
                (0..task.key.layout.placement_count())
                    .map(|i| task.key.layout.placement(i).expect("bounded placement"))
                    .chain([PiecePlacementMask::new(piece, cells)]),
            )
            .map_err(|_| contract("pc4_compact_union_layout_invalid"))?;
            let key = StateKey {
                layout,
                field: target_field,
                frame,
            };
            let supply = task.next_supply.as_ref().expect("advanced supply");
            if layout.placement_count() == self.placement_count {
                // Existence is already witnessed by this nonempty supply
                // prefix. A terminal has no successor to union/expand, so its
                // canonical layout need not occupy the next layer or copy a
                // supply frontier. Completion still waits for every branch.
                self.accept_terminal(layout, target_field)?;
            } else if let Some(previous) = self.next_layer.get(&key) {
                let old_len = previous.state_count();
                let old_bytes = previous.retained_state_capacity_bytes();
                self.authorize_frontier(
                    self.language
                        .maximum_frontier_capacity_bytes()
                        .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?,
                )?;
                let merged = self
                    .language
                    .merge(previous, supply, &|| {
                        PcCandidatePageGuard::is_cancelled(guard)
                    })
                    .map_err(CompactGraphUnionError::Supply)?;
                self.retain_supply(merged.state_count(), merged.retained_state_capacity_bytes())?;
                self.next_layer.insert(key, merged);
                self.retained_supply_states -= old_len;
                self.retention.release(old_bytes)?;
                self.usage.merged_states += 1;
            } else {
                self.reserve_next_layer()?;
                self.authorize_frontier(supply.retained_state_capacity_bytes())?;
                let copy = supply.try_clone().map_err(CompactGraphUnionError::Supply)?;
                self.retain_supply(copy.state_count(), copy.retained_state_capacity_bytes())?;
                self.next_layer.insert(key, copy);
                self.usage.generated_states += 1;
            }
        }
        task.placement_index += 1;
        Ok(WorkStep::Continue)
    }

    fn retain_supply(&mut self, added: usize, bytes: usize) -> Result<(), CompactGraphUnionError> {
        let count = self
            .retained_supply_states
            .checked_add(added)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        self.retain_frontier(bytes)?;
        self.retained_supply_states = count;
        Ok(())
    }

    fn frontier_outer_bytes(&self) -> Result<usize, CompactGraphUnionError> {
        let ready = capacity_bytes::<Work>(self.ready.capacity())?;
        let waiting = capacity_bytes::<(u32, Vec<Work>)>(self.waiting.capacity())?;
        let next =
            capacity_bytes::<(StateKey, CompactPatternUnionFrontier)>(self.next_layer.capacity())?;
        let promotion = capacity_bytes::<(StateKey, CompactPatternUnionFrontier)>(
            self.promotion
                .as_ref()
                .map_or(0, |layer| layer.table_capacity),
        )?;
        let pins = capacity_bytes::<(u32, usize)>(self.pins.capacity())?;
        ready
            .checked_add(waiting)
            .and_then(|n| n.checked_add(next))
            .and_then(|n| n.checked_add(promotion))
            .and_then(|n| n.checked_add(pins))
            .ok_or_else(|| FrontierRetentionError::Overflow.into())
    }

    fn authorize_frontier(&self, additional: usize) -> Result<(), CompactGraphUnionError> {
        self.retention
            .authorize(self.frontier_outer_bytes()?, additional)?;
        Ok(())
    }

    fn pin_field(&mut self, field: u32) -> Result<(), CompactGraphUnionError> {
        if let Some(count) = self.pins.get_mut(&field) {
            *count = count
                .checked_add(1)
                .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
            return Ok(());
        }
        self.authorize_frontier(capacity_bytes::<(u32, usize)>(
            (self.pins.len() + 1).saturating_sub(self.pins.capacity()),
        )?)?;
        self.pins.try_reserve(1).map_err(|_| allocation())?;
        let outer = self.frontier_outer_bytes()?;
        self.retention.observe(outer)?;
        self.pins.insert(field, 1);
        Ok(())
    }

    fn unpin_field(&mut self, field: u32) -> Result<(), CompactGraphUnionError> {
        let count = self
            .pins
            .get_mut(&field)
            .ok_or_else(|| contract("pc4_compact_union_pin_accounting_failed"))?;
        *count = count
            .checked_sub(1)
            .ok_or_else(|| contract("pc4_compact_union_pin_accounting_failed"))?;
        if *count == 0 {
            self.pins.remove(&field);
        }
        Ok(())
    }

    fn retain_frontier(&mut self, bytes: usize) -> Result<(), CompactGraphUnionError> {
        let outer = self.frontier_outer_bytes()?;
        self.retention.retain(outer, bytes)?;
        Ok(())
    }

    // Pre-admit requested growth, then measure actual capacity immediately.
    // Hash table control/allocator metadata follows the existing logical
    // payload model, not an invented process RSS claim.
    fn reserve_ready(&mut self, additional: usize) -> Result<(), CompactGraphUnionError> {
        let requested = self
            .ready
            .len()
            .checked_add(additional)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        self.authorize_frontier(capacity_bytes::<Work>(
            requested.saturating_sub(self.ready.capacity()),
        )?)?;
        self.ready
            .try_reserve(additional)
            .map_err(|_| allocation())?;
        let outer = self.frontier_outer_bytes()?;
        self.retention.observe(outer)?;
        Ok(())
    }

    fn reserve_waiting_field(&mut self) -> Result<(), CompactGraphUnionError> {
        self.authorize_frontier(capacity_bytes::<(u32, Vec<Work>)>(
            (self.waiting.len() + 1).saturating_sub(self.waiting.capacity()),
        )?)?;
        self.waiting.try_reserve(1).map_err(|_| allocation())?;
        let outer = self.frontier_outer_bytes()?;
        self.retention.observe(outer)?;
        Ok(())
    }

    fn reserve_waiting_work(&mut self, id: u32) -> Result<(), CompactGraphUnionError> {
        let (old_capacity, requested) = self
            .waiting
            .get(&id)
            .map_or((0, 1), |tasks| (tasks.capacity(), tasks.len() + 1));
        self.authorize_frontier(capacity_bytes::<Work>(
            requested.saturating_sub(old_capacity),
        )?)?;
        let new_capacity = {
            let tasks = self.waiting.entry(id).or_default();
            tasks.try_reserve(1).map_err(|_| allocation())?;
            tasks.capacity()
        };
        self.retain_frontier(capacity_bytes::<Work>(new_capacity - old_capacity)?)?;
        Ok(())
    }

    fn reserve_next_layer(&mut self) -> Result<(), CompactGraphUnionError> {
        self.authorize_frontier(capacity_bytes::<(StateKey, CompactPatternUnionFrontier)>(
            (self.next_layer.len() + 1).saturating_sub(self.next_layer.capacity()),
        )?)?;
        self.next_layer.try_reserve(1).map_err(|_| allocation())?;
        let outer = self.frontier_outer_bytes()?;
        self.retention.observe(outer)?;
        Ok(())
    }

    fn accept_terminal(
        &mut self,
        layout: StandardBoard64TilingIdentity,
        field: u32,
    ) -> Result<(), CompactGraphUnionError> {
        if field != self.target.terminal_field().field_id() {
            return Ok(());
        }
        let occupied = layout
            .placement_masks()
            .iter()
            .fold(self.source.initial_board_mask(), |all, mask| all | mask);
        if layout.placement_count() != self.placement_count || occupied != self.full_mask {
            return Err(contract("pc4_compact_union_terminal_layout_mismatch"));
        }
        self.usage.terminal_arrivals += 1;
        if self.candidates.contains(&layout) {
            return Ok(());
        }
        let count = self
            .candidates
            .len()
            .checked_add(1)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        limit(
            "pc4_compact_union_candidate_limit",
            self.limits.candidates,
            count,
        )?;
        self.check_candidate_capacity(self.candidates.capacity().max(count), count)?;
        self.candidates.try_reserve(1).map_err(|_| allocation())?;
        self.check_candidate_capacity(self.candidates.capacity(), count)?;
        self.candidates.insert(layout);
        Ok(())
    }

    fn check_candidate_capacity(
        &self,
        table: usize,
        count: usize,
    ) -> Result<(), CompactGraphUnionError> {
        // Reserve both the retained set and the eventual exact output vector;
        // completing traversal must not discover a predictable copy cliff.
        let elements = table
            .checked_add(count)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        limit(
            "pc4_compact_union_candidate_byte_limit",
            self.limits.canonicalization_bytes,
            capacity_bytes::<StandardBoard64TilingIdentity>(elements)?,
        )
    }

    /// Consuming handoff only after complete traversal/materialization. There
    /// is no public raw-parts constructor and no observation/replay certificate.
    pub(crate) fn into_reducer_input<G: Pc4GraphCandidateGuard>(
        self,
        guard: &G,
    ) -> Result<PcCandidateReducerInput, CompactGraphUnionError> {
        check_guard(&self.source, &self.target, guard)?;
        if self.terminated
            || !self.completed
            || !self.ready.is_empty()
            || !self.waiting.is_empty()
            || !self.next_layer.is_empty()
            || self.promotion.is_some()
            || self.residents != 0
            || !self.pins.is_empty()
        {
            return Err(contract("pc4_compact_union_incomplete"));
        }
        let (candidates, digest) = self
            .canonicalizer
            .ok_or_else(|| contract("pc4_compact_union_incomplete"))?
            .into_parts()
            .map_err(CompactGraphUnionError::Canonicalization)?;
        check_guard(&self.source, &self.target, guard)?;
        let evidence = PcCandidateCompletenessEvidence::from_verified_complete_source(
            self.source,
            Some(self.target),
            candidates.len() as u64,
            digest,
        )
        .map_err(CompactGraphUnionError::Boundary)?;
        Ok(PcCandidateReducerInput {
            universe_identity: PcCandidateUniverseIdentity::from_complete_evidence(evidence)
                .map_err(CompactGraphUnionError::Boundary)?,
            candidates,
        })
    }
}

fn contract(reason: &'static str) -> CompactGraphUnionError {
    CompactGraphUnionError::Contract(reason)
}
fn allocation() -> CompactGraphUnionError {
    contract("pc4_compact_union_allocation_failed")
}
fn limit(
    kind: &'static str,
    maximum: NonZeroUsize,
    attempted: usize,
) -> Result<(), CompactGraphUnionError> {
    if attempted > maximum.get() {
        Err(CompactGraphUnionError::Limit {
            kind,
            limit: maximum.get(),
            attempted,
        })
    } else {
        Ok(())
    }
}
fn check_guard<G: Pc4GraphCandidateGuard>(
    source: &PcCandidateSourceBinding,
    target: &QualifiedPc4TargetIdentity,
    guard: &G,
) -> Result<(), CompactGraphUnionError> {
    if PcCandidatePageGuard::is_cancelled(guard) {
        return Err(contract("pc4_compact_union_cancelled"));
    }
    if !PcCandidatePageGuard::is_current_source(guard, source) {
        return Err(contract("pc4_compact_union_stale_source"));
    }
    if !PcCandidatePageGuard::is_current_snapshot(guard, target.snapshot()) {
        return Err(contract("pc4_compact_union_stale_snapshot"));
    }
    Ok(())
}
