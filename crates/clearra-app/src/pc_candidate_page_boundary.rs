// SRP rationale: this module has one behavior-level change reason: accepting
// request-bound canonical PC candidate pages and sealing only a source-proven
// complete universe for the shared application reducer boundary. Transport,
// tablebase parsing, placement materialization, coverage reduction, fallback,
// and product presentation remain owned by their existing layers.

use core::num::NonZeroU64;

use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
use clearra_pc4_tablebase::{Pc4RuleProfile, QualifiedSnapshotIdentity};
use sha2::{Digest, Sha256};

pub const PC_CANDIDATE_PAGE_CONTRACT: &str = "pc-concrete-candidate-page.v1";
pub const PC_CANDIDATE_SET_DIGEST_ALGORITHM: &str = "sha256:clearra-pc-canonical-candidate-set-v1";

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

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

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

    pub fn online_pc4(
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

    #[cfg(test)]
    fn calculate(
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
    exact_candidate_count: u64,
    candidate_set_digest: PcCandidateSetDigest,
}

impl PcCandidateCompletenessEvidence {
    #[cfg(test)]
    fn from_verified_complete_source(
        source: PcCandidateSourceBinding,
        exact_candidate_count: u64,
        candidate_set_digest: PcCandidateSetDigest,
    ) -> Self {
        Self {
            source,
            exact_candidate_count,
            candidate_set_digest,
        }
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
        Ok(PcCandidateReducerInput {
            source: self.source,
            candidates: self.candidates,
        })
    }
}

/// Complete provider-neutral candidate universe before coverage reduction.
/// This is intentionally not an `ExecutionResult` and carries no claim that
/// coverage, probability, score, or replay work has run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcCandidateReducerInput {
    source: PcCandidateSourceBinding,
    candidates: Vec<StandardBoard64TilingIdentity>,
}

impl PcCandidateReducerInput {
    #[cfg(test)]
    pub(crate) fn from_test_parts(
        source: PcCandidateSourceBinding,
        candidates: Vec<StandardBoard64TilingIdentity>,
    ) -> Self {
        Self { source, candidates }
    }

    pub const fn source(&self) -> &PcCandidateSourceBinding {
        &self.source
    }

    pub fn candidates(&self) -> &[StandardBoard64TilingIdentity] {
        &self.candidates
    }
}

pub struct PcCandidatePageCollector {
    source: PcCandidateSourceBinding,
    expected_cursor: PcCandidatePageCursor,
    candidates: Vec<StandardBoard64TilingIdentity>,
    terminal_completeness: Option<PcCandidateCollectionCompleteness>,
}

impl PcCandidatePageCollector {
    pub fn new(source: PcCandidateSourceBinding) -> Self {
        Self {
            source,
            expected_cursor: PcCandidatePageCursor::initial(),
            candidates: Vec::new(),
            terminal_completeness: None,
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
