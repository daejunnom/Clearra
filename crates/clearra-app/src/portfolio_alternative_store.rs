// SRP rationale: this module has one behavior-level change reason: enumerating, checkpointing, and paging deterministic coverage-portfolio alternatives.

use std::sync::Arc;

use clearra_coverage::{
    cover::{
        ExactMinimumCoverEnumerationStop, ExactMinimumCoverPortfolioEnumerator,
        ExactMinimumCoverPortfolioError, ExactMinimumCoverPortfolioPage,
        ExactMinimumCoverPortfolioPreparationAdvance, ExactMinimumCoverPortfolioPreparationSession,
    },
    pattern::pattern_bitset::PatternBitSet,
};
use sha2::{Digest, Sha256};

pub const PORTFOLIO_ALTERNATIVE_SET_CONTRACT: &str = "portfolio-alternative-set.v1";
pub const PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT: &str = "portfolio-alternative-page.v1";
pub const PORTFOLIO_MEMBER_PAGE_CONTRACT: &str = "portfolio-member-page.v1";
pub const PORTFOLIO_SNAPSHOT_CONTRACT: &str = "portfolio-snapshot.v1";
pub const PORTFOLIO_MEMBER_PAGE_SIZE: usize = 100;
pub const PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT: usize = 3;

// Work steps remain the primary replay budget. This secondary cap bounds runs
// of immediately available alternatives so hosts can observe cancellation.
const PORTFOLIO_REPLAY_PAGE_TRANSITION_LIMIT: usize = 64;

const CANDIDATE_MAP_DIGEST_DOMAIN: &[u8] = b"clearra.portfolio-candidate-map.v1\0";
const SET_IDENTITY_DIGEST_DOMAIN: &[u8] = b"clearra.portfolio-set-identity.v1\0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortfolioAlternativeSetIdentity {
    query_identity: String,
    source_identity: String,
    profile_identity: String,
    universe_identity: String,
    build_identity: String,
}

impl PortfolioAlternativeSetIdentity {
    pub fn new(
        query_identity: impl Into<String>,
        source_identity: impl Into<String>,
        profile_identity: impl Into<String>,
        universe_identity: impl Into<String>,
        build_identity: impl Into<String>,
    ) -> Result<Self, PortfolioAlternativeError> {
        let identity = Self {
            query_identity: query_identity.into(),
            source_identity: source_identity.into(),
            profile_identity: profile_identity.into(),
            universe_identity: universe_identity.into(),
            build_identity: build_identity.into(),
        };
        for (component, value) in [
            ("query", identity.query_identity.as_str()),
            ("source", identity.source_identity.as_str()),
            ("profile", identity.profile_identity.as_str()),
            ("universe", identity.universe_identity.as_str()),
            ("build", identity.build_identity.as_str()),
        ] {
            if value.is_empty() || value.chars().any(char::is_control) {
                return Err(PortfolioAlternativeError::InvalidIdentity { component });
            }
        }
        Ok(identity)
    }

    pub fn query_identity(&self) -> &str {
        &self.query_identity
    }

    pub fn source_identity(&self) -> &str {
        &self.source_identity
    }

    pub fn profile_identity(&self) -> &str {
        &self.profile_identity
    }

    pub fn universe_identity(&self) -> &str {
        &self.universe_identity
    }

    pub fn build_identity(&self) -> &str {
        &self.build_identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortfolioCandidate {
    candidate_id: u64,
    normalized_key: String,
}

impl PortfolioCandidate {
    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub fn normalized_key(&self) -> &str {
        &self.normalized_key
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortfolioAlternative {
    candidate_ids: Vec<u64>,
}

impl PortfolioAlternative {
    pub fn candidate_ids(&self) -> &[u64] {
        &self.candidate_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortfolioAlternativePage {
    contract_id: &'static str,
    set_identity_sha256: String,
    candidate_map_sha256: String,
    alternative_index_decimal: String,
    portfolio: PortfolioAlternative,
    optimal_cardinality: usize,
    known_alternative_count_decimal: String,
    total_alternative_count_decimal: Option<String>,
    enumeration_complete: bool,
}

impl PortfolioAlternativePage {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub fn alternative_index_decimal(&self) -> &str {
        &self.alternative_index_decimal
    }

    pub fn set_identity_sha256(&self) -> &str {
        &self.set_identity_sha256
    }

    pub fn candidate_map_sha256(&self) -> &str {
        &self.candidate_map_sha256
    }

    pub const fn portfolio(&self) -> &PortfolioAlternative {
        &self.portfolio
    }

    pub const fn optimal_cardinality(&self) -> usize {
        self.optimal_cardinality
    }

    pub fn known_alternative_count_decimal(&self) -> &str {
        &self.known_alternative_count_decimal
    }

    pub fn total_alternative_count_decimal(&self) -> Option<&str> {
        self.total_alternative_count_decimal.as_deref()
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.enumeration_complete
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortfolioEnumerationStop {
    PageFull,
    WorkBudgetExhausted,
    Cancelled,
    Sealed,
}

impl PortfolioEnumerationStop {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PageFull => "page-full",
            Self::WorkBudgetExhausted => "work-budget-exhausted",
            Self::Cancelled => "cancelled",
            Self::Sealed => "sealed",
        }
    }
}

impl From<ExactMinimumCoverEnumerationStop> for PortfolioEnumerationStop {
    fn from(value: ExactMinimumCoverEnumerationStop) -> Self {
        match value {
            ExactMinimumCoverEnumerationStop::PageFull => Self::PageFull,
            ExactMinimumCoverEnumerationStop::WorkBudgetExhausted => Self::WorkBudgetExhausted,
            ExactMinimumCoverEnumerationStop::Cancelled => Self::Cancelled,
            ExactMinimumCoverEnumerationStop::Sealed => Self::Sealed,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortfolioAlternativeCheckpoint {
    contract_id: &'static str,
    set_identity_sha256: String,
    candidate_map_sha256: String,
    optimal_cardinality: usize,
    next_combination: Option<Vec<usize>>,
    known_alternative_count_decimal: String,
    enumeration_complete: bool,
}

impl PortfolioAlternativeCheckpoint {
    pub fn from_restart_fields(
        set_identity_sha256: String,
        candidate_map_sha256: String,
        optimal_cardinality: usize,
        next_combination: Option<Vec<usize>>,
        known_alternative_count_decimal: String,
        enumeration_complete: bool,
    ) -> Result<Self, PortfolioAlternativeError> {
        if !is_sha256_hex(&set_identity_sha256)
            || !is_sha256_hex(&candidate_map_sha256)
            || optimal_cardinality == 0
            || !is_canonical_nonzero_decimal(&known_alternative_count_decimal)
            || (enumeration_complete && next_combination.is_some())
        {
            return Err(PortfolioAlternativeError::CheckpointIdentityMismatch);
        }
        Ok(Self {
            contract_id: PORTFOLIO_SNAPSHOT_CONTRACT,
            set_identity_sha256,
            candidate_map_sha256,
            optimal_cardinality,
            next_combination,
            known_alternative_count_decimal,
            enumeration_complete,
        })
    }

    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub fn set_identity_sha256(&self) -> &str {
        &self.set_identity_sha256
    }

    pub fn candidate_map_sha256(&self) -> &str {
        &self.candidate_map_sha256
    }

    pub const fn optimal_cardinality(&self) -> usize {
        self.optimal_cardinality
    }

    pub fn next_combination(&self) -> Option<&[usize]> {
        self.next_combination.as_deref()
    }

    pub fn known_alternative_count_decimal(&self) -> &str {
        &self.known_alternative_count_decimal
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.enumeration_complete
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortfolioAlternativeAdvance {
    page: Option<PortfolioAlternativePage>,
    stop: PortfolioEnumerationStop,
    work_steps: u64,
    checkpoint: PortfolioAlternativeCheckpoint,
}

impl PortfolioAlternativeAdvance {
    pub fn page(&self) -> Option<&PortfolioAlternativePage> {
        self.page.as_ref()
    }

    pub const fn stop(&self) -> PortfolioEnumerationStop {
        self.stop
    }

    pub const fn work_steps(&self) -> u64 {
        self.work_steps
    }

    pub const fn checkpoint(&self) -> &PortfolioAlternativeCheckpoint {
        &self.checkpoint
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = checked_checkpoint_retained_capacity_bytes(&self.checkpoint)?;
        if let Some(page) = &self.page {
            bytes = bytes.checked_add(checked_page_nested_retained_capacity_bytes(page)?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortfolioPageLoadState {
    Page,
    WorkBudgetExhausted,
    Cancelled,
}

impl PortfolioPageLoadState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::WorkBudgetExhausted => "work-budget-exhausted",
            Self::Cancelled => "cancelled",
        }
    }
}

/// One bounded cache-replay slice. Only `Page` carries a retained slot;
/// incomplete and cancelled slices can never be projected as an empty page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortfolioPageLoadAdvance {
    retained_slot: Option<usize>,
    state: PortfolioPageLoadState,
    work_steps: u64,
}

impl PortfolioPageLoadAdvance {
    pub const fn retained_slot(self) -> Option<usize> {
        self.retained_slot
    }

    pub const fn state(self) -> PortfolioPageLoadState {
        self.state
    }

    pub const fn work_steps(self) -> u64 {
        self.work_steps
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortfolioMember {
    candidate_id: u64,
    normalized_key: String,
}

impl PortfolioMember {
    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub fn normalized_key(&self) -> &str {
        &self.normalized_key
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortfolioMemberPage {
    contract_id: &'static str,
    alternative_index_decimal: String,
    member_page_number: usize,
    total_member_pages: usize,
    members: Vec<PortfolioMember>,
}

impl PortfolioMemberPage {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub fn alternative_index_decimal(&self) -> &str {
        &self.alternative_index_decimal
    }

    pub const fn member_page_number(&self) -> usize {
        self.member_page_number
    }

    pub const fn total_member_pages(&self) -> usize {
        self.total_member_pages
    }

    pub fn members(&self) -> &[PortfolioMember] {
        &self.members
    }
}

/// Trusted in-memory continuation immediately after the canonical portfolio.
///
/// The continuation is derived and checked while the immutable alternative
/// set is constructed. Keeping it with that owner lets every GUI/Desktop/WASM
/// page handle clone the already-proven exact frontier instead of proving the
/// same minimum and rediscovering page one on every open. It is never restored
/// from serialized fields: externally supplied checkpoints continue through
/// `resume_from_fields`, which independently reproves the optimum.
#[derive(Clone, Debug)]
struct CanonicalPortfolioContinuation {
    enumerator: ExactMinimumCoverPortfolioEnumerator,
}

impl PartialEq for CanonicalPortfolioContinuation {
    fn eq(&self, other: &Self) -> bool {
        self.enumerator.optimal_cardinality() == other.enumerator.optimal_cardinality()
            && self.enumerator.known_alternative_count_decimal()
                == other.enumerator.known_alternative_count_decimal()
            && self.enumerator.enumeration_complete() == other.enumerator.enumeration_complete()
            && self.enumerator.restart_state() == other.enumerator.restart_state()
    }
}

impl Eq for CanonicalPortfolioContinuation {}

#[derive(Debug)]
pub(crate) enum CoveragePortfolioAlternativeSetPreparationAdvance {
    Pending { work_steps: u64 },
    Completed(CoveragePortfolioAlternativeSet),
    Cancelled { work_steps: u64 },
}

enum CoveragePortfolioAlternativeSetPreparationState {
    Proving(ExactMinimumCoverPortfolioPreparationSession),
    SelectingCanonical(ExactMinimumCoverPortfolioEnumerator),
    Finished,
}

/// Product-side continuation that proves `k*` and selects the first
/// original-row lexicographic portfolio without replaying either authority.
/// Candidate IDs and digests are fixed before the proof starts, while every
/// expensive exact step is driven only from [`Self::advance`].
pub(crate) struct CoveragePortfolioAlternativeSetPreparation {
    identity: PortfolioAlternativeSetIdentity,
    set_identity_sha256: String,
    candidate_map_sha256: String,
    candidates: Vec<PortfolioCandidate>,
    required: PatternBitSet,
    rows: Vec<PatternBitSet>,
    state: CoveragePortfolioAlternativeSetPreparationState,
}

impl CoveragePortfolioAlternativeSetPreparation {
    pub(crate) fn new(
        identity: PortfolioAlternativeSetIdentity,
        candidate_keys: Vec<String>,
        required: PatternBitSet,
        rows: Vec<PatternBitSet>,
    ) -> Result<Self, PortfolioAlternativeError> {
        let (candidate_keys, rows) =
            canonicalize_candidate_keys_and_rows(candidate_keys, rows, &required)?;
        let candidates = candidate_keys
            .into_iter()
            .enumerate()
            .map(|(index, normalized_key)| {
                let candidate_id = u64::try_from(index)
                    .ok()
                    .and_then(|index| index.checked_add(1))
                    .ok_or(PortfolioAlternativeError::CandidateCountOverflow)?;
                Ok(PortfolioCandidate {
                    candidate_id,
                    normalized_key,
                })
            })
            .collect::<Result<Vec<_>, PortfolioAlternativeError>>()?;
        let candidate_map_sha256 = candidate_map_digest(&candidates);
        let set_identity_sha256 = set_identity_digest(&identity, &candidate_map_sha256);
        let proof = ExactMinimumCoverPortfolioPreparationSession::new(&required, &rows)
            .map_err(PortfolioAlternativeError::Enumeration)?;
        Ok(Self {
            identity,
            set_identity_sha256,
            candidate_map_sha256,
            candidates,
            required,
            rows,
            state: CoveragePortfolioAlternativeSetPreparationState::Proving(proof),
        })
    }

    pub(crate) fn advance(
        &mut self,
        maximum_work_steps: u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<CoveragePortfolioAlternativeSetPreparationAdvance, PortfolioAlternativeError> {
        if cancelled() {
            self.state = CoveragePortfolioAlternativeSetPreparationState::Finished;
            return Ok(
                CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled { work_steps: 0 },
            );
        }
        if maximum_work_steps == 0 {
            return Ok(CoveragePortfolioAlternativeSetPreparationAdvance::Pending {
                work_steps: 0,
            });
        }

        let mut consumed = 0_u64;
        loop {
            let state = core::mem::replace(
                &mut self.state,
                CoveragePortfolioAlternativeSetPreparationState::Finished,
            );
            match state {
                CoveragePortfolioAlternativeSetPreparationState::Proving(mut proof) => {
                    let remaining = maximum_work_steps.saturating_sub(consumed);
                    match proof
                        .advance_with_memory_guard_and_control(
                            remaining,
                            &mut |_| Ok(()),
                            cancelled,
                        )
                        .map_err(PortfolioAlternativeError::Enumeration)?
                    {
                        ExactMinimumCoverPortfolioPreparationAdvance::Pending { visited_nodes } => {
                            consumed = consumed.saturating_add(visited_nodes);
                            self.state =
                                CoveragePortfolioAlternativeSetPreparationState::Proving(proof);
                            return Ok(
                                CoveragePortfolioAlternativeSetPreparationAdvance::Pending {
                                    work_steps: consumed,
                                },
                            );
                        }
                        ExactMinimumCoverPortfolioPreparationAdvance::Coverable {
                            proof: exact,
                            enumerator,
                            visited_nodes,
                        } => {
                            consumed = consumed.saturating_add(visited_nodes);
                            if !exact.complete()
                                || exact.covered_patterns() != &self.required
                                || exact.row_indices().len() != enumerator.optimal_cardinality()
                            {
                                return Err(PortfolioAlternativeError::Enumeration(
                                    ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof,
                                ));
                            }
                            self.state =
                                CoveragePortfolioAlternativeSetPreparationState::SelectingCanonical(
                                    enumerator,
                                );
                            if consumed >= maximum_work_steps {
                                return Ok(
                                    CoveragePortfolioAlternativeSetPreparationAdvance::Pending {
                                        work_steps: consumed,
                                    },
                                );
                            }
                        }
                        ExactMinimumCoverPortfolioPreparationAdvance::Incomplete {
                            proof: exact,
                            ..
                        } => {
                            return Err(PortfolioAlternativeError::Enumeration(
                                ExactMinimumCoverPortfolioError::RequiredPatternsNotCoverable {
                                    covered_pattern_count: exact.covered_patterns().count_ones(),
                                    required_pattern_count: self.required.count_ones(),
                                },
                            ));
                        }
                        ExactMinimumCoverPortfolioPreparationAdvance::Cancelled {
                            visited_nodes,
                        } => {
                            return Ok(
                                CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled {
                                    work_steps: consumed.saturating_add(visited_nodes),
                                },
                            );
                        }
                        ExactMinimumCoverPortfolioPreparationAdvance::Finished => {
                            return Err(PortfolioAlternativeError::Enumeration(
                                ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof,
                            ));
                        }
                    }
                }
                CoveragePortfolioAlternativeSetPreparationState::SelectingCanonical(
                    mut enumerator,
                ) => {
                    let remaining = maximum_work_steps.saturating_sub(consumed);
                    let page = enumerator
                        .next_page_owned_with_memory_guard_and_control(
                            1,
                            remaining,
                            &mut |_| Ok(()),
                            cancelled,
                        )
                        .map_err(PortfolioAlternativeError::Enumeration)?;
                    consumed = consumed.saturating_add(page.work_steps());
                    if page.stop() == ExactMinimumCoverEnumerationStop::Cancelled {
                        return Ok(
                            CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled {
                                work_steps: consumed,
                            },
                        );
                    }
                    let canonical =
                        match page.portfolios() {
                            [] => {
                                self.state = CoveragePortfolioAlternativeSetPreparationState::
                                SelectingCanonical(enumerator);
                                return Ok(
                                    CoveragePortfolioAlternativeSetPreparationAdvance::Pending {
                                        work_steps: consumed,
                                    },
                                );
                            }
                            [canonical] => canonical,
                            _ => return Err(PortfolioAlternativeError::PageCardinalityMismatch),
                        };
                    let optimal_cardinality = enumerator.optimal_cardinality();
                    if canonical.row_indices().len() != optimal_cardinality {
                        return Err(PortfolioAlternativeError::CanonicalPortfolioMismatch);
                    }
                    let canonical_page = PortfolioAlternativePage {
                        contract_id: PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT,
                        set_identity_sha256: self.set_identity_sha256.clone(),
                        candidate_map_sha256: self.candidate_map_sha256.clone(),
                        alternative_index_decimal: "1".to_owned(),
                        portfolio: portfolio_from_rows(canonical.row_indices())?,
                        optimal_cardinality,
                        known_alternative_count_decimal: page
                            .known_alternative_count_decimal()
                            .to_owned(),
                        total_alternative_count_decimal: page
                            .total_alternative_count_decimal()
                            .map(ToOwned::to_owned),
                        enumeration_complete: page.enumeration_complete(),
                    };
                    let set = CoveragePortfolioAlternativeSet {
                        contract_id: PORTFOLIO_ALTERNATIVE_SET_CONTRACT,
                        identity: self.identity.clone(),
                        set_identity_sha256: self.set_identity_sha256.clone(),
                        candidate_map_sha256: self.candidate_map_sha256.clone(),
                        candidates: core::mem::take(&mut self.candidates).into(),
                        public_candidate_ids: None,
                        required: self.required.clone(),
                        rows: core::mem::take(&mut self.rows).into(),
                        optimal_cardinality,
                        canonical_page,
                        canonical_continuation: CanonicalPortfolioContinuation { enumerator },
                    };
                    return Ok(CoveragePortfolioAlternativeSetPreparationAdvance::Completed(set));
                }
                CoveragePortfolioAlternativeSetPreparationState::Finished => {
                    return Err(PortfolioAlternativeError::CanonicalPortfolioMissing);
                }
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoveragePortfolioAlternativeSet {
    contract_id: &'static str,
    identity: PortfolioAlternativeSetIdentity,
    set_identity_sha256: String,
    candidate_map_sha256: String,
    candidates: Arc<[PortfolioCandidate]>,
    /// Optional product-owned public IDs indexed by the store's stable dense
    /// candidate ID. Coverage enumeration always remains dense; only the
    /// transport-facing member identity is projected through this map.
    public_candidate_ids: Option<Arc<[u64]>>,
    required: PatternBitSet,
    rows: Arc<[PatternBitSet]>,
    optimal_cardinality: usize,
    canonical_page: PortfolioAlternativePage,
    canonical_continuation: CanonicalPortfolioContinuation,
}

impl CoveragePortfolioAlternativeSet {
    /// Builds the immutable all-optima set and verifies an independently
    /// supplied canonical selection. Use this at producer trust boundaries
    /// where the producer is required to materialize the same lexicographic
    /// first portfolio.
    pub fn new(
        identity: PortfolioAlternativeSetIdentity,
        candidate_keys: Vec<String>,
        required: PatternBitSet,
        rows: Vec<PatternBitSet>,
        expected_canonical_keys: &[String],
    ) -> Result<Self, PortfolioAlternativeError> {
        let set = Self::new_canonical(identity, candidate_keys, required, rows)?;
        let actual_keys = keys_for_portfolio(&set.candidates, set.canonical_page.portfolio())?;
        if actual_keys
            .iter()
            .copied()
            .ne(expected_canonical_keys.iter().map(String::as_str))
        {
            return Err(PortfolioAlternativeError::CanonicalPortfolioMismatch);
        }
        Ok(set)
    }

    /// Builds the exact set with its canonical representative derived from
    /// the original-row lexicographic portfolio authority itself. Product
    /// reducers that do not receive a separately authoritative canonical
    /// selection must use this constructor rather than treating the
    /// branch-and-bound proof row as a presentation identity.
    pub fn new_canonical(
        identity: PortfolioAlternativeSetIdentity,
        candidate_keys: Vec<String>,
        required: PatternBitSet,
        rows: Vec<PatternBitSet>,
    ) -> Result<Self, PortfolioAlternativeError> {
        let mut preparation = CoveragePortfolioAlternativeSetPreparation::new(
            identity,
            candidate_keys,
            required,
            rows,
        )?;
        loop {
            match preparation.advance(u64::MAX, &mut || false)? {
                CoveragePortfolioAlternativeSetPreparationAdvance::Pending { work_steps } => {
                    if work_steps == 0 {
                        return Err(PortfolioAlternativeError::CanonicalPortfolioMissing);
                    }
                }
                CoveragePortfolioAlternativeSetPreparationAdvance::Completed(set) => {
                    return Ok(set)
                }
                CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled { .. } => {
                    return Err(PortfolioAlternativeError::CanonicalPortfolioMissing)
                }
            }
        }
    }

    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn identity(&self) -> &PortfolioAlternativeSetIdentity {
        &self.identity
    }

    pub fn set_identity_sha256(&self) -> &str {
        &self.set_identity_sha256
    }

    pub fn candidate_map_sha256(&self) -> &str {
        &self.candidate_map_sha256
    }

    pub fn candidates(&self) -> &[PortfolioCandidate] {
        &self.candidates
    }

    /// Attaches a product-owned public candidate identity without changing
    /// the dense IDs used by exact-cover enumeration. The supplied order must
    /// match the already canonicalized candidate-key order.
    ///
    /// The default constructor deliberately leaves this map absent, preserving
    /// the existing pc.minimals candidate-map digest and serialized bytes.
    pub fn with_public_candidate_ids(
        mut self,
        public_candidate_ids: Vec<u64>,
    ) -> Result<Self, PortfolioAlternativeError> {
        if public_candidate_ids.len() != self.candidates.len() || public_candidate_ids.contains(&0)
        {
            return Err(PortfolioAlternativeError::PublicCandidateMapInvalid);
        }
        let mut canonical_ids = public_candidate_ids.clone();
        canonical_ids.sort_unstable();
        if canonical_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(PortfolioAlternativeError::PublicCandidateMapInvalid);
        }
        self.public_candidate_ids = Some(public_candidate_ids.into());
        Ok(self)
    }

    /// Resolves a stable dense portfolio ID to the product's public candidate
    /// identity. Products without a separate identity map preserve the dense
    /// ID exactly.
    pub fn public_candidate_id(&self, dense_candidate_id: u64) -> Option<u64> {
        let index = candidate_index(dense_candidate_id).ok()?;
        match &self.public_candidate_ids {
            Some(candidate_ids) => candidate_ids.get(index).copied(),
            None => self
                .candidates
                .get(index)
                .filter(|candidate| candidate.candidate_id == dense_candidate_id)
                .map(|candidate| candidate.candidate_id),
        }
    }

    pub const fn required_patterns(&self) -> &PatternBitSet {
        &self.required
    }

    pub fn coverage_rows(&self) -> &[PatternBitSet] {
        &self.rows
    }

    pub const fn optimal_cardinality(&self) -> usize {
        self.optimal_cardinality
    }

    pub const fn canonical_page(&self) -> &PortfolioAlternativePage {
        &self.canonical_page
    }

    /// Returns every member key of the canonical outer alternative. This is
    /// the presentation identity for a minimum-cover product; the exact
    /// branch-and-bound proof row is intentionally not exposed as canonical.
    pub fn canonical_candidate_keys_owned(&self) -> Result<Vec<String>, PortfolioAlternativeError> {
        keys_for_portfolio(&self.candidates, self.canonical_page.portfolio())
            .map(|keys| keys.into_iter().map(str::to_owned).collect())
    }

    pub fn known_alternative_count_decimal(&self) -> &str {
        self.canonical_page.known_alternative_count_decimal()
    }

    pub fn total_alternative_count_decimal(&self) -> Option<&str> {
        self.canonical_page.total_alternative_count_decimal()
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.canonical_page.enumeration_complete()
    }

    pub const fn incomplete_reason(&self) -> Option<&'static str> {
        if self.enumeration_complete() {
            None
        } else {
            Some("enumeration-pending")
        }
    }

    /// Exact heap payload retained by this immutable set owner. Inline values,
    /// allocator metadata, and `Arc` control blocks are excluded. Candidate
    /// strings and the independently retained coverage-row storage are both
    /// included, so callers cannot account only the small canonical page while
    /// silently omitting the restartable source universe.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.identity.query_identity.capacity(),
            self.identity.source_identity.capacity(),
            self.identity.profile_identity.capacity(),
            self.identity.universe_identity.capacity(),
            self.identity.build_identity.capacity(),
            self.set_identity_sha256.capacity(),
            self.candidate_map_sha256.capacity(),
            self.canonical_page.set_identity_sha256.capacity(),
            self.canonical_page.candidate_map_sha256.capacity(),
            self.canonical_page.alternative_index_decimal.capacity(),
            self.canonical_page
                .known_alternative_count_decimal
                .capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?;
        if let Some(total) = &self.canonical_page.total_alternative_count_decimal {
            bytes = bytes.checked_add(total.capacity() as u128)?;
        }
        bytes = bytes.checked_add(
            (self.candidates.len() as u128)
                .checked_mul(core::mem::size_of::<PortfolioCandidate>() as u128)?,
        )?;
        if let Some(public_candidate_ids) = &self.public_candidate_ids {
            bytes = bytes.checked_add(
                (public_candidate_ids.len() as u128)
                    .checked_mul(core::mem::size_of::<u64>() as u128)?,
            )?;
        }
        for candidate in self.candidates.iter() {
            bytes = bytes.checked_add(candidate.normalized_key.capacity() as u128)?;
        }
        bytes = bytes.checked_add(self.required.checked_storage_retained_bytes()?)?;
        bytes = bytes.checked_add(
            (self.rows.len() as u128).checked_mul(core::mem::size_of::<PatternBitSet>() as u128)?,
        )?;
        for row in self.rows.iter() {
            bytes = bytes.checked_add(row.checked_storage_retained_bytes()?)?;
        }
        bytes = bytes.checked_add(
            self.canonical_continuation
                .enumerator
                .checked_retained_capacity_bytes()?,
        )?;
        bytes = bytes.checked_add(
            (self.canonical_page.portfolio.candidate_ids.capacity() as u128)
                .checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?;
        Some(bytes)
    }

    pub fn open_store(
        &self,
    ) -> Result<CoveragePortfolioAlternativeStore, PortfolioAlternativeError> {
        Self::open_owned_store(Arc::new(self.clone()))
    }

    /// Opens a restartable enumerator while retaining the caller's immutable
    /// set owner. GUI and WASM page handles use this path so the candidate map
    /// and coverage rows are not deep-cloned at the transport boundary.
    pub fn open_owned_store(
        set: Arc<Self>,
    ) -> Result<CoveragePortfolioAlternativeStore, PortfolioAlternativeError> {
        let enumerator = set.canonical_continuation.enumerator.clone();
        if enumerator.optimal_cardinality() != set.optimal_cardinality
            || enumerator.known_alternative_count_decimal()
                != set.canonical_page.known_alternative_count_decimal
            || enumerator.enumeration_complete() != set.canonical_page.enumeration_complete
        {
            return Err(PortfolioAlternativeError::CanonicalPortfolioMismatch);
        }
        Ok(CoveragePortfolioAlternativeStore { set, enumerator })
    }

    pub fn resume_store(
        &self,
        checkpoint: &PortfolioAlternativeCheckpoint,
    ) -> Result<CoveragePortfolioAlternativeStore, PortfolioAlternativeError> {
        Self::resume_owned_store(Arc::new(self.clone()), checkpoint)
    }

    pub fn resume_owned_store(
        set: Arc<Self>,
        checkpoint: &PortfolioAlternativeCheckpoint,
    ) -> Result<CoveragePortfolioAlternativeStore, PortfolioAlternativeError> {
        if checkpoint.contract_id != PORTFOLIO_SNAPSHOT_CONTRACT
            || checkpoint.set_identity_sha256 != set.set_identity_sha256
            || checkpoint.candidate_map_sha256 != set.candidate_map_sha256
        {
            return Err(PortfolioAlternativeError::CheckpointIdentityMismatch);
        }
        let enumerator = set
            .canonical_continuation
            .enumerator
            .resume_from_proven_fields(
                checkpoint.optimal_cardinality,
                checkpoint.next_combination.clone(),
                &checkpoint.known_alternative_count_decimal,
                checkpoint.enumeration_complete,
            )
            .map_err(PortfolioAlternativeError::Enumeration)?;
        Ok(CoveragePortfolioAlternativeStore { set, enumerator })
    }

    pub fn member_page(
        &self,
        page: &PortfolioAlternativePage,
        member_page_number: usize,
    ) -> Result<PortfolioMemberPage, PortfolioAlternativeError> {
        if page.set_identity_sha256 != self.set_identity_sha256
            || page.candidate_map_sha256 != self.candidate_map_sha256
        {
            return Err(PortfolioAlternativeError::PageIdentityMismatch);
        }
        let member_count = page.portfolio.candidate_ids.len();
        let total_member_pages = member_count.div_ceil(PORTFOLIO_MEMBER_PAGE_SIZE).max(1);
        if member_page_number == 0 || member_page_number > total_member_pages {
            return Err(PortfolioAlternativeError::InvalidMemberPage);
        }
        let start = (member_page_number - 1)
            .checked_mul(PORTFOLIO_MEMBER_PAGE_SIZE)
            .ok_or(PortfolioAlternativeError::InvalidMemberPage)?;
        let end = start
            .saturating_add(PORTFOLIO_MEMBER_PAGE_SIZE)
            .min(member_count);
        let mut members = Vec::new();
        members
            .try_reserve_exact(end.saturating_sub(start))
            .map_err(|_| PortfolioAlternativeError::AllocationFailed)?;
        for candidate_id in &page.portfolio.candidate_ids[start..end] {
            let candidate = self
                .candidates
                .get(candidate_index(*candidate_id)?)
                .ok_or(PortfolioAlternativeError::InvalidCandidateId)?;
            if candidate.candidate_id != *candidate_id {
                return Err(PortfolioAlternativeError::InvalidCandidateId);
            }
            members.push(PortfolioMember {
                candidate_id: self
                    .public_candidate_id(*candidate_id)
                    .ok_or(PortfolioAlternativeError::InvalidCandidateId)?,
                normalized_key: candidate.normalized_key.clone(),
            });
        }
        Ok(PortfolioMemberPage {
            contract_id: PORTFOLIO_MEMBER_PAGE_CONTRACT,
            alternative_index_decimal: page.alternative_index_decimal.clone(),
            member_page_number,
            total_member_pages,
            members,
        })
    }
}

#[derive(Clone, Debug)]
pub struct CoveragePortfolioAlternativeStore {
    set: Arc<CoveragePortfolioAlternativeSet>,
    enumerator: ExactMinimumCoverPortfolioEnumerator,
}

impl CoveragePortfolioAlternativeStore {
    pub fn alternative_set(&self) -> &Arc<CoveragePortfolioAlternativeSet> {
        &self.set
    }

    pub fn checked_enumerator_retained_capacity_bytes(&self) -> Option<u128> {
        self.enumerator.checked_retained_capacity_bytes()
    }

    pub fn next_page(
        &mut self,
        maximum_work_steps: u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PortfolioAlternativeAdvance, PortfolioAlternativeError> {
        self.next_page_with_memory_guard(maximum_work_steps, &mut |_| true, cancelled)
    }

    /// Advances the high-water enumerator while reporting its full retained
    /// owner plus every exact and App-page allocation that coexists with it.
    /// The caller adds any persistent owners outside this store. Returning
    /// `false` is a hard, fail-closed denial and can never be reinterpreted as
    /// an empty or sealed portfolio family.
    pub fn next_page_with_memory_guard(
        &mut self,
        maximum_work_steps: u64,
        memory_admission: &mut impl FnMut(u128) -> bool,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PortfolioAlternativeAdvance, PortfolioAlternativeError> {
        let original_live = self
            .checked_enumerator_retained_capacity_bytes()
            .ok_or_else(portfolio_projection_overflow)?;
        guard_portfolio_memory(original_live, memory_admission)?;
        let mut staged = self.try_clone_with_memory_guard(original_live, memory_admission)?;
        let mut overflowed = false;
        let advance_result = staged.next_page_in_place_with_memory_guard(
            maximum_work_steps,
            &mut |staged_peak| {
                let Some(whole_live) = original_live.checked_add(staged_peak) else {
                    overflowed = true;
                    return false;
                };
                memory_admission(whole_live)
            },
            cancelled,
        );
        if overflowed {
            return Err(portfolio_projection_overflow());
        }
        let advance = advance_result?;
        guard_portfolio_memory(
            original_live
                .checked_add(
                    staged
                        .checked_enumerator_retained_capacity_bytes()
                        .ok_or_else(portfolio_projection_overflow)?,
                )
                .and_then(|bytes| bytes.checked_add(advance.checked_retained_capacity_bytes()?))
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
        *self = staged;
        Ok(advance)
    }

    fn try_clone_with_memory_guard(
        &self,
        external_live_bytes: u128,
        memory_admission: &mut impl FnMut(u128) -> bool,
    ) -> Result<Self, PortfolioAlternativeError> {
        let enumerator = self
            .enumerator
            .try_clone_with_memory_guard(&mut |staged_enumerator_peak| {
                let whole_live = external_live_bytes
                    .checked_add(staged_enumerator_peak)
                    .ok_or(clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow)?;
                if memory_admission(whole_live) {
                    Ok(())
                } else {
                    Err(clearra_coverage::cover::ExactMinimumCoverError::MemoryGuardRejected)
                }
            })
            .map_err(PortfolioAlternativeError::Enumeration)?;
        Ok(Self {
            set: Arc::clone(&self.set),
            enumerator,
        })
    }

    fn next_page_in_place_with_memory_guard(
        &mut self,
        maximum_work_steps: u64,
        memory_admission: &mut impl FnMut(u128) -> bool,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PortfolioAlternativeAdvance, PortfolioAlternativeError> {
        let page = self
            .enumerator
            .next_page_with_memory_guard_and_control(
                1,
                maximum_work_steps,
                &mut |active_enumerator_peak| {
                    if memory_admission(active_enumerator_peak) {
                        Ok(())
                    } else {
                        Err(clearra_coverage::cover::ExactMinimumCoverError::MemoryGuardRejected)
                    }
                },
                cancelled,
            )
            .map_err(PortfolioAlternativeError::Enumeration)?;
        let exact_page_live = page.checked_retained_capacity_bytes().ok_or_else(|| {
            portfolio_minimum_cover_error(
                clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
            )
        })?;
        let active_enumerator_live = self
            .enumerator
            .checked_retained_capacity_bytes()
            .ok_or_else(|| {
                portfolio_minimum_cover_error(
                    clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
                )
            })?;
        let projected_advance_live = checked_projected_advance_capacity_bytes(
            &page,
            &self.set.set_identity_sha256,
            &self.set.candidate_map_sha256,
        )
        .ok_or_else(|| {
            portfolio_minimum_cover_error(
                clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
            )
        })?;
        guard_portfolio_memory(
            active_enumerator_live
                .checked_add(exact_page_live)
                .and_then(|bytes| bytes.checked_add(projected_advance_live))
                .ok_or_else(|| {
                    portfolio_minimum_cover_error(
                        clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
                    )
                })?,
            memory_admission,
        )?;
        let portfolio = match page.portfolios() {
            [] => None,
            [portfolio] => Some(PortfolioAlternativePage {
                contract_id: PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT,
                set_identity_sha256: self.set.set_identity_sha256.clone(),
                candidate_map_sha256: self.set.candidate_map_sha256.clone(),
                alternative_index_decimal: page.known_alternative_count_decimal().to_owned(),
                portfolio: portfolio_from_rows(portfolio.row_indices())?,
                optimal_cardinality: page.optimal_cardinality(),
                known_alternative_count_decimal: page.known_alternative_count_decimal().to_owned(),
                total_alternative_count_decimal: page
                    .total_alternative_count_decimal()
                    .map(ToOwned::to_owned),
                enumeration_complete: page.enumeration_complete(),
            }),
            _ => return Err(PortfolioAlternativeError::PageCardinalityMismatch),
        };
        let checkpoint = checkpoint_from_exact_page(
            &page,
            &self.set.set_identity_sha256,
            &self.set.candidate_map_sha256,
        );
        let advance = PortfolioAlternativeAdvance {
            page: portfolio,
            stop: page.stop().into(),
            work_steps: page.work_steps(),
            checkpoint,
        };
        let advance_live = advance.checked_retained_capacity_bytes().ok_or_else(|| {
            portfolio_minimum_cover_error(
                clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
            )
        })?;
        guard_portfolio_memory(
            active_enumerator_live
                .checked_add(exact_page_live)
                .and_then(|bytes| bytes.checked_add(advance_live))
                .ok_or_else(|| {
                    portfolio_minimum_cover_error(
                        clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
                    )
                })?,
            memory_admission,
        )?;
        drop(page);
        guard_portfolio_memory(
            active_enumerator_live
                .checked_add(advance_live)
                .ok_or_else(|| {
                    portfolio_minimum_cover_error(
                        clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
                    )
                })?,
            memory_admission,
        )?;
        Ok(advance)
    }

    pub fn checkpoint(&self) -> PortfolioAlternativeCheckpoint {
        checkpoint_from_enumerator(
            &self.enumerator,
            &self.set.set_identity_sha256,
            &self.set.candidate_map_sha256,
        )
    }
}

/// Immutable producer owner transferred across App/Host/WASM/Desktop
/// boundaries. The enum is deliberately product-neutral: future public page
/// families can add variants without changing the surrounding lifecycle
/// carrier. A variant is only added when that product's surface is activated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductPageSourceOwner {
    CoveragePortfolio(Arc<CoveragePortfolioAlternativeSet>),
    ParityReport(Arc<crate::parity_page_store::ParityReportPageSource>),
}

impl ProductPageSourceOwner {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::CoveragePortfolio(set) => set.checked_retained_capacity_bytes(),
            Self::ParityReport(source) => source.checked_retained_capacity_bytes(),
        }
    }
}

/// Mutable runtime page handle built from a transferred immutable source.
/// Only a fixed current/adjacent window is retained. An evicted materialized
/// page is rebuilt from the immutable enumerator origin when requested by its
/// exact decimal alternative identity.
#[derive(Debug)]
// Both variants are runtime-owned page stores. Keeping them inline preserves
// the existing allocation-failure and whole-live accounting boundary; boxing
// only the coverage variant would add an unguarded heap owner to construction.
#[allow(clippy::large_enum_variant)]
pub enum ProductPageStore {
    CoveragePortfolio(CoveragePortfolioPageStore),
    ParityReport(crate::parity_page_store::ParityReportPageStore),
}

impl ProductPageStore {
    pub fn from_source(source: ProductPageSourceOwner) -> Result<Self, PortfolioAlternativeError> {
        match source {
            ProductPageSourceOwner::CoveragePortfolio(set) => Ok(Self::CoveragePortfolio(
                CoveragePortfolioPageStore::new(set)?,
            )),
            ProductPageSourceOwner::ParityReport(source) => Ok(Self::ParityReport(
                crate::parity_page_store::ParityReportPageStore::new(source)?,
            )),
        }
    }

    pub const fn coverage_portfolio(&self) -> Option<&CoveragePortfolioPageStore> {
        match self {
            Self::CoveragePortfolio(store) => Some(store),
            Self::ParityReport(_) => None,
        }
    }

    pub fn coverage_portfolio_mut(&mut self) -> Option<&mut CoveragePortfolioPageStore> {
        match self {
            Self::CoveragePortfolio(store) => Some(store),
            Self::ParityReport(_) => None,
        }
    }

    pub const fn parity_report(&self) -> Option<&crate::parity_page_store::ParityReportPageStore> {
        match self {
            Self::ParityReport(store) => Some(store),
            Self::CoveragePortfolio(_) => None,
        }
    }

    pub fn parity_report_mut(
        &mut self,
    ) -> Option<&mut crate::parity_page_store::ParityReportPageStore> {
        match self {
            Self::ParityReport(store) => Some(store),
            Self::CoveragePortfolio(_) => None,
        }
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::CoveragePortfolio(store) => store.checked_retained_capacity_bytes(),
            Self::ParityReport(store) => store.checked_retained_capacity_bytes(),
        }
    }
}

#[derive(Debug)]
pub struct CoveragePortfolioPageStore {
    store: CoveragePortfolioAlternativeStore,
    replay_origin: CoveragePortfolioAlternativeStore,
    pending_replay: Option<CoveragePortfolioReplaySession>,
    loaded_pages: Vec<PortfolioAlternativePage>,
    focused_slot: usize,
}

#[derive(Debug)]
struct CoveragePortfolioReplaySession {
    target_alternative_index_decimal: String,
    replay: CoveragePortfolioAlternativeStore,
}

impl CoveragePortfolioPageStore {
    pub fn new(
        set: Arc<CoveragePortfolioAlternativeSet>,
    ) -> Result<Self, PortfolioAlternativeError> {
        let canonical_page = set.canonical_page().clone();
        let store = CoveragePortfolioAlternativeSet::open_owned_store(set)?;
        let replay_origin = store.clone();
        let mut loaded_pages = Vec::new();
        loaded_pages
            .try_reserve_exact(PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT)
            .map_err(|_| PortfolioAlternativeError::AllocationFailed)?;
        loaded_pages.push(canonical_page);
        Ok(Self {
            store,
            replay_origin,
            pending_replay: None,
            loaded_pages,
            focused_slot: 0,
        })
    }

    pub fn source(&self) -> &Arc<CoveragePortfolioAlternativeSet> {
        self.store.alternative_set()
    }

    pub fn loaded_page_count(&self) -> usize {
        self.loaded_pages.len()
    }

    /// Returns a loaded page by its one-based outer page number.
    pub fn page(&self, page_number: usize) -> Option<&PortfolioAlternativePage> {
        (page_number != 0)
            .then(|| page_number.to_string())
            .and_then(|identity| self.page_by_alternative_index(&identity))
    }

    pub fn page_by_alternative_index(
        &self,
        alternative_index_decimal: &str,
    ) -> Option<&PortfolioAlternativePage> {
        self.loaded_pages
            .iter()
            .find(|page| page.alternative_index_decimal() == alternative_index_decimal)
    }

    pub fn retained_page(&self, retained_slot: usize) -> Option<&PortfolioAlternativePage> {
        self.loaded_pages.get(retained_slot)
    }

    pub fn known_alternative_count_decimal(&self) -> String {
        self.store.enumerator.known_alternative_count_decimal()
    }

    pub fn enumeration_complete(&self) -> bool {
        self.store.enumerator.enumeration_complete()
    }

    pub fn replay_cursor_alternative_index_decimal(&self) -> Option<String> {
        self.pending_replay
            .as_ref()
            .map(|session| session.replay.enumerator.known_alternative_count_decimal())
    }

    /// Loads an already materialized alternative by exact decimal identity.
    /// Cache misses replay the immutable source from the canonical origin;
    /// they never change the high-water enumerator owned by `next_page`.
    pub fn load_page_by_alternative_index(
        &mut self,
        alternative_index_decimal: &str,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<usize, PortfolioAlternativeError> {
        self.load_page_by_alternative_index_with_memory_guard(
            alternative_index_decimal,
            &mut |_| true,
            cancelled,
        )
    }

    pub fn load_page_by_alternative_index_slice(
        &mut self,
        alternative_index_decimal: &str,
        maximum_work_steps: u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PortfolioPageLoadAdvance, PortfolioAlternativeError> {
        self.load_page_by_alternative_index_slice_with_memory_guard(
            alternative_index_decimal,
            maximum_work_steps,
            &mut |_| true,
            cancelled,
        )
    }

    /// Advances an evicted-page replay by one bounded slice. The immutable
    /// canonical origin and high-water enumerator are never mutated. Replay
    /// progress is staged and committed only for a retryable work-budget stop;
    /// cancellation and errors preserve the prior cursor and loaded window.
    pub fn load_page_by_alternative_index_slice_with_memory_guard(
        &mut self,
        alternative_index_decimal: &str,
        maximum_work_steps: u64,
        memory_admission: &mut impl FnMut(u128) -> bool,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PortfolioPageLoadAdvance, PortfolioAlternativeError> {
        let maximum_work_steps = maximum_work_steps.max(1);
        let original_live = self
            .checked_retained_capacity_bytes()
            .ok_or_else(portfolio_projection_overflow)?;
        guard_portfolio_memory(original_live, memory_admission)?;
        if !is_canonical_nonzero_decimal(alternative_index_decimal) {
            return Err(PortfolioAlternativeError::InvalidAlternativeIndex);
        }
        if cancelled() {
            return Ok(PortfolioPageLoadAdvance {
                retained_slot: None,
                state: PortfolioPageLoadState::Cancelled,
                work_steps: 0,
            });
        }
        if let Some(retained_slot) = self.retained_page_slot(alternative_index_decimal) {
            self.focused_slot = retained_slot;
            self.pending_replay = None;
            return Ok(PortfolioPageLoadAdvance {
                retained_slot: Some(retained_slot),
                state: PortfolioPageLoadState::Page,
                work_steps: 0,
            });
        }
        if compare_canonical_decimals(
            alternative_index_decimal,
            &self.store.enumerator.known_alternative_count_decimal(),
        )
        .is_gt()
        {
            return Err(PortfolioAlternativeError::PageNotLoaded);
        }
        if alternative_index_decimal == "1" {
            let retained_page = clone_page_with_memory_guard(
                self.source().canonical_page(),
                original_live,
                memory_admission,
            )?;
            let retained_slot = self.remember_page(retained_page)?;
            self.focused_slot = retained_slot;
            self.pending_replay = None;
            return Ok(PortfolioPageLoadAdvance {
                retained_slot: Some(retained_slot),
                state: PortfolioPageLoadState::Page,
                work_steps: 0,
            });
        }

        let target_alternative_index_decimal = try_clone_string_with_memory_guard(
            alternative_index_decimal,
            original_live,
            memory_admission,
        )?;
        let request_live = original_live
            .checked_add(target_alternative_index_decimal.capacity() as u128)
            .ok_or_else(portfolio_projection_overflow)?;
        let replay_source = self
            .pending_replay
            .as_ref()
            .filter(|session| session.target_alternative_index_decimal == alternative_index_decimal)
            .map_or(&self.replay_origin, |session| &session.replay);
        let mut staged_replay =
            replay_source.try_clone_with_memory_guard(request_live, memory_admission)?;
        let mut work_steps = 0_u64;
        let mut page_transitions = 0_usize;

        loop {
            if cancelled() {
                return Ok(PortfolioPageLoadAdvance {
                    retained_slot: None,
                    state: PortfolioPageLoadState::Cancelled,
                    work_steps,
                });
            }
            let remaining_work_steps = maximum_work_steps.saturating_sub(work_steps).max(1);
            let mut overflowed = false;
            let advance_result = staged_replay.next_page_in_place_with_memory_guard(
                remaining_work_steps,
                &mut |active_replay_peak| {
                    let Some(whole_live) = request_live.checked_add(active_replay_peak) else {
                        overflowed = true;
                        return false;
                    };
                    memory_admission(whole_live)
                },
                cancelled,
            );
            if overflowed {
                return Err(portfolio_projection_overflow());
            }
            let mut advance = advance_result?;
            work_steps = work_steps
                .checked_add(advance.work_steps())
                .ok_or_else(portfolio_projection_overflow)?;
            let stop = advance.stop();

            if let Some(page) = advance.page.take() {
                page_transitions = page_transitions.saturating_add(1);
                match compare_canonical_decimals(
                    page.alternative_index_decimal(),
                    alternative_index_decimal,
                ) {
                    core::cmp::Ordering::Less => {}
                    core::cmp::Ordering::Equal => {
                        let replay_live = staged_replay
                            .checked_enumerator_retained_capacity_bytes()
                            .ok_or_else(portfolio_projection_overflow)?;
                        let transient_live = request_live
                            .checked_add(replay_live)
                            .and_then(|bytes| {
                                bytes.checked_add(
                                    advance.checked_retained_capacity_bytes().and_then(
                                        |advance_bytes| {
                                            advance_bytes.checked_add(
                                                checked_page_nested_retained_capacity_bytes(&page)?,
                                            )
                                        },
                                    )?,
                                )
                            })
                            .ok_or_else(portfolio_projection_overflow)?;
                        let retained_page =
                            clone_page_with_memory_guard(&page, transient_live, memory_admission)?;
                        let retained_slot = self.remember_page(retained_page)?;
                        self.focused_slot = retained_slot;
                        self.pending_replay = None;
                        return Ok(PortfolioPageLoadAdvance {
                            retained_slot: Some(retained_slot),
                            state: PortfolioPageLoadState::Page,
                            work_steps,
                        });
                    }
                    core::cmp::Ordering::Greater => {
                        return Err(PortfolioAlternativeError::PageNotLoaded);
                    }
                }
            }

            match stop {
                PortfolioEnumerationStop::Cancelled => {
                    return Ok(PortfolioPageLoadAdvance {
                        retained_slot: None,
                        state: PortfolioPageLoadState::Cancelled,
                        work_steps,
                    });
                }
                PortfolioEnumerationStop::Sealed => {
                    return Err(PortfolioAlternativeError::PageNotLoaded);
                }
                PortfolioEnumerationStop::WorkBudgetExhausted
                | PortfolioEnumerationStop::PageFull => {}
            }

            if work_steps >= maximum_work_steps
                || page_transitions >= PORTFOLIO_REPLAY_PAGE_TRANSITION_LIMIT
                || stop == PortfolioEnumerationStop::WorkBudgetExhausted
            {
                let transient_live = request_live
                    .checked_add(
                        staged_replay
                            .checked_enumerator_retained_capacity_bytes()
                            .ok_or_else(portfolio_projection_overflow)?,
                    )
                    .and_then(|bytes| bytes.checked_add(advance.checked_retained_capacity_bytes()?))
                    .ok_or_else(portfolio_projection_overflow)?;
                guard_portfolio_memory(transient_live, memory_admission)?;
                self.pending_replay = Some(CoveragePortfolioReplaySession {
                    target_alternative_index_decimal,
                    replay: staged_replay,
                });
                return Ok(PortfolioPageLoadAdvance {
                    retained_slot: None,
                    state: PortfolioPageLoadState::WorkBudgetExhausted,
                    work_steps,
                });
            }
        }
    }

    /// Replays an evicted page while retaining the high-water store unchanged
    /// and charging the temporary replay enumerator, rebuilt page window, and
    /// every inner exact decision against one whole-live admission callback.
    pub fn load_page_by_alternative_index_with_memory_guard(
        &mut self,
        alternative_index_decimal: &str,
        memory_admission: &mut impl FnMut(u128) -> bool,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<usize, PortfolioAlternativeError> {
        guard_portfolio_memory(
            self.checked_retained_capacity_bytes()
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
        if !is_canonical_nonzero_decimal(alternative_index_decimal) {
            return Err(PortfolioAlternativeError::InvalidAlternativeIndex);
        }
        if self
            .loaded_pages
            .get(self.focused_slot)
            .is_some_and(|page| page.alternative_index_decimal() == alternative_index_decimal)
        {
            return Ok(self.focused_slot);
        }
        let store_live = self
            .checked_retained_capacity_bytes()
            .ok_or_else(portfolio_projection_overflow)?;
        let known_alternative_count_decimal = self
            .store
            .enumerator
            .known_alternative_count_decimal_with_memory_guard(store_live, &mut |whole_live| {
                if memory_admission(whole_live) {
                    Ok(())
                } else {
                    Err(clearra_coverage::cover::ExactMinimumCoverError::MemoryGuardRejected)
                }
            })
            .map_err(PortfolioAlternativeError::Enumeration)?;
        let persistent_live = store_live
            .checked_add(known_alternative_count_decimal.capacity() as u128)
            .ok_or_else(portfolio_projection_overflow)?;
        guard_portfolio_memory(persistent_live, memory_admission)?;
        if compare_canonical_decimals(alternative_index_decimal, &known_alternative_count_decimal)
            .is_gt()
        {
            return Err(PortfolioAlternativeError::PageNotLoaded);
        }
        let replay_projected_live = self
            .replay_origin
            .checked_enumerator_retained_capacity_bytes()
            .ok_or_else(portfolio_projection_overflow)?;
        guard_portfolio_memory(
            persistent_live
                .checked_add(replay_projected_live)
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
        let mut replay = self
            .replay_origin
            .try_clone_with_memory_guard(persistent_live, memory_admission)?;
        let replay_live = replay
            .checked_enumerator_retained_capacity_bytes()
            .ok_or_else(portfolio_projection_overflow)?;
        guard_portfolio_memory(
            persistent_live
                .checked_add(replay_live)
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
        let mut rebuilt_pages = Vec::new();
        let rebuilt_requested_bytes = (PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT as u128)
            .checked_mul(core::mem::size_of::<PortfolioAlternativePage>() as u128)
            .ok_or_else(portfolio_projection_overflow)?;
        guard_portfolio_memory(
            persistent_live
                .checked_add(replay_live)
                .and_then(|bytes| bytes.checked_add(rebuilt_requested_bytes))
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
        rebuilt_pages
            .try_reserve_exact(PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT)
            .map_err(|_| PortfolioAlternativeError::AllocationFailed)?;
        guard_portfolio_memory(
            persistent_live
                .checked_add(replay_live)
                .and_then(|bytes| {
                    bytes.checked_add(checked_page_vec_retained_capacity_bytes(&rebuilt_pages)?)
                })
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;

        if alternative_index_decimal == "1" {
            let replay_base =
                checked_cache_replay_live(persistent_live, replay_live, &rebuilt_pages, None)
                    .ok_or_else(portfolio_projection_overflow)?;
            rebuilt_pages.push(clone_page_with_memory_guard(
                self.source().canonical_page(),
                replay_base,
                memory_admission,
            )?);
            if known_alternative_count_decimal != "1" {
                let replay_base = checked_cache_replay_live(
                    persistent_live,
                    replay
                        .checked_enumerator_retained_capacity_bytes()
                        .ok_or_else(portfolio_projection_overflow)?,
                    &rebuilt_pages,
                    None,
                )
                .ok_or_else(portfolio_projection_overflow)?;
                rebuilt_pages.push(next_replayed_page_with_memory_guard(
                    &mut replay,
                    replay_base,
                    memory_admission,
                    cancelled,
                )?);
            }
        } else {
            let replay_base =
                checked_cache_replay_live(persistent_live, replay_live, &rebuilt_pages, None)
                    .ok_or_else(portfolio_projection_overflow)?;
            let mut previous_page = clone_page_with_memory_guard(
                self.source().canonical_page(),
                replay_base,
                memory_admission,
            )?;
            loop {
                let replay_base = checked_cache_replay_live(
                    persistent_live,
                    replay
                        .checked_enumerator_retained_capacity_bytes()
                        .ok_or_else(portfolio_projection_overflow)?,
                    &rebuilt_pages,
                    Some(&previous_page),
                )
                .ok_or_else(portfolio_projection_overflow)?;
                let page = next_replayed_page_with_memory_guard(
                    &mut replay,
                    replay_base,
                    memory_admission,
                    cancelled,
                )?;
                match compare_canonical_decimals(
                    page.alternative_index_decimal(),
                    alternative_index_decimal,
                ) {
                    core::cmp::Ordering::Less => previous_page = page,
                    core::cmp::Ordering::Equal => {
                        rebuilt_pages.push(previous_page);
                        rebuilt_pages.push(page);
                        if compare_canonical_decimals(
                            alternative_index_decimal,
                            &known_alternative_count_decimal,
                        )
                        .is_lt()
                        {
                            let replay_base = checked_cache_replay_live(
                                persistent_live,
                                replay
                                    .checked_enumerator_retained_capacity_bytes()
                                    .ok_or_else(portfolio_projection_overflow)?,
                                &rebuilt_pages,
                                None,
                            )
                            .ok_or_else(portfolio_projection_overflow)?;
                            rebuilt_pages.push(next_replayed_page_with_memory_guard(
                                &mut replay,
                                replay_base,
                                memory_admission,
                                cancelled,
                            )?);
                        }
                        break;
                    }
                    core::cmp::Ordering::Greater => {
                        return Err(PortfolioAlternativeError::PageNotLoaded);
                    }
                }
            }
        }

        let focused_slot = rebuilt_pages
            .iter()
            .position(|page| page.alternative_index_decimal() == alternative_index_decimal)
            .ok_or(PortfolioAlternativeError::PageNotLoaded)?;
        guard_portfolio_memory(
            checked_cache_replay_live(
                persistent_live,
                replay
                    .checked_enumerator_retained_capacity_bytes()
                    .ok_or_else(portfolio_projection_overflow)?,
                &rebuilt_pages,
                None,
            )
            .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
        self.loaded_pages = rebuilt_pages;
        self.focused_slot = focused_slot;
        Ok(focused_slot)
    }

    pub fn next_page(
        &mut self,
        maximum_work_steps: u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PortfolioAlternativeAdvance, PortfolioAlternativeError> {
        self.next_page_with_memory_guard(maximum_work_steps, &mut |_| true, cancelled)
    }

    /// Advances one outer page under a whole-live admission callback. The
    /// persistent baseline excludes only the active high-water enumerator,
    /// because that enumerator reports its own full live and future peak from
    /// the exact layer. Source, replay origin, retained pages, and temporary
    /// cache-replay owners remain charged here.
    pub fn next_page_with_memory_guard(
        &mut self,
        maximum_work_steps: u64,
        memory_admission: &mut impl FnMut(u128) -> bool,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PortfolioAlternativeAdvance, PortfolioAlternativeError> {
        let original_live = self
            .checked_retained_capacity_bytes()
            .ok_or_else(portfolio_projection_overflow)?;
        guard_portfolio_memory(original_live, memory_admission)?;
        let mut staged = self.try_clone_with_memory_guard(original_live, memory_admission)?;
        let mut overflowed = false;
        let advance_result = staged.next_page_in_place_with_memory_guard(
            maximum_work_steps,
            &mut |staged_peak| {
                let Some(whole_live) = original_live.checked_add(staged_peak) else {
                    overflowed = true;
                    return false;
                };
                memory_admission(whole_live)
            },
            cancelled,
        );
        if overflowed {
            return Err(portfolio_projection_overflow());
        }
        let advance = advance_result?;
        guard_portfolio_memory(
            original_live
                .checked_add(
                    staged
                        .checked_retained_capacity_bytes()
                        .ok_or_else(portfolio_projection_overflow)?,
                )
                .and_then(|bytes| bytes.checked_add(advance.checked_retained_capacity_bytes()?))
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
        *self = staged;
        Ok(advance)
    }

    fn try_clone_with_memory_guard(
        &self,
        external_live_bytes: u128,
        memory_admission: &mut impl FnMut(u128) -> bool,
    ) -> Result<Self, PortfolioAlternativeError> {
        let store = self
            .store
            .try_clone_with_memory_guard(external_live_bytes, memory_admission)?;
        let store_live = store
            .checked_enumerator_retained_capacity_bytes()
            .ok_or_else(portfolio_projection_overflow)?;
        let replay_external = external_live_bytes
            .checked_add(store_live)
            .ok_or_else(portfolio_projection_overflow)?;
        let replay_origin = self
            .replay_origin
            .try_clone_with_memory_guard(replay_external, memory_admission)?;
        let replay_live = replay_origin
            .checked_enumerator_retained_capacity_bytes()
            .ok_or_else(portfolio_projection_overflow)?;
        let pages_external = replay_external
            .checked_add(replay_live)
            .ok_or_else(portfolio_projection_overflow)?;
        let loaded_pages = try_clone_page_vec_with_memory_guard(
            &self.loaded_pages,
            pages_external,
            memory_admission,
        )?;
        let loaded_pages_live = checked_page_vec_retained_capacity_bytes(&loaded_pages)
            .ok_or_else(portfolio_projection_overflow)?;
        let pending_external = pages_external
            .checked_add(loaded_pages_live)
            .ok_or_else(portfolio_projection_overflow)?;
        let pending_replay = match &self.pending_replay {
            Some(session) => {
                let target_alternative_index_decimal = try_clone_string_with_memory_guard(
                    &session.target_alternative_index_decimal,
                    pending_external,
                    memory_admission,
                )?;
                let replay_external = pending_external
                    .checked_add(target_alternative_index_decimal.capacity() as u128)
                    .ok_or_else(portfolio_projection_overflow)?;
                let replay = session
                    .replay
                    .try_clone_with_memory_guard(replay_external, memory_admission)?;
                Some(CoveragePortfolioReplaySession {
                    target_alternative_index_decimal,
                    replay,
                })
            }
            None => None,
        };
        let pending_live = match &pending_replay {
            Some(session) => (session.target_alternative_index_decimal.capacity() as u128)
                .checked_add(
                    session
                        .replay
                        .checked_enumerator_retained_capacity_bytes()
                        .ok_or_else(portfolio_projection_overflow)?,
                )
                .ok_or_else(portfolio_projection_overflow)?,
            None => 0,
        };
        guard_portfolio_memory(
            pending_external
                .checked_add(pending_live)
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
        Ok(Self {
            store,
            replay_origin,
            pending_replay,
            loaded_pages,
            focused_slot: self.focused_slot,
        })
    }

    fn next_page_in_place_with_memory_guard(
        &mut self,
        maximum_work_steps: u64,
        memory_admission: &mut impl FnMut(u128) -> bool,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PortfolioAlternativeAdvance, PortfolioAlternativeError> {
        guard_portfolio_memory(
            self.checked_retained_capacity_bytes()
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
        let page_store_live = self
            .checked_retained_capacity_bytes()
            .ok_or_else(portfolio_projection_overflow)?;
        let previous_high_water = self
            .store
            .enumerator
            .known_alternative_count_decimal_with_memory_guard(page_store_live, &mut |whole_live| {
                if memory_admission(whole_live) {
                    Ok(())
                } else {
                    Err(clearra_coverage::cover::ExactMinimumCoverError::MemoryGuardRejected)
                }
            })
            .map_err(PortfolioAlternativeError::Enumeration)?;
        if let Some(slot) = self.retained_page_slot(&previous_high_water) {
            self.focused_slot = slot;
        }
        let persistent_base = self
            .checked_retained_capacity_bytes_without_high_water_enumerator()
            .and_then(|bytes| bytes.checked_add(previous_high_water.capacity() as u128))
            .ok_or_else(portfolio_projection_overflow)?;
        let mut overflowed = false;
        let advance_result = self.store.next_page_in_place_with_memory_guard(
            maximum_work_steps,
            &mut |active_enumerator_peak| {
                let Some(whole_live) = persistent_base.checked_add(active_enumerator_peak) else {
                    overflowed = true;
                    return false;
                };
                memory_admission(whole_live)
            },
            cancelled,
        );
        if overflowed {
            return Err(portfolio_projection_overflow());
        }
        let advance = advance_result?;
        if let Some(page) = advance.page() {
            let current_live = self
                .checked_retained_capacity_bytes()
                .and_then(|bytes| bytes.checked_add(previous_high_water.capacity() as u128))
                .and_then(|bytes| bytes.checked_add(advance.checked_retained_capacity_bytes()?))
                .ok_or_else(portfolio_projection_overflow)?;
            let retained_page = clone_page_with_memory_guard(page, current_live, memory_admission)?;
            let retained_slot = self.remember_page(retained_page)?;
            self.focused_slot = retained_slot.saturating_sub(1);
        }
        guard_portfolio_memory(
            self.checked_retained_capacity_bytes()
                .and_then(|bytes| bytes.checked_add(previous_high_water.capacity() as u128))
                .and_then(|bytes| bytes.checked_add(advance.checked_retained_capacity_bytes()?))
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
        Ok(advance)
    }

    pub fn member_page(
        &self,
        outer_page_number: usize,
        member_page_number: usize,
    ) -> Result<PortfolioMemberPage, PortfolioAlternativeError> {
        let page = self
            .page(outer_page_number)
            .ok_or(PortfolioAlternativeError::PageNotLoaded)?;
        self.store
            .alternative_set()
            .member_page(page, member_page_number)
    }

    pub fn retained_page_slot(&self, alternative_index_decimal: &str) -> Option<usize> {
        self.loaded_pages
            .iter()
            .position(|page| page.alternative_index_decimal() == alternative_index_decimal)
    }

    fn remember_page(
        &mut self,
        page: PortfolioAlternativePage,
    ) -> Result<usize, PortfolioAlternativeError> {
        if let Some(slot) = self.retained_page_slot(page.alternative_index_decimal()) {
            return Ok(slot);
        }
        if self.loaded_pages.len() == PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT {
            self.loaded_pages.remove(0);
        }
        self.loaded_pages.push(page);
        Ok(self.loaded_pages.len() - 1)
    }

    /// Encodes the complete selected portfolio, never merely its visible
    /// 100-member page. The set/page identity checks live in the shared App
    /// artifact authority and fail closed on drift.
    pub fn bounded_solution_set_artifact_payload(
        &self,
        outer_page_number: usize,
        source_result_kind: &str,
        maximum_bytes: u64,
    ) -> Option<clearra_host_contract::SolutionSetArtifactPayload> {
        crate::app_response::solution_set_artifact::materialize_loaded_portfolio_page(
            self,
            outer_page_number,
            source_result_kind,
        )
        .and_then(|source| {
            crate::app_response::solution_set_artifact::encode_bound_payload(source, maximum_bytes)
        })
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self
            .source()
            .checked_retained_capacity_bytes()?
            .checked_add(self.store.checked_enumerator_retained_capacity_bytes()?)?
            // The replay origin shares immutable enumerator input with the
            // high-water store, but retains its own restart cursor.
            .checked_add(
                self.replay_origin
                    .checked_enumerator_retained_capacity_bytes()?,
            )?;
        if let Some(session) = &self.pending_replay {
            bytes = bytes
                .checked_add(session.target_alternative_index_decimal.capacity() as u128)?
                .checked_add(
                    session
                        .replay
                        .checked_enumerator_retained_capacity_bytes()?,
                )?;
        }
        bytes = bytes.checked_add(
            (self.loaded_pages.capacity() as u128)
                .checked_mul(core::mem::size_of::<PortfolioAlternativePage>() as u128)?,
        )?;
        for page in &self.loaded_pages {
            bytes = bytes.checked_add(checked_page_nested_retained_capacity_bytes(page)?)?;
        }
        Some(bytes)
    }

    fn checked_retained_capacity_bytes_without_high_water_enumerator(&self) -> Option<u128> {
        self.checked_retained_capacity_bytes()?
            .checked_sub(self.store.checked_enumerator_retained_capacity_bytes()?)
    }
}

fn next_replayed_page_with_memory_guard(
    replay: &mut CoveragePortfolioAlternativeStore,
    external_live_bytes: u128,
    memory_admission: &mut impl FnMut(u128) -> bool,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<PortfolioAlternativePage, PortfolioAlternativeError> {
    loop {
        let mut overflowed = false;
        let advance_result = replay.next_page_in_place_with_memory_guard(
            u64::MAX,
            &mut |active_replay_peak| {
                let Some(whole_live) = external_live_bytes.checked_add(active_replay_peak) else {
                    overflowed = true;
                    return false;
                };
                memory_admission(whole_live)
            },
            cancelled,
        );
        if overflowed {
            return Err(portfolio_projection_overflow());
        }
        let mut advance = advance_result?;
        if let Some(page) = advance.page.take() {
            return Ok(page);
        }
        match advance.stop() {
            PortfolioEnumerationStop::WorkBudgetExhausted => continue,
            PortfolioEnumerationStop::Cancelled => {
                return Err(PortfolioAlternativeError::PageReplayCancelled);
            }
            PortfolioEnumerationStop::Sealed | PortfolioEnumerationStop::PageFull => {
                return Err(PortfolioAlternativeError::PageNotLoaded);
            }
        }
    }
}

fn checked_page_vec_retained_capacity_bytes(pages: &Vec<PortfolioAlternativePage>) -> Option<u128> {
    let mut bytes = (pages.capacity() as u128)
        .checked_mul(core::mem::size_of::<PortfolioAlternativePage>() as u128)?;
    for page in pages {
        bytes = bytes.checked_add(checked_page_nested_retained_capacity_bytes(page)?)?;
    }
    Some(bytes)
}

fn try_clone_page_vec_with_memory_guard(
    pages: &[PortfolioAlternativePage],
    external_live_bytes: u128,
    memory_admission: &mut impl FnMut(u128) -> bool,
) -> Result<Vec<PortfolioAlternativePage>, PortfolioAlternativeError> {
    let inline_requested = (PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT as u128)
        .checked_mul(core::mem::size_of::<PortfolioAlternativePage>() as u128)
        .ok_or_else(portfolio_projection_overflow)?;
    guard_portfolio_memory(
        external_live_bytes
            .checked_add(inline_requested)
            .ok_or_else(portfolio_projection_overflow)?,
        memory_admission,
    )?;
    let mut cloned = Vec::new();
    cloned
        .try_reserve_exact(PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT)
        .map_err(|_| PortfolioAlternativeError::AllocationFailed)?;
    guard_portfolio_memory(
        external_live_bytes
            .checked_add(
                checked_page_vec_retained_capacity_bytes(&cloned)
                    .ok_or_else(portfolio_projection_overflow)?,
            )
            .ok_or_else(portfolio_projection_overflow)?,
        memory_admission,
    )?;
    for page in pages {
        let page_base = external_live_bytes
            .checked_add(
                checked_page_vec_retained_capacity_bytes(&cloned)
                    .ok_or_else(portfolio_projection_overflow)?,
            )
            .ok_or_else(portfolio_projection_overflow)?;
        cloned.push(clone_page_with_memory_guard(
            page,
            page_base,
            memory_admission,
        )?);
        guard_portfolio_memory(
            external_live_bytes
                .checked_add(
                    checked_page_vec_retained_capacity_bytes(&cloned)
                        .ok_or_else(portfolio_projection_overflow)?,
                )
                .ok_or_else(portfolio_projection_overflow)?,
            memory_admission,
        )?;
    }
    Ok(cloned)
}

fn checked_cache_replay_live(
    persistent_live_bytes: u128,
    active_replay_enumerator_bytes: u128,
    rebuilt_pages: &Vec<PortfolioAlternativePage>,
    previous_page: Option<&PortfolioAlternativePage>,
) -> Option<u128> {
    let mut bytes = persistent_live_bytes
        .checked_add(active_replay_enumerator_bytes)?
        .checked_add(checked_page_vec_retained_capacity_bytes(rebuilt_pages)?)?;
    if let Some(previous_page) = previous_page {
        bytes = bytes.checked_add(checked_page_nested_retained_capacity_bytes(previous_page)?)?;
    }
    Some(bytes)
}

fn clone_page_with_memory_guard(
    page: &PortfolioAlternativePage,
    external_live_bytes: u128,
    memory_admission: &mut impl FnMut(u128) -> bool,
) -> Result<PortfolioAlternativePage, PortfolioAlternativeError> {
    let projected = checked_projected_page_clone_capacity_bytes(page)
        .ok_or_else(portfolio_projection_overflow)?;
    guard_portfolio_memory(
        external_live_bytes
            .checked_add(projected)
            .ok_or_else(portfolio_projection_overflow)?,
        memory_admission,
    )?;
    let mut live = external_live_bytes;
    let set_identity_sha256 =
        try_clone_string_with_memory_guard(&page.set_identity_sha256, live, memory_admission)?;
    live = live
        .checked_add(set_identity_sha256.capacity() as u128)
        .ok_or_else(portfolio_projection_overflow)?;
    let candidate_map_sha256 =
        try_clone_string_with_memory_guard(&page.candidate_map_sha256, live, memory_admission)?;
    live = live
        .checked_add(candidate_map_sha256.capacity() as u128)
        .ok_or_else(portfolio_projection_overflow)?;
    let alternative_index_decimal = try_clone_string_with_memory_guard(
        &page.alternative_index_decimal,
        live,
        memory_admission,
    )?;
    live = live
        .checked_add(alternative_index_decimal.capacity() as u128)
        .ok_or_else(portfolio_projection_overflow)?;
    let known_alternative_count_decimal = try_clone_string_with_memory_guard(
        &page.known_alternative_count_decimal,
        live,
        memory_admission,
    )?;
    live = live
        .checked_add(known_alternative_count_decimal.capacity() as u128)
        .ok_or_else(portfolio_projection_overflow)?;
    let total_alternative_count_decimal = match &page.total_alternative_count_decimal {
        Some(total) => {
            let total = try_clone_string_with_memory_guard(total, live, memory_admission)?;
            live = live
                .checked_add(total.capacity() as u128)
                .ok_or_else(portfolio_projection_overflow)?;
            Some(total)
        }
        None => None,
    };
    let candidate_ids = try_clone_u64_slice_with_memory_guard(
        &page.portfolio.candidate_ids,
        live,
        memory_admission,
    )?;
    let cloned = PortfolioAlternativePage {
        contract_id: page.contract_id,
        set_identity_sha256,
        candidate_map_sha256,
        alternative_index_decimal,
        portfolio: PortfolioAlternative { candidate_ids },
        optimal_cardinality: page.optimal_cardinality,
        known_alternative_count_decimal,
        total_alternative_count_decimal,
        enumeration_complete: page.enumeration_complete,
    };
    guard_portfolio_memory(
        external_live_bytes
            .checked_add(
                checked_page_nested_retained_capacity_bytes(&cloned)
                    .ok_or_else(portfolio_projection_overflow)?,
            )
            .ok_or_else(portfolio_projection_overflow)?,
        memory_admission,
    )?;
    Ok(cloned)
}

fn try_clone_string_with_memory_guard(
    value: &str,
    external_live_bytes: u128,
    memory_admission: &mut impl FnMut(u128) -> bool,
) -> Result<String, PortfolioAlternativeError> {
    guard_portfolio_memory(
        external_live_bytes
            .checked_add(value.len() as u128)
            .ok_or_else(portfolio_projection_overflow)?,
        memory_admission,
    )?;
    let mut cloned = String::new();
    cloned
        .try_reserve_exact(value.len())
        .map_err(|_| PortfolioAlternativeError::AllocationFailed)?;
    cloned.push_str(value);
    guard_portfolio_memory(
        external_live_bytes
            .checked_add(cloned.capacity() as u128)
            .ok_or_else(portfolio_projection_overflow)?,
        memory_admission,
    )?;
    Ok(cloned)
}

fn try_clone_u64_slice_with_memory_guard(
    values: &[u64],
    external_live_bytes: u128,
    memory_admission: &mut impl FnMut(u128) -> bool,
) -> Result<Vec<u64>, PortfolioAlternativeError> {
    let requested = (values.len() as u128)
        .checked_mul(core::mem::size_of::<u64>() as u128)
        .ok_or_else(portfolio_projection_overflow)?;
    guard_portfolio_memory(
        external_live_bytes
            .checked_add(requested)
            .ok_or_else(portfolio_projection_overflow)?,
        memory_admission,
    )?;
    let mut cloned = Vec::new();
    cloned
        .try_reserve_exact(values.len())
        .map_err(|_| PortfolioAlternativeError::AllocationFailed)?;
    cloned.extend_from_slice(values);
    guard_portfolio_memory(
        external_live_bytes
            .checked_add(
                (cloned.capacity() as u128)
                    .checked_mul(core::mem::size_of::<u64>() as u128)
                    .ok_or_else(portfolio_projection_overflow)?,
            )
            .ok_or_else(portfolio_projection_overflow)?,
        memory_admission,
    )?;
    Ok(cloned)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortfolioAlternativeError {
    InvalidIdentity { component: &'static str },
    CandidateMapLengthMismatch,
    CandidateMapNotCanonical,
    PublicCandidateMapInvalid,
    CandidateCountOverflow,
    PatternUniverseMismatch,
    Enumeration(ExactMinimumCoverPortfolioError),
    CanonicalPortfolioMissing,
    CanonicalPortfolioMismatch,
    CheckpointIdentityMismatch,
    PageIdentityMismatch,
    PageCardinalityMismatch,
    InvalidCandidateId,
    InvalidMemberPage,
    InvalidAlternativeIndex,
    PageNotLoaded,
    PageReplayCancelled,
    AllocationFailed,
    InvalidParityPage,
    ParityPageCountOverflow,
}

impl PortfolioAlternativeError {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidIdentity { .. } => "portfolio-identity-invalid",
            Self::CandidateMapLengthMismatch => "portfolio-candidate-map-length-mismatch",
            Self::CandidateMapNotCanonical => "portfolio-candidate-map-not-canonical",
            Self::PublicCandidateMapInvalid => "portfolio-public-candidate-map-invalid",
            Self::CandidateCountOverflow => "portfolio-candidate-count-overflow",
            Self::PatternUniverseMismatch => "portfolio-pattern-universe-mismatch",
            Self::Enumeration(_) => "portfolio-enumeration-failed",
            Self::CanonicalPortfolioMissing => "portfolio-canonical-result-missing",
            Self::CanonicalPortfolioMismatch => "portfolio-canonical-result-mismatch",
            Self::CheckpointIdentityMismatch => "portfolio-checkpoint-identity-mismatch",
            Self::PageIdentityMismatch => "portfolio-page-identity-mismatch",
            Self::PageCardinalityMismatch => "portfolio-page-cardinality-mismatch",
            Self::InvalidCandidateId => "portfolio-candidate-id-invalid",
            Self::InvalidMemberPage => "portfolio-member-page-invalid",
            Self::InvalidAlternativeIndex => "portfolio-alternative-index-invalid",
            Self::PageNotLoaded => "portfolio-page-not-loaded",
            Self::PageReplayCancelled => "portfolio-page-replay-cancelled",
            Self::AllocationFailed => "portfolio-allocation-failed",
            Self::InvalidParityPage => "parity-page-invalid",
            Self::ParityPageCountOverflow => "parity-page-count-overflow",
        }
    }
}

fn portfolio_minimum_cover_error(
    error: clearra_coverage::cover::ExactMinimumCoverError,
) -> PortfolioAlternativeError {
    PortfolioAlternativeError::Enumeration(ExactMinimumCoverPortfolioError::MinimumCover(error))
}

fn portfolio_projection_overflow() -> PortfolioAlternativeError {
    portfolio_minimum_cover_error(
        clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
    )
}

fn guard_portfolio_memory(
    whole_live_and_future_bytes: u128,
    memory_admission: &mut impl FnMut(u128) -> bool,
) -> Result<(), PortfolioAlternativeError> {
    if memory_admission(whole_live_and_future_bytes) {
        Ok(())
    } else {
        Err(portfolio_minimum_cover_error(
            clearra_coverage::cover::ExactMinimumCoverError::MemoryGuardRejected,
        ))
    }
}

fn checked_projected_advance_capacity_bytes(
    page: &ExactMinimumCoverPortfolioPage,
    set_identity_sha256: &str,
    candidate_map_sha256: &str,
) -> Option<u128> {
    let known_len = page.known_alternative_count_decimal().len() as u128;
    let mut bytes = (set_identity_sha256.len() as u128)
        .checked_add(candidate_map_sha256.len() as u128)?
        .checked_add(known_len)?;
    if let Some(restart) = page.restart() {
        if let Some(next_combination) = restart.next_combination() {
            bytes = bytes.checked_add(
                (next_combination.len() as u128)
                    .checked_mul(core::mem::size_of::<usize>() as u128)?,
            )?;
        }
    }
    if let [portfolio] = page.portfolios() {
        bytes = bytes
            .checked_add(set_identity_sha256.len() as u128)?
            .checked_add(candidate_map_sha256.len() as u128)?
            .checked_add(known_len.checked_mul(2)?)?
            .checked_add(
                (portfolio.row_indices().len() as u128)
                    .checked_mul(core::mem::size_of::<u64>() as u128)?,
            )?;
        if let Some(total) = page.total_alternative_count_decimal() {
            bytes = bytes.checked_add(total.len() as u128)?;
        }
    }
    Some(bytes)
}

fn checked_projected_page_clone_capacity_bytes(page: &PortfolioAlternativePage) -> Option<u128> {
    let mut bytes = (page.set_identity_sha256.len() as u128)
        .checked_add(page.candidate_map_sha256.len() as u128)?
        .checked_add(page.alternative_index_decimal.len() as u128)?
        .checked_add(page.known_alternative_count_decimal.len() as u128)?
        .checked_add(
            (page.portfolio.candidate_ids.len() as u128)
                .checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?;
    if let Some(total) = &page.total_alternative_count_decimal {
        bytes = bytes.checked_add(total.len() as u128)?;
    }
    Some(bytes)
}

fn checked_checkpoint_retained_capacity_bytes(
    checkpoint: &PortfolioAlternativeCheckpoint,
) -> Option<u128> {
    let mut bytes = (checkpoint.set_identity_sha256.capacity() as u128)
        .checked_add(checkpoint.candidate_map_sha256.capacity() as u128)?
        .checked_add(checkpoint.known_alternative_count_decimal.capacity() as u128)?;
    if let Some(next_combination) = &checkpoint.next_combination {
        bytes = bytes.checked_add(
            (next_combination.capacity() as u128)
                .checked_mul(core::mem::size_of::<usize>() as u128)?,
        )?;
    }
    Some(bytes)
}

fn checked_page_nested_retained_capacity_bytes(page: &PortfolioAlternativePage) -> Option<u128> {
    let mut bytes = [
        page.set_identity_sha256.capacity(),
        page.candidate_map_sha256.capacity(),
        page.alternative_index_decimal.capacity(),
        page.known_alternative_count_decimal.capacity(),
    ]
    .into_iter()
    .try_fold(0_u128, |total, capacity| {
        total.checked_add(capacity as u128)
    })?;
    if let Some(total) = &page.total_alternative_count_decimal {
        bytes = bytes.checked_add(total.capacity() as u128)?;
    }
    bytes.checked_add(
        (page.portfolio.candidate_ids.capacity() as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)?,
    )
}

fn canonicalize_candidate_keys_and_rows(
    candidate_keys: Vec<String>,
    rows: Vec<PatternBitSet>,
    required: &PatternBitSet,
) -> Result<(Vec<String>, Vec<PatternBitSet>), PortfolioAlternativeError> {
    if candidate_keys.len() != rows.len() {
        return Err(PortfolioAlternativeError::CandidateMapLengthMismatch);
    }
    if candidate_keys
        .iter()
        .any(|key| key.is_empty() || key.chars().any(char::is_control))
    {
        return Err(PortfolioAlternativeError::CandidateMapNotCanonical);
    }
    if rows
        .iter()
        .any(|row| row.pattern_count() != required.pattern_count())
    {
        return Err(PortfolioAlternativeError::PatternUniverseMismatch);
    }
    let mut keyed_rows = candidate_keys.into_iter().zip(rows).collect::<Vec<_>>();
    keyed_rows.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    if keyed_rows.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(PortfolioAlternativeError::CandidateMapNotCanonical);
    }

    let mut candidate_keys = Vec::new();
    let mut rows = Vec::new();
    candidate_keys
        .try_reserve_exact(keyed_rows.len())
        .map_err(|_| PortfolioAlternativeError::AllocationFailed)?;
    rows.try_reserve_exact(keyed_rows.len())
        .map_err(|_| PortfolioAlternativeError::AllocationFailed)?;
    for (key, row) in keyed_rows {
        candidate_keys.push(key);
        rows.push(row);
    }
    Ok((candidate_keys, rows))
}

fn portfolio_from_rows(
    row_indices: &[usize],
) -> Result<PortfolioAlternative, PortfolioAlternativeError> {
    let candidate_ids = row_indices
        .iter()
        .map(|index| {
            u64::try_from(*index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or(PortfolioAlternativeError::CandidateCountOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PortfolioAlternative { candidate_ids })
}

fn keys_for_portfolio<'a>(
    candidates: &'a [PortfolioCandidate],
    portfolio: &PortfolioAlternative,
) -> Result<Vec<&'a str>, PortfolioAlternativeError> {
    portfolio
        .candidate_ids
        .iter()
        .map(|candidate_id| {
            candidates
                .get(candidate_index(*candidate_id)?)
                .map(|candidate| candidate.normalized_key.as_str())
                .ok_or(PortfolioAlternativeError::InvalidCandidateId)
        })
        .collect()
}

fn candidate_index(candidate_id: u64) -> Result<usize, PortfolioAlternativeError> {
    candidate_id
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or(PortfolioAlternativeError::InvalidCandidateId)
}

fn checkpoint_from_enumerator(
    enumerator: &ExactMinimumCoverPortfolioEnumerator,
    set_identity_sha256: &str,
    candidate_map_sha256: &str,
) -> PortfolioAlternativeCheckpoint {
    let restart = enumerator.restart_state();
    PortfolioAlternativeCheckpoint {
        contract_id: PORTFOLIO_SNAPSHOT_CONTRACT,
        set_identity_sha256: set_identity_sha256.to_owned(),
        candidate_map_sha256: candidate_map_sha256.to_owned(),
        optimal_cardinality: enumerator.optimal_cardinality(),
        next_combination: restart
            .as_ref()
            .and_then(|restart| restart.next_combination().map(ToOwned::to_owned)),
        known_alternative_count_decimal: enumerator.known_alternative_count_decimal(),
        enumeration_complete: enumerator.enumeration_complete(),
    }
}

fn checkpoint_from_exact_page(
    page: &ExactMinimumCoverPortfolioPage,
    set_identity_sha256: &str,
    candidate_map_sha256: &str,
) -> PortfolioAlternativeCheckpoint {
    PortfolioAlternativeCheckpoint {
        contract_id: PORTFOLIO_SNAPSHOT_CONTRACT,
        set_identity_sha256: set_identity_sha256.to_owned(),
        candidate_map_sha256: candidate_map_sha256.to_owned(),
        optimal_cardinality: page.optimal_cardinality(),
        next_combination: page
            .restart()
            .and_then(|restart| restart.next_combination().map(ToOwned::to_owned)),
        known_alternative_count_decimal: page.known_alternative_count_decimal().to_owned(),
        enumeration_complete: page.enumeration_complete(),
    }
}

fn candidate_map_digest(candidates: &[PortfolioCandidate]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(CANDIDATE_MAP_DIGEST_DOMAIN);
    hasher.update((candidates.len() as u64).to_be_bytes());
    for candidate in candidates {
        hasher.update(candidate.candidate_id.to_be_bytes());
        update_length_delimited(&mut hasher, candidate.normalized_key.as_bytes());
    }
    hex_sha256(hasher.finalize())
}

fn set_identity_digest(
    identity: &PortfolioAlternativeSetIdentity,
    candidate_map_sha256: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SET_IDENTITY_DIGEST_DOMAIN);
    for value in [
        identity.query_identity(),
        identity.source_identity(),
        identity.profile_identity(),
        identity.universe_identity(),
        identity.build_identity(),
        candidate_map_sha256,
    ] {
        update_length_delimited(&mut hasher, value.as_bytes());
    }
    hex_sha256(hasher.finalize())
}

fn update_length_delimited(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn hex_sha256(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_canonical_nonzero_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value != "0"
        && !value.starts_with('0')
}

fn compare_canonical_decimals(left: &str, right: &str) -> core::cmp::Ordering {
    left.len()
        .cmp(&right.len())
        .then_with(|| left.as_bytes().cmp(right.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> PortfolioAlternativeSetIdentity {
        PortfolioAlternativeSetIdentity::new(
            "query-a",
            "source-a",
            "profile-a",
            "universe-a",
            "build-a",
        )
        .expect("identity")
    }

    fn row(pattern_count: usize, patterns: &[u32]) -> PatternBitSet {
        PatternBitSet::from_pattern_indices(pattern_count, patterns.to_vec()).expect("row")
    }

    fn tied_set() -> CoveragePortfolioAlternativeSet {
        let (keys, required, rows) = tied_input();
        CoveragePortfolioAlternativeSet::new(
            identity(),
            keys,
            required,
            rows,
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
        )
        .expect("tied set")
    }

    fn tied_input() -> (Vec<String>, PatternBitSet, Vec<PatternBitSet>) {
        let keys = ["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let rows = vec![
            row(3, &[0]),
            row(3, &[1]),
            row(3, &[2]),
            row(3, &[0]),
            row(3, &[1]),
            row(3, &[2]),
        ];
        (keys, PatternBitSet::all(3), rows)
    }

    #[test]
    fn canonical_preparation_is_zero_budget_stable_resumable_and_blocking_parity_exact() {
        let (keys, required, rows) = tied_input();
        let mut preparation = CoveragePortfolioAlternativeSetPreparation::new(
            identity(),
            keys.clone(),
            required.clone(),
            rows.clone(),
        )
        .expect("bounded preparation");
        assert!(matches!(
            preparation
                .advance(0, &mut || false)
                .expect("zero-budget advance"),
            CoveragePortfolioAlternativeSetPreparationAdvance::Pending { work_steps: 0 }
        ));

        let mut pending_calls = 0_usize;
        let sliced = loop {
            match preparation
                .advance(1, &mut || false)
                .expect("bounded preparation advance")
            {
                CoveragePortfolioAlternativeSetPreparationAdvance::Pending { work_steps } => {
                    assert!(work_steps <= 1);
                    pending_calls += 1;
                }
                CoveragePortfolioAlternativeSetPreparationAdvance::Completed(set) => break set,
                CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled { .. } => {
                    panic!("uncancelled preparation cancelled")
                }
            }
        };
        assert!(pending_calls > 0, "one-node slices must yield observably");

        let blocking =
            CoveragePortfolioAlternativeSet::new_canonical(identity(), keys, required, rows)
                .expect("blocking shared-session result");
        assert_eq!(sliced, blocking);
        assert_eq!(
            sliced.canonical_page().portfolio().candidate_ids(),
            &[1, 2, 3]
        );
    }

    #[test]
    fn canonical_preparation_cancellation_is_immediate_and_terminal() {
        let (keys, required, rows) = tied_input();
        let mut preparation =
            CoveragePortfolioAlternativeSetPreparation::new(identity(), keys, required, rows)
                .expect("cancelled preparation");
        assert!(matches!(
            preparation
                .advance(1, &mut || true)
                .expect("cancel advance"),
            CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled { work_steps: 0 }
        ));
        assert!(matches!(
            preparation.advance(1, &mut || false),
            Err(PortfolioAlternativeError::CanonicalPortfolioMissing)
        ));
    }

    #[test]
    fn candidate_ids_and_portfolios_are_numeric_lexicographic_and_one_based() {
        let set = tied_set();
        assert_eq!(
            set.candidates()
                .iter()
                .map(PortfolioCandidate::candidate_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6]
        );
        assert_eq!(set.canonical_page().portfolio().candidate_ids(), &[1, 2, 3]);
        assert_eq!(set.canonical_page().total_alternative_count_decimal(), None);

        let mut store = set.open_store().expect("store");
        let advance = store.next_page(u64::MAX, &mut || false).expect("page");
        assert_eq!(
            advance
                .page()
                .expect("second page")
                .portfolio()
                .candidate_ids(),
            &[1, 2, 6]
        );
        assert_eq!(
            advance
                .page()
                .expect("second page")
                .alternative_index_decimal(),
            "2"
        );
    }

    #[test]
    fn guarded_alternative_advance_is_transactional_through_the_final_app_allocation() {
        let set = tied_set();
        let mut baseline = set.open_store().expect("baseline store");
        let mut trace = Vec::new();
        let expected = baseline
            .next_page_with_memory_guard(
                u64::MAX,
                &mut |whole_live| {
                    trace.push(whole_live);
                    true
                },
                &mut || false,
            )
            .expect("guarded baseline");
        assert!(trace.len() > 2);

        let mut denied = set.open_store().expect("denied store");
        let before = denied.checkpoint();
        let deny_at = trace.len();
        let mut calls = 0_usize;
        let error = denied
            .next_page_with_memory_guard(
                u64::MAX,
                &mut |_| {
                    calls += 1;
                    calls != deny_at
                },
                &mut || false,
            )
            .expect_err("final App allocation denial");
        assert_eq!(calls, deny_at);
        assert!(matches!(
            error,
            PortfolioAlternativeError::Enumeration(ExactMinimumCoverPortfolioError::MinimumCover(
                clearra_coverage::cover::ExactMinimumCoverError::MemoryGuardRejected
            ))
        ));
        assert_eq!(denied.checkpoint(), before);
        assert_eq!(
            denied
                .next_page(u64::MAX, &mut || false)
                .expect("retry after denial"),
            expected
        );
    }

    #[test]
    fn guarded_runtime_page_advance_denial_keeps_high_water_and_cache_for_retry() {
        let source = Arc::new(tied_set());
        let mut baseline =
            CoveragePortfolioPageStore::new(Arc::clone(&source)).expect("baseline page store");
        let mut trace = Vec::new();
        let expected = baseline
            .next_page_with_memory_guard(
                u64::MAX,
                &mut |whole_live| {
                    trace.push(whole_live);
                    true
                },
                &mut || false,
            )
            .expect("guarded page baseline");
        assert!(trace.len() > 2);

        assert!(trace.len() > 3);
        // The penultimate callback is the allocator-observed retained-page
        // clone; the final callback is the complete staged page-store peak.
        // Rejection at either late boundary must leave the original owner
        // untouched and make a retry byte-for-byte equivalent to baseline.
        for deny_at in [trace.len() - 1, trace.len()] {
            let mut denied =
                CoveragePortfolioPageStore::new(Arc::clone(&source)).expect("denied page store");
            let checkpoint_before = denied.store.checkpoint();
            let pages_before = denied.loaded_pages.clone();
            let focused_before = denied.focused_slot;
            let mut calls = 0_usize;
            let error = denied
                .next_page_with_memory_guard(
                    u64::MAX,
                    &mut |_| {
                        calls += 1;
                        calls != deny_at
                    },
                    &mut || false,
                )
                .expect_err("late page-store denial");
            assert_eq!(calls, deny_at);
            assert!(matches!(
                error,
                PortfolioAlternativeError::Enumeration(
                    ExactMinimumCoverPortfolioError::MinimumCover(
                        clearra_coverage::cover::ExactMinimumCoverError::MemoryGuardRejected
                    )
                )
            ));
            assert_eq!(denied.store.checkpoint(), checkpoint_before);
            assert_eq!(denied.loaded_pages, pages_before);
            assert_eq!(denied.focused_slot, focused_before);
            assert_eq!(
                denied
                    .next_page(u64::MAX, &mut || false)
                    .expect("page-store retry"),
                expected
            );
            assert_eq!(denied.loaded_pages, baseline.loaded_pages);
            assert_eq!(denied.focused_slot, baseline.focused_slot);
        }
    }

    #[test]
    fn source_row_order_cannot_change_candidate_ids_or_portfolio_order() {
        let canonical = tied_set();
        let reordered = CoveragePortfolioAlternativeSet::new(
            identity(),
            vec![
                "f".to_owned(),
                "a".to_owned(),
                "e".to_owned(),
                "c".to_owned(),
                "d".to_owned(),
                "b".to_owned(),
            ],
            PatternBitSet::all(3),
            vec![
                row(3, &[2]),
                row(3, &[0]),
                row(3, &[1]),
                row(3, &[2]),
                row(3, &[0]),
                row(3, &[1]),
            ],
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
        )
        .expect("reordered source set");

        assert_eq!(reordered, canonical);
        let mut canonical_store = canonical.open_store().expect("canonical store");
        let mut reordered_store = reordered.open_store().expect("reordered store");
        for _ in 0..4 {
            let canonical_page = canonical_store
                .next_page(u64::MAX, &mut || false)
                .expect("canonical page");
            let reordered_page = reordered_store
                .next_page(u64::MAX, &mut || false)
                .expect("reordered page");
            assert_eq!(canonical_page, reordered_page);
        }
    }

    #[test]
    fn retained_capacity_includes_candidate_dictionary_and_coverage_rows() {
        let set = tied_set();
        let minimum_candidate_payload = (set.candidates().len() as u128)
            * core::mem::size_of::<PortfolioCandidate>() as u128
            + set
                .candidates()
                .iter()
                .map(|candidate| candidate.normalized_key.capacity() as u128)
                .sum::<u128>();
        let minimum_row_payload = (set.rows.len() as u128)
            * core::mem::size_of::<PatternBitSet>() as u128
            + set
                .rows
                .iter()
                .map(|row| row.checked_storage_retained_bytes().unwrap())
                .sum::<u128>();

        assert!(
            set.checked_retained_capacity_bytes().unwrap()
                >= minimum_candidate_payload + minimum_row_payload
        );
    }

    #[test]
    fn optional_public_candidate_ids_preserve_default_identity_and_project_members_only() {
        let default_set = tied_set();
        let default_candidate_map = default_set.candidate_map_sha256().to_owned();
        let default_set_identity = default_set.set_identity_sha256().to_owned();
        let default_members = default_set
            .member_page(default_set.canonical_page(), 1)
            .expect("default member page");
        assert_eq!(
            default_members
                .members()
                .iter()
                .map(PortfolioMember::candidate_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        let mapped = tied_set()
            .with_public_candidate_ids(vec![101, 205, 309, 401, 505, 609])
            .expect("public candidate map");
        assert_eq!(mapped.candidate_map_sha256(), default_candidate_map);
        assert_eq!(mapped.set_identity_sha256(), default_set_identity);
        assert_eq!(mapped.public_candidate_id(1), Some(101));
        assert_eq!(mapped.public_candidate_id(6), Some(609));
        assert_eq!(
            mapped
                .member_page(mapped.canonical_page(), 1)
                .expect("mapped member page")
                .members()
                .iter()
                .map(PortfolioMember::candidate_id)
                .collect::<Vec<_>>(),
            vec![101, 205, 309]
        );
        assert!(
            mapped.checked_retained_capacity_bytes().unwrap()
                > default_set.checked_retained_capacity_bytes().unwrap()
        );
        assert_eq!(
            tied_set().with_public_candidate_ids(vec![1, 1, 2, 3, 4, 5]),
            Err(PortfolioAlternativeError::PublicCandidateMapInvalid)
        );
    }

    #[test]
    fn checkpoint_resume_is_exact_and_identity_bound() {
        let set = tied_set();
        let mut uninterrupted = set.open_store().expect("store");
        let first_advance = uninterrupted
            .next_page(u64::MAX, &mut || false)
            .expect("second portfolio");
        let checkpoint = first_advance.checkpoint().clone();
        let expected = uninterrupted
            .next_page(u64::MAX, &mut || false)
            .expect("third portfolio")
            .page()
            .expect("third page")
            .clone();

        let mut resumed = set.resume_store(&checkpoint).expect("resume");
        let actual = resumed
            .next_page(u64::MAX, &mut || false)
            .expect("resumed third portfolio")
            .page()
            .expect("resumed page")
            .clone();
        assert_eq!(actual, expected);

        let mut tampered = checkpoint;
        let replacement = if tampered.candidate_map_sha256.starts_with('f') {
            "e"
        } else {
            "f"
        };
        tampered
            .candidate_map_sha256
            .replace_range(0..1, replacement);
        assert!(matches!(
            set.resume_store(&tampered),
            Err(PortfolioAlternativeError::CheckpointIdentityMismatch)
        ));

        // Opening an in-memory page store may reuse the trusted continuation,
        // but a persisted checkpoint still has to reprove k* before accepting
        // its frontier. Matching identity digests cannot make a false optimum
        // authoritative.
        let mut false_optimum = first_advance.checkpoint().clone();
        false_optimum.optimal_cardinality = 2;
        assert!(matches!(
            set.resume_store(&false_optimum),
            Err(PortfolioAlternativeError::Enumeration(
                ExactMinimumCoverPortfolioError::InvalidRestart
            ))
        ));
    }

    #[test]
    fn a_work_budget_stop_never_claims_a_page_or_total() {
        let set = tied_set();
        let mut store = set.open_store().expect("store");
        let advance = store.next_page(0, &mut || false).expect("bounded stop");
        assert!(advance.page().is_none());
        assert_eq!(
            advance.stop(),
            PortfolioEnumerationStop::WorkBudgetExhausted
        );
        assert!(!advance.checkpoint().enumeration_complete());
    }

    #[test]
    fn member_pages_are_fixed_at_one_hundred_without_losing_candidate_identity() {
        let pattern_count = 101;
        let keys = (0..pattern_count)
            .map(|index| format!("candidate-{index:03}"))
            .collect::<Vec<_>>();
        let rows = (0..pattern_count)
            .map(|index| row(pattern_count, &[index as u32]))
            .collect::<Vec<_>>();
        let expected = keys.clone();
        let set = CoveragePortfolioAlternativeSet::new(
            identity(),
            keys,
            PatternBitSet::all(pattern_count),
            rows,
            &expected,
        )
        .expect("large canonical portfolio");

        let first = set
            .member_page(set.canonical_page(), 1)
            .expect("first member page");
        let second = set
            .member_page(set.canonical_page(), 2)
            .expect("second member page");
        assert_eq!(first.members().len(), PORTFOLIO_MEMBER_PAGE_SIZE);
        assert_eq!(first.total_member_pages(), 2);
        assert_eq!(first.members()[0].candidate_id(), 1);
        assert_eq!(second.members().len(), 1);
        assert_eq!(second.members()[0].candidate_id(), 101);
        assert_eq!(second.members()[0].normalized_key(), "candidate-100");
    }

    #[test]
    fn runtime_page_store_retains_prefetched_pages_for_backward_member_navigation() {
        let source = Arc::new(tied_set());
        let source_bytes = source
            .checked_retained_capacity_bytes()
            .expect("source accounting");
        let mut store = ProductPageStore::from_source(ProductPageSourceOwner::CoveragePortfolio(
            Arc::clone(&source),
        ))
        .expect("runtime page store");
        let coverage = store.coverage_portfolio_mut().expect("coverage store");

        assert_eq!(coverage.loaded_page_count(), 1);
        assert_eq!(
            coverage
                .member_page(1, 1)
                .expect("canonical members")
                .members()
                .iter()
                .map(PortfolioMember::candidate_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        let prefetched = coverage
            .next_page(u64::MAX, &mut || false)
            .expect("prefetched alternative");
        assert_eq!(
            prefetched
                .page()
                .expect("second outer page")
                .alternative_index_decimal(),
            "2"
        );
        assert_eq!(coverage.loaded_page_count(), 2);
        assert_eq!(
            coverage
                .member_page(1, 1)
                .expect("backward canonical navigation")
                .members()
                .iter()
                .map(PortfolioMember::candidate_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            coverage
                .member_page(2, 1)
                .expect("prefetched members")
                .members()
                .iter()
                .map(PortfolioMember::candidate_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 6]
        );
        assert_eq!(
            coverage.member_page(3, 1),
            Err(PortfolioAlternativeError::PageNotLoaded)
        );
        assert!(
            coverage
                .checked_retained_capacity_bytes()
                .expect("runtime accounting")
                > source_bytes
        );
    }

    #[test]
    fn runtime_page_store_bounds_outer_cache_and_replays_evicted_exact_identity() {
        let mut store = CoveragePortfolioPageStore::new(Arc::new(tied_set())).expect("page store");
        let mut observed = vec!["1".to_owned()];
        loop {
            let advance = store
                .next_page(u64::MAX, &mut || false)
                .expect("advance exact alternatives");
            if let Some(page) = advance.page() {
                observed.push(page.alternative_index_decimal().to_owned());
            }
            assert!(store.loaded_page_count() <= PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT);
            if advance.checkpoint().enumeration_complete() {
                break;
            }
        }
        assert_eq!(
            observed,
            (1..=8).map(|value| value.to_string()).collect::<Vec<_>>()
        );
        assert_eq!(
            store.loaded_page_count(),
            PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT
        );
        let retained_identities = |store: &CoveragePortfolioPageStore| {
            (0..store.loaded_page_count())
                .map(|slot| {
                    store
                        .retained_page(slot)
                        .expect("retained outer page")
                        .alternative_index_decimal()
                        .to_owned()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(retained_identities(&store), ["6", "7", "8"]);
        assert!(store.page_by_alternative_index("1").is_none());

        let canonical_slot = store
            .load_page_by_alternative_index("1", &mut || false)
            .expect("replay canonical page");
        assert_eq!(
            store
                .retained_page(canonical_slot)
                .expect("canonical retained")
                .alternative_index_decimal(),
            "1"
        );
        assert_eq!(retained_identities(&store), ["1", "2"]);
        let replayed_slot = store
            .load_page_by_alternative_index("6", &mut || false)
            .expect("replay evicted sixth page");
        assert_eq!(
            store
                .retained_page(replayed_slot)
                .expect("sixth retained")
                .alternative_index_decimal(),
            "6"
        );
        assert_eq!(retained_identities(&store), ["5", "6", "7"]);
        assert_eq!(
            store.load_page_by_alternative_index("4", &mut || true),
            Err(PortfolioAlternativeError::PageReplayCancelled)
        );
        assert_eq!(retained_identities(&store), ["5", "6", "7"]);
        assert_eq!(
            store.load_page_by_alternative_index("4294967297", &mut || false),
            Err(PortfolioAlternativeError::PageNotLoaded)
        );
    }

    #[test]
    fn evicted_page_replay_is_bounded_resumable_and_cancel_transactional() {
        let mut store = CoveragePortfolioPageStore::new(Arc::new(tied_set())).expect("page store");
        let mut expected_sixth = None;
        loop {
            let advance = store
                .next_page(u64::MAX, &mut || false)
                .expect("materialize exact alternatives");
            if advance
                .page()
                .is_some_and(|page| page.alternative_index_decimal() == "6")
            {
                expected_sixth = advance.page().cloned();
            }
            if advance.checkpoint().enumeration_complete() {
                break;
            }
        }
        let expected_sixth = expected_sixth.expect("sixth alternative");
        let canonical = store
            .load_page_by_alternative_index_slice("1", 1, &mut || false)
            .expect("canonical cache load");
        assert_eq!(canonical.state(), PortfolioPageLoadState::Page);
        assert!(store.page_by_alternative_index("6").is_none());

        let high_water_before = store.store.checkpoint();
        let loaded_before = store.loaded_pages.clone();
        let focused_before = store.focused_slot;
        let mut previous_cursor = "1".to_owned();
        let first = store
            .load_page_by_alternative_index_slice("6", 1, &mut || false)
            .expect("first bounded replay slice");
        assert_eq!(first.state(), PortfolioPageLoadState::WorkBudgetExhausted);
        assert!(first.retained_slot().is_none());
        assert_eq!(store.loaded_pages, loaded_before);
        assert_eq!(store.focused_slot, focused_before);
        let first_cursor = store
            .replay_cursor_alternative_index_decimal()
            .expect("retry cursor");
        assert!(
            first.work_steps() > 0
                || compare_canonical_decimals(&first_cursor, &previous_cursor).is_gt()
        );
        previous_cursor = first_cursor;

        let replay_cursor_before_cancel = store.replay_cursor_alternative_index_decimal();
        let cancelled = store
            .load_page_by_alternative_index_slice("6", 1, &mut || true)
            .expect("cancelled replay slice");
        assert_eq!(cancelled.state(), PortfolioPageLoadState::Cancelled);
        assert!(cancelled.retained_slot().is_none());
        assert_eq!(cancelled.work_steps(), 0);
        assert_eq!(store.loaded_pages, loaded_before);
        assert_eq!(store.focused_slot, focused_before);
        assert_eq!(
            store.replay_cursor_alternative_index_decimal(),
            replay_cursor_before_cancel
        );
        assert_eq!(store.store.checkpoint(), high_water_before);

        let replayed_slot = loop {
            let advance = store
                .load_page_by_alternative_index_slice("6", 1, &mut || false)
                .expect("resume exact replay");
            match advance.state() {
                PortfolioPageLoadState::Page => {
                    break advance.retained_slot().expect("retained sixth page");
                }
                PortfolioPageLoadState::Cancelled => panic!("unexpected replay cancellation"),
                PortfolioPageLoadState::WorkBudgetExhausted => {
                    let cursor = store
                        .replay_cursor_alternative_index_decimal()
                        .expect("monotonic replay cursor");
                    assert!(
                        advance.work_steps() > 0
                            || compare_canonical_decimals(&cursor, &previous_cursor).is_gt()
                    );
                    previous_cursor = cursor;
                }
            }
        };
        assert_eq!(store.retained_page(replayed_slot), Some(&expected_sixth));
        assert_eq!(store.store.checkpoint(), high_water_before);
        assert!(store.pending_replay.is_none());
    }

    #[test]
    fn guarded_cache_miss_replay_denial_preserves_loaded_window_for_retry() {
        let fully_advanced = || {
            let mut store =
                CoveragePortfolioPageStore::new(Arc::new(tied_set())).expect("page store");
            loop {
                let advance = store
                    .next_page(u64::MAX, &mut || false)
                    .expect("advance all alternatives");
                if advance.checkpoint().enumeration_complete() {
                    break;
                }
            }
            store
        };

        let mut baseline = fully_advanced();
        let mut trace = Vec::new();
        let expected_slot = baseline
            .load_page_by_alternative_index_with_memory_guard(
                "1",
                &mut |whole_live| {
                    trace.push(whole_live);
                    true
                },
                &mut || false,
            )
            .expect("guarded replay baseline");
        assert!(trace.len() > 2);

        let mut denied = fully_advanced();
        let pages_before = denied.loaded_pages.clone();
        let focused_before = denied.focused_slot;
        let checkpoint_before = denied.store.checkpoint();
        let deny_at = trace.len();
        let mut calls = 0_usize;
        let error = denied
            .load_page_by_alternative_index_with_memory_guard(
                "1",
                &mut |_| {
                    calls += 1;
                    calls != deny_at
                },
                &mut || false,
            )
            .expect_err("late replay denial");
        assert_eq!(calls, deny_at);
        assert!(matches!(
            error,
            PortfolioAlternativeError::Enumeration(ExactMinimumCoverPortfolioError::MinimumCover(
                clearra_coverage::cover::ExactMinimumCoverError::MemoryGuardRejected
            ))
        ));
        assert_eq!(denied.loaded_pages, pages_before);
        assert_eq!(denied.focused_slot, focused_before);
        assert_eq!(denied.store.checkpoint(), checkpoint_before);

        let actual_slot = denied
            .load_page_by_alternative_index("1", &mut || false)
            .expect("replay retry");
        assert_eq!(actual_slot, expected_slot);
        assert_eq!(denied.loaded_pages, baseline.loaded_pages);
        assert_eq!(denied.focused_slot, baseline.focused_slot);
        assert_eq!(denied.store.checkpoint(), baseline.store.checkpoint());
    }

    #[test]
    fn canonical_decimal_order_does_not_depend_on_machine_integer_width() {
        assert!(compare_canonical_decimals("4294967297", "4294967296").is_gt());
        assert!(compare_canonical_decimals("184467440737095516160", "9007199254740992").is_gt());
        assert!(compare_canonical_decimals("999", "1000").is_lt());
    }

    #[test]
    fn canonical_result_mismatch_fails_closed() {
        let error = CoveragePortfolioAlternativeSet::new(
            identity(),
            vec!["a".to_owned(), "b".to_owned()],
            PatternBitSet::all(1),
            vec![row(1, &[0]), row(1, &[0])],
            &["b".to_owned()],
        )
        .expect_err("wrong canonical result");
        assert_eq!(error, PortfolioAlternativeError::CanonicalPortfolioMismatch);
    }

    #[test]
    fn canonical_constructor_keeps_lex_first_original_dominated_row_identity() {
        // Row 1 is a proper subset of row 2, so the cardinality solver may
        // discard it as dominated. It nevertheless belongs to the
        // lexicographically first original-row optimum [0, 1]. Product
        // identity must therefore come from the portfolio enumerator, not the
        // reduced proof rows.
        let set = CoveragePortfolioAlternativeSet::new_canonical(
            identity(),
            vec![
                "a".to_owned(),
                "b".to_owned(),
                "c".to_owned(),
                "d".to_owned(),
            ],
            PatternBitSet::all(3),
            vec![row(3, &[1, 2]), row(3, &[0]), row(3, &[0, 1]), row(3, &[])],
        )
        .expect("canonical original-row set");

        assert_eq!(set.optimal_cardinality(), 2);
        assert_eq!(set.canonical_page().portfolio().candidate_ids(), &[1, 2]);
        assert_eq!(
            set.canonical_candidate_keys_owned()
                .expect("canonical keys"),
            ["a", "b"]
        );
    }
}
