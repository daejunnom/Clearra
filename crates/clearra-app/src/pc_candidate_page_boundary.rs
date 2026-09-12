// SRP rationale: this module has one behavior-level change reason: accepting
// request-bound canonical PC candidate pages and sealing only a source-proven
// complete universe for the shared application reducer boundary. Transport,
// tablebase parsing, placement materialization, coverage reduction, fallback,
// and product presentation remain owned by their existing layers.

use core::num::NonZeroU64;

use clearra_core_domain::{
    board::standard_pc_board::StandardPcBoard,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_pc4_tablebase::{
    FixedQueueHoldState, Pc4GraphPiece, Pc4RuleProfile, Pc4TargetLines, Pc4TerminalUseCase,
    QualifiedPc4TargetIdentity, QualifiedSnapshotIdentity,
};
use sha2::{Digest, Sha256};

use crate::pc4_input_disclosure_policy::{
    Pc4HiddenQueueSource, Pc4PreparedOnlineInput, Pc4PreparedQueueInput,
};

#[path = "pc4_graph_candidate_adapter.rs"]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) mod graph_candidate_adapter;

pub const PC_CANDIDATE_PAGE_CONTRACT: &str = "pc-concrete-candidate-page.v1";
pub const PC_CANDIDATE_REQUEST_IDENTITY_ALGORITHM: &str =
    "sha256:clearra-pc4-candidate-universe-request-v1";
pub const PC_CANDIDATE_SET_DIGEST_ALGORITHM: &str = "sha256:clearra-pc-canonical-candidate-set-v1";

const CANDIDATE_REQUEST_IDENTITY_DOMAIN: &[u8] = b"clearra.pc4-candidate-universe-request.v1\0";
const CANDIDATE_SET_DIGEST_DOMAIN: &[u8] = b"clearra.pc-canonical-candidate-set.v1\0";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PcCandidateSessionId(NonZeroU64);

impl PcCandidateSessionId {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PcCandidateRequestIdentity([u8; 32]);

impl PcCandidateRequestIdentity {
    pub const fn from_sha256(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Derives the canonical identity of one PC4 candidate-universe query.
    ///
    /// This binds the verified target authority, normalized initial board,
    /// queue/reveal semantics, and initial hold state. The input surface is
    /// intentionally excluded, and product objectives are not accepted by this
    /// domain boundary: either may reduce the same complete candidate universe
    /// later without changing which candidates belong to it.
    pub fn derive_pc4_candidate_universe(
        prepared_input: &Pc4PreparedOnlineInput,
        initial_board: StandardPcBoard,
        initial_hold: FixedQueueHoldState,
    ) -> Result<Self, PcCandidateRequestIdentityError> {
        let mut hasher =
            candidate_universe_hasher(prepared_input.target(), initial_board, initial_hold)?;
        match prepared_input.queue() {
            Pc4PreparedQueueInput::FixedExplicit(queue) => {
                hasher.update([0]);
                hash_pieces(&mut hasher, queue)?;
            }
            Pc4PreparedQueueInput::PatternOrHidden {
                source,
                visible_queue,
                scope,
                bag_state,
            } => {
                hasher.update([1]);
                hasher.update([hidden_source_tag(*source)]);
                hash_pieces(&mut hasher, visible_queue)?;
                hash_usize(&mut hasher, scope.visible_piece_count())?;
                hash_usize(&mut hasher, scope.preview_length())?;
                hash_usize(&mut hasher, scope.hidden_draws())?;
                hash_usize(&mut hasher, scope.placement_count())?;
                match bag_state {
                    Some(bag_state) => {
                        hasher.update([1]);
                        for count in bag_state.profile().counts() {
                            hasher.update(count.to_be_bytes());
                        }
                        for count in bag_state.remainder() {
                            hasher.update(count.to_be_bytes());
                        }
                        hasher.update(bag_state.epoch().to_be_bytes());
                    }
                    None => hasher.update([0]),
                }
            }
        }
        Ok(Self(hasher.finalize().into()))
    }

    /// Recomputes the same request identity for the fixed-queue representation
    /// retained by a compiled `SearchProblem`. This remains crate-private so a
    /// caller cannot mint candidate-completeness authority from request data.
    pub(crate) fn derive_pc4_fixed_queue_candidate_universe(
        target: &QualifiedPc4TargetIdentity,
        initial_board: StandardPcBoard,
        initial_hold: FixedQueueHoldState,
        queue: &[Pc4GraphPiece],
    ) -> Result<Self, PcCandidateRequestIdentityError> {
        let mut hasher = candidate_universe_hasher(target, initial_board, initial_hold)?;
        hasher.update([0]);
        hash_pieces(&mut hasher, queue)?;
        Ok(Self(hasher.finalize().into()))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcCandidateRequestIdentityError {
    CanonicalLengthOverflow,
}

impl PcCandidateRequestIdentityError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::CanonicalLengthOverflow => "pc_candidate_request_canonical_length_overflow",
        }
    }
}

impl core::fmt::Display for PcCandidateRequestIdentityError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for PcCandidateRequestIdentityError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcCandidateSourceBindingError {
    InitialBoardTargetLinesMismatch { board_lines: u8, target_lines: u8 },
    InitialBoardNotCompact,
    RequestIdentity(PcCandidateRequestIdentityError),
}

impl PcCandidateSourceBindingError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::InitialBoardTargetLinesMismatch { .. } => {
                "pc_candidate_source_initial_board_target_lines_mismatch"
            }
            Self::InitialBoardNotCompact => "pc_candidate_source_initial_board_not_compact",
            Self::RequestIdentity(error) => error.reason(),
        }
    }
}

impl core::fmt::Display for PcCandidateSourceBindingError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for PcCandidateSourceBindingError {}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PcCandidateSourceIdentity([u8; 32]);

impl PcCandidateSourceIdentity {
    pub const fn from_sha256(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcCandidateProviderKind {
    OfflineExact,
    OnlinePc4,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PcCandidateProviderProvenance {
    OfflineExact,
    OnlinePc4(QualifiedSnapshotIdentity),
}

/// Immutable identity shared by every page in one candidate-source session.
///
/// An online binding cannot be constructed from a mutable branch, generation
/// label, or unverified manifest: it owns the nominal identity minted by the
/// PC4 snapshot verifier. This still grants no completeness authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcCandidateSourceBinding {
    session_id: PcCandidateSessionId,
    request_identity: PcCandidateRequestIdentity,
    source_identity: PcCandidateSourceIdentity,
    profile: Pc4RuleProfile,
    initial_board_mask: u64,
    provider: PcCandidateProviderProvenance,
}

impl PcCandidateSourceBinding {
    pub const fn offline_exact(
        session_id: PcCandidateSessionId,
        request_identity: PcCandidateRequestIdentity,
        source_identity: PcCandidateSourceIdentity,
        profile: Pc4RuleProfile,
        initial_board_mask: u64,
    ) -> Self {
        Self {
            session_id,
            request_identity,
            source_identity,
            profile,
            initial_board_mask,
            provider: PcCandidateProviderProvenance::OfflineExact,
        }
    }

    #[cfg(test)]
    pub(crate) fn online_pc4(
        session_id: PcCandidateSessionId,
        request_identity: PcCandidateRequestIdentity,
        source_identity: PcCandidateSourceIdentity,
        profile: Pc4RuleProfile,
        initial_board_mask: u64,
        snapshot: QualifiedSnapshotIdentity,
    ) -> Self {
        Self {
            session_id,
            request_identity,
            source_identity,
            profile,
            initial_board_mask,
            provider: PcCandidateProviderProvenance::OnlinePc4(snapshot),
        }
    }

    /// Creates the only public online-PC4 source binding from a fully prepared
    /// input. The request identity is derived here so product adapters cannot
    /// attach an arbitrary queue identity to a qualified target or board.
    pub fn online_pc4_for_prepared_input(
        session_id: PcCandidateSessionId,
        source_identity: PcCandidateSourceIdentity,
        prepared_input: &Pc4PreparedOnlineInput,
        initial_board: StandardPcBoard,
        initial_hold: FixedQueueHoldState,
    ) -> Result<Self, PcCandidateSourceBindingError> {
        let target_lines = prepared_input.target_lines().get();
        if initial_board.lines() != target_lines {
            return Err(
                PcCandidateSourceBindingError::InitialBoardTargetLinesMismatch {
                    board_lines: initial_board.lines(),
                    target_lines,
                },
            );
        }
        let initial_board_mask = initial_board
            .occupied()
            .compact_board64()
            .ok_or(PcCandidateSourceBindingError::InitialBoardNotCompact)?;
        let request_identity = PcCandidateRequestIdentity::derive_pc4_candidate_universe(
            prepared_input,
            initial_board,
            initial_hold,
        )
        .map_err(PcCandidateSourceBindingError::RequestIdentity)?;
        Ok(Self {
            session_id,
            request_identity,
            source_identity,
            profile: prepared_input.profile(),
            initial_board_mask,
            provider: PcCandidateProviderProvenance::OnlinePc4(
                prepared_input.target().snapshot().clone(),
            ),
        })
    }

    pub const fn session_id(&self) -> PcCandidateSessionId {
        self.session_id
    }

    pub const fn request_identity(&self) -> PcCandidateRequestIdentity {
        self.request_identity
    }

    pub const fn source_identity(&self) -> PcCandidateSourceIdentity {
        self.source_identity
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.profile
    }

    pub const fn initial_board_mask(&self) -> u64 {
        self.initial_board_mask
    }

    pub const fn provider_kind(&self) -> PcCandidateProviderKind {
        match &self.provider {
            PcCandidateProviderProvenance::OfflineExact => PcCandidateProviderKind::OfflineExact,
            PcCandidateProviderProvenance::OnlinePc4(_) => PcCandidateProviderKind::OnlinePc4,
        }
    }

    pub fn qualified_snapshot(&self) -> Option<&QualifiedSnapshotIdentity> {
        match &self.provider {
            PcCandidateProviderProvenance::OfflineExact => None,
            PcCandidateProviderProvenance::OnlinePc4(snapshot) => Some(snapshot),
        }
    }
}

/// Host freshness/cancellation observation sampled before a page is inspected
/// and again before its logical state is committed.
pub trait PcCandidatePageGuard {
    fn is_cancelled(&self) -> bool;
    fn is_current_source(&self, source: &PcCandidateSourceBinding) -> bool;
    fn is_current_snapshot(&self, snapshot: &QualifiedSnapshotIdentity) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcCandidatePageCursor {
    next_ordinal: u64,
    after: Option<StandardBoard64TilingIdentity>,
}

impl PcCandidatePageCursor {
    pub const fn initial() -> Self {
        Self {
            next_ordinal: 0,
            after: None,
        }
    }

    pub const fn next_ordinal(self) -> u64 {
        self.next_ordinal
    }

    pub const fn after(self) -> Option<StandardBoard64TilingIdentity> {
        self.after
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PcCandidateSetDigest([u8; 32]);

impl PcCandidateSetDigest {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub(crate) fn calculate(
        candidates: &[StandardBoard64TilingIdentity],
    ) -> Result<Self, PcCandidateBoundaryError> {
        Self::calculate_parts(candidates, &[])
    }

    fn calculate_parts(
        accepted: &[StandardBoard64TilingIdentity],
        page: &[StandardBoard64TilingIdentity],
    ) -> Result<Self, PcCandidateBoundaryError> {
        let candidate_count = accepted
            .len()
            .checked_add(page.len())
            .and_then(|count| u64::try_from(count).ok())
            .ok_or(PcCandidateBoundaryError::CandidateOrdinalOverflow)?;
        let mut hasher = Sha256::new();
        hasher.update(CANDIDATE_SET_DIGEST_DOMAIN);
        hasher.update(candidate_count.to_be_bytes());
        for candidate in accepted.iter().chain(page) {
            hasher.update(candidate.initial_board_mask().to_be_bytes());
            hasher.update(
                u64::try_from(candidate.placement_count())
                    .expect("standard Board64 candidate placement count fits u64")
                    .to_be_bytes(),
            );
            hasher.update(candidate.packed_piece_codes().to_be_bytes());
            for placement_mask in candidate.placement_masks() {
                hasher.update(placement_mask.to_be_bytes());
            }
        }
        Ok(Self(hasher.finalize().into()))
    }
}

/// Unforgeable outside `clearra-app`: a producer adapter may mint this only
/// after its own exact source has proved the full request universe, count, and
/// canonical set digest. The existing PC reducers cannot mint it from summary
/// booleans or a tablebase hit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcCandidateCompletenessEvidence {
    source: PcCandidateSourceBinding,
    qualified_target: Option<QualifiedPc4TargetIdentity>,
    exact_candidate_count: u64,
    candidate_set_digest: PcCandidateSetDigest,
}

impl PcCandidateCompletenessEvidence {
    fn from_verified_complete_source(
        source: PcCandidateSourceBinding,
        qualified_target: Option<QualifiedPc4TargetIdentity>,
        exact_candidate_count: u64,
        candidate_set_digest: PcCandidateSetDigest,
    ) -> Result<Self, PcCandidateBoundaryError> {
        validate_complete_target_binding(&source, qualified_target.as_ref())?;
        Ok(Self {
            source,
            qualified_target,
            exact_candidate_count,
            candidate_set_digest,
        })
    }

    #[cfg(test)]
    fn from_test_verified_complete_source(
        source: PcCandidateSourceBinding,
        qualified_target: Option<QualifiedPc4TargetIdentity>,
        exact_candidate_count: u64,
        candidate_set_digest: PcCandidateSetDigest,
    ) -> Result<Self, PcCandidateBoundaryError> {
        Self::from_verified_complete_source(
            source,
            qualified_target,
            exact_candidate_count,
            candidate_set_digest,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcConcreteCandidatePage {
    contract_id: &'static str,
    source: PcCandidateSourceBinding,
    cursor: PcCandidatePageCursor,
    next_cursor: PcCandidatePageCursor,
    candidates: Vec<StandardBoard64TilingIdentity>,
    terminal: bool,
    completeness: Option<PcCandidateCompletenessEvidence>,
}

impl PcConcreteCandidatePage {
    /// Constructs a page which carries no full-universe authority. `terminal`
    /// means only that this partial producer has no more known candidates.
    pub fn partial(
        source: PcCandidateSourceBinding,
        cursor: PcCandidatePageCursor,
        candidates: Vec<StandardBoard64TilingIdentity>,
        terminal: bool,
    ) -> Result<Self, PcCandidateBoundaryError> {
        Self::new(source, cursor, candidates, terminal, None)
    }

    /// Seals the terminal page with evidence minted by a future validated
    /// producer adapter. No such evidence producer is exported in v0.9.0's
    /// feature-off state, so callers cannot promote a lookup hit themselves.
    pub fn complete(
        source: PcCandidateSourceBinding,
        cursor: PcCandidatePageCursor,
        candidates: Vec<StandardBoard64TilingIdentity>,
        evidence: PcCandidateCompletenessEvidence,
    ) -> Result<Self, PcCandidateBoundaryError> {
        Self::new(source, cursor, candidates, true, Some(evidence))
    }

    fn new(
        source: PcCandidateSourceBinding,
        cursor: PcCandidatePageCursor,
        candidates: Vec<StandardBoard64TilingIdentity>,
        terminal: bool,
        completeness: Option<PcCandidateCompletenessEvidence>,
    ) -> Result<Self, PcCandidateBoundaryError> {
        if candidates.is_empty() && !terminal {
            return Err(PcCandidateBoundaryError::EmptyNonTerminalPage);
        }
        validate_candidate_order(&candidates)?;
        if candidates
            .iter()
            .any(|candidate| candidate.initial_board_mask() != source.initial_board_mask())
        {
            return Err(PcCandidateBoundaryError::InitialBoardMismatch);
        }
        if completeness
            .as_ref()
            .is_some_and(|evidence| evidence.source != source)
        {
            return Err(PcCandidateBoundaryError::CompletenessBindingMismatch);
        }
        let candidate_count = u64::try_from(candidates.len())
            .map_err(|_| PcCandidateBoundaryError::CandidateOrdinalOverflow)?;
        let next_ordinal = cursor
            .next_ordinal
            .checked_add(candidate_count)
            .ok_or(PcCandidateBoundaryError::CandidateOrdinalOverflow)?;
        let next_cursor = PcCandidatePageCursor {
            next_ordinal,
            after: candidates.last().copied().or(cursor.after),
        };
        Ok(Self {
            contract_id: PC_CANDIDATE_PAGE_CONTRACT,
            source,
            cursor,
            next_cursor,
            candidates,
            terminal,
            completeness,
        })
    }

    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn source(&self) -> &PcCandidateSourceBinding {
        &self.source
    }

    pub const fn cursor(&self) -> PcCandidatePageCursor {
        self.cursor
    }

    pub const fn next_cursor(&self) -> PcCandidatePageCursor {
        self.next_cursor
    }

    pub fn candidates(&self) -> &[StandardBoard64TilingIdentity] {
        &self.candidates
    }

    pub const fn terminal(&self) -> bool {
        self.terminal
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcCandidateCollectionCompleteness {
    PartialKnownCandidates,
    CompleteRequestUniverse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcCandidateCollection {
    source: PcCandidateSourceBinding,
    candidates: Vec<StandardBoard64TilingIdentity>,
    completeness: PcCandidateCollectionCompleteness,
    complete_universe_evidence: Option<PcCandidateCompletenessEvidence>,
}

impl PcCandidateCollection {
    pub const fn source(&self) -> &PcCandidateSourceBinding {
        &self.source
    }

    pub fn candidates(&self) -> &[StandardBoard64TilingIdentity] {
        &self.candidates
    }

    pub const fn completeness(&self) -> PcCandidateCollectionCompleteness {
        self.completeness
    }

    /// Only a fully attested universe can cross the candidate-universe reducer
    /// seam. Coverage rows, probabilities, objective proofs, and an eventual
    /// `CoreExecutionResult` remain downstream responsibilities.
    pub fn into_reducer_input(self) -> Result<PcCandidateReducerInput, PcCandidateBoundaryError> {
        if self.completeness != PcCandidateCollectionCompleteness::CompleteRequestUniverse {
            return Err(PcCandidateBoundaryError::IncompleteCannotReduce);
        }
        let complete_universe_evidence = self
            .complete_universe_evidence
            .ok_or(PcCandidateBoundaryError::IncompleteCannotReduce)?;
        Ok(PcCandidateReducerInput {
            universe_identity: PcCandidateUniverseIdentity::from_complete_evidence(
                complete_universe_evidence,
            )?,
            candidates: self.candidates,
        })
    }
}

/// Immutable identity of an exactly complete canonical candidate universe.
///
/// The source binding retains the request/source/snapshot and normalized
/// initial-board identities. Online PC4 universes additionally retain their
/// exact qualified target, including use case and target line count. Count and
/// digest are minted only from producer completeness evidence; no public
/// constructor can promote a naked boolean or candidate vector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcCandidateUniverseIdentity {
    source: PcCandidateSourceBinding,
    qualified_target: Option<QualifiedPc4TargetIdentity>,
    exact_candidate_count: u64,
    candidate_set_digest: PcCandidateSetDigest,
}

impl PcCandidateUniverseIdentity {
    fn from_complete_evidence(
        evidence: PcCandidateCompletenessEvidence,
    ) -> Result<Self, PcCandidateBoundaryError> {
        validate_complete_target_binding(&evidence.source, evidence.qualified_target.as_ref())?;
        Ok(Self {
            source: evidence.source,
            qualified_target: evidence.qualified_target,
            exact_candidate_count: evidence.exact_candidate_count,
            candidate_set_digest: evidence.candidate_set_digest,
        })
    }

    pub const fn source(&self) -> &PcCandidateSourceBinding {
        &self.source
    }

    pub const fn request_identity(&self) -> PcCandidateRequestIdentity {
        self.source.request_identity()
    }

    pub const fn source_identity(&self) -> PcCandidateSourceIdentity {
        self.source.source_identity()
    }

    pub fn qualified_snapshot(&self) -> Option<&QualifiedSnapshotIdentity> {
        self.source.qualified_snapshot()
    }

    pub const fn initial_board_mask(&self) -> u64 {
        self.source.initial_board_mask()
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.source.profile()
    }

    pub const fn qualified_target(&self) -> Option<&QualifiedPc4TargetIdentity> {
        self.qualified_target.as_ref()
    }

    pub fn use_case(&self) -> Option<Pc4TerminalUseCase> {
        self.qualified_target
            .as_ref()
            .map(|target| target.use_case())
    }

    pub fn target_lines(&self) -> Option<Pc4TargetLines> {
        self.qualified_target
            .as_ref()
            .map(|target| target.target_lines())
    }

    pub const fn exact_candidate_count(&self) -> u64 {
        self.exact_candidate_count
    }

    pub const fn candidate_set_digest(&self) -> PcCandidateSetDigest {
        self.candidate_set_digest
    }
}

/// Complete provider-neutral candidate universe before coverage reduction.
/// This is intentionally not an `ExecutionResult` and carries no claim that
/// coverage, probability, score, or replay work has run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcCandidateReducerInput {
    universe_identity: PcCandidateUniverseIdentity,
    candidates: Vec<StandardBoard64TilingIdentity>,
}

impl PcCandidateReducerInput {
    /// Only the observation adapter's sealed, completely exhausted family may
    /// cross this seam. Raw candidate parts cannot mint completeness evidence.
    pub(crate) fn from_complete_observation_union(
        family: &crate::pc4_observation_candidate_adapter::Pc4CompleteObservationCandidateFamily,
    ) -> Result<Self, PcCandidateBoundaryError> {
        let source = family.source();
        let candidates = family.canonical_candidates();
        validate_candidate_order(candidates)?;
        if candidates
            .iter()
            .any(|candidate| candidate.initial_board_mask() != source.initial_board_mask())
        {
            return Err(PcCandidateBoundaryError::InitialBoardMismatch);
        }
        let exact_candidate_count = u64::try_from(candidates.len())
            .map_err(|_| PcCandidateBoundaryError::CandidateOrdinalOverflow)?;
        let candidate_set_digest = PcCandidateSetDigest::calculate_parts(&[], candidates)?;
        let evidence = PcCandidateCompletenessEvidence::from_verified_complete_source(
            source.clone(),
            Some(family.target().clone()),
            exact_candidate_count,
            candidate_set_digest,
        )?;
        let mut owned_candidates = Vec::new();
        owned_candidates
            .try_reserve_exact(candidates.len())
            .map_err(|_| PcCandidateBoundaryError::CandidateAllocationFailed)?;
        owned_candidates.extend_from_slice(candidates);
        Ok(Self {
            universe_identity: PcCandidateUniverseIdentity::from_complete_evidence(evidence)?,
            candidates: owned_candidates,
        })
    }

    #[cfg(test)]
    pub(crate) fn from_test_parts(
        source: PcCandidateSourceBinding,
        qualified_target: Option<QualifiedPc4TargetIdentity>,
        candidates: Vec<StandardBoard64TilingIdentity>,
    ) -> Self {
        let exact_candidate_count =
            u64::try_from(candidates.len()).expect("test candidate count fits u64");
        let candidate_set_digest =
            PcCandidateSetDigest::calculate(&candidates).expect("test candidate digest");
        let evidence = PcCandidateCompletenessEvidence::from_test_verified_complete_source(
            source,
            qualified_target,
            exact_candidate_count,
            candidate_set_digest,
        )
        .expect("test source and target binding are consistent");
        Self {
            universe_identity: PcCandidateUniverseIdentity::from_complete_evidence(evidence)
                .expect("test completeness evidence is internally consistent"),
            candidates,
        }
    }

    pub const fn source(&self) -> &PcCandidateSourceBinding {
        self.universe_identity.source()
    }

    pub const fn universe_identity(&self) -> &PcCandidateUniverseIdentity {
        &self.universe_identity
    }

    pub fn candidates(&self) -> &[StandardBoard64TilingIdentity] {
        &self.candidates
    }

    #[cfg(test)]
    pub(crate) fn with_test_exact_candidate_count(mut self, exact_candidate_count: u64) -> Self {
        self.universe_identity.exact_candidate_count = exact_candidate_count;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_test_candidate_set_digest(mut self, digest: [u8; 32]) -> Self {
        self.universe_identity.candidate_set_digest = PcCandidateSetDigest(digest);
        self
    }
}

pub struct PcCandidatePageCollector {
    source: PcCandidateSourceBinding,
    expected_cursor: PcCandidatePageCursor,
    candidates: Vec<StandardBoard64TilingIdentity>,
    terminal_completeness: Option<PcCandidateCollectionCompleteness>,
    complete_universe_evidence: Option<PcCandidateCompletenessEvidence>,
}

impl PcCandidatePageCollector {
    pub fn new(source: PcCandidateSourceBinding) -> Self {
        Self {
            source,
            expected_cursor: PcCandidatePageCursor::initial(),
            candidates: Vec::new(),
            terminal_completeness: None,
            complete_universe_evidence: None,
        }
    }

    pub const fn source(&self) -> &PcCandidateSourceBinding {
        &self.source
    }

    pub const fn expected_cursor(&self) -> PcCandidatePageCursor {
        self.expected_cursor
    }

    pub fn accept<G: PcCandidatePageGuard>(
        &mut self,
        page: PcConcreteCandidatePage,
        guard: &G,
    ) -> Result<(), PcCandidateBoundaryError> {
        self.check_guard(guard)?;
        if self.terminal_completeness.is_some() {
            return Err(PcCandidateBoundaryError::AlreadyTerminal);
        }
        if page.source != self.source {
            return Err(PcCandidateBoundaryError::SourceBindingMismatch);
        }
        if page.cursor != self.expected_cursor {
            return Err(PcCandidateBoundaryError::CursorMismatch);
        }
        if let (Some(previous), Some(first)) = (self.candidates.last(), page.candidates.first()) {
            if first <= previous {
                return Err(PcCandidateBoundaryError::CandidatesNotStrictlyCanonical);
            }
        }

        let new_len = self
            .candidates
            .len()
            .checked_add(page.candidates.len())
            .ok_or(PcCandidateBoundaryError::CandidateOrdinalOverflow)?;
        let terminal_completeness = if page.terminal {
            Some(match &page.completeness {
                Some(evidence) => {
                    if evidence.source != self.source {
                        return Err(PcCandidateBoundaryError::CompletenessBindingMismatch);
                    }
                    let actual_count = u64::try_from(new_len)
                        .map_err(|_| PcCandidateBoundaryError::CandidateOrdinalOverflow)?;
                    if evidence.exact_candidate_count != actual_count {
                        return Err(PcCandidateBoundaryError::CompletenessCountMismatch);
                    }
                    let actual_digest =
                        PcCandidateSetDigest::calculate_parts(&self.candidates, &page.candidates)?;
                    if evidence.candidate_set_digest != actual_digest {
                        return Err(PcCandidateBoundaryError::CompletenessDigestMismatch);
                    }
                    PcCandidateCollectionCompleteness::CompleteRequestUniverse
                }
                None => PcCandidateCollectionCompleteness::PartialKnownCandidates,
            })
        } else {
            None
        };

        self.check_guard(guard)?;
        self.candidates
            .try_reserve_exact(page.candidates.len())
            .map_err(|_| PcCandidateBoundaryError::CandidateAllocationFailed)?;
        self.candidates.extend(page.candidates);
        self.expected_cursor = page.next_cursor;
        self.terminal_completeness = terminal_completeness;
        self.complete_universe_evidence = page.completeness;
        Ok(())
    }

    pub fn finish(self) -> Result<PcCandidateCollection, PcCandidateBoundaryError> {
        let completeness = self
            .terminal_completeness
            .ok_or(PcCandidateBoundaryError::SourceNotTerminal)?;
        Ok(PcCandidateCollection {
            source: self.source,
            candidates: self.candidates,
            completeness,
            complete_universe_evidence: self.complete_universe_evidence,
        })
    }

    fn check_guard<G: PcCandidatePageGuard>(
        &self,
        guard: &G,
    ) -> Result<(), PcCandidateBoundaryError> {
        if guard.is_cancelled() {
            return Err(PcCandidateBoundaryError::Cancelled);
        }
        if !guard.is_current_source(&self.source) {
            return Err(PcCandidateBoundaryError::StaleSession);
        }
        if self
            .source
            .qualified_snapshot()
            .is_some_and(|snapshot| !guard.is_current_snapshot(snapshot))
        {
            return Err(PcCandidateBoundaryError::StaleSnapshot);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcCandidateBoundaryError {
    Cancelled,
    StaleSession,
    StaleSnapshot,
    SourceBindingMismatch,
    CursorMismatch,
    EmptyNonTerminalPage,
    InitialBoardMismatch,
    CandidatesNotStrictlyCanonical,
    CandidateOrdinalOverflow,
    CandidateAllocationFailed,
    CompletenessBindingMismatch,
    CompletenessTargetBindingMismatch,
    CompletenessCountMismatch,
    CompletenessDigestMismatch,
    AlreadyTerminal,
    SourceNotTerminal,
    IncompleteCannotReduce,
}

impl PcCandidateBoundaryError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Cancelled => "pc_candidate_page_cancelled",
            Self::StaleSession => "pc_candidate_page_stale_session",
            Self::StaleSnapshot => "pc_candidate_page_stale_snapshot",
            Self::SourceBindingMismatch => "pc_candidate_page_source_binding_mismatch",
            Self::CursorMismatch => "pc_candidate_page_cursor_mismatch",
            Self::EmptyNonTerminalPage => "pc_candidate_page_empty_non_terminal",
            Self::InitialBoardMismatch => "pc_candidate_page_initial_board_mismatch",
            Self::CandidatesNotStrictlyCanonical => {
                "pc_candidate_page_candidates_not_strictly_canonical"
            }
            Self::CandidateOrdinalOverflow => "pc_candidate_page_ordinal_overflow",
            Self::CandidateAllocationFailed => "pc_candidate_page_allocation_failed",
            Self::CompletenessBindingMismatch => "pc_candidate_page_completeness_binding_mismatch",
            Self::CompletenessTargetBindingMismatch => {
                "pc_candidate_page_completeness_target_binding_mismatch"
            }
            Self::CompletenessCountMismatch => "pc_candidate_page_completeness_count_mismatch",
            Self::CompletenessDigestMismatch => "pc_candidate_page_completeness_digest_mismatch",
            Self::AlreadyTerminal => "pc_candidate_page_already_terminal",
            Self::SourceNotTerminal => "pc_candidate_page_source_not_terminal",
            Self::IncompleteCannotReduce => "pc_candidate_page_incomplete_cannot_reduce",
        }
    }
}

impl core::fmt::Display for PcCandidateBoundaryError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for PcCandidateBoundaryError {}

fn hash_qualified_target(
    hasher: &mut Sha256,
    target: &QualifiedPc4TargetIdentity,
) -> Result<(), PcCandidateRequestIdentityError> {
    let snapshot = target.snapshot();
    let snapshot_identity = snapshot.snapshot_identity();
    hash_bytes(hasher, snapshot_identity.repository().as_bytes())?;
    hash_bytes(hasher, snapshot_identity.revision().as_bytes())?;
    hash_bytes(hasher, snapshot_identity.generation().as_bytes())?;
    hash_bytes(
        hasher,
        snapshot.manifest_content_identity().as_str().as_bytes(),
    )?;
    hash_bytes(
        hasher,
        snapshot
            .verification_attestation()
            .evidence_identity()
            .as_bytes(),
    )?;
    hasher.update([profile_tag(target.profile())]);
    hasher.update([use_case_tag(target.use_case())]);
    hasher.update([target.target_lines().get()]);

    let qualification = target.qualification();
    let terminal_field = qualification.terminal_field();
    hasher.update(terminal_field.field_id().to_be_bytes());
    hasher.update(terminal_field.field_hash().to_be_bytes());
    hash_bytes(
        hasher,
        qualification.terminal_semantics_identity().as_bytes(),
    )?;
    hash_bytes(
        hasher,
        qualification
            .outgoing_edge_completeness_identity()
            .as_bytes(),
    )?;
    hash_bytes(hasher, qualification.known_answer_identity().as_bytes())?;
    hash_bytes(
        hasher,
        qualification.offline_exact_parity_identity().as_bytes(),
    )?;
    Ok(())
}

fn candidate_universe_hasher(
    target: &QualifiedPc4TargetIdentity,
    initial_board: StandardPcBoard,
    initial_hold: FixedQueueHoldState,
) -> Result<Sha256, PcCandidateRequestIdentityError> {
    let mut hasher = Sha256::new();
    hasher.update(CANDIDATE_REQUEST_IDENTITY_DOMAIN);
    hash_qualified_target(&mut hasher, target)?;
    hasher.update([initial_board.lines()]);
    for word in initial_board.occupied().words() {
        hasher.update(word.to_be_bytes());
    }
    hash_hold_state(&mut hasher, initial_hold);
    Ok(hasher)
}

fn hash_bytes(hasher: &mut Sha256, value: &[u8]) -> Result<(), PcCandidateRequestIdentityError> {
    hash_usize(hasher, value.len())?;
    hasher.update(value);
    Ok(())
}

fn hash_usize(hasher: &mut Sha256, value: usize) -> Result<(), PcCandidateRequestIdentityError> {
    let value = u64::try_from(value)
        .map_err(|_| PcCandidateRequestIdentityError::CanonicalLengthOverflow)?;
    hasher.update(value.to_be_bytes());
    Ok(())
}

fn hash_pieces(
    hasher: &mut Sha256,
    pieces: &[Pc4GraphPiece],
) -> Result<(), PcCandidateRequestIdentityError> {
    hash_usize(hasher, pieces.len())?;
    for &piece in pieces {
        hasher.update([piece_tag(piece)]);
    }
    Ok(())
}

fn hash_hold_state(hasher: &mut Sha256, hold: FixedQueueHoldState) {
    match hold {
        FixedQueueHoldState::Disabled => hasher.update([0]),
        FixedQueueHoldState::Empty => hasher.update([1]),
        FixedQueueHoldState::Occupied(piece) => hasher.update([2, piece_tag(piece)]),
    }
}

const fn profile_tag(profile: Pc4RuleProfile) -> u8 {
    match profile {
        Pc4RuleProfile::Srs => 0,
        Pc4RuleProfile::SrsPlus => 1,
        Pc4RuleProfile::SrsX => 2,
        Pc4RuleProfile::Jstris180 => 3,
        Pc4RuleProfile::NoKick => 4,
    }
}

const fn use_case_tag(use_case: Pc4TerminalUseCase) -> u8 {
    match use_case {
        Pc4TerminalUseCase::PcSearch => 0,
        Pc4TerminalUseCase::SetupSearch => 1,
    }
}

const fn piece_tag(piece: Pc4GraphPiece) -> u8 {
    match piece {
        Pc4GraphPiece::I => 0,
        Pc4GraphPiece::O => 1,
        Pc4GraphPiece::T => 2,
        Pc4GraphPiece::S => 3,
        Pc4GraphPiece::Z => 4,
        Pc4GraphPiece::J => 5,
        Pc4GraphPiece::L => 6,
    }
}

const fn hidden_source_tag(source: Pc4HiddenQueueSource) -> u8 {
    match source {
        Pc4HiddenQueueSource::Pattern => 0,
        Pc4HiddenQueueSource::HiddenQueue => 1,
    }
}

fn validate_complete_target_binding(
    source: &PcCandidateSourceBinding,
    qualified_target: Option<&QualifiedPc4TargetIdentity>,
) -> Result<(), PcCandidateBoundaryError> {
    match (source.provider_kind(), qualified_target) {
        (PcCandidateProviderKind::OfflineExact, None) => Ok(()),
        (PcCandidateProviderKind::OnlinePc4, Some(target))
            if source.profile() == target.profile()
                && source.qualified_snapshot() == Some(target.snapshot()) =>
        {
            Ok(())
        }
        (PcCandidateProviderKind::OfflineExact, Some(_))
        | (PcCandidateProviderKind::OnlinePc4, None)
        | (PcCandidateProviderKind::OnlinePc4, Some(_)) => {
            Err(PcCandidateBoundaryError::CompletenessTargetBindingMismatch)
        }
    }
}

fn validate_candidate_order(
    candidates: &[StandardBoard64TilingIdentity],
) -> Result<(), PcCandidateBoundaryError> {
    if candidates.windows(2).any(|pair| pair[0] >= pair[1]) {
        Err(PcCandidateBoundaryError::CandidatesNotStrictlyCanonical)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
