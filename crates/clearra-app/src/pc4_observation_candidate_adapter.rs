// SRP rationale: this module owns only the pure, target-qualified conversion from a
// completely exhausted observation-graph path family to canonical concrete candidate
// evidence. It does not own transport, fallback, observation policy, probability
// reduction, product activation, or presentation.

use core::{convert::Infallible, fmt, num::NonZeroUsize};
use std::sync::Arc;

use clearra_core_domain::{
    board::standard_pc_board::StandardPcBoard,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{
        NormalizedTilingSolutionError, PiecePlacementMask, StandardBoard64TilingIdentity,
    },
};
use clearra_pc4_tablebase::{
    prepare_fixed_queue_concrete_family, ClearraPlacementIdentity,
    ConcretePathMaterializationBudgets, ConcretePathMaterializationError, ConcretePathPageError,
    FixedQueueConcretePath, FixedQueueConcretePathCursor, FixedQueueConcretePathFamily,
    FixedQueueHoldState, FixedQueueHoldStep, FixedQueuePathMaterializationRequest,
    FixedQueueTraversalGuard, MaterializationGuard, Pc4BagState, Pc4ExactProbability,
    Pc4GraphPiece, Pc4ObservationGraphCursor, Pc4ObservationGraphFamily,
    Pc4ObservationGraphPageError, Pc4ObservationGraphPath, Pc4ObservationRevealLedgerCursor,
    Pc4ObservationRevealLedgerFamily, Pc4ObservationRevealLedgerPageError,
    Pc4ObservationRevealOutcome, Pc4PlacementMaterializer, QualifiedCompleteAdjacencyProvider,
    QualifiedPc4TargetIdentity,
};

use super::pc_candidate_page_boundary::{
    PcCandidateBoundaryError, PcCandidatePageGuard, PcCandidateProviderKind,
    PcCandidateReducerInput, PcCandidateRequestIdentity, PcCandidateSourceBinding,
};
use crate::pc4_input_disclosure_policy::{Pc4PreparedOnlineInput, Pc4PreparedQueueInput};

pub use super::pc_candidate_page_boundary::graph_candidate_adapter::{
    ManifestQualifiedPc4Terminal as ManifestQualifiedPc4ObservationTerminal,
    QualifiedPc4CandidateTerminalPredicate as QualifiedPc4ObservationCandidateTerminalPredicate,
};

pub const PC4_OBSERVATION_CANDIDATE_FAMILY_CONTRACT: &str =
    "pc4-complete-observation-candidate-family.v1";

/// Combined freshness/cancellation boundary required by graph traversal,
/// concrete placement materialization, and the request-bound candidate source.
pub trait Pc4ObservationCandidateGuard:
    FixedQueueTraversalGuard + MaterializationGuard + PcCandidatePageGuard
{
}

impl<T> Pc4ObservationCandidateGuard for T where
    T: FixedQueueTraversalGuard + MaterializationGuard + PcCandidatePageGuard
{
}

/// Finite work slices plus retained-family limits.
///
/// `graph_paths_per_advance`, `concrete_paths_per_page`, and
/// `discoveries_per_advance` bound one call. The remaining values bound the
/// retained complete family across all calls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4ObservationCandidateBudgets {
    graph_paths_per_advance: NonZeroUsize,
    concrete_paths_per_page: NonZeroUsize,
    discoveries_per_advance: NonZeroUsize,
    reveal_outcomes: NonZeroUsize,
    candidate_memberships: NonZeroUsize,
    replay_provenances: NonZeroUsize,
    retained_elements: NonZeroUsize,
}

impl Pc4ObservationCandidateBudgets {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        graph_paths_per_advance: NonZeroUsize,
        concrete_paths_per_page: NonZeroUsize,
        discoveries_per_advance: NonZeroUsize,
        reveal_outcomes: NonZeroUsize,
        candidate_memberships: NonZeroUsize,
        replay_provenances: NonZeroUsize,
        retained_elements: NonZeroUsize,
    ) -> Self {
        Self {
            graph_paths_per_advance,
            concrete_paths_per_page,
            discoveries_per_advance,
            reveal_outcomes,
            candidate_memberships,
            replay_provenances,
            retained_elements,
        }
    }

    pub const fn graph_paths_per_advance(self) -> usize {
        self.graph_paths_per_advance.get()
    }

    pub const fn concrete_paths_per_page(self) -> usize {
        self.concrete_paths_per_page.get()
    }

    pub const fn discoveries_per_advance(self) -> usize {
        self.discoveries_per_advance.get()
    }

    pub const fn reveal_outcomes(self) -> usize {
        self.reveal_outcomes.get()
    }

    pub const fn candidate_memberships(self) -> usize {
        self.candidate_memberships.get()
    }

    pub const fn replay_provenances(self) -> usize {
        self.replay_provenances.get()
    }

    pub const fn retained_elements(self) -> usize {
        self.retained_elements.get()
    }
}

pub struct Pc4ObservationCandidateAdapterRequest<'a> {
    target: &'a QualifiedPc4TargetIdentity,
    prepared_input: &'a Pc4PreparedOnlineInput,
    source: &'a PcCandidateSourceBinding,
    source_field_id: u32,
    materialization_budgets: ConcretePathMaterializationBudgets,
    adapter_budgets: Pc4ObservationCandidateBudgets,
}

impl<'a> Pc4ObservationCandidateAdapterRequest<'a> {
    pub const fn new(
        target: &'a QualifiedPc4TargetIdentity,
        prepared_input: &'a Pc4PreparedOnlineInput,
        source: &'a PcCandidateSourceBinding,
        source_field_id: u32,
        materialization_budgets: ConcretePathMaterializationBudgets,
        adapter_budgets: Pc4ObservationCandidateBudgets,
    ) -> Self {
        Self {
            target,
            prepared_input,
            source,
            source_field_id,
            materialization_budgets,
            adapter_budgets,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4ObservationCandidateBindingError {
    SourceIsNotOnlinePc4,
    SourceSnapshotMismatch,
    SourceProfileMismatch,
    PreparedInputTargetMismatch,
    PreparedInputNotPatternOrHidden,
    QueueScopeMismatch,
    RequestIdentityMismatch,
    InitialBoardMismatch,
    GraphTargetMismatch,
    GraphSourceFieldMismatch,
    ProviderTargetMismatch,
    TerminalTargetMismatch,
    TerminalSemanticsMismatch,
    MaterializerProfileMismatch,
    ConcreteSourceFieldMismatch,
    ConcretePathLengthMismatch,
}

impl Pc4ObservationCandidateBindingError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::SourceIsNotOnlinePc4 => "pc4_observation_candidate_source_is_not_online_pc4",
            Self::SourceSnapshotMismatch => "pc4_observation_candidate_source_snapshot_mismatch",
            Self::SourceProfileMismatch => "pc4_observation_candidate_source_profile_mismatch",
            Self::PreparedInputTargetMismatch => {
                "pc4_observation_candidate_prepared_input_target_mismatch"
            }
            Self::PreparedInputNotPatternOrHidden => {
                "pc4_observation_candidate_prepared_input_not_pattern_or_hidden"
            }
            Self::QueueScopeMismatch => "pc4_observation_candidate_queue_scope_mismatch",
            Self::RequestIdentityMismatch => "pc4_observation_candidate_request_identity_mismatch",
            Self::InitialBoardMismatch => "pc4_observation_candidate_initial_board_mismatch",
            Self::GraphTargetMismatch => "pc4_observation_candidate_graph_target_mismatch",
            Self::GraphSourceFieldMismatch => {
                "pc4_observation_candidate_graph_source_field_mismatch"
            }
            Self::ProviderTargetMismatch => "pc4_observation_candidate_provider_target_mismatch",
            Self::TerminalTargetMismatch => "pc4_observation_candidate_terminal_target_mismatch",
            Self::TerminalSemanticsMismatch => {
                "pc4_observation_candidate_terminal_semantics_mismatch"
            }
            Self::MaterializerProfileMismatch => {
                "pc4_observation_candidate_materializer_profile_mismatch"
            }
            Self::ConcreteSourceFieldMismatch => {
                "pc4_observation_candidate_concrete_source_field_mismatch"
            }
            Self::ConcretePathLengthMismatch => {
                "pc4_observation_candidate_concrete_path_length_mismatch"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4ObservationCandidateSemanticError {
    InconsistentRevealEvidence { reveal_rank: u128 },
}

impl Pc4ObservationCandidateSemanticError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::InconsistentRevealEvidence { .. } => {
                "pc4_observation_candidate_inconsistent_reveal_evidence"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4ObservationCandidateBudgetKind {
    RevealOutcomes,
    CandidateMemberships,
    ReplayProvenances,
    RetainedElements,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4ObservationCandidateBudgetExceeded {
    pub kind: Pc4ObservationCandidateBudgetKind,
    pub limit: usize,
    pub attempted: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError> {
    Cancelled,
    StaleSource,
    StaleSnapshot,
    Binding(Pc4ObservationCandidateBindingError),
    Semantic(Pc4ObservationCandidateSemanticError),
    BudgetExceeded(Pc4ObservationCandidateBudgetExceeded),
    CounterOverflow,
    AllocationFailed,
    AlreadyExhausted,
    AdvanceLimitExceeded { limit: usize, attempted: usize },
    IncompleteCannotFinalize,
    CandidateIdentity(NormalizedTilingSolutionError),
    ObservationGraph(Pc4ObservationGraphPageError<ProviderError, TerminalError>),
    RevealLedger(Pc4ObservationRevealLedgerPageError),
    Materialization(ConcretePathMaterializationError<MaterializerError>),
    ConcretePage(ConcretePathPageError),
}

impl<ProviderError, TerminalError, MaterializerError>
    Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>
{
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_observation_candidate_cancelled",
            Self::StaleSource => "pc4_observation_candidate_stale_source",
            Self::StaleSnapshot => "pc4_observation_candidate_stale_snapshot",
            Self::Binding(error) => error.reason(),
            Self::Semantic(error) => error.reason(),
            Self::BudgetExceeded(_) => "pc4_observation_candidate_budget_exceeded",
            Self::CounterOverflow => "pc4_observation_candidate_counter_overflow",
            Self::AllocationFailed => "pc4_observation_candidate_allocation_failed",
            Self::AlreadyExhausted => "pc4_observation_candidate_already_exhausted",
            Self::AdvanceLimitExceeded { .. } => "pc4_observation_candidate_advance_limit_exceeded",
            Self::IncompleteCannotFinalize => {
                "pc4_observation_candidate_incomplete_cannot_finalize"
            }
            Self::CandidateIdentity(_) => "pc4_observation_candidate_identity_invalid",
            Self::ObservationGraph(error) => error.reason(),
            Self::RevealLedger(error) => error.reason(),
            Self::Materialization(error) => error.reason(),
            Self::ConcretePage(error) => error.reason(),
        }
    }
}

impl<ProviderError, TerminalError, MaterializerError> fmt::Display
    for Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

pub type Pc4ObservationCandidateSessionError =
    Pc4ObservationCandidateError<Infallible, Infallible, Infallible>;

/// Exact random evidence for one canonical hidden reveal, including reveals
/// whose complete graph traversal has no terminal path.
///
/// Probability is deliberately owned here, once per canonical reveal rank.
/// Candidate and hold provenance types contain no probability field, so
/// controllable hold siblings cannot be mistaken for independent random mass.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Pc4ObservationRevealEvidence {
    reveal_rank: u128,
    revealed_pieces: Vec<Pc4GraphPiece>,
    probability: Pc4ExactProbability,
    terminal_bag_state: Pc4BagState,
}

impl Pc4ObservationRevealEvidence {
    pub const fn reveal_rank(&self) -> u128 {
        self.reveal_rank
    }

    /// Exact hidden sequence provenance. Observation-policy code must use each
    /// provenance's `terminal_observation` instead of reading hidden suffixes.
    pub fn revealed_pieces(&self) -> &[Pc4GraphPiece] {
        &self.revealed_pieces
    }

    pub const fn probability(&self) -> Pc4ExactProbability {
        self.probability
    }

    pub const fn terminal_bag_state(&self) -> Pc4BagState {
        self.terminal_bag_state
    }
}

/// One exact concrete replay witness for a canonical candidate.
///
/// Hold decisions remain controllable provenance. This type intentionally has
/// no reveal-probability member.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Pc4ObservationCandidateProvenance {
    frontier_entry_index: u128,
    hold_path_index: usize,
    placement_queue: Vec<Pc4GraphPiece>,
    hold_steps: Vec<FixedQueueHoldStep>,
    terminal_cursor: usize,
    terminal_hold: FixedQueueHoldState,
    terminal_observation: Vec<Pc4GraphPiece>,
    terminal_observation_is_complete: bool,
    source_field_id: u32,
    terminal_field_id: u32,
    target_field_ids: Vec<u32>,
    placements: Vec<ClearraPlacementIdentity>,
}

impl Pc4ObservationCandidateProvenance {
    pub const fn frontier_entry_index(&self) -> u128 {
        self.frontier_entry_index
    }

    pub const fn hold_path_index(&self) -> usize {
        self.hold_path_index
    }

    pub fn placement_queue(&self) -> &[Pc4GraphPiece] {
        &self.placement_queue
    }

    pub fn hold_steps(&self) -> &[FixedQueueHoldStep] {
        &self.hold_steps
    }

    pub const fn terminal_cursor(&self) -> usize {
        self.terminal_cursor
    }

    pub const fn terminal_hold(&self) -> FixedQueueHoldState {
        self.terminal_hold
    }

    /// Current piece plus the configured preview window. This is the only
    /// queue slice intended for a future observation-policy composition.
    pub fn terminal_observation(&self) -> &[Pc4GraphPiece] {
        &self.terminal_observation
    }

    pub const fn terminal_observation_is_complete(&self) -> bool {
        self.terminal_observation_is_complete
    }

    pub const fn source_field_id(&self) -> u32 {
        self.source_field_id
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ObservationCanonicalCandidate {
    identity: StandardBoard64TilingIdentity,
    provenances: Vec<Pc4ObservationCandidateProvenance>,
}

impl Pc4ObservationCanonicalCandidate {
    pub const fn identity(&self) -> StandardBoard64TilingIdentity {
        self.identity
    }

    pub fn provenances(&self) -> &[Pc4ObservationCandidateProvenance] {
        &self.provenances
    }
}

/// Canonical candidates supported by one mutually exclusive reveal sequence.
/// The exact reveal probability appears once even when multiple hold or graph
/// witnesses support the same or different candidates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ObservationCandidateOutcome {
    reveal: Pc4ObservationRevealEvidence,
    candidates: Vec<Pc4ObservationCanonicalCandidate>,
}

impl Pc4ObservationCandidateOutcome {
    pub const fn reveal(&self) -> &Pc4ObservationRevealEvidence {
        &self.reveal
    }

    pub fn candidates(&self) -> &[Pc4ObservationCanonicalCandidate] {
        &self.candidates
    }
}

/// Provider-neutral, canonical candidate evidence minted only after the source
/// observation graph and every concrete path product have been exhausted.
///
/// Every reveal rank from the graph-derived ledger is present exactly once.
/// Empty `candidates` is therefore exact zero-solution evidence, not omission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4CompleteObservationCandidateFamily {
    contract_id: &'static str,
    target: QualifiedPc4TargetIdentity,
    source: PcCandidateSourceBinding,
    canonical_candidates: Vec<StandardBoard64TilingIdentity>,
    reveal_outcomes: Vec<Pc4ObservationCandidateOutcome>,
    total_reveal_probability: Pc4ExactProbability,
    replay_provenance_count: usize,
    retained_element_count: usize,
}

impl Pc4CompleteObservationCandidateFamily {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn source(&self) -> &PcCandidateSourceBinding {
        &self.source
    }

    /// Canonical union across every reveal outcome.
    pub fn canonical_candidates(&self) -> &[StandardBoard64TilingIdentity] {
        &self.canonical_candidates
    }

    pub fn reveal_outcomes(&self) -> &[Pc4ObservationCandidateOutcome] {
        &self.reveal_outcomes
    }

    /// Compatibility name retained for the feature-gated pre-ledger surface.
    /// The returned slice now also contains zero-solution outcomes.
    pub fn successful_reveals(&self) -> &[Pc4ObservationCandidateOutcome] {
        self.reveal_outcomes()
    }

    pub const fn total_reveal_probability(&self) -> Pc4ExactProbability {
        self.total_reveal_probability
    }

    pub const fn replay_provenance_count(&self) -> usize {
        self.replay_provenance_count
    }

    pub const fn retained_element_count(&self) -> usize {
        self.retained_element_count
    }

    /// Clones only the canonical union into the shared reducer seam. The
    /// complete family remains available so exact per-outcome probability and
    /// provenance evidence are not collapsed into reducer input.
    pub fn reducer_input(&self) -> Result<PcCandidateReducerInput, PcCandidateBoundaryError> {
        PcCandidateReducerInput::from_complete_observation_union(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4ObservationCandidateAdvanceStatus {
    InProgress,
    ExhaustedAwaitingFinalize,
}

/// Bounded progress only. Candidate identities are withheld until finalization
/// so a partial observation page cannot be mistaken for a complete universe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4ObservationCandidateAdvance {
    discovered_concrete_paths: usize,
    observed_concrete_paths: usize,
    observed_reveal_outcomes: usize,
    observed_candidate_memberships: usize,
    status: Pc4ObservationCandidateAdvanceStatus,
}

impl Pc4ObservationCandidateAdvance {
    pub const fn discovered_concrete_paths(self) -> usize {
        self.discovered_concrete_paths
    }

    pub const fn observed_concrete_paths(self) -> usize {
        self.observed_concrete_paths
    }

    pub const fn observed_successful_reveals(self) -> usize {
        self.observed_reveal_outcomes
    }

    pub const fn observed_reveal_outcomes(self) -> usize {
        self.observed_reveal_outcomes
    }

    pub const fn observed_candidate_memberships(self) -> usize {
        self.observed_candidate_memberships
    }

    pub const fn status(self) -> Pc4ObservationCandidateAdvanceStatus {
        self.status
    }
}

#[derive(Clone)]
struct ObservationCandidateBinding {
    target: QualifiedPc4TargetIdentity,
    source: PcCandidateSourceBinding,
    source_field_id: u32,
    materialization_budgets: ConcretePathMaterializationBudgets,
    adapter_budgets: Pc4ObservationCandidateBudgets,
}

impl ObservationCandidateBinding {
    fn from_request(request: Pc4ObservationCandidateAdapterRequest<'_>) -> Self {
        Self {
            target: request.target.clone(),
            source: request.source.clone(),
            source_field_id: request.source_field_id,
            materialization_budgets: request.materialization_budgets,
            adapter_budgets: request.adapter_budgets,
        }
    }
}

#[derive(Clone)]
struct ActiveConcretePathFamily {
    observation_path: Pc4ObservationGraphPath,
    family: Arc<FixedQueueConcretePathFamily>,
    cursor: FixedQueueConcretePathCursor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingCandidate {
    reveal: Pc4ObservationRevealEvidence,
    identity: StandardBoard64TilingIdentity,
    provenance: Pc4ObservationCandidateProvenance,
}

impl PendingCandidate {
    fn retained_elements(&self) -> Option<usize> {
        [
            self.reveal.revealed_pieces.len(),
            self.provenance.placement_queue.len(),
            self.provenance.hold_steps.len(),
            self.provenance.terminal_observation.len(),
            self.provenance.target_field_ids.len(),
            self.provenance.placements.len(),
        ]
        .into_iter()
        .try_fold(0usize, usize::checked_add)
    }
}

struct PreparedBatch {
    pending: Vec<PendingCandidate>,
    new_path_reveals: Vec<Pc4ObservationRevealEvidence>,
    new_ledger_reveals: Vec<Pc4ObservationRevealEvidence>,
    new_memberships: Vec<(u128, StandardBoard64TilingIdentity)>,
    next_replay_count: usize,
    next_retained_element_count: usize,
}

struct CandidateAccumulator {
    pending: Vec<PendingCandidate>,
    seen_path_reveals: Vec<Pc4ObservationRevealEvidence>,
    ledger_reveals: Vec<Pc4ObservationRevealEvidence>,
    seen_memberships: Vec<(u128, StandardBoard64TilingIdentity)>,
    replay_count: usize,
    retained_element_count: usize,
}

impl CandidateAccumulator {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            seen_path_reveals: Vec::new(),
            ledger_reveals: Vec::new(),
            seen_memberships: Vec::new(),
            replay_count: 0,
            retained_element_count: 0,
        }
    }

    fn prepare_batch<ProviderError, TerminalError, MaterializerError>(
        &self,
        binding: &ObservationCandidateBinding,
        pending: Vec<PendingCandidate>,
        mut ledger_reveals: Vec<Pc4ObservationRevealEvidence>,
    ) -> Result<
        PreparedBatch,
        Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>,
    > {
        let next_replay_count = self
            .replay_count
            .checked_add(pending.len())
            .ok_or(Pc4ObservationCandidateError::CounterOverflow)?;
        enforce_budget(
            Pc4ObservationCandidateBudgetKind::ReplayProvenances,
            binding.adapter_budgets.replay_provenances(),
            next_replay_count,
        )?;

        let batch_retained_elements = pending.iter().try_fold(0usize, |count, candidate| {
            count
                .checked_add(
                    candidate
                        .retained_elements()
                        .ok_or(Pc4ObservationCandidateError::CounterOverflow)?,
                )
                .ok_or(Pc4ObservationCandidateError::CounterOverflow)
        })?;
        let batch_retained_elements =
            ledger_reveals
                .iter()
                .try_fold(batch_retained_elements, |count, reveal| {
                    count
                        .checked_add(reveal.revealed_pieces.len())
                        .ok_or(Pc4ObservationCandidateError::CounterOverflow)
                })?;
        let next_retained_element_count = self
            .retained_element_count
            .checked_add(batch_retained_elements)
            .ok_or(Pc4ObservationCandidateError::CounterOverflow)?;
        enforce_budget(
            Pc4ObservationCandidateBudgetKind::RetainedElements,
            binding.adapter_budgets.retained_elements(),
            next_retained_element_count,
        )?;

        let mut batch_reveals = Vec::new();
        batch_reveals
            .try_reserve_exact(pending.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        batch_reveals.extend(pending.iter().map(|candidate| candidate.reveal.clone()));
        batch_reveals.sort_unstable_by_key(Pc4ObservationRevealEvidence::reveal_rank);
        for pair in batch_reveals.windows(2) {
            if pair[0].reveal_rank == pair[1].reveal_rank && pair[0] != pair[1] {
                return Err(Pc4ObservationCandidateError::Semantic(
                    Pc4ObservationCandidateSemanticError::InconsistentRevealEvidence {
                        reveal_rank: pair[0].reveal_rank,
                    },
                ));
            }
        }
        batch_reveals.dedup_by_key(|reveal| reveal.reveal_rank);

        let mut new_path_reveals = Vec::new();
        new_path_reveals
            .try_reserve_exact(batch_reveals.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        for reveal in batch_reveals {
            match self
                .seen_path_reveals
                .binary_search_by_key(&reveal.reveal_rank, |seen| seen.reveal_rank)
            {
                Ok(index) if self.seen_path_reveals[index] != reveal => {
                    return Err(Pc4ObservationCandidateError::Semantic(
                        Pc4ObservationCandidateSemanticError::InconsistentRevealEvidence {
                            reveal_rank: reveal.reveal_rank,
                        },
                    ));
                }
                Ok(_) => {}
                Err(_) => new_path_reveals.push(reveal),
            }
        }

        ledger_reveals.sort_unstable_by_key(Pc4ObservationRevealEvidence::reveal_rank);
        for pair in ledger_reveals.windows(2) {
            if pair[0].reveal_rank == pair[1].reveal_rank {
                return Err(Pc4ObservationCandidateError::Semantic(
                    Pc4ObservationCandidateSemanticError::InconsistentRevealEvidence {
                        reveal_rank: pair[0].reveal_rank,
                    },
                ));
            }
        }
        let mut new_ledger_reveals = Vec::new();
        new_ledger_reveals
            .try_reserve_exact(ledger_reveals.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        for reveal in ledger_reveals {
            match self
                .ledger_reveals
                .binary_search_by_key(&reveal.reveal_rank, |seen| seen.reveal_rank)
            {
                Ok(_) => {
                    return Err(Pc4ObservationCandidateError::Semantic(
                        Pc4ObservationCandidateSemanticError::InconsistentRevealEvidence {
                            reveal_rank: reveal.reveal_rank,
                        },
                    ));
                }
                Err(_) => new_ledger_reveals.push(reveal),
            }
        }

        for reveal in new_path_reveals.iter().chain(self.seen_path_reveals.iter()) {
            if let Ok(index) = self
                .ledger_reveals
                .binary_search_by_key(&reveal.reveal_rank, |seen| seen.reveal_rank)
            {
                if self.ledger_reveals[index] != *reveal {
                    return Err(Pc4ObservationCandidateError::Semantic(
                        Pc4ObservationCandidateSemanticError::InconsistentRevealEvidence {
                            reveal_rank: reveal.reveal_rank,
                        },
                    ));
                }
            }
            if let Ok(index) = new_ledger_reveals
                .binary_search_by_key(&reveal.reveal_rank, |seen| seen.reveal_rank)
            {
                if new_ledger_reveals[index] != *reveal {
                    return Err(Pc4ObservationCandidateError::Semantic(
                        Pc4ObservationCandidateSemanticError::InconsistentRevealEvidence {
                            reveal_rank: reveal.reveal_rank,
                        },
                    ));
                }
            }
        }
        let next_reveal_count = self
            .ledger_reveals
            .len()
            .checked_add(new_ledger_reveals.len())
            .ok_or(Pc4ObservationCandidateError::CounterOverflow)?;
        enforce_budget(
            Pc4ObservationCandidateBudgetKind::RevealOutcomes,
            binding.adapter_budgets.reveal_outcomes(),
            next_reveal_count,
        )?;

        let mut batch_memberships = Vec::new();
        batch_memberships
            .try_reserve_exact(pending.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        batch_memberships.extend(
            pending
                .iter()
                .map(|candidate| (candidate.reveal.reveal_rank, candidate.identity)),
        );
        batch_memberships.sort_unstable();
        batch_memberships.dedup();
        let mut new_memberships = Vec::new();
        new_memberships
            .try_reserve_exact(batch_memberships.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        new_memberships.extend(
            batch_memberships
                .into_iter()
                .filter(|membership| self.seen_memberships.binary_search(membership).is_err()),
        );
        let next_membership_count = self
            .seen_memberships
            .len()
            .checked_add(new_memberships.len())
            .ok_or(Pc4ObservationCandidateError::CounterOverflow)?;
        enforce_budget(
            Pc4ObservationCandidateBudgetKind::CandidateMemberships,
            binding.adapter_budgets.candidate_memberships(),
            next_membership_count,
        )?;

        Ok(PreparedBatch {
            pending,
            new_path_reveals,
            new_ledger_reveals,
            new_memberships,
            next_replay_count,
            next_retained_element_count,
        })
    }

    fn reserve_batch<ProviderError, TerminalError, MaterializerError>(
        &mut self,
        batch: &PreparedBatch,
    ) -> Result<(), Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>>
    {
        self.pending
            .try_reserve_exact(batch.pending.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        self.seen_path_reveals
            .try_reserve_exact(batch.new_path_reveals.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        self.ledger_reveals
            .try_reserve_exact(batch.new_ledger_reveals.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        self.seen_memberships
            .try_reserve_exact(batch.new_memberships.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        Ok(())
    }

    fn commit_batch(&mut self, batch: PreparedBatch) {
        self.pending.extend(batch.pending);
        self.seen_path_reveals.extend(batch.new_path_reveals);
        self.seen_path_reveals
            .sort_unstable_by_key(Pc4ObservationRevealEvidence::reveal_rank);
        self.ledger_reveals.extend(batch.new_ledger_reveals);
        self.ledger_reveals
            .sort_unstable_by_key(Pc4ObservationRevealEvidence::reveal_rank);
        self.seen_memberships.extend(batch.new_memberships);
        self.seen_memberships.sort_unstable();
        self.replay_count = batch.next_replay_count;
        self.retained_element_count = batch.next_retained_element_count;
    }

    fn finish<ProviderError, TerminalError, MaterializerError, G>(
        mut self,
        binding: ObservationCandidateBinding,
        total_reveal_probability: Pc4ExactProbability,
        guard: &G,
    ) -> Result<
        Pc4CompleteObservationCandidateFamily,
        Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>,
    >
    where
        G: Pc4ObservationCandidateGuard,
    {
        check_guard(&binding.source, guard)?;
        if total_reveal_probability != Pc4ExactProbability::one() {
            return Err(Pc4ObservationCandidateError::IncompleteCannotFinalize);
        }
        for path_reveal in &self.seen_path_reveals {
            let index = self
                .ledger_reveals
                .binary_search_by_key(&path_reveal.reveal_rank, |reveal| reveal.reveal_rank)
                .map_err(|_| {
                    Pc4ObservationCandidateError::Semantic(
                        Pc4ObservationCandidateSemanticError::InconsistentRevealEvidence {
                            reveal_rank: path_reveal.reveal_rank,
                        },
                    )
                })?;
            if self.ledger_reveals[index] != *path_reveal {
                return Err(Pc4ObservationCandidateError::Semantic(
                    Pc4ObservationCandidateSemanticError::InconsistentRevealEvidence {
                        reveal_rank: path_reveal.reveal_rank,
                    },
                ));
            }
        }
        self.pending.sort_unstable_by(|left, right| {
            left.reveal
                .reveal_rank
                .cmp(&right.reveal.reveal_rank)
                .then_with(|| left.identity.cmp(&right.identity))
                .then_with(|| left.provenance.cmp(&right.provenance))
        });

        let mut reveal_outcomes: Vec<Pc4ObservationCandidateOutcome> = Vec::new();
        reveal_outcomes
            .try_reserve_exact(self.ledger_reveals.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        reveal_outcomes.extend(self.ledger_reveals.into_iter().map(|reveal| {
            Pc4ObservationCandidateOutcome {
                reveal,
                candidates: Vec::new(),
            }
        }));
        for pending in self.pending {
            let outcome_index = reveal_outcomes
                .binary_search_by_key(&pending.reveal.reveal_rank, |outcome| {
                    outcome.reveal.reveal_rank
                })
                .map_err(|_| {
                    Pc4ObservationCandidateError::Semantic(
                        Pc4ObservationCandidateSemanticError::InconsistentRevealEvidence {
                            reveal_rank: pending.reveal.reveal_rank,
                        },
                    )
                })?;
            let outcome = &mut reveal_outcomes[outcome_index];
            if outcome
                .candidates
                .last()
                .is_some_and(|candidate| candidate.identity == pending.identity)
            {
                outcome
                    .candidates
                    .last_mut()
                    .expect("a candidate was just observed")
                    .provenances
                    .try_reserve_exact(1)
                    .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
                outcome
                    .candidates
                    .last_mut()
                    .expect("a candidate was just observed")
                    .provenances
                    .push(pending.provenance);
            } else {
                outcome
                    .candidates
                    .try_reserve_exact(1)
                    .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
                let mut provenances = Vec::new();
                provenances
                    .try_reserve_exact(1)
                    .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
                provenances.push(pending.provenance);
                outcome.candidates.push(Pc4ObservationCanonicalCandidate {
                    identity: pending.identity,
                    provenances,
                });
            }
        }

        let mut canonical_candidates = Vec::new();
        canonical_candidates
            .try_reserve_exact(self.seen_memberships.len())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        canonical_candidates.extend(
            reveal_outcomes
                .iter()
                .flat_map(|outcome| outcome.candidates.iter())
                .map(Pc4ObservationCanonicalCandidate::identity),
        );
        canonical_candidates.sort_unstable();
        canonical_candidates.dedup();

        check_guard(&binding.source, guard)?;
        Ok(Pc4CompleteObservationCandidateFamily {
            contract_id: PC4_OBSERVATION_CANDIDATE_FAMILY_CONTRACT,
            target: binding.target,
            source: binding.source,
            canonical_candidates,
            reveal_outcomes,
            total_reveal_probability,
            replay_provenance_count: self.replay_count,
            retained_element_count: self.retained_element_count,
        })
    }
}

/// Resumable no-I/O composition runtime. The session owns no provider and
/// exposes no candidate identities before complete exhaustion.
pub struct Pc4ObservationCandidateSession {
    binding: ObservationCandidateBinding,
    graph_family: Pc4ObservationGraphFamily,
    graph_cursor: Pc4ObservationGraphCursor,
    reveal_ledger_family: Pc4ObservationRevealLedgerFamily,
    reveal_ledger_cursor: Pc4ObservationRevealLedgerCursor,
    complete_reveal_probability: Option<Pc4ExactProbability>,
    pending_graph_paths: Vec<Pc4ObservationGraphPath>,
    active_concrete_family: Option<ActiveConcretePathFamily>,
    accumulator: CandidateAccumulator,
}

impl Pc4ObservationCandidateSession {
    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.binding.target
    }

    pub const fn source(&self) -> &PcCandidateSourceBinding {
        &self.binding.source
    }

    pub fn observed_concrete_path_count(&self) -> usize {
        self.accumulator.replay_count
    }

    pub fn observed_successful_reveal_count(&self) -> usize {
        self.accumulator.ledger_reveals.len()
    }

    pub fn observed_reveal_outcome_count(&self) -> usize {
        self.accumulator.ledger_reveals.len()
    }

    pub fn observed_candidate_membership_count(&self) -> usize {
        self.accumulator.seen_memberships.len()
    }

    pub fn is_exhausted(&self) -> bool {
        self.graph_cursor.is_exhausted()
            && self.reveal_ledger_cursor.is_exhausted()
            && self.complete_reveal_probability.is_some()
            && self.pending_graph_paths.is_empty()
            && self.active_concrete_family.is_none()
    }

    pub fn advance<P, T, M, G>(
        &mut self,
        limit: NonZeroUsize,
        provider: &mut P,
        terminal_predicate: &mut T,
        materializer: &mut M,
        guard: &G,
    ) -> Result<
        Pc4ObservationCandidateAdvance,
        Pc4ObservationCandidateError<P::Error, T::Error, M::Error>,
    >
    where
        P: QualifiedCompleteAdjacencyProvider,
        T: QualifiedPc4ObservationCandidateTerminalPredicate,
        M: Pc4PlacementMaterializer,
        G: Pc4ObservationCandidateGuard,
    {
        if self.is_exhausted() {
            return Err(Pc4ObservationCandidateError::AlreadyExhausted);
        }
        if limit.get() > self.binding.adapter_budgets.discoveries_per_advance() {
            return Err(Pc4ObservationCandidateError::AdvanceLimitExceeded {
                limit: self.binding.adapter_budgets.discoveries_per_advance(),
                attempted: limit.get(),
            });
        }
        validate_call_binding(
            &self.binding,
            provider,
            terminal_predicate,
            materializer,
            guard,
        )?;

        let mut discoveries = Vec::new();
        discoveries
            .try_reserve_exact(limit.get())
            .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
        let mut next_graph_cursor = self.graph_cursor.clone();
        let mut next_reveal_ledger_cursor = self.reveal_ledger_cursor.clone();
        let mut next_complete_reveal_probability = self.complete_reveal_probability;
        let mut next_pending_graph_paths = self.pending_graph_paths.clone();
        let mut next_active_concrete_family = self.active_concrete_family.clone();
        let mut graph_page_advanced = false;
        let mut graph_paths_materialized = 0usize;

        let mut ledger_reveals = Vec::new();
        if !next_reveal_ledger_cursor.is_exhausted() {
            let remaining_reveal_capacity = self
                .binding
                .adapter_budgets
                .reveal_outcomes()
                .checked_sub(self.accumulator.ledger_reveals.len())
                .ok_or(Pc4ObservationCandidateError::CounterOverflow)?;
            if remaining_reveal_capacity == 0 {
                return Err(Pc4ObservationCandidateError::BudgetExceeded(
                    Pc4ObservationCandidateBudgetExceeded {
                        kind: Pc4ObservationCandidateBudgetKind::RevealOutcomes,
                        limit: self.binding.adapter_budgets.reveal_outcomes(),
                        attempted: self
                            .accumulator
                            .ledger_reveals
                            .len()
                            .checked_add(1)
                            .ok_or(Pc4ObservationCandidateError::CounterOverflow)?,
                    },
                ));
            }
            let ledger_limit = remaining_reveal_capacity
                .min(self.reveal_ledger_family.page_limit())
                .min(limit.get());
            let ledger_page = self
                .reveal_ledger_family
                .next_page(
                    &mut next_reveal_ledger_cursor,
                    NonZeroUsize::new(ledger_limit).expect("remaining reveal capacity is non-zero"),
                    guard,
                )
                .map_err(map_reveal_ledger_error)?;
            ledger_reveals
                .try_reserve_exact(ledger_page.outcomes().len())
                .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
            for outcome in ledger_page.outcomes() {
                ledger_reveals.push(reveal_evidence_from_ledger(outcome)?);
            }
            if ledger_page.is_exhausted() {
                next_complete_reveal_probability = ledger_page.complete_probability();
            }
        }

        while discoveries.len() < limit.get() {
            check_guard(&self.binding.source, guard)?;

            if let Some(active) = next_active_concrete_family.as_mut() {
                let remaining = limit.get() - discoveries.len();
                let concrete_limit =
                    remaining.min(self.binding.adapter_budgets.concrete_paths_per_page());
                let concrete_limit = NonZeroUsize::new(concrete_limit)
                    .expect("remaining discovery capacity is non-zero");
                let mut next_cursor = active.cursor.clone();
                let concrete_paths = active
                    .family
                    .next_page(&mut next_cursor, concrete_limit, guard)
                    .map_err(map_concrete_page_error)?;
                let concrete_exhausted = next_cursor.is_exhausted();
                let mut batch = build_pending_candidates::<P::Error, T::Error, M::Error>(
                    &self.binding,
                    &active.observation_path,
                    &concrete_paths,
                )?;
                active.cursor = next_cursor;
                discoveries.append(&mut batch);
                if concrete_exhausted {
                    next_active_concrete_family = None;
                }
                continue;
            }

            if graph_paths_materialized == self.binding.adapter_budgets.graph_paths_per_advance() {
                break;
            }

            if let Some(observation_path) = next_pending_graph_paths.last() {
                let concrete_family = prepare_fixed_queue_concrete_family(
                    FixedQueuePathMaterializationRequest::new(
                        &self.binding.target,
                        observation_path.graph_path(),
                        self.binding.materialization_budgets,
                    ),
                    materializer,
                    guard,
                )
                .map_err(map_materialization_error)?;
                check_guard(&self.binding.source, guard)?;
                let observation_path = next_pending_graph_paths
                    .pop()
                    .expect("the pending path was just inspected");
                let cursor = concrete_family.cursor();
                next_active_concrete_family = Some(ActiveConcretePathFamily {
                    observation_path,
                    family: Arc::new(concrete_family),
                    cursor,
                });
                graph_paths_materialized = graph_paths_materialized
                    .checked_add(1)
                    .ok_or(Pc4ObservationCandidateError::CounterOverflow)?;
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
                    self.binding.adapter_budgets.graph_paths_per_advance,
                    provider,
                    terminal_predicate,
                    guard,
                )
                .map_err(map_graph_page_error)?;
            next_pending_graph_paths
                .try_reserve_exact(graph_page.paths().len())
                .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
            let mut next_paths = Vec::new();
            next_paths
                .try_reserve_exact(graph_page.paths().len())
                .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
            next_paths.extend(graph_page.paths().iter().cloned());
            check_guard(&self.binding.source, guard)?;
            next_graph_cursor = staged_graph_cursor;
            next_pending_graph_paths.extend(next_paths.into_iter().rev());
            graph_page_advanced = true;

            if graph_page.paths().is_empty() {
                break;
            }
        }

        let discovered_concrete_paths = discoveries.len();
        let prepared =
            self.accumulator
                .prepare_batch(&self.binding, discoveries, ledger_reveals)?;
        self.accumulator.reserve_batch(&prepared)?;
        check_guard(&self.binding.source, guard)?;
        let exhausted = next_graph_cursor.is_exhausted()
            && next_reveal_ledger_cursor.is_exhausted()
            && next_complete_reveal_probability.is_some()
            && next_pending_graph_paths.is_empty()
            && next_active_concrete_family.is_none();
        self.accumulator.commit_batch(prepared);
        self.graph_cursor = next_graph_cursor;
        self.reveal_ledger_cursor = next_reveal_ledger_cursor;
        self.complete_reveal_probability = next_complete_reveal_probability;
        self.pending_graph_paths = next_pending_graph_paths;
        self.active_concrete_family = next_active_concrete_family;

        Ok(Pc4ObservationCandidateAdvance {
            discovered_concrete_paths,
            observed_concrete_paths: self.accumulator.replay_count,
            observed_reveal_outcomes: self.accumulator.ledger_reveals.len(),
            observed_candidate_memberships: self.accumulator.seen_memberships.len(),
            status: if exhausted {
                Pc4ObservationCandidateAdvanceStatus::ExhaustedAwaitingFinalize
            } else {
                Pc4ObservationCandidateAdvanceStatus::InProgress
            },
        })
    }

    pub fn finish<G>(
        self,
        guard: &G,
    ) -> Result<Pc4CompleteObservationCandidateFamily, Pc4ObservationCandidateSessionError>
    where
        G: Pc4ObservationCandidateGuard,
    {
        if !self.is_exhausted() {
            return Err(Pc4ObservationCandidateError::IncompleteCannotFinalize);
        }
        self.accumulator
            .finish::<Infallible, Infallible, Infallible, _>(
                self.binding,
                self.complete_reveal_probability
                    .ok_or(Pc4ObservationCandidateError::IncompleteCannotFinalize)?,
                guard,
            )
    }
}

pub fn prepare_pc4_observation_candidate_session<G>(
    request: Pc4ObservationCandidateAdapterRequest<'_>,
    graph_family: &Pc4ObservationGraphFamily,
    guard: &G,
) -> Result<Pc4ObservationCandidateSession, Pc4ObservationCandidateSessionError>
where
    G: Pc4ObservationCandidateGuard,
{
    validate_source_binding(&request, guard)?;
    if graph_family.target() != request.target {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::GraphTargetMismatch,
        ));
    }
    if graph_family.source_field_id() != request.source_field_id {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::GraphSourceFieldMismatch,
        ));
    }
    validate_queue_scope_binding(&request, graph_family)?;
    if request.adapter_budgets.graph_paths_per_advance()
        > graph_family.budgets().page_output_paths()
    {
        return Err(Pc4ObservationCandidateError::AdvanceLimitExceeded {
            limit: graph_family.budgets().page_output_paths(),
            attempted: request.adapter_budgets.graph_paths_per_advance(),
        });
    }
    if request.adapter_budgets.concrete_paths_per_page()
        > request.materialization_budgets.page_solutions()
    {
        return Err(Pc4ObservationCandidateError::AdvanceLimitExceeded {
            limit: request.materialization_budgets.page_solutions(),
            attempted: request.adapter_budgets.concrete_paths_per_page(),
        });
    }
    request
        .adapter_budgets
        .graph_paths_per_advance()
        .checked_mul(request.materialization_budgets.graph_edges())
        .ok_or(Pc4ObservationCandidateError::CounterOverflow)?;

    let binding = ObservationCandidateBinding::from_request(request);
    let reveal_ledger_family = graph_family.reveal_ledger_family();
    let reveal_ledger_cursor = reveal_ledger_family.cursor();
    Ok(Pc4ObservationCandidateSession {
        graph_cursor: graph_family.cursor(),
        graph_family: graph_family.clone(),
        reveal_ledger_family,
        reveal_ledger_cursor,
        complete_reveal_probability: None,
        binding,
        pending_graph_paths: Vec::new(),
        active_concrete_family: None,
        accumulator: CandidateAccumulator::new(),
    })
}

fn validate_queue_scope_binding<ProviderError, TerminalError, MaterializerError>(
    request: &Pc4ObservationCandidateAdapterRequest<'_>,
    graph_family: &Pc4ObservationGraphFamily,
) -> Result<(), Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>> {
    if request.prepared_input.target() != request.target {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::PreparedInputTargetMismatch,
        ));
    }
    let Pc4PreparedQueueInput::PatternOrHidden {
        visible_queue,
        scope,
        bag_state,
        ..
    } = request.prepared_input.queue()
    else {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::PreparedInputNotPatternOrHidden,
        ));
    };
    let graph_scope = graph_family.queue_scope();
    if visible_queue.as_slice() != graph_scope.initial_visible_queue()
        || scope.visible_piece_count() != graph_scope.initial_visible_queue().len()
        || scope.preview_length() != graph_scope.preview_length()
        || scope.hidden_draws() != graph_scope.hidden_draws()
        || scope.placement_count() != graph_scope.placement_count()
        || bag_state.as_ref().copied() != Some(graph_scope.hidden_source_state())
    {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::QueueScopeMismatch,
        ));
    }

    let initial_board = StandardPcBoard::from_words(
        request.target.target_lines().get(),
        [request.source.initial_board_mask(), 0, 0, 0],
    )
    .map_err(|_| {
        Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::InitialBoardMismatch,
        )
    })?;
    let expected_request_identity = PcCandidateRequestIdentity::derive_pc4_candidate_universe(
        request.prepared_input,
        initial_board,
        graph_scope.initial_hold(),
    )
    .map_err(|_| {
        Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::RequestIdentityMismatch,
        )
    })?;
    if request.source.request_identity() != expected_request_identity {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::RequestIdentityMismatch,
        ));
    }
    Ok(())
}

fn validate_source_binding<ProviderError, TerminalError, MaterializerError, G>(
    request: &Pc4ObservationCandidateAdapterRequest<'_>,
    guard: &G,
) -> Result<(), Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>>
where
    G: Pc4ObservationCandidateGuard,
{
    check_guard(request.source, guard)?;
    if request.source.provider_kind() != PcCandidateProviderKind::OnlinePc4 {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::SourceIsNotOnlinePc4,
        ));
    }
    if request.source.qualified_snapshot() != Some(request.target.snapshot()) {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::SourceSnapshotMismatch,
        ));
    }
    if request.source.profile() != request.target.profile() {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::SourceProfileMismatch,
        ));
    }
    Ok(())
}

fn validate_call_binding<ProviderError, T, M, G, P>(
    binding: &ObservationCandidateBinding,
    provider: &P,
    terminal_predicate: &T,
    materializer: &M,
    guard: &G,
) -> Result<(), Pc4ObservationCandidateError<ProviderError, T::Error, M::Error>>
where
    P: QualifiedCompleteAdjacencyProvider<Error = ProviderError>,
    T: QualifiedPc4ObservationCandidateTerminalPredicate,
    M: Pc4PlacementMaterializer,
    G: Pc4ObservationCandidateGuard,
{
    check_guard(&binding.source, guard)?;
    if provider.target() != &binding.target {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::ProviderTargetMismatch,
        ));
    }
    if terminal_predicate.target() != &binding.target {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::TerminalTargetMismatch,
        ));
    }
    if terminal_predicate.terminal_semantics_identity()
        != binding.target.qualification().terminal_semantics_identity()
    {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::TerminalSemanticsMismatch,
        ));
    }
    if materializer.profile() != binding.target.profile() {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::MaterializerProfileMismatch,
        ));
    }
    Ok(())
}

fn build_pending_candidates<ProviderError, TerminalError, MaterializerError>(
    binding: &ObservationCandidateBinding,
    observation_path: &Pc4ObservationGraphPath,
    concrete_paths: &[FixedQueueConcretePath],
) -> Result<
    Vec<PendingCandidate>,
    Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>,
> {
    if observation_path.target() != &binding.target
        || observation_path.source_field_id() != binding.source_field_id
        || observation_path.graph_path().start_field_id() != binding.source_field_id
    {
        return Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::ConcreteSourceFieldMismatch,
        ));
    }

    let frontier = observation_path.frontier_entry();
    let mut pending = Vec::new();
    pending
        .try_reserve_exact(concrete_paths.len())
        .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
    for path in concrete_paths {
        if path.start_field_id() != binding.source_field_id {
            return Err(Pc4ObservationCandidateError::Binding(
                Pc4ObservationCandidateBindingError::ConcreteSourceFieldMismatch,
            ));
        }
        if path.target_field_ids().len() != path.placements().len() {
            return Err(Pc4ObservationCandidateError::Binding(
                Pc4ObservationCandidateBindingError::ConcretePathLengthMismatch,
            ));
        }
        let identity = StandardBoard64TilingIdentity::from_placements(
            binding.source.initial_board_mask(),
            path.placements().iter().copied().map(|placement| {
                PiecePlacementMask::new(piece_kind(placement.piece()), placement.occupied_cells())
            }),
        )
        .map_err(Pc4ObservationCandidateError::CandidateIdentity)?;
        pending.push(PendingCandidate {
            reveal: Pc4ObservationRevealEvidence {
                reveal_rank: frontier.reveal_rank(),
                revealed_pieces: try_copy_slice(frontier.revealed_pieces())?,
                probability: frontier.probability(),
                terminal_bag_state: frontier.terminal_bag_state(),
            },
            identity,
            provenance: Pc4ObservationCandidateProvenance {
                frontier_entry_index: frontier.entry_index(),
                hold_path_index: frontier.hold_path_index(),
                placement_queue: try_copy_slice(frontier.hold_path().placement_queue())?,
                hold_steps: try_copy_slice(frontier.hold_path().steps())?,
                terminal_cursor: frontier.hold_path().terminal_cursor(),
                terminal_hold: frontier.hold_path().terminal_hold(),
                terminal_observation: try_copy_slice(frontier.terminal_observation())?,
                terminal_observation_is_complete: frontier.terminal_observation_is_complete(),
                source_field_id: path.start_field_id(),
                terminal_field_id: path.terminal_field_id(),
                target_field_ids: try_copy_slice(path.target_field_ids())?,
                placements: try_copy_slice(path.placements())?,
            },
        });
    }
    Ok(pending)
}

fn reveal_evidence_from_ledger<ProviderError, TerminalError, MaterializerError>(
    outcome: &Pc4ObservationRevealOutcome,
) -> Result<
    Pc4ObservationRevealEvidence,
    Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>,
> {
    Ok(Pc4ObservationRevealEvidence {
        reveal_rank: outcome.rank(),
        revealed_pieces: try_copy_slice(outcome.pieces())?,
        probability: outcome.probability(),
        terminal_bag_state: outcome.terminal_bag_state(),
    })
}

fn try_copy_slice<T: Copy, ProviderError, TerminalError, MaterializerError>(
    source: &[T],
) -> Result<Vec<T>, Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(source.len())
        .map_err(|_| Pc4ObservationCandidateError::AllocationFailed)?;
    output.extend_from_slice(source);
    Ok(output)
}

fn check_guard<ProviderError, TerminalError, MaterializerError, G>(
    source: &PcCandidateSourceBinding,
    guard: &G,
) -> Result<(), Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>>
where
    G: Pc4ObservationCandidateGuard,
{
    if PcCandidatePageGuard::is_cancelled(guard) {
        return Err(Pc4ObservationCandidateError::Cancelled);
    }
    if !guard.is_current_source(source) {
        return Err(Pc4ObservationCandidateError::StaleSource);
    }
    if source
        .qualified_snapshot()
        .is_some_and(|snapshot| !PcCandidatePageGuard::is_current_snapshot(guard, snapshot))
    {
        return Err(Pc4ObservationCandidateError::StaleSnapshot);
    }
    Ok(())
}

fn enforce_budget<ProviderError, TerminalError, MaterializerError>(
    kind: Pc4ObservationCandidateBudgetKind,
    limit: usize,
    attempted: usize,
) -> Result<(), Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError>> {
    if attempted > limit {
        Err(Pc4ObservationCandidateError::BudgetExceeded(
            Pc4ObservationCandidateBudgetExceeded {
                kind,
                limit,
                attempted,
            },
        ))
    } else {
        Ok(())
    }
}

fn map_graph_page_error<ProviderError, TerminalError, MaterializerError>(
    error: Pc4ObservationGraphPageError<ProviderError, TerminalError>,
) -> Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError> {
    match error {
        Pc4ObservationGraphPageError::Cancelled => Pc4ObservationCandidateError::Cancelled,
        Pc4ObservationGraphPageError::StaleSnapshot => Pc4ObservationCandidateError::StaleSnapshot,
        other => Pc4ObservationCandidateError::ObservationGraph(other),
    }
}

fn map_reveal_ledger_error<ProviderError, TerminalError, MaterializerError>(
    error: Pc4ObservationRevealLedgerPageError,
) -> Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError> {
    match error {
        Pc4ObservationRevealLedgerPageError::Cancelled => Pc4ObservationCandidateError::Cancelled,
        Pc4ObservationRevealLedgerPageError::StaleSnapshot => {
            Pc4ObservationCandidateError::StaleSnapshot
        }
        other => Pc4ObservationCandidateError::RevealLedger(other),
    }
}

fn map_materialization_error<ProviderError, TerminalError, MaterializerError>(
    error: ConcretePathMaterializationError<MaterializerError>,
) -> Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError> {
    match error {
        ConcretePathMaterializationError::Cancelled => Pc4ObservationCandidateError::Cancelled,
        ConcretePathMaterializationError::StaleSnapshot => {
            Pc4ObservationCandidateError::StaleSnapshot
        }
        ConcretePathMaterializationError::Edge {
            source: clearra_pc4_tablebase::PlacementMaterializationError::Cancelled,
            ..
        } => Pc4ObservationCandidateError::Cancelled,
        ConcretePathMaterializationError::Edge {
            source: clearra_pc4_tablebase::PlacementMaterializationError::StaleSnapshot,
            ..
        } => Pc4ObservationCandidateError::StaleSnapshot,
        other => Pc4ObservationCandidateError::Materialization(other),
    }
}

fn map_concrete_page_error<ProviderError, TerminalError, MaterializerError>(
    error: ConcretePathPageError,
) -> Pc4ObservationCandidateError<ProviderError, TerminalError, MaterializerError> {
    match error {
        ConcretePathPageError::Cancelled => Pc4ObservationCandidateError::Cancelled,
        ConcretePathPageError::StaleSnapshot => Pc4ObservationCandidateError::StaleSnapshot,
        other => Pc4ObservationCandidateError::ConcretePage(other),
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
#[path = "pc4_observation_candidate_adapter_tests.rs"]
mod tests;
