// SRP rationale: this module owns the target-qualified streaming seam between a PC4 graph
// family and canonical candidate pages while retaining every concrete replay provenance.
// Discovery-order observations are deliberately non-authoritative; only an exhausted stream
// can be sealed into canonical pages or reducer input.

use core::{convert::Infallible, fmt, num::NonZeroUsize};
use std::sync::Arc;

use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{
        NormalizedTilingSolutionError, PiecePlacementMask, StandardBoard64TilingIdentity,
    },
};
use clearra_pc4_tablebase::{
    prepare_fixed_queue_concrete_family, ClearraPlacementIdentity,
    ConcretePathMaterializationBudgetKind, ConcretePathMaterializationBudgets,
    ConcretePathMaterializationError, ConcretePathPageError, FixedQueueBudgetKind,
    FixedQueueConcretePath, FixedQueueConcretePathCursor, FixedQueueConcretePathFamily,
    FixedQueueGraphPath, FixedQueuePathMaterializationRequest, FixedQueueTerminalPredicate,
    FixedQueueTraversalCursor, FixedQueueTraversalError, FixedQueueTraversalFamily,
    FixedQueueTraversalGuard, FixedQueueTraversalPageError, MaterializationGuard, Pc4GraphPiece,
    Pc4PlacementMaterializer, QualifiedCompleteAdjacencyProvider, QualifiedPc4TargetIdentity,
};

#[cfg(test)]
use clearra_pc4_tablebase::{traverse_fixed_queue, FixedQueueTraversalRequest};

use super::{
    PcCandidateBoundaryError, PcCandidateCompletenessEvidence, PcCandidatePageCursor,
    PcCandidatePageGuard, PcCandidateProviderKind, PcCandidateReducerInput, PcCandidateSetDigest,
    PcCandidateSourceBinding, PcConcreteCandidatePage,
};

pub const PC4_GRAPH_CANDIDATE_ADAPTER_CONTRACT: &str =
    "pc4-target-qualified-complete-candidate-family.v1";

/// Terminal owner accepted by the candidate adapter.
///
/// The identity must name the exact terminal semantics carried by the verified
/// profile/use-case/target qualification. Merely returning `true` for a graph
/// state does not grant candidate-universe completeness.
pub trait QualifiedPc4CandidateTerminalPredicate: FixedQueueTerminalPredicate {
    fn target(&self) -> &QualifiedPc4TargetIdentity;

    fn terminal_semantics_identity(&self) -> &str;
}

pub trait Pc4GraphCandidateGuard:
    FixedQueueTraversalGuard + MaterializationGuard + PcCandidatePageGuard
{
}

impl<T> Pc4GraphCandidateGuard for T where
    T: FixedQueueTraversalGuard + MaterializationGuard + PcCandidatePageGuard
{
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4GraphCandidateAdapterBudgets {
    traversal_paths_per_page: NonZeroUsize,
    concrete_paths_per_page: NonZeroUsize,
    canonical_candidates: NonZeroUsize,
    replay_provenances: NonZeroUsize,
    placement_identities: NonZeroUsize,
    candidate_page_size: NonZeroUsize,
}

impl Pc4GraphCandidateAdapterBudgets {
    pub const fn new(
        traversal_paths_per_page: NonZeroUsize,
        concrete_paths_per_page: NonZeroUsize,
        canonical_candidates: NonZeroUsize,
        replay_provenances: NonZeroUsize,
        placement_identities: NonZeroUsize,
        candidate_page_size: NonZeroUsize,
    ) -> Self {
        Self {
            traversal_paths_per_page,
            concrete_paths_per_page,
            canonical_candidates,
            replay_provenances,
            placement_identities,
            candidate_page_size,
        }
    }

    pub const fn traversal_paths_per_page(self) -> usize {
        self.traversal_paths_per_page.get()
    }

    pub const fn concrete_paths_per_page(self) -> usize {
        self.concrete_paths_per_page.get()
    }

    pub const fn canonical_candidates(self) -> usize {
        self.canonical_candidates.get()
    }

    pub const fn replay_provenances(self) -> usize {
        self.replay_provenances.get()
    }

    pub const fn placement_identities(self) -> usize {
        self.placement_identities.get()
    }

    pub const fn candidate_page_size(self) -> usize {
        self.candidate_page_size.get()
    }
}

pub struct Pc4GraphCandidateAdapterRequest<'a> {
    target: &'a QualifiedPc4TargetIdentity,
    source: &'a PcCandidateSourceBinding,
    start_field_id: u32,
    materialization_budgets: ConcretePathMaterializationBudgets,
    adapter_budgets: Pc4GraphCandidateAdapterBudgets,
}

impl<'a> Pc4GraphCandidateAdapterRequest<'a> {
    pub const fn new(
        target: &'a QualifiedPc4TargetIdentity,
        source: &'a PcCandidateSourceBinding,
        start_field_id: u32,
        materialization_budgets: ConcretePathMaterializationBudgets,
        adapter_budgets: Pc4GraphCandidateAdapterBudgets,
    ) -> Self {
        Self {
            target,
            source,
            start_field_id,
            materialization_budgets,
            adapter_budgets,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4GraphCandidateBindingError {
    SourceIsNotOnlinePc4,
    SourceSnapshotMismatch,
    SourceProfileMismatch,
    TraversalTargetMismatch,
    TraversalStartFieldMismatch,
    ProviderTargetMismatch,
    TerminalTargetMismatch,
    TerminalSemanticsMismatch,
}

impl Pc4GraphCandidateBindingError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::SourceIsNotOnlinePc4 => "pc4_candidate_source_is_not_online_pc4",
            Self::SourceSnapshotMismatch => "pc4_candidate_source_snapshot_mismatch",
            Self::SourceProfileMismatch => "pc4_candidate_source_profile_mismatch",
            Self::TraversalTargetMismatch => "pc4_candidate_traversal_target_mismatch",
            Self::TraversalStartFieldMismatch => "pc4_candidate_traversal_start_field_mismatch",
            Self::ProviderTargetMismatch => "pc4_candidate_provider_target_mismatch",
            Self::TerminalTargetMismatch => "pc4_candidate_terminal_target_mismatch",
            Self::TerminalSemanticsMismatch => "pc4_candidate_terminal_semantics_mismatch",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4GraphCandidateBudgetKind {
    Traversal(FixedQueueBudgetKind),
    Materialization(ConcretePathMaterializationBudgetKind),
    CanonicalCandidates,
    ReplayProvenances,
    PlacementIdentities,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4GraphCandidateBudgetExceeded {
    pub kind: Pc4GraphCandidateBudgetKind,
    pub limit: usize,
    pub attempted: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError> {
    Cancelled,
    StaleSource,
    StaleSnapshot,
    Binding(Pc4GraphCandidateBindingError),
    BudgetExceeded(Pc4GraphCandidateBudgetExceeded),
    CounterOverflow,
    AllocationFailed,
    ObservationAlreadyExhausted,
    ObservationPageLimitExceeded { limit: usize, attempted: usize },
    IncompleteCannotFinalize,
    CandidateIdentity(NormalizedTilingSolutionError),
    Traversal(FixedQueueTraversalError<ProviderError, TerminalError>),
    TraversalPage(FixedQueueTraversalPageError<ProviderError, TerminalError>),
    Materialization(ConcretePathMaterializationError<MaterializerError>),
    ConcretePage(ConcretePathPageError),
    CandidateBoundary(PcCandidateBoundaryError),
}

impl<ProviderError, TerminalError, MaterializerError>
    Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError>
{
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_graph_candidate_prepare_cancelled",
            Self::StaleSource => "pc4_graph_candidate_prepare_stale_source",
            Self::StaleSnapshot => "pc4_graph_candidate_prepare_stale_snapshot",
            Self::Binding(error) => error.reason(),
            Self::BudgetExceeded(_) => "pc4_graph_candidate_prepare_budget_exceeded",
            Self::CounterOverflow => "pc4_graph_candidate_prepare_counter_overflow",
            Self::AllocationFailed => "pc4_graph_candidate_prepare_allocation_failed",
            Self::ObservationAlreadyExhausted => {
                "pc4_graph_candidate_observation_already_exhausted"
            }
            Self::ObservationPageLimitExceeded { .. } => {
                "pc4_graph_candidate_observation_page_limit_exceeded"
            }
            Self::IncompleteCannotFinalize => "pc4_graph_candidate_incomplete_cannot_finalize",
            Self::CandidateIdentity(_) => "pc4_graph_candidate_identity_invalid",
            Self::Traversal(error) => error.reason(),
            Self::TraversalPage(error) => error.reason(),
            Self::Materialization(error) => error.reason(),
            Self::ConcretePage(error) => error.reason(),
            Self::CandidateBoundary(error) => error.reason(),
        }
    }
}

impl<ProviderError, TerminalError, MaterializerError> fmt::Display
    for Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

pub type Pc4GraphCandidateObservationPageResult<ProviderError, TerminalError, MaterializerError> =
    Result<
        Pc4GraphCandidateObservationPage,
        Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError>,
    >;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Pc4ConcreteReplayProvenance {
    start_field_id: u32,
    terminal_field_id: u32,
    target_field_ids: Vec<u32>,
    placements: Vec<ClearraPlacementIdentity>,
}

impl Pc4ConcreteReplayProvenance {
    pub const fn start_field_id(&self) -> u32 {
        self.start_field_id
    }

    pub const fn terminal_field_id(&self) -> u32 {
        self.terminal_field_id
    }

    pub fn target_field_ids(&self) -> &[u32] {
        &self.target_field_ids
    }

    pub fn placements(&self) -> &[ClearraPlacementIdentity] {
        &self.placements
    }

    fn try_from_path(path: &FixedQueueConcretePath) -> Result<Self, Pc4GraphCandidatePageError> {
        let mut target_field_ids = Vec::new();
        target_field_ids
            .try_reserve_exact(path.target_field_ids().len())
            .map_err(|_| Pc4GraphCandidatePageError::AllocationFailed)?;
        target_field_ids.extend_from_slice(path.target_field_ids());
        let mut placements = Vec::new();
        placements
            .try_reserve_exact(path.placements().len())
            .map_err(|_| Pc4GraphCandidatePageError::AllocationFailed)?;
        placements.extend_from_slice(path.placements());
        Ok(Self {
            start_field_id: path.start_field_id(),
            terminal_field_id: path.terminal_field_id(),
            target_field_ids,
            placements,
        })
    }

    fn try_clone(&self) -> Result<Self, Pc4GraphCandidatePageError> {
        let mut target_field_ids = Vec::new();
        target_field_ids
            .try_reserve_exact(self.target_field_ids.len())
            .map_err(|_| Pc4GraphCandidatePageError::AllocationFailed)?;
        target_field_ids.extend_from_slice(&self.target_field_ids);
        let mut placements = Vec::new();
        placements
            .try_reserve_exact(self.placements.len())
            .map_err(|_| Pc4GraphCandidatePageError::AllocationFailed)?;
        placements.extend_from_slice(&self.placements);
        Ok(Self {
            start_field_id: self.start_field_id,
            terminal_field_id: self.terminal_field_id,
            target_field_ids,
            placements,
        })
    }
}

/// One discovery-order observation. Repeated candidate identities are
/// intentional: each record owns one distinct concrete replay provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4GraphCandidateObservation {
    identity: StandardBoard64TilingIdentity,
    provenance: Pc4ConcreteReplayProvenance,
}

impl Pc4GraphCandidateObservation {
    pub const fn identity(&self) -> StandardBoard64TilingIdentity {
        self.identity
    }

    pub const fn provenance(&self) -> &Pc4ConcreteReplayProvenance {
        &self.provenance
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4GraphCandidateObservationStatus {
    InProgress,
    ExhaustedAwaitingSeal,
}

/// Bounded first-seen traversal output. This page deliberately cannot be
/// converted into `PcConcreteCandidatePage` or reducer input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4GraphCandidateObservationPage {
    first_replay_ordinal: usize,
    observations: Vec<Pc4GraphCandidateObservation>,
    status: Pc4GraphCandidateObservationStatus,
}

impl Pc4GraphCandidateObservationPage {
    pub const fn first_replay_ordinal(&self) -> usize {
        self.first_replay_ordinal
    }

    pub fn observations(&self) -> &[Pc4GraphCandidateObservation] {
        &self.observations
    }

    pub const fn status(&self) -> Pc4GraphCandidateObservationStatus {
        self.status
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4CanonicalCandidate {
    identity: StandardBoard64TilingIdentity,
    replay_provenances: Vec<Pc4ConcreteReplayProvenance>,
}

impl Pc4CanonicalCandidate {
    pub const fn identity(&self) -> StandardBoard64TilingIdentity {
        self.identity
    }

    pub fn replay_provenances(&self) -> &[Pc4ConcreteReplayProvenance] {
        &self.replay_provenances
    }

    fn try_clone(&self) -> Result<Self, Pc4GraphCandidatePageError> {
        let mut replay_provenances = Vec::new();
        replay_provenances
            .try_reserve_exact(self.replay_provenances.len())
            .map_err(|_| Pc4GraphCandidatePageError::AllocationFailed)?;
        for provenance in &self.replay_provenances {
            replay_provenances.push(provenance.try_clone()?);
        }
        Ok(Self {
            identity: self.identity,
            replay_provenances,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Pc4GraphCandidatePageCursor {
    family_token: Arc<()>,
    boundary_cursor: PcCandidatePageCursor,
    candidate_index: usize,
    exhausted: bool,
}

impl Pc4GraphCandidatePageCursor {
    pub const fn candidate_index(&self) -> usize {
        self.candidate_index
    }

    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4GraphCandidatePage {
    candidate_page: PcConcreteCandidatePage,
    candidates: Vec<Pc4CanonicalCandidate>,
}

impl Pc4GraphCandidatePage {
    pub const fn candidate_page(&self) -> &PcConcreteCandidatePage {
        &self.candidate_page
    }

    pub fn candidates(&self) -> &[Pc4CanonicalCandidate] {
        &self.candidates
    }

    pub fn into_candidate_page(self) -> PcConcreteCandidatePage {
        self.candidate_page
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4GraphCandidatePageError {
    Cancelled,
    StaleSource,
    StaleSnapshot,
    CursorMismatch,
    AlreadyExhausted,
    PageLimitExceeded { limit: usize, attempted: usize },
    CounterOverflow,
    AllocationFailed,
    CandidateBoundary(PcCandidateBoundaryError),
}

impl Pc4GraphCandidatePageError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_graph_candidate_page_cancelled",
            Self::StaleSource => "pc4_graph_candidate_page_stale_source",
            Self::StaleSnapshot => "pc4_graph_candidate_page_stale_snapshot",
            Self::CursorMismatch => "pc4_graph_candidate_page_cursor_mismatch",
            Self::AlreadyExhausted => "pc4_graph_candidate_page_already_exhausted",
            Self::PageLimitExceeded { .. } => "pc4_graph_candidate_page_limit_exceeded",
            Self::CounterOverflow => "pc4_graph_candidate_page_counter_overflow",
            Self::AllocationFailed => "pc4_graph_candidate_page_allocation_failed",
            Self::CandidateBoundary(error) => error.reason(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Pc4GraphCandidateFamily {
    contract_id: &'static str,
    target: QualifiedPc4TargetIdentity,
    source: PcCandidateSourceBinding,
    candidates: Vec<Pc4CanonicalCandidate>,
    completeness: PcCandidateCompletenessEvidence,
    maximum_page_candidates: usize,
    replay_provenance_count: usize,
    cursor_token: Arc<()>,
}

impl Pc4GraphCandidateFamily {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn source(&self) -> &PcCandidateSourceBinding {
        &self.source
    }

    pub fn candidates(&self) -> &[Pc4CanonicalCandidate] {
        &self.candidates
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    pub const fn replay_provenance_count(&self) -> usize {
        self.replay_provenance_count
    }

    pub fn cursor(&self) -> Pc4GraphCandidatePageCursor {
        Pc4GraphCandidatePageCursor {
            family_token: Arc::clone(&self.cursor_token),
            boundary_cursor: PcCandidatePageCursor::initial(),
            candidate_index: 0,
            exhausted: false,
        }
    }

    pub fn next_page<G: PcCandidatePageGuard>(
        &self,
        cursor: &mut Pc4GraphCandidatePageCursor,
        limit: NonZeroUsize,
        guard: &G,
    ) -> Result<Pc4GraphCandidatePage, Pc4GraphCandidatePageError> {
        if !Arc::ptr_eq(&cursor.family_token, &self.cursor_token) {
            return Err(Pc4GraphCandidatePageError::CursorMismatch);
        }
        if cursor.exhausted {
            return Err(Pc4GraphCandidatePageError::AlreadyExhausted);
        }
        if limit.get() > self.maximum_page_candidates {
            return Err(Pc4GraphCandidatePageError::PageLimitExceeded {
                limit: self.maximum_page_candidates,
                attempted: limit.get(),
            });
        }
        check_page_guard(&self.source, guard)?;
        let end = cursor
            .candidate_index
            .checked_add(limit.get())
            .ok_or(Pc4GraphCandidatePageError::CounterOverflow)?
            .min(self.candidates.len());
        let source_candidates = self
            .candidates
            .get(cursor.candidate_index..end)
            .ok_or(Pc4GraphCandidatePageError::CursorMismatch)?;
        let mut candidates = Vec::new();
        candidates
            .try_reserve_exact(source_candidates.len())
            .map_err(|_| Pc4GraphCandidatePageError::AllocationFailed)?;
        let mut identities = Vec::new();
        identities
            .try_reserve_exact(source_candidates.len())
            .map_err(|_| Pc4GraphCandidatePageError::AllocationFailed)?;
        for candidate in source_candidates {
            identities.push(candidate.identity());
            candidates.push(candidate.try_clone()?);
        }
        let terminal = end == self.candidates.len();
        let candidate_page = if terminal {
            PcConcreteCandidatePage::complete(
                self.source.clone(),
                cursor.boundary_cursor,
                identities,
                self.completeness.clone(),
            )
        } else {
            PcConcreteCandidatePage::partial(
                self.source.clone(),
                cursor.boundary_cursor,
                identities,
                false,
            )
        }
        .map_err(Pc4GraphCandidatePageError::CandidateBoundary)?;
        check_page_guard(&self.source, guard)?;
        cursor.boundary_cursor = candidate_page.next_cursor();
        cursor.candidate_index = end;
        cursor.exhausted = terminal;
        Ok(Pc4GraphCandidatePage {
            candidate_page,
            candidates,
        })
    }

    pub fn try_reducer_input<G: PcCandidatePageGuard>(
        &self,
        guard: &G,
    ) -> Result<PcCandidateReducerInput, Pc4GraphCandidatePageError> {
        check_page_guard(&self.source, guard)?;
        let mut candidates = Vec::new();
        candidates
            .try_reserve_exact(self.candidates.len())
            .map_err(|_| Pc4GraphCandidatePageError::AllocationFailed)?;
        candidates.extend(self.candidates.iter().map(Pc4CanonicalCandidate::identity));
        check_page_guard(&self.source, guard)?;
        Ok(PcCandidateReducerInput {
            source: self.source.clone(),
            candidates,
        })
    }
}

struct PendingCandidate {
    identity: StandardBoard64TilingIdentity,
    provenance: Pc4ConcreteReplayProvenance,
}

#[derive(Clone)]
struct Pc4GraphCandidateBinding {
    target: QualifiedPc4TargetIdentity,
    source: PcCandidateSourceBinding,
    start_field_id: u32,
    materialization_budgets: ConcretePathMaterializationBudgets,
    adapter_budgets: Pc4GraphCandidateAdapterBudgets,
}

impl Pc4GraphCandidateBinding {
    fn from_request(request: Pc4GraphCandidateAdapterRequest<'_>) -> Self {
        Self {
            target: request.target.clone(),
            source: request.source.clone(),
            start_field_id: request.start_field_id,
            materialization_budgets: request.materialization_budgets,
            adapter_budgets: request.adapter_budgets,
        }
    }
}

struct PreparedCandidateBatch {
    pending: Vec<PendingCandidate>,
    new_candidate_identities: Vec<StandardBoard64TilingIdentity>,
    next_replay_count: usize,
    next_placement_count: usize,
}

struct CandidateAccumulator {
    pending: Vec<PendingCandidate>,
    seen_candidate_identities: Vec<StandardBoard64TilingIdentity>,
    replay_count: usize,
    placement_count: usize,
}

impl CandidateAccumulator {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            seen_candidate_identities: Vec::new(),
            replay_count: 0,
            placement_count: 0,
        }
    }

    fn prepare_batch<ProviderError, TerminalError, MaterializerError>(
        &self,
        binding: &Pc4GraphCandidateBinding,
        observations: &[Pc4GraphCandidateObservation],
    ) -> Result<
        PreparedCandidateBatch,
        Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError>,
    > {
        let next_replay_count = self
            .replay_count
            .checked_add(observations.len())
            .ok_or(Pc4GraphCandidatePrepareError::CounterOverflow)?;
        enforce_adapter_budget(
            Pc4GraphCandidateBudgetKind::ReplayProvenances,
            binding.adapter_budgets.replay_provenances(),
            next_replay_count,
        )?;
        let batch_placement_count =
            observations.iter().try_fold(0usize, |count, observation| {
                count
                    .checked_add(observation.provenance.placements.len())
                    .ok_or(Pc4GraphCandidatePrepareError::CounterOverflow)
            })?;
        let next_placement_count = self
            .placement_count
            .checked_add(batch_placement_count)
            .ok_or(Pc4GraphCandidatePrepareError::CounterOverflow)?;
        enforce_adapter_budget(
            Pc4GraphCandidateBudgetKind::PlacementIdentities,
            binding.adapter_budgets.placement_identities(),
            next_placement_count,
        )?;

        let mut new_candidate_identities = Vec::new();
        new_candidate_identities
            .try_reserve_exact(observations.len())
            .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
        new_candidate_identities
            .extend(observations.iter().map(|observation| observation.identity));
        new_candidate_identities.sort_unstable();
        new_candidate_identities.dedup();
        new_candidate_identities.retain(|identity| {
            self.seen_candidate_identities
                .binary_search(identity)
                .is_err()
        });
        let next_candidate_count = self
            .seen_candidate_identities
            .len()
            .checked_add(new_candidate_identities.len())
            .ok_or(Pc4GraphCandidatePrepareError::CounterOverflow)?;
        enforce_adapter_budget(
            Pc4GraphCandidateBudgetKind::CanonicalCandidates,
            binding.adapter_budgets.canonical_candidates(),
            next_candidate_count,
        )?;

        let mut pending = Vec::new();
        pending
            .try_reserve_exact(observations.len())
            .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
        for observation in observations {
            pending.push(PendingCandidate {
                identity: observation.identity,
                provenance: observation
                    .provenance
                    .try_clone()
                    .map_err(|error| match error {
                        Pc4GraphCandidatePageError::AllocationFailed => {
                            Pc4GraphCandidatePrepareError::AllocationFailed
                        }
                        _ => unreachable!("provenance cloning only allocates"),
                    })?,
            });
        }

        Ok(PreparedCandidateBatch {
            pending,
            new_candidate_identities,
            next_replay_count,
            next_placement_count,
        })
    }

    fn reserve_batch<ProviderError, TerminalError, MaterializerError>(
        &mut self,
        batch: &PreparedCandidateBatch,
    ) -> Result<(), Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError>>
    {
        self.pending
            .try_reserve_exact(batch.pending.len())
            .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
        self.seen_candidate_identities
            .try_reserve_exact(batch.new_candidate_identities.len())
            .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
        Ok(())
    }

    fn commit_batch(&mut self, batch: PreparedCandidateBatch) {
        self.pending.extend(batch.pending);
        self.seen_candidate_identities
            .extend(batch.new_candidate_identities);
        self.seen_candidate_identities.sort_unstable();
        self.replay_count = batch.next_replay_count;
        self.placement_count = batch.next_placement_count;
    }

    fn finish<ProviderError, TerminalError, MaterializerError, G>(
        mut self,
        binding: Pc4GraphCandidateBinding,
        guard: &G,
    ) -> Result<
        Pc4GraphCandidateFamily,
        Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError>,
    >
    where
        G: Pc4GraphCandidateGuard,
    {
        check_prepare_guard(&binding.source, guard)?;
        self.pending.sort_unstable_by(|left, right| {
            left.identity
                .cmp(&right.identity)
                .then_with(|| left.provenance.cmp(&right.provenance))
        });
        let candidate_count = self
            .pending
            .iter()
            .enumerate()
            .filter(|(index, candidate)| {
                *index == 0 || self.pending[*index - 1].identity != candidate.identity
            })
            .count();
        enforce_adapter_budget(
            Pc4GraphCandidateBudgetKind::CanonicalCandidates,
            binding.adapter_budgets.canonical_candidates(),
            candidate_count,
        )?;
        let mut candidates: Vec<Pc4CanonicalCandidate> = Vec::new();
        candidates
            .try_reserve_exact(candidate_count)
            .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
        for pending in self.pending {
            if candidates
                .last()
                .is_some_and(|candidate| candidate.identity == pending.identity)
            {
                let replays = &mut candidates
                    .last_mut()
                    .expect("last candidate was just observed")
                    .replay_provenances;
                replays
                    .try_reserve_exact(1)
                    .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
                replays.push(pending.provenance);
            } else {
                let mut replay_provenances = Vec::new();
                replay_provenances
                    .try_reserve_exact(1)
                    .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
                replay_provenances.push(pending.provenance);
                candidates.push(Pc4CanonicalCandidate {
                    identity: pending.identity,
                    replay_provenances,
                });
            }
        }
        let identities = candidates
            .iter()
            .map(Pc4CanonicalCandidate::identity)
            .collect::<Vec<_>>();
        let candidate_set_digest = PcCandidateSetDigest::calculate_parts(&identities, &[])
            .map_err(Pc4GraphCandidatePrepareError::CandidateBoundary)?;
        let exact_candidate_count = u64::try_from(candidates.len())
            .map_err(|_| Pc4GraphCandidatePrepareError::CounterOverflow)?;
        let completeness = PcCandidateCompletenessEvidence {
            source: binding.source.clone(),
            exact_candidate_count,
            candidate_set_digest,
        };
        check_prepare_guard(&binding.source, guard)?;
        Ok(Pc4GraphCandidateFamily {
            contract_id: PC4_GRAPH_CANDIDATE_ADAPTER_CONTRACT,
            target: binding.target,
            source: binding.source,
            candidates,
            completeness,
            maximum_page_candidates: binding.adapter_budgets.candidate_page_size(),
            replay_provenance_count: self.replay_count,
            cursor_token: Arc::new(()),
        })
    }
}

#[derive(Clone)]
struct ActiveConcretePathFamily {
    family: Arc<FixedQueueConcretePathFamily>,
    cursor: FixedQueueConcretePathCursor,
}

/// Target-qualified, output-sensitive traversal state. Preparing this session
/// performs no provider, terminal, or placement-materializer callback.
pub struct Pc4GraphCandidateStreamSession {
    binding: Pc4GraphCandidateBinding,
    graph_family: FixedQueueTraversalFamily,
    graph_cursor: FixedQueueTraversalCursor,
    pending_graph_paths: Vec<FixedQueueGraphPath>,
    active_concrete_family: Option<ActiveConcretePathFamily>,
    accumulator: CandidateAccumulator,
}

impl Pc4GraphCandidateStreamSession {
    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.binding.target
    }

    pub const fn source(&self) -> &PcCandidateSourceBinding {
        &self.binding.source
    }

    pub fn observed_replay_count(&self) -> usize {
        self.accumulator.replay_count
    }

    pub fn observed_candidate_count(&self) -> usize {
        self.accumulator.seen_candidate_identities.len()
    }

    /// Upper bound on placement-materializer callbacks made by one observation
    /// page. A graph path cannot exceed the per-path edge budget, and at most
    /// one traversal-page worth of graph paths is prepared by one call.
    pub fn maximum_materializer_callbacks_per_observation_page(&self) -> usize {
        self.binding
            .adapter_budgets
            .traversal_paths_per_page()
            .checked_mul(self.binding.materialization_budgets.graph_edges())
            .expect("callback bound overflow is rejected while preparing the stream")
    }

    pub fn is_exhausted(&self) -> bool {
        self.graph_cursor.is_exhausted()
            && self.pending_graph_paths.is_empty()
            && self.active_concrete_family.is_none()
    }

    pub fn next_observation_page<P, T, M, G>(
        &mut self,
        limit: NonZeroUsize,
        provider: &mut P,
        terminal_predicate: &mut T,
        materializer: &mut M,
        guard: &G,
    ) -> Pc4GraphCandidateObservationPageResult<P::Error, T::Error, M::Error>
    where
        P: QualifiedCompleteAdjacencyProvider,
        T: QualifiedPc4CandidateTerminalPredicate,
        M: Pc4PlacementMaterializer,
        G: Pc4GraphCandidateGuard,
    {
        if self.is_exhausted() {
            return Err(Pc4GraphCandidatePrepareError::ObservationAlreadyExhausted);
        }
        if limit.get() > self.binding.adapter_budgets.candidate_page_size() {
            return Err(
                Pc4GraphCandidatePrepareError::ObservationPageLimitExceeded {
                    limit: self.binding.adapter_budgets.candidate_page_size(),
                    attempted: limit.get(),
                },
            );
        }
        validate_stream_call_binding(&self.binding, provider, terminal_predicate, guard)?;

        let first_replay_ordinal = self.accumulator.replay_count;
        let mut observations = Vec::new();
        observations
            .try_reserve_exact(limit.get())
            .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
        let mut next_graph_cursor = self.graph_cursor.clone();
        let mut next_pending_graph_paths = self.pending_graph_paths.clone();
        let mut next_active_concrete_family = self.active_concrete_family.clone();
        let mut graph_page_advanced = false;
        let mut graph_paths_materialized = 0usize;

        while observations.len() < limit.get() {
            check_prepare_guard(&self.binding.source, guard)?;

            if let Some(active) = next_active_concrete_family.as_mut() {
                let remaining = limit.get() - observations.len();
                let concrete_limit =
                    remaining.min(self.binding.adapter_budgets.concrete_paths_per_page());
                let concrete_limit = NonZeroUsize::new(concrete_limit)
                    .expect("remaining observation capacity is non-zero");
                let mut next_cursor = active.cursor.clone();
                let concrete_paths = active
                    .family
                    .next_page(&mut next_cursor, concrete_limit, guard)
                    .map_err(map_concrete_page_error)?;
                let concrete_exhausted = next_cursor.is_exhausted();
                let batch_observations = build_observations::<P::Error, T::Error, M::Error>(
                    &self.binding,
                    &concrete_paths,
                )?;
                active.cursor = next_cursor;
                observations.extend(batch_observations);
                if concrete_exhausted {
                    next_active_concrete_family = None;
                }
                continue;
            }

            if graph_paths_materialized == self.binding.adapter_budgets.traversal_paths_per_page() {
                break;
            }

            if let Some(graph_path) = next_pending_graph_paths.last() {
                let concrete_family = prepare_fixed_queue_concrete_family(
                    FixedQueuePathMaterializationRequest::new(
                        &self.binding.target,
                        graph_path,
                        self.binding.materialization_budgets,
                    ),
                    materializer,
                    guard,
                )
                .map_err(map_materialization_error)?;
                check_prepare_guard(&self.binding.source, guard)?;
                next_pending_graph_paths.pop();
                let cursor = concrete_family.cursor();
                next_active_concrete_family = Some(ActiveConcretePathFamily {
                    family: Arc::new(concrete_family),
                    cursor,
                });
                graph_paths_materialized = graph_paths_materialized
                    .checked_add(1)
                    .ok_or(Pc4GraphCandidatePrepareError::CounterOverflow)?;
                continue;
            }

            if next_graph_cursor.is_exhausted() || graph_page_advanced {
                break;
            }

            let mut staged_graph_cursor = next_graph_cursor.clone();
            let graph_page = self
                .graph_family
                .next_page(
                    &mut staged_graph_cursor,
                    self.binding.adapter_budgets.traversal_paths_per_page,
                    provider,
                    terminal_predicate,
                    guard,
                )
                .map_err(map_traversal_page_error)?;
            next_pending_graph_paths
                .try_reserve_exact(graph_page.paths().len())
                .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
            let mut next_paths = Vec::new();
            next_paths
                .try_reserve_exact(graph_page.paths().len())
                .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
            next_paths.extend(graph_page.paths().iter().cloned());
            check_prepare_guard(&self.binding.source, guard)?;
            next_graph_cursor = staged_graph_cursor;
            next_pending_graph_paths.extend(next_paths.into_iter().rev());
            graph_page_advanced = true;

            if graph_page.paths().is_empty() {
                break;
            }
        }

        let prepared = self
            .accumulator
            .prepare_batch(&self.binding, &observations)?;
        self.accumulator.reserve_batch(&prepared)?;
        check_prepare_guard(&self.binding.source, guard)?;
        let exhausted = next_graph_cursor.is_exhausted()
            && next_pending_graph_paths.is_empty()
            && next_active_concrete_family.is_none();
        self.accumulator.commit_batch(prepared);
        self.graph_cursor = next_graph_cursor;
        self.pending_graph_paths = next_pending_graph_paths;
        self.active_concrete_family = next_active_concrete_family;
        let status = if exhausted {
            Pc4GraphCandidateObservationStatus::ExhaustedAwaitingSeal
        } else {
            Pc4GraphCandidateObservationStatus::InProgress
        };
        Ok(Pc4GraphCandidateObservationPage {
            first_replay_ordinal,
            observations,
            status,
        })
    }

    pub fn finish<G>(
        self,
        guard: &G,
    ) -> Result<Pc4GraphCandidateFamily, Pc4GraphCandidateSessionError>
    where
        G: Pc4GraphCandidateGuard,
    {
        if !self.is_exhausted() {
            return Err(Pc4GraphCandidatePrepareError::IncompleteCannotFinalize);
        }
        self.accumulator
            .finish::<Infallible, Infallible, Infallible, _>(self.binding, guard)
    }
}

pub type Pc4GraphCandidateSessionError =
    Pc4GraphCandidatePrepareError<Infallible, Infallible, Infallible>;

pub fn prepare_pc4_graph_candidate_stream<G>(
    request: Pc4GraphCandidateAdapterRequest<'_>,
    graph_family: &FixedQueueTraversalFamily,
    guard: &G,
) -> Result<Pc4GraphCandidateStreamSession, Pc4GraphCandidateSessionError>
where
    G: Pc4GraphCandidateGuard,
{
    validate_source_binding(&request, guard)?;
    if graph_family.target() != request.target {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::TraversalTargetMismatch,
        ));
    }
    if graph_family.start_field_id() != request.start_field_id {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::TraversalStartFieldMismatch,
        ));
    }
    if request.adapter_budgets.traversal_paths_per_page()
        > graph_family.page_budgets().output_paths()
    {
        return Err(Pc4GraphCandidatePrepareError::BudgetExceeded(
            Pc4GraphCandidateBudgetExceeded {
                kind: Pc4GraphCandidateBudgetKind::Traversal(FixedQueueBudgetKind::OutputPaths),
                limit: graph_family.page_budgets().output_paths(),
                attempted: request.adapter_budgets.traversal_paths_per_page(),
            },
        ));
    }
    request
        .adapter_budgets
        .traversal_paths_per_page()
        .checked_mul(request.materialization_budgets.graph_edges())
        .ok_or(Pc4GraphCandidatePrepareError::CounterOverflow)?;
    let binding = Pc4GraphCandidateBinding::from_request(request);
    Ok(Pc4GraphCandidateStreamSession {
        graph_cursor: graph_family.cursor(),
        graph_family: graph_family.clone(),
        binding,
        pending_graph_paths: Vec::new(),
        active_concrete_family: None,
        accumulator: CandidateAccumulator::new(),
    })
}

#[cfg(test)]
fn prepare_pc4_graph_candidate_family_from_eager_reference<P, T, M, G>(
    request: Pc4GraphCandidateAdapterRequest<'_>,
    traversal_request: FixedQueueTraversalRequest<'_>,
    provider: &mut P,
    terminal_predicate: &mut T,
    materializer: &mut M,
    guard: &G,
) -> Result<Pc4GraphCandidateFamily, Pc4GraphCandidatePrepareError<P::Error, T::Error, M::Error>>
where
    P: QualifiedCompleteAdjacencyProvider,
    T: QualifiedPc4CandidateTerminalPredicate,
    M: Pc4PlacementMaterializer,
    G: Pc4GraphCandidateGuard,
{
    validate_common_binding(&request, terminal_predicate, guard)?;
    if traversal_request.target() != request.target {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::TraversalTargetMismatch,
        ));
    }
    if traversal_request.start_field_id() != request.start_field_id {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::TraversalStartFieldMismatch,
        ));
    }
    let traversal = traverse_fixed_queue(traversal_request, provider, terminal_predicate, guard)
        .map_err(map_traversal_error)?;
    let binding = Pc4GraphCandidateBinding::from_request(request);
    let mut accumulator = CandidateAccumulator::new();
    for graph_path in traversal.paths() {
        let concrete_family = prepare_fixed_queue_concrete_family(
            FixedQueuePathMaterializationRequest::new(
                &binding.target,
                graph_path,
                binding.materialization_budgets,
            ),
            materializer,
            guard,
        )
        .map_err(map_materialization_error)?;
        let mut cursor = concrete_family.cursor();
        while !cursor.is_exhausted() {
            let concrete_paths = concrete_family
                .next_page(
                    &mut cursor,
                    binding.adapter_budgets.concrete_paths_per_page,
                    guard,
                )
                .map_err(map_concrete_page_error)?;
            let observations =
                build_observations::<P::Error, T::Error, M::Error>(&binding, &concrete_paths)?;
            let prepared = accumulator.prepare_batch(&binding, &observations)?;
            accumulator.reserve_batch(&prepared)?;
            check_prepare_guard(&binding.source, guard)?;
            accumulator.commit_batch(prepared);
        }
    }
    accumulator.finish(binding, guard)
}

fn validate_common_binding<ProviderError, T, MaterializerError, G>(
    request: &Pc4GraphCandidateAdapterRequest<'_>,
    terminal_predicate: &T,
    guard: &G,
) -> Result<(), Pc4GraphCandidatePrepareError<ProviderError, T::Error, MaterializerError>>
where
    T: QualifiedPc4CandidateTerminalPredicate,
    G: Pc4GraphCandidateGuard,
{
    validate_source_binding(request, guard)?;
    if terminal_predicate.target() != request.target {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::TerminalTargetMismatch,
        ));
    }
    if terminal_predicate.terminal_semantics_identity()
        != request.target.qualification().terminal_semantics_identity()
    {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::TerminalSemanticsMismatch,
        ));
    }
    Ok(())
}

fn validate_source_binding<ProviderError, TerminalError, MaterializerError, G>(
    request: &Pc4GraphCandidateAdapterRequest<'_>,
    guard: &G,
) -> Result<(), Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError>>
where
    G: Pc4GraphCandidateGuard,
{
    check_prepare_guard(request.source, guard)?;
    if request.source.provider_kind() != PcCandidateProviderKind::OnlinePc4 {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::SourceIsNotOnlinePc4,
        ));
    }
    if request.source.qualified_snapshot() != Some(request.target.snapshot()) {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::SourceSnapshotMismatch,
        ));
    }
    if request.source.profile() != request.target.profile() {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::SourceProfileMismatch,
        ));
    }
    Ok(())
}

fn validate_stream_call_binding<ProviderError, T, MaterializerError, P, G>(
    binding: &Pc4GraphCandidateBinding,
    provider: &P,
    terminal_predicate: &T,
    guard: &G,
) -> Result<(), Pc4GraphCandidatePrepareError<ProviderError, T::Error, MaterializerError>>
where
    P: QualifiedCompleteAdjacencyProvider<Error = ProviderError>,
    T: QualifiedPc4CandidateTerminalPredicate,
    G: Pc4GraphCandidateGuard,
{
    check_prepare_guard(&binding.source, guard)?;
    if provider.target() != &binding.target {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::ProviderTargetMismatch,
        ));
    }
    if terminal_predicate.target() != &binding.target {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::TerminalTargetMismatch,
        ));
    }
    if terminal_predicate.terminal_semantics_identity()
        != binding.target.qualification().terminal_semantics_identity()
    {
        return Err(Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::TerminalSemanticsMismatch,
        ));
    }
    Ok(())
}

fn build_observations<ProviderError, TerminalError, MaterializerError>(
    binding: &Pc4GraphCandidateBinding,
    paths: &[FixedQueueConcretePath],
) -> Result<
    Vec<Pc4GraphCandidateObservation>,
    Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError>,
> {
    let mut observations = Vec::new();
    observations
        .try_reserve_exact(paths.len())
        .map_err(|_| Pc4GraphCandidatePrepareError::AllocationFailed)?;
    for path in paths {
        if path.target_field_ids().len() != path.placements().len()
            || path.start_field_id() != binding.start_field_id
        {
            return Err(Pc4GraphCandidatePrepareError::Binding(
                Pc4GraphCandidateBindingError::TraversalStartFieldMismatch,
            ));
        }
        let identity = StandardBoard64TilingIdentity::from_placements(
            binding.source.initial_board_mask(),
            path.placements().iter().copied().map(|placement| {
                PiecePlacementMask::new(piece_kind(placement.piece()), placement.occupied_cells())
            }),
        )
        .map_err(Pc4GraphCandidatePrepareError::CandidateIdentity)?;
        let provenance =
            Pc4ConcreteReplayProvenance::try_from_path(path).map_err(|error| match error {
                Pc4GraphCandidatePageError::AllocationFailed => {
                    Pc4GraphCandidatePrepareError::AllocationFailed
                }
                _ => unreachable!("provenance construction only allocates"),
            })?;
        observations.push(Pc4GraphCandidateObservation {
            identity,
            provenance,
        });
    }
    Ok(observations)
}

fn check_prepare_guard<ProviderError, TerminalError, MaterializerError, G>(
    source: &PcCandidateSourceBinding,
    guard: &G,
) -> Result<(), Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError>>
where
    G: Pc4GraphCandidateGuard,
{
    if PcCandidatePageGuard::is_cancelled(guard) {
        return Err(Pc4GraphCandidatePrepareError::Cancelled);
    }
    if !guard.is_current_source(source) {
        return Err(Pc4GraphCandidatePrepareError::StaleSource);
    }
    if source
        .qualified_snapshot()
        .is_some_and(|snapshot| !PcCandidatePageGuard::is_current_snapshot(guard, snapshot))
    {
        return Err(Pc4GraphCandidatePrepareError::StaleSnapshot);
    }
    Ok(())
}

fn check_page_guard<G: PcCandidatePageGuard>(
    source: &PcCandidateSourceBinding,
    guard: &G,
) -> Result<(), Pc4GraphCandidatePageError> {
    if guard.is_cancelled() {
        return Err(Pc4GraphCandidatePageError::Cancelled);
    }
    if !guard.is_current_source(source) {
        return Err(Pc4GraphCandidatePageError::StaleSource);
    }
    if source
        .qualified_snapshot()
        .is_some_and(|snapshot| !guard.is_current_snapshot(snapshot))
    {
        return Err(Pc4GraphCandidatePageError::StaleSnapshot);
    }
    Ok(())
}

fn enforce_adapter_budget<ProviderError, TerminalError, MaterializerError>(
    kind: Pc4GraphCandidateBudgetKind,
    limit: usize,
    attempted: usize,
) -> Result<(), Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError>> {
    if attempted > limit {
        Err(Pc4GraphCandidatePrepareError::BudgetExceeded(
            Pc4GraphCandidateBudgetExceeded {
                kind,
                limit,
                attempted,
            },
        ))
    } else {
        Ok(())
    }
}

fn map_traversal_error<ProviderError, TerminalError, MaterializerError>(
    error: FixedQueueTraversalError<ProviderError, TerminalError>,
) -> Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError> {
    match error {
        FixedQueueTraversalError::Cancelled => Pc4GraphCandidatePrepareError::Cancelled,
        FixedQueueTraversalError::StaleSnapshot => Pc4GraphCandidatePrepareError::StaleSnapshot,
        FixedQueueTraversalError::BudgetExceeded(error) => {
            Pc4GraphCandidatePrepareError::BudgetExceeded(Pc4GraphCandidateBudgetExceeded {
                kind: Pc4GraphCandidateBudgetKind::Traversal(error.kind),
                limit: error.limit,
                attempted: error.attempted,
            })
        }
        other => Pc4GraphCandidatePrepareError::Traversal(other),
    }
}

fn map_traversal_page_error<ProviderError, TerminalError, MaterializerError>(
    error: FixedQueueTraversalPageError<ProviderError, TerminalError>,
) -> Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError> {
    match error {
        FixedQueueTraversalPageError::Cancelled => Pc4GraphCandidatePrepareError::Cancelled,
        FixedQueueTraversalPageError::StaleSnapshot => Pc4GraphCandidatePrepareError::StaleSnapshot,
        FixedQueueTraversalPageError::CounterOverflow => {
            Pc4GraphCandidatePrepareError::CounterOverflow
        }
        FixedQueueTraversalPageError::AllocationFailed => {
            Pc4GraphCandidatePrepareError::AllocationFailed
        }
        FixedQueueTraversalPageError::BudgetExceeded(error) => {
            Pc4GraphCandidatePrepareError::BudgetExceeded(Pc4GraphCandidateBudgetExceeded {
                kind: Pc4GraphCandidateBudgetKind::Traversal(error.kind),
                limit: error.limit,
                attempted: error.attempted,
            })
        }
        other => Pc4GraphCandidatePrepareError::TraversalPage(other),
    }
}

fn map_materialization_error<ProviderError, TerminalError, MaterializerError>(
    error: ConcretePathMaterializationError<MaterializerError>,
) -> Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError> {
    match error {
        ConcretePathMaterializationError::Cancelled => Pc4GraphCandidatePrepareError::Cancelled,
        ConcretePathMaterializationError::StaleSnapshot => {
            Pc4GraphCandidatePrepareError::StaleSnapshot
        }
        ConcretePathMaterializationError::BudgetExceeded {
            kind,
            limit,
            actual,
            ..
        } => Pc4GraphCandidatePrepareError::BudgetExceeded(Pc4GraphCandidateBudgetExceeded {
            kind: Pc4GraphCandidateBudgetKind::Materialization(kind),
            limit,
            attempted: actual,
        }),
        ConcretePathMaterializationError::Edge {
            source: clearra_pc4_tablebase::PlacementMaterializationError::Cancelled,
            ..
        } => Pc4GraphCandidatePrepareError::Cancelled,
        ConcretePathMaterializationError::Edge {
            source: clearra_pc4_tablebase::PlacementMaterializationError::StaleSnapshot,
            ..
        } => Pc4GraphCandidatePrepareError::StaleSnapshot,
        other => Pc4GraphCandidatePrepareError::Materialization(other),
    }
}

fn map_concrete_page_error<ProviderError, TerminalError, MaterializerError>(
    error: ConcretePathPageError,
) -> Pc4GraphCandidatePrepareError<ProviderError, TerminalError, MaterializerError> {
    match error {
        ConcretePathPageError::Cancelled => Pc4GraphCandidatePrepareError::Cancelled,
        ConcretePathPageError::StaleSnapshot => Pc4GraphCandidatePrepareError::StaleSnapshot,
        other => Pc4GraphCandidatePrepareError::ConcretePage(other),
    }
}

const fn piece_kind(piece: Pc4GraphPiece) -> PieceKind {
    match piece {
        Pc4GraphPiece::I => PieceKind::I,
        Pc4GraphPiece::O => PieceKind::O,
        Pc4GraphPiece::T => PieceKind::T,
        Pc4GraphPiece::S => PieceKind::S,
        Pc4GraphPiece::Z => PieceKind::Z,
        Pc4GraphPiece::J => PieceKind::J,
        Pc4GraphPiece::L => PieceKind::L,
    }
}

#[cfg(test)]
#[path = "pc4_graph_candidate_adapter_tests.rs"]
mod tests;
