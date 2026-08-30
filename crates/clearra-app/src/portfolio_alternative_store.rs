// SRP rationale: this module has one behavior-level change reason: enumerating, checkpointing, and paging deterministic coverage-portfolio alternatives.

use std::sync::Arc;

use clearra_coverage::{
    cover::{
        ExactMinimumCoverEnumerationStop, ExactMinimumCoverPortfolioEnumerator,
        ExactMinimumCoverPortfolioError,
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
}

impl CoveragePortfolioAlternativeSet {
    pub fn new(
        identity: PortfolioAlternativeSetIdentity,
        candidate_keys: Vec<String>,
        required: PatternBitSet,
        rows: Vec<PatternBitSet>,
        expected_canonical_keys: &[String],
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
        let mut enumerator = ExactMinimumCoverPortfolioEnumerator::new(&required, &rows)
            .map_err(PortfolioAlternativeError::Enumeration)?;
        let optimal_cardinality = enumerator.optimal_cardinality();
        let canonical = enumerator
            .next_portfolio()
            .map_err(PortfolioAlternativeError::Enumeration)?
            .ok_or(PortfolioAlternativeError::CanonicalPortfolioMissing)?;
        let canonical_portfolio = portfolio_from_rows(canonical.row_indices())?;
        let actual_keys = keys_for_portfolio(&candidates, &canonical_portfolio)?;
        if actual_keys
            .iter()
            .copied()
            .ne(expected_canonical_keys.iter().map(String::as_str))
        {
            return Err(PortfolioAlternativeError::CanonicalPortfolioMismatch);
        }
        let known = enumerator.known_alternative_count_decimal();
        let total = enumerator.enumeration_complete().then(|| known.clone());
        let canonical_page = PortfolioAlternativePage {
            contract_id: PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT,
            set_identity_sha256: set_identity_sha256.clone(),
            candidate_map_sha256: candidate_map_sha256.clone(),
            alternative_index_decimal: "1".to_owned(),
            portfolio: canonical_portfolio,
            optimal_cardinality,
            known_alternative_count_decimal: known,
            total_alternative_count_decimal: total,
            enumeration_complete: enumerator.enumeration_complete(),
        };
        Ok(Self {
            contract_id: PORTFOLIO_ALTERNATIVE_SET_CONTRACT,
            identity,
            set_identity_sha256,
            candidate_map_sha256,
            candidates: candidates.into(),
            public_candidate_ids: None,
            required,
            rows: rows.into(),
            optimal_cardinality,
            canonical_page,
        })
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
        if public_candidate_ids.len() != self.candidates.len()
            || public_candidate_ids
                .iter()
                .any(|candidate_id| *candidate_id == 0)
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
        let mut enumerator = ExactMinimumCoverPortfolioEnumerator::new(&set.required, &set.rows)
            .map_err(PortfolioAlternativeError::Enumeration)?;
        let canonical = enumerator
            .next_portfolio()
            .map_err(PortfolioAlternativeError::Enumeration)?
            .ok_or(PortfolioAlternativeError::CanonicalPortfolioMissing)?;
        if portfolio_from_rows(canonical.row_indices())? != set.canonical_page.portfolio {
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
        let enumerator = ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
            &set.required,
            &set.rows,
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
        let page = self
            .enumerator
            .next_page_with_control(1, maximum_work_steps, cancelled)
            .map_err(PortfolioAlternativeError::Enumeration)?;
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
        let checkpoint = checkpoint_from_enumerator(
            &self.enumerator,
            &self.set.set_identity_sha256,
            &self.set.candidate_map_sha256,
        );
        Ok(PortfolioAlternativeAdvance {
            page: portfolio,
            stop: page.stop().into(),
            work_steps: page.work_steps(),
            checkpoint,
        })
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
    loaded_pages: Vec<PortfolioAlternativePage>,
    focused_slot: usize,
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

    /// Loads an already materialized alternative by exact decimal identity.
    /// Cache misses replay the immutable source from the canonical origin;
    /// they never change the high-water enumerator owned by `next_page`.
    pub fn load_page_by_alternative_index(
        &mut self,
        alternative_index_decimal: &str,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<usize, PortfolioAlternativeError> {
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
        if compare_canonical_decimals(
            alternative_index_decimal,
            &self.store.checkpoint().known_alternative_count_decimal,
        )
        .is_gt()
        {
            return Err(PortfolioAlternativeError::PageNotLoaded);
        }

        let known_alternative_count_decimal = self
            .store
            .checkpoint()
            .known_alternative_count_decimal
            .clone();
        let mut replay = self.replay_origin.clone();
        let mut rebuilt_pages = Vec::new();
        rebuilt_pages
            .try_reserve_exact(PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT)
            .map_err(|_| PortfolioAlternativeError::AllocationFailed)?;

        if alternative_index_decimal == "1" {
            rebuilt_pages.push(self.source().canonical_page().clone());
            if known_alternative_count_decimal != "1" {
                rebuilt_pages.push(next_replayed_page(&mut replay, cancelled)?);
            }
        } else {
            let mut previous_page = self.source().canonical_page().clone();
            loop {
                let page = next_replayed_page(&mut replay, cancelled)?;
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
                            rebuilt_pages.push(next_replayed_page(&mut replay, cancelled)?);
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
        self.loaded_pages = rebuilt_pages;
        self.focused_slot = focused_slot;
        Ok(focused_slot)
    }

    pub fn next_page(
        &mut self,
        maximum_work_steps: u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PortfolioAlternativeAdvance, PortfolioAlternativeError> {
        let previous_high_water = self
            .store
            .checkpoint()
            .known_alternative_count_decimal
            .clone();
        if let Some(slot) = self.retained_page_slot(&previous_high_water) {
            self.focused_slot = slot;
        } else {
            self.load_page_by_alternative_index(&previous_high_water, cancelled)?;
        }
        let advance = self.store.next_page(maximum_work_steps, cancelled)?;
        if let Some(page) = advance.page() {
            let retained_slot = self.remember_page(page.clone())?;
            self.focused_slot = retained_slot.saturating_sub(1);
        }
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
            // high-water store. Counting its full retained bytes is a
            // conservative admission bound which also reserves one transient
            // replay clone used by a cache miss.
            .checked_add(
                self.replay_origin
                    .checked_enumerator_retained_capacity_bytes()?,
            )?;
        bytes = bytes.checked_add(
            (self.loaded_pages.capacity() as u128)
                .checked_mul(core::mem::size_of::<PortfolioAlternativePage>() as u128)?,
        )?;
        for page in &self.loaded_pages {
            bytes = bytes.checked_add(checked_page_nested_retained_capacity_bytes(page)?)?;
        }
        Some(bytes)
    }
}

fn next_replayed_page(
    replay: &mut CoveragePortfolioAlternativeStore,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<PortfolioAlternativePage, PortfolioAlternativeError> {
    loop {
        let advance = replay.next_page(u64::MAX, cancelled)?;
        if let Some(page) = advance.page() {
            return Ok(page.clone());
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
        && (!value.starts_with('0') || value == "0")
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
        CoveragePortfolioAlternativeSet::new(
            identity(),
            keys,
            PatternBitSet::all(3),
            rows,
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
        )
        .expect("tied set")
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
}
