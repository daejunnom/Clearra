// SRP rationale: this module has one behavior-level change reason: defining and validating distinct typed Build v2 target-search and supplied-solution boundaries.

//! Build-owned typed product boundary for the v2 Build family.
//!
//! Target search and supplied-solution evaluation deliberately have separate
//! query snapshots, reported result identities, validators, and validated
//! result authorities. There is no shared open-string Build request type and
//! no conversion between the two input authorities.

use clearra_problem::BuildProbabilityQuery;
use clearra_supply::QueueObservationPolicy;

use super::build_v2_options::{
    BuildExecutionSemantics, BuildQueueKnowledge, BuildScoreProfile, BuildV2Capability,
    BuildV2OptionError, BuildV2OptionRequest, ValidatedBuildV2Options,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuildTargetSearchContract {
    Cover,
    Setup,
    Congruent,
    CongruentCover,
    SetupCover,
    SetupCoverPercent,
    SetupCoverScore,
}

impl BuildTargetSearchContract {
    pub(crate) const fn capability_id(self) -> &'static str {
        match self {
            Self::Cover => "build.cover",
            Self::Setup => "build.setup",
            Self::Congruent => "build.congruent",
            Self::CongruentCover => "build.congruent-cover",
            Self::SetupCover => "build.setup-cover",
            Self::SetupCoverPercent => "build.setup-cover-percent",
            Self::SetupCoverScore => "build.setup-cover-score",
        }
    }

    pub(crate) const fn problem_contract_id(self) -> &'static str {
        match self {
            Self::Cover => "build-base-target-search.v2",
            Self::Setup => "build-colored-target.v2",
            Self::Congruent => "build-colored-congruence.v1",
            Self::CongruentCover => "build-colored-congruence-coverage.v1",
            Self::SetupCover | Self::SetupCoverPercent => "build-setup-cover.v1",
            Self::SetupCoverScore => "build-setup-cover-score.v1",
        }
    }

    pub(crate) const fn input_schema_id(self) -> &'static str {
        match self {
            Self::Cover => "build-base-target.v2",
            Self::Setup
            | Self::Congruent
            | Self::CongruentCover
            | Self::SetupCover
            | Self::SetupCoverPercent => "build-colored-target.v2",
            Self::SetupCoverScore => "build-colored-target-score.v1",
        }
    }

    pub(crate) const fn result_contract_id(self) -> &'static str {
        match self {
            Self::Cover => "build-coverage.v2",
            Self::Setup => "build-target-family.v2",
            Self::Congruent => "build-congruence-family.v1",
            Self::CongruentCover => "build-congruence-coverage.v1",
            Self::SetupCover => "build-setup-cover.v1",
            Self::SetupCoverPercent => "build-setup-cover-probability.v1",
            Self::SetupCoverScore => "build-setup-cover-score.v1",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuildSuppliedSolutionEvaluationContract {
    Cover,
    Minimals,
    Score,
    B2bCover,
    CoverPercent,
}

impl BuildSuppliedSolutionEvaluationContract {
    pub(crate) const fn capability_id(self) -> &'static str {
        match self {
            Self::Cover => "build.evaluate.cover",
            Self::Minimals => "build.evaluate.minimals",
            Self::Score => "build.evaluate.score",
            Self::B2bCover => "build.evaluate.b2b-cover",
            Self::CoverPercent => "build.evaluate.cover-percent",
        }
    }

    pub(crate) const fn problem_contract_id(self) -> &'static str {
        match self {
            Self::Cover | Self::Minimals | Self::CoverPercent => {
                "supplied-solution-build-evaluation.v1"
            }
            Self::Score => "supplied-solution-build-score.v1",
            Self::B2bCover => "supplied-solution-b2b-coverage.v1",
        }
    }

    pub(crate) const fn input_schema_id(self) -> &'static str {
        match self {
            Self::Cover | Self::Minimals | Self::B2bCover | Self::CoverPercent => {
                "build-solution-document.v1"
            }
            Self::Score => "build-solution-score-document.v1",
        }
    }

    pub(crate) const fn result_contract_id(self) -> &'static str {
        match self {
            Self::Cover => "build-supplied-coverage.v1",
            Self::Minimals => "build-supplied-minimum-cover.v1",
            Self::Score => "build-supplied-score.v1",
            Self::B2bCover => "build-supplied-b2b-coverage.v1",
            Self::CoverPercent => "build-supplied-probability.v1",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BuildDocumentSnapshotError {
    EmptyDocument,
    EmptyNormalizedEntrySet,
    NormalizedEntryCountExceedsPageCount,
    EmptyDocumentHash,
    NonCanonicalDocumentHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BuildDocumentFingerprint {
    initial_board_mask: u64,
    visible_height: u8,
    page_count: usize,
    normalized_entry_count: usize,
    operation_replay_available: bool,
    document_hash: String,
}

impl BuildDocumentFingerprint {
    fn new(
        initial_board_mask: u64,
        visible_height: u8,
        page_count: usize,
        normalized_entry_count: usize,
        operation_replay_available: bool,
        document_hash: impl Into<String>,
    ) -> Result<Self, BuildDocumentSnapshotError> {
        if page_count == 0 {
            return Err(BuildDocumentSnapshotError::EmptyDocument);
        }
        if normalized_entry_count == 0 {
            return Err(BuildDocumentSnapshotError::EmptyNormalizedEntrySet);
        }
        if normalized_entry_count > page_count {
            return Err(BuildDocumentSnapshotError::NormalizedEntryCountExceedsPageCount);
        }
        let document_hash = document_hash.into();
        if document_hash.is_empty() {
            return Err(BuildDocumentSnapshotError::EmptyDocumentHash);
        }
        if document_hash.trim() != document_hash {
            return Err(BuildDocumentSnapshotError::NonCanonicalDocumentHash);
        }
        Ok(Self {
            initial_board_mask,
            visible_height,
            page_count,
            normalized_entry_count,
            operation_replay_available,
            document_hash,
        })
    }

    const fn initial_board_mask(&self) -> u64 {
        self.initial_board_mask
    }

    const fn visible_height(&self) -> u8 {
        self.visible_height
    }

    const fn page_count(&self) -> usize {
        self.page_count
    }

    const fn normalized_entry_count(&self) -> usize {
        self.normalized_entry_count
    }

    const fn operation_replay_available(&self) -> bool {
        self.operation_replay_available
    }

    fn document_hash(&self) -> &str {
        &self.document_hash
    }
}

/// Identity of a normalized colored target owned by target-search input.
///
/// This intentionally has no conversion to
/// [`BuildSuppliedSolutionDocumentSnapshot`]. The same source document must be
/// normalized again by the receiving product family before it can acquire a
/// different authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuildColoredTargetDocumentSnapshot {
    fingerprint: BuildDocumentFingerprint,
}

impl BuildColoredTargetDocumentSnapshot {
    pub(crate) fn new(
        initial_board_mask: u64,
        visible_height: u8,
        page_count: usize,
        normalized_target_count: usize,
        operation_replay_available: bool,
        document_hash: impl Into<String>,
    ) -> Result<Self, BuildDocumentSnapshotError> {
        Ok(Self {
            fingerprint: BuildDocumentFingerprint::new(
                initial_board_mask,
                visible_height,
                page_count,
                normalized_target_count,
                operation_replay_available,
                document_hash,
            )?,
        })
    }

    pub(crate) const fn initial_board_mask(&self) -> u64 {
        self.fingerprint.initial_board_mask()
    }

    pub(crate) const fn visible_height(&self) -> u8 {
        self.fingerprint.visible_height()
    }

    pub(crate) const fn page_count(&self) -> usize {
        self.fingerprint.page_count()
    }

    pub(crate) const fn normalized_target_count(&self) -> usize {
        self.fingerprint.normalized_entry_count()
    }

    pub(crate) const fn operation_replay_available(&self) -> bool {
        self.fingerprint.operation_replay_available()
    }

    pub(crate) fn document_hash(&self) -> &str {
        self.fingerprint.document_hash()
    }

    pub(crate) const fn checked_retained_capacity_bytes(&self) -> u128 {
        self.fingerprint.document_hash.capacity() as u128
    }
}

/// Identity of a normalized solution document owned by supplied evaluation.
///
/// This is a distinct nominal type from a colored target even when both were
/// decoded from the same external syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuildSuppliedSolutionDocumentSnapshot {
    fingerprint: BuildDocumentFingerprint,
}

impl BuildSuppliedSolutionDocumentSnapshot {
    pub(crate) fn new(
        initial_board_mask: u64,
        visible_height: u8,
        page_count: usize,
        normalized_solution_count: usize,
        operation_replay_available: bool,
        document_hash: impl Into<String>,
    ) -> Result<Self, BuildDocumentSnapshotError> {
        Ok(Self {
            fingerprint: BuildDocumentFingerprint::new(
                initial_board_mask,
                visible_height,
                page_count,
                normalized_solution_count,
                operation_replay_available,
                document_hash,
            )?,
        })
    }

    pub(crate) const fn initial_board_mask(&self) -> u64 {
        self.fingerprint.initial_board_mask()
    }

    pub(crate) const fn visible_height(&self) -> u8 {
        self.fingerprint.visible_height()
    }

    pub(crate) const fn page_count(&self) -> usize {
        self.fingerprint.page_count()
    }

    pub(crate) const fn normalized_solution_count(&self) -> usize {
        self.fingerprint.normalized_entry_count()
    }

    pub(crate) const fn operation_replay_available(&self) -> bool {
        self.fingerprint.operation_replay_available()
    }

    pub(crate) fn document_hash(&self) -> &str {
        self.fingerprint.document_hash()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BuildScoreQueryBinding {
    score_profile: BuildScoreProfile,
    initial_b2b: u16,
}

impl BuildScoreQueryBinding {
    const fn new(score_profile: BuildScoreProfile, initial_b2b: u16) -> Self {
        Self {
            score_profile,
            initial_b2b,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildColoredTargetScoreQuerySnapshot {
    query: BuildProbabilityQuery,
    document: BuildColoredTargetDocumentSnapshot,
    score: BuildScoreQueryBinding,
}

impl BuildColoredTargetScoreQuerySnapshot {
    pub(crate) fn new(
        query: BuildProbabilityQuery,
        document: BuildColoredTargetDocumentSnapshot,
        score_profile: BuildScoreProfile,
        initial_b2b: u16,
    ) -> Self {
        Self {
            query,
            document,
            score: BuildScoreQueryBinding::new(score_profile, initial_b2b),
        }
    }

    pub(crate) const fn query(&self) -> &BuildProbabilityQuery {
        &self.query
    }

    pub(crate) const fn document(&self) -> &BuildColoredTargetDocumentSnapshot {
        &self.document
    }

    pub(crate) const fn score_profile(&self) -> BuildScoreProfile {
        self.score.score_profile
    }

    pub(crate) const fn score_profile_id(&self) -> &'static str {
        self.score.score_profile.as_str()
    }

    pub(crate) const fn initial_b2b(&self) -> u16 {
        self.score.initial_b2b
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildColoredTargetQuerySnapshot {
    query: BuildProbabilityQuery,
    document: BuildColoredTargetDocumentSnapshot,
}

impl BuildColoredTargetQuerySnapshot {
    pub(crate) fn new(
        query: BuildProbabilityQuery,
        document: BuildColoredTargetDocumentSnapshot,
    ) -> Self {
        Self { query, document }
    }

    pub(crate) const fn query(&self) -> &BuildProbabilityQuery {
        &self.query
    }

    pub(crate) const fn document(&self) -> &BuildColoredTargetDocumentSnapshot {
        &self.document
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuildSuppliedSolutionScoreQuerySnapshot {
    document: BuildSuppliedSolutionDocumentSnapshot,
    score: BuildScoreQueryBinding,
}

impl BuildSuppliedSolutionScoreQuerySnapshot {
    pub(crate) fn new(
        document: BuildSuppliedSolutionDocumentSnapshot,
        score_profile: BuildScoreProfile,
        initial_b2b: u16,
    ) -> Self {
        Self {
            document,
            score: BuildScoreQueryBinding::new(score_profile, initial_b2b),
        }
    }

    pub(crate) const fn document(&self) -> &BuildSuppliedSolutionDocumentSnapshot {
        &self.document
    }

    pub(crate) const fn score_profile(&self) -> BuildScoreProfile {
        self.score.score_profile
    }

    pub(crate) const fn score_profile_id(&self) -> &'static str {
        self.score.score_profile.as_str()
    }

    pub(crate) const fn initial_b2b(&self) -> u16 {
        self.score.initial_b2b
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuildV2QuerySnapshotError {
    Options(BuildV2OptionError),
    QueueKnowledgeTransportMismatch,
    ExecutionSemanticsTransportMismatch,
    SuppliedSolutionReplayUnavailable,
}

impl From<BuildV2OptionError> for BuildV2QuerySnapshotError {
    fn from(value: BuildV2OptionError) -> Self {
        Self::Options(value)
    }
}

const fn build_queue_knowledge_from_transport(
    policy: QueueObservationPolicy,
) -> BuildQueueKnowledge {
    match policy {
        QueueObservationPolicy::FullQueueOracle => BuildQueueKnowledge::Oracle,
        QueueObservationPolicy::VisibleSeven => BuildQueueKnowledge::VisibleSeven,
    }
}

const fn build_execution_semantics_from_query(
    query: &BuildProbabilityQuery,
) -> BuildExecutionSemantics {
    if query.aggregation().is_tiling_only() {
        BuildExecutionSemantics::TilingOnly
    } else {
        BuildExecutionSemantics::Reachable
    }
}

fn target_option_request(query: &BuildProbabilityQuery) -> BuildV2OptionRequest {
    BuildV2OptionRequest::default()
        .with_queue_knowledge(build_queue_knowledge_from_transport(
            query.queue_observation_policy(),
        ))
        .with_execution_semantics(build_execution_semantics_from_query(query))
}

#[derive(Clone, Debug, PartialEq)]
enum BuildTargetSearchQueryPayload {
    Cover(BuildProbabilityQuery),
    Setup(BuildColoredTargetQuerySnapshot),
    Congruent(BuildColoredTargetQuerySnapshot),
    CongruentCover(BuildColoredTargetQuerySnapshot),
    SetupCover(BuildColoredTargetQuerySnapshot),
    SetupCoverPercent(BuildColoredTargetQuerySnapshot),
    SetupCoverScore(BuildColoredTargetScoreQuerySnapshot),
}

/// Closed target-search query owner. Its contract is derived from the payload
/// variant and cannot be supplied independently as a possibly mismatched ID.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildTargetSearchQuerySnapshot {
    payload: BuildTargetSearchQueryPayload,
    options: ValidatedBuildV2Options,
}

impl BuildTargetSearchQuerySnapshot {
    pub(crate) fn cover(query: BuildProbabilityQuery) -> Result<Self, BuildV2QuerySnapshotError> {
        let request = target_option_request(&query);
        let options =
            request.validate(BuildV2Capability::Target(BuildTargetSearchContract::Cover))?;
        Ok(Self {
            payload: BuildTargetSearchQueryPayload::Cover(query),
            options,
        })
    }

    pub(crate) fn setup(
        query: BuildProbabilityQuery,
        document: BuildColoredTargetDocumentSnapshot,
    ) -> Self {
        let options = target_option_request(&query)
            .validate(BuildV2Capability::Target(BuildTargetSearchContract::Setup))
            .expect("the query-bound default is valid for build.setup");
        Self {
            payload: BuildTargetSearchQueryPayload::Setup(BuildColoredTargetQuerySnapshot::new(
                query, document,
            )),
            options,
        }
    }

    pub(crate) fn congruent(
        query: BuildProbabilityQuery,
        document: BuildColoredTargetDocumentSnapshot,
    ) -> Self {
        let options = target_option_request(&query)
            .validate(BuildV2Capability::Target(
                BuildTargetSearchContract::Congruent,
            ))
            .expect("the query-bound default is valid for build.congruent");
        Self {
            payload: BuildTargetSearchQueryPayload::Congruent(
                BuildColoredTargetQuerySnapshot::new(query, document),
            ),
            options,
        }
    }

    pub(crate) fn congruent_cover(
        query: BuildProbabilityQuery,
        document: BuildColoredTargetDocumentSnapshot,
    ) -> Self {
        let options = target_option_request(&query)
            .validate(BuildV2Capability::Target(
                BuildTargetSearchContract::CongruentCover,
            ))
            .expect("the query-bound default is valid for build.congruent-cover");
        Self {
            payload: BuildTargetSearchQueryPayload::CongruentCover(
                BuildColoredTargetQuerySnapshot::new(query, document),
            ),
            options,
        }
    }

    pub(crate) fn setup_cover(
        query: BuildProbabilityQuery,
        document: BuildColoredTargetDocumentSnapshot,
    ) -> Self {
        let options = target_option_request(&query)
            .validate(BuildV2Capability::Target(
                BuildTargetSearchContract::SetupCover,
            ))
            .expect("the query-bound default is valid for build.setup-cover");
        Self {
            payload: BuildTargetSearchQueryPayload::SetupCover(
                BuildColoredTargetQuerySnapshot::new(query, document),
            ),
            options,
        }
    }

    pub(crate) fn setup_cover_percent(
        query: BuildProbabilityQuery,
        document: BuildColoredTargetDocumentSnapshot,
    ) -> Self {
        let options = target_option_request(&query)
            .validate(BuildV2Capability::Target(
                BuildTargetSearchContract::SetupCoverPercent,
            ))
            .expect("the query-bound default is valid for build.setup-cover-percent");
        Self {
            payload: BuildTargetSearchQueryPayload::SetupCoverPercent(
                BuildColoredTargetQuerySnapshot::new(query, document),
            ),
            options,
        }
    }

    pub(crate) fn setup_cover_score(query: BuildColoredTargetScoreQuerySnapshot) -> Self {
        let options = target_option_request(query.query())
            .with_score_profile(query.score_profile())
            .with_initial_b2b(query.initial_b2b())
            .validate(BuildV2Capability::Target(
                BuildTargetSearchContract::SetupCoverScore,
            ))
            .expect("typed score fields are valid for build.setup-cover-score");
        Self {
            payload: BuildTargetSearchQueryPayload::SetupCoverScore(query),
            options,
        }
    }

    pub(crate) fn with_options(
        mut self,
        request: BuildV2OptionRequest,
    ) -> Result<Self, BuildV2QuerySnapshotError> {
        let capability = BuildV2Capability::Target(self.contract());
        let options = request.validate(capability)?;
        match &mut self.payload {
            BuildTargetSearchQueryPayload::Cover(query) => {
                if request.queue_knowledge()
                    != build_queue_knowledge_from_transport(query.queue_observation_policy())
                {
                    return Err(BuildV2QuerySnapshotError::QueueKnowledgeTransportMismatch);
                }
                if request.execution_semantics() != build_execution_semantics_from_query(query) {
                    return Err(BuildV2QuerySnapshotError::ExecutionSemanticsTransportMismatch);
                }
            }
            BuildTargetSearchQueryPayload::SetupCoverScore(query) => {
                if request.queue_knowledge()
                    != build_queue_knowledge_from_transport(
                        query.query().queue_observation_policy(),
                    )
                {
                    return Err(BuildV2QuerySnapshotError::QueueKnowledgeTransportMismatch);
                }
                if request.execution_semantics()
                    != build_execution_semantics_from_query(query.query())
                {
                    return Err(BuildV2QuerySnapshotError::ExecutionSemanticsTransportMismatch);
                }
                query.score = BuildScoreQueryBinding::new(
                    options
                        .score_profile()
                        .expect("score-capable validation supplies a profile"),
                    options
                        .initial_b2b()
                        .expect("score-capable validation supplies initial B2B"),
                );
            }
            BuildTargetSearchQueryPayload::Setup(query)
            | BuildTargetSearchQueryPayload::Congruent(query)
            | BuildTargetSearchQueryPayload::CongruentCover(query)
            | BuildTargetSearchQueryPayload::SetupCover(query)
            | BuildTargetSearchQueryPayload::SetupCoverPercent(query) => {
                if request.queue_knowledge()
                    != build_queue_knowledge_from_transport(
                        query.query().queue_observation_policy(),
                    )
                {
                    return Err(BuildV2QuerySnapshotError::QueueKnowledgeTransportMismatch);
                }
                if request.execution_semantics()
                    != build_execution_semantics_from_query(query.query())
                {
                    return Err(BuildV2QuerySnapshotError::ExecutionSemanticsTransportMismatch);
                }
            }
        }
        self.options = options;
        Ok(self)
    }

    pub(crate) const fn options(&self) -> ValidatedBuildV2Options {
        self.options
    }

    pub(crate) const fn contract(&self) -> BuildTargetSearchContract {
        match &self.payload {
            BuildTargetSearchQueryPayload::Cover(_) => BuildTargetSearchContract::Cover,
            BuildTargetSearchQueryPayload::Setup(_) => BuildTargetSearchContract::Setup,
            BuildTargetSearchQueryPayload::Congruent(_) => BuildTargetSearchContract::Congruent,
            BuildTargetSearchQueryPayload::CongruentCover(_) => {
                BuildTargetSearchContract::CongruentCover
            }
            BuildTargetSearchQueryPayload::SetupCover(_) => BuildTargetSearchContract::SetupCover,
            BuildTargetSearchQueryPayload::SetupCoverPercent(_) => {
                BuildTargetSearchContract::SetupCoverPercent
            }
            BuildTargetSearchQueryPayload::SetupCoverScore(_) => {
                BuildTargetSearchContract::SetupCoverScore
            }
        }
    }

    pub(crate) fn base_target_query(&self) -> Option<&BuildProbabilityQuery> {
        match &self.payload {
            BuildTargetSearchQueryPayload::Cover(query) => Some(query),
            _ => None,
        }
    }

    pub(crate) const fn target_query(&self) -> &BuildProbabilityQuery {
        match &self.payload {
            BuildTargetSearchQueryPayload::Cover(query) => query,
            BuildTargetSearchQueryPayload::Setup(query)
            | BuildTargetSearchQueryPayload::Congruent(query)
            | BuildTargetSearchQueryPayload::CongruentCover(query)
            | BuildTargetSearchQueryPayload::SetupCover(query)
            | BuildTargetSearchQueryPayload::SetupCoverPercent(query) => query.query(),
            BuildTargetSearchQueryPayload::SetupCoverScore(query) => query.query(),
        }
    }

    pub(crate) fn colored_target_document(&self) -> Option<&BuildColoredTargetDocumentSnapshot> {
        match &self.payload {
            BuildTargetSearchQueryPayload::Setup(query)
            | BuildTargetSearchQueryPayload::Congruent(query)
            | BuildTargetSearchQueryPayload::CongruentCover(query)
            | BuildTargetSearchQueryPayload::SetupCover(query)
            | BuildTargetSearchQueryPayload::SetupCoverPercent(query) => Some(query.document()),
            BuildTargetSearchQueryPayload::Cover(_)
            | BuildTargetSearchQueryPayload::SetupCoverScore(_) => None,
        }
    }

    pub(crate) fn colored_target_score_query(
        &self,
    ) -> Option<&BuildColoredTargetScoreQuerySnapshot> {
        match &self.payload {
            BuildTargetSearchQueryPayload::SetupCoverScore(query) => Some(query),
            _ => None,
        }
    }

    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let query = self.target_query();
        let document_bytes = self
            .colored_target_document()
            .map(BuildColoredTargetDocumentSnapshot::checked_retained_capacity_bytes)
            .or_else(|| {
                self.colored_target_score_query()
                    .map(|query| query.document().checked_retained_capacity_bytes())
            })
            .unwrap_or(0);
        query
            .checked_retained_capacity_bytes()?
            .checked_add(document_bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum BuildSuppliedSolutionEvaluationQueryPayload {
    Cover(BuildSuppliedSolutionDocumentSnapshot),
    Minimals(BuildSuppliedSolutionDocumentSnapshot),
    Score(BuildSuppliedSolutionScoreQuerySnapshot),
    B2bCover(BuildSuppliedSolutionDocumentSnapshot),
    CoverPercent(BuildSuppliedSolutionDocumentSnapshot),
}

/// Closed supplied-solution query owner. It cannot hold a target-search input
/// or a target-search contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuildSuppliedSolutionEvaluationQuerySnapshot {
    payload: BuildSuppliedSolutionEvaluationQueryPayload,
    options: ValidatedBuildV2Options,
}

impl BuildSuppliedSolutionEvaluationQuerySnapshot {
    pub(crate) fn cover(
        document: BuildSuppliedSolutionDocumentSnapshot,
    ) -> Result<Self, BuildV2QuerySnapshotError> {
        Self::from_document(document, BuildSuppliedSolutionEvaluationContract::Cover)
    }

    pub(crate) fn minimals(
        document: BuildSuppliedSolutionDocumentSnapshot,
    ) -> Result<Self, BuildV2QuerySnapshotError> {
        Self::from_document(document, BuildSuppliedSolutionEvaluationContract::Minimals)
    }

    pub(crate) fn score(
        query: BuildSuppliedSolutionScoreQuerySnapshot,
    ) -> Result<Self, BuildV2QuerySnapshotError> {
        if !query.document().operation_replay_available() {
            return Err(BuildV2QuerySnapshotError::SuppliedSolutionReplayUnavailable);
        }
        let options = BuildV2OptionRequest::default()
            .with_score_profile(query.score_profile())
            .with_initial_b2b(query.initial_b2b())
            .validate(BuildV2Capability::Supplied(
                BuildSuppliedSolutionEvaluationContract::Score,
            ))?;
        Ok(Self {
            payload: BuildSuppliedSolutionEvaluationQueryPayload::Score(query),
            options,
        })
    }

    pub(crate) fn b2b_cover(
        document: BuildSuppliedSolutionDocumentSnapshot,
    ) -> Result<Self, BuildV2QuerySnapshotError> {
        Self::from_document(document, BuildSuppliedSolutionEvaluationContract::B2bCover)
    }

    pub(crate) fn cover_percent(
        document: BuildSuppliedSolutionDocumentSnapshot,
    ) -> Result<Self, BuildV2QuerySnapshotError> {
        Self::from_document(
            document,
            BuildSuppliedSolutionEvaluationContract::CoverPercent,
        )
    }

    fn from_document(
        document: BuildSuppliedSolutionDocumentSnapshot,
        contract: BuildSuppliedSolutionEvaluationContract,
    ) -> Result<Self, BuildV2QuerySnapshotError> {
        if !document.operation_replay_available() {
            return Err(BuildV2QuerySnapshotError::SuppliedSolutionReplayUnavailable);
        }
        let payload = match contract {
            BuildSuppliedSolutionEvaluationContract::Cover => {
                BuildSuppliedSolutionEvaluationQueryPayload::Cover(document)
            }
            BuildSuppliedSolutionEvaluationContract::Minimals => {
                BuildSuppliedSolutionEvaluationQueryPayload::Minimals(document)
            }
            BuildSuppliedSolutionEvaluationContract::B2bCover => {
                BuildSuppliedSolutionEvaluationQueryPayload::B2bCover(document)
            }
            BuildSuppliedSolutionEvaluationContract::CoverPercent => {
                BuildSuppliedSolutionEvaluationQueryPayload::CoverPercent(document)
            }
            BuildSuppliedSolutionEvaluationContract::Score => {
                unreachable!("score documents use their typed score constructor")
            }
        };
        let options =
            BuildV2OptionRequest::default().validate(BuildV2Capability::Supplied(contract))?;
        Ok(Self { payload, options })
    }

    pub(crate) fn with_options(
        mut self,
        request: BuildV2OptionRequest,
    ) -> Result<Self, BuildV2QuerySnapshotError> {
        let options = request.validate(BuildV2Capability::Supplied(self.contract()))?;
        if let BuildSuppliedSolutionEvaluationQueryPayload::Score(query) = &mut self.payload {
            query.score = BuildScoreQueryBinding::new(
                options
                    .score_profile()
                    .expect("score-capable validation supplies a profile"),
                options
                    .initial_b2b()
                    .expect("score-capable validation supplies initial B2B"),
            );
        }
        self.options = options;
        Ok(self)
    }

    pub(crate) const fn options(&self) -> ValidatedBuildV2Options {
        self.options
    }

    pub(crate) const fn contract(&self) -> BuildSuppliedSolutionEvaluationContract {
        match &self.payload {
            BuildSuppliedSolutionEvaluationQueryPayload::Cover(_) => {
                BuildSuppliedSolutionEvaluationContract::Cover
            }
            BuildSuppliedSolutionEvaluationQueryPayload::Minimals(_) => {
                BuildSuppliedSolutionEvaluationContract::Minimals
            }
            BuildSuppliedSolutionEvaluationQueryPayload::Score(_) => {
                BuildSuppliedSolutionEvaluationContract::Score
            }
            BuildSuppliedSolutionEvaluationQueryPayload::B2bCover(_) => {
                BuildSuppliedSolutionEvaluationContract::B2bCover
            }
            BuildSuppliedSolutionEvaluationQueryPayload::CoverPercent(_) => {
                BuildSuppliedSolutionEvaluationContract::CoverPercent
            }
        }
    }

    pub(crate) fn document(&self) -> &BuildSuppliedSolutionDocumentSnapshot {
        match &self.payload {
            BuildSuppliedSolutionEvaluationQueryPayload::Cover(document)
            | BuildSuppliedSolutionEvaluationQueryPayload::Minimals(document)
            | BuildSuppliedSolutionEvaluationQueryPayload::B2bCover(document)
            | BuildSuppliedSolutionEvaluationQueryPayload::CoverPercent(document) => document,
            BuildSuppliedSolutionEvaluationQueryPayload::Score(query) => query.document(),
        }
    }

    pub(crate) fn score_query(&self) -> Option<&BuildSuppliedSolutionScoreQuerySnapshot> {
        match &self.payload {
            BuildSuppliedSolutionEvaluationQueryPayload::Score(query) => Some(query),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReportedBuildTargetSearchResultIdentity {
    capability_id: String,
    problem_contract_id: String,
    input_schema_id: String,
    result_contract_id: String,
}

impl ReportedBuildTargetSearchResultIdentity {
    pub(crate) fn new(
        capability_id: impl Into<String>,
        problem_contract_id: impl Into<String>,
        input_schema_id: impl Into<String>,
        result_contract_id: impl Into<String>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            problem_contract_id: problem_contract_id.into(),
            input_schema_id: input_schema_id.into(),
            result_contract_id: result_contract_id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReportedBuildSuppliedSolutionEvaluationResultIdentity {
    capability_id: String,
    problem_contract_id: String,
    input_schema_id: String,
    result_contract_id: String,
}

impl ReportedBuildSuppliedSolutionEvaluationResultIdentity {
    pub(crate) fn new(
        capability_id: impl Into<String>,
        problem_contract_id: impl Into<String>,
        input_schema_id: impl Into<String>,
        result_contract_id: impl Into<String>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            problem_contract_id: problem_contract_id.into(),
            input_schema_id: input_schema_id.into(),
            result_contract_id: result_contract_id.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
// The repeated suffix is part of the published validation-error taxonomy.
#[allow(clippy::enum_variant_names)]
pub(crate) enum BuildTargetSearchResultValidationError {
    CapabilityIdMismatch,
    ProblemContractIdMismatch,
    InputSchemaIdMismatch,
    ResultContractIdMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
// The repeated suffix is part of the published validation-error taxonomy.
#[allow(clippy::enum_variant_names)]
pub(crate) enum BuildSuppliedSolutionEvaluationResultValidationError {
    CapabilityIdMismatch,
    ProblemContractIdMismatch,
    InputSchemaIdMismatch,
    ResultContractIdMismatch,
}

/// Result authority that can only be minted by the target-search validator.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValidatedBuildTargetSearchResultAuthority {
    query: BuildTargetSearchQuerySnapshot,
    identity: ReportedBuildTargetSearchResultIdentity,
}

impl ValidatedBuildTargetSearchResultAuthority {
    pub(crate) fn validate(
        query: BuildTargetSearchQuerySnapshot,
        identity: ReportedBuildTargetSearchResultIdentity,
    ) -> Result<Self, BuildTargetSearchResultValidationError> {
        let contract = query.contract();
        if identity.capability_id != contract.capability_id() {
            return Err(BuildTargetSearchResultValidationError::CapabilityIdMismatch);
        }
        if identity.problem_contract_id != contract.problem_contract_id() {
            return Err(BuildTargetSearchResultValidationError::ProblemContractIdMismatch);
        }
        if identity.input_schema_id != contract.input_schema_id() {
            return Err(BuildTargetSearchResultValidationError::InputSchemaIdMismatch);
        }
        if identity.result_contract_id != contract.result_contract_id() {
            return Err(BuildTargetSearchResultValidationError::ResultContractIdMismatch);
        }
        Ok(Self { query, identity })
    }

    pub(crate) fn query(&self) -> &BuildTargetSearchQuerySnapshot {
        &self.query
    }

    pub(crate) const fn contract(&self) -> BuildTargetSearchContract {
        self.query.contract()
    }

    // Retained so adapters can audit the exact identity that minted this authority.
    #[allow(dead_code)]
    pub(crate) fn identity(&self) -> &ReportedBuildTargetSearchResultIdentity {
        &self.identity
    }

    // Retained for parity checks at alternate product ingress boundaries.
    #[allow(dead_code)]
    pub(crate) fn matches_query(&self, expected: &BuildTargetSearchQuerySnapshot) -> bool {
        self.query == *expected
    }
}

/// Result authority that can only be minted by the supplied-solution
/// evaluation validator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedBuildSuppliedSolutionEvaluationResultAuthority {
    query: BuildSuppliedSolutionEvaluationQuerySnapshot,
    identity: ReportedBuildSuppliedSolutionEvaluationResultIdentity,
}

impl ValidatedBuildSuppliedSolutionEvaluationResultAuthority {
    pub(crate) fn validate(
        query: BuildSuppliedSolutionEvaluationQuerySnapshot,
        identity: ReportedBuildSuppliedSolutionEvaluationResultIdentity,
    ) -> Result<Self, BuildSuppliedSolutionEvaluationResultValidationError> {
        let contract = query.contract();
        if identity.capability_id != contract.capability_id() {
            return Err(BuildSuppliedSolutionEvaluationResultValidationError::CapabilityIdMismatch);
        }
        if identity.problem_contract_id != contract.problem_contract_id() {
            return Err(
                BuildSuppliedSolutionEvaluationResultValidationError::ProblemContractIdMismatch,
            );
        }
        if identity.input_schema_id != contract.input_schema_id() {
            return Err(
                BuildSuppliedSolutionEvaluationResultValidationError::InputSchemaIdMismatch,
            );
        }
        if identity.result_contract_id != contract.result_contract_id() {
            return Err(
                BuildSuppliedSolutionEvaluationResultValidationError::ResultContractIdMismatch,
            );
        }
        Ok(Self { query, identity })
    }

    pub(crate) fn query(&self) -> &BuildSuppliedSolutionEvaluationQuerySnapshot {
        &self.query
    }

    pub(crate) const fn contract(&self) -> BuildSuppliedSolutionEvaluationContract {
        self.query.contract()
    }

    // Retained so adapters can audit the exact identity that minted this authority.
    #[allow(dead_code)]
    pub(crate) fn identity(&self) -> &ReportedBuildSuppliedSolutionEvaluationResultIdentity {
        &self.identity
    }

    // Retained for parity checks at alternate product ingress boundaries.
    #[allow(dead_code)]
    pub(crate) fn matches_query(
        &self,
        expected: &BuildSuppliedSolutionEvaluationQuerySnapshot,
    ) -> bool {
        self.query == *expected
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{
        BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityQuery,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;
    use clearra_supply::QueueObservationPolicy;

    use super::super::build_v2_options::{
        BuildExecutionSemantics, BuildObjective, BuildQueueKnowledge, BuildScoreProfile,
        BuildV2OptionError, BuildV2OptionRequest,
    };

    use super::{
        BuildColoredTargetDocumentSnapshot, BuildColoredTargetScoreQuerySnapshot,
        BuildDocumentSnapshotError, BuildSuppliedSolutionDocumentSnapshot,
        BuildSuppliedSolutionEvaluationContract, BuildSuppliedSolutionEvaluationQuerySnapshot,
        BuildSuppliedSolutionEvaluationResultValidationError,
        BuildSuppliedSolutionScoreQuerySnapshot, BuildTargetSearchContract,
        BuildTargetSearchQuerySnapshot, BuildTargetSearchResultValidationError,
        BuildV2QuerySnapshotError, ReportedBuildSuppliedSolutionEvaluationResultIdentity,
        ReportedBuildTargetSearchResultIdentity,
        ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
        ValidatedBuildTargetSearchResultAuthority,
    };

    fn build_cover_query(target_mask: u64) -> BuildProbabilityQuery {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [target_mask, 0, 0, 0])
                .expect("test target fits the Build field");
        BuildProbabilityQuery::new(core, field)
    }

    fn colored_target(hash: &str) -> BuildColoredTargetDocumentSnapshot {
        BuildColoredTargetDocumentSnapshot::new(0, 4, 2, 2, true, hash)
            .expect("canonical colored target snapshot")
    }

    fn supplied_solutions(hash: &str) -> BuildSuppliedSolutionDocumentSnapshot {
        BuildSuppliedSolutionDocumentSnapshot::new(0, 4, 3, 2, true, hash)
            .expect("canonical supplied-solution snapshot")
    }

    fn target_identity(
        contract: BuildTargetSearchContract,
    ) -> ReportedBuildTargetSearchResultIdentity {
        ReportedBuildTargetSearchResultIdentity::new(
            contract.capability_id(),
            contract.problem_contract_id(),
            contract.input_schema_id(),
            contract.result_contract_id(),
        )
    }

    fn supplied_identity(
        contract: BuildSuppliedSolutionEvaluationContract,
    ) -> ReportedBuildSuppliedSolutionEvaluationResultIdentity {
        ReportedBuildSuppliedSolutionEvaluationResultIdentity::new(
            contract.capability_id(),
            contract.problem_contract_id(),
            contract.input_schema_id(),
            contract.result_contract_id(),
        )
    }

    #[test]
    fn target_search_contract_matrix_is_closed_and_exact() {
        let rows = [
            (
                BuildTargetSearchContract::Cover,
                "build.cover",
                "build-base-target-search.v2",
                "build-base-target.v2",
                "build-coverage.v2",
            ),
            (
                BuildTargetSearchContract::Setup,
                "build.setup",
                "build-colored-target.v2",
                "build-colored-target.v2",
                "build-target-family.v2",
            ),
            (
                BuildTargetSearchContract::Congruent,
                "build.congruent",
                "build-colored-congruence.v1",
                "build-colored-target.v2",
                "build-congruence-family.v1",
            ),
            (
                BuildTargetSearchContract::CongruentCover,
                "build.congruent-cover",
                "build-colored-congruence-coverage.v1",
                "build-colored-target.v2",
                "build-congruence-coverage.v1",
            ),
            (
                BuildTargetSearchContract::SetupCover,
                "build.setup-cover",
                "build-setup-cover.v1",
                "build-colored-target.v2",
                "build-setup-cover.v1",
            ),
            (
                BuildTargetSearchContract::SetupCoverPercent,
                "build.setup-cover-percent",
                "build-setup-cover.v1",
                "build-colored-target.v2",
                "build-setup-cover-probability.v1",
            ),
            (
                BuildTargetSearchContract::SetupCoverScore,
                "build.setup-cover-score",
                "build-setup-cover-score.v1",
                "build-colored-target-score.v1",
                "build-setup-cover-score.v1",
            ),
        ];

        for (contract, capability, problem, input, result) in rows {
            assert_eq!(contract.capability_id(), capability);
            assert_eq!(contract.problem_contract_id(), problem);
            assert_eq!(contract.input_schema_id(), input);
            assert_eq!(contract.result_contract_id(), result);
        }
    }

    #[test]
    fn supplied_solution_contract_matrix_is_closed_and_exact() {
        let rows = [
            (
                BuildSuppliedSolutionEvaluationContract::Cover,
                "build.evaluate.cover",
                "supplied-solution-build-evaluation.v1",
                "build-solution-document.v1",
                "build-supplied-coverage.v1",
            ),
            (
                BuildSuppliedSolutionEvaluationContract::Minimals,
                "build.evaluate.minimals",
                "supplied-solution-build-evaluation.v1",
                "build-solution-document.v1",
                "build-supplied-minimum-cover.v1",
            ),
            (
                BuildSuppliedSolutionEvaluationContract::Score,
                "build.evaluate.score",
                "supplied-solution-build-score.v1",
                "build-solution-score-document.v1",
                "build-supplied-score.v1",
            ),
            (
                BuildSuppliedSolutionEvaluationContract::B2bCover,
                "build.evaluate.b2b-cover",
                "supplied-solution-b2b-coverage.v1",
                "build-solution-document.v1",
                "build-supplied-b2b-coverage.v1",
            ),
            (
                BuildSuppliedSolutionEvaluationContract::CoverPercent,
                "build.evaluate.cover-percent",
                "supplied-solution-build-evaluation.v1",
                "build-solution-document.v1",
                "build-supplied-probability.v1",
            ),
        ];

        for (contract, capability, problem, input, result) in rows {
            assert_eq!(contract.capability_id(), capability);
            assert_eq!(contract.problem_contract_id(), problem);
            assert_eq!(contract.input_schema_id(), input);
            assert_eq!(contract.result_contract_id(), result);
        }
    }

    #[test]
    fn query_contract_is_derived_from_the_nominal_payload_variant() {
        let query = build_cover_query(0xf);
        let target = colored_target("colored:setup");
        let target_score = BuildColoredTargetScoreQuerySnapshot::new(
            query.clone(),
            colored_target("colored:score"),
            BuildScoreProfile::Guideline,
            1,
        );
        let target_queries = [
            BuildTargetSearchQuerySnapshot::setup(query.clone(), target.clone()),
            BuildTargetSearchQuerySnapshot::congruent(query.clone(), target.clone()),
            BuildTargetSearchQuerySnapshot::congruent_cover(query.clone(), target.clone()),
            BuildTargetSearchQuerySnapshot::setup_cover(query.clone(), target.clone()),
            BuildTargetSearchQuerySnapshot::setup_cover_percent(query, target),
            BuildTargetSearchQuerySnapshot::setup_cover_score(target_score),
        ];
        let target_contracts = [
            BuildTargetSearchContract::Setup,
            BuildTargetSearchContract::Congruent,
            BuildTargetSearchContract::CongruentCover,
            BuildTargetSearchContract::SetupCover,
            BuildTargetSearchContract::SetupCoverPercent,
            BuildTargetSearchContract::SetupCoverScore,
        ];
        for (query, expected) in target_queries.iter().zip(target_contracts) {
            assert_eq!(query.contract(), expected);
        }

        let supplied = supplied_solutions("supplied:cover");
        let supplied_score = BuildSuppliedSolutionScoreQuerySnapshot::new(
            supplied_solutions("supplied:score"),
            BuildScoreProfile::Guideline,
            0,
        );
        let supplied_queries = [
            BuildSuppliedSolutionEvaluationQuerySnapshot::cover(supplied.clone())
                .expect("replayable cover query"),
            BuildSuppliedSolutionEvaluationQuerySnapshot::minimals(supplied.clone())
                .expect("replayable minimals query"),
            BuildSuppliedSolutionEvaluationQuerySnapshot::score(supplied_score)
                .expect("replayable score query"),
            BuildSuppliedSolutionEvaluationQuerySnapshot::b2b_cover(supplied.clone())
                .expect("replayable B2B query"),
            BuildSuppliedSolutionEvaluationQuerySnapshot::cover_percent(supplied)
                .expect("replayable percent query"),
        ];
        let supplied_contracts = [
            BuildSuppliedSolutionEvaluationContract::Cover,
            BuildSuppliedSolutionEvaluationContract::Minimals,
            BuildSuppliedSolutionEvaluationContract::Score,
            BuildSuppliedSolutionEvaluationContract::B2bCover,
            BuildSuppliedSolutionEvaluationContract::CoverPercent,
        ];
        for (query, expected) in supplied_queries.iter().zip(supplied_contracts) {
            assert_eq!(query.contract(), expected);
        }
    }

    #[test]
    fn document_inputs_are_distinct_nominal_authorities() {
        assert_ne!(
            TypeId::of::<BuildColoredTargetDocumentSnapshot>(),
            TypeId::of::<BuildSuppliedSolutionDocumentSnapshot>()
        );
        let target = colored_target("same-source-hash");
        let supplied = supplied_solutions("same-source-hash");
        assert_eq!(target.initial_board_mask(), supplied.initial_board_mask());
        assert_eq!(target.visible_height(), supplied.visible_height());
        assert_ne!(target.page_count(), supplied.page_count());
        assert_eq!(target.normalized_target_count(), 2);
        assert_eq!(supplied.normalized_solution_count(), 2);
        assert!(target.operation_replay_available());
        assert!(supplied.operation_replay_available());
        assert_eq!(target.document_hash(), supplied.document_hash());
    }

    #[test]
    fn document_snapshots_fail_closed_on_non_authoritative_counts_and_ids() {
        assert_eq!(
            BuildColoredTargetDocumentSnapshot::new(0, 4, 0, 0, false, "hash"),
            Err(BuildDocumentSnapshotError::EmptyDocument)
        );
        assert_eq!(
            BuildSuppliedSolutionDocumentSnapshot::new(0, 4, 1, 0, false, "hash"),
            Err(BuildDocumentSnapshotError::EmptyNormalizedEntrySet)
        );
        assert_eq!(
            BuildSuppliedSolutionDocumentSnapshot::new(0, 4, 1, 2, false, "hash"),
            Err(BuildDocumentSnapshotError::NormalizedEntryCountExceedsPageCount)
        );
        assert_eq!(
            BuildColoredTargetDocumentSnapshot::new(0, 4, 1, 1, false, ""),
            Err(BuildDocumentSnapshotError::EmptyDocumentHash)
        );
        assert_eq!(
            BuildSuppliedSolutionDocumentSnapshot::new(0, 4, 1, 1, true, " hash"),
            Err(BuildDocumentSnapshotError::NonCanonicalDocumentHash)
        );
    }

    #[test]
    fn cover_snapshot_binds_options_to_the_actual_execution_transport() {
        let visible_query = build_cover_query(0xf)
            .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
        let visible = BuildTargetSearchQuerySnapshot::cover(visible_query)
            .expect("reachable visible-seven Build query");
        assert_eq!(
            visible.options().queue_knowledge(),
            BuildQueueKnowledge::VisibleSeven
        );
        assert_eq!(
            visible
                .clone()
                .with_options(BuildV2OptionRequest::default()),
            Err(BuildV2QuerySnapshotError::QueueKnowledgeTransportMismatch)
        );

        let oracle = BuildTargetSearchQuerySnapshot::cover(build_cover_query(0xf))
            .expect("reachable oracle Build query");
        assert_eq!(
            oracle.with_options(
                BuildV2OptionRequest::default()
                    .with_execution_semantics(BuildExecutionSemantics::TilingOnly)
            ),
            Err(BuildV2QuerySnapshotError::ExecutionSemanticsTransportMismatch)
        );

        let invalid_transport = build_cover_query(0xf)
            .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
            .with_aggregation(BuildProbabilityAggregation::TilingOnly);
        assert_eq!(
            BuildTargetSearchQuerySnapshot::cover(invalid_transport),
            Err(BuildV2QuerySnapshotError::Options(
                BuildV2OptionError::VisibleSevenUnavailableWithTilingOnly
            ))
        );
    }

    #[test]
    fn supplied_snapshots_require_replay_and_resolve_fixed_objectives_before_execution() {
        let no_replay = BuildSuppliedSolutionDocumentSnapshot::new(0, 4, 1, 1, false, "no-replay")
            .expect("document identity remains well formed");
        assert_eq!(
            BuildSuppliedSolutionEvaluationQuerySnapshot::cover(no_replay),
            Err(BuildV2QuerySnapshotError::SuppliedSolutionReplayUnavailable)
        );

        let minimals = BuildSuppliedSolutionEvaluationQuerySnapshot::minimals(supplied_solutions(
            "fixed-minimals",
        ))
        .expect("replayable minimals query");
        assert_eq!(minimals.options().objective(), BuildObjective::MinCover);
        assert!(matches!(
            minimals.with_options(
                BuildV2OptionRequest::default().with_objective(BuildObjective::Unique)
            ),
            Err(BuildV2QuerySnapshotError::Options(
                BuildV2OptionError::FixedObjectiveConflict { .. }
            ))
        ));
    }

    #[test]
    fn score_snapshot_options_and_typed_payload_cannot_diverge() {
        let score = BuildSuppliedSolutionEvaluationQuerySnapshot::score(
            BuildSuppliedSolutionScoreQuerySnapshot::new(
                supplied_solutions("score-options"),
                BuildScoreProfile::Guideline,
                4,
            ),
        )
        .expect("replayable score query");
        assert_eq!(
            score.options().score_profile(),
            Some(BuildScoreProfile::Guideline)
        );
        assert_eq!(score.options().initial_b2b(), Some(4));

        let score = score
            .with_options(
                BuildV2OptionRequest::default()
                    .with_objective(BuildObjective::MaxScoreCover)
                    .with_score_profile(BuildScoreProfile::JstrisUltra)
                    .with_initial_b2b(u16::MAX),
            )
            .expect("identical fixed objective and valid score options");
        let payload = score.score_query().expect("typed score payload");
        assert_eq!(payload.score_profile(), BuildScoreProfile::JstrisUltra);
        assert_eq!(payload.initial_b2b(), u16::MAX);
        assert_eq!(
            score.options().score_accuracy(),
            Some("basic-approximation")
        );
        assert_eq!(score.options().profile_specific_exact(), Some(false));
    }

    #[test]
    fn target_result_identity_is_validated_fieldwise_and_bound_to_the_query() {
        let query = BuildTargetSearchQuerySnapshot::cover(build_cover_query(0xf))
            .expect("valid Build cover query");
        let contract = query.contract();
        let validated = ValidatedBuildTargetSearchResultAuthority::validate(
            query.clone(),
            target_identity(contract),
        )
        .expect("exact target identity");
        assert_eq!(validated.contract(), BuildTargetSearchContract::Cover);
        assert!(validated.matches_query(&query));
        assert!(!validated.matches_query(
            &BuildTargetSearchQuerySnapshot::cover(build_cover_query(0x1e))
                .expect("different valid Build cover query")
        ));
        assert_eq!(validated.query(), &query);
        assert_eq!(
            validated.identity(),
            &target_identity(BuildTargetSearchContract::Cover)
        );

        let wrong = [
            (
                ReportedBuildTargetSearchResultIdentity::new(
                    "build.setup",
                    contract.problem_contract_id(),
                    contract.input_schema_id(),
                    contract.result_contract_id(),
                ),
                BuildTargetSearchResultValidationError::CapabilityIdMismatch,
            ),
            (
                ReportedBuildTargetSearchResultIdentity::new(
                    contract.capability_id(),
                    "supplied-solution-build-evaluation.v1",
                    contract.input_schema_id(),
                    contract.result_contract_id(),
                ),
                BuildTargetSearchResultValidationError::ProblemContractIdMismatch,
            ),
            (
                ReportedBuildTargetSearchResultIdentity::new(
                    contract.capability_id(),
                    contract.problem_contract_id(),
                    "build-solution-document.v1",
                    contract.result_contract_id(),
                ),
                BuildTargetSearchResultValidationError::InputSchemaIdMismatch,
            ),
            (
                ReportedBuildTargetSearchResultIdentity::new(
                    contract.capability_id(),
                    contract.problem_contract_id(),
                    contract.input_schema_id(),
                    "build-supplied-coverage.v1",
                ),
                BuildTargetSearchResultValidationError::ResultContractIdMismatch,
            ),
        ];
        for (identity, expected) in wrong {
            assert_eq!(
                ValidatedBuildTargetSearchResultAuthority::validate(query.clone(), identity),
                Err(expected)
            );
        }
    }

    #[test]
    fn supplied_result_identity_is_validated_fieldwise_and_bound_to_the_query() {
        let query = BuildSuppliedSolutionEvaluationQuerySnapshot::cover(supplied_solutions(
            "supplied:canonical",
        ))
        .expect("replayable supplied query");
        let contract = query.contract();
        let validated = ValidatedBuildSuppliedSolutionEvaluationResultAuthority::validate(
            query.clone(),
            supplied_identity(contract),
        )
        .expect("exact supplied identity");
        assert_eq!(
            validated.contract(),
            BuildSuppliedSolutionEvaluationContract::Cover
        );
        assert!(validated.matches_query(&query));
        assert!(!validated.matches_query(
            &BuildSuppliedSolutionEvaluationQuerySnapshot::cover(supplied_solutions(
                "supplied:different",
            ))
            .expect("different replayable supplied query")
        ));
        assert_eq!(validated.query(), &query);
        assert_eq!(
            validated.query().document().document_hash(),
            "supplied:canonical"
        );
        assert_eq!(
            validated.identity(),
            &supplied_identity(BuildSuppliedSolutionEvaluationContract::Cover)
        );

        let wrong = [
            (
                ReportedBuildSuppliedSolutionEvaluationResultIdentity::new(
                    "build.cover",
                    contract.problem_contract_id(),
                    contract.input_schema_id(),
                    contract.result_contract_id(),
                ),
                BuildSuppliedSolutionEvaluationResultValidationError::CapabilityIdMismatch,
            ),
            (
                ReportedBuildSuppliedSolutionEvaluationResultIdentity::new(
                    contract.capability_id(),
                    "build-base-target-search.v2",
                    contract.input_schema_id(),
                    contract.result_contract_id(),
                ),
                BuildSuppliedSolutionEvaluationResultValidationError::ProblemContractIdMismatch,
            ),
            (
                ReportedBuildSuppliedSolutionEvaluationResultIdentity::new(
                    contract.capability_id(),
                    contract.problem_contract_id(),
                    "build-base-target.v2",
                    contract.result_contract_id(),
                ),
                BuildSuppliedSolutionEvaluationResultValidationError::InputSchemaIdMismatch,
            ),
            (
                ReportedBuildSuppliedSolutionEvaluationResultIdentity::new(
                    contract.capability_id(),
                    contract.problem_contract_id(),
                    contract.input_schema_id(),
                    "build-coverage.v2",
                ),
                BuildSuppliedSolutionEvaluationResultValidationError::ResultContractIdMismatch,
            ),
        ];
        for (identity, expected) in wrong {
            assert_eq!(
                ValidatedBuildSuppliedSolutionEvaluationResultAuthority::validate(
                    query.clone(),
                    identity,
                ),
                Err(expected)
            );
        }
    }

    #[test]
    fn score_queries_keep_family_specific_documents_and_fieldwise_options() {
        let target = BuildColoredTargetScoreQuerySnapshot::new(
            build_cover_query(0xf),
            colored_target("target-score"),
            BuildScoreProfile::Guideline,
            u16::MAX,
        );
        let supplied = BuildSuppliedSolutionScoreQuerySnapshot::new(
            supplied_solutions("supplied-score"),
            BuildScoreProfile::JstrisUltra,
            0,
        );

        assert_eq!(target.document().document_hash(), "target-score");
        assert_eq!(target.score_profile(), BuildScoreProfile::Guideline);
        assert_eq!(target.score_profile_id(), "guideline");
        assert_eq!(target.initial_b2b(), u16::MAX);
        assert_eq!(supplied.document().document_hash(), "supplied-score");
        assert_eq!(supplied.score_profile(), BuildScoreProfile::JstrisUltra);
        assert_eq!(supplied.score_profile_id(), "jstris-ultra");
        assert_eq!(supplied.initial_b2b(), 0);
    }
}
