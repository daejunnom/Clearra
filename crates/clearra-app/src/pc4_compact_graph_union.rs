//! SRP: enumerate the complete canonical graph x compact-supply union without
//! reveal histories. I/O, observation probabilities and product reduction stay
//! outside. Missing records suspend individual work items, never mean no edge.
use core::{convert::Infallible, num::NonZeroUsize};
use std::collections::{HashMap, HashSet, VecDeque};

use clearra_core_domain::{
    board::standard_pc_board::StandardPcBoard,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
};
use clearra_pc4_tablebase::{
    clearra_board64_mask_to_hydra_field_hash_v1, materialize_qualified_graph_edge,
    read_qualified_pc4_adjacency, ClearraPlacementIdentity, FixedQueueTraversalPageError,
    Pc4RowFrame, Pc4TerminalUseCase, PlacementMaterializationError, QualifiedPc4GraphEdge,
    QualifiedPc4TargetIdentity,
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

/// Logical retained-state/work limits, in addition to the separately bounded
/// graph cache and supply frontier. They never authorize a truncated union.
#[derive(Clone, Copy)]
pub(crate) struct CompactGraphUnionLimits {
    pub states: NonZeroUsize,
    pub supply_states: NonZeroUsize,
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
            Self::Limit { kind, .. } => kind,
        }
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
    pub materialized_edges: usize,
    pub peak_waiting_fields: usize,
    pub canonicalization_work: usize,
    pub peak_canonicalization_buffer_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct StateKey {
    layout: StandardBoard64TilingIdentity,
    field: u32,
    frame: Pc4RowFrame,
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
        }
    }

    fn supply_states(&self) -> usize {
        self.supply.state_count() + self.next_supply.as_ref().map_or(0, |s| s.state_count())
    }

    fn next_piece(&mut self) -> usize {
        let released = self.next_supply.take().map_or(0, |s| s.state_count());
        self.targets = None;
        self.placements = None;
        self.target_index = 0;
        self.placement_index = 0;
        self.piece += 1;
        released
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
    candidates: HashSet<StandardBoard64TilingIdentity>,
    canonicalizer: Option<CooperativeCandidateCanonicalizer>,
    retained_supply_states: usize,
    cache_records_seen: usize,
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
            && (self.canonicalizer.is_some() || !self.ready.is_empty() || self.waiting.is_empty())
    }

    /// Watermarks throttle new CPU demand before the hard retained-state
    /// limits are reached. They are counts, not whole-owner byte authority.
    pub(crate) fn io_demand_is_full(&self, fields: usize, continuations: usize) -> bool {
        // Before its first verified record the root is a real pending demand,
        // but has not been parked in `waiting` yet. Match pending_fields rather
        // than silently reporting zero demand for that bootstrap phase.
        let root = usize::from(!self.start_verified && !self.completed && !self.terminated);
        self.waiting.len() + root >= fields || self.waiting_count + root >= continuations
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
        if target.use_case() != Pc4TerminalUseCase::PcSearch
            || cache.target() != target
            || start_field >= cache.field_count()
        {
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
        limit(
            "pc4_compact_union_supply_state_limit",
            limits.supply_states,
            retained_supply_states,
        )?;
        let mut ready = VecDeque::new();
        ready.try_reserve(1).map_err(|_| allocation())?;
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
            candidates: HashSet::new(),
            canonicalizer: None,
            retained_supply_states,
            cache_records_seen: 0,
            completed: false,
            terminated: false,
            usage: CompactGraphUnionUsage::default(),
        }))
    }

    /// Bounded demand list: duplicates share one cache record. Exposing demands
    /// while CPU progress remains lets the host start I/O without a global wait.
    pub(crate) fn pending_fields(&self, maximum: usize) -> Vec<u32> {
        if self.terminated || self.completed || maximum == 0 {
            return Vec::new();
        }
        if !self.start_verified {
            return vec![self.start_field];
        }
        let mut ids: Vec<_> = self.waiting.keys().copied().collect();
        ids.sort_unstable();
        ids.truncate(maximum);
        ids
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
            self.ready.clear();
            self.waiting.clear();
            self.next_layer.clear();
            self.candidates.clear();
            self.canonicalizer = None;
            self.waiting_count = 0;
            self.retained_supply_states = 0;
        }
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
        if cache.usage().record_count() != self.cache_records_seen || self.ready.is_empty() {
            let available: Vec<_> = self
                .waiting
                .keys()
                .copied()
                .filter(|id| cache.contains_field_id(*id))
                .collect();
            for id in available {
                let tasks = self.waiting.remove(&id).expect("collected waiting field");
                self.ready
                    .try_reserve(tasks.len())
                    .map_err(|_| allocation())?;
                self.waiting_count -= tasks.len();
                self.ready.extend(tasks);
            }
            self.cache_records_seen = cache.usage().record_count();
        }
        for _ in 0..work.get() {
            check_guard(&self.source, &self.target, guard)?;
            if self.ready.is_empty() {
                if !self.waiting.is_empty() {
                    return Ok(CompactGraphUnionStep::Waiting);
                }
                if self.next_layer.is_empty() {
                    // No histories remain. Release queue backing stores before
                    // retaining both candidate buffers during canonicalization.
                    self.ready = VecDeque::new();
                    self.waiting = HashMap::new();
                    self.next_layer = HashMap::new();
                    self.canonicalizer = Some(
                        CooperativeCandidateCanonicalizer::begin(
                            core::mem::take(&mut self.candidates),
                            self.limits.canonicalization_bytes,
                        )
                        .map_err(CompactGraphUnionError::Canonicalization)?,
                    );
                    return Ok(CompactGraphUnionStep::Progress);
                }
                self.ready
                    .try_reserve(self.next_layer.len())
                    .map_err(|_| allocation())?;
                for (key, supply) in self.next_layer.drain() {
                    self.ready.push_back(Work::new(key, supply));
                }
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
            let mut task = self.ready.pop_front().expect("ready layer");
            match self.step_work(&mut task, cache, guard)? {
                WorkStep::Continue => self.ready.push_back(task),
                WorkStep::Done => self.retained_supply_states -= task.supply_states(),
                WorkStep::Missing(id) => {
                    if !self.waiting.contains_key(&id) {
                        limit(
                            "pc4_compact_union_waiting_field_limit",
                            self.limits.waiting_fields,
                            self.waiting.len() + 1,
                        )?;
                        self.waiting.try_reserve(1).map_err(|_| allocation())?;
                    }
                    let waiting = self.waiting.entry(id).or_default();
                    waiting.try_reserve(1).map_err(|_| allocation())?;
                    waiting.push(task);
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
            if task.key.field == self.target.terminal_field().field_id() {
                let occupied = task
                    .key
                    .layout
                    .placement_masks()
                    .iter()
                    .fold(self.source.initial_board_mask(), |all, mask| all | mask);
                if occupied != self.full_mask {
                    return Err(contract("pc4_compact_union_terminal_layout_mismatch"));
                }
                if !self.candidates.contains(&task.key.layout) {
                    limit(
                        "pc4_compact_union_candidate_limit",
                        self.limits.candidates,
                        self.candidates.len() + 1,
                    )?;
                    self.candidates.try_reserve(1).map_err(|_| allocation())?;
                    self.candidates.insert(task.key.layout);
                }
            }
            return Ok(WorkStep::Done);
        }
        if task.piece == PieceKind::STANDARD_TETROMINOES.len() {
            return Ok(WorkStep::Done);
        }
        let piece = PieceKind::STANDARD_TETROMINOES[task.piece];
        if task.next_supply.is_none() {
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
            self.retain_supply(next.state_count())?;
            task.next_supply = Some(next);
            return Ok(WorkStep::Continue);
        }
        if task.targets.is_none() {
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
            task.targets = Some(
                edges
                    .into_iter()
                    .map(|edge| edge.target_field_id())
                    .collect(),
            );
            return Ok(WorkStep::Continue);
        }
        let targets = task.targets.as_ref().expect("prepared piece adjacency");
        let Some(&target_field) = targets.get(task.target_index) else {
            self.retained_supply_states -= task.next_piece();
            return Ok(WorkStep::Continue);
        };
        if task.placements.is_none() {
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
            task.placements = Some(placements);
            return Ok(WorkStep::Continue);
        }
        let Some(&placement) = task
            .placements
            .as_ref()
            .expect("materialized edge")
            .get(task.placement_index)
        else {
            task.placements = None;
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
            if let Some(previous) = self.next_layer.get(&key) {
                let old_len = previous.state_count();
                let merged = self
                    .language
                    .merge(previous, supply, &|| {
                        PcCandidatePageGuard::is_cancelled(guard)
                    })
                    .map_err(CompactGraphUnionError::Supply)?;
                self.retain_supply(merged.state_count() - old_len)?;
                self.next_layer.insert(key, merged);
                self.usage.merged_states += 1;
            } else {
                limit(
                    "pc4_compact_union_state_limit",
                    self.limits.states,
                    self.ready.len() + self.waiting_count + self.next_layer.len() + 2,
                )?;
                self.retain_supply(supply.state_count())?;
                self.next_layer.try_reserve(1).map_err(|_| allocation())?;
                self.next_layer.insert(key, supply.clone());
                self.usage.generated_states += 1;
            }
        }
        task.placement_index += 1;
        Ok(WorkStep::Continue)
    }

    fn retain_supply(&mut self, added: usize) -> Result<(), CompactGraphUnionError> {
        let count = self
            .retained_supply_states
            .checked_add(added)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        limit(
            "pc4_compact_union_supply_state_limit",
            self.limits.supply_states,
            count,
        )?;
        self.retained_supply_states = count;
        Ok(())
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
