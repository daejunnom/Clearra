//! Closed Host DTO for the remaining Build v2 product families.
//! SRP rationale: this module has one change reason: the closed host payload contract for Build v2 products.
//!
//! The producer selects one of four nominal shapes. Optional fields exist only
//! to keep a single stable wire object; `try_*` constructors validate the exact
//! capability/result pairing and reject fields that do not belong to that
//! shape. In particular, score equality is always score-only. Attack is an
//! informational value attached to the canonical equal-score trace and never
//! participates in eligibility or ordering.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum BuildV2PayloadKind {
    CandidateFamily,
    Probability,
    Portfolio,
    ScorePortfolio,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildV2CompletenessPayload {
    input_identity_bound: bool,
    producer_filter_bound: bool,
    buildability_replay_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
    exact_minimum_proven: bool,
    score_evidence_complete: bool,
}

impl BuildV2CompletenessPayload {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        input_identity_bound: bool,
        producer_filter_bound: bool,
        buildability_replay_complete: bool,
        coverage_rows_complete: bool,
        probability_weights_complete: bool,
        exact_minimum_proven: bool,
        score_evidence_complete: bool,
    ) -> Self {
        Self {
            input_identity_bound,
            producer_filter_bound,
            buildability_replay_complete,
            coverage_rows_complete,
            probability_weights_complete,
            exact_minimum_proven,
            score_evidence_complete,
        }
    }

    pub const fn input_identity_bound(self) -> bool {
        self.input_identity_bound
    }
    pub const fn producer_filter_bound(self) -> bool {
        self.producer_filter_bound
    }
    pub const fn buildability_replay_complete(self) -> bool {
        self.buildability_replay_complete
    }
    pub const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }
    pub const fn probability_weights_complete(self) -> bool {
        self.probability_weights_complete
    }
    pub const fn exact_minimum_proven(self) -> bool {
        self.exact_minimum_proven
    }
    pub const fn score_evidence_complete(self) -> bool {
        self.score_evidence_complete
    }
    pub const fn replay_complete(self) -> bool {
        self.input_identity_bound
            && self.producer_filter_bound
            && self.buildability_replay_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
    }
    pub const fn portfolio_complete(self) -> bool {
        self.replay_complete() && self.exact_minimum_proven
    }
    pub const fn score_portfolio_complete(self) -> bool {
        self.portfolio_complete() && self.score_evidence_complete
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildV2CandidateCoveragePayload {
    candidate_key: String,
    covered_pattern_count: String,
}

impl BuildV2CandidateCoveragePayload {
    pub fn try_new(
        candidate_key: impl Into<String>,
        covered_pattern_count: impl Into<String>,
    ) -> Result<Self, BuildV2ProductPayloadError> {
        Self::try_from_owned_memory_authorized_parts(
            candidate_key.into(),
            covered_pattern_count.into(),
        )
    }

    /// Allocation-free validation seam for a boundary that has already
    /// created and memory-authorized both retained strings.
    pub fn try_from_owned_memory_authorized_parts(
        candidate_key: String,
        covered_pattern_count: String,
    ) -> Result<Self, BuildV2ProductPayloadError> {
        let value = Self {
            candidate_key,
            covered_pattern_count,
        };
        if value.candidate_key.is_empty() {
            return Err(BuildV2ProductPayloadError::CandidateKeyInvalid);
        }
        canonical_decimal(&value.covered_pattern_count)
            .then_some(value)
            .ok_or(BuildV2ProductPayloadError::DecimalInvalid(
                "covered_pattern_count",
            ))
    }

    pub fn candidate_key(&self) -> &str {
        &self.candidate_key
    }
    pub fn covered_pattern_count(&self) -> &str {
        &self.covered_pattern_count
    }
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.candidate_key.capacity() as u128)
            .checked_add(self.covered_pattern_count.capacity() as u128)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildV2ScoreWinnerPayload {
    pattern_id: String,
    candidate_key: String,
    score: String,
    informational_attack: String,
}

impl BuildV2ScoreWinnerPayload {
    pub fn try_new(
        pattern_id: impl Into<String>,
        candidate_key: impl Into<String>,
        score: impl Into<String>,
        informational_attack: impl Into<String>,
    ) -> Result<Self, BuildV2ProductPayloadError> {
        Self::try_from_owned_memory_authorized_parts(
            pattern_id.into(),
            candidate_key.into(),
            score.into(),
            informational_attack.into(),
        )
    }

    /// Allocation-free validation seam for a boundary that has already
    /// created and memory-authorized every retained string.
    pub fn try_from_owned_memory_authorized_parts(
        pattern_id: String,
        candidate_key: String,
        score: String,
        informational_attack: String,
    ) -> Result<Self, BuildV2ProductPayloadError> {
        let value = Self {
            pattern_id,
            candidate_key,
            score,
            informational_attack,
        };
        if value.candidate_key.is_empty() {
            return Err(BuildV2ProductPayloadError::CandidateKeyInvalid);
        }
        for (name, text) in [
            ("pattern_id", value.pattern_id.as_str()),
            ("score", value.score.as_str()),
            ("informational_attack", value.informational_attack.as_str()),
        ] {
            if !canonical_decimal(text) {
                return Err(BuildV2ProductPayloadError::DecimalInvalid(name));
            }
        }
        Ok(value)
    }

    pub fn pattern_id(&self) -> &str {
        &self.pattern_id
    }
    pub fn candidate_key(&self) -> &str {
        &self.candidate_key
    }
    pub fn score(&self) -> &str {
        &self.score
    }
    pub fn informational_attack(&self) -> &str {
        &self.informational_attack
    }
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        [
            &self.pattern_id,
            &self.candidate_key,
            &self.score,
            &self.informational_attack,
        ]
        .into_iter()
        .try_fold(0_u128, |total, text| {
            total.checked_add(text.capacity() as u128)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildV2ProductPayloadError {
    CapabilityContractMismatch,
    IdentityInvalid(&'static str),
    ObjectiveInvalid,
    DecimalInvalid(&'static str),
    CountMismatch,
    ProbabilityInvalid,
    CandidateKeyInvalid,
    ScoreSemanticsInvalid,
    CompletenessInvalid,
    PageSourceInvalid,
    ShapeInvalid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildV2ProductPayload {
    kind: BuildV2PayloadKind,
    capability_id: String,
    result_contract: String,
    input_identity_sha256: String,
    evaluation_identity_sha256: Option<String>,
    replay_basis: Option<String>,
    objective: String,
    score_profile: Option<String>,
    initial_b2b: Option<String>,
    score_accuracy: Option<String>,
    profile_specific_exact: Option<bool>,
    score_equality_basis: Option<String>,
    informational_attack_basis: Option<String>,
    source_candidate_count: String,
    reachable_candidate_count: String,
    selected_candidate_count: Option<String>,
    pattern_count: String,
    covered_pattern_count: Option<String>,
    required_pattern_count: Option<String>,
    union_probability: Option<String>,
    b2b_preservation_required: Option<bool>,
    candidates: Vec<BuildV2CandidateCoveragePayload>,
    canonical_candidate_keys: Vec<String>,
    winners: Vec<BuildV2ScoreWinnerPayload>,
    completeness: BuildV2CompletenessPayload,
    page_source_available: bool,
    page_source_identity_sha256: Option<String>,
}

impl BuildV2ProductPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn try_candidate_family(
        capability_id: impl Into<String>,
        result_contract: impl Into<String>,
        input_identity_sha256: impl Into<String>,
        evaluation_identity_sha256: impl Into<String>,
        objective: impl Into<String>,
        source_candidate_count: impl Into<String>,
        reachable_candidate_count: impl Into<String>,
        pattern_count: impl Into<String>,
        covered_pattern_count: impl Into<String>,
        union_probability: impl Into<String>,
        b2b_preservation_required: Option<bool>,
        candidates: Vec<BuildV2CandidateCoveragePayload>,
        completeness: BuildV2CompletenessPayload,
    ) -> Result<Self, BuildV2ProductPayloadError> {
        Self::finish(Self {
            kind: BuildV2PayloadKind::CandidateFamily,
            capability_id: capability_id.into(),
            result_contract: result_contract.into(),
            input_identity_sha256: input_identity_sha256.into(),
            evaluation_identity_sha256: Some(evaluation_identity_sha256.into()),
            replay_basis: None,
            objective: objective.into(),
            score_profile: None,
            initial_b2b: None,
            score_accuracy: None,
            profile_specific_exact: None,
            score_equality_basis: None,
            informational_attack_basis: None,
            source_candidate_count: source_candidate_count.into(),
            reachable_candidate_count: reachable_candidate_count.into(),
            selected_candidate_count: None,
            pattern_count: pattern_count.into(),
            covered_pattern_count: Some(covered_pattern_count.into()),
            required_pattern_count: None,
            union_probability: Some(union_probability.into()),
            b2b_preservation_required,
            candidates,
            canonical_candidate_keys: Vec::new(),
            winners: Vec::new(),
            completeness,
            page_source_available: false,
            page_source_identity_sha256: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_probability(
        capability_id: impl Into<String>,
        result_contract: impl Into<String>,
        input_identity_sha256: impl Into<String>,
        evaluation_identity_sha256: impl Into<String>,
        replay_basis: Option<String>,
        objective: impl Into<String>,
        source_candidate_count: impl Into<String>,
        reachable_candidate_count: impl Into<String>,
        pattern_count: impl Into<String>,
        covered_pattern_count: impl Into<String>,
        union_probability: impl Into<String>,
        completeness: BuildV2CompletenessPayload,
    ) -> Result<Self, BuildV2ProductPayloadError> {
        Self::finish(Self {
            kind: BuildV2PayloadKind::Probability,
            capability_id: capability_id.into(),
            result_contract: result_contract.into(),
            input_identity_sha256: input_identity_sha256.into(),
            evaluation_identity_sha256: Some(evaluation_identity_sha256.into()),
            replay_basis,
            objective: objective.into(),
            score_profile: None,
            initial_b2b: None,
            score_accuracy: None,
            profile_specific_exact: None,
            score_equality_basis: None,
            informational_attack_basis: None,
            source_candidate_count: source_candidate_count.into(),
            reachable_candidate_count: reachable_candidate_count.into(),
            selected_candidate_count: None,
            pattern_count: pattern_count.into(),
            covered_pattern_count: Some(covered_pattern_count.into()),
            required_pattern_count: None,
            union_probability: Some(union_probability.into()),
            b2b_preservation_required: None,
            candidates: Vec::new(),
            canonical_candidate_keys: Vec::new(),
            winners: Vec::new(),
            completeness,
            page_source_available: false,
            page_source_identity_sha256: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_portfolio(
        capability_id: impl Into<String>,
        result_contract: impl Into<String>,
        input_identity_sha256: impl Into<String>,
        replay_basis: Option<String>,
        objective: impl Into<String>,
        source_candidate_count: impl Into<String>,
        reachable_candidate_count: impl Into<String>,
        selected_candidate_count: impl Into<String>,
        pattern_count: impl Into<String>,
        required_pattern_count: impl Into<String>,
        union_probability: impl Into<String>,
        canonical_candidate_keys: Vec<String>,
        completeness: BuildV2CompletenessPayload,
        page_source_identity_sha256: impl Into<String>,
    ) -> Result<Self, BuildV2ProductPayloadError> {
        Self::finish(Self {
            kind: BuildV2PayloadKind::Portfolio,
            capability_id: capability_id.into(),
            result_contract: result_contract.into(),
            input_identity_sha256: input_identity_sha256.into(),
            evaluation_identity_sha256: None,
            replay_basis,
            objective: objective.into(),
            score_profile: None,
            initial_b2b: None,
            score_accuracy: None,
            profile_specific_exact: None,
            score_equality_basis: None,
            informational_attack_basis: None,
            source_candidate_count: source_candidate_count.into(),
            reachable_candidate_count: reachable_candidate_count.into(),
            selected_candidate_count: Some(selected_candidate_count.into()),
            pattern_count: pattern_count.into(),
            covered_pattern_count: None,
            required_pattern_count: Some(required_pattern_count.into()),
            union_probability: Some(union_probability.into()),
            b2b_preservation_required: None,
            candidates: Vec::new(),
            canonical_candidate_keys,
            winners: Vec::new(),
            completeness,
            page_source_available: true,
            page_source_identity_sha256: Some(page_source_identity_sha256.into()),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_score_portfolio(
        capability_id: impl Into<String>,
        result_contract: impl Into<String>,
        input_identity_sha256: impl Into<String>,
        score_profile: impl Into<String>,
        initial_b2b: impl Into<String>,
        score_accuracy: impl Into<String>,
        profile_specific_exact: bool,
        score_equality_basis: impl Into<String>,
        informational_attack_basis: impl Into<String>,
        source_candidate_count: impl Into<String>,
        reachable_candidate_count: impl Into<String>,
        selected_candidate_count: impl Into<String>,
        pattern_count: impl Into<String>,
        required_pattern_count: impl Into<String>,
        canonical_candidate_keys: Vec<String>,
        winners: Vec<BuildV2ScoreWinnerPayload>,
        completeness: BuildV2CompletenessPayload,
        page_source_identity_sha256: impl Into<String>,
    ) -> Result<Self, BuildV2ProductPayloadError> {
        Self::finish(Self {
            kind: BuildV2PayloadKind::ScorePortfolio,
            capability_id: capability_id.into(),
            result_contract: result_contract.into(),
            input_identity_sha256: input_identity_sha256.into(),
            evaluation_identity_sha256: None,
            replay_basis: None,
            objective: "max-score-cover".to_owned(),
            score_profile: Some(score_profile.into()),
            initial_b2b: Some(initial_b2b.into()),
            score_accuracy: Some(score_accuracy.into()),
            profile_specific_exact: Some(profile_specific_exact),
            score_equality_basis: Some(score_equality_basis.into()),
            informational_attack_basis: Some(informational_attack_basis.into()),
            source_candidate_count: source_candidate_count.into(),
            reachable_candidate_count: reachable_candidate_count.into(),
            selected_candidate_count: Some(selected_candidate_count.into()),
            pattern_count: pattern_count.into(),
            covered_pattern_count: None,
            required_pattern_count: Some(required_pattern_count.into()),
            union_probability: None,
            b2b_preservation_required: None,
            candidates: Vec::new(),
            canonical_candidate_keys,
            winners,
            completeness,
            page_source_available: true,
            page_source_identity_sha256: Some(page_source_identity_sha256.into()),
        })
    }

    /// Allocation-free owned-parts seam for a boundary that has already
    /// created and memory-authorized every retained string and vector. The
    /// supplied objective is preserved instead of allocating the fixed score
    /// objective inside `try_score_portfolio`; validation performs no heap
    /// allocation.
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_owned_memory_authorized_parts(
        kind: BuildV2PayloadKind,
        capability_id: String,
        result_contract: String,
        input_identity_sha256: String,
        evaluation_identity_sha256: Option<String>,
        replay_basis: Option<String>,
        objective: String,
        score_profile: Option<String>,
        initial_b2b: Option<String>,
        score_accuracy: Option<String>,
        profile_specific_exact: Option<bool>,
        score_equality_basis: Option<String>,
        informational_attack_basis: Option<String>,
        source_candidate_count: String,
        reachable_candidate_count: String,
        selected_candidate_count: Option<String>,
        pattern_count: String,
        covered_pattern_count: Option<String>,
        required_pattern_count: Option<String>,
        union_probability: Option<String>,
        b2b_preservation_required: Option<bool>,
        candidates: Vec<BuildV2CandidateCoveragePayload>,
        canonical_candidate_keys: Vec<String>,
        winners: Vec<BuildV2ScoreWinnerPayload>,
        completeness: BuildV2CompletenessPayload,
        page_source_available: bool,
        page_source_identity_sha256: Option<String>,
    ) -> Result<Self, BuildV2ProductPayloadError> {
        Self::finish(Self {
            kind,
            capability_id,
            result_contract,
            input_identity_sha256,
            evaluation_identity_sha256,
            replay_basis,
            objective,
            score_profile,
            initial_b2b,
            score_accuracy,
            profile_specific_exact,
            score_equality_basis,
            informational_attack_basis,
            source_candidate_count,
            reachable_candidate_count,
            selected_candidate_count,
            pattern_count,
            covered_pattern_count,
            required_pattern_count,
            union_probability,
            b2b_preservation_required,
            candidates,
            canonical_candidate_keys,
            winners,
            completeness,
            page_source_available,
            page_source_identity_sha256,
        })
    }

    fn finish(value: Self) -> Result<Self, BuildV2ProductPayloadError> {
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), BuildV2ProductPayloadError> {
        let contract_pair_valid = match self.kind {
            BuildV2PayloadKind::CandidateFamily => matches!(
                (self.capability_id.as_str(), self.result_contract.as_str()),
                ("build.congruent", "build-congruence-family.v1")
                    | ("build.evaluate.cover", "build-supplied-coverage.v1")
                    | ("build.evaluate.b2b-cover", "build-supplied-b2b-coverage.v1")
            ),
            BuildV2PayloadKind::Probability => matches!(
                (self.capability_id.as_str(), self.result_contract.as_str()),
                (
                    "build.setup-cover-percent",
                    "build-setup-cover-probability.v1"
                ) | (
                    "build.evaluate.cover-percent",
                    "build-supplied-probability.v1"
                )
            ),
            BuildV2PayloadKind::Portfolio => matches!(
                (self.capability_id.as_str(), self.result_contract.as_str()),
                ("build.congruent-cover", "build-congruence-coverage.v1")
                    | ("build.setup-cover", "build-setup-cover.v1")
                    | ("build.evaluate.minimals", "build-supplied-minimum-cover.v1")
            ),
            BuildV2PayloadKind::ScorePortfolio => matches!(
                (self.capability_id.as_str(), self.result_contract.as_str()),
                ("build.setup-cover-score", "build-setup-cover-score.v1")
                    | ("build.evaluate.score", "build-supplied-score.v1")
            ),
        };
        if !contract_pair_valid {
            return Err(BuildV2ProductPayloadError::CapabilityContractMismatch);
        }
        if !sha256_text(&self.input_identity_sha256) {
            return Err(BuildV2ProductPayloadError::IdentityInvalid(
                "input_identity_sha256",
            ));
        }
        if self
            .evaluation_identity_sha256
            .as_deref()
            .is_some_and(|value| !sha256_text(value))
        {
            return Err(BuildV2ProductPayloadError::IdentityInvalid(
                "evaluation_identity_sha256",
            ));
        }
        if self.replay_basis.as_deref().is_some_and(str::is_empty) {
            return Err(BuildV2ProductPayloadError::ShapeInvalid);
        }
        self.validate_objective()?;
        for (name, text) in [
            (
                "source_candidate_count",
                self.source_candidate_count.as_str(),
            ),
            (
                "reachable_candidate_count",
                self.reachable_candidate_count.as_str(),
            ),
            ("pattern_count", self.pattern_count.as_str()),
        ] {
            if !canonical_decimal(text) {
                return Err(BuildV2ProductPayloadError::DecimalInvalid(name));
            }
        }
        for (name, value) in [
            (
                "selected_candidate_count",
                self.selected_candidate_count.as_deref(),
            ),
            (
                "covered_pattern_count",
                self.covered_pattern_count.as_deref(),
            ),
            (
                "required_pattern_count",
                self.required_pattern_count.as_deref(),
            ),
            ("initial_b2b", self.initial_b2b.as_deref()),
        ] {
            if value.is_some_and(|text| !canonical_decimal(text)) {
                return Err(BuildV2ProductPayloadError::DecimalInvalid(name));
            }
        }
        let source = decimal_u128(&self.source_candidate_count)?;
        let reachable = decimal_u128(&self.reachable_candidate_count)?;
        let patterns = decimal_u128(&self.pattern_count)?;
        if reachable > source {
            return Err(BuildV2ProductPayloadError::CountMismatch);
        }
        match self.kind {
            BuildV2PayloadKind::CandidateFamily => {
                if self.evaluation_identity_sha256.is_none()
                    || self.covered_pattern_count.is_none()
                    || self.union_probability.is_none()
                    || self.selected_candidate_count.is_some()
                    || self.required_pattern_count.is_some()
                    || self.page_source_available
                    || self.page_source_identity_sha256.is_some()
                    || !self.canonical_candidate_keys.is_empty()
                    || !self.winners.is_empty()
                    || self.score_profile.is_some()
                {
                    return Err(BuildV2ProductPayloadError::ShapeInvalid);
                }
                if source != self.candidates.len() as u128 {
                    return Err(BuildV2ProductPayloadError::CountMismatch);
                }
                let mut positive = 0_u128;
                let mut previous = None;
                for row in &self.candidates {
                    let count = decimal_u128(row.covered_pattern_count())?;
                    if count > patterns {
                        return Err(BuildV2ProductPayloadError::CountMismatch);
                    }
                    positive += u128::from(count > 0);
                    if previous.is_some_and(|key| key >= row.candidate_key()) {
                        return Err(BuildV2ProductPayloadError::CandidateKeyInvalid);
                    }
                    previous = Some(row.candidate_key());
                }
                if positive != reachable {
                    return Err(BuildV2ProductPayloadError::CountMismatch);
                }
                let expected_b2b = match self.capability_id.as_str() {
                    "build.congruent" => None,
                    "build.evaluate.cover" => Some(false),
                    "build.evaluate.b2b-cover" => Some(true),
                    _ => unreachable!("contract pair checked above"),
                };
                if self.b2b_preservation_required != expected_b2b
                    || !self.completeness.replay_complete()
                    || self.completeness.exact_minimum_proven()
                    || self.completeness.score_evidence_complete()
                {
                    return Err(BuildV2ProductPayloadError::CompletenessInvalid);
                }
                self.validate_coverage(patterns)?;
            }
            BuildV2PayloadKind::Probability => {
                if self.evaluation_identity_sha256.is_none()
                    || self.covered_pattern_count.is_none()
                    || self.union_probability.is_none()
                    || self.selected_candidate_count.is_some()
                    || self.required_pattern_count.is_some()
                    || !self.candidates.is_empty()
                    || !self.canonical_candidate_keys.is_empty()
                    || !self.winners.is_empty()
                    || self.page_source_available
                    || self.page_source_identity_sha256.is_some()
                    || self.b2b_preservation_required.is_some()
                    || !self.completeness.replay_complete()
                    || self.completeness.exact_minimum_proven()
                    || self.completeness.score_evidence_complete()
                {
                    return Err(BuildV2ProductPayloadError::ShapeInvalid);
                }
                self.validate_coverage(patterns)?;
            }
            BuildV2PayloadKind::Portfolio => {
                self.validate_portfolio_shape(source, reachable, patterns, false)?;
                if !self.winners.is_empty()
                    || self.score_profile.is_some()
                    || !self.completeness.portfolio_complete()
                    || self.completeness.score_evidence_complete()
                {
                    return Err(BuildV2ProductPayloadError::CompletenessInvalid);
                }
                if self.union_probability.as_deref().is_none_or(str::is_empty) {
                    return Err(BuildV2ProductPayloadError::ProbabilityInvalid);
                }
            }
            BuildV2PayloadKind::ScorePortfolio => {
                self.validate_portfolio_shape(source, reachable, patterns, true)?;
                if self.union_probability.is_some()
                    || self.score_profile.as_deref().is_none_or(str::is_empty)
                    || self.score_accuracy.as_deref().is_none_or(str::is_empty)
                    || self.profile_specific_exact.is_none()
                    || self.score_equality_basis.as_deref() != Some("score-only")
                    || self.informational_attack_basis.as_deref()
                        != Some("canonical-equal-score-trace")
                    || !self.completeness.score_portfolio_complete()
                {
                    return Err(BuildV2ProductPayloadError::ScoreSemanticsInvalid);
                }
                let required = decimal_u128(
                    self.required_pattern_count
                        .as_deref()
                        .ok_or(BuildV2ProductPayloadError::ShapeInvalid)?,
                )?;
                if required != self.winners.len() as u128 {
                    return Err(BuildV2ProductPayloadError::CountMismatch);
                }
                let mut previous_pattern = None;
                for winner in &self.winners {
                    let pattern = decimal_u128(winner.pattern_id())?;
                    if pattern >= patterns
                        || previous_pattern.is_some_and(|previous| previous >= pattern)
                        || self
                            .canonical_candidate_keys
                            .binary_search_by(|candidate_key| {
                                candidate_key.as_str().cmp(winner.candidate_key())
                            })
                            .is_err()
                    {
                        return Err(BuildV2ProductPayloadError::CountMismatch);
                    }
                    previous_pattern = Some(pattern);
                }
            }
        }
        Ok(())
    }

    fn validate_objective(&self) -> Result<(), BuildV2ProductPayloadError> {
        let valid = match self.kind {
            BuildV2PayloadKind::CandidateFamily => match self.capability_id.as_str() {
                "build.congruent" => matches!(self.objective.as_str(), "all" | "unique"),
                "build.evaluate.cover" | "build.evaluate.b2b-cover" => self.objective == "all",
                _ => false,
            },
            BuildV2PayloadKind::Probability => match self.capability_id.as_str() {
                "build.setup-cover-percent" => {
                    matches!(self.objective.as_str(), "all" | "unique")
                }
                "build.evaluate.cover-percent" => self.objective == "unique",
                _ => false,
            },
            BuildV2PayloadKind::Portfolio => match self.capability_id.as_str() {
                "build.evaluate.minimals" => self.objective == "min-cover",
                _ => matches!(
                    self.objective.as_str(),
                    "min-cover" | "max-probability-minimum"
                ),
            },
            BuildV2PayloadKind::ScorePortfolio => self.objective == "max-score-cover",
        };
        valid
            .then_some(())
            .ok_or(BuildV2ProductPayloadError::ObjectiveInvalid)
    }

    fn validate_coverage(&self, patterns: u128) -> Result<(), BuildV2ProductPayloadError> {
        let covered = decimal_u128(
            self.covered_pattern_count
                .as_deref()
                .ok_or(BuildV2ProductPayloadError::ShapeInvalid)?,
        )?;
        if covered > patterns || self.union_probability.as_deref().is_none_or(str::is_empty) {
            return Err(BuildV2ProductPayloadError::ProbabilityInvalid);
        }
        Ok(())
    }

    fn validate_portfolio_shape(
        &self,
        _source: u128,
        reachable: u128,
        patterns: u128,
        score: bool,
    ) -> Result<(), BuildV2ProductPayloadError> {
        if self.evaluation_identity_sha256.is_some()
            || self.covered_pattern_count.is_some()
            || self.b2b_preservation_required.is_some()
            || !self.candidates.is_empty()
            || !self.page_source_available
            || self
                .page_source_identity_sha256
                .as_deref()
                .is_none_or(|value| !sha256_text(value))
        {
            return Err(BuildV2ProductPayloadError::PageSourceInvalid);
        }
        let selected = decimal_u128(
            self.selected_candidate_count
                .as_deref()
                .ok_or(BuildV2ProductPayloadError::ShapeInvalid)?,
        )?;
        let required = decimal_u128(
            self.required_pattern_count
                .as_deref()
                .ok_or(BuildV2ProductPayloadError::ShapeInvalid)?,
        )?;
        if selected == 0
            || selected > reachable
            || required > patterns
            || selected != self.canonical_candidate_keys.len() as u128
        {
            return Err(BuildV2ProductPayloadError::CountMismatch);
        }
        let mut previous = None;
        for key in &self.canonical_candidate_keys {
            if key.is_empty() || previous.is_some_and(|value| value >= key.as_str()) {
                return Err(BuildV2ProductPayloadError::CandidateKeyInvalid);
            }
            previous = Some(key.as_str());
        }
        if score != self.union_probability.is_none() {
            return Err(BuildV2ProductPayloadError::ShapeInvalid);
        }
        Ok(())
    }

    pub const fn kind(&self) -> BuildV2PayloadKind {
        self.kind
    }
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    pub fn result_contract(&self) -> &str {
        &self.result_contract
    }
    pub fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }
    pub fn evaluation_identity_sha256(&self) -> Option<&str> {
        self.evaluation_identity_sha256.as_deref()
    }
    pub fn replay_basis(&self) -> Option<&str> {
        self.replay_basis.as_deref()
    }
    pub fn objective(&self) -> &str {
        &self.objective
    }
    pub fn score_profile(&self) -> Option<&str> {
        self.score_profile.as_deref()
    }
    pub fn initial_b2b(&self) -> Option<&str> {
        self.initial_b2b.as_deref()
    }
    pub fn score_accuracy(&self) -> Option<&str> {
        self.score_accuracy.as_deref()
    }
    pub const fn profile_specific_exact(&self) -> Option<bool> {
        self.profile_specific_exact
    }
    pub fn score_equality_basis(&self) -> Option<&str> {
        self.score_equality_basis.as_deref()
    }
    pub fn informational_attack_basis(&self) -> Option<&str> {
        self.informational_attack_basis.as_deref()
    }
    pub fn source_candidate_count(&self) -> &str {
        &self.source_candidate_count
    }
    pub fn reachable_candidate_count(&self) -> &str {
        &self.reachable_candidate_count
    }
    pub fn selected_candidate_count(&self) -> Option<&str> {
        self.selected_candidate_count.as_deref()
    }
    pub fn pattern_count(&self) -> &str {
        &self.pattern_count
    }
    pub fn covered_pattern_count(&self) -> Option<&str> {
        self.covered_pattern_count.as_deref()
    }
    pub fn required_pattern_count(&self) -> Option<&str> {
        self.required_pattern_count.as_deref()
    }
    pub fn union_probability(&self) -> Option<&str> {
        self.union_probability.as_deref()
    }
    pub const fn b2b_preservation_required(&self) -> Option<bool> {
        self.b2b_preservation_required
    }
    pub fn candidates(&self) -> &[BuildV2CandidateCoveragePayload] {
        &self.candidates
    }
    pub fn canonical_candidate_keys(&self) -> &[String] {
        &self.canonical_candidate_keys
    }
    pub fn winners(&self) -> &[BuildV2ScoreWinnerPayload] {
        &self.winners
    }
    pub const fn completeness(&self) -> BuildV2CompletenessPayload {
        self.completeness
    }
    pub const fn page_source_available(&self) -> bool {
        self.page_source_available
    }
    pub fn page_source_identity_sha256(&self) -> Option<&str> {
        self.page_source_identity_sha256.as_deref()
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut total = 0_u128;
        for text in [
            Some(&self.capability_id),
            Some(&self.result_contract),
            Some(&self.input_identity_sha256),
            self.evaluation_identity_sha256.as_ref(),
            self.replay_basis.as_ref(),
            Some(&self.objective),
            self.score_profile.as_ref(),
            self.initial_b2b.as_ref(),
            self.score_accuracy.as_ref(),
            self.score_equality_basis.as_ref(),
            self.informational_attack_basis.as_ref(),
            Some(&self.source_candidate_count),
            Some(&self.reachable_candidate_count),
            self.selected_candidate_count.as_ref(),
            Some(&self.pattern_count),
            self.covered_pattern_count.as_ref(),
            self.required_pattern_count.as_ref(),
            self.union_probability.as_ref(),
            self.page_source_identity_sha256.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            total = total.checked_add(text.capacity() as u128)?;
        }
        total =
            total
                .checked_add((self.candidates.capacity() as u128).checked_mul(
                    core::mem::size_of::<BuildV2CandidateCoveragePayload>() as u128,
                )?)?;
        for row in &self.candidates {
            total = total.checked_add(row.checked_retained_capacity_bytes()?)?;
        }
        total = total.checked_add(
            (self.canonical_candidate_keys.capacity() as u128)
                .checked_mul(core::mem::size_of::<String>() as u128)?,
        )?;
        for key in &self.canonical_candidate_keys {
            total = total.checked_add(key.capacity() as u128)?;
        }
        total = total.checked_add(
            (self.winners.capacity() as u128)
                .checked_mul(core::mem::size_of::<BuildV2ScoreWinnerPayload>() as u128)?,
        )?;
        for winner in &self.winners {
            total = total.checked_add(winner.checked_retained_capacity_bytes()?)?;
        }
        Some(total)
    }
}

fn canonical_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn decimal_u128(value: &str) -> Result<u128, BuildV2ProductPayloadError> {
    if !canonical_decimal(value) {
        return Err(BuildV2ProductPayloadError::CountMismatch);
    }
    value
        .parse::<u128>()
        .map_err(|_| BuildV2ProductPayloadError::CountMismatch)
}

fn sha256_text(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(exact: bool, score: bool) -> BuildV2CompletenessPayload {
        BuildV2CompletenessPayload::new(true, true, true, true, true, exact, score)
    }

    #[test]
    fn score_portfolio_accepts_attack_only_as_informational_trace_data() {
        let winner =
            BuildV2ScoreWinnerPayload::try_new("0", "candidate-a", "1200", "4").expect("winner");
        let payload = BuildV2ProductPayload::try_score_portfolio(
            "build.evaluate.score",
            "build-supplied-score.v1",
            "a".repeat(64),
            "tetrio",
            "0",
            "basic-approximation",
            false,
            "score-only",
            "canonical-equal-score-trace",
            "1",
            "1",
            "1",
            "1",
            "1",
            vec!["candidate-a".to_owned()],
            vec![winner],
            complete(true, true),
            "b".repeat(64),
        )
        .expect("closed score portfolio");
        assert_eq!(payload.score_equality_basis(), Some("score-only"));
        assert_eq!(payload.winners()[0].informational_attack(), "4");

        assert_eq!(
            BuildV2ProductPayload::try_score_portfolio(
                "build.evaluate.score",
                "build-supplied-score.v1",
                "a".repeat(64),
                "tetrio",
                "0",
                "basic-approximation",
                false,
                "score-then-attack",
                "canonical-equal-score-trace",
                "1",
                "1",
                "1",
                "1",
                "1",
                vec!["candidate-a".to_owned()],
                vec![BuildV2ScoreWinnerPayload::try_new("0", "candidate-a", "1200", "4").unwrap()],
                complete(true, true),
                "b".repeat(64),
            ),
            Err(BuildV2ProductPayloadError::ScoreSemanticsInvalid)
        );
    }

    #[test]
    fn portfolio_requires_an_exact_complete_page_source() {
        let payload = BuildV2ProductPayload::try_portfolio(
            "build.evaluate.minimals",
            "build-supplied-minimum-cover.v1",
            "a".repeat(64),
            Some("normalized-colored-solution-replay.v1".to_owned()),
            "min-cover",
            "2",
            "2",
            "1",
            "2",
            "2",
            "100%",
            vec!["candidate-a".to_owned()],
            complete(true, false),
            "b".repeat(64),
        )
        .expect("closed portfolio");
        assert!(payload.page_source_available());
        assert_eq!(payload.canonical_candidate_keys(), ["candidate-a"]);
    }
}
