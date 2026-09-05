//! Typed, fail-closed audit and portfolio planning for normalized solution sets.
//! SRP rationale: this module has one change reason: exact audit and portfolio-plan semantics for one normalized solution set.
//!
//! Probability and execution-constraint authority stay in their existing owners. This module
//! consumes their already-materialized normalized coverage evidence and records how a solution
//! collection changes across the public search pipeline.

use std::fmt::Write;

use clearra_core_domain::solution::normalized_tiling_solution::normalized_tiling_solution_key_set_hash_from_sorted_strings;
use clearra_coverage::{
    cover::{
        exact_minimum_cover::ExactMinimumCoverError,
        exact_minimum_cover_portfolios::ExactMinimumCoverPortfolioError,
        minimum_cover_solver::MinimumCoverSolver,
    },
    matrix::{coverage_matrix::CoverageMatrix, coverage_row::CoverageRow as MatrixCoverageRow},
    pattern::pattern_bitset::PatternBitSet,
};
use sha2::{Digest, Sha256};

pub const SOLUTION_SET_AUDIT_SCHEMA: &str = "clearra-solution-set-audit/v1";
const MAX_EXACT_PATTERN_STORAGE_DEDUP_COMPARISONS: u128 = 4_194_304;
const REDACTED_AUDIT_BASE_FIELD_COUNT: usize = 17;
const REDACTED_AUDIT_STAGE_FIELD_SUFFIXES: [(&str, &str); 7] = [
    ("_input_count", "unknown"),
    ("_output_count", "unknown"),
    ("_complete", "false"),
    ("_rejection_count", "unknown"),
    ("_rejection_reasons", "solution-authority-not-materialized"),
    ("_input_identity_hash", "not-materialized"),
    ("_output_identity_hash", "not-materialized"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolutionSetAuditFieldProjection {
    pub field_count: usize,
    pub required_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionSetAuditFieldBuildError {
    ProjectionOverflow,
    AllocationFailed { required_bytes: u128 },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SolutionProductFamily {
    Pc,
    BuildProbability,
}

impl SolutionProductFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pc => "pc",
            Self::BuildProbability => "build-probability",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionPortfolioSelectionPolicy {
    EquivalentCoverageRepresentatives,
    /// Audits the source dictionary while exact portfolio selection remains
    /// owned by the typed product coordinator. The report stays incomplete
    /// until that product authority performs and validates the exact proof.
    ProductDeferredExactMinimumCover,
    ExactMinimumCover,
}

impl SolutionPortfolioSelectionPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EquivalentCoverageRepresentatives => "equivalent-coverage-representatives",
            Self::ProductDeferredExactMinimumCover => {
                "product-deferred-two-pass-exact-minimum-cover"
            }
            Self::ExactMinimumCover => "two-pass-exact-minimum-cover",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionSetAuditStageKind {
    Produced,
    ExecutionValidated,
    SpinB2bFiltered,
    Normalized,
    CoverageClassed,
    PortfolioSelected,
    MaterializedPaged,
}

impl SolutionSetAuditStageKind {
    pub const ALL: [Self; 7] = [
        Self::Produced,
        Self::ExecutionValidated,
        Self::SpinB2bFiltered,
        Self::Normalized,
        Self::CoverageClassed,
        Self::PortfolioSelected,
        Self::MaterializedPaged,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Produced => "produced",
            Self::ExecutionValidated => "execution-validated",
            Self::SpinB2bFiltered => "spin-b2b-filtered",
            Self::Normalized => "normalized",
            Self::CoverageClassed => "coverage-classed",
            Self::PortfolioSelected => "portfolio-selected",
            Self::MaterializedPaged => "materialized-paged",
        }
    }

    fn field_prefix(self) -> &'static str {
        match self {
            Self::Produced => "solution_set_audit_produced",
            Self::ExecutionValidated => "solution_set_audit_execution_validated",
            Self::SpinB2bFiltered => "solution_set_audit_spin_b2b_filtered",
            Self::Normalized => "solution_set_audit_normalized",
            Self::CoverageClassed => "solution_set_audit_coverage_classed",
            Self::PortfolioSelected => "solution_set_audit_portfolio_selected",
            Self::MaterializedPaged => "solution_set_audit_materialized_paged",
        }
    }

    fn rejection_reason(self) -> &'static str {
        match self {
            Self::Produced => "production-rejected-solutions",
            Self::ExecutionValidated => "execution-validation-rejected-solutions",
            Self::SpinB2bFiltered => "spin-b2b-filter-rejected-solutions",
            Self::Normalized => "normalization-rejected-solutions",
            Self::CoverageClassed => "coverage-equivalent-solutions-collapsed",
            Self::PortfolioSelected => "portfolio-policy-unselected-coverage-classes",
            Self::MaterializedPaged => "portfolio-materialization-rejected-entries",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionAuditCheckpoint {
    count: Option<usize>,
    complete: bool,
    identity_hash: Option<String>,
    incomplete_reasons: Vec<String>,
}

impl SolutionAuditCheckpoint {
    pub fn new(
        count: Option<usize>,
        complete: bool,
        identity_hash: Option<String>,
        incomplete_reasons: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let mut incomplete_reasons = incomplete_reasons
            .into_iter()
            .map(Into::into)
            .filter(|reason| !reason.trim().is_empty() && reason != "none")
            .collect::<Vec<_>>();
        let identity_hash = identity_hash.filter(|identity| {
            !identity.trim().is_empty()
                && !matches!(
                    identity.as_str(),
                    "unknown" | "not-calculated" | "not-materialized" | "unavailable"
                )
        });
        let complete = complete && count.is_some() && identity_hash.is_some();
        if !complete && incomplete_reasons.is_empty() {
            incomplete_reasons.push("checkpoint-evidence-incomplete".to_owned());
        }
        incomplete_reasons.sort_unstable();
        incomplete_reasons.dedup();
        Self {
            count,
            complete,
            identity_hash,
            incomplete_reasons,
        }
    }

    pub fn known(count: usize, identity_hash: impl Into<String>) -> Self {
        Self::new(
            Some(count),
            true,
            Some(identity_hash.into()),
            Vec::<String>::new(),
        )
    }

    pub fn unknown(reason: impl Into<String>) -> Self {
        Self::new(None, false, None, [reason.into()])
    }

    pub const fn count(&self) -> Option<usize> {
        self.count
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn identity_hash(&self) -> Option<&str> {
        self.identity_hash.as_deref()
    }

    pub fn incomplete_reasons(&self) -> &[String] {
        &self.incomplete_reasons
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionSetAuditStage {
    kind: SolutionSetAuditStageKind,
    input_count: Option<usize>,
    output_count: Option<usize>,
    complete: bool,
    rejection_count: Option<usize>,
    rejection_reasons: Vec<String>,
    input_identity_hash: Option<String>,
    output_identity_hash: Option<String>,
}

impl SolutionSetAuditStage {
    fn transition(
        kind: SolutionSetAuditStageKind,
        input: &SolutionAuditCheckpoint,
        output: &SolutionAuditCheckpoint,
        reasons: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let mut rejection_reasons = input
            .incomplete_reasons()
            .iter()
            .chain(output.incomplete_reasons())
            .cloned()
            .collect::<Vec<_>>();
        rejection_reasons.extend(
            reasons
                .into_iter()
                .map(Into::into)
                .filter(|reason| !reason.trim().is_empty() && reason != "none"),
        );
        let rejection_count = match (input.count(), output.count()) {
            (Some(input), Some(output)) if output <= input => Some(input - output),
            (Some(_), Some(_)) => {
                rejection_reasons.push("stage-count-increased".to_owned());
                None
            }
            _ => None,
        };
        if rejection_count.is_some_and(|count| count != 0) {
            rejection_reasons.push(kind.rejection_reason().to_owned());
        }
        let complete = input.complete()
            && output.complete()
            && rejection_count.is_some()
            && input.identity_hash().is_some()
            && output.identity_hash().is_some();
        if !complete && rejection_reasons.is_empty() {
            rejection_reasons.push("stage-evidence-incomplete".to_owned());
        }
        rejection_reasons.sort_unstable();
        rejection_reasons.dedup();
        Self {
            kind,
            input_count: input.count(),
            output_count: output.count(),
            complete,
            rejection_count,
            rejection_reasons,
            input_identity_hash: input.identity_hash().map(ToOwned::to_owned),
            output_identity_hash: output.identity_hash().map(ToOwned::to_owned),
        }
    }

    fn source(kind: SolutionSetAuditStageKind, output: &SolutionAuditCheckpoint) -> Self {
        Self::transition(kind, output, output, Vec::<String>::new())
    }

    pub const fn kind(&self) -> SolutionSetAuditStageKind {
        self.kind
    }

    pub const fn input_count(&self) -> Option<usize> {
        self.input_count
    }

    pub const fn output_count(&self) -> Option<usize> {
        self.output_count
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub const fn rejection_count(&self) -> Option<usize> {
        self.rejection_count
    }

    pub fn rejection_reasons(&self) -> &[String] {
        &self.rejection_reasons
    }

    pub fn input_identity_hash(&self) -> Option<&str> {
        self.input_identity_hash.as_deref()
    }

    pub fn output_identity_hash(&self) -> Option<&str> {
        self.output_identity_hash.as_deref()
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SolutionSemanticDimensions {
    product_family: SolutionProductFamily,
    objective: String,
    score_profile: String,
    spin_profile: String,
    b2b_policy: String,
}

impl SolutionSemanticDimensions {
    pub fn new(
        product_family: SolutionProductFamily,
        objective: impl Into<String>,
        score_profile: impl Into<String>,
        spin_profile: impl Into<String>,
        b2b_policy: impl Into<String>,
    ) -> Self {
        Self {
            product_family,
            objective: canonical_dimension(objective.into()),
            score_profile: canonical_dimension(score_profile.into()),
            spin_profile: canonical_dimension(spin_profile.into()),
            b2b_policy: canonical_dimension(b2b_policy.into()),
        }
    }

    pub const fn product_family(&self) -> SolutionProductFamily {
        self.product_family
    }

    pub fn objective(&self) -> &str {
        &self.objective
    }

    pub fn score_profile(&self) -> &str {
        &self.score_profile
    }

    pub fn spin_profile(&self) -> &str {
        &self.spin_profile
    }

    pub fn b2b_policy(&self) -> &str {
        &self.b2b_policy
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionAuditCandidate {
    canonical_key: String,
    coverage: PatternBitSet,
    dimensions: SolutionSemanticDimensions,
}

impl SolutionAuditCandidate {
    pub fn new(
        canonical_key: impl Into<String>,
        coverage: PatternBitSet,
        dimensions: SolutionSemanticDimensions,
    ) -> Result<Self, SolutionSetAuditError> {
        let canonical_key = canonical_key.into();
        if canonical_key.trim().is_empty() {
            return Err(SolutionSetAuditError::EmptyCanonicalKey);
        }
        Ok(Self {
            canonical_key,
            coverage,
            dimensions,
        })
    }

    pub fn canonical_key(&self) -> &str {
        &self.canonical_key
    }

    pub fn coverage(&self) -> &PatternBitSet {
        &self.coverage
    }

    pub fn dimensions(&self) -> &SolutionSemanticDimensions {
        &self.dimensions
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionSetAuditInput {
    product_family: SolutionProductFamily,
    produced: SolutionAuditCheckpoint,
    execution_validated: SolutionAuditCheckpoint,
    spin_b2b_filtered: SolutionAuditCheckpoint,
    normalized_keys: Vec<String>,
    normalized_complete: bool,
    normalized_incomplete_reasons: Vec<String>,
    candidates: Vec<SolutionAuditCandidate>,
    required_patterns: PatternBitSet,
    selection_policy: SolutionPortfolioSelectionPolicy,
}

impl SolutionSetAuditInput {
    pub fn new(
        product_family: SolutionProductFamily,
        required_patterns: PatternBitSet,
        selection_policy: SolutionPortfolioSelectionPolicy,
    ) -> Self {
        Self {
            product_family,
            produced: SolutionAuditCheckpoint::unknown("produced-evidence-missing"),
            execution_validated: SolutionAuditCheckpoint::unknown(
                "execution-validation-evidence-missing",
            ),
            spin_b2b_filtered: SolutionAuditCheckpoint::unknown("spin-b2b-filter-evidence-missing"),
            normalized_keys: Vec::new(),
            normalized_complete: false,
            normalized_incomplete_reasons: vec!["normalized-evidence-missing".to_owned()],
            candidates: Vec::new(),
            required_patterns,
            selection_policy,
        }
    }

    pub fn with_source_checkpoints(
        mut self,
        produced: SolutionAuditCheckpoint,
        execution_validated: SolutionAuditCheckpoint,
        spin_b2b_filtered: SolutionAuditCheckpoint,
    ) -> Self {
        self.produced = produced;
        self.execution_validated = execution_validated;
        self.spin_b2b_filtered = spin_b2b_filtered;
        self
    }

    pub fn with_normalized_keys(
        mut self,
        normalized_keys: Vec<String>,
        complete: bool,
        incomplete_reasons: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.normalized_keys = normalized_keys;
        self.normalized_complete = complete;
        self.normalized_incomplete_reasons =
            incomplete_reasons.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_candidates(mut self, candidates: Vec<SolutionAuditCandidate>) -> Self {
        self.candidates = candidates;
        self
    }

    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = self.required_patterns.checked_shared_retained_bytes()?;
        bytes = bytes.checked_add(self.produced.checked_nested_retained_bytes()?)?;
        bytes = bytes.checked_add(self.execution_validated.checked_nested_retained_bytes()?)?;
        bytes = bytes.checked_add(self.spin_b2b_filtered.checked_nested_retained_bytes()?)?;
        bytes = bytes.checked_add(checked_string_vec_retained_bytes(&self.normalized_keys)?)?;
        bytes = bytes.checked_add(checked_string_vec_retained_bytes(
            &self.normalized_incomplete_reasons,
        )?)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes::<SolutionAuditCandidate>(
            &self.candidates,
        )?)?;
        for candidate in &self.candidates {
            bytes = bytes
                .checked_add(candidate.canonical_key.capacity() as u128)?
                .checked_add(checked_dimensions_retained_bytes(&candidate.dimensions)?)?;
            // Candidate bitsets can share result storage. Counting each owner
            // here is conservative for the external-live input boundary.
            bytes = bytes.checked_add(candidate.coverage.checked_storage_retained_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CoverageClassKey {
    dimensions: SolutionSemanticDimensions,
    pattern_count: usize,
    words: Vec<u64>,
}

impl Ord for CoverageClassKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.dimensions
            .cmp(&other.dimensions)
            .then_with(|| self.pattern_count.cmp(&other.pattern_count))
            .then_with(|| self.words.cmp(&other.words))
    }
}

impl PartialOrd for CoverageClassKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EquivalentCoverageClass {
    class_id: String,
    dimensions: SolutionSemanticDimensions,
    coverage: PatternBitSet,
    member_keys: Vec<String>,
    representative_key: String,
}

impl EquivalentCoverageClass {
    pub fn class_id(&self) -> &str {
        &self.class_id
    }

    pub fn dimensions(&self) -> &SolutionSemanticDimensions {
        &self.dimensions
    }

    pub fn coverage(&self) -> &PatternBitSet {
        &self.coverage
    }

    pub fn member_keys(&self) -> &[String] {
        &self.member_keys
    }

    pub fn representative_key(&self) -> &str {
        &self.representative_key
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionPortfolioFamily {
    family_id: String,
    dimensions: SolutionSemanticDimensions,
    coverage_classes: Vec<EquivalentCoverageClass>,
    selected_class_ids: Vec<String>,
    representative_keys: Vec<String>,
    required_patterns: PatternBitSet,
    covered_patterns: PatternBitSet,
    complete: bool,
    exact_minimum_proven: bool,
    incomplete_reasons: Vec<String>,
}

impl SolutionPortfolioFamily {
    pub fn family_id(&self) -> &str {
        &self.family_id
    }

    pub fn dimensions(&self) -> &SolutionSemanticDimensions {
        &self.dimensions
    }

    pub fn coverage_classes(&self) -> &[EquivalentCoverageClass] {
        &self.coverage_classes
    }

    pub fn selected_class_ids(&self) -> &[String] {
        &self.selected_class_ids
    }

    pub fn representative_keys(&self) -> &[String] {
        &self.representative_keys
    }

    pub fn required_patterns(&self) -> &PatternBitSet {
        &self.required_patterns
    }

    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub const fn exact_minimum_proven(&self) -> bool {
        self.exact_minimum_proven
    }

    pub fn incomplete_reasons(&self) -> &[String] {
        &self.incomplete_reasons
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SolutionPortfolioPageEntry {
    family_id: String,
    class_id: String,
    representative_key: String,
}

impl SolutionPortfolioPageEntry {
    fn new(
        family_id: impl Into<String>,
        class_id: impl Into<String>,
        representative_key: impl Into<String>,
    ) -> Self {
        Self {
            family_id: family_id.into(),
            class_id: class_id.into(),
            representative_key: representative_key.into(),
        }
    }

    pub fn family_id(&self) -> &str {
        &self.family_id
    }

    pub fn class_id(&self) -> &str {
        &self.class_id
    }

    pub fn representative_key(&self) -> &str {
        &self.representative_key
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionPortfolioCursor {
    snapshot_id: String,
    offset: usize,
}

impl SolutionPortfolioCursor {
    pub fn new(snapshot_id: impl Into<String>, offset: usize) -> Self {
        Self {
            snapshot_id: snapshot_id.into(),
            offset,
        }
    }

    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    pub const fn offset(&self) -> usize {
        self.offset
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionPortfolioPage {
    snapshot_id: String,
    offset: usize,
    entries: Vec<SolutionPortfolioPageEntry>,
    next_cursor: Option<SolutionPortfolioCursor>,
    complete: bool,
}

impl SolutionPortfolioPage {
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    pub const fn offset(&self) -> usize {
        self.offset
    }

    pub fn entries(&self) -> &[SolutionPortfolioPageEntry] {
        &self.entries
    }

    pub fn next_cursor(&self) -> Option<&SolutionPortfolioCursor> {
        self.next_cursor.as_ref()
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SolutionPortfolioPageError {
    ZeroLimit,
    SnapshotDrift { expected: String, actual: String },
    CursorOutOfRange { offset: usize, entry_count: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionPortfolioSnapshot {
    snapshot_id: String,
    entries: Vec<SolutionPortfolioPageEntry>,
    complete: bool,
}

impl SolutionPortfolioSnapshot {
    fn new(
        mut entries: Vec<SolutionPortfolioPageEntry>,
        complete: bool,
        selection_policy: SolutionPortfolioSelectionPolicy,
        coverage_class_set_identity: &str,
        required_patterns: &PatternBitSet,
    ) -> Self {
        entries.sort_unstable();
        entries.dedup();
        let snapshot_id = hash_portfolio_snapshot(
            &entries,
            complete,
            selection_policy,
            coverage_class_set_identity,
            required_patterns,
        );
        Self {
            snapshot_id,
            entries,
            complete,
        }
    }

    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn page(
        &self,
        cursor: Option<&SolutionPortfolioCursor>,
        limit: usize,
    ) -> Result<SolutionPortfolioPage, SolutionPortfolioPageError> {
        if limit == 0 {
            return Err(SolutionPortfolioPageError::ZeroLimit);
        }
        let offset = if let Some(cursor) = cursor {
            if cursor.snapshot_id() != self.snapshot_id() {
                return Err(SolutionPortfolioPageError::SnapshotDrift {
                    expected: self.snapshot_id.clone(),
                    actual: cursor.snapshot_id().to_owned(),
                });
            }
            cursor.offset()
        } else {
            0
        };
        if offset > self.entries.len() {
            return Err(SolutionPortfolioPageError::CursorOutOfRange {
                offset,
                entry_count: self.entries.len(),
            });
        }
        let end = offset.saturating_add(limit).min(self.entries.len());
        let next_cursor = (end < self.entries.len())
            .then(|| SolutionPortfolioCursor::new(self.snapshot_id.clone(), end));
        Ok(SolutionPortfolioPage {
            snapshot_id: self.snapshot_id.clone(),
            offset,
            entries: self.entries[offset..end].to_vec(),
            next_cursor,
            complete: self.complete && end == self.entries.len(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionSetAuditReport {
    product_family: SolutionProductFamily,
    selection_policy: SolutionPortfolioSelectionPolicy,
    stages: Vec<SolutionSetAuditStage>,
    coverage_classes: Vec<EquivalentCoverageClass>,
    portfolio_families: Vec<SolutionPortfolioFamily>,
    portfolio_snapshot: SolutionPortfolioSnapshot,
    required_patterns: PatternBitSet,
    complete: bool,
    incomplete_reasons: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolutionSetAuditMemoryProjection {
    pub required_peak_bytes: u128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SolutionSetAuditMemoryGuardError {
    ProjectionOverflow,
    MemoryCapacityExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    Audit(SolutionSetAuditError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SolutionSetAuditGuardedError<E> {
    ProjectionOverflow,
    MemoryGuard(E),
    Audit(SolutionSetAuditError),
}

impl SolutionSetAuditReport {
    /// Sound owner-count upper bound for the sorted-Vec audit implementation.
    /// The input owners are excluded and must be supplied as
    /// `already_retained_bytes` by the caller.
    pub fn checked_analysis_memory_projection(
        input: &SolutionSetAuditInput,
    ) -> Option<SolutionSetAuditMemoryProjection> {
        let normalized_count = input.normalized_keys.len();
        let candidate_count = input.candidates.len();
        let owner_count = normalized_count.checked_add(candidate_count)?;
        let normalized_key_bytes = input
            .normalized_keys
            .iter()
            .try_fold(0_u128, |bytes, key| bytes.checked_add(key.len() as u128))?;
        let candidate_key_bytes = input
            .candidates
            .iter()
            .try_fold(0_u128, |bytes, candidate| {
                bytes.checked_add(candidate.canonical_key().len() as u128)
            })?;
        let dimension_bytes = input
            .candidates
            .iter()
            .try_fold(0_u128, |bytes, candidate| {
                bytes.checked_add(checked_dimensions_clone_bytes(candidate.dimensions())?)
            })?;
        // Every normalized/candidate key can coexist in: missing-coverage
        // scratch, class membership, class representative, the family-owned
        // class clone, selected representatives, and the page snapshot. Two
        // further copies cover error/stage scratch and sort/dedup ownership.
        let key_payload_bytes = normalized_key_bytes
            .checked_add(candidate_key_bytes)?
            .checked_mul(8)?;
        // Dimensions coexist in the coverage class, its family-owned class
        // clone, and the family identity itself; one sort/hash scratch copy is
        // included as the fourth owner.
        let dimension_payload_bytes = dimension_bytes.checked_mul(4)?;
        // Checkpoint reasons retain independently allocated stage clones and
        // formatted aggregate owners. This projection uses the actual source
        // Vec/String capacities rather than treating payload bytes as the only
        // owner; see `checked_checkpoint_reason_owner_peak_bytes`.
        let checkpoint_reason_owner_peak_bytes = checked_checkpoint_reason_owner_peak_bytes(input)?;

        let owner_count_u128 = owner_count as u128;
        let candidate_count_u128 = candidate_count as u128;
        let slot_bytes = owner_count_u128
            .checked_mul(core::mem::size_of::<String>() as u128)?
            .checked_mul(8)?
            .checked_add(
                candidate_count_u128
                    .checked_mul(core::mem::size_of::<usize>() as u128)?
                    .checked_mul(4)?,
            )?
            .checked_add(
                candidate_count_u128
                    .checked_mul(core::mem::size_of::<EquivalentCoverageClass>() as u128)?
                    .checked_mul(2)?,
            )?
            .checked_add(
                candidate_count_u128
                    .checked_mul(core::mem::size_of::<SolutionPortfolioFamily>() as u128)?,
            )?
            .checked_add(
                candidate_count_u128
                    .checked_mul(core::mem::size_of::<SolutionPortfolioPageEntry>() as u128)?,
            )?
            .checked_add(
                (SolutionSetAuditStageKind::ALL.len() as u128)
                    .checked_mul(core::mem::size_of::<SolutionSetAuditStage>() as u128)?,
            )?;
        // SHA-256 identities are at most seven-byte prefixes plus 64 hex
        // digits. Four identities per candidate cover class, family, selected
        // entry, and snapshot ownership; fixed stage/checkpoint identities use
        // the final sixteen slots.
        let identity_bytes = candidate_count_u128
            .checked_mul(4)?
            .checked_add(16)?
            .checked_mul(71)?
            // Source checkpoint identities are public input strings, not
            // fixed SHA-256 values. Their transition clones must therefore be
            // sized from the actual input lengths.
            .checked_add(checked_checkpoint_identity_clone_bytes(input)?)?;
        let pattern_owner_count = candidate_count_u128.checked_mul(8)?.checked_add(16)?;
        let all_pattern_ids =
            pattern_owner_count.checked_mul(input.required_patterns.pattern_count() as u128)?;
        let pattern_bytes = PatternBitSet::checked_shared_construction_upper_bound(
            input.required_patterns.pattern_count(),
            pattern_owner_count,
            all_pattern_ids,
        )?;
        let required_peak_bytes = slot_bytes
            .checked_add(key_payload_bytes)?
            .checked_add(dimension_payload_bytes)?
            .checked_add(checkpoint_reason_owner_peak_bytes)?
            .checked_add(identity_bytes)?
            .checked_add(pattern_bytes)?;
        Some(SolutionSetAuditMemoryProjection {
            required_peak_bytes,
        })
    }

    pub fn analyze_with_memory_limit(
        input: SolutionSetAuditInput,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<(Self, SolutionSetAuditMemoryProjection), SolutionSetAuditMemoryGuardError> {
        Self::analyze_with_memory_guard(input, &mut |owned_bytes| {
            let required_memory_bytes = already_retained_bytes
                .checked_add(owned_bytes)
                .ok_or(SolutionSetAuditMemoryGuardError::ProjectionOverflow)?;
            if required_memory_bytes > max_memory_bytes {
                return Err(SolutionSetAuditMemoryGuardError::MemoryCapacityExceeded {
                    required_memory_bytes,
                    max_memory_bytes,
                });
            }
            Ok(())
        })
        .map_err(|error| match error {
            SolutionSetAuditGuardedError::ProjectionOverflow => {
                SolutionSetAuditMemoryGuardError::ProjectionOverflow
            }
            SolutionSetAuditGuardedError::MemoryGuard(error) => error,
            SolutionSetAuditGuardedError::Audit(error) => {
                SolutionSetAuditMemoryGuardError::Audit(error)
            }
        })
    }

    /// Applies one caller-owned authority to the audit's conservative base
    /// projection, every exact-solver growth, and the final actual report.
    /// Input storage is excluded and remains the caller's responsibility.
    pub fn analyze_with_memory_guard<E>(
        input: SolutionSetAuditInput,
        memory_guard: &mut impl FnMut(u128) -> Result<(), E>,
    ) -> Result<(Self, SolutionSetAuditMemoryProjection), SolutionSetAuditGuardedError<E>> {
        let projection = Self::checked_analysis_memory_projection(&input)
            .ok_or(SolutionSetAuditGuardedError::ProjectionOverflow)?;
        memory_guard(projection.required_peak_bytes)
            .map_err(SolutionSetAuditGuardedError::MemoryGuard)?;

        let mut caller_guard_error = None;
        let mut exact_solver_error = None;
        let report = {
            let mut exact_solver_guard = |solver_owned_bytes: u128| {
                let owned_bytes = projection
                    .required_peak_bytes
                    .checked_add(solver_owned_bytes)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                match memory_guard(owned_bytes) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        caller_guard_error = Some(error);
                        Err(ExactMinimumCoverError::MemoryGuardRejected)
                    }
                }
            };
            Self::analyze_inner(input, &mut exact_solver_guard, &mut exact_solver_error)
        };
        if let Some(error) = caller_guard_error {
            return Err(SolutionSetAuditGuardedError::MemoryGuard(error));
        }
        if let Some(ExactMinimumCoverError::ProjectionOverflow) = exact_solver_error {
            return Err(SolutionSetAuditGuardedError::ProjectionOverflow);
        }
        let report = report.map_err(SolutionSetAuditGuardedError::Audit)?;
        let actual_retained_bytes = report
            .checked_nested_retained_bytes()
            .ok_or(SolutionSetAuditGuardedError::ProjectionOverflow)?;
        memory_guard(actual_retained_bytes).map_err(SolutionSetAuditGuardedError::MemoryGuard)?;
        Ok((report, projection))
    }

    /// Allocation-free projection for the public redacted audit summary. This
    /// path deliberately does not construct a private audit report.
    pub fn checked_redacted_summary_field_projection(
        product_family: SolutionProductFamily,
        selection_policy: SolutionPortfolioSelectionPolicy,
        required_pattern_count: usize,
    ) -> Option<SolutionSetAuditFieldProjection> {
        let field_count = REDACTED_AUDIT_BASE_FIELD_COUNT.checked_add(
            SolutionSetAuditStageKind::ALL
                .len()
                .checked_mul(REDACTED_AUDIT_STAGE_FIELD_SUFFIXES.len())?,
        )?;
        let mut string_bytes = 0_u128;
        for (key, value_len) in [
            ("solution_set_audit_schema", SOLUTION_SET_AUDIT_SCHEMA.len()),
            (
                "solution_set_audit_product_family",
                product_family.as_str().len(),
            ),
            ("solution_set_audit_complete", "false".len()),
            (
                "solution_set_audit_incomplete_reasons",
                "solution-authority-not-materialized".len(),
            ),
            (
                "solution_set_audit_stage_count",
                decimal_len(SolutionSetAuditStageKind::ALL.len()),
            ),
            (
                "solution_set_audit_private_authority",
                "not-materialized".len(),
            ),
            ("solution_coverage_class_count", "not-materialized".len()),
            ("solution_portfolio_family_count", "not-materialized".len()),
            (
                "solution_portfolio_representative_count",
                "not-materialized".len(),
            ),
            (
                "solution_portfolio_selection",
                selection_policy.as_str().len(),
            ),
            ("solution_portfolio_two_pass_exact", "false".len()),
            ("solution_portfolio_exact_minimum_proven", "false".len()),
            (
                "solution_portfolio_required_pattern_count",
                decimal_len(required_pattern_count),
            ),
            ("solution_portfolio_snapshot_id", "not-materialized".len()),
            ("solution_portfolio_snapshot_complete", "false".len()),
            (
                "solution_portfolio_cursor_semantics",
                "snapshot-id+offset".len(),
            ),
            (
                "solution_portfolio_cursor_drift_policy",
                "fail-closed".len(),
            ),
        ] {
            string_bytes = string_bytes
                .checked_add(key.len() as u128)?
                .checked_add(value_len as u128)?;
        }
        for kind in SolutionSetAuditStageKind::ALL {
            for (suffix, value) in REDACTED_AUDIT_STAGE_FIELD_SUFFIXES {
                string_bytes = string_bytes
                    .checked_add(kind.field_prefix().len() as u128)?
                    .checked_add(suffix.len() as u128)?
                    .checked_add(value.len() as u128)?;
            }
        }
        let required_bytes = (field_count as u128)
            .checked_mul(core::mem::size_of::<(String, String)>() as u128)?
            .checked_add(string_bytes)?;
        Some(SolutionSetAuditFieldProjection {
            field_count,
            required_bytes,
        })
    }

    /// Builds only the redacted public fields with fallible reserves. The
    /// returned byte count uses actual allocator capacities.
    pub fn try_redacted_summary_fields(
        product_family: SolutionProductFamily,
        selection_policy: SolutionPortfolioSelectionPolicy,
        required_pattern_count: usize,
    ) -> Result<(Vec<(String, String)>, u128), SolutionSetAuditFieldBuildError> {
        let projection = Self::checked_redacted_summary_field_projection(
            product_family,
            selection_policy,
            required_pattern_count,
        )
        .ok_or(SolutionSetAuditFieldBuildError::ProjectionOverflow)?;
        let allocation_error = || SolutionSetAuditFieldBuildError::AllocationFailed {
            required_bytes: projection.required_bytes,
        };
        let mut fields = Vec::new();
        fields
            .try_reserve_exact(projection.field_count)
            .map_err(|_| allocation_error())?;
        for (key, value) in [
            ("solution_set_audit_schema", SOLUTION_SET_AUDIT_SCHEMA),
            ("solution_set_audit_product_family", product_family.as_str()),
            ("solution_set_audit_complete", "false"),
            (
                "solution_set_audit_incomplete_reasons",
                "solution-authority-not-materialized",
            ),
        ] {
            try_push_owned_field(&mut fields, key, value, projection.required_bytes)?;
        }
        try_push_usize_field(
            &mut fields,
            "solution_set_audit_stage_count",
            SolutionSetAuditStageKind::ALL.len(),
            projection.required_bytes,
        )?;
        for (key, value) in [
            ("solution_set_audit_private_authority", "not-materialized"),
            ("solution_coverage_class_count", "not-materialized"),
            ("solution_portfolio_family_count", "not-materialized"),
            (
                "solution_portfolio_representative_count",
                "not-materialized",
            ),
            ("solution_portfolio_selection", selection_policy.as_str()),
            ("solution_portfolio_two_pass_exact", "false"),
            ("solution_portfolio_exact_minimum_proven", "false"),
        ] {
            try_push_owned_field(&mut fields, key, value, projection.required_bytes)?;
        }
        try_push_usize_field(
            &mut fields,
            "solution_portfolio_required_pattern_count",
            required_pattern_count,
            projection.required_bytes,
        )?;
        for (key, value) in [
            ("solution_portfolio_snapshot_id", "not-materialized"),
            ("solution_portfolio_snapshot_complete", "false"),
            ("solution_portfolio_cursor_semantics", "snapshot-id+offset"),
            ("solution_portfolio_cursor_drift_policy", "fail-closed"),
        ] {
            try_push_owned_field(&mut fields, key, value, projection.required_bytes)?;
        }
        for kind in SolutionSetAuditStageKind::ALL {
            for (suffix, value) in REDACTED_AUDIT_STAGE_FIELD_SUFFIXES {
                let key =
                    try_joined_string(kind.field_prefix(), suffix, projection.required_bytes)?;
                let value = try_owned_string(value, projection.required_bytes)?;
                fields.push((key, value));
            }
        }
        debug_assert_eq!(fields.len(), projection.field_count);
        let actual_bytes = checked_owned_field_storage_bytes(&fields)
            .ok_or(SolutionSetAuditFieldBuildError::ProjectionOverflow)?;
        Ok((fields, actual_bytes))
    }

    pub fn analyze(input: SolutionSetAuditInput) -> Result<Self, SolutionSetAuditError> {
        let mut exact_solver_error = None;
        let mut unbounded_guard = |_| Ok(());
        Self::analyze_inner(input, &mut unbounded_guard, &mut exact_solver_error)
    }

    fn analyze_inner(
        input: SolutionSetAuditInput,
        exact_solver_memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        exact_solver_error: &mut Option<ExactMinimumCoverError>,
    ) -> Result<Self, SolutionSetAuditError> {
        let mut normalized_reasons = input.normalized_incomplete_reasons;
        let normalized_input_count = input.normalized_keys.len();
        let mut normalized_keys = input
            .normalized_keys
            .into_iter()
            .filter(|key| {
                let keep = !key.trim().is_empty();
                if !keep {
                    normalized_reasons.push("empty-normalized-key-rejected".to_owned());
                }
                keep
            })
            .collect::<Vec<_>>();
        normalized_keys.sort_unstable();
        let before_dedup = normalized_keys.len();
        normalized_keys.dedup();
        if normalized_keys.len() != before_dedup {
            normalized_reasons.push("duplicate-normalized-key-rejected".to_owned());
        }
        let normalized_hash =
            normalized_tiling_solution_key_set_hash_from_sorted_strings(&normalized_keys);
        let normalized_checkpoint = SolutionAuditCheckpoint::new(
            Some(normalized_keys.len()),
            input.normalized_complete,
            Some(normalized_hash),
            normalized_reasons.clone(),
        );

        let mut candidate_by_key = input.candidates;
        for candidate in &candidate_by_key {
            if candidate.dimensions().product_family() != input.product_family {
                return Err(SolutionSetAuditError::ProductFamilyMismatch {
                    key: candidate.canonical_key().to_owned(),
                    expected: input.product_family,
                    actual: candidate.dimensions().product_family(),
                });
            }
            if candidate.coverage().pattern_count() != input.required_patterns.pattern_count() {
                return Err(SolutionSetAuditError::PatternCountMismatch {
                    key: candidate.canonical_key().to_owned(),
                    expected: input.required_patterns.pattern_count(),
                    actual: candidate.coverage().pattern_count(),
                });
            }
            if normalized_keys
                .binary_search_by(|key| key.as_str().cmp(candidate.canonical_key()))
                .is_err()
            {
                return Err(SolutionSetAuditError::OrphanCoverageEvidence {
                    key: candidate.canonical_key().to_owned(),
                });
            }
        }
        candidate_by_key
            .sort_unstable_by(|left, right| left.canonical_key().cmp(right.canonical_key()));
        let mut candidate_index = 1_usize;
        while candidate_index < candidate_by_key.len() {
            if candidate_by_key[candidate_index - 1].canonical_key()
                == candidate_by_key[candidate_index].canonical_key()
            {
                if candidate_by_key[candidate_index - 1] != candidate_by_key[candidate_index] {
                    return Err(SolutionSetAuditError::ConflictingCoverageEvidence {
                        key: candidate_by_key[candidate_index].canonical_key().to_owned(),
                    });
                }
                candidate_by_key.remove(candidate_index);
            } else {
                candidate_index += 1;
            }
        }

        let missing_coverage = normalized_keys
            .iter()
            .filter(|key| {
                candidate_by_key
                    .binary_search_by(|candidate| candidate.canonical_key().cmp(key.as_str()))
                    .is_err()
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut classing_reasons = Vec::new();
        if !missing_coverage.is_empty() {
            classing_reasons.push("normalized-solution-coverage-missing".to_owned());
        }
        let mut grouped_indices = (0..candidate_by_key.len()).collect::<Vec<_>>();
        grouped_indices.sort_unstable_by(|left, right| {
            let left = &candidate_by_key[*left];
            let right = &candidate_by_key[*right];
            left.dimensions()
                .cmp(right.dimensions())
                .then_with(|| {
                    left.coverage()
                        .pattern_count()
                        .cmp(&right.coverage().pattern_count())
                })
                .then_with(|| left.coverage().words().cmp(right.coverage().words()))
                .then_with(|| left.canonical_key().cmp(right.canonical_key()))
        });
        let mut coverage_classes = Vec::with_capacity(candidate_by_key.len());
        let mut group_start = 0_usize;
        while group_start < grouped_indices.len() {
            let first = &candidate_by_key[grouped_indices[group_start]];
            let mut group_end = group_start + 1;
            while group_end < grouped_indices.len() {
                let candidate = &candidate_by_key[grouped_indices[group_end]];
                if candidate.dimensions() != first.dimensions()
                    || candidate.coverage() != first.coverage()
                {
                    break;
                }
                group_end += 1;
            }
            let mut member_keys = grouped_indices[group_start..group_end]
                .iter()
                .map(|index| candidate_by_key[*index].canonical_key().to_owned())
                .collect::<Vec<_>>();
            member_keys.sort_unstable();
            member_keys.dedup();
            let representative_key = member_keys
                .first()
                .expect("coverage class has at least one member")
                .clone();
            let key = CoverageClassKey {
                dimensions: first.dimensions().clone(),
                pattern_count: first.coverage().pattern_count(),
                words: first.coverage().words().to_vec(),
            };
            coverage_classes.push(EquivalentCoverageClass {
                class_id: hash_coverage_class(&key),
                dimensions: key.dimensions,
                coverage: first.coverage().clone(),
                member_keys,
                representative_key,
            });
            group_start = group_end;
        }
        if coverage_classes.len() < candidate_by_key.len() {
            classing_reasons.push("coverage-equivalent-members-collapsed".to_owned());
        }
        let classed_complete = normalized_checkpoint.complete() && missing_coverage.is_empty();
        let classed_hash = hash_coverage_classes(&coverage_classes);
        let classed_checkpoint = SolutionAuditCheckpoint::new(
            Some(coverage_classes.len()),
            classed_complete,
            Some(classed_hash.clone()),
            classing_reasons.clone(),
        );

        let (portfolio_families, mut portfolio_reasons) = build_portfolio_families(
            &coverage_classes,
            &input.required_patterns,
            input.selection_policy,
            classed_checkpoint.complete(),
            exact_solver_memory_guard,
            exact_solver_error,
        )?;
        let selection_deferred = input.selection_policy
            == SolutionPortfolioSelectionPolicy::ProductDeferredExactMinimumCover;
        let portfolio_complete = classed_checkpoint.complete()
            && !selection_deferred
            && if portfolio_families.is_empty() {
                input.required_patterns.is_empty()
            } else {
                portfolio_families
                    .iter()
                    .all(SolutionPortfolioFamily::complete)
            };
        if selection_deferred
            && !portfolio_reasons.iter().any(|reason| {
                reason == "exact-minimum-cover-selection-deferred-to-product-coordinator"
            })
        {
            portfolio_reasons
                .push("exact-minimum-cover-selection-deferred-to-product-coordinator".to_owned());
        }
        if portfolio_families.is_empty() && !input.required_patterns.is_empty() {
            portfolio_reasons.push("required-patterns-have-no-coverage-family".to_owned());
        }
        portfolio_reasons.sort_unstable();
        portfolio_reasons.dedup();
        let mut page_entries = Vec::new();
        for family in &portfolio_families {
            for (class_id, representative_key) in family
                .selected_class_ids()
                .iter()
                .zip(family.representative_keys())
            {
                page_entries.push(SolutionPortfolioPageEntry::new(
                    family.family_id(),
                    class_id,
                    representative_key,
                ));
            }
        }
        let portfolio_snapshot = SolutionPortfolioSnapshot::new(
            page_entries,
            portfolio_complete,
            input.selection_policy,
            &classed_hash,
            &input.required_patterns,
        );
        let portfolio_checkpoint = SolutionAuditCheckpoint::new(
            Some(portfolio_snapshot.len()),
            portfolio_complete,
            Some(portfolio_snapshot.snapshot_id().to_owned()),
            portfolio_reasons.clone(),
        );
        let materialized_checkpoint = SolutionAuditCheckpoint::new(
            Some(portfolio_snapshot.len()),
            portfolio_snapshot.complete(),
            Some(portfolio_snapshot.snapshot_id().to_owned()),
            portfolio_reasons.clone(),
        );

        let mut stages = vec![
            SolutionSetAuditStage::source(SolutionSetAuditStageKind::Produced, &input.produced),
            SolutionSetAuditStage::transition(
                SolutionSetAuditStageKind::ExecutionValidated,
                &input.produced,
                &input.execution_validated,
                Vec::<String>::new(),
            ),
            SolutionSetAuditStage::transition(
                SolutionSetAuditStageKind::SpinB2bFiltered,
                &input.execution_validated,
                &input.spin_b2b_filtered,
                Vec::<String>::new(),
            ),
            SolutionSetAuditStage::transition(
                SolutionSetAuditStageKind::Normalized,
                &input.spin_b2b_filtered,
                &normalized_checkpoint,
                (normalized_input_count != normalized_keys.len())
                    .then_some("normalization-rejected-noncanonical-keys"),
            ),
            SolutionSetAuditStage::transition(
                SolutionSetAuditStageKind::CoverageClassed,
                &normalized_checkpoint,
                &classed_checkpoint,
                classing_reasons,
            ),
            SolutionSetAuditStage::transition(
                SolutionSetAuditStageKind::PortfolioSelected,
                &classed_checkpoint,
                &portfolio_checkpoint,
                portfolio_reasons.clone(),
            ),
            SolutionSetAuditStage::transition(
                SolutionSetAuditStageKind::MaterializedPaged,
                &portfolio_checkpoint,
                &materialized_checkpoint,
                portfolio_reasons,
            ),
        ];
        debug_assert_eq!(stages.len(), SolutionSetAuditStageKind::ALL.len());
        let complete = stages.iter().all(SolutionSetAuditStage::complete);
        let mut incomplete_reasons = stages
            .iter()
            .filter(|stage| !stage.complete())
            .flat_map(|stage| {
                stage
                    .rejection_reasons()
                    .iter()
                    .map(move |reason| format!("{}:{reason}", stage.kind().as_str()))
            })
            .collect::<Vec<_>>();
        incomplete_reasons.sort_unstable();
        incomplete_reasons.dedup();
        if !complete && incomplete_reasons.is_empty() {
            incomplete_reasons.push("audit-evidence-incomplete".to_owned());
        }
        stages.shrink_to_fit();

        Ok(Self {
            product_family: input.product_family,
            selection_policy: input.selection_policy,
            stages,
            coverage_classes,
            portfolio_families,
            portfolio_snapshot,
            required_patterns: input.required_patterns,
            complete,
            incomplete_reasons,
        })
    }

    pub fn unavailable(product_family: SolutionProductFamily, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self::analyze(
            SolutionSetAuditInput::new(
                product_family,
                PatternBitSet::new(0),
                SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives,
            )
            .with_source_checkpoints(
                SolutionAuditCheckpoint::unknown(reason.clone()),
                SolutionAuditCheckpoint::unknown(reason.clone()),
                SolutionAuditCheckpoint::unknown(reason.clone()),
            )
            .with_normalized_keys(Vec::new(), false, [reason]),
        )
        .expect("an unavailable audit contains no malformed solution evidence")
    }

    pub const fn product_family(&self) -> SolutionProductFamily {
        self.product_family
    }

    pub const fn selection_policy(&self) -> SolutionPortfolioSelectionPolicy {
        self.selection_policy
    }

    pub fn stages(&self) -> &[SolutionSetAuditStage] {
        &self.stages
    }

    pub fn stage(&self, kind: SolutionSetAuditStageKind) -> &SolutionSetAuditStage {
        self.stages
            .iter()
            .find(|stage| stage.kind() == kind)
            .expect("solution-set audit always owns every canonical stage")
    }

    pub fn coverage_classes(&self) -> &[EquivalentCoverageClass] {
        &self.coverage_classes
    }

    pub fn portfolio_families(&self) -> &[SolutionPortfolioFamily] {
        &self.portfolio_families
    }

    pub fn portfolio_snapshot(&self) -> &SolutionPortfolioSnapshot {
        &self.portfolio_snapshot
    }

    pub fn required_patterns(&self) -> &PatternBitSet {
        &self.required_patterns
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn exact_minimum_proven(&self) -> bool {
        self.selection_policy == SolutionPortfolioSelectionPolicy::ExactMinimumCover
            && self.complete
            && if self.portfolio_families.is_empty() {
                self.required_patterns.is_empty()
            } else {
                self.portfolio_families
                    .iter()
                    .all(SolutionPortfolioFamily::exact_minimum_proven)
            }
    }

    pub fn incomplete_reasons(&self) -> &[String] {
        &self.incomplete_reasons
    }

    pub fn checked_summary_field_projection(&self) -> Option<SolutionSetAuditFieldProjection> {
        let field_count =
            REDACTED_AUDIT_BASE_FIELD_COUNT.checked_add(self.stages.len().checked_mul(7)?)?;
        let mut string_bytes = 0_u128;
        let mut add = |key: &str, value_len: usize| -> Option<()> {
            string_bytes = string_bytes
                .checked_add(key.len() as u128)?
                .checked_add(value_len as u128)?;
            Some(())
        };
        for (key, value_len) in [
            ("solution_set_audit_schema", SOLUTION_SET_AUDIT_SCHEMA.len()),
            (
                "solution_set_audit_product_family",
                self.product_family.as_str().len(),
            ),
            ("solution_set_audit_complete", bool_len(self.complete)),
            (
                "solution_set_audit_incomplete_reasons",
                joined_reasons_len(&self.incomplete_reasons)?,
            ),
            (
                "solution_set_audit_stage_count",
                decimal_len(self.stages.len()),
            ),
            ("solution_set_audit_private_authority", "attached".len()),
            (
                "solution_coverage_class_count",
                decimal_len(self.coverage_classes.len()),
            ),
            (
                "solution_portfolio_family_count",
                decimal_len(self.portfolio_families.len()),
            ),
            (
                "solution_portfolio_representative_count",
                decimal_len(self.portfolio_snapshot.len()),
            ),
            (
                "solution_portfolio_selection",
                self.selection_policy.as_str().len(),
            ),
            (
                "solution_portfolio_two_pass_exact",
                bool_len(
                    self.selection_policy == SolutionPortfolioSelectionPolicy::ExactMinimumCover,
                ),
            ),
            (
                "solution_portfolio_exact_minimum_proven",
                bool_len(self.exact_minimum_proven()),
            ),
            (
                "solution_portfolio_required_pattern_count",
                decimal_len(self.required_patterns.count_ones() as usize),
            ),
            (
                "solution_portfolio_snapshot_id",
                self.portfolio_snapshot.snapshot_id().len(),
            ),
            (
                "solution_portfolio_snapshot_complete",
                bool_len(self.portfolio_snapshot.complete()),
            ),
            (
                "solution_portfolio_cursor_semantics",
                "snapshot-id+offset".len(),
            ),
            (
                "solution_portfolio_cursor_drift_policy",
                "fail-closed".len(),
            ),
        ] {
            add(key, value_len)?;
        }
        for stage in &self.stages {
            let prefix = stage.kind().field_prefix();
            for (suffix, value_len) in [
                ("_input_count", optional_count_len(stage.input_count())),
                ("_output_count", optional_count_len(stage.output_count())),
                ("_complete", bool_len(stage.complete())),
                (
                    "_rejection_count",
                    optional_count_len(stage.rejection_count()),
                ),
                (
                    "_rejection_reasons",
                    joined_reasons_len(stage.rejection_reasons())?,
                ),
                (
                    "_input_identity_hash",
                    stage.input_identity_hash().unwrap_or("unknown").len(),
                ),
                (
                    "_output_identity_hash",
                    stage.output_identity_hash().unwrap_or("unknown").len(),
                ),
            ] {
                string_bytes = string_bytes
                    .checked_add(prefix.len() as u128)?
                    .checked_add(suffix.len() as u128)?
                    .checked_add(value_len as u128)?;
            }
        }
        let required_bytes = (field_count as u128)
            .checked_mul(core::mem::size_of::<(String, String)>() as u128)?
            .checked_add(string_bytes)?;
        Some(SolutionSetAuditFieldProjection {
            field_count,
            required_bytes,
        })
    }

    /// Summary fields are a projection only; the typed report remains the audit authority.
    pub fn summary_fields(&self) -> Vec<(String, String)> {
        self.try_summary_fields()
            .expect("solution-set audit summary has a checked address-space projection")
            .0
    }

    pub fn try_summary_fields(
        &self,
    ) -> Result<(Vec<(String, String)>, u128), SolutionSetAuditFieldBuildError> {
        let projection = self
            .checked_summary_field_projection()
            .ok_or(SolutionSetAuditFieldBuildError::ProjectionOverflow)?;
        let mut fields = Vec::new();
        fields
            .try_reserve_exact(projection.field_count)
            .map_err(|_| SolutionSetAuditFieldBuildError::AllocationFailed {
                required_bytes: projection.required_bytes,
            })?;
        try_push_owned_field(
            &mut fields,
            "solution_set_audit_schema",
            SOLUTION_SET_AUDIT_SCHEMA,
            projection.required_bytes,
        )?;
        try_push_owned_field(
            &mut fields,
            "solution_set_audit_product_family",
            self.product_family.as_str(),
            projection.required_bytes,
        )?;
        try_push_bool_field(
            &mut fields,
            "solution_set_audit_complete",
            self.complete,
            projection.required_bytes,
        )?;
        try_push_joined_reasons_field(
            &mut fields,
            "solution_set_audit_incomplete_reasons",
            &self.incomplete_reasons,
            projection.required_bytes,
        )?;
        try_push_usize_field(
            &mut fields,
            "solution_set_audit_stage_count",
            self.stages.len(),
            projection.required_bytes,
        )?;
        try_push_owned_field(
            &mut fields,
            "solution_set_audit_private_authority",
            "attached",
            projection.required_bytes,
        )?;
        try_push_usize_field(
            &mut fields,
            "solution_coverage_class_count",
            self.coverage_classes.len(),
            projection.required_bytes,
        )?;
        try_push_usize_field(
            &mut fields,
            "solution_portfolio_family_count",
            self.portfolio_families.len(),
            projection.required_bytes,
        )?;
        try_push_usize_field(
            &mut fields,
            "solution_portfolio_representative_count",
            self.portfolio_snapshot.len(),
            projection.required_bytes,
        )?;
        try_push_owned_field(
            &mut fields,
            "solution_portfolio_selection",
            self.selection_policy.as_str(),
            projection.required_bytes,
        )?;
        try_push_bool_field(
            &mut fields,
            "solution_portfolio_two_pass_exact",
            self.selection_policy == SolutionPortfolioSelectionPolicy::ExactMinimumCover,
            projection.required_bytes,
        )?;
        try_push_bool_field(
            &mut fields,
            "solution_portfolio_exact_minimum_proven",
            self.exact_minimum_proven(),
            projection.required_bytes,
        )?;
        try_push_usize_field(
            &mut fields,
            "solution_portfolio_required_pattern_count",
            self.required_patterns.count_ones() as usize,
            projection.required_bytes,
        )?;
        try_push_owned_field(
            &mut fields,
            "solution_portfolio_snapshot_id",
            self.portfolio_snapshot.snapshot_id(),
            projection.required_bytes,
        )?;
        try_push_bool_field(
            &mut fields,
            "solution_portfolio_snapshot_complete",
            self.portfolio_snapshot.complete(),
            projection.required_bytes,
        )?;
        try_push_owned_field(
            &mut fields,
            "solution_portfolio_cursor_semantics",
            "snapshot-id+offset",
            projection.required_bytes,
        )?;
        try_push_owned_field(
            &mut fields,
            "solution_portfolio_cursor_drift_policy",
            "fail-closed",
            projection.required_bytes,
        )?;
        for stage in &self.stages {
            let prefix = stage.kind().field_prefix();
            try_push_optional_usize_stage_field(
                &mut fields,
                prefix,
                "_input_count",
                stage.input_count(),
                projection.required_bytes,
            )?;
            try_push_optional_usize_stage_field(
                &mut fields,
                prefix,
                "_output_count",
                stage.output_count(),
                projection.required_bytes,
            )?;
            try_push_bool_stage_field(
                &mut fields,
                prefix,
                "_complete",
                stage.complete(),
                projection.required_bytes,
            )?;
            try_push_optional_usize_stage_field(
                &mut fields,
                prefix,
                "_rejection_count",
                stage.rejection_count(),
                projection.required_bytes,
            )?;
            try_push_joined_reasons_stage_field(
                &mut fields,
                prefix,
                "_rejection_reasons",
                stage.rejection_reasons(),
                projection.required_bytes,
            )?;
            try_push_owned_stage_field(
                &mut fields,
                prefix,
                "_input_identity_hash",
                stage.input_identity_hash().unwrap_or("unknown"),
                projection.required_bytes,
            )?;
            try_push_owned_stage_field(
                &mut fields,
                prefix,
                "_output_identity_hash",
                stage.output_identity_hash().unwrap_or("unknown"),
                projection.required_bytes,
            )?;
        }
        debug_assert_eq!(fields.len(), projection.field_count);
        let actual_bytes = checked_owned_field_storage_bytes(&fields)
            .ok_or(SolutionSetAuditFieldBuildError::ProjectionOverflow)?;
        Ok((fields, actual_bytes))
    }

    /// Removes all representative, class-count, and snapshot authority from a public summary.
    pub fn redacted_summary_fields(&self) -> Vec<(String, String)> {
        Self::try_redacted_summary_fields(
            self.product_family,
            self.selection_policy,
            self.required_patterns.count_ones() as usize,
        )
        .expect("redacted audit summary has a checked address-space projection")
        .0
    }
}

impl SolutionAuditCheckpoint {
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = self
            .identity_hash
            .as_ref()
            .map_or(0, |value| value.capacity() as u128);
        bytes = bytes.checked_add(checked_string_vec_retained_bytes(&self.incomplete_reasons)?)?;
        Some(bytes)
    }

    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        let mut bytes = self
            .identity_hash
            .as_ref()
            .map_or(0, |value| value.len() as u128);
        bytes = bytes.checked_add(checked_string_vec_clone_bytes(&self.incomplete_reasons)?)?;
        Some(bytes)
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()?
            .checked_add(self.checked_clone_nested_bytes()?)
    }
}

impl SolutionSetAuditReport {
    /// Checked owner-local backing for the complete typed audit authority.
    /// Private vectors, nested strings, coverage payloads, and the immutable
    /// portfolio snapshot are all included before the report is published.
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        self.checked_non_pattern_storage_retained_bytes()?
            .checked_add(self.checked_unique_pattern_storage_bytes()?)
    }

    pub(crate) fn checked_non_pattern_storage_retained_bytes(&self) -> Option<u128> {
        let mut bytes = checked_vec_capacity_bytes::<SolutionSetAuditStage>(&self.stages)?;
        for stage in &self.stages {
            bytes = bytes.checked_add(checked_stage_retained_bytes(stage)?)?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes::<EquivalentCoverageClass>(
            &self.coverage_classes,
        )?)?;
        for class in &self.coverage_classes {
            bytes = bytes.checked_add(checked_coverage_class_retained_bytes(class)?)?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes::<SolutionPortfolioFamily>(
            &self.portfolio_families,
        )?)?;
        for family in &self.portfolio_families {
            bytes = bytes.checked_add(checked_portfolio_family_retained_bytes(family)?)?;
        }
        bytes = bytes.checked_add(checked_portfolio_snapshot_retained_bytes(
            &self.portfolio_snapshot,
        )?)?;
        bytes = bytes.checked_add(checked_string_vec_retained_bytes(&self.incomplete_reasons)?)?;
        Some(bytes)
    }

    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        let mut bytes = checked_slice_slot_bytes::<SolutionSetAuditStage>(&self.stages)?;
        for stage in &self.stages {
            bytes = bytes.checked_add(checked_stage_clone_bytes(stage)?)?;
        }
        bytes = bytes.checked_add(checked_slice_slot_bytes::<EquivalentCoverageClass>(
            &self.coverage_classes,
        )?)?;
        for class in &self.coverage_classes {
            bytes = bytes.checked_add(checked_coverage_class_clone_bytes(class)?)?;
        }
        bytes = bytes.checked_add(checked_slice_slot_bytes::<SolutionPortfolioFamily>(
            &self.portfolio_families,
        )?)?;
        for family in &self.portfolio_families {
            bytes = bytes.checked_add(checked_portfolio_family_clone_bytes(family)?)?;
        }
        bytes = bytes.checked_add(checked_portfolio_snapshot_clone_bytes(
            &self.portfolio_snapshot,
        )?)?;
        bytes = bytes.checked_add(checked_string_vec_clone_bytes(&self.incomplete_reasons)?)?;
        Some(bytes)
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()?
            .checked_add(self.checked_clone_nested_bytes()?)
    }

    fn pattern_bitsets(&self) -> impl Iterator<Item = &PatternBitSet> {
        std::iter::once(&self.required_patterns)
            .chain(
                self.coverage_classes
                    .iter()
                    .map(EquivalentCoverageClass::coverage),
            )
            .chain(self.portfolio_families.iter().flat_map(|family| {
                std::iter::once(&family.required_patterns)
                    .chain(std::iter::once(&family.covered_patterns))
                    .chain(
                        family
                            .coverage_classes
                            .iter()
                            .map(EquivalentCoverageClass::coverage),
                    )
            }))
    }

    pub(crate) fn pattern_storage_components(
        &self,
    ) -> impl Iterator<Item = clearra_coverage::pattern::pattern_bitset::PatternBitSetStorageComponent>
           + '_ {
        self.pattern_bitsets().flat_map(|bitset| {
            (0..bitset.storage_component_count()).map(move |index| {
                bitset
                    .storage_component(index)
                    .expect("component index is bounded by the owner count")
            })
        })
    }

    fn checked_unique_pattern_storage_bytes(&self) -> Option<u128> {
        self.checked_unique_pattern_storage_bytes_with_limit(
            MAX_EXACT_PATTERN_STORAGE_DEDUP_COMPARISONS,
        )
    }

    fn checked_unique_pattern_storage_bytes_with_limit(
        &self,
        max_comparisons: u128,
    ) -> Option<u128> {
        let mut bytes = 0_u128;
        let mut comparisons = 0_u128;
        for (index, current) in self.pattern_storage_components().enumerate() {
            let mut seen = false;
            for prior in self.pattern_storage_components().take(index) {
                comparisons = comparisons.checked_add(1)?;
                if comparisons > max_comparisons {
                    return None;
                }
                if current == prior {
                    seen = true;
                    break;
                }
            }
            if !seen {
                bytes = bytes.checked_add(current.retained_bytes())?;
            }
        }
        Some(bytes)
    }
}

fn checked_vec_capacity_bytes<T>(values: &Vec<T>) -> Option<u128> {
    checked_count_bytes(values.capacity() as u128, core::mem::size_of::<T>() as u128)
}

const fn decimal_len(mut value: usize) -> usize {
    let mut len = 1;
    while value >= 10 {
        value /= 10;
        len += 1;
    }
    len
}

const fn bool_len(value: bool) -> usize {
    if value {
        "true".len()
    } else {
        "false".len()
    }
}

fn optional_count_len(value: Option<usize>) -> usize {
    value.map_or("unknown".len(), decimal_len)
}

fn joined_reasons_len(reasons: &[String]) -> Option<usize> {
    if reasons.is_empty() {
        return Some("none".len());
    }
    reasons
        .iter()
        .try_fold(reasons.len().checked_sub(1)?, |bytes, reason| {
            bytes.checked_add(reason.len())
        })
}

fn try_owned_string(
    value: &str,
    required_bytes: u128,
) -> Result<String, SolutionSetAuditFieldBuildError> {
    let mut owned = String::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|_| SolutionSetAuditFieldBuildError::AllocationFailed { required_bytes })?;
    owned.push_str(value);
    Ok(owned)
}

fn try_joined_string(
    prefix: &str,
    suffix: &str,
    required_bytes: u128,
) -> Result<String, SolutionSetAuditFieldBuildError> {
    let len = prefix
        .len()
        .checked_add(suffix.len())
        .ok_or(SolutionSetAuditFieldBuildError::ProjectionOverflow)?;
    let mut value = String::new();
    value
        .try_reserve_exact(len)
        .map_err(|_| SolutionSetAuditFieldBuildError::AllocationFailed { required_bytes })?;
    value.push_str(prefix);
    value.push_str(suffix);
    Ok(value)
}

fn try_usize_string(
    value: usize,
    required_bytes: u128,
) -> Result<String, SolutionSetAuditFieldBuildError> {
    let mut rendered = String::new();
    rendered
        .try_reserve_exact(decimal_len(value))
        .map_err(|_| SolutionSetAuditFieldBuildError::AllocationFailed { required_bytes })?;
    write!(&mut rendered, "{value}")
        .map_err(|_| SolutionSetAuditFieldBuildError::AllocationFailed { required_bytes })?;
    Ok(rendered)
}

fn try_push_owned_field(
    fields: &mut Vec<(String, String)>,
    key: &str,
    value: &str,
    required_bytes: u128,
) -> Result<(), SolutionSetAuditFieldBuildError> {
    fields.push((
        try_owned_string(key, required_bytes)?,
        try_owned_string(value, required_bytes)?,
    ));
    Ok(())
}

fn try_push_usize_field(
    fields: &mut Vec<(String, String)>,
    key: &str,
    value: usize,
    required_bytes: u128,
) -> Result<(), SolutionSetAuditFieldBuildError> {
    fields.push((
        try_owned_string(key, required_bytes)?,
        try_usize_string(value, required_bytes)?,
    ));
    Ok(())
}

fn try_push_bool_field(
    fields: &mut Vec<(String, String)>,
    key: &str,
    value: bool,
    required_bytes: u128,
) -> Result<(), SolutionSetAuditFieldBuildError> {
    try_push_owned_field(
        fields,
        key,
        if value { "true" } else { "false" },
        required_bytes,
    )
}

fn try_joined_reasons_string(
    reasons: &[String],
    required_bytes: u128,
) -> Result<String, SolutionSetAuditFieldBuildError> {
    let len =
        joined_reasons_len(reasons).ok_or(SolutionSetAuditFieldBuildError::ProjectionOverflow)?;
    let mut joined = String::new();
    joined
        .try_reserve_exact(len)
        .map_err(|_| SolutionSetAuditFieldBuildError::AllocationFailed { required_bytes })?;
    if reasons.is_empty() {
        joined.push_str("none");
    } else {
        for (index, reason) in reasons.iter().enumerate() {
            if index != 0 {
                joined.push('|');
            }
            joined.push_str(reason);
        }
    }
    Ok(joined)
}

fn try_push_joined_reasons_field(
    fields: &mut Vec<(String, String)>,
    key: &str,
    reasons: &[String],
    required_bytes: u128,
) -> Result<(), SolutionSetAuditFieldBuildError> {
    fields.push((
        try_owned_string(key, required_bytes)?,
        try_joined_reasons_string(reasons, required_bytes)?,
    ));
    Ok(())
}

fn try_push_owned_stage_field(
    fields: &mut Vec<(String, String)>,
    prefix: &str,
    suffix: &str,
    value: &str,
    required_bytes: u128,
) -> Result<(), SolutionSetAuditFieldBuildError> {
    fields.push((
        try_joined_string(prefix, suffix, required_bytes)?,
        try_owned_string(value, required_bytes)?,
    ));
    Ok(())
}

fn try_push_optional_usize_stage_field(
    fields: &mut Vec<(String, String)>,
    prefix: &str,
    suffix: &str,
    value: Option<usize>,
    required_bytes: u128,
) -> Result<(), SolutionSetAuditFieldBuildError> {
    let value = match value {
        Some(value) => try_usize_string(value, required_bytes)?,
        None => try_owned_string("unknown", required_bytes)?,
    };
    fields.push((try_joined_string(prefix, suffix, required_bytes)?, value));
    Ok(())
}

fn try_push_bool_stage_field(
    fields: &mut Vec<(String, String)>,
    prefix: &str,
    suffix: &str,
    value: bool,
    required_bytes: u128,
) -> Result<(), SolutionSetAuditFieldBuildError> {
    try_push_owned_stage_field(
        fields,
        prefix,
        suffix,
        if value { "true" } else { "false" },
        required_bytes,
    )
}

fn try_push_joined_reasons_stage_field(
    fields: &mut Vec<(String, String)>,
    prefix: &str,
    suffix: &str,
    reasons: &[String],
    required_bytes: u128,
) -> Result<(), SolutionSetAuditFieldBuildError> {
    fields.push((
        try_joined_string(prefix, suffix, required_bytes)?,
        try_joined_reasons_string(reasons, required_bytes)?,
    ));
    Ok(())
}

fn checked_owned_field_storage_bytes(fields: &Vec<(String, String)>) -> Option<u128> {
    let mut bytes =
        (fields.capacity() as u128).checked_mul(core::mem::size_of::<(String, String)>() as u128)?;
    for (key, value) in fields {
        bytes = bytes
            .checked_add(key.capacity() as u128)?
            .checked_add(value.capacity() as u128)?;
    }
    Some(bytes)
}

fn checked_slice_slot_bytes<T>(values: &[T]) -> Option<u128> {
    checked_count_bytes(values.len() as u128, core::mem::size_of::<T>() as u128)
}

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
}

fn checked_checkpoint_reason_owner_peak_bytes(input: &SolutionSetAuditInput) -> Option<u128> {
    let formatted_prefix_bytes = (SolutionSetAuditStageKind::ALL
        .iter()
        .map(|kind| kind.as_str().len())
        .max()
        .unwrap_or(0) as u128)
        .checked_add(1)?;
    [
        // Produced is cloned twice into its source transition and once into
        // the execution-validation transition. Dedup leaves two simultaneous
        // payload owners, both of which are also formatted in the aggregate.
        (&input.produced.incomplete_reasons, 3_u128, 2_u128, 2_u128),
        (&input.execution_validated.incomplete_reasons, 2, 2, 2),
        (&input.spin_b2b_filtered.incomplete_reasons, 2, 2, 2),
        // Normalization reasons are first cloned into a checkpoint, then into
        // the normalized and classed stages, before both formatted owners are
        // collected.
        (&input.normalized_incomplete_reasons, 3, 3, 2),
    ]
    .into_iter()
    .try_fold(
        0_u128,
        |bytes, (reasons, slot_owners, clone_payload_owners, format_owners)| {
            bytes.checked_add(checked_checkpoint_reason_source_peak_bytes(
                reasons,
                slot_owners,
                clone_payload_owners,
                format_owners,
                formatted_prefix_bytes,
            )?)
        },
    )
}

fn checked_checkpoint_reason_source_peak_bytes(
    reasons: &Vec<String>,
    slot_owners: u128,
    clone_payload_owners: u128,
    format_owners: u128,
    formatted_prefix_bytes: u128,
) -> Option<u128> {
    let slot_bytes = (reasons.capacity() as u128)
        .checked_mul(core::mem::size_of::<String>() as u128)?
        .checked_mul(slot_owners.checked_add(format_owners)?)?;
    let source_payload_bytes = reasons.iter().try_fold(0_u128, |bytes, reason| {
        bytes.checked_add(reason.capacity() as u128)
    })?;
    let clone_payload_bytes = source_payload_bytes.checked_mul(clone_payload_owners)?;
    let formatted_payload_bytes = reasons.iter().try_fold(0_u128, |bytes, reason| {
        bytes.checked_add(
            (reason.capacity() as u128)
                .checked_add(formatted_prefix_bytes)?
                .checked_mul(format_owners)?,
        )
    })?;
    slot_bytes
        .checked_add(clone_payload_bytes)?
        .checked_add(formatted_payload_bytes)
}

fn checked_checkpoint_identity_clone_bytes(input: &SolutionSetAuditInput) -> Option<u128> {
    let identity_bytes = |checkpoint: &SolutionAuditCheckpoint| {
        checkpoint
            .identity_hash
            .as_ref()
            .map_or(0_u128, |identity| identity.len() as u128)
    };
    identity_bytes(&input.produced)
        .checked_mul(3)?
        .checked_add(identity_bytes(&input.execution_validated).checked_mul(2)?)?
        .checked_add(identity_bytes(&input.spin_b2b_filtered).checked_mul(2)?)
}

fn checked_string_vec_retained_bytes(values: &Vec<String>) -> Option<u128> {
    let mut bytes = checked_vec_capacity_bytes::<String>(values)?;
    for value in values {
        bytes = bytes.checked_add(value.capacity() as u128)?;
    }
    Some(bytes)
}

fn checked_string_vec_clone_bytes(values: &[String]) -> Option<u128> {
    let mut bytes = checked_slice_slot_bytes::<String>(values)?;
    for value in values {
        bytes = bytes.checked_add(value.len() as u128)?;
    }
    Some(bytes)
}

fn checked_dimensions_retained_bytes(dimensions: &SolutionSemanticDimensions) -> Option<u128> {
    (dimensions.objective.capacity() as u128)
        .checked_add(dimensions.score_profile.capacity() as u128)?
        .checked_add(dimensions.spin_profile.capacity() as u128)?
        .checked_add(dimensions.b2b_policy.capacity() as u128)
}

fn checked_dimensions_clone_bytes(dimensions: &SolutionSemanticDimensions) -> Option<u128> {
    (dimensions.objective.len() as u128)
        .checked_add(dimensions.score_profile.len() as u128)?
        .checked_add(dimensions.spin_profile.len() as u128)?
        .checked_add(dimensions.b2b_policy.len() as u128)
}

fn checked_stage_retained_bytes(stage: &SolutionSetAuditStage) -> Option<u128> {
    let mut bytes = checked_string_vec_retained_bytes(&stage.rejection_reasons)?;
    bytes = bytes.checked_add(
        stage
            .input_identity_hash
            .as_ref()
            .map_or(0, |value| value.capacity() as u128),
    )?;
    bytes = bytes.checked_add(
        stage
            .output_identity_hash
            .as_ref()
            .map_or(0, |value| value.capacity() as u128),
    )?;
    Some(bytes)
}

fn checked_stage_clone_bytes(stage: &SolutionSetAuditStage) -> Option<u128> {
    let mut bytes = checked_string_vec_clone_bytes(&stage.rejection_reasons)?;
    bytes = bytes.checked_add(
        stage
            .input_identity_hash
            .as_ref()
            .map_or(0, |value| value.len() as u128),
    )?;
    bytes = bytes.checked_add(
        stage
            .output_identity_hash
            .as_ref()
            .map_or(0, |value| value.len() as u128),
    )?;
    Some(bytes)
}

fn checked_coverage_class_retained_bytes(class: &EquivalentCoverageClass) -> Option<u128> {
    let mut bytes = class.class_id.capacity() as u128;
    bytes = bytes.checked_add(checked_dimensions_retained_bytes(&class.dimensions)?)?;
    bytes = bytes.checked_add(checked_string_vec_retained_bytes(&class.member_keys)?)?;
    bytes = bytes.checked_add(class.representative_key.capacity() as u128)?;
    Some(bytes)
}

fn checked_coverage_class_clone_bytes(class: &EquivalentCoverageClass) -> Option<u128> {
    let mut bytes = class.class_id.len() as u128;
    bytes = bytes.checked_add(checked_dimensions_clone_bytes(&class.dimensions)?)?;
    bytes = bytes.checked_add(checked_string_vec_clone_bytes(&class.member_keys)?)?;
    bytes = bytes.checked_add(class.representative_key.len() as u128)?;
    Some(bytes)
}

fn checked_portfolio_family_retained_bytes(family: &SolutionPortfolioFamily) -> Option<u128> {
    let mut bytes = family.family_id.capacity() as u128;
    bytes = bytes.checked_add(checked_dimensions_retained_bytes(&family.dimensions)?)?;
    bytes = bytes.checked_add(checked_vec_capacity_bytes::<EquivalentCoverageClass>(
        &family.coverage_classes,
    )?)?;
    for class in &family.coverage_classes {
        bytes = bytes.checked_add(checked_coverage_class_retained_bytes(class)?)?;
    }
    bytes = bytes.checked_add(checked_string_vec_retained_bytes(
        &family.selected_class_ids,
    )?)?;
    bytes = bytes.checked_add(checked_string_vec_retained_bytes(
        &family.representative_keys,
    )?)?;
    bytes = bytes.checked_add(checked_string_vec_retained_bytes(
        &family.incomplete_reasons,
    )?)?;
    Some(bytes)
}

fn checked_portfolio_family_clone_bytes(family: &SolutionPortfolioFamily) -> Option<u128> {
    let mut bytes = family.family_id.len() as u128;
    bytes = bytes.checked_add(checked_dimensions_clone_bytes(&family.dimensions)?)?;
    bytes = bytes.checked_add(checked_slice_slot_bytes::<EquivalentCoverageClass>(
        &family.coverage_classes,
    )?)?;
    for class in &family.coverage_classes {
        bytes = bytes.checked_add(checked_coverage_class_clone_bytes(class)?)?;
    }
    bytes = bytes.checked_add(checked_string_vec_clone_bytes(&family.selected_class_ids)?)?;
    bytes = bytes.checked_add(checked_string_vec_clone_bytes(&family.representative_keys)?)?;
    bytes = bytes.checked_add(checked_string_vec_clone_bytes(&family.incomplete_reasons)?)?;
    Some(bytes)
}

fn checked_portfolio_entry_retained_bytes(entry: &SolutionPortfolioPageEntry) -> Option<u128> {
    (entry.family_id.capacity() as u128)
        .checked_add(entry.class_id.capacity() as u128)?
        .checked_add(entry.representative_key.capacity() as u128)
}

fn checked_portfolio_entry_clone_bytes(entry: &SolutionPortfolioPageEntry) -> Option<u128> {
    (entry.family_id.len() as u128)
        .checked_add(entry.class_id.len() as u128)?
        .checked_add(entry.representative_key.len() as u128)
}

fn checked_portfolio_snapshot_retained_bytes(snapshot: &SolutionPortfolioSnapshot) -> Option<u128> {
    let mut bytes = snapshot.snapshot_id.capacity() as u128;
    bytes = bytes.checked_add(checked_vec_capacity_bytes::<SolutionPortfolioPageEntry>(
        &snapshot.entries,
    )?)?;
    for entry in &snapshot.entries {
        bytes = bytes.checked_add(checked_portfolio_entry_retained_bytes(entry)?)?;
    }
    Some(bytes)
}

fn checked_portfolio_snapshot_clone_bytes(snapshot: &SolutionPortfolioSnapshot) -> Option<u128> {
    let mut bytes = snapshot.snapshot_id.len() as u128;
    bytes = bytes.checked_add(checked_slice_slot_bytes::<SolutionPortfolioPageEntry>(
        &snapshot.entries,
    )?)?;
    for entry in &snapshot.entries {
        bytes = bytes.checked_add(checked_portfolio_entry_clone_bytes(entry)?)?;
    }
    Some(bytes)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SolutionSetAuditError {
    EmptyCanonicalKey,
    ProductFamilyMismatch {
        key: String,
        expected: SolutionProductFamily,
        actual: SolutionProductFamily,
    },
    PatternCountMismatch {
        key: String,
        expected: usize,
        actual: usize,
    },
    OrphanCoverageEvidence {
        key: String,
    },
    ConflictingCoverageEvidence {
        key: String,
    },
    CoverageMatrixInvalid,
    ExactMinimumCoverInvalid,
    RequiredCoverageLost,
}

impl SolutionSetAuditError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EmptyCanonicalKey => "solution_set_audit_empty_canonical_key",
            Self::ProductFamilyMismatch { .. } => "solution_set_audit_product_family_mismatch",
            Self::PatternCountMismatch { .. } => "solution_set_audit_pattern_count_mismatch",
            Self::OrphanCoverageEvidence { .. } => "solution_set_audit_orphan_coverage_evidence",
            Self::ConflictingCoverageEvidence { .. } => {
                "solution_set_audit_conflicting_coverage_evidence"
            }
            Self::CoverageMatrixInvalid => "solution_set_audit_coverage_matrix_invalid",
            Self::ExactMinimumCoverInvalid => "solution_set_audit_exact_minimum_cover_invalid",
            Self::RequiredCoverageLost => "solution_set_audit_required_coverage_lost",
        }
    }
}

fn build_portfolio_families(
    coverage_classes: &[EquivalentCoverageClass],
    required_patterns: &PatternBitSet,
    selection_policy: SolutionPortfolioSelectionPolicy,
    classed_complete: bool,
    exact_solver_memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    exact_solver_error: &mut Option<ExactMinimumCoverError>,
) -> Result<(Vec<SolutionPortfolioFamily>, Vec<String>), SolutionSetAuditError> {
    let mut by_dimensions = coverage_classes.to_vec();
    by_dimensions.sort_unstable_by(|left, right| {
        left.dimensions()
            .cmp(right.dimensions())
            .then_with(|| left.coverage().words().cmp(right.coverage().words()))
            .then_with(|| left.representative_key().cmp(right.representative_key()))
    });
    let mut families = Vec::with_capacity(by_dimensions.len());
    let mut aggregate_reasons = Vec::new();
    let mut family_start = 0_usize;
    while family_start < by_dimensions.len() {
        let dimensions = by_dimensions[family_start].dimensions().clone();
        let mut family_end = family_start + 1;
        while family_end < by_dimensions.len()
            && by_dimensions[family_end].dimensions() == &dimensions
        {
            family_end += 1;
        }
        let mut classes = by_dimensions[family_start..family_end].to_vec();
        classes.sort_unstable_by(|left, right| {
            left.coverage()
                .words()
                .cmp(right.coverage().words())
                .then_with(|| left.representative_key().cmp(right.representative_key()))
        });
        let family_id = hash_semantic_dimensions("spf1", &dimensions);
        let (
            selected_indices,
            covered_patterns,
            required_coverage_complete,
            selection_complete,
            exact_minimum_proven,
        ) = match selection_policy {
            SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives
            | SolutionPortfolioSelectionPolicy::ProductDeferredExactMinimumCover => {
                let matrix =
                    coverage_matrix_for_classes(required_patterns.pattern_count(), &classes)?;
                let indices = (0..classes.len()).collect::<Vec<_>>();
                let covered = matrix
                    .union_rows(&indices)
                    .map_err(|_| SolutionSetAuditError::CoverageMatrixInvalid)?;
                let covers_required = covered
                    .is_superset(required_patterns)
                    .map_err(|_| SolutionSetAuditError::RequiredCoverageLost)?;
                let selection_deferred = selection_policy
                    == SolutionPortfolioSelectionPolicy::ProductDeferredExactMinimumCover;
                (
                    indices,
                    covered,
                    covers_required,
                    covers_required && !selection_deferred,
                    false,
                )
            }
            SolutionPortfolioSelectionPolicy::ExactMinimumCover => {
                let matrix =
                    coverage_matrix_for_classes(required_patterns.pattern_count(), &classes)?;
                let selection = MinimumCoverSolver::solve_exact_canonical_with_memory_guard(
                    &matrix,
                    required_patterns,
                    exact_solver_memory_guard,
                );
                let selection = match selection {
                    Ok(selection) => selection,
                    Err(error) => {
                        if let ExactMinimumCoverPortfolioError::MinimumCover(error) = error {
                            *exact_solver_error = Some(error);
                        }
                        return Err(SolutionSetAuditError::ExactMinimumCoverInvalid);
                    }
                };
                let covers_required = selection
                    .covered_patterns()
                    .is_superset(required_patterns)
                    .map_err(|_| SolutionSetAuditError::RequiredCoverageLost)?;
                if selection.is_complete() && !covers_required {
                    return Err(SolutionSetAuditError::RequiredCoverageLost);
                }
                (
                    selection.row_indices().to_vec(),
                    selection.covered_patterns().clone(),
                    covers_required,
                    selection.is_complete() && covers_required,
                    selection.is_proven_minimum(),
                )
            }
        };
        let complete = classed_complete && selection_complete;
        let mut incomplete_reasons = Vec::new();
        if !classed_complete {
            incomplete_reasons.push("coverage-class-input-incomplete".to_owned());
        }
        if !required_coverage_complete {
            incomplete_reasons.push("required-pattern-cover-incomplete".to_owned());
        }
        if selection_policy == SolutionPortfolioSelectionPolicy::ProductDeferredExactMinimumCover {
            incomplete_reasons
                .push("exact-minimum-cover-selection-deferred-to-product-coordinator".to_owned());
        }
        let selected_classes = selected_indices
            .iter()
            .map(|index| {
                classes
                    .get(*index)
                    .expect("exact solver returns matrix-owned class indices")
            })
            .collect::<Vec<_>>();
        let selected_class_ids = selected_classes
            .iter()
            .map(|class| class.class_id().to_owned())
            .collect::<Vec<_>>();
        let representative_keys = selected_classes
            .iter()
            .map(|class| class.representative_key().to_owned())
            .collect::<Vec<_>>();
        aggregate_reasons.extend(incomplete_reasons.iter().cloned());
        families.push(SolutionPortfolioFamily {
            family_id,
            dimensions,
            coverage_classes: classes,
            selected_class_ids,
            representative_keys,
            required_patterns: required_patterns.clone(),
            covered_patterns,
            complete,
            exact_minimum_proven: complete && exact_minimum_proven,
            incomplete_reasons,
        });
        family_start = family_end;
    }
    aggregate_reasons.sort_unstable();
    aggregate_reasons.dedup();
    Ok((families, aggregate_reasons))
}

fn coverage_matrix_for_classes(
    pattern_count: usize,
    classes: &[EquivalentCoverageClass],
) -> Result<CoverageMatrix, SolutionSetAuditError> {
    CoverageMatrix::from_rows(
        pattern_count,
        classes
            .iter()
            .enumerate()
            .map(|(index, class)| MatrixCoverageRow::new(index, class.coverage().clone()))
            .collect(),
    )
    .map_err(|_| SolutionSetAuditError::CoverageMatrixInvalid)
}

fn canonical_dimension(value: String) -> String {
    let value = value.trim().to_ascii_lowercase().replace('_', "-");
    if value.is_empty() {
        "unknown".to_owned()
    } else {
        value
    }
}

fn hash_coverage_class(key: &CoverageClassKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra-solution-coverage-class-v1\0");
    update_dimensions(&mut hasher, &key.dimensions);
    update_usize(&mut hasher, key.pattern_count);
    for word in &key.words {
        hasher.update(word.to_be_bytes());
    }
    format!("scc1:{}", lower_hex(&hasher.finalize()))
}

fn hash_coverage_classes(classes: &[EquivalentCoverageClass]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra-solution-coverage-class-set-v1\0");
    for class in classes {
        update_string(&mut hasher, class.class_id());
        for member in class.member_keys() {
            update_string(&mut hasher, member);
        }
    }
    format!("sccs1:{}", lower_hex(&hasher.finalize()))
}

fn hash_semantic_dimensions(prefix: &str, dimensions: &SolutionSemanticDimensions) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra-solution-portfolio-family-v1\0");
    update_dimensions(&mut hasher, dimensions);
    format!("{prefix}:{}", lower_hex(&hasher.finalize()))
}

fn hash_portfolio_snapshot(
    entries: &[SolutionPortfolioPageEntry],
    complete: bool,
    selection_policy: SolutionPortfolioSelectionPolicy,
    coverage_class_set_identity: &str,
    required_patterns: &PatternBitSet,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra-solution-portfolio-snapshot-v1\0");
    update_string(&mut hasher, selection_policy.as_str());
    update_string(&mut hasher, coverage_class_set_identity);
    update_usize(&mut hasher, required_patterns.pattern_count());
    for word in required_patterns.words() {
        hasher.update(word.to_be_bytes());
    }
    hasher.update([u8::from(complete)]);
    for entry in entries {
        update_string(&mut hasher, entry.family_id());
        update_string(&mut hasher, entry.class_id());
        update_string(&mut hasher, entry.representative_key());
    }
    format!("ssp1:{}", lower_hex(&hasher.finalize()))
}

fn update_dimensions(hasher: &mut Sha256, dimensions: &SolutionSemanticDimensions) {
    update_string(hasher, dimensions.product_family().as_str());
    update_string(hasher, dimensions.objective());
    update_string(hasher, dimensions.score_profile());
    update_string(hasher, dimensions.spin_profile());
    update_string(hasher, dimensions.b2b_policy());
}

fn update_string(hasher: &mut Sha256, value: &str) {
    update_usize(hasher, value.len());
    hasher.update(value.as_bytes());
}

fn update_usize(hasher: &mut Sha256, value: usize) {
    hasher.update((value as u128).to_be_bytes());
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0xf)] as char);
    }
    output
}

#[cfg(test)]
mod memory_projection_tests {
    use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

    use super::{
        checked_count_bytes, EquivalentCoverageClass, SolutionAuditCheckpoint,
        SolutionPortfolioFamily, SolutionPortfolioPageEntry, SolutionPortfolioSelectionPolicy,
        SolutionPortfolioSnapshot, SolutionProductFamily, SolutionSemanticDimensions,
        SolutionSetAuditInput, SolutionSetAuditMemoryGuardError, SolutionSetAuditReport,
        SolutionSetAuditStage,
    };

    fn reserved(value: &str, capacity: usize) -> String {
        let mut result = String::with_capacity(capacity);
        result.push_str(value);
        result
    }

    #[test]
    fn checkpoint_projection_uses_string_and_outer_vector_capacities() {
        let mut reasons = Vec::with_capacity(7);
        reasons.push(reserved("reason", 31));
        let checkpoint = SolutionAuditCheckpoint {
            count: Some(1),
            complete: true,
            identity_hash: Some(reserved("hash", 37)),
            incomplete_reasons: reasons,
        };
        let retained = checkpoint.identity_hash.as_ref().unwrap().capacity()
            + checkpoint.incomplete_reasons.capacity() * core::mem::size_of::<String>()
            + checkpoint.incomplete_reasons[0].capacity();
        let clone = checkpoint.identity_hash.as_ref().unwrap().len()
            + core::mem::size_of::<String>()
            + checkpoint.incomplete_reasons[0].len();
        assert_eq!(
            checkpoint.checked_nested_retained_bytes(),
            Some(retained as u128)
        );
        assert_eq!(checkpoint.checked_clone_nested_bytes(), Some(clone as u128));
        assert_eq!(
            checkpoint.checked_clone_peak_bytes(),
            Some((retained + clone) as u128)
        );
    }

    fn checkpoint_heavy_input() -> SolutionSetAuditInput {
        fn reasons(prefix: &str, count: usize, outer_capacity: usize) -> Vec<String> {
            let mut reasons = Vec::with_capacity(outer_capacity);
            for index in 0..count {
                let reason = format!("{prefix}-{index}");
                reasons.push(reserved(&reason, 193 + index));
            }
            reasons
        }

        fn checkpoint(prefix: &str, hash_payload_len: usize) -> SolutionAuditCheckpoint {
            let identity = format!("{prefix}:{}", "x".repeat(hash_payload_len));
            SolutionAuditCheckpoint {
                count: Some(0),
                complete: false,
                identity_hash: Some(reserved(&identity, identity.len() + 127)),
                incomplete_reasons: reasons(prefix, 6, 17),
            }
        }

        let mut input = SolutionSetAuditInput::new(
            SolutionProductFamily::BuildProbability,
            PatternBitSet::new(0),
            SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives,
        );
        input.produced = checkpoint("produced", 521);
        input.execution_validated = checkpoint("execution", 389);
        input.spin_b2b_filtered = checkpoint("spin", 277);
        input.normalized_incomplete_reasons = reasons("normalized", 7, 19);
        input
    }

    #[test]
    fn checkpoint_clone_and_format_projection_accepts_exact_peak_and_rejects_peak_minus_one() {
        let input = checkpoint_heavy_input();
        let expected_projection =
            SolutionSetAuditReport::checked_analysis_memory_projection(&input)
                .expect("checkpoint-heavy projection");
        let mut observed_peak = 0_u128;
        let (_, projection) =
            SolutionSetAuditReport::analyze_with_memory_guard(input, &mut |owned_bytes| {
                observed_peak = observed_peak.max(owned_bytes);
                Ok::<(), ()>(())
            })
            .expect("checkpoint-heavy dry audit");
        assert_eq!(projection, expected_projection);
        assert_eq!(observed_peak, projection.required_peak_bytes);

        let exact_input = checkpoint_heavy_input();
        let input_bytes = exact_input
            .checked_nested_retained_bytes()
            .expect("checkpoint-heavy input bytes");
        let exact_cap = input_bytes
            .checked_add(observed_peak)
            .expect("checkpoint-heavy exact cap");
        SolutionSetAuditReport::analyze_with_memory_limit(exact_input, input_bytes, exact_cap)
            .expect("checkpoint-heavy exact peak");

        let rejected_input = checkpoint_heavy_input();
        let rejected_input_bytes = rejected_input
            .checked_nested_retained_bytes()
            .expect("checkpoint-heavy rejected input bytes");
        assert_eq!(rejected_input_bytes, input_bytes);
        assert!(matches!(
            SolutionSetAuditReport::analyze_with_memory_limit(
                rejected_input,
                rejected_input_bytes,
                exact_cap - 1,
            ),
            Err(SolutionSetAuditMemoryGuardError::MemoryCapacityExceeded { .. })
        ));
    }

    #[test]
    fn report_projection_covers_every_top_level_owner_and_fails_closed_on_overflow() {
        let stages = Vec::with_capacity(3);
        let coverage_classes = Vec::with_capacity(5);
        let portfolio_families = Vec::with_capacity(7);
        let mut entries = Vec::with_capacity(11);
        let entry = SolutionPortfolioPageEntry {
            family_id: reserved("family", 29),
            class_id: reserved("class", 31),
            representative_key: reserved("key", 37),
        };
        entries.push(entry);
        let portfolio_snapshot = SolutionPortfolioSnapshot {
            snapshot_id: reserved("snapshot", 41),
            entries,
            complete: false,
        };
        let mut incomplete_reasons = Vec::with_capacity(13);
        incomplete_reasons.push(reserved("incomplete", 43));
        let report = SolutionSetAuditReport {
            product_family: SolutionProductFamily::BuildProbability,
            selection_policy: SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives,
            stages,
            coverage_classes,
            portfolio_families,
            portfolio_snapshot,
            required_patterns: PatternBitSet::new(65),
            complete: false,
            incomplete_reasons,
        };

        let retained = report.stages.capacity() * core::mem::size_of::<SolutionSetAuditStage>()
            + report.coverage_classes.capacity() * core::mem::size_of::<EquivalentCoverageClass>()
            + report.portfolio_families.capacity()
                * core::mem::size_of::<SolutionPortfolioFamily>()
            + report.portfolio_snapshot.snapshot_id.capacity()
            + report.portfolio_snapshot.entries.capacity()
                * core::mem::size_of::<SolutionPortfolioPageEntry>()
            + report.portfolio_snapshot.entries[0].family_id.capacity()
            + report.portfolio_snapshot.entries[0].class_id.capacity()
            + report.portfolio_snapshot.entries[0]
                .representative_key
                .capacity()
            + report.required_patterns.retained_bytes()
            + report.incomplete_reasons.capacity() * core::mem::size_of::<String>()
            + report.incomplete_reasons[0].capacity();
        assert_eq!(
            report.checked_nested_retained_bytes(),
            Some(retained as u128)
        );
        let clone = report.portfolio_snapshot.snapshot_id.len()
            + core::mem::size_of::<SolutionPortfolioPageEntry>()
            + report.portfolio_snapshot.entries[0].family_id.len()
            + report.portfolio_snapshot.entries[0].class_id.len()
            + report.portfolio_snapshot.entries[0]
                .representative_key
                .len()
            + core::mem::size_of::<String>()
            + report.incomplete_reasons[0].len();
        assert_eq!(report.checked_clone_nested_bytes(), Some(clone as u128));
        assert_eq!(
            report.checked_clone_peak_bytes(),
            Some((retained + clone) as u128)
        );
        assert_eq!(checked_count_bytes(u128::MAX, 2), None);
    }

    #[test]
    fn report_counts_pointer_identical_pattern_storage_once_across_families() {
        let shared = PatternBitSet::from_pattern_indices(128, vec![5])
            .expect("one pattern uses sparse storage");
        let family = SolutionPortfolioFamily {
            family_id: "family".to_owned(),
            dimensions: SolutionSemanticDimensions::new(
                SolutionProductFamily::BuildProbability,
                "objective",
                "score",
                "spin",
                "b2b",
            ),
            coverage_classes: Vec::new(),
            selected_class_ids: Vec::new(),
            representative_keys: Vec::new(),
            required_patterns: shared.clone(),
            covered_patterns: shared.clone(),
            complete: true,
            exact_minimum_proven: false,
            incomplete_reasons: Vec::new(),
        };
        let mut report = SolutionSetAuditReport::unavailable(
            SolutionProductFamily::BuildProbability,
            "test-unavailable",
        );
        report.required_patterns = shared.clone();
        report.coverage_classes.clear();
        report.portfolio_families = vec![family];

        assert_eq!(
            report.checked_unique_pattern_storage_bytes(),
            shared.checked_storage_retained_bytes()
        );
        assert_eq!(
            report.checked_unique_pattern_storage_bytes_with_limit(0),
            None
        );
    }
}

#[cfg(test)]
#[path = "solution_set_audit_tests.rs"]
mod tests;
