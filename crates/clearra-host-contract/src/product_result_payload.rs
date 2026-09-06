//! Closed public payloads for typed product results.
//! SRP rationale: this module has one change reason: the finite serialized host contract for typed product results.
//!
//! The host contract owns only finite, serializable DTOs. Live solver stores
//! stay behind a runtime-specific page handle; `page_handle_available` tells a
//! host whether that separately owned handle exists without exposing pointers
//! or implementation-private checkpoints.

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProductResultPayload {
    contract: String,
    result_kind: String,
    content: ProductResultPayloadContent,
}

impl ProductResultPayload {
    pub fn new(
        contract: impl Into<String>,
        result_kind: impl Into<String>,
        content: ProductResultPayloadContent,
    ) -> Self {
        Self::from_owned_memory_authorized_parts(contract.into(), result_kind.into(), content)
    }

    /// Allocation-free owned-parts seam for a boundary that has already
    /// created and memory-authorized both retained strings and the content.
    pub fn from_owned_memory_authorized_parts(
        contract: String,
        result_kind: String,
        content: ProductResultPayloadContent,
    ) -> Self {
        Self {
            contract,
            result_kind,
            content,
        }
    }

    pub fn contract(&self) -> &str {
        &self.contract
    }

    pub fn result_kind(&self) -> &str {
        &self.result_kind
    }

    pub const fn content(&self) -> &ProductResultPayloadContent {
        &self.content
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.contract.capacity() as u128)
            .checked_add(self.result_kind.capacity() as u128)?
            .checked_add(self.content.checked_retained_capacity_bytes()?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "payload_kind", content = "payload", rename_all = "kebab-case")
)]
pub enum ProductResultPayloadContent {
    CoveragePortfolio(CoveragePortfolioPagePayload),
    BuildCoveragePortfolioV2(BuildCoveragePortfolioV2Payload),
    BuildSetupFamilyV1(BuildSetupFamilyV1Payload),
    BuildV2(crate::BuildV2ProductPayload),
    SetupRankedFamily(SetupRankedFamilyPayload),
    SetupScoreRanking(SetupScoreRankingPayload),
    SpinStructureFamily(SpinStructureFamilyPayload),
    PcScoreFieldSummary(PcScoreFieldSummaryPayload),
    ScorePatternWinnerFamily(ScorePatternWinnerFamilyPayload),
    PcPathFamily(PcPathFamilyPayload),
    BuildPathFamily(BuildPathFamilyPayload),
    PcSaveGroups(PcSaveGroupsPayload),
    PcBestSave(PcBestSavePayload),
    ParityReportPage(crate::ParityReportPagePayload),
    FieldDocument(crate::FieldDocumentPayload),
    FieldDocumentSet(crate::FieldDocumentSetPayload),
    RenderArtifact(crate::RenderArtifactPayload),
}

impl ProductResultPayloadContent {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::CoveragePortfolio(payload) => payload.checked_retained_capacity_bytes(),
            Self::BuildCoveragePortfolioV2(payload) => payload.checked_retained_capacity_bytes(),
            Self::BuildSetupFamilyV1(payload) => payload.checked_retained_capacity_bytes(),
            Self::BuildV2(payload) => payload.checked_retained_capacity_bytes(),
            Self::SetupRankedFamily(payload) => payload.checked_retained_capacity_bytes(),
            Self::SetupScoreRanking(payload) => payload.checked_retained_capacity_bytes(),
            Self::SpinStructureFamily(payload) => payload.checked_retained_capacity_bytes(),
            Self::PcScoreFieldSummary(payload) => payload.checked_retained_capacity_bytes(),
            Self::ScorePatternWinnerFamily(payload) => payload.checked_retained_capacity_bytes(),
            Self::PcPathFamily(payload) => payload.checked_retained_capacity_bytes(),
            Self::BuildPathFamily(payload) => payload.checked_retained_capacity_bytes(),
            Self::PcSaveGroups(payload) => payload.checked_retained_capacity_bytes(),
            Self::PcBestSave(payload) => payload.checked_retained_capacity_bytes(),
            Self::ParityReportPage(payload) => payload.checked_retained_capacity_bytes(),
            Self::FieldDocument(payload) => payload.checked_retained_capacity_bytes(),
            Self::FieldDocumentSet(payload) => payload.checked_retained_capacity_bytes(),
            Self::RenderArtifact(payload) => payload.checked_retained_capacity_bytes(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcPathStepPayload {
    step_index: String,
    operation_id: String,
    active_piece: String,
    input_cursor: String,
    output_cursor: String,
    input_hold_piece: Option<String>,
    output_hold_piece: Option<String>,
    hold_decision: String,
    rotation: String,
    x: String,
    y: String,
    placement_mask: String,
    board_before_mask: String,
    board_after_placement_mask: String,
    board_after_line_clear_mask: String,
    cleared_row_mask: String,
    cleared_lines: String,
    line_clear_identity: String,
}

impl PcPathStepPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        step_index: impl Into<String>,
        operation_id: impl Into<String>,
        active_piece: impl Into<String>,
        input_cursor: impl Into<String>,
        output_cursor: impl Into<String>,
        input_hold_piece: Option<String>,
        output_hold_piece: Option<String>,
        hold_decision: impl Into<String>,
        rotation: impl Into<String>,
        x: impl Into<String>,
        y: impl Into<String>,
        placement_mask: impl Into<String>,
        board_before_mask: impl Into<String>,
        board_after_placement_mask: impl Into<String>,
        board_after_line_clear_mask: impl Into<String>,
        cleared_row_mask: impl Into<String>,
        cleared_lines: impl Into<String>,
        line_clear_identity: impl Into<String>,
    ) -> Self {
        Self {
            step_index: step_index.into(),
            operation_id: operation_id.into(),
            active_piece: active_piece.into(),
            input_cursor: input_cursor.into(),
            output_cursor: output_cursor.into(),
            input_hold_piece,
            output_hold_piece,
            hold_decision: hold_decision.into(),
            rotation: rotation.into(),
            x: x.into(),
            y: y.into(),
            placement_mask: placement_mask.into(),
            board_before_mask: board_before_mask.into(),
            board_after_placement_mask: board_after_placement_mask.into(),
            board_after_line_clear_mask: board_after_line_clear_mask.into(),
            cleared_row_mask: cleared_row_mask.into(),
            cleared_lines: cleared_lines.into(),
            line_clear_identity: line_clear_identity.into(),
        }
    }

    pub fn step_index(&self) -> &str {
        &self.step_index
    }
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }
    pub fn active_piece(&self) -> &str {
        &self.active_piece
    }
    pub fn input_cursor(&self) -> &str {
        &self.input_cursor
    }
    pub fn output_cursor(&self) -> &str {
        &self.output_cursor
    }
    pub fn input_hold_piece(&self) -> Option<&str> {
        self.input_hold_piece.as_deref()
    }
    pub fn output_hold_piece(&self) -> Option<&str> {
        self.output_hold_piece.as_deref()
    }
    pub fn hold_decision(&self) -> &str {
        &self.hold_decision
    }
    pub fn rotation(&self) -> &str {
        &self.rotation
    }
    pub fn x(&self) -> &str {
        &self.x
    }
    pub fn y(&self) -> &str {
        &self.y
    }
    pub fn placement_mask(&self) -> &str {
        &self.placement_mask
    }
    pub fn board_before_mask(&self) -> &str {
        &self.board_before_mask
    }
    pub fn board_after_placement_mask(&self) -> &str {
        &self.board_after_placement_mask
    }
    pub fn board_after_line_clear_mask(&self) -> &str {
        &self.board_after_line_clear_mask
    }
    pub fn cleared_row_mask(&self) -> &str {
        &self.cleared_row_mask
    }
    pub fn cleared_lines(&self) -> &str {
        &self.cleared_lines
    }
    pub fn line_clear_identity(&self) -> &str {
        &self.line_clear_identity
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.step_index.capacity(),
            self.operation_id.capacity(),
            self.active_piece.capacity(),
            self.input_cursor.capacity(),
            self.output_cursor.capacity(),
            self.hold_decision.capacity(),
            self.rotation.capacity(),
            self.x.capacity(),
            self.y.capacity(),
            self.placement_mask.capacity(),
            self.board_before_mask.capacity(),
            self.board_after_placement_mask.capacity(),
            self.board_after_line_clear_mask.capacity(),
            self.cleared_row_mask.capacity(),
            self.cleared_lines.capacity(),
            self.line_clear_identity.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?;
        if let Some(value) = &self.input_hold_piece {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        if let Some(value) = &self.output_hold_piece {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcPathWitnessPayload {
    candidate_id: String,
    producer_candidate_id: String,
    pattern_id: String,
    trace_identity: String,
    normalized_trace_key: String,
    consumed_piece_count: String,
    terminal_hold_piece: Option<String>,
    steps: Vec<PcPathStepPayload>,
}

impl PcPathWitnessPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        candidate_id: impl Into<String>,
        producer_candidate_id: impl Into<String>,
        pattern_id: impl Into<String>,
        trace_identity: impl Into<String>,
        normalized_trace_key: impl Into<String>,
        consumed_piece_count: impl Into<String>,
        terminal_hold_piece: Option<String>,
        steps: Vec<PcPathStepPayload>,
    ) -> Self {
        Self {
            candidate_id: candidate_id.into(),
            producer_candidate_id: producer_candidate_id.into(),
            pattern_id: pattern_id.into(),
            trace_identity: trace_identity.into(),
            normalized_trace_key: normalized_trace_key.into(),
            consumed_piece_count: consumed_piece_count.into(),
            terminal_hold_piece,
            steps,
        }
    }

    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }
    pub fn producer_candidate_id(&self) -> &str {
        &self.producer_candidate_id
    }
    pub fn pattern_id(&self) -> &str {
        &self.pattern_id
    }
    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }
    pub fn normalized_trace_key(&self) -> &str {
        &self.normalized_trace_key
    }
    pub fn consumed_piece_count(&self) -> &str {
        &self.consumed_piece_count
    }
    pub fn terminal_hold_piece(&self) -> Option<&str> {
        self.terminal_hold_piece.as_deref()
    }
    pub fn steps(&self) -> &[PcPathStepPayload] {
        &self.steps
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.candidate_id.capacity(),
            self.producer_candidate_id.capacity(),
            self.pattern_id.capacity(),
            self.trace_identity.capacity(),
            self.normalized_trace_key.capacity(),
            self.consumed_piece_count.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?;
        if let Some(value) = &self.terminal_hold_piece {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        bytes = bytes.checked_add(
            (self.steps.capacity() as u128)
                .checked_mul(core::mem::size_of::<PcPathStepPayload>() as u128)?,
        )?;
        for step in &self.steps {
            bytes = bytes.checked_add(step.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

/// Query-bound ordinary replay paging metadata; outer identities enumerate
/// canonical geometries, never optimal-portfolio alternatives.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcReplayPageMetadata {
    pub page_contract: String,
    pub page_source_available: bool,
    pub page_source_identity_sha256: String,
    pub geometry_count: String,
    pub geometry_page_number: String,
    pub candidate_id: String,
    pub geometry_witness_count: String,
    pub geometry_pattern_count: String,
    pub member_page_number: String,
    pub member_page_count: String,
}

impl PcReplayPageMetadata {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        [
            &self.page_contract,
            &self.page_source_identity_sha256,
            &self.geometry_count,
            &self.geometry_page_number,
            &self.candidate_id,
            &self.geometry_witness_count,
            &self.geometry_pattern_count,
            &self.member_page_number,
            &self.member_page_count,
        ]
        .iter()
        .try_fold(0_u128, |bytes, value| {
            bytes.checked_add(value.capacity() as u128)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcReplayPagePayload {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub metadata: PcReplayPageMetadata,
    pub witness_count: String,
    pub materialized_pattern_count: String,
    pub witnesses: Vec<PcPathWitnessPayload>,
}

impl PcReplayPagePayload {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let bytes = self
            .metadata
            .checked_retained_capacity_bytes()?
            .checked_add(self.witness_count.capacity() as u128)?
            .checked_add(self.materialized_pattern_count.capacity() as u128)?
            .checked_add(
                (self.witnesses.capacity() as u128)
                    .checked_mul(core::mem::size_of::<PcPathWitnessPayload>() as u128)?,
            )?;
        self.witnesses.iter().try_fold(bytes, |bytes, witness| {
            bytes.checked_add(witness.checked_retained_capacity_bytes()?)
        })
    }
}

/// Complete ordinary path family. This is not an optimal-portfolio tie set.
/// The canonical pair is producer-owned so bounded presenters never re-rank
/// otherwise equivalent witnesses.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcPathFamilyPayload {
    witness_contract: String,
    ordering: String,
    problem_id: String,
    materialized_pattern_count: String,
    witness_count: String,
    complete: bool,
    canonical_selection: String,
    canonical_witness: Option<PcPathWitnessPayload>,
    witnesses: Vec<PcPathWitnessPayload>,
    #[cfg_attr(feature = "serde", serde(flatten))]
    page_metadata: Option<PcReplayPageMetadata>,
}

impl PcPathFamilyPayload {
    // The constructor mirrors the versioned wire contract's independent fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        witness_contract: impl Into<String>,
        ordering: impl Into<String>,
        problem_id: impl Into<String>,
        materialized_pattern_count: impl Into<String>,
        witness_count: impl Into<String>,
        complete: bool,
        canonical_selection: impl Into<String>,
        canonical_witness: Option<PcPathWitnessPayload>,
        witnesses: Vec<PcPathWitnessPayload>,
    ) -> Self {
        Self {
            witness_contract: witness_contract.into(),
            ordering: ordering.into(),
            problem_id: problem_id.into(),
            materialized_pattern_count: materialized_pattern_count.into(),
            witness_count: witness_count.into(),
            complete,
            canonical_selection: canonical_selection.into(),
            canonical_witness,
            witnesses,
            page_metadata: None,
        }
    }

    pub fn witness_contract(&self) -> &str {
        &self.witness_contract
    }
    pub fn ordering(&self) -> &str {
        &self.ordering
    }
    pub fn problem_id(&self) -> &str {
        &self.problem_id
    }
    pub fn materialized_pattern_count(&self) -> &str {
        &self.materialized_pattern_count
    }
    pub fn witness_count(&self) -> &str {
        &self.witness_count
    }
    pub const fn complete(&self) -> bool {
        self.complete
    }
    pub fn canonical_selection(&self) -> &str {
        &self.canonical_selection
    }
    pub const fn canonical_witness(&self) -> Option<&PcPathWitnessPayload> {
        self.canonical_witness.as_ref()
    }
    pub fn witnesses(&self) -> &[PcPathWitnessPayload] {
        &self.witnesses
    }

    pub fn with_page_metadata(mut self, metadata: PcReplayPageMetadata) -> Self {
        self.page_metadata = Some(metadata);
        self
    }

    pub fn page_metadata(&self) -> Option<&PcReplayPageMetadata> {
        self.page_metadata.as_ref()
    }

    pub fn with_optional_page_metadata(mut self, metadata: Option<PcReplayPageMetadata>) -> Self {
        self.page_metadata = metadata;
        self
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.witness_contract.capacity(),
            self.ordering.capacity(),
            self.problem_id.capacity(),
            self.materialized_pattern_count.capacity(),
            self.witness_count.capacity(),
            self.canonical_selection.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?;
        if let Some(witness) = &self.canonical_witness {
            bytes = bytes.checked_add(witness.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(
            (self.witnesses.capacity() as u128)
                .checked_mul(core::mem::size_of::<PcPathWitnessPayload>() as u128)?,
        )?;
        for witness in &self.witnesses {
            bytes = bytes.checked_add(witness.checked_retained_capacity_bytes()?)?;
        }
        if let Some(metadata) = &self.page_metadata {
            bytes = bytes.checked_add(metadata.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

/// Complete Build-probability replay family.
///
/// Unlike `PcPathFamilyPayload`, a Build replay is allowed to terminate on a
/// non-empty board.  The requested terminal is therefore carried as an
/// authoritative mask instead of letting a presenter infer PC semantics from
/// the shared witness shape.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildPathFamilyPayload {
    witness_contract: String,
    ordering: String,
    problem_id: String,
    target_terminal_board_mask: String,
    #[cfg_attr(feature = "serde", serde(default))]
    mirrored_terminal_board_mask: Option<String>,
    materialized_pattern_count: String,
    witness_count: String,
    complete: bool,
    canonical_selection: String,
    canonical_witness: Option<PcPathWitnessPayload>,
    witnesses: Vec<PcPathWitnessPayload>,
}

impl BuildPathFamilyPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        witness_contract: impl Into<String>,
        ordering: impl Into<String>,
        problem_id: impl Into<String>,
        target_terminal_board_mask: impl Into<String>,
        materialized_pattern_count: impl Into<String>,
        witness_count: impl Into<String>,
        complete: bool,
        canonical_selection: impl Into<String>,
        canonical_witness: Option<PcPathWitnessPayload>,
        witnesses: Vec<PcPathWitnessPayload>,
    ) -> Self {
        Self {
            witness_contract: witness_contract.into(),
            ordering: ordering.into(),
            problem_id: problem_id.into(),
            target_terminal_board_mask: target_terminal_board_mask.into(),
            mirrored_terminal_board_mask: None,
            materialized_pattern_count: materialized_pattern_count.into(),
            witness_count: witness_count.into(),
            complete,
            canonical_selection: canonical_selection.into(),
            canonical_witness,
            witnesses,
        }
    }

    pub fn witness_contract(&self) -> &str {
        &self.witness_contract
    }
    pub fn ordering(&self) -> &str {
        &self.ordering
    }
    pub fn problem_id(&self) -> &str {
        &self.problem_id
    }
    pub fn target_terminal_board_mask(&self) -> &str {
        &self.target_terminal_board_mask
    }
    pub fn with_mirrored_terminal_board_mask(mut self, mask: Option<String>) -> Self {
        self.mirrored_terminal_board_mask = mask;
        self
    }
    pub fn mirrored_terminal_board_mask(&self) -> Option<&str> {
        self.mirrored_terminal_board_mask.as_deref()
    }
    pub fn materialized_pattern_count(&self) -> &str {
        &self.materialized_pattern_count
    }
    pub fn witness_count(&self) -> &str {
        &self.witness_count
    }
    pub const fn complete(&self) -> bool {
        self.complete
    }
    pub fn canonical_selection(&self) -> &str {
        &self.canonical_selection
    }
    pub const fn canonical_witness(&self) -> Option<&PcPathWitnessPayload> {
        self.canonical_witness.as_ref()
    }
    pub fn witnesses(&self) -> &[PcPathWitnessPayload] {
        &self.witnesses
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.witness_contract.capacity(),
            self.ordering.capacity(),
            self.problem_id.capacity(),
            self.target_terminal_board_mask.capacity(),
            self.mirrored_terminal_board_mask
                .as_ref()
                .map_or(0, String::capacity),
            self.materialized_pattern_count.capacity(),
            self.witness_count.capacity(),
            self.canonical_selection.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?;
        if let Some(witness) = &self.canonical_witness {
            bytes = bytes.checked_add(witness.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(
            (self.witnesses.capacity() as u128)
                .checked_mul(core::mem::size_of::<PcPathWitnessPayload>() as u128)?,
        )?;
        for witness in &self.witnesses {
            bytes = bytes.checked_add(witness.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupScoreRankingPayloadError {
    SchemaInvalid,
    IdentityInvalid,
    OrderingInvalid,
    DecimalInvalid(&'static str),
    ProbabilityInvalid(&'static str),
    ScoreInvalid,
    CountMismatch,
    CandidateInvalid,
    CandidateDuplicated,
    CompletenessInvalid,
}

/// One member of the normal `setup.score` ranked family.
///
/// Score equality never creates a portfolio alternative. The stable
/// canonical candidate ID is the final order coordinate and no tie metadata
/// exists in this DTO.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SetupScoreCandidatePayload {
    rank: String,
    candidate_id: String,
    completed_board_mask: String,
    setup_covered_pattern_count: String,
    setup_covered_probability: String,
    continuation_probability: String,
    unconditional_expected_score: String,
}

impl SetupScoreCandidatePayload {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        rank: impl Into<String>,
        candidate_id: impl Into<String>,
        completed_board_mask: impl Into<String>,
        setup_covered_pattern_count: impl Into<String>,
        setup_covered_probability: impl Into<String>,
        continuation_probability: impl Into<String>,
        unconditional_expected_score: impl Into<String>,
    ) -> Result<Self, SetupScoreRankingPayloadError> {
        let value = Self {
            rank: rank.into(),
            candidate_id: candidate_id.into(),
            completed_board_mask: completed_board_mask.into(),
            setup_covered_pattern_count: setup_covered_pattern_count.into(),
            setup_covered_probability: setup_covered_probability.into(),
            continuation_probability: continuation_probability.into(),
            unconditional_expected_score: unconditional_expected_score.into(),
        };
        if !canonical_decimal(&value.rank) || value.rank == "0" {
            return Err(SetupScoreRankingPayloadError::DecimalInvalid("rank"));
        }
        if value.candidate_id.is_empty()
            || value.candidate_id.trim() != value.candidate_id
            || !value.completed_board_mask.starts_with("0x")
            || u64::from_str_radix(&value.completed_board_mask[2..], 16).is_err()
        {
            return Err(SetupScoreRankingPayloadError::CandidateInvalid);
        }
        if !canonical_decimal(&value.setup_covered_pattern_count) {
            return Err(SetupScoreRankingPayloadError::DecimalInvalid(
                "setup_covered_pattern_count",
            ));
        }
        for (name, number) in [
            (
                "setup_covered_probability",
                value.setup_covered_probability.as_str(),
            ),
            (
                "continuation_probability",
                value.continuation_probability.as_str(),
            ),
        ] {
            if number
                .parse::<f64>()
                .ok()
                .is_none_or(|number| !number.is_finite() || !(0.0..=1.0).contains(&number))
            {
                return Err(SetupScoreRankingPayloadError::ProbabilityInvalid(name));
            }
        }
        if value
            .unconditional_expected_score
            .parse::<f64>()
            .ok()
            .is_none_or(|number| !number.is_finite() || number < 0.0)
        {
            return Err(SetupScoreRankingPayloadError::ScoreInvalid);
        }
        Ok(value)
    }

    pub fn rank(&self) -> &str {
        &self.rank
    }
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }
    pub fn completed_board_mask(&self) -> &str {
        &self.completed_board_mask
    }
    pub fn setup_covered_pattern_count(&self) -> &str {
        &self.setup_covered_pattern_count
    }
    pub fn setup_covered_probability(&self) -> &str {
        &self.setup_covered_probability
    }
    pub fn continuation_probability(&self) -> &str {
        &self.continuation_probability
    }
    pub fn unconditional_expected_score(&self) -> &str {
        &self.unconditional_expected_score
    }

    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        [
            &self.rank,
            &self.candidate_id,
            &self.completed_board_mask,
            &self.setup_covered_pattern_count,
            &self.setup_covered_probability,
            &self.continuation_probability,
            &self.unconditional_expected_score,
        ]
        .into_iter()
        .try_fold(0_u128, |total, value| {
            total.checked_add(value.capacity() as u128)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SetupScoreRankingPayload {
    schema_id: String,
    input_identity_sha256: String,
    evaluation_identity_sha256: String,
    document_format: String,
    rule_profile: String,
    score_profile: String,
    initial_b2b: String,
    ordering: String,
    source_page_count: String,
    candidate_count: String,
    setup_pattern_count: String,
    average_priority_score: String,
    complete: bool,
    candidates: Vec<SetupScoreCandidatePayload>,
}

impl SetupScoreRankingPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        schema_id: impl Into<String>,
        input_identity_sha256: impl Into<String>,
        evaluation_identity_sha256: impl Into<String>,
        document_format: impl Into<String>,
        rule_profile: impl Into<String>,
        score_profile: impl Into<String>,
        initial_b2b: impl Into<String>,
        ordering: impl Into<String>,
        source_page_count: impl Into<String>,
        candidate_count: impl Into<String>,
        setup_pattern_count: impl Into<String>,
        average_priority_score: impl Into<String>,
        complete: bool,
        candidates: Vec<SetupScoreCandidatePayload>,
    ) -> Result<Self, SetupScoreRankingPayloadError> {
        let value = Self {
            schema_id: schema_id.into(),
            input_identity_sha256: input_identity_sha256.into(),
            evaluation_identity_sha256: evaluation_identity_sha256.into(),
            document_format: document_format.into(),
            rule_profile: rule_profile.into(),
            score_profile: score_profile.into(),
            initial_b2b: initial_b2b.into(),
            ordering: ordering.into(),
            source_page_count: source_page_count.into(),
            candidate_count: candidate_count.into(),
            setup_pattern_count: setup_pattern_count.into(),
            average_priority_score: average_priority_score.into(),
            complete,
            candidates,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), SetupScoreRankingPayloadError> {
        if self.schema_id != "setup-score-ranking.v1" {
            return Err(SetupScoreRankingPayloadError::SchemaInvalid);
        }
        if !sha256_text(&self.input_identity_sha256)
            || !sha256_text(&self.evaluation_identity_sha256)
            || self.rule_profile.is_empty()
            || self.score_profile.is_empty()
            || !matches!(self.document_format.as_str(), "ctk3" | "fumen")
        {
            return Err(SetupScoreRankingPayloadError::IdentityInvalid);
        }
        if self.ordering != "unconditional-expected-score-descending-then-canonical-candidate-id" {
            return Err(SetupScoreRankingPayloadError::OrderingInvalid);
        }
        for (name, value) in [
            ("initial_b2b", self.initial_b2b.as_str()),
            ("source_page_count", self.source_page_count.as_str()),
            ("candidate_count", self.candidate_count.as_str()),
            ("setup_pattern_count", self.setup_pattern_count.as_str()),
        ] {
            if !canonical_decimal(value) {
                return Err(SetupScoreRankingPayloadError::DecimalInvalid(name));
            }
        }
        if !self.complete {
            return Err(SetupScoreRankingPayloadError::CompletenessInvalid);
        }
        let candidate_count = self
            .candidate_count
            .parse::<usize>()
            .map_err(|_| SetupScoreRankingPayloadError::CountMismatch)?;
        let source_page_count = self
            .source_page_count
            .parse::<usize>()
            .map_err(|_| SetupScoreRankingPayloadError::CountMismatch)?;
        let setup_pattern_count = self
            .setup_pattern_count
            .parse::<usize>()
            .map_err(|_| SetupScoreRankingPayloadError::CountMismatch)?;
        if candidate_count == 0
            || candidate_count != self.candidates.len()
            || source_page_count < candidate_count
            || setup_pattern_count == 0
        {
            return Err(SetupScoreRankingPayloadError::CountMismatch);
        }
        if self
            .average_priority_score
            .parse::<f64>()
            .ok()
            .is_none_or(|number| !number.is_finite() || number < 0.0)
        {
            return Err(SetupScoreRankingPayloadError::ScoreInvalid);
        }
        let mut previous_score = None;
        let mut previous_candidate_id: Option<&str> = None;
        let mut ids = std::collections::BTreeSet::new();
        for (index, candidate) in self.candidates.iter().enumerate() {
            if !ids.insert(candidate.candidate_id()) {
                return Err(SetupScoreRankingPayloadError::CandidateDuplicated);
            }
            if candidate.rank().parse::<usize>().ok() != Some(index + 1)
                || candidate
                    .setup_covered_pattern_count()
                    .parse::<usize>()
                    .ok()
                    .is_none_or(|count| count > setup_pattern_count)
            {
                return Err(SetupScoreRankingPayloadError::CandidateInvalid);
            }
            let score = candidate
                .unconditional_expected_score()
                .parse::<f64>()
                .map_err(|_| SetupScoreRankingPayloadError::ScoreInvalid)?;
            if previous_score.is_some_and(|previous: f64| previous < score)
                || previous_score == Some(score)
                    && previous_candidate_id
                        .is_some_and(|previous| previous >= candidate.candidate_id())
            {
                return Err(SetupScoreRankingPayloadError::OrderingInvalid);
            }
            previous_score = Some(score);
            previous_candidate_id = Some(candidate.candidate_id());
        }
        Ok(())
    }

    pub fn schema_id(&self) -> &str {
        &self.schema_id
    }
    pub fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }
    pub fn evaluation_identity_sha256(&self) -> &str {
        &self.evaluation_identity_sha256
    }
    pub fn document_format(&self) -> &str {
        &self.document_format
    }
    pub fn rule_profile(&self) -> &str {
        &self.rule_profile
    }
    pub fn score_profile(&self) -> &str {
        &self.score_profile
    }
    pub fn initial_b2b(&self) -> &str {
        &self.initial_b2b
    }
    pub fn ordering(&self) -> &str {
        &self.ordering
    }
    pub fn source_page_count(&self) -> &str {
        &self.source_page_count
    }
    pub fn candidate_count(&self) -> &str {
        &self.candidate_count
    }
    pub fn setup_pattern_count(&self) -> &str {
        &self.setup_pattern_count
    }
    pub fn average_priority_score(&self) -> &str {
        &self.average_priority_score
    }
    pub const fn complete(&self) -> bool {
        self.complete
    }
    pub fn candidates(&self) -> &[SetupScoreCandidatePayload] {
        &self.candidates
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut total = [
            &self.schema_id,
            &self.input_identity_sha256,
            &self.evaluation_identity_sha256,
            &self.document_format,
            &self.rule_profile,
            &self.score_profile,
            &self.initial_b2b,
            &self.ordering,
            &self.source_page_count,
            &self.candidate_count,
            &self.setup_pattern_count,
            &self.average_priority_score,
        ]
        .into_iter()
        .try_fold(0_u128, |total, value| {
            total.checked_add(value.capacity() as u128)
        })?;
        total = total.checked_add(
            (self.candidates.capacity() as u128)
                .checked_mul(core::mem::size_of::<SetupScoreCandidatePayload>() as u128)?,
        )?;
        for candidate in &self.candidates {
            total = total.checked_add(candidate.checked_retained_capacity_bytes()?)?;
        }
        Some(total)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RankedFamilyPayloadError {
    SchemaInvalid,
    IdentityInvalid(&'static str),
    OrderingInvalid,
    LengthPreferenceInvalid,
    CandidateInvalid(&'static str),
    CandidateCountInvalid,
    CandidateDuplicated,
    PartitionInvalid,
    PlacementCountInvalid,
    MinimumPlacementInvalid,
    GuaranteeMetadataInvalid,
    CompletenessInvalid,
}

/// Closed Host row for one member of a normal Setup ranked family.
///
/// This is deliberately not a portfolio row: equal-ranked Setup candidates
/// remain ordinary family members and therefore have no alternative index,
/// optimal-cardinality, enumeration, or snapshot fields.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SetupRankedCandidatePayload {
    candidate_id: String,
    condition_id: String,
    setup_id: String,
}

impl SetupRankedCandidatePayload {
    pub fn try_new(
        candidate_id: impl Into<String>,
        condition_id: impl Into<String>,
        setup_id: impl Into<String>,
    ) -> Result<Self, RankedFamilyPayloadError> {
        let value = Self {
            candidate_id: candidate_id.into(),
            condition_id: condition_id.into(),
            setup_id: setup_id.into(),
        };
        if !value.candidate_id.starts_with("setup-candidate.v1:") {
            return Err(RankedFamilyPayloadError::CandidateInvalid("candidate_id"));
        }
        if value.condition_id.is_empty() {
            return Err(RankedFamilyPayloadError::CandidateInvalid("condition_id"));
        }
        if value.setup_id.is_empty() {
            return Err(RankedFamilyPayloadError::CandidateInvalid("setup_id"));
        }
        Ok(value)
    }

    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    pub fn condition_id(&self) -> &str {
        &self.condition_id
    }

    pub fn setup_id(&self) -> &str {
        &self.setup_id
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.candidate_id.capacity() as u128)
            .checked_add(self.condition_id.capacity() as u128)?
            .checked_add(self.setup_id.capacity() as u128)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SetupRankedFamilyPayload {
    schema_id: String,
    query_identity_sha256: String,
    rule_profile: String,
    supply_identity_sha256: String,
    universe_identity_sha256: String,
    product_build: String,
    ordering: String,
    resolved_length_preference: String,
    candidate_count: String,
    candidates: Vec<SetupRankedCandidatePayload>,
}

impl SetupRankedFamilyPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        schema_id: impl Into<String>,
        query_identity_sha256: impl Into<String>,
        rule_profile: impl Into<String>,
        supply_identity_sha256: impl Into<String>,
        universe_identity_sha256: impl Into<String>,
        product_build: impl Into<String>,
        ordering: impl Into<String>,
        resolved_length_preference: impl Into<String>,
        candidate_count: impl Into<String>,
        candidates: Vec<SetupRankedCandidatePayload>,
    ) -> Result<Self, RankedFamilyPayloadError> {
        let value = Self {
            schema_id: schema_id.into(),
            query_identity_sha256: query_identity_sha256.into(),
            rule_profile: rule_profile.into(),
            supply_identity_sha256: supply_identity_sha256.into(),
            universe_identity_sha256: universe_identity_sha256.into(),
            product_build: product_build.into(),
            ordering: ordering.into(),
            resolved_length_preference: resolved_length_preference.into(),
            candidate_count: candidate_count.into(),
            candidates,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), RankedFamilyPayloadError> {
        if !matches!(
            self.schema_id.as_str(),
            "setup-joint-ranking.v2" | "setup-build-ranking.v2" | "setup-pc-ranking.v2"
        ) {
            return Err(RankedFamilyPayloadError::SchemaInvalid);
        }
        for (field, identity) in [
            ("query_identity_sha256", self.query_identity_sha256.as_str()),
            (
                "supply_identity_sha256",
                self.supply_identity_sha256.as_str(),
            ),
            (
                "universe_identity_sha256",
                self.universe_identity_sha256.as_str(),
            ),
        ] {
            if !sha256_text(identity) {
                return Err(RankedFamilyPayloadError::IdentityInvalid(field));
            }
        }
        if self.rule_profile.is_empty() || self.product_build.is_empty() {
            return Err(RankedFamilyPayloadError::IdentityInvalid(
                "profile_or_build",
            ));
        }
        if !matches!(
            self.ordering.as_str(),
            "joint-probability-descending"
                | "build-probability-descending"
                | "conditional-pc-probability-descending"
        ) {
            return Err(RankedFamilyPayloadError::OrderingInvalid);
        }
        if !matches!(
            self.resolved_length_preference.as_str(),
            "longer" | "shorter"
        ) {
            return Err(RankedFamilyPayloadError::LengthPreferenceInvalid);
        }
        let count = decimal_u128(&self.candidate_count)
            .ok_or(RankedFamilyPayloadError::CandidateCountInvalid)?;
        if count != self.candidates.len() as u128 {
            return Err(RankedFamilyPayloadError::CandidateCountInvalid);
        }
        for (index, candidate) in self.candidates.iter().enumerate() {
            if self.candidates[..index]
                .iter()
                .any(|previous| previous.candidate_id == candidate.candidate_id)
            {
                return Err(RankedFamilyPayloadError::CandidateDuplicated);
            }
        }
        Ok(())
    }

    pub fn schema_id(&self) -> &str {
        &self.schema_id
    }
    pub fn query_identity_sha256(&self) -> &str {
        &self.query_identity_sha256
    }
    pub fn rule_profile(&self) -> &str {
        &self.rule_profile
    }
    pub fn supply_identity_sha256(&self) -> &str {
        &self.supply_identity_sha256
    }
    pub fn universe_identity_sha256(&self) -> &str {
        &self.universe_identity_sha256
    }
    pub fn product_build(&self) -> &str {
        &self.product_build
    }
    pub fn ordering(&self) -> &str {
        &self.ordering
    }
    pub fn resolved_length_preference(&self) -> &str {
        &self.resolved_length_preference
    }
    pub fn candidate_count(&self) -> &str {
        &self.candidate_count
    }
    pub fn candidates(&self) -> &[SetupRankedCandidatePayload] {
        &self.candidates
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut total = [
            &self.schema_id,
            &self.query_identity_sha256,
            &self.rule_profile,
            &self.supply_identity_sha256,
            &self.universe_identity_sha256,
            &self.product_build,
            &self.ordering,
            &self.resolved_length_preference,
            &self.candidate_count,
        ]
        .into_iter()
        .try_fold(0_u128, |total, value| {
            total.checked_add(value.capacity() as u128)
        })?;
        total = total.checked_add(
            (self.candidates.capacity() as u128)
                .checked_mul(core::mem::size_of::<SetupRankedCandidatePayload>() as u128)?,
        )?;
        for candidate in &self.candidates {
            total = total.checked_add(candidate.checked_retained_capacity_bytes()?)?;
        }
        Some(total)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SpinStructureCandidatePayload {
    candidate_id: String,
    partition: String,
    placement_count: String,
}

impl SpinStructureCandidatePayload {
    pub fn try_new(
        candidate_id: impl Into<String>,
        partition: impl Into<String>,
        placement_count: impl Into<String>,
    ) -> Result<Self, RankedFamilyPayloadError> {
        let value = Self {
            candidate_id: candidate_id.into(),
            partition: partition.into(),
            placement_count: placement_count.into(),
        };
        if !value
            .candidate_id
            .starts_with("spin-structure-candidate.v1:")
        {
            return Err(RankedFamilyPayloadError::CandidateInvalid("candidate_id"));
        }
        if !matches!(value.partition.as_str(), "regular" | "mini") {
            return Err(RankedFamilyPayloadError::PartitionInvalid);
        }
        if decimal_u128(&value.placement_count).is_none() {
            return Err(RankedFamilyPayloadError::PlacementCountInvalid);
        }
        Ok(value)
    }

    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }
    pub fn partition(&self) -> &str {
        &self.partition
    }
    pub fn placement_count(&self) -> &str {
        &self.placement_count
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.candidate_id.capacity() as u128)
            .checked_add(self.partition.capacity() as u128)?
            .checked_add(self.placement_count.capacity() as u128)
    }
}

/// Closed summary for the normal `spin-structure.search` family.
///
/// Regular and Mini stay separate through `partition`; no portfolio/tie
/// metadata is admitted on this payload.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SpinStructureFamilyPayload {
    schema_id: String,
    query_identity_sha256: String,
    rule_profile: String,
    spin_profile: String,
    supply_identity_sha256: String,
    universe_identity_sha256: String,
    product_build: String,
    ordering: String,
    minimum_placements: Option<String>,
    guaranteed_final_piece: Option<String>,
    guarantee_basis: Option<String>,
    dependency_report_included: Option<bool>,
    dependency_relation: Option<String>,
    dependency_edge_count: Option<String>,
    regular_count: String,
    mini_count: String,
    candidate_count: String,
    complete: bool,
    candidates: Vec<SpinStructureCandidatePayload>,
}

impl SpinStructureFamilyPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        schema_id: impl Into<String>,
        query_identity_sha256: impl Into<String>,
        rule_profile: impl Into<String>,
        spin_profile: impl Into<String>,
        supply_identity_sha256: impl Into<String>,
        universe_identity_sha256: impl Into<String>,
        product_build: impl Into<String>,
        ordering: impl Into<String>,
        minimum_placements: Option<String>,
        guaranteed_final_piece: Option<String>,
        guarantee_basis: Option<String>,
        dependency_report_included: Option<bool>,
        dependency_relation: Option<String>,
        dependency_edge_count: Option<String>,
        regular_count: impl Into<String>,
        mini_count: impl Into<String>,
        candidate_count: impl Into<String>,
        complete: bool,
        candidates: Vec<SpinStructureCandidatePayload>,
    ) -> Result<Self, RankedFamilyPayloadError> {
        let value = Self {
            schema_id: schema_id.into(),
            query_identity_sha256: query_identity_sha256.into(),
            rule_profile: rule_profile.into(),
            spin_profile: spin_profile.into(),
            supply_identity_sha256: supply_identity_sha256.into(),
            universe_identity_sha256: universe_identity_sha256.into(),
            product_build: product_build.into(),
            ordering: ordering.into(),
            minimum_placements,
            guaranteed_final_piece,
            guarantee_basis,
            dependency_report_included,
            dependency_relation,
            dependency_edge_count,
            regular_count: regular_count.into(),
            mini_count: mini_count.into(),
            candidate_count: candidate_count.into(),
            complete,
            candidates,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), RankedFamilyPayloadError> {
        if !matches!(
            self.schema_id.as_str(),
            "spin-structure-family.v2" | "spin-structure-guaranteed.v1"
        ) {
            return Err(RankedFamilyPayloadError::SchemaInvalid);
        }
        for (field, identity) in [
            ("query_identity_sha256", self.query_identity_sha256.as_str()),
            (
                "supply_identity_sha256",
                self.supply_identity_sha256.as_str(),
            ),
            (
                "universe_identity_sha256",
                self.universe_identity_sha256.as_str(),
            ),
        ] {
            if !sha256_text(identity) {
                return Err(RankedFamilyPayloadError::IdentityInvalid(field));
            }
        }
        if self.rule_profile.is_empty()
            || self.spin_profile.is_empty()
            || self.product_build.is_empty()
        {
            return Err(RankedFamilyPayloadError::IdentityInvalid(
                "profile_or_build",
            ));
        }
        if self.ordering != "regular-then-mini-canonical-operation-key" {
            return Err(RankedFamilyPayloadError::OrderingInvalid);
        }
        match self.schema_id.as_str() {
            "spin-structure-family.v2" => {
                if self.guaranteed_final_piece.is_some()
                    || self.guarantee_basis.is_some()
                    || self.dependency_report_included.is_some()
                    || self.dependency_relation.is_some()
                    || self.dependency_edge_count.is_some()
                {
                    return Err(RankedFamilyPayloadError::GuaranteeMetadataInvalid);
                }
            }
            "spin-structure-guaranteed.v1" => {
                let final_piece = self.guaranteed_final_piece.as_deref();
                if !matches!(final_piece, Some("I" | "O" | "T" | "S" | "Z" | "J" | "L"))
                    || self.guarantee_basis.as_deref()
                        != Some("every-unique-non-target-piece-order-exact-replay-final-piece-last")
                {
                    return Err(RankedFamilyPayloadError::GuaranteeMetadataInvalid);
                }
                match self.dependency_report_included {
                    Some(true)
                        if self.dependency_relation.as_deref()
                            == Some("non-target-universal-precedence")
                            && self.dependency_edge_count.as_deref() == Some("0") => {}
                    Some(false)
                        if self.dependency_relation.is_none()
                            && self.dependency_edge_count.is_none() => {}
                    _ => return Err(RankedFamilyPayloadError::GuaranteeMetadataInvalid),
                }
            }
            _ => return Err(RankedFamilyPayloadError::SchemaInvalid),
        }
        if !self.complete {
            return Err(RankedFamilyPayloadError::CompletenessInvalid);
        }
        let regular = decimal_u128(&self.regular_count)
            .ok_or(RankedFamilyPayloadError::CandidateCountInvalid)?;
        let mini = decimal_u128(&self.mini_count)
            .ok_or(RankedFamilyPayloadError::CandidateCountInvalid)?;
        let total = decimal_u128(&self.candidate_count)
            .ok_or(RankedFamilyPayloadError::CandidateCountInvalid)?;
        if regular.checked_add(mini) != Some(total) || total != self.candidates.len() as u128 {
            return Err(RankedFamilyPayloadError::CandidateCountInvalid);
        }
        if self
            .candidates
            .iter()
            .filter(|row| row.partition == "regular")
            .count() as u128
            != regular
            || self
                .candidates
                .iter()
                .filter(|row| row.partition == "mini")
                .count() as u128
                != mini
            || self.candidates[..regular as usize]
                .iter()
                .any(|row| row.partition != "regular")
        {
            return Err(RankedFamilyPayloadError::PartitionInvalid);
        }
        for (index, candidate) in self.candidates.iter().enumerate() {
            if self.candidates[..index]
                .iter()
                .any(|previous| previous.candidate_id == candidate.candidate_id)
            {
                return Err(RankedFamilyPayloadError::CandidateDuplicated);
            }
        }
        let actual_minimum = self
            .candidates
            .iter()
            .filter_map(|candidate| decimal_u128(&candidate.placement_count))
            .min();
        let declared_minimum = match self.minimum_placements.as_deref() {
            Some(value) => {
                Some(decimal_u128(value).ok_or(RankedFamilyPayloadError::MinimumPlacementInvalid)?)
            }
            None => None,
        };
        if declared_minimum != actual_minimum {
            return Err(RankedFamilyPayloadError::MinimumPlacementInvalid);
        }
        Ok(())
    }

    pub fn schema_id(&self) -> &str {
        &self.schema_id
    }
    pub fn query_identity_sha256(&self) -> &str {
        &self.query_identity_sha256
    }
    pub fn rule_profile(&self) -> &str {
        &self.rule_profile
    }
    pub fn spin_profile(&self) -> &str {
        &self.spin_profile
    }
    pub fn supply_identity_sha256(&self) -> &str {
        &self.supply_identity_sha256
    }
    pub fn universe_identity_sha256(&self) -> &str {
        &self.universe_identity_sha256
    }
    pub fn product_build(&self) -> &str {
        &self.product_build
    }
    pub fn ordering(&self) -> &str {
        &self.ordering
    }
    pub fn minimum_placements(&self) -> Option<&str> {
        self.minimum_placements.as_deref()
    }
    pub fn guaranteed_final_piece(&self) -> Option<&str> {
        self.guaranteed_final_piece.as_deref()
    }
    pub fn guarantee_basis(&self) -> Option<&str> {
        self.guarantee_basis.as_deref()
    }
    pub const fn dependency_report_included(&self) -> Option<bool> {
        self.dependency_report_included
    }
    pub fn dependency_relation(&self) -> Option<&str> {
        self.dependency_relation.as_deref()
    }
    pub fn dependency_edge_count(&self) -> Option<&str> {
        self.dependency_edge_count.as_deref()
    }
    pub fn regular_count(&self) -> &str {
        &self.regular_count
    }
    pub fn mini_count(&self) -> &str {
        &self.mini_count
    }
    pub fn candidate_count(&self) -> &str {
        &self.candidate_count
    }
    pub const fn complete(&self) -> bool {
        self.complete
    }
    pub fn candidates(&self) -> &[SpinStructureCandidatePayload] {
        &self.candidates
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut total = [
            &self.schema_id,
            &self.query_identity_sha256,
            &self.rule_profile,
            &self.spin_profile,
            &self.supply_identity_sha256,
            &self.universe_identity_sha256,
            &self.product_build,
            &self.ordering,
            &self.regular_count,
            &self.mini_count,
            &self.candidate_count,
        ]
        .into_iter()
        .try_fold(0_u128, |total, value| {
            total.checked_add(value.capacity() as u128)
        })?;
        if let Some(minimum) = &self.minimum_placements {
            total = total.checked_add(minimum.capacity() as u128)?;
        }
        for value in [
            self.guaranteed_final_piece.as_ref(),
            self.guarantee_basis.as_ref(),
            self.dependency_relation.as_ref(),
            self.dependency_edge_count.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            total = total.checked_add(value.capacity() as u128)?;
        }
        total = total.checked_add(
            (self.candidates.capacity() as u128)
                .checked_mul(core::mem::size_of::<SpinStructureCandidatePayload>() as u128)?,
        )?;
        for candidate in &self.candidates {
            total = total.checked_add(candidate.checked_retained_capacity_bytes()?)?;
        }
        Some(total)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildCoverageCompletenessPayload {
    source_universe_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
    exact_minimum_proven: bool,
    query_bound: bool,
}

impl BuildCoverageCompletenessPayload {
    pub const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
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
    pub const fn query_bound(self) -> bool {
        self.query_bound
    }
    pub const fn new(
        source_universe_complete: bool,
        coverage_rows_complete: bool,
        probability_weights_complete: bool,
        exact_minimum_proven: bool,
        query_bound: bool,
    ) -> Self {
        Self {
            source_universe_complete,
            coverage_rows_complete,
            probability_weights_complete,
            exact_minimum_proven,
            query_bound,
        }
    }

    pub const fn complete(self) -> bool {
        self.source_universe_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
            && self.exact_minimum_proven
            && self.query_bound
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildCoveragePortfolioPayloadError {
    ContractInvalid,
    ObjectiveInvalid,
    ProbabilityBasisInvalid,
    DecimalInvalid(&'static str),
    IdentityInvalid(&'static str),
    CandidateCountInvalid,
    CanonicalFirstCandidateMissing,
    CompletenessInvalid,
    PageSourceInvalid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildCoveragePortfolioV2Payload {
    contract: String,
    objective: String,
    probability_basis: String,
    source_candidate_count: String,
    selected_candidate_count: String,
    pattern_count: String,
    required_pattern_count: String,
    union_probability: String,
    normalized_solution_set_hash: String,
    canonical_first_candidate_id: String,
    completeness: BuildCoverageCompletenessPayload,
    page_source_available: bool,
    page_source_identity_sha256: Option<String>,
}

impl BuildCoveragePortfolioV2Payload {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        contract: impl Into<String>,
        objective: impl Into<String>,
        probability_basis: impl Into<String>,
        source_candidate_count: impl Into<String>,
        selected_candidate_count: impl Into<String>,
        pattern_count: impl Into<String>,
        required_pattern_count: impl Into<String>,
        union_probability: impl Into<String>,
        normalized_solution_set_hash: impl Into<String>,
        canonical_first_candidate_id: impl Into<String>,
        completeness: BuildCoverageCompletenessPayload,
        page_source_available: bool,
        page_source_identity_sha256: Option<String>,
    ) -> Result<Self, BuildCoveragePortfolioPayloadError> {
        let payload = Self {
            contract: contract.into(),
            objective: objective.into(),
            probability_basis: probability_basis.into(),
            source_candidate_count: source_candidate_count.into(),
            selected_candidate_count: selected_candidate_count.into(),
            pattern_count: pattern_count.into(),
            required_pattern_count: required_pattern_count.into(),
            union_probability: union_probability.into(),
            normalized_solution_set_hash: normalized_solution_set_hash.into(),
            canonical_first_candidate_id: canonical_first_candidate_id.into(),
            completeness,
            page_source_available,
            page_source_identity_sha256,
        };
        payload.validate()?;
        Ok(payload)
    }

    fn validate(&self) -> Result<(), BuildCoveragePortfolioPayloadError> {
        if self.contract != "build-coverage-portfolio.v2" {
            return Err(BuildCoveragePortfolioPayloadError::ContractInvalid);
        }
        if !matches!(
            self.objective.as_str(),
            "min-cover" | "max-probability-minimum"
        ) {
            return Err(BuildCoveragePortfolioPayloadError::ObjectiveInvalid);
        }
        if self.probability_basis.is_empty() {
            return Err(BuildCoveragePortfolioPayloadError::ProbabilityBasisInvalid);
        }
        for (name, value) in [
            (
                "source_candidate_count",
                self.source_candidate_count.as_str(),
            ),
            (
                "selected_candidate_count",
                self.selected_candidate_count.as_str(),
            ),
            ("pattern_count", self.pattern_count.as_str()),
            (
                "required_pattern_count",
                self.required_pattern_count.as_str(),
            ),
        ] {
            if !canonical_decimal(value) {
                return Err(BuildCoveragePortfolioPayloadError::DecimalInvalid(name));
            }
        }
        let source = decimal_u128(&self.source_candidate_count).ok_or(
            BuildCoveragePortfolioPayloadError::DecimalInvalid("source_candidate_count"),
        )?;
        let selected = decimal_u128(&self.selected_candidate_count).ok_or(
            BuildCoveragePortfolioPayloadError::DecimalInvalid("selected_candidate_count"),
        )?;
        let exact_empty = source == 0
            && selected == 0
            && self.required_pattern_count == "0"
            && self.union_probability == "0"
            && self.canonical_first_candidate_id.is_empty();
        if (selected == 0 && !exact_empty) || selected > source {
            return Err(BuildCoveragePortfolioPayloadError::CandidateCountInvalid);
        }
        if self.canonical_first_candidate_id.is_empty() && !exact_empty {
            return Err(BuildCoveragePortfolioPayloadError::CanonicalFirstCandidateMissing);
        }
        if self.normalized_solution_set_hash.is_empty() {
            return Err(BuildCoveragePortfolioPayloadError::IdentityInvalid(
                "normalized_solution_set_hash",
            ));
        }
        if self.union_probability.is_empty() {
            return Err(BuildCoveragePortfolioPayloadError::DecimalInvalid(
                "union_probability",
            ));
        }
        if !self.completeness.complete() {
            return Err(BuildCoveragePortfolioPayloadError::CompletenessInvalid);
        }
        match (
            self.page_source_available,
            self.page_source_identity_sha256.as_deref(),
        ) {
            (true, Some(identity)) if sha256_text(identity) => {}
            (false, None) => {}
            _ => return Err(BuildCoveragePortfolioPayloadError::PageSourceInvalid),
        }
        Ok(())
    }

    pub fn contract(&self) -> &str {
        &self.contract
    }
    pub fn objective(&self) -> &str {
        &self.objective
    }
    pub fn probability_basis(&self) -> &str {
        &self.probability_basis
    }
    pub fn source_candidate_count(&self) -> &str {
        &self.source_candidate_count
    }
    pub fn selected_candidate_count(&self) -> &str {
        &self.selected_candidate_count
    }
    pub fn pattern_count(&self) -> &str {
        &self.pattern_count
    }
    pub fn required_pattern_count(&self) -> &str {
        &self.required_pattern_count
    }
    pub fn union_probability(&self) -> &str {
        &self.union_probability
    }
    pub fn normalized_solution_set_hash(&self) -> &str {
        &self.normalized_solution_set_hash
    }
    pub fn canonical_first_candidate_id(&self) -> &str {
        &self.canonical_first_candidate_id
    }
    pub const fn completeness(&self) -> BuildCoverageCompletenessPayload {
        self.completeness
    }
    pub const fn page_source_available(&self) -> bool {
        self.page_source_available
    }
    pub fn page_source_identity_sha256(&self) -> Option<&str> {
        self.page_source_identity_sha256.as_deref()
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut total = [
            &self.contract,
            &self.objective,
            &self.probability_basis,
            &self.source_candidate_count,
            &self.selected_candidate_count,
            &self.pattern_count,
            &self.required_pattern_count,
            &self.union_probability,
            &self.normalized_solution_set_hash,
            &self.canonical_first_candidate_id,
        ]
        .into_iter()
        .try_fold(0_u128, |total, value| {
            total.checked_add(value.capacity() as u128)
        })?;
        if let Some(identity) = &self.page_source_identity_sha256 {
            total = total.checked_add(identity.capacity() as u128)?;
        }
        Some(total)
    }
}

fn canonical_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn decimal_u128(value: &str) -> Option<u128> {
    canonical_decimal(value)
        .then(|| value.parse().ok())
        .flatten()
}

fn sha256_text(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildSetupFamilyPayloadError {
    ContractInvalid,
    IdentityInvalid,
    ObjectiveInvalid,
    DecimalInvalid(&'static str),
    CountMismatch,
    CoverageInvalid,
    CandidateKeyInvalid,
    CompletenessInvalid,
    ProbabilityInvalid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildSetupCandidateCoverageV1Payload {
    candidate_key: String,
    covered_pattern_count: String,
}

impl BuildSetupCandidateCoverageV1Payload {
    pub fn try_new(
        candidate_key: impl Into<String>,
        covered_pattern_count: impl Into<String>,
    ) -> Result<Self, BuildSetupFamilyPayloadError> {
        let value = Self {
            candidate_key: candidate_key.into(),
            covered_pattern_count: covered_pattern_count.into(),
        };
        if value.candidate_key.is_empty() {
            return Err(BuildSetupFamilyPayloadError::CandidateKeyInvalid);
        }
        if !canonical_decimal(&value.covered_pattern_count) {
            return Err(BuildSetupFamilyPayloadError::DecimalInvalid(
                "covered_pattern_count",
            ));
        }
        Ok(value)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildSetupCompletenessPayload {
    input_identity_bound: bool,
    producer_filter_bound: bool,
    buildability_replay_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
}
impl BuildSetupCompletenessPayload {
    pub const fn new(a: bool, b: bool, c: bool, d: bool, e: bool) -> Self {
        Self {
            input_identity_bound: a,
            producer_filter_bound: b,
            buildability_replay_complete: c,
            coverage_rows_complete: d,
            probability_weights_complete: e,
        }
    }
    pub const fn complete(self) -> bool {
        self.input_identity_bound
            && self.producer_filter_bound
            && self.buildability_replay_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildSetupFamilyV1Payload {
    contract: String,
    input_identity_sha256: String,
    evaluation_identity_sha256: String,
    objective: String,
    source_candidate_count: String,
    reachable_candidate_count: String,
    pattern_count: String,
    covered_pattern_count: String,
    union_probability: String,
    completeness: BuildSetupCompletenessPayload,
    candidates: Vec<BuildSetupCandidateCoverageV1Payload>,
}
impl BuildSetupFamilyV1Payload {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        contract: impl Into<String>,
        input: impl Into<String>,
        evaluation: impl Into<String>,
        objective: impl Into<String>,
        source: impl Into<String>,
        reachable: impl Into<String>,
        patterns: impl Into<String>,
        covered: impl Into<String>,
        probability: impl Into<String>,
        completeness: BuildSetupCompletenessPayload,
        candidates: Vec<BuildSetupCandidateCoverageV1Payload>,
    ) -> Result<Self, BuildSetupFamilyPayloadError> {
        let value = Self {
            contract: contract.into(),
            input_identity_sha256: input.into(),
            evaluation_identity_sha256: evaluation.into(),
            objective: objective.into(),
            source_candidate_count: source.into(),
            reachable_candidate_count: reachable.into(),
            pattern_count: patterns.into(),
            covered_pattern_count: covered.into(),
            union_probability: probability.into(),
            completeness,
            candidates,
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), BuildSetupFamilyPayloadError> {
        if self.contract != "build-target-family.v2" {
            return Err(BuildSetupFamilyPayloadError::ContractInvalid);
        }
        if !sha256_text(&self.input_identity_sha256)
            || !sha256_text(&self.evaluation_identity_sha256)
        {
            return Err(BuildSetupFamilyPayloadError::IdentityInvalid);
        }
        if !matches!(self.objective.as_str(), "all" | "unique") {
            return Err(BuildSetupFamilyPayloadError::ObjectiveInvalid);
        }
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
            ("covered_pattern_count", self.covered_pattern_count.as_str()),
        ] {
            if !canonical_decimal(text) {
                return Err(BuildSetupFamilyPayloadError::DecimalInvalid(name));
            }
        }
        let source = decimal_u128(&self.source_candidate_count)
            .ok_or(BuildSetupFamilyPayloadError::CountMismatch)?;
        let reachable = decimal_u128(&self.reachable_candidate_count)
            .ok_or(BuildSetupFamilyPayloadError::CountMismatch)?;
        let patterns =
            decimal_u128(&self.pattern_count).ok_or(BuildSetupFamilyPayloadError::CountMismatch)?;
        let covered = decimal_u128(&self.covered_pattern_count)
            .ok_or(BuildSetupFamilyPayloadError::CountMismatch)?;
        if source != self.candidates.len() as u128 || reachable > source || covered > patterns {
            return Err(BuildSetupFamilyPayloadError::CountMismatch);
        }
        let mut positive = 0_u128;
        let mut keys = std::collections::BTreeSet::new();
        for row in &self.candidates {
            let count = decimal_u128(row.covered_pattern_count())
                .ok_or(BuildSetupFamilyPayloadError::CoverageInvalid)?;
            if count > patterns {
                return Err(BuildSetupFamilyPayloadError::CoverageInvalid);
            }
            if count > 0 {
                positive += 1;
            }
            if !keys.insert(row.candidate_key()) {
                return Err(BuildSetupFamilyPayloadError::CandidateKeyInvalid);
            }
        }
        if positive != reachable {
            return Err(BuildSetupFamilyPayloadError::CountMismatch);
        }
        if self.union_probability.is_empty() {
            return Err(BuildSetupFamilyPayloadError::ProbabilityInvalid);
        }
        if !self.completeness.complete() {
            return Err(BuildSetupFamilyPayloadError::CompletenessInvalid);
        }
        Ok(())
    }
    pub fn contract(&self) -> &str {
        &self.contract
    }
    pub fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }
    pub fn evaluation_identity_sha256(&self) -> &str {
        &self.evaluation_identity_sha256
    }
    pub fn objective(&self) -> &str {
        &self.objective
    }
    pub fn source_candidate_count(&self) -> &str {
        &self.source_candidate_count
    }
    pub fn reachable_candidate_count(&self) -> &str {
        &self.reachable_candidate_count
    }
    pub fn pattern_count(&self) -> &str {
        &self.pattern_count
    }
    pub fn covered_pattern_count(&self) -> &str {
        &self.covered_pattern_count
    }
    pub fn union_probability(&self) -> &str {
        &self.union_probability
    }
    pub fn candidates(&self) -> &[BuildSetupCandidateCoverageV1Payload] {
        &self.candidates
    }
    pub const fn completeness(&self) -> BuildSetupCompletenessPayload {
        self.completeness
    }
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut total = [
            &self.contract,
            &self.input_identity_sha256,
            &self.evaluation_identity_sha256,
            &self.objective,
            &self.source_candidate_count,
            &self.reachable_candidate_count,
            &self.pattern_count,
            &self.covered_pattern_count,
            &self.union_probability,
        ]
        .into_iter()
        .try_fold(0_u128, |n, s| n.checked_add(s.capacity() as u128))?;
        total = total
            .checked_add((self.candidates.capacity() as u128).checked_mul(
                core::mem::size_of::<BuildSetupCandidateCoverageV1Payload>() as u128,
            )?)?;
        for row in &self.candidates {
            total = total.checked_add(row.checked_retained_capacity_bytes()?)?;
        }
        Some(total)
    }
}

/// Shared, exact execution metadata for both PC save result products.
///
/// Every count and producer identity is transported as canonical base-10 text
/// so a JavaScript host never rounds a Rust `usize` or `u64`. Probabilities are
/// likewise canonical decimal text rather than binary JSON numbers.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcSaveRunMetadataPayload {
    origin: String,
    problem_preset: String,
    problem_id: String,
    piece_source_id: String,
    pattern_universe_id: String,
    pattern_weight_model_id: String,
    materialized_pattern_count: String,
    pc_success_pattern_count: String,
    pc_probability: String,
    completeness: PcSaveCompletenessPayload,
}

impl PcSaveRunMetadataPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        origin: impl Into<String>,
        problem_preset: impl Into<String>,
        problem_id: impl Into<String>,
        piece_source_id: impl Into<String>,
        pattern_universe_id: impl Into<String>,
        pattern_weight_model_id: impl Into<String>,
        materialized_pattern_count: impl Into<String>,
        pc_success_pattern_count: impl Into<String>,
        pc_probability: impl Into<String>,
        completeness: PcSaveCompletenessPayload,
    ) -> Self {
        Self {
            origin: origin.into(),
            problem_preset: problem_preset.into(),
            problem_id: problem_id.into(),
            piece_source_id: piece_source_id.into(),
            pattern_universe_id: pattern_universe_id.into(),
            pattern_weight_model_id: pattern_weight_model_id.into(),
            materialized_pattern_count: materialized_pattern_count.into(),
            pc_success_pattern_count: pc_success_pattern_count.into(),
            pc_probability: pc_probability.into(),
            completeness,
        }
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn problem_preset(&self) -> &str {
        &self.problem_preset
    }

    pub fn problem_id(&self) -> &str {
        &self.problem_id
    }

    pub fn piece_source_id(&self) -> &str {
        &self.piece_source_id
    }

    pub fn pattern_universe_id(&self) -> &str {
        &self.pattern_universe_id
    }

    pub fn pattern_weight_model_id(&self) -> &str {
        &self.pattern_weight_model_id
    }

    pub fn materialized_pattern_count(&self) -> &str {
        &self.materialized_pattern_count
    }

    pub fn pc_success_pattern_count(&self) -> &str {
        &self.pc_success_pattern_count
    }

    pub fn pc_probability(&self) -> &str {
        &self.pc_probability
    }

    pub const fn completeness(&self) -> PcSaveCompletenessPayload {
        self.completeness
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        [
            self.origin.capacity(),
            self.problem_preset.capacity(),
            self.problem_id.capacity(),
            self.piece_source_id.capacity(),
            self.pattern_universe_id.capacity(),
            self.pattern_weight_model_id.capacity(),
            self.materialized_pattern_count.capacity(),
            self.pc_success_pattern_count.capacity(),
            self.pc_probability.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcSaveCompletenessPayload {
    source_universe_complete: bool,
    fixed_bag_boundary_proven: bool,
    execution_batch_complete: bool,
    pattern_weights_complete: bool,
    count_complete: bool,
    probability_complete: bool,
    complete: bool,
}

impl PcSaveCompletenessPayload {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        source_universe_complete: bool,
        fixed_bag_boundary_proven: bool,
        execution_batch_complete: bool,
        pattern_weights_complete: bool,
        count_complete: bool,
        probability_complete: bool,
        complete: bool,
    ) -> Self {
        Self {
            source_universe_complete,
            fixed_bag_boundary_proven,
            execution_batch_complete,
            pattern_weights_complete,
            count_complete,
            probability_complete,
            complete,
        }
    }

    pub const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub const fn fixed_bag_boundary_proven(self) -> bool {
        self.fixed_bag_boundary_proven
    }

    pub const fn execution_batch_complete(self) -> bool {
        self.execution_batch_complete
    }

    pub const fn pattern_weights_complete(self) -> bool {
        self.pattern_weights_complete
    }

    pub const fn count_complete(self) -> bool {
        self.count_complete
    }

    pub const fn probability_complete(self) -> bool {
        self.probability_complete
    }

    pub const fn complete(self) -> bool {
        self.complete
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcSavePieceMultisetPayload {
    canonical_id: String,
    t: u8,
    i: u8,
    o: u8,
    j: u8,
    l: u8,
    s: u8,
    z: u8,
    total_count: u8,
}

impl PcSavePieceMultisetPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        canonical_id: impl Into<String>,
        t: u8,
        i: u8,
        o: u8,
        j: u8,
        l: u8,
        s: u8,
        z: u8,
        total_count: u8,
    ) -> Self {
        Self {
            canonical_id: canonical_id.into(),
            t,
            i,
            o,
            j,
            l,
            s,
            z,
            total_count,
        }
    }

    pub fn canonical_id(&self) -> &str {
        &self.canonical_id
    }

    pub const fn counts(&self) -> [u8; 7] {
        [self.t, self.i, self.o, self.j, self.l, self.s, self.z]
    }

    pub const fn total_count(&self) -> u8 {
        self.total_count
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(self.canonical_id.capacity() as u128)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcSaveWitnessPayload {
    pattern_index: String,
    candidate_id: String,
    trace_identity: String,
    source_cursor: String,
    terminal_hold: Option<String>,
    active_bag_remainder: PcSavePieceMultisetPayload,
}

impl PcSaveWitnessPayload {
    pub fn new(
        pattern_index: impl Into<String>,
        candidate_id: impl Into<String>,
        trace_identity: impl Into<String>,
        source_cursor: impl Into<String>,
        terminal_hold: Option<String>,
        active_bag_remainder: PcSavePieceMultisetPayload,
    ) -> Self {
        Self {
            pattern_index: pattern_index.into(),
            candidate_id: candidate_id.into(),
            trace_identity: trace_identity.into(),
            source_cursor: source_cursor.into(),
            terminal_hold,
            active_bag_remainder,
        }
    }

    pub fn pattern_index(&self) -> &str {
        &self.pattern_index
    }

    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }

    pub fn source_cursor(&self) -> &str {
        &self.source_cursor
    }

    pub fn terminal_hold(&self) -> Option<&str> {
        self.terminal_hold.as_deref()
    }

    pub const fn active_bag_remainder(&self) -> &PcSavePieceMultisetPayload {
        &self.active_bag_remainder
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.pattern_index.capacity(),
            self.candidate_id.capacity(),
            self.trace_identity.capacity(),
            self.source_cursor.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?;
        if let Some(terminal_hold) = &self.terminal_hold {
            bytes = bytes.checked_add(terminal_hold.capacity() as u128)?;
        }
        bytes.checked_add(
            self.active_bag_remainder
                .checked_retained_capacity_bytes()?,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcSaveGroupPayload {
    identity_contract: String,
    identity: PcSavePieceMultisetPayload,
    successful_pattern_count: String,
    unconditional_probability: String,
    conditional_probability_given_pc: String,
    canonical_candidate_id: String,
    witnesses: Vec<PcSaveWitnessPayload>,
}

impl PcSaveGroupPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity_contract: impl Into<String>,
        identity: PcSavePieceMultisetPayload,
        successful_pattern_count: impl Into<String>,
        unconditional_probability: impl Into<String>,
        conditional_probability_given_pc: impl Into<String>,
        canonical_candidate_id: impl Into<String>,
        witnesses: Vec<PcSaveWitnessPayload>,
    ) -> Self {
        Self {
            identity_contract: identity_contract.into(),
            identity,
            successful_pattern_count: successful_pattern_count.into(),
            unconditional_probability: unconditional_probability.into(),
            conditional_probability_given_pc: conditional_probability_given_pc.into(),
            canonical_candidate_id: canonical_candidate_id.into(),
            witnesses,
        }
    }

    pub fn identity_contract(&self) -> &str {
        &self.identity_contract
    }

    pub const fn identity(&self) -> &PcSavePieceMultisetPayload {
        &self.identity
    }

    pub fn successful_pattern_count(&self) -> &str {
        &self.successful_pattern_count
    }

    pub fn unconditional_probability(&self) -> &str {
        &self.unconditional_probability
    }

    pub fn conditional_probability_given_pc(&self) -> &str {
        &self.conditional_probability_given_pc
    }

    pub fn canonical_candidate_id(&self) -> &str {
        &self.canonical_candidate_id
    }

    pub fn witnesses(&self) -> &[PcSaveWitnessPayload] {
        &self.witnesses
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.identity_contract.capacity(),
            self.successful_pattern_count.capacity(),
            self.unconditional_probability.capacity(),
            self.conditional_probability_given_pc.capacity(),
            self.canonical_candidate_id.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?
        .checked_add(self.identity.checked_retained_capacity_bytes()?)?
        .checked_add(
            (self.witnesses.capacity() as u128)
                .checked_mul(core::mem::size_of::<PcSaveWitnessPayload>() as u128)?,
        )?;
        for witness in &self.witnesses {
            bytes = bytes.checked_add(witness.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcSaveGroupsPayload {
    schema_id: String,
    page_size: String,
    group_count: String,
    metadata: PcSaveRunMetadataPayload,
    groups: Vec<PcSaveGroupPayload>,
}

impl PcSaveGroupsPayload {
    pub fn new(
        schema_id: impl Into<String>,
        page_size: impl Into<String>,
        group_count: impl Into<String>,
        metadata: PcSaveRunMetadataPayload,
        groups: Vec<PcSaveGroupPayload>,
    ) -> Self {
        Self {
            schema_id: schema_id.into(),
            page_size: page_size.into(),
            group_count: group_count.into(),
            metadata,
            groups,
        }
    }

    pub fn schema_id(&self) -> &str {
        &self.schema_id
    }

    pub fn page_size(&self) -> &str {
        &self.page_size
    }

    pub fn group_count(&self) -> &str {
        &self.group_count
    }

    pub const fn metadata(&self) -> &PcSaveRunMetadataPayload {
        &self.metadata
    }

    pub fn groups(&self) -> &[PcSaveGroupPayload] {
        &self.groups
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.schema_id.capacity() as u128)
            .checked_add(self.page_size.capacity() as u128)?
            .checked_add(self.group_count.capacity() as u128)?
            .checked_add(self.metadata.checked_retained_capacity_bytes()?)?
            .checked_add(
                (self.groups.capacity() as u128)
                    .checked_mul(core::mem::size_of::<PcSaveGroupPayload>() as u128)?,
            )?;
        for group in &self.groups {
            bytes = bytes.checked_add(group.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcBestSaveWinnerPayload {
    weighted_total: String,
    balanced_jl_count: String,
    exact_group_probability: String,
    group: PcSaveGroupPayload,
}

impl PcBestSaveWinnerPayload {
    pub fn new(
        weighted_total: impl Into<String>,
        balanced_jl_count: impl Into<String>,
        exact_group_probability: impl Into<String>,
        group: PcSaveGroupPayload,
    ) -> Self {
        Self {
            weighted_total: weighted_total.into(),
            balanced_jl_count: balanced_jl_count.into(),
            exact_group_probability: exact_group_probability.into(),
            group,
        }
    }

    pub fn weighted_total(&self) -> &str {
        &self.weighted_total
    }

    pub fn balanced_jl_count(&self) -> &str {
        &self.balanced_jl_count
    }

    pub fn exact_group_probability(&self) -> &str {
        &self.exact_group_probability
    }

    pub const fn group(&self) -> &PcSaveGroupPayload {
        &self.group
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.weighted_total.capacity() as u128)
            .checked_add(self.balanced_jl_count.capacity() as u128)?
            .checked_add(self.exact_group_probability.capacity() as u128)?
            .checked_add(self.group.checked_retained_capacity_bytes()?)
    }
}

/// Exact best-save ties as one ordinary, finite winner list.
///
/// There is deliberately no portfolio id, alternative cursor, tie marker, or
/// live page handle in this payload. GUI clients may page the array locally.
/// The App supplies the canonical pair for presenters that show one winner.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcBestSavePayload {
    schema_id: String,
    probability_basis: String,
    ordering: String,
    equality: String,
    page_size: String,
    winner_count: String,
    canonical_selection: String,
    canonical_winner: Option<PcBestSaveWinnerPayload>,
    metadata: PcSaveRunMetadataPayload,
    winners: Vec<PcBestSaveWinnerPayload>,
}

impl PcBestSavePayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        schema_id: impl Into<String>,
        probability_basis: impl Into<String>,
        ordering: impl Into<String>,
        equality: impl Into<String>,
        page_size: impl Into<String>,
        winner_count: impl Into<String>,
        canonical_selection: impl Into<String>,
        canonical_winner: Option<PcBestSaveWinnerPayload>,
        metadata: PcSaveRunMetadataPayload,
        winners: Vec<PcBestSaveWinnerPayload>,
    ) -> Self {
        Self {
            schema_id: schema_id.into(),
            probability_basis: probability_basis.into(),
            ordering: ordering.into(),
            equality: equality.into(),
            page_size: page_size.into(),
            winner_count: winner_count.into(),
            canonical_selection: canonical_selection.into(),
            canonical_winner,
            metadata,
            winners,
        }
    }

    pub fn schema_id(&self) -> &str {
        &self.schema_id
    }

    pub fn probability_basis(&self) -> &str {
        &self.probability_basis
    }

    pub fn ordering(&self) -> &str {
        &self.ordering
    }

    pub fn equality(&self) -> &str {
        &self.equality
    }

    pub fn page_size(&self) -> &str {
        &self.page_size
    }

    pub fn winner_count(&self) -> &str {
        &self.winner_count
    }

    pub fn canonical_selection(&self) -> &str {
        &self.canonical_selection
    }

    pub const fn canonical_winner(&self) -> Option<&PcBestSaveWinnerPayload> {
        self.canonical_winner.as_ref()
    }

    pub const fn metadata(&self) -> &PcSaveRunMetadataPayload {
        &self.metadata
    }

    pub fn winners(&self) -> &[PcBestSaveWinnerPayload] {
        &self.winners
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.schema_id.capacity(),
            self.probability_basis.capacity(),
            self.ordering.capacity(),
            self.equality.capacity(),
            self.page_size.capacity(),
            self.winner_count.capacity(),
            self.canonical_selection.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?
        .checked_add(self.metadata.checked_retained_capacity_bytes()?)?
        .checked_add(
            (self.winners.capacity() as u128)
                .checked_mul(core::mem::size_of::<PcBestSaveWinnerPayload>() as u128)?,
        )?;
        if let Some(winner) = &self.canonical_winner {
            bytes = bytes.checked_add(winner.checked_retained_capacity_bytes()?)?;
        }
        for winner in &self.winners {
            bytes = bytes.checked_add(winner.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CoveragePortfolioPagePayload {
    set_contract: String,
    page_contract: String,
    member_page_contract: String,
    set_identity_sha256: String,
    candidate_map_sha256: String,
    alternative_index: String,
    optimal_cardinality: String,
    known_alternative_count: String,
    total_alternative_count: Option<String>,
    enumeration_complete: bool,
    member_page_number: String,
    total_member_pages: String,
    members: Vec<ProductCandidateMemberPayload>,
    page_handle_available: bool,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    canonical_selection: Option<String>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    canonical_witness: Option<ProductCandidateMemberPayload>,
}

impl CoveragePortfolioPagePayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        set_contract: impl Into<String>,
        page_contract: impl Into<String>,
        member_page_contract: impl Into<String>,
        set_identity_sha256: impl Into<String>,
        candidate_map_sha256: impl Into<String>,
        alternative_index: impl Into<String>,
        optimal_cardinality: impl Into<String>,
        known_alternative_count: impl Into<String>,
        total_alternative_count: Option<String>,
        enumeration_complete: bool,
        member_page_number: impl Into<String>,
        total_member_pages: impl Into<String>,
        members: Vec<ProductCandidateMemberPayload>,
        page_handle_available: bool,
    ) -> Self {
        Self {
            set_contract: set_contract.into(),
            page_contract: page_contract.into(),
            member_page_contract: member_page_contract.into(),
            set_identity_sha256: set_identity_sha256.into(),
            candidate_map_sha256: candidate_map_sha256.into(),
            alternative_index: alternative_index.into(),
            optimal_cardinality: optimal_cardinality.into(),
            known_alternative_count: known_alternative_count.into(),
            total_alternative_count,
            enumeration_complete,
            member_page_number: member_page_number.into(),
            total_member_pages: total_member_pages.into(),
            members,
            page_handle_available,
            canonical_selection: None,
            canonical_witness: None,
        }
    }

    /// Attaches a product-specific, upstream-selected witness without asking
    /// any host adapter to choose again from `members`.
    pub fn with_canonical_witness(
        mut self,
        selection: impl Into<String>,
        witness: ProductCandidateMemberPayload,
    ) -> Self {
        self.canonical_selection = Some(selection.into());
        self.canonical_witness = Some(witness);
        self
    }

    pub fn set_contract(&self) -> &str {
        &self.set_contract
    }

    pub fn page_contract(&self) -> &str {
        &self.page_contract
    }

    pub fn member_page_contract(&self) -> &str {
        &self.member_page_contract
    }

    pub fn set_identity_sha256(&self) -> &str {
        &self.set_identity_sha256
    }

    pub fn candidate_map_sha256(&self) -> &str {
        &self.candidate_map_sha256
    }

    pub fn alternative_index(&self) -> &str {
        &self.alternative_index
    }

    pub fn optimal_cardinality(&self) -> &str {
        &self.optimal_cardinality
    }

    pub fn known_alternative_count(&self) -> &str {
        &self.known_alternative_count
    }

    pub fn total_alternative_count(&self) -> Option<&str> {
        self.total_alternative_count.as_deref()
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.enumeration_complete
    }

    pub fn member_page_number(&self) -> &str {
        &self.member_page_number
    }

    pub fn total_member_pages(&self) -> &str {
        &self.total_member_pages
    }

    pub fn members(&self) -> &[ProductCandidateMemberPayload] {
        &self.members
    }

    pub const fn page_handle_available(&self) -> bool {
        self.page_handle_available
    }

    pub fn canonical_selection(&self) -> Option<&str> {
        self.canonical_selection.as_deref()
    }

    pub const fn canonical_witness(&self) -> Option<&ProductCandidateMemberPayload> {
        self.canonical_witness.as_ref()
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.set_contract.capacity(),
            self.page_contract.capacity(),
            self.member_page_contract.capacity(),
            self.set_identity_sha256.capacity(),
            self.candidate_map_sha256.capacity(),
            self.alternative_index.capacity(),
            self.optimal_cardinality.capacity(),
            self.known_alternative_count.capacity(),
            self.member_page_number.capacity(),
            self.total_member_pages.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?;
        if let Some(total) = &self.total_alternative_count {
            bytes = bytes.checked_add(total.capacity() as u128)?;
        }
        if let Some(selection) = &self.canonical_selection {
            bytes = bytes.checked_add(selection.capacity() as u128)?;
        }
        if let Some(witness) = &self.canonical_witness {
            bytes = bytes.checked_add(witness.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(
            (self.members.capacity() as u128)
                .checked_mul(core::mem::size_of::<ProductCandidateMemberPayload>() as u128)?,
        )?;
        for member in &self.members {
            bytes = bytes.checked_add(member.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProductCandidateMemberPayload {
    candidate_id: String,
    normalized_solution_key: String,
}

impl ProductCandidateMemberPayload {
    pub fn new(
        candidate_id: impl Into<String>,
        normalized_solution_key: impl Into<String>,
    ) -> Self {
        Self {
            candidate_id: candidate_id.into(),
            normalized_solution_key: normalized_solution_key.into(),
        }
    }

    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    pub fn normalized_solution_key(&self) -> &str {
        &self.normalized_solution_key
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.candidate_id.capacity() as u128)
            .checked_add(self.normalized_solution_key.capacity() as u128)
    }
}

/// Closed Host payload for the ordinary `pc.score` field-average product.
///
/// It has exactly one row per normalized solution field. Each row averages
/// that field over the whole materialized pattern universe, assigning zero to
/// patterns the field cannot solve. Candidate IDs, attack values, trace
/// selectors and portfolio membership are deliberately absent.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcScoreFieldSummaryPayload {
    field_contract: String,
    ordering: String,
    solution_field_average_basis: String,
    score_evaluation_basis: String,
    score_evaluation_scope: String,
    overall_score_basis: String,
    piece_source_id: String,
    pattern_universe_id: String,
    pattern_weight_model_id: String,
    materialized_pattern_count: String,
    solution_field_count: String,
    scored_pattern_count: String,
    failed_pc_pattern_count: String,
    covered_probability: String,
    overall_score: String,
    score_covered_pattern_conditional_average_score: Option<String>,
    complete: bool,
    fields: Vec<PcScoreFieldPayload>,
}

impl PcScoreFieldSummaryPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        field_contract: impl Into<String>,
        ordering: impl Into<String>,
        solution_field_average_basis: impl Into<String>,
        score_evaluation_basis: impl Into<String>,
        score_evaluation_scope: impl Into<String>,
        overall_score_basis: impl Into<String>,
        piece_source_id: impl Into<String>,
        pattern_universe_id: impl Into<String>,
        pattern_weight_model_id: impl Into<String>,
        materialized_pattern_count: impl Into<String>,
        solution_field_count: impl Into<String>,
        scored_pattern_count: impl Into<String>,
        failed_pc_pattern_count: impl Into<String>,
        covered_probability: impl Into<String>,
        overall_score: impl Into<String>,
        score_covered_pattern_conditional_average_score: Option<String>,
        complete: bool,
        fields: Vec<PcScoreFieldPayload>,
    ) -> Self {
        Self {
            field_contract: field_contract.into(),
            ordering: ordering.into(),
            solution_field_average_basis: solution_field_average_basis.into(),
            score_evaluation_basis: score_evaluation_basis.into(),
            score_evaluation_scope: score_evaluation_scope.into(),
            overall_score_basis: overall_score_basis.into(),
            piece_source_id: piece_source_id.into(),
            pattern_universe_id: pattern_universe_id.into(),
            pattern_weight_model_id: pattern_weight_model_id.into(),
            materialized_pattern_count: materialized_pattern_count.into(),
            solution_field_count: solution_field_count.into(),
            scored_pattern_count: scored_pattern_count.into(),
            failed_pc_pattern_count: failed_pc_pattern_count.into(),
            covered_probability: covered_probability.into(),
            overall_score: overall_score.into(),
            score_covered_pattern_conditional_average_score,
            complete,
            fields,
        }
    }

    pub fn field_contract(&self) -> &str {
        &self.field_contract
    }
    pub fn ordering(&self) -> &str {
        &self.ordering
    }
    pub fn solution_field_average_basis(&self) -> &str {
        &self.solution_field_average_basis
    }
    pub fn score_evaluation_basis(&self) -> &str {
        &self.score_evaluation_basis
    }
    pub fn score_evaluation_scope(&self) -> &str {
        &self.score_evaluation_scope
    }
    pub fn overall_score_basis(&self) -> &str {
        &self.overall_score_basis
    }
    pub fn piece_source_id(&self) -> &str {
        &self.piece_source_id
    }
    pub fn pattern_universe_id(&self) -> &str {
        &self.pattern_universe_id
    }
    pub fn pattern_weight_model_id(&self) -> &str {
        &self.pattern_weight_model_id
    }
    pub fn materialized_pattern_count(&self) -> &str {
        &self.materialized_pattern_count
    }
    pub fn solution_field_count(&self) -> &str {
        &self.solution_field_count
    }
    pub fn scored_pattern_count(&self) -> &str {
        &self.scored_pattern_count
    }
    pub fn failed_pc_pattern_count(&self) -> &str {
        &self.failed_pc_pattern_count
    }
    pub fn covered_probability(&self) -> &str {
        &self.covered_probability
    }
    pub fn overall_score(&self) -> &str {
        &self.overall_score
    }
    pub fn score_covered_pattern_conditional_average_score(&self) -> Option<&str> {
        self.score_covered_pattern_conditional_average_score
            .as_deref()
    }
    pub const fn complete(&self) -> bool {
        self.complete
    }
    pub fn fields(&self) -> &[PcScoreFieldPayload] {
        &self.fields
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            &self.field_contract,
            &self.ordering,
            &self.solution_field_average_basis,
            &self.score_evaluation_basis,
            &self.score_evaluation_scope,
            &self.overall_score_basis,
            &self.piece_source_id,
            &self.pattern_universe_id,
            &self.pattern_weight_model_id,
            &self.materialized_pattern_count,
            &self.solution_field_count,
            &self.scored_pattern_count,
            &self.failed_pc_pattern_count,
            &self.covered_probability,
            &self.overall_score,
        ]
        .into_iter()
        .try_fold(0_u128, |total, value| {
            total.checked_add(value.capacity() as u128)
        })?;
        if let Some(value) = &self.score_covered_pattern_conditional_average_score {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        bytes = bytes.checked_add(
            (self.fields.capacity() as u128)
                .checked_mul(core::mem::size_of::<PcScoreFieldPayload>() as u128)?,
        )?;
        for field in &self.fields {
            bytes = bytes.checked_add(field.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcScoreFieldPayload {
    normalized_field_key: String,
    average_score: String,
    covered_pattern_count: String,
    pattern_count: String,
    score_complete: bool,
}

impl PcScoreFieldPayload {
    pub fn new(
        normalized_field_key: impl Into<String>,
        average_score: impl Into<String>,
        covered_pattern_count: impl Into<String>,
        pattern_count: impl Into<String>,
        score_complete: bool,
    ) -> Self {
        Self {
            normalized_field_key: normalized_field_key.into(),
            average_score: average_score.into(),
            covered_pattern_count: covered_pattern_count.into(),
            pattern_count: pattern_count.into(),
            score_complete,
        }
    }

    pub fn normalized_field_key(&self) -> &str {
        &self.normalized_field_key
    }
    pub fn average_score(&self) -> &str {
        &self.average_score
    }
    pub fn covered_pattern_count(&self) -> &str {
        &self.covered_pattern_count
    }
    pub fn pattern_count(&self) -> &str {
        &self.pattern_count
    }
    pub const fn score_complete(&self) -> bool {
        self.score_complete
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.normalized_field_key.capacity() as u128)
            .checked_add(self.average_score.capacity() as u128)?
            .checked_add(self.covered_pattern_count.capacity() as u128)?
            .checked_add(self.pattern_count.capacity() as u128)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScorePatternWinnerFamilyPayload {
    winner_contract: String,
    ordering: String,
    equality: String,
    informational_attack_basis: String,
    page_size: String,
    winner_count: String,
    canonical_selection: String,
    canonical_winner: ScorePatternWinnerPayload,
    winners: Vec<ScorePatternWinnerPayload>,
}

impl ScorePatternWinnerFamilyPayload {
    // The constructor mirrors the versioned wire contract's independent fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        winner_contract: impl Into<String>,
        ordering: impl Into<String>,
        equality: impl Into<String>,
        informational_attack_basis: impl Into<String>,
        page_size: impl Into<String>,
        winner_count: impl Into<String>,
        canonical_selection: impl Into<String>,
        canonical_winner: ScorePatternWinnerPayload,
        winners: Vec<ScorePatternWinnerPayload>,
    ) -> Self {
        Self {
            winner_contract: winner_contract.into(),
            ordering: ordering.into(),
            equality: equality.into(),
            informational_attack_basis: informational_attack_basis.into(),
            page_size: page_size.into(),
            winner_count: winner_count.into(),
            canonical_selection: canonical_selection.into(),
            canonical_winner,
            winners,
        }
    }

    pub fn winner_contract(&self) -> &str {
        &self.winner_contract
    }

    pub fn ordering(&self) -> &str {
        &self.ordering
    }

    pub fn equality(&self) -> &str {
        &self.equality
    }

    pub fn informational_attack_basis(&self) -> &str {
        &self.informational_attack_basis
    }

    pub fn page_size(&self) -> &str {
        &self.page_size
    }

    pub fn winner_count(&self) -> &str {
        &self.winner_count
    }

    pub fn canonical_selection(&self) -> &str {
        &self.canonical_selection
    }

    pub const fn canonical_winner(&self) -> &ScorePatternWinnerPayload {
        &self.canonical_winner
    }

    pub fn winners(&self) -> &[ScorePatternWinnerPayload] {
        &self.winners
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.winner_contract.capacity(),
            self.ordering.capacity(),
            self.equality.capacity(),
            self.informational_attack_basis.capacity(),
            self.page_size.capacity(),
            self.winner_count.capacity(),
            self.canonical_selection.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?;
        bytes = bytes.checked_add(self.canonical_winner.checked_retained_capacity_bytes()?)?;
        bytes = bytes.checked_add(
            (self.winners.capacity() as u128)
                .checked_mul(core::mem::size_of::<ScorePatternWinnerPayload>() as u128)?,
        )?;
        for winner in &self.winners {
            bytes = bytes.checked_add(winner.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScorePatternWinnerPayload {
    pattern_id: String,
    candidate_id: String,
    normalized_solution_key: String,
    score: String,
    informational_attack: String,
}

impl ScorePatternWinnerPayload {
    pub fn new(
        pattern_id: impl Into<String>,
        candidate_id: impl Into<String>,
        normalized_solution_key: impl Into<String>,
        score: impl Into<String>,
        informational_attack: impl Into<String>,
    ) -> Self {
        Self {
            pattern_id: pattern_id.into(),
            candidate_id: candidate_id.into(),
            normalized_solution_key: normalized_solution_key.into(),
            score: score.into(),
            informational_attack: informational_attack.into(),
        }
    }

    pub fn pattern_id(&self) -> &str {
        &self.pattern_id
    }

    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    pub fn normalized_solution_key(&self) -> &str {
        &self.normalized_solution_key
    }

    pub fn score(&self) -> &str {
        &self.score
    }

    pub fn informational_attack(&self) -> &str {
        &self.informational_attack
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        [
            self.pattern_id.capacity(),
            self.candidate_id.capacity(),
            self.normalized_solution_key.capacity(),
            self.score.capacity(),
            self.informational_attack.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_ids_and_large_counts_serialize_as_decimal_strings() {
        let payload = ProductResultPayload::new(
            "pc.minimals",
            "pc-minimum-cover.v2",
            ProductResultPayloadContent::CoveragePortfolio(CoveragePortfolioPagePayload::new(
                "set",
                "page",
                "members",
                "a".repeat(64),
                "b".repeat(64),
                "18446744073709551616",
                "1",
                "18446744073709551616",
                None,
                false,
                "1",
                "1",
                vec![ProductCandidateMemberPayload::new(
                    "18446744073709551615",
                    "key",
                )],
                true,
            )),
        );
        let value = serde_json::to_value(payload).expect("payload JSON");
        assert_eq!(
            value["content"]["payload"]["members"][0]["candidate_id"],
            "18446744073709551615"
        );
        assert_eq!(
            value["content"]["payload"]["known_alternative_count"],
            "18446744073709551616"
        );
    }

    fn complete_build_evidence() -> BuildCoverageCompletenessPayload {
        BuildCoverageCompletenessPayload::new(true, true, true, true, true)
    }

    fn build_payload() -> BuildCoveragePortfolioV2Payload {
        BuildCoveragePortfolioV2Payload::try_new(
            "build-coverage-portfolio.v2",
            "min-cover",
            "exact-pattern-weight-union",
            "12",
            "2",
            "5040",
            "5040",
            "1",
            "a".repeat(64),
            "candidate-0001",
            complete_build_evidence(),
            true,
            Some("b".repeat(64)),
        )
        .expect("valid build coverage portfolio")
    }

    #[test]
    fn build_coverage_portfolio_round_trips_with_finite_identity() {
        let payload = ProductResultPayload::new(
            "build.cover",
            "build-coverage-portfolio.v2",
            ProductResultPayloadContent::BuildCoveragePortfolioV2(build_payload()),
        );
        let json = serde_json::to_string(&payload).expect("serialize");
        let decoded: ProductResultPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, payload);
        let ProductResultPayloadContent::BuildCoveragePortfolioV2(build) = decoded.content() else {
            panic!("build payload kind");
        };
        assert_eq!(build.canonical_first_candidate_id(), "candidate-0001");
        assert!(build.completeness().complete());
        assert!(build.page_source_available());
        assert_eq!(
            build.page_source_identity_sha256(),
            Some("b".repeat(64).as_str())
        );
    }

    #[test]
    fn build_coverage_portfolio_accepts_only_complete_exact_empty_results() {
        let create = |source: &str, required: &str, probability: &str, first: &str, exact: bool| {
            BuildCoveragePortfolioV2Payload::try_new(
                "build-coverage-portfolio.v2",
                "min-cover",
                "exact-pattern-weight-union",
                source,
                "0",
                "1",
                required,
                probability,
                "cts1:0000000000000000",
                first,
                BuildCoverageCompletenessPayload::new(true, true, true, exact, true),
                true,
                Some("b".repeat(64)),
            )
        };
        let empty = create("0", "0", "0", "", true).expect("complete empty portfolio");
        assert_eq!(empty.selected_candidate_count(), "0");
        assert_eq!(empty.canonical_first_candidate_id(), "");
        for (source, required, probability, first, exact) in [
            ("1", "0", "0", "", true),
            ("0", "1", "0", "", true),
            ("0", "0", "1", "", true),
            ("0", "0", "0", "candidate", true),
            ("0", "0", "0", "", false),
        ] {
            assert!(create(source, required, probability, first, exact).is_err());
        }
    }

    #[test]
    fn build_coverage_portfolio_rejects_invalid_counts_and_owner_identity() {
        let invalid_count = BuildCoveragePortfolioV2Payload::try_new(
            "build-coverage-portfolio.v2",
            "min-cover",
            "exact",
            "1",
            "2",
            "1",
            "1",
            "1",
            "a".repeat(64),
            "candidate",
            complete_build_evidence(),
            false,
            None,
        );
        assert_eq!(
            invalid_count,
            Err(BuildCoveragePortfolioPayloadError::CandidateCountInvalid)
        );

        let invalid_owner = BuildCoveragePortfolioV2Payload::try_new(
            "build-coverage-portfolio.v2",
            "min-cover",
            "exact",
            "2",
            "1",
            "1",
            "1",
            "1",
            "a".repeat(64),
            "candidate",
            complete_build_evidence(),
            true,
            None,
        );
        assert_eq!(
            invalid_owner,
            Err(BuildCoveragePortfolioPayloadError::PageSourceInvalid)
        );
    }

    #[test]
    fn build_coverage_portfolio_rejects_incomplete_or_noncanonical_identity() {
        let incomplete = BuildCoveragePortfolioV2Payload::try_new(
            "build-coverage-portfolio.v2",
            "min-cover",
            "exact",
            "2",
            "1",
            "1",
            "1",
            "1",
            "a".repeat(64),
            "candidate",
            BuildCoverageCompletenessPayload::new(true, true, true, false, true),
            false,
            None,
        );
        assert_eq!(
            incomplete,
            Err(BuildCoveragePortfolioPayloadError::CompletenessInvalid)
        );

        let invalid_hash = BuildCoveragePortfolioV2Payload::try_new(
            "build-coverage-portfolio.v2",
            "min-cover",
            "exact",
            "2",
            "1",
            "1",
            "1",
            "1",
            "",
            "candidate",
            complete_build_evidence(),
            false,
            None,
        );
        assert_eq!(
            invalid_hash,
            Err(BuildCoveragePortfolioPayloadError::IdentityInvalid(
                "normalized_solution_set_hash"
            ))
        );
    }

    #[test]
    fn build_setup_family_round_trips_and_rejects_count_drift() {
        let rows = vec![
            BuildSetupCandidateCoverageV1Payload::try_new("a", "1").unwrap(),
            BuildSetupCandidateCoverageV1Payload::try_new("b", "0").unwrap(),
        ];
        let payload = BuildSetupFamilyV1Payload::try_new(
            "build-target-family.v2",
            "a".repeat(64),
            "b".repeat(64),
            "unique",
            "2",
            "1",
            "3",
            "1",
            "0.5",
            BuildSetupCompletenessPayload::new(true, true, true, true, true),
            rows.clone(),
        )
        .unwrap();
        let json = serde_json::to_string(&payload).unwrap();
        assert_eq!(
            serde_json::from_str::<BuildSetupFamilyV1Payload>(&json).unwrap(),
            payload
        );
        assert_eq!(
            BuildSetupFamilyV1Payload::try_new(
                "build-target-family.v2",
                "a".repeat(64),
                "b".repeat(64),
                "unique",
                "2",
                "2",
                "3",
                "1",
                "0.5",
                BuildSetupCompletenessPayload::new(true, true, true, true, true),
                rows
            ),
            Err(BuildSetupFamilyPayloadError::CountMismatch)
        );
    }

    #[test]
    fn score_winner_family_requires_and_round_trips_the_core_owned_witness() {
        let canonical = ScorePatternWinnerPayload::new("0", "2", "solution-2", "1200", "9");
        let family = ScorePatternWinnerFamilyPayload::new(
            "pc-score-pattern-winner.v1",
            "pattern-id-ascending-then-candidate-id-ascending",
            "score-only-attack-informational",
            "canonical-equal-score-trace",
            "100",
            "1",
            "smallest-canonical-candidate-id",
            canonical.clone(),
            vec![canonical],
        );
        let value = serde_json::to_value(&family).expect("score winner family JSON");
        assert_eq!(
            serde_json::from_value::<ScorePatternWinnerFamilyPayload>(value.clone())
                .expect("typed score winner family"),
            family
        );

        for missing in ["canonical_selection", "canonical_winner"] {
            let mut incomplete = value.clone();
            incomplete
                .as_object_mut()
                .expect("family object")
                .remove(missing);
            assert!(
                serde_json::from_value::<ScorePatternWinnerFamilyPayload>(incomplete).is_err(),
                "{missing} must be mandatory"
            );
        }
    }
}
