use clearra_app::{
    BuildColoredTargetDocument, BuildColoredTargetSetV1, BuildCongruentCoverV1Request,
    BuildCongruentV1Request, BuildCoverV2Request, BuildEvaluateB2bCoverV1Request,
    BuildEvaluateCoverPercentV1Request, BuildEvaluateCoverV1Request,
    BuildEvaluateMinimalsV1Request, BuildEvaluateScoreV1Request, BuildObjective,
    BuildQueueKnowledge, BuildScoreProfile, BuildSetupCoverPercentV1Request,
    BuildSetupCoverScoreV1Request, BuildSetupCoverV1Request, BuildSetupV1Request,
    BuildSuppliedSolutionSetV1, BuildV2AppCommand, FieldDocumentFormat,
    FIELD_DOCUMENT_MAX_INPUT_BYTES,
};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::solution::StandardBoard64ColoredTilingIdentity;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{PcExecutionPolicy, PcQueueInput};
use clearra_problem::BuildSolutionProbabilityPolicy;
use clearra_rules::profile::rule_profile::RuleProfile;
use clearra_supply::QueueObservationPolicy;

use crate::{WebBuildProbabilityInput, WebCommandError, WebCommandErrorCode};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WebBuildV2Capability {
    Cover,
    PinnedMinimals,
    Setup,
    Congruent,
    CongruentCover,
    SetupCover,
    SetupCoverPercent,
    SetupCoverScore,
    EvaluateCover,
    EvaluateMinimals,
    EvaluateScore,
    EvaluateB2bCover,
    EvaluateCoverPercent,
}

impl WebBuildV2Capability {
    pub const fn capability_id(self) -> &'static str {
        match self {
            Self::Cover => "build.cover",
            Self::PinnedMinimals => "build.pinned-minimals",
            Self::Setup => "build.setup",
            Self::Congruent => "build.congruent",
            Self::CongruentCover => "build.congruent-cover",
            Self::SetupCover => "build.setup-cover",
            Self::SetupCoverPercent => "build.setup-cover-percent",
            Self::SetupCoverScore => "build.setup-cover-score",
            Self::EvaluateCover => "build.evaluate.cover",
            Self::EvaluateMinimals => "build.evaluate.minimals",
            Self::EvaluateScore => "build.evaluate.score",
            Self::EvaluateB2bCover => "build.evaluate.b2b-cover",
            Self::EvaluateCoverPercent => "build.evaluate.cover-percent",
        }
    }

    pub const fn default_objective(self) -> BuildObjective {
        match self {
            Self::Cover | Self::PinnedMinimals | Self::CongruentCover | Self::SetupCover => {
                BuildObjective::MinCover
            }
            Self::Setup | Self::Congruent | Self::SetupCoverPercent => BuildObjective::Unique,
            Self::SetupCoverScore | Self::EvaluateScore => BuildObjective::MaxScoreCover,
            Self::EvaluateCover | Self::EvaluateB2bCover => BuildObjective::All,
            Self::EvaluateMinimals => BuildObjective::MinCover,
            Self::EvaluateCoverPercent => BuildObjective::Unique,
        }
    }

    pub const fn supports_objective(self, objective: BuildObjective) -> bool {
        match self {
            Self::Cover | Self::PinnedMinimals | Self::CongruentCover | Self::SetupCover => {
                matches!(
                    objective,
                    BuildObjective::MinCover | BuildObjective::MaxProbabilityMinimum
                )
            }
            Self::Setup | Self::Congruent | Self::SetupCoverPercent => {
                matches!(objective, BuildObjective::All | BuildObjective::Unique)
            }
            Self::SetupCoverScore | Self::EvaluateScore => {
                matches!(objective, BuildObjective::MaxScoreCover)
            }
            Self::EvaluateCover | Self::EvaluateB2bCover => {
                matches!(objective, BuildObjective::All)
            }
            Self::EvaluateMinimals => matches!(objective, BuildObjective::MinCover),
            Self::EvaluateCoverPercent => matches!(objective, BuildObjective::Unique),
        }
    }

    pub const fn uses_target_document(self) -> bool {
        matches!(
            self,
            Self::Setup
                | Self::Congruent
                | Self::CongruentCover
                | Self::SetupCover
                | Self::SetupCoverPercent
                | Self::SetupCoverScore
        )
    }

    pub const fn uses_solution_document(self) -> bool {
        matches!(
            self,
            Self::EvaluateCover
                | Self::EvaluateMinimals
                | Self::EvaluateScore
                | Self::EvaluateB2bCover
                | Self::EvaluateCoverPercent
        )
    }

    pub const fn score_capable(self) -> bool {
        matches!(self, Self::SetupCoverScore | Self::EvaluateScore)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WebBuildV2Source {
    BaseTarget,
    Target(BuildColoredTargetSetV1),
    Supplied(BuildSuppliedSolutionSetV1),
}

/// Nominal public Web input for the twelve Build v2 capabilities.
///
/// Target-search and supplied-solution documents deliberately become distinct
/// owners here. A target owner is never exposed as a supplied-solution owner,
/// even when both were decoded from identical document bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebBuildV2Input {
    capability: WebBuildV2Capability,
    query_input: WebBuildProbabilityInput,
    source: WebBuildV2Source,
    objective: BuildObjective,
    queue_knowledge: BuildQueueKnowledge,
    score_profile: Option<BuildScoreProfile>,
    initial_b2b: Option<u16>,
    pinned_candidate_keys: Vec<String>,
    pinned_colored_identities: Vec<StandardBoard64ColoredTilingIdentity>,
    expected_source_set_hash: Option<String>,
}

impl WebBuildV2Input {
    pub fn cover(
        base_words: [u64; 4],
        target_words: [u64; 4],
        visible_height: u16,
        objective: BuildObjective,
    ) -> Result<Self, WebCommandError> {
        let capability = WebBuildV2Capability::Cover;
        validate_objective(capability, objective)?;
        Ok(Self {
            capability,
            query_input: WebBuildProbabilityInput::from_words(
                base_words,
                target_words,
                visible_height,
            )
            .with_visible_height_preserved(true),
            source: WebBuildV2Source::BaseTarget,
            objective,
            queue_knowledge: BuildQueueKnowledge::Oracle,
            score_profile: None,
            initial_b2b: None,
            pinned_candidate_keys: Vec::new(),
            pinned_colored_identities: Vec::new(),
            expected_source_set_hash: None,
        })
    }

    pub fn pinned_minimals(
        base_words: [u64; 4],
        target_words: [u64; 4],
        visible_height: u16,
        objective: BuildObjective,
    ) -> Result<Self, WebCommandError> {
        let mut input = Self::cover(base_words, target_words, visible_height, objective)?;
        input.capability = WebBuildV2Capability::PinnedMinimals;
        Ok(input)
    }

    pub fn target_document(
        capability: WebBuildV2Capability,
        format: FieldDocumentFormat,
        document: &str,
        objective: BuildObjective,
    ) -> Result<Self, WebCommandError> {
        if !capability.uses_target_document() {
            return Err(invalid(format!(
                "{} does not accept a colored target document",
                capability.capability_id()
            )));
        }
        validate_objective(capability, objective)?;
        let decoded = decode_document(format, document, "target")?;
        let target = decoded.target();
        let query_input = document_query_input(
            target.initial_board_mask(),
            target.target_cells_mask(),
            target.visible_height(),
            decoded.source_piece_count(),
        );
        let score_capable = capability.score_capable();
        Ok(Self {
            capability,
            query_input,
            source: WebBuildV2Source::Target(decoded.into_target()),
            objective,
            queue_knowledge: BuildQueueKnowledge::Oracle,
            score_profile: score_capable.then_some(BuildScoreProfile::default()),
            initial_b2b: score_capable.then_some(0),
            pinned_candidate_keys: Vec::new(),
            pinned_colored_identities: Vec::new(),
            expected_source_set_hash: None,
        })
    }

    pub fn solution_document(
        capability: WebBuildV2Capability,
        format: FieldDocumentFormat,
        document: &str,
        objective: BuildObjective,
    ) -> Result<Self, WebCommandError> {
        if !capability.uses_solution_document() {
            return Err(invalid(format!(
                "{} does not accept a supplied-solution document",
                capability.capability_id()
            )));
        }
        validate_objective(capability, objective)?;
        let decoded = decode_document(format, document, "solution")?;
        let normalized = decoded.target();
        let query_input = document_query_input(
            normalized.initial_board_mask(),
            normalized.target_cells_mask(),
            normalized.visible_height(),
            decoded.source_piece_count(),
        );
        // Construct the supplied owner from the normalized source evidence.
        // No target-to-supplied conversion is exported across this boundary.
        let supplied = BuildSuppliedSolutionSetV1::new(
            normalized.visible_height(),
            normalized.page_count(),
            normalized.document_hash().to_owned(),
            normalized.identities().iter().copied(),
        )
        .map_err(|error| {
            invalid(format!(
                "invalid supplied-solution document for {}: {error:?}",
                capability.capability_id()
            ))
        })?;
        let score_capable = capability.score_capable();
        Ok(Self {
            capability,
            query_input,
            source: WebBuildV2Source::Supplied(supplied),
            objective,
            queue_knowledge: BuildQueueKnowledge::Oracle,
            score_profile: score_capable.then_some(BuildScoreProfile::default()),
            initial_b2b: score_capable.then_some(0),
            pinned_candidate_keys: Vec::new(),
            pinned_colored_identities: Vec::new(),
            expected_source_set_hash: None,
        })
    }

    pub fn with_queue_knowledge(mut self, queue_knowledge: BuildQueueKnowledge) -> Self {
        self.queue_knowledge = queue_knowledge;
        self
    }

    pub fn with_pinned_candidate_keys(
        mut self,
        mut keys: Vec<String>,
    ) -> Result<Self, WebCommandError> {
        if self.capability != WebBuildV2Capability::EvaluateMinimals && !keys.is_empty() {
            return Err(invalid(
                "only build.evaluate.minimals accepts supplied candidate keys",
            ));
        }
        if !keys.is_empty() {
            let WebBuildV2Source::Supplied(supplied) = &self.source else {
                return Err(invalid(
                    "pinned candidates require a supplied solution document",
                ));
            };
            if keys
                .iter()
                .any(|key| !supplied.candidate_keys().contains(key))
            {
                return Err(invalid(
                    "pinned solution is not in the supplied solution document",
                ));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        keys.retain(|key| seen.insert(key.clone()));
        self.pinned_candidate_keys = keys;
        Ok(self)
    }

    /// Required drawings constrain the full source, which remains the only
    /// coverage authority. Unlike a target document, a selection may contain
    /// both original and mirrored masks; the producer resolves every identity.
    pub fn with_pinned_solution_document(
        self,
        format: FieldDocumentFormat,
        document: &str,
    ) -> Result<Self, WebCommandError> {
        if !matches!(
            self.capability,
            WebBuildV2Capability::EvaluateMinimals | WebBuildV2Capability::PinnedMinimals
        ) {
            return Err(invalid(
                "only build.evaluate.minimals and build.pinned-minimals accept a pinned solution document",
            ));
        }
        if self.capability == WebBuildV2Capability::PinnedMinimals {
            validate_document_size(document, "pinned solution")?;
            let decoded = clearra_app::BuildPinnedSolutionDocument::decode(format, document)
                .map_err(|error| invalid(format!("invalid pinned solution document: {error:?}")))?;
            let mut selected = self;
            selected.pinned_colored_identities = decoded.into_identities();
            return Ok(selected);
        }
        let decoded = decode_document(format, document, "pinned solution")?;
        let normalized = decoded.target();
        let pins = BuildSuppliedSolutionSetV1::new(
            normalized.visible_height(),
            normalized.page_count(),
            normalized.document_hash().to_owned(),
            normalized.identities().iter().copied(),
        )
        .map_err(|error| invalid(format!("invalid pinned solution document: {error:?}")))?;
        self.with_pinned_candidate_keys(pins.candidate_keys().to_vec())
    }

    pub fn with_expected_source_set_hash(
        mut self,
        digest: String,
    ) -> Result<Self, WebCommandError> {
        let valid_hash = digest
            .strip_prefix("cts1:")
            .is_some_and(|hex| hex.len() == 16 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
        if self.capability != WebBuildV2Capability::PinnedMinimals || !valid_hash {
            return Err(invalid(
                "build.pinned-minimals requires the canonical cts1:<16 hex digits> source-set hash",
            ));
        }
        self.expected_source_set_hash = Some(digest.to_ascii_lowercase());
        Ok(self)
    }

    /// CLI-facing one-based indices in the canonical supplied candidate map.
    /// Normalized solution keys may contain command-control characters, so
    /// they are never required as raw command-line tokens.
    pub fn with_pinned_candidate_ordinals(
        self,
        ordinals: Vec<usize>,
    ) -> Result<Self, WebCommandError> {
        if ordinals.is_empty() {
            return Ok(self);
        }
        let WebBuildV2Source::Supplied(supplied) = &self.source else {
            return Err(invalid(
                "pinned candidates require a supplied solution document",
            ));
        };
        let keys = ordinals
            .into_iter()
            .map(|ordinal| {
                ordinal
                    .checked_sub(1)
                    .and_then(|index| supplied.candidate_keys().get(index))
                    .cloned()
                    .ok_or_else(|| invalid("pinned candidate ordinal is out of range"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.with_pinned_candidate_keys(keys)
    }

    pub fn with_hold_piece(mut self, hold_piece: Option<PieceKind>) -> Self {
        self.query_input = self.query_input.with_hold_piece(hold_piece);
        self
    }

    pub fn with_allow_hold(mut self, allow_hold: bool) -> Self {
        self.query_input = self.query_input.with_allow_hold(allow_hold);
        self
    }

    pub fn with_leading_hold_piece(mut self, piece: PieceKind) -> Self {
        self.query_input = self.query_input.with_leading_hold_piece(piece);
        self
    }

    pub fn with_source_piece_count(
        mut self,
        source_piece_count: usize,
    ) -> Result<Self, WebCommandError> {
        if !matches!(
            self.capability,
            WebBuildV2Capability::Cover | WebBuildV2Capability::PinnedMinimals
        ) {
            return Err(invalid(
                "document-backed Build v2 derives its source-piece count from the document",
            ));
        }
        self.query_input = self.query_input.with_source_piece_count(source_piece_count);
        Ok(self)
    }

    pub fn with_score_options(
        mut self,
        score_profile: BuildScoreProfile,
        initial_b2b: u16,
    ) -> Result<Self, WebCommandError> {
        if !self.capability.score_capable() {
            return Err(invalid(format!(
                "{} does not accept score-profile or initial-b2b options",
                self.capability.capability_id()
            )));
        }
        self.score_profile = Some(score_profile);
        self.initial_b2b = Some(initial_b2b);
        Ok(self)
    }

    pub const fn capability(&self) -> WebBuildV2Capability {
        self.capability
    }

    pub const fn objective(&self) -> BuildObjective {
        self.objective
    }

    pub const fn queue_knowledge(&self) -> BuildQueueKnowledge {
        self.queue_knowledge
    }

    pub const fn hold_piece(&self) -> Option<PieceKind> {
        self.query_input.hold_piece()
    }

    pub const fn allow_hold(&self) -> bool {
        self.query_input.allow_hold()
    }

    pub(crate) fn to_app_command(
        &self,
        queue: PcQueueInput,
        execution_policy: PcExecutionPolicy,
        finite_standard_bag_len: Option<usize>,
        rule: RuleProfile,
    ) -> Result<BuildV2AppCommand, WebCommandError> {
        validate_objective(self.capability, self.objective)?;
        let query = self
            .query_input
            .to_query(
                queue,
                execution_policy,
                finite_standard_bag_len,
                rule,
                ObjectivePolicy::unique(),
            )
            .map_err(|error| {
                invalid(format!(
                    "invalid {} field: {error:?}",
                    self.capability.capability_id()
                ))
            })?
            .with_queue_observation_policy(match self.queue_knowledge {
                BuildQueueKnowledge::Oracle => QueueObservationPolicy::FullQueueOracle,
                BuildQueueKnowledge::VisibleSeven => QueueObservationPolicy::VisibleSeven,
            })
            .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);

        match (self.capability, &self.source) {
            (WebBuildV2Capability::Cover, WebBuildV2Source::BaseTarget) => {
                BuildCoverV2Request::new(query, self.objective)
                    .map(BuildV2AppCommand::build_cover)
                    .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::PinnedMinimals, WebBuildV2Source::BaseTarget) => {
                if self.pinned_colored_identities.is_empty() {
                    return Err(invalid("build.pinned-minimals requires selected solutions"));
                }
                BuildCoverV2Request::new(query, self.objective)
                    .map(|request| {
                        let request = request
                            .with_pinned_colored_identities(self.pinned_colored_identities.clone());
                        let request = match &self.expected_source_set_hash {
                            Some(digest) => request.with_expected_source_set_hash(digest.clone()),
                            None => request,
                        };
                        BuildV2AppCommand::build_cover(request)
                    })
                    .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::Setup, WebBuildV2Source::Target(target)) => {
                BuildSetupV1Request::new(query, target.clone(), self.objective)
                    .map(BuildV2AppCommand::build_setup)
                    .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::Congruent, WebBuildV2Source::Target(target)) => {
                BuildCongruentV1Request::new(query, target.clone(), self.objective)
                    .map(BuildV2AppCommand::build_congruent)
                    .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::CongruentCover, WebBuildV2Source::Target(target)) => {
                BuildCongruentCoverV1Request::new(query, target.clone(), self.objective)
                    .map(BuildV2AppCommand::build_congruent_cover)
                    .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::SetupCover, WebBuildV2Source::Target(target)) => {
                BuildSetupCoverV1Request::new(query, target.clone(), self.objective)
                    .map(BuildV2AppCommand::build_setup_cover)
                    .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::SetupCoverPercent, WebBuildV2Source::Target(target)) => {
                BuildSetupCoverPercentV1Request::new(query, target.clone(), self.objective)
                    .map(BuildV2AppCommand::build_setup_cover_percent)
                    .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::SetupCoverScore, WebBuildV2Source::Target(target)) => {
                BuildSetupCoverScoreV1Request::new(
                    query,
                    target.clone(),
                    self.score_profile
                        .ok_or_else(|| missing_score(self.capability))?,
                    self.initial_b2b
                        .ok_or_else(|| missing_score(self.capability))?,
                )
                .map(BuildV2AppCommand::build_setup_cover_score)
                .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::EvaluateCover, WebBuildV2Source::Supplied(supplied)) => {
                BuildEvaluateCoverV1Request::new(query, supplied.clone())
                    .map(BuildV2AppCommand::build_evaluate_cover)
                    .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::EvaluateMinimals, WebBuildV2Source::Supplied(supplied)) => {
                BuildEvaluateMinimalsV1Request::new(query, supplied.clone())
                    .and_then(|request| {
                        request.with_pinned_candidate_keys(self.pinned_candidate_keys.clone())
                    })
                    .map(BuildV2AppCommand::build_evaluate_minimals)
                    .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::EvaluateScore, WebBuildV2Source::Supplied(supplied)) => {
                BuildEvaluateScoreV1Request::new(
                    query,
                    supplied.clone(),
                    self.score_profile
                        .ok_or_else(|| missing_score(self.capability))?,
                    self.initial_b2b
                        .ok_or_else(|| missing_score(self.capability))?,
                )
                .map(BuildV2AppCommand::build_evaluate_score)
                .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::EvaluateB2bCover, WebBuildV2Source::Supplied(supplied)) => {
                BuildEvaluateB2bCoverV1Request::new(query, supplied.clone())
                    .map(BuildV2AppCommand::build_evaluate_b2b_cover)
                    .map_err(|error| request_error(self.capability, error))
            }
            (WebBuildV2Capability::EvaluateCoverPercent, WebBuildV2Source::Supplied(supplied)) => {
                BuildEvaluateCoverPercentV1Request::new(query, supplied.clone())
                    .map(BuildV2AppCommand::build_evaluate_cover_percent)
                    .map_err(|error| request_error(self.capability, error))
            }
            _ => Err(invalid(format!(
                "{} input owner does not match its capability",
                self.capability.capability_id()
            ))),
        }
    }
}

fn decode_document(
    format: FieldDocumentFormat,
    document: &str,
    role: &str,
) -> Result<BuildColoredTargetDocument, WebCommandError> {
    validate_document_size(document, role)?;
    BuildColoredTargetDocument::decode(format, document)
        .map_err(|error| invalid(format!("invalid Build v2 {role} document: {error:?}")))
}

fn validate_document_size(document: &str, role: &str) -> Result<(), WebCommandError> {
    if document.len() > FIELD_DOCUMENT_MAX_INPUT_BYTES {
        return Err(invalid(format!(
            "Build v2 {role} document exceeds {FIELD_DOCUMENT_MAX_INPUT_BYTES} bytes"
        )));
    }
    Ok(())
}

fn document_query_input(
    initial_board_mask: u64,
    target_cells_mask: u64,
    visible_height: u8,
    source_piece_count: usize,
) -> WebBuildProbabilityInput {
    WebBuildProbabilityInput::from_words(
        [initial_board_mask, 0, 0, 0],
        [target_cells_mask, 0, 0, 0],
        u16::from(visible_height),
    )
    .with_source_piece_count(source_piece_count)
    .with_horizontal_mirror_included(false)
    .with_visible_height_preserved(true)
}

fn validate_objective(
    capability: WebBuildV2Capability,
    objective: BuildObjective,
) -> Result<(), WebCommandError> {
    if capability.supports_objective(objective) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{} does not accept objective '{}'",
            capability.capability_id(),
            objective.as_str()
        )))
    }
}

fn missing_score(capability: WebBuildV2Capability) -> WebCommandError {
    invalid(format!(
        "{} is missing its closed score options",
        capability.capability_id()
    ))
}

fn request_error(
    capability: WebBuildV2Capability,
    error: impl core::fmt::Debug,
) -> WebCommandError {
    invalid(format!(
        "invalid {} request: {error:?}",
        capability.capability_id()
    ))
}

fn invalid(message: impl Into<String>) -> WebCommandError {
    WebCommandError::new(WebCommandErrorCode::InvalidValue, message)
}
