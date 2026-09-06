// SRP rationale: this module has one behavior-level change reason: projecting validated product execution evidence into typed host payloads.

use std::sync::Arc;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_host_contract::{
    AppCommandKind, BuildCoverageCompletenessPayload, BuildCoveragePortfolioV2Payload,
    CoveragePortfolioPagePayload, ExecutionAvailabilityState, ExecutionCompletenessState,
    PcBestSavePayload, PcBestSaveWinnerPayload, PcPathFamilyPayload, PcPathStepPayload,
    PcPathWitnessPayload, PcSaveCompletenessPayload, PcSaveGroupPayload, PcSaveGroupsPayload,
    PcSavePieceMultisetPayload, PcSaveRunMetadataPayload, PcSaveWitnessPayload,
    PcScoreFieldPayload, PcScoreFieldSummaryPayload, ProductCandidateMemberPayload,
    ProductResultPayload, ProductResultPayloadContent, QueryEnvelope,
    ScorePatternWinnerFamilyPayload, ScorePatternWinnerPayload,
};

use crate::{
    app_response::{AppResponse, AppStatus},
    build_solution_probability_result::build_v2_facade::{BuildCoveragePortfolioV2, BuildSetupV1},
    pc_allspin_result::PcAllSpinResultReport,
    pc_chance_probability_result::PcProbabilityV2Result,
    pc_failed_queue_result::PcFailedQueueV2Result,
    pc_minimum_cover_result::{
        validate_pc_minimum_cover_v2_source, PcMinimumCoverV2Preparation,
        PcMinimumCoverV2PreparationAdvance, PcMinimumCoverV2Result,
    },
    pc_path_result::{validate_pc_path_family_v2_result, PcPathFamilyV2Result},
    pc_save_result::{
        PcBestSaveV2Result, PcSaveCompletenessEvidence, PcSaveExecutionReport, PcSaveGroupV2,
        PcSaveGroupsV2Result, PcSavePieceMultiset, PcSaveResultMode, PcSaveWitness,
        PC_BEST_SAVE_SCHEMA,
    },
    pc_score_minimum_cover_result::PcScorePortfolioV2Result,
    pc_score_summary_result::PcScoreSummaryV2Result,
    pc_tiling_family_result::PcTilingFamilyV1Result,
    portfolio_alternative_store::ProductPageSourceOwner,
    product_capability_contract::{
        ProductCapabilityContract, ProductCapabilityContractError,
        ValidatedProductCapabilityContract,
    },
    render::AppRenderModel,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductCapabilityResultKind {
    PcTilingFamilyV1,
    PcSaveGroupsV2,
    PcBestSaveV2,
    PcMinimumCoverV2,
    PcPathFamilyV2,
    PcProbabilityV2,
    PcFailedQueueV2,
    PcScoreSummaryV2,
    PcFixedScoreWitnessV2,
    PcScorePortfolioV2,
    PcB2bPreservingWitnessV1,
    PcB2bPreservationProbabilityV1,
    BuildCoveragePortfolioV2,
    BuildSetupFamilyV1,
}

impl ProductCapabilityResultKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PcTilingFamilyV1 => "pc-tiling-family.v1",
            Self::PcSaveGroupsV2 => "pc-save-groups.v2",
            Self::PcBestSaveV2 => "pc-best-save.v2",
            Self::PcMinimumCoverV2 => "pc-minimum-cover.v2",
            Self::PcPathFamilyV2 => "pc-path-family.v2",
            Self::PcProbabilityV2 => "pc-probability.v2",
            Self::PcFailedQueueV2 => "pc-failed-queue.v2",
            Self::PcScoreSummaryV2 => "pc-score-summary.v2",
            Self::PcFixedScoreWitnessV2 => "pc-fixed-score-witness.v2",
            Self::PcScorePortfolioV2 => "pc-score-portfolio.v2",
            Self::PcB2bPreservingWitnessV1 => "pc-b2b-preserving-witness.v1",
            Self::PcB2bPreservationProbabilityV1 => "pc-b2b-preservation-probability.v1",
            Self::BuildCoveragePortfolioV2 => "build-coverage-portfolio.v2",
            Self::BuildSetupFamilyV1 => "build-target-family.v2",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductCapabilityResourceEvidence {
    solver_executed: bool,
    availability: ExecutionAvailabilityState,
    completeness: ExecutionCompletenessState,
    truncated: bool,
    probability_complete: bool,
}

impl ProductCapabilityResourceEvidence {
    pub const fn solver_executed(self) -> bool {
        self.solver_executed
    }

    pub const fn availability(self) -> ExecutionAvailabilityState {
        self.availability
    }

    pub const fn completeness(self) -> ExecutionCompletenessState {
        self.completeness
    }

    pub const fn truncated(self) -> bool {
        self.truncated
    }

    pub const fn probability_complete(self) -> bool {
        self.probability_complete
    }
}

/// A target result that can only be constructed after both halves of the
/// product capability contract have passed fieldwise validation.
#[derive(Clone, Debug, PartialEq)]
pub struct ProductCapabilityResult {
    contract: ProductCapabilityContract,
    result_kind: ProductCapabilityResultKind,
    command_kind: AppCommandKind,
    query: QueryEnvelope,
    pc_probability_v2: Option<PcProbabilityV2Result>,
    pc_failed_queue_v2: Option<PcFailedQueueV2Result>,
    pc_score_summary_v2: Option<PcScoreSummaryV2Result>,
    pc_score_portfolio_v2: Option<Arc<PcScorePortfolioV2Result>>,
    pc_tiling_family_v1: Option<PcTilingFamilyV1Result>,
    pc_minimum_cover_v2: Option<PcMinimumCoverV2Result>,
    pc_path_family_v2: Option<PcPathFamilyV2Result>,
    pc_save_groups_v2: Option<PcSaveGroupsV2Result>,
    pc_best_save_v2: Option<PcBestSaveV2Result>,
    build_coverage_portfolio_v2: Option<Arc<BuildCoveragePortfolioV2>>,
    build_setup_v1: Option<BuildSetupV1>,
    resource_evidence: ProductCapabilityResourceEvidence,
    validation_count: u8,
}

#[derive(Debug)]
pub(crate) enum PcMinimumCoverProductPreparationAdvance {
    Pending { work_steps: u64 },
    Completed(ProductCapabilityResult),
    Cancelled { work_steps: u64 },
}

/// Exactly-once product wrapper continuation for `pc.minimals`.
/// Response-envelope, resource, query and source evidence validation all run
/// in the constructor; later advances own only exact proof/canonical work.
pub(crate) struct PcMinimumCoverProductPreparation {
    validated: Option<ValidatedProductCapabilityContract>,
    report: PcMinimumCoverV2Preparation,
}

impl PcMinimumCoverProductPreparation {
    pub(crate) fn parallel_source_dimensions(&self) -> Option<(usize, usize)> {
        self.report.parallel_source_dimensions()
    }

    /// Heap-only upper bound; the enclosing cooperative owner charges its
    /// inline storage separately. Shared query Arcs are conservatively counted
    /// per proof/source owner, never replaced with an unknown zero allowance.
    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.report
            .checked_retained_capacity_bytes()?
            .checked_add(match &self.validated {
                Some(validated) => validated.checked_minimum_cover_retained_capacity_bytes()?,
                None => 0,
            })
    }

    pub(crate) fn enable_parallel(
        &mut self,
        partitions: usize,
    ) -> Result<(), ProductCapabilityContractError> {
        self.report
            .enable_parallel(partitions)
            .map_err(ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch)
    }

    pub(crate) fn parallel_query_satisfied(&self) -> bool {
        self.report.parallel_query_satisfied()
    }

    pub(crate) fn parallel_query(&self) -> Option<&clearra_coverage::cover::ExactAtMostQuery> {
        self.report.parallel_query()
    }

    pub(crate) fn take_parallel_task(
        &mut self,
    ) -> Option<clearra_coverage::cover::ExactAtMostTask> {
        self.report.take_parallel_task()
    }

    pub(crate) fn prepare_parallel_idle_assist(
        &mut self,
        maximum_children: usize,
        guard: &mut impl FnMut(u128) -> Result<(), clearra_coverage::cover::ExactMinimumCoverError>,
    ) -> Result<bool, &'static str> {
        self.report
            .prepare_parallel_idle_assist(maximum_children, guard)
    }

    pub(crate) fn parallel_task_is_redundant(
        &self,
        identity: clearra_coverage::cover::ExactAtMostQueryIdentity,
        partition_id: u64,
    ) -> Result<bool, &'static str> {
        self.report
            .parallel_task_is_redundant(identity, partition_id)
    }

    pub(crate) fn accept_parallel_receipt(
        &mut self,
        receipt: clearra_coverage::cover::ExactAtMostReceipt,
    ) -> Result<(), ProductCapabilityContractError> {
        self.report
            .accept_parallel_receipt(receipt)
            .map_err(ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch)
    }

    pub(crate) fn advance(
        &mut self,
        maximum_work_steps: u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PcMinimumCoverProductPreparationAdvance, ProductCapabilityContractError> {
        self.advance_with_memory_guard(maximum_work_steps, &mut |_| Ok(()), cancelled)
    }

    /// The host receives a whole preparation inline+heap peak. It can replace
    /// the preparation's previous bytes inside its full App memory envelope
    /// without recounting the retained response for every solver allocation.
    pub(crate) fn advance_with_memory_guard(
        &mut self,
        maximum_work_steps: u64,
        memory_guard: &mut impl FnMut(
            u128,
        )
            -> Result<(), clearra_coverage::cover::ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PcMinimumCoverProductPreparationAdvance, ProductCapabilityContractError> {
        let validated_heap = match &self.validated {
            Some(validated) => validated.checked_minimum_cover_retained_capacity_bytes(),
            None => Some(0),
        };
        let outer_live = validated_heap
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() as u128))
            .and_then(|bytes| {
                bytes.checked_sub(core::mem::size_of::<PcMinimumCoverV2Preparation>() as u128)
            })
            .ok_or(
                ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch(
                    "pc_minimum_cover_memory_projection_overflow",
                ),
            )?;
        match self
            .report
            .advance_with_memory_guard(
                maximum_work_steps,
                &mut |report_peak| {
                    memory_guard(outer_live.checked_add(report_peak).ok_or(
                        clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
                    )?)
                },
                cancelled,
            )
            .map_err(ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch)?
        {
            PcMinimumCoverV2PreparationAdvance::Pending { work_steps } => {
                Ok(PcMinimumCoverProductPreparationAdvance::Pending { work_steps })
            }
            PcMinimumCoverV2PreparationAdvance::Completed(report) => {
                let completed_peak = self
                    .checked_retained_capacity_bytes()
                    .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() as u128))
                    .and_then(|bytes| {
                        bytes.checked_add(core::mem::size_of::<ProductCapabilityResult>() as u128)
                    })
                    .and_then(|bytes| bytes.checked_add(report.checked_retained_capacity_bytes()?))
                    .ok_or(
                        ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch(
                            "pc_minimum_cover_memory_projection_overflow",
                        ),
                    )?;
                memory_guard(completed_peak).map_err(|_| {
                    ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch(
                        "pc_minimum_cover_memory_limit_exceeded",
                    )
                })?;
                let validated = self.validated.take().ok_or(
                    ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch(
                        "pc minimals product preparation completed more than once",
                    ),
                )?;
                ProductCapabilityResult::from_validated_pc_minimum_cover_report(validated, report)
                    .map(PcMinimumCoverProductPreparationAdvance::Completed)
            }
            PcMinimumCoverV2PreparationAdvance::Cancelled { work_steps } => {
                self.validated = None;
                Ok(PcMinimumCoverProductPreparationAdvance::Cancelled { work_steps })
            }
        }
    }

    fn complete(mut self) -> Result<ProductCapabilityResult, ProductCapabilityContractError> {
        loop {
            match self.advance(u64::MAX, &mut || false)? {
                PcMinimumCoverProductPreparationAdvance::Pending { work_steps } => {
                    if work_steps == 0 {
                        return Err(
                            ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch(
                                "pc minimals product preparation made no progress",
                            ),
                        );
                    }
                }
                PcMinimumCoverProductPreparationAdvance::Completed(result) => return Ok(result),
                PcMinimumCoverProductPreparationAdvance::Cancelled { .. } => {
                    return Err(
                        ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch(
                            "pc minimals product preparation was cancelled",
                        ),
                    )
                }
            }
        }
    }
}

impl Eq for ProductCapabilityResult {}

impl ProductCapabilityResult {
    pub const fn contract(&self) -> ProductCapabilityContract {
        self.contract
    }

    pub const fn result_kind(&self) -> ProductCapabilityResultKind {
        self.result_kind
    }

    pub const fn command_kind(&self) -> AppCommandKind {
        self.command_kind
    }

    pub fn query(&self) -> &QueryEnvelope {
        &self.query
    }

    pub const fn resource_evidence(&self) -> ProductCapabilityResourceEvidence {
        self.resource_evidence
    }

    pub const fn validation_count(&self) -> u8 {
        self.validation_count
    }

    pub fn pc_probability_v2(&self) -> Option<&PcProbabilityV2Result> {
        self.pc_probability_v2.as_ref()
    }

    pub fn pc_failed_queue_v2(&self) -> Option<&PcFailedQueueV2Result> {
        self.pc_failed_queue_v2.as_ref()
    }

    pub fn pc_score_summary_v2(&self) -> Option<&PcScoreSummaryV2Result> {
        self.pc_score_summary_v2.as_ref()
    }

    pub fn pc_score_portfolio_v2(&self) -> Option<&PcScorePortfolioV2Result> {
        self.pc_score_portfolio_v2.as_deref()
    }

    pub fn pc_tiling_family_v1(&self) -> Option<&PcTilingFamilyV1Result> {
        self.pc_tiling_family_v1.as_ref()
    }

    pub fn pc_minimum_cover_v2(&self) -> Option<&PcMinimumCoverV2Result> {
        self.pc_minimum_cover_v2.as_ref()
    }

    pub fn pc_path_family_v2(&self) -> Option<&PcPathFamilyV2Result> {
        self.pc_path_family_v2.as_ref()
    }

    pub fn pc_save_groups_v2(&self) -> Option<&PcSaveGroupsV2Result> {
        self.pc_save_groups_v2.as_ref()
    }

    pub fn pc_best_save_v2(&self) -> Option<&PcBestSaveV2Result> {
        self.pc_best_save_v2.as_ref()
    }

    pub fn build_coverage_portfolio_v2(&self) -> Option<&BuildCoveragePortfolioV2> {
        self.build_coverage_portfolio_v2.as_deref()
    }

    pub fn build_setup_v1(&self) -> Option<&BuildSetupV1> {
        self.build_setup_v1.as_ref()
    }

    pub fn from_build_coverage_portfolio_v2(
        report: BuildCoveragePortfolioV2,
    ) -> Result<Self, ProductCapabilityContractError> {
        if report.contract_id() != ProductCapabilityResultKind::BuildCoveragePortfolioV2.as_str()
            || !report.completeness().complete()
            || (report.canonical_candidate_keys().is_empty()
                && (report.source_candidate_count() != 0
                    || report.selected_candidate_count() != 0
                    || report.required_pattern_count() != 0
                    || report.union_probability() != "0"))
        {
            return Err(ProductCapabilityContractError::ResponseResultContractMismatch);
        }
        let owner = report
            .portfolio_alternative_owner()
            .ok_or(ProductCapabilityContractError::ResponseResultContractMismatch)?;
        if owner.set_identity_sha256().len() != 64 {
            return Err(ProductCapabilityContractError::ResponseResultContractMismatch);
        }
        Ok(Self {
            contract: ProductCapabilityContract::BuildCover,
            result_kind: ProductCapabilityResultKind::BuildCoveragePortfolioV2,
            command_kind: AppCommandKind::BuildProbability,
            query: QueryEnvelope::BuildCoverage,
            pc_probability_v2: None,
            pc_failed_queue_v2: None,
            pc_score_summary_v2: None,
            pc_score_portfolio_v2: None,
            pc_tiling_family_v1: None,
            pc_minimum_cover_v2: None,
            pc_path_family_v2: None,
            pc_save_groups_v2: None,
            pc_best_save_v2: None,
            build_coverage_portfolio_v2: Some(Arc::new(report)),
            build_setup_v1: None,
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: true,
            },
            validation_count: 1,
        })
    }

    pub fn from_build_setup_v1(
        report: BuildSetupV1,
    ) -> Result<Self, ProductCapabilityContractError> {
        if report.contract_id() != ProductCapabilityResultKind::BuildSetupFamilyV1.as_str()
            || !report.completeness().replay_complete()
            || crate::project_build_setup_v1(&report).is_err()
        {
            return Err(ProductCapabilityContractError::ResponseResultContractMismatch);
        }
        Ok(Self {
            contract: ProductCapabilityContract::BuildSetup,
            result_kind: ProductCapabilityResultKind::BuildSetupFamilyV1,
            command_kind: AppCommandKind::BuildProbability,
            query: QueryEnvelope::BuildCoverage,
            pc_probability_v2: None,
            pc_failed_queue_v2: None,
            pc_score_summary_v2: None,
            pc_score_portfolio_v2: None,
            pc_tiling_family_v1: None,
            pc_minimum_cover_v2: None,
            pc_path_family_v2: None,
            pc_save_groups_v2: None,
            pc_best_save_v2: None,
            build_coverage_portfolio_v2: None,
            build_setup_v1: Some(report),
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: true,
            },
            validation_count: 1,
        })
    }

    /// Finite, serializable public DTO for product result families whose GUI
    /// surface is active. Live portfolio enumeration remains behind the
    /// separately owned page handle; this descriptor carries the canonical
    /// first alternative and a fixed 100-member page only.
    pub fn public_result_payload(&self) -> Option<ProductResultPayload> {
        match (self.contract, self.result_kind) {
            (
                ProductCapabilityContract::BuildSetup,
                ProductCapabilityResultKind::BuildSetupFamilyV1,
            ) => {
                let report = self.build_setup_v1.as_ref()?;
                let payload = crate::project_build_setup_v1(report).ok()?;
                Some(ProductResultPayload::new(
                    self.contract.as_str(),
                    self.result_kind.as_str(),
                    ProductResultPayloadContent::BuildSetupFamilyV1(payload),
                ))
            }
            (
                ProductCapabilityContract::BuildCover,
                ProductCapabilityResultKind::BuildCoveragePortfolioV2,
            ) => {
                let report = self.build_coverage_portfolio_v2.as_deref()?;
                let owner = report.portfolio_alternative_owner()?;
                let completeness = report.completeness();
                let payload = BuildCoveragePortfolioV2Payload::try_new(
                    report.contract_id(),
                    report.objective().as_str(),
                    report.probability_basis(),
                    report.source_candidate_count().to_string(),
                    report.selected_candidate_count().to_string(),
                    report.pattern_count().to_string(),
                    report.required_pattern_count().to_string(),
                    report.union_probability(),
                    report.normalized_solution_set_hash(),
                    report
                        .canonical_candidate_keys()
                        .first()
                        .map_or("", String::as_str),
                    BuildCoverageCompletenessPayload::new(
                        completeness.source_universe_complete(),
                        completeness.coverage_rows_complete(),
                        completeness.probability_weights_complete(),
                        completeness.exact_minimum_proven(),
                        completeness.query_bound(),
                    ),
                    true,
                    Some(owner.set_identity_sha256().to_owned()),
                )
                .ok()?;
                Some(ProductResultPayload::new(
                    self.contract.as_str(),
                    self.result_kind.as_str(),
                    ProductResultPayloadContent::BuildCoveragePortfolioV2(payload),
                ))
            }
            (
                ProductCapabilityContract::PcMinimals,
                ProductCapabilityResultKind::PcMinimumCoverV2,
            ) => {
                let report = self.pc_minimum_cover_v2.as_ref()?;
                let set = report.portfolio_alternatives();
                let page = set.canonical_page();
                let member_count = page.portfolio().candidate_ids().len();
                let end = member_count.min(crate::PORTFOLIO_MEMBER_PAGE_SIZE);
                let mut members = Vec::with_capacity(end);
                for candidate_id in &page.portfolio().candidate_ids()[..end] {
                    let index = candidate_id
                        .checked_sub(1)
                        .and_then(|value| usize::try_from(value).ok())?;
                    let candidate = set.candidates().get(index)?;
                    if candidate.candidate_id() != *candidate_id {
                        return None;
                    }
                    members.push(ProductCandidateMemberPayload::new(
                        candidate_id.to_string(),
                        candidate.normalized_key(),
                    ));
                }
                let canonical_candidate = report.canonical_candidate();
                if canonical_candidate.is_none()
                    && (page.optimal_cardinality() != 0
                        || !page.portfolio().candidate_ids().is_empty())
                {
                    return None;
                }
                let page_payload = CoveragePortfolioPagePayload::new(
                    set.contract_id(),
                    page.contract_id(),
                    crate::PORTFOLIO_MEMBER_PAGE_CONTRACT,
                    set.set_identity_sha256(),
                    set.candidate_map_sha256(),
                    page.alternative_index_decimal(),
                    page.optimal_cardinality().to_string(),
                    page.known_alternative_count_decimal(),
                    page.total_alternative_count_decimal()
                        .map(ToOwned::to_owned),
                    page.enumeration_complete(),
                    "1",
                    member_count
                        .div_ceil(crate::PORTFOLIO_MEMBER_PAGE_SIZE)
                        .max(1)
                        .to_string(),
                    members,
                    true,
                );
                let page_payload = match canonical_candidate {
                    Some((canonical_candidate_id, canonical_solution_key)) => page_payload
                        .with_canonical_witness(
                            report.canonical_selection(),
                            ProductCandidateMemberPayload::new(
                                canonical_candidate_id.to_string(),
                                canonical_solution_key,
                            ),
                        ),
                    None => page_payload,
                };
                Some(ProductResultPayload::new(
                    self.contract.as_str(),
                    self.result_kind.as_str(),
                    ProductResultPayloadContent::CoveragePortfolio(page_payload),
                ))
            }
            (ProductCapabilityContract::PcPath, ProductCapabilityResultKind::PcPathFamilyV2) => {
                let report = self.pc_path_family_v2.as_ref()?;
                let witnesses = report
                    .witnesses()
                    .iter()
                    .map(|witness| {
                        let steps = witness
                            .steps()
                            .iter()
                            .map(|step| {
                                PcPathStepPayload::new(
                                    step.step_index().to_string(),
                                    step.operation_id().to_string(),
                                    step.active_piece().as_ascii().to_string(),
                                    step.input_cursor().to_string(),
                                    step.output_cursor().to_string(),
                                    step.input_hold_piece()
                                        .map(|piece| piece.as_ascii().to_string()),
                                    step.output_hold_piece()
                                        .map(|piece| piece.as_ascii().to_string()),
                                    step.hold_decision(),
                                    step.rotation().to_string(),
                                    step.x().to_string(),
                                    step.y().to_string(),
                                    format!("0x{:016x}", step.placement_mask()),
                                    format!("0x{:016x}", step.board_before_mask()),
                                    format!("0x{:016x}", step.board_after_placement_mask()),
                                    format!("0x{:016x}", step.board_after_line_clear_mask()),
                                    format!("0x{:016x}", step.cleared_row_mask()),
                                    step.cleared_lines().to_string(),
                                    step.line_clear_identity(),
                                )
                            })
                            .collect::<Vec<_>>();
                        PcPathWitnessPayload::new(
                            witness.candidate_id().to_string(),
                            witness.producer_candidate_id().to_string(),
                            witness.pattern_id().to_string(),
                            witness.trace_identity(),
                            witness.normalized_trace_key(),
                            witness.consumed_piece_count().to_string(),
                            witness
                                .terminal_hold_piece()
                                .map(|piece| piece.as_ascii().to_string()),
                            steps,
                        )
                    })
                    .collect::<Vec<_>>();
                let canonical_witness = match report.canonical_witness() {
                    Some(_) => Some(witnesses.first()?.clone()),
                    None => None,
                };
                Some(ProductResultPayload::new(
                    self.contract.as_str(),
                    self.result_kind.as_str(),
                    ProductResultPayloadContent::PcPathFamily(
                        PcPathFamilyPayload::new(
                            report.witness_contract(),
                            report.ordering(),
                            report.problem_id(),
                            report.materialized_pattern_count().to_string(),
                            report.witness_count().to_string(),
                            report.completeness().complete(),
                            report.canonical_selection(),
                            canonical_witness,
                            witnesses,
                        )
                        .with_optional_page_metadata(
                            report.page_source().and_then(|source| {
                                (source.geometry_count() != 0).then(|| {
                                    source.page_metadata(1, 1).expect(
                                        "validated replay source owns its canonical member page",
                                    )
                                })
                            }),
                        ),
                    ),
                ))
            }
            (ProductCapabilityContract::PcScore, ProductCapabilityResultKind::PcScoreSummaryV2) => {
                let report = self.pc_score_summary_v2.as_ref()?;
                let fields = report
                    .solution_field_averages()
                    .iter()
                    .map(|field| {
                        PcScoreFieldPayload::new(
                            field.normalized_field_key().as_str(),
                            field.average_score().to_string(),
                            field.covered_pattern_count().to_string(),
                            field.pattern_count().to_string(),
                            field.score_complete(),
                        )
                    })
                    .collect::<Vec<_>>();
                Some(ProductResultPayload::new(
                    self.contract.as_str(),
                    self.result_kind.as_str(),
                    ProductResultPayloadContent::PcScoreFieldSummary(
                        PcScoreFieldSummaryPayload::new(
                            crate::PC_SCORE_SOLUTION_FIELD_CONTRACT,
                            report.solution_field_ordering(),
                            report.solution_field_average_basis(),
                            report.score_evaluation_basis(),
                            report.score_evaluation_scope(),
                            report.overall_score_basis(),
                            report.piece_source_id().to_string(),
                            report.pattern_universe_id().to_string(),
                            report.pattern_weight_model_id().to_string(),
                            report.materialized_pattern_count().to_string(),
                            report.solution_field_count().to_string(),
                            report.pattern_optimal_count().to_string(),
                            report.failed_pc_pattern_count().to_string(),
                            report.covered_probability(),
                            report.overall_score(),
                            report
                                .covered_pattern_conditional_average_score()
                                .map(ToOwned::to_owned),
                            report.completeness().complete(),
                            fields,
                        ),
                    ),
                ))
            }
            (
                ProductCapabilityContract::PcScoreFinder,
                ProductCapabilityResultKind::PcFixedScoreWitnessV2,
            ) => {
                let report = self.pc_score_summary_v2.as_ref()?;
                let canonical_winner = report.canonical_winner()?;
                let canonical_winner_payload = ScorePatternWinnerPayload::new(
                    canonical_winner.pattern_id().to_string(),
                    canonical_winner.candidate_id().to_string(),
                    canonical_winner.normalized_solution_key().to_string(),
                    canonical_winner.score().to_string(),
                    canonical_winner.informational_attack().to_string(),
                );
                let winners = report
                    .pattern_winners()
                    .iter()
                    .map(|winner| {
                        ScorePatternWinnerPayload::new(
                            winner.pattern_id().to_string(),
                            winner.candidate_id().to_string(),
                            winner.normalized_solution_key().to_string(),
                            winner.score().to_string(),
                            winner.informational_attack().to_string(),
                        )
                    })
                    .collect::<Vec<_>>();
                Some(ProductResultPayload::new(
                    self.contract.as_str(),
                    self.result_kind.as_str(),
                    ProductResultPayloadContent::ScorePatternWinnerFamily(
                        ScorePatternWinnerFamilyPayload::new(
                            crate::PC_SCORE_PATTERN_WINNER_CONTRACT,
                            "pattern-id-ascending-then-candidate-id-ascending",
                            "score-only-attack-informational",
                            crate::PC_SCORE_INFORMATIONAL_ATTACK_BASIS,
                            crate::PORTFOLIO_MEMBER_PAGE_SIZE.to_string(),
                            winners.len().to_string(),
                            report.canonical_selection(),
                            canonical_winner_payload,
                            winners,
                        ),
                    ),
                ))
            }
            (
                ProductCapabilityContract::PcScoreMinimals,
                ProductCapabilityResultKind::PcScorePortfolioV2,
            ) => {
                let report = self.pc_score_portfolio_v2.as_deref()?;
                let set = report.portfolio_alternatives();
                let page = set.canonical_page();
                let member_count = page.portfolio().candidate_ids().len();
                let end = member_count.min(crate::PORTFOLIO_MEMBER_PAGE_SIZE);
                let mut members = Vec::with_capacity(end);
                for dense_candidate_id in &page.portfolio().candidate_ids()[..end] {
                    let index = dense_candidate_id
                        .checked_sub(1)
                        .and_then(|value| usize::try_from(value).ok())?;
                    let candidate = set.candidates().get(index)?;
                    if candidate.candidate_id() != *dense_candidate_id {
                        return None;
                    }
                    let public_candidate_id = set.public_candidate_id(*dense_candidate_id)?;
                    members.push(ProductCandidateMemberPayload::new(
                        public_candidate_id.to_string(),
                        candidate.normalized_key(),
                    ));
                }
                Some(ProductResultPayload::new(
                    self.contract.as_str(),
                    self.result_kind.as_str(),
                    ProductResultPayloadContent::CoveragePortfolio(
                        CoveragePortfolioPagePayload::new(
                            set.contract_id(),
                            page.contract_id(),
                            crate::PORTFOLIO_MEMBER_PAGE_CONTRACT,
                            set.set_identity_sha256(),
                            set.candidate_map_sha256(),
                            page.alternative_index_decimal(),
                            page.optimal_cardinality().to_string(),
                            page.known_alternative_count_decimal(),
                            page.total_alternative_count_decimal()
                                .map(ToOwned::to_owned),
                            page.enumeration_complete(),
                            "1",
                            member_count
                                .div_ceil(crate::PORTFOLIO_MEMBER_PAGE_SIZE)
                                .max(1)
                                .to_string(),
                            members,
                            true,
                        ),
                    ),
                ))
            }
            (ProductCapabilityContract::PcSaves, ProductCapabilityResultKind::PcSaveGroupsV2) => {
                let report = self.pc_save_groups_v2.as_ref()?;
                let groups = report
                    .groups()
                    .iter()
                    .map(pc_save_group_payload)
                    .collect::<Vec<_>>();
                Some(ProductResultPayload::new(
                    self.contract.as_str(),
                    self.result_kind.as_str(),
                    ProductResultPayloadContent::PcSaveGroups(PcSaveGroupsPayload::new(
                        PC_BEST_SAVE_SCHEMA,
                        crate::PORTFOLIO_MEMBER_PAGE_SIZE.to_string(),
                        groups.len().to_string(),
                        pc_save_run_metadata(
                            report.origin().as_str(),
                            report.problem_preset().as_str(),
                            report.problem_id(),
                            report.piece_source_id(),
                            report.pattern_universe_id(),
                            report.pattern_weight_model_id(),
                            report.materialized_pattern_count(),
                            report.pc_success_pattern_count(),
                            report.pc_probability().decimal(),
                            report.completeness(),
                        ),
                        groups,
                    )),
                ))
            }
            (ProductCapabilityContract::PcBestSave, ProductCapabilityResultKind::PcBestSaveV2) => {
                let report = self.pc_best_save_v2.as_ref()?;
                let winners = report
                    .winners()
                    .iter()
                    .map(|winner| {
                        PcBestSaveWinnerPayload::new(
                            winner.weighted_total().to_string(),
                            winner.balanced_jl_count().to_string(),
                            winner.exact_group_probability().decimal(),
                            pc_save_group_payload(winner.group()),
                        )
                    })
                    .collect::<Vec<_>>();
                let canonical_winner = match report.canonical_winner() {
                    Some(_) => Some(winners.first()?.clone()),
                    None => None,
                };
                Some(ProductResultPayload::new(
                    self.contract.as_str(),
                    self.result_kind.as_str(),
                    ProductResultPayloadContent::PcBestSave(PcBestSavePayload::new(
                        report.schema_id(),
                        report.probability_basis(),
                        "weighted-total-descending-then-balanced-jl-descending-then-unconditional-probability-descending-then-canonical-candidate-id-ascending",
                        "weighted-total-balanced-jl-and-exact-unconditional-probability",
                        crate::PORTFOLIO_MEMBER_PAGE_SIZE.to_string(),
                        winners.len().to_string(),
                        report.canonical_selection(),
                        canonical_winner,
                        pc_save_run_metadata(
                            report.origin().as_str(),
                            report.problem_preset().as_str(),
                            report.problem_id(),
                            report.piece_source_id(),
                            report.pattern_universe_id(),
                            report.pattern_weight_model_id(),
                            report.materialized_pattern_count(),
                            report.pc_success_pattern_count(),
                            report.pc_probability().decimal(),
                            report.completeness(),
                        ),
                        winners,
                    )),
                ))
            }
            _ => None,
        }
    }

    /// Transfers only the immutable producer owner required by an activated
    /// live page family. Score winner pages are finite in the public DTO and
    /// therefore deliberately do not allocate a live handle. Saves and other
    /// unfinished products remain absent rather than being activated by this
    /// common lifecycle seam.
    pub fn public_page_source_owner(&self) -> Option<ProductPageSourceOwner> {
        match (self.contract, self.result_kind) {
            (ProductCapabilityContract::PcPath, ProductCapabilityResultKind::PcPathFamilyV2) => {
                self.pc_path_family_v2
                    .as_ref()
                    .and_then(|report| report.page_source())
                    .filter(|source| source.geometry_count() != 0)
                    .map(|source| ProductPageSourceOwner::PcReplay(Arc::clone(source)))
            }
            (
                ProductCapabilityContract::PcMinimals,
                ProductCapabilityResultKind::PcMinimumCoverV2,
            ) => self.pc_minimum_cover_v2.as_ref().map(|report| {
                ProductPageSourceOwner::CoveragePortfolio(
                    report.portfolio_alternative_owner().clone(),
                )
            }),
            (
                ProductCapabilityContract::PcScoreMinimals,
                ProductCapabilityResultKind::PcScorePortfolioV2,
            ) => self.pc_score_portfolio_v2.as_ref().map(|report| {
                ProductPageSourceOwner::CoveragePortfolio(
                    report.portfolio_alternative_owner().clone(),
                )
            }),
            (
                ProductCapabilityContract::BuildCover,
                ProductCapabilityResultKind::BuildCoveragePortfolioV2,
            ) => self
                .build_coverage_portfolio_v2
                .as_ref()
                .and_then(|report| {
                    report
                        .portfolio_alternative_owner()
                        .map(|owner| ProductPageSourceOwner::CoveragePortfolio(owner.clone()))
                }),
            _ => None,
        }
    }

    pub(crate) fn validate(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
    ) -> Result<Self, ProductCapabilityContractError> {
        Self::validate_with_optional_pc_replay_source(validated, response, None)
    }

    pub(crate) fn validate_with_pc_replay_source(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
        source: Arc<crate::PcReplayPageSource>,
    ) -> Result<Self, ProductCapabilityContractError> {
        if validated.contract() != ProductCapabilityContract::PcPath {
            return Err(
                ProductCapabilityContractError::ResponsePathEvidenceMismatch(
                    "replay source requires pc.path authority",
                ),
            );
        }
        Self::validate_with_optional_pc_replay_source(validated, response, Some(source))
    }

    fn validate_with_optional_pc_replay_source(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
        page_source: Option<Arc<crate::PcReplayPageSource>>,
    ) -> Result<Self, ProductCapabilityContractError> {
        if validated.contract() == ProductCapabilityContract::PcMinimals {
            return Self::prepare_pc_minimum_cover(validated, response)?.complete();
        }
        if response.status() != AppStatus::Success {
            return Err(ProductCapabilityContractError::ResponseStatusNotSuccessful);
        }
        if response.product_capability_result().is_some() {
            return Err(ProductCapabilityContractError::ResponseAlreadyWrapped);
        }
        if response.command() != Some(validated.command_kind()) {
            return Err(ProductCapabilityContractError::ResponseCommandMismatch);
        }

        let expected_app_result = validated.expected_result_kind();
        let result = response
            .result()
            .ok_or(ProductCapabilityContractError::ResponseResultMissing)?;
        if result.kind() != expected_app_result.as_str() {
            return Err(ProductCapabilityContractError::ResponseResultKindMismatch);
        }
        let render_model = response
            .render_model()
            .ok_or(ProductCapabilityContractError::ResponseRenderModelMissing)?;
        if render_model.kind() != expected_app_result {
            return Err(ProductCapabilityContractError::ResponseRenderKindMismatch);
        }

        if validated.contract() == ProductCapabilityContract::PcFailedQueue {
            let AppRenderModel::Percent(core_result) = render_model else {
                return Err(ProductCapabilityContractError::ResponseRenderFamilyMismatch);
            };
            return Self::validate_pc_failed_queue(validated, response, core_result);
        }

        let core_result = match (validated.query(), render_model) {
            (QueryEnvelope::PcOpening, AppRenderModel::Pc(result))
            | (QueryEnvelope::PcScenario, AppRenderModel::Scenario(result)) => result,
            _ => return Err(ProductCapabilityContractError::ResponseRenderFamilyMismatch),
        };

        if validated.contract() == ProductCapabilityContract::PcTiling {
            return Self::validate_pc_tiling(validated, response, core_result);
        }
        if matches!(
            validated.contract(),
            ProductCapabilityContract::PcSaves | ProductCapabilityContract::PcBestSave
        ) {
            return Self::validate_pc_save(validated, response, core_result);
        }
        if validated.contract() == ProductCapabilityContract::PcPath {
            return Self::validate_pc_path(validated, response, core_result, page_source);
        }
        if validated.contract() == ProductCapabilityContract::PcChance {
            return Self::validate_pc_chance(validated, response, core_result);
        }
        if matches!(
            validated.contract(),
            ProductCapabilityContract::PcScore | ProductCapabilityContract::PcScoreFinder
        ) {
            return Self::validate_pc_score(validated, response, core_result);
        }
        if validated.contract() == ProductCapabilityContract::PcScoreMinimals {
            return Self::validate_pc_score_minimals(validated, response, core_result);
        }

        let result_kind = match validated.contract() {
            ProductCapabilityContract::PcTiling => {
                unreachable!("pc tiling result returned through its dedicated validator")
            }
            ProductCapabilityContract::PcSaves | ProductCapabilityContract::PcBestSave => {
                unreachable!("pc save result returned through its dedicated validator")
            }
            ProductCapabilityContract::PcMinimals => {
                unreachable!("pc minimals result returned through its dedicated validator")
            }
            ProductCapabilityContract::PcPath => {
                unreachable!("pc path result returned through its dedicated validator")
            }
            ProductCapabilityContract::PcChance => {
                unreachable!("pc chance result returned through its dedicated validator")
            }
            ProductCapabilityContract::PcFailedQueue => {
                unreachable!("pc failed-queue result returned through its dedicated validator")
            }
            ProductCapabilityContract::PcScore => {
                unreachable!("pc score result returned through its dedicated validator")
            }
            ProductCapabilityContract::PcScoreFinder => {
                unreachable!("pc score-finder result returned through its dedicated validator")
            }
            ProductCapabilityContract::PcScoreMinimals => {
                unreachable!("pc score-minimals result returned through its dedicated validator")
            }
            ProductCapabilityContract::PcAllSpinSolution => {
                ProductCapabilityResultKind::PcB2bPreservingWitnessV1
            }
            ProductCapabilityContract::PcAllSpinPreservationChance => {
                ProductCapabilityResultKind::PcB2bPreservationProbabilityV1
            }
            ProductCapabilityContract::BuildCover => {
                unreachable!("build cover uses the direct validated facade constructor")
            }
            ProductCapabilityContract::BuildSetup => {
                unreachable!("build setup uses the direct validated facade constructor")
            }
        };
        let projection = validated
            .pc_allspin_projection()
            .ok_or(ProductCapabilityContractError::ResponseResultContractMismatch)?;
        let expected_profile = projection
            .spin_profile()
            .expect("validated product capability projections select a spin profile");
        let expected_preset = validated.expected_problem_preset();
        if core_result.field("pc_allspin_result_contract") != Some(result_kind.as_str())
            || core_result.field("pc_allspin_mode") != projection.mode()
            || core_result.field("pc_allspin_spin_profile") != Some(expected_profile.as_str())
            || core_result.field("pc_allspin_clear_contract") != Some("inverse-lock-clear-to-empty")
            || core_result.field("pc_allspin_denominator_semantics")
                != Some("original-materialized-queue")
            || core_result.field("pc_allspin_evaluation_basis")
                != Some("candidate-pattern-existence")
            || core_result.bool_field("pc_allspin_path_multiplicity_counted") != Some(false)
            || core_result.bool_field("pc_allspin_initial_field_supplied")
                != Some(expected_preset.initial_field_supplied())
            || core_result.bool_field("pc_allspin_target_field_supplied") != Some(false)
            || core_result.bool_field("pc_allspin_count_complete") != Some(true)
            || core_result.bool_field("pc_allspin_probability_complete") != Some(true)
            || core_result.bool_field("pc_allspin_complete") != Some(true)
            || core_result.field("pc_allspin_incomplete_reason") != Some("none")
        {
            return Err(ProductCapabilityContractError::ResponseResultContractMismatch);
        }

        let projection_report =
            PcAllSpinResultReport::from_execution_result(core_result, projection)
                .ok_or(ProductCapabilityContractError::ResponseResultContractMismatch)?;
        if !projection_report.complete() {
            return Err(ProductCapabilityContractError::ResponseResultProjectionIncomplete);
        }
        let expected_preset = expected_preset.as_str();
        if projection_report.problem_preset() != Some(expected_preset)
            || core_result.field("pc_allspin_problem_preset") != Some(expected_preset)
        {
            return Err(ProductCapabilityContractError::ResponseProblemPresetMismatch);
        }
        if core_result.usize_field("pc_allspin_preserving_queue_count")
            != projection_report.preserving_queue_count()
            || core_result.usize_field("pc_allspin_original_queue_count")
                != projection_report.original_queue_count()
            || core_result.field("pc_allspin_preservation_probability")
                != projection_report.preservation_probability()
        {
            return Err(ProductCapabilityContractError::ResponseResultContractMismatch);
        }

        if let Some(preserves_back_to_back) = projection_report.preserves_back_to_back() {
            if core_result.bool_field("pc_allspin_preserves_b2b") != Some(preserves_back_to_back)
                || core_result.bool_field("pc_allspin_witness_available")
                    != Some(projection_report.witness().is_some())
            {
                return Err(ProductCapabilityContractError::ResponseResultContractMismatch);
            }
            if let Some(witness) = projection_report.witness() {
                if core_result.field("pc_allspin_witness_candidate_key")
                    != Some(witness.candidate_key())
                    || core_result.usize_field("pc_allspin_witness_pattern_index")
                        != Some(witness.pattern_index())
                    || core_result.bool_field("pc_allspin_witness_deterministic") != Some(true)
                {
                    return Err(ProductCapabilityContractError::ResponseResultContractMismatch);
                }
            }
        }

        let resources = response.resource_report();
        if !resources.solver_executed() {
            return Err(ProductCapabilityContractError::SolverNotExecuted);
        }
        if resources.execution_availability().state() != ExecutionAvailabilityState::Available {
            return Err(ProductCapabilityContractError::ExecutionUnavailable);
        }
        if resources.execution_availability().reason().is_some() {
            return Err(ProductCapabilityContractError::AvailabilityReasonPresent);
        }
        if resources.result_completeness() != ExecutionCompletenessState::Complete {
            return Err(ProductCapabilityContractError::ResultIncomplete);
        }
        if resources.truncated() {
            return Err(ProductCapabilityContractError::ResultTruncated);
        }
        if resources.truncation_reason().is_some() {
            return Err(ProductCapabilityContractError::TruncationReasonPresent);
        }

        Ok(Self {
            contract: validated.contract(),
            result_kind,
            command_kind: validated.command_kind(),
            query: validated.query().clone(),
            pc_probability_v2: None,
            pc_failed_queue_v2: None,
            pc_score_summary_v2: None,
            pc_score_portfolio_v2: None,
            pc_tiling_family_v1: None,
            pc_minimum_cover_v2: None,
            pc_path_family_v2: None,
            pc_save_groups_v2: None,
            pc_best_save_v2: None,
            build_coverage_portfolio_v2: None,
            build_setup_v1: None,
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: resources.probability_complete(),
            },
            validation_count: 1,
        })
    }

    fn validate_pc_tiling(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
        core_result: &clearra_core_executor::CoreExecutionResult,
    ) -> Result<Self, ProductCapabilityContractError> {
        let (expected_query, expected_origin) = validated
            .pc_tiling_binding()
            .ok_or(ProductCapabilityContractError::ResponseTilingEvidenceMismatch)?;
        let evidence = response
            .pc_tiling_execution_evidence()
            .ok_or(ProductCapabilityContractError::ResponseTilingEvidenceMissing)?;
        let report = evidence.report();
        if !evidence.matches_core_result(core_result)
            || report.contract_id() != ProductCapabilityResultKind::PcTilingFamilyV1.as_str()
            || !expected_query.matches_snapshot(report.query())
            || report.origin() != expected_origin
            || report.problem_preset().as_str() != validated.expected_problem_preset().as_str()
            || !report.completeness().family_complete()
            || report.completeness().incomplete_reason() != "none"
            || !report.completeness().initial_page_complete()
        {
            return Err(ProductCapabilityContractError::ResponseTilingEvidenceMismatch);
        }

        let resources = response.resource_report();
        if !resources.solver_executed() {
            return Err(ProductCapabilityContractError::SolverNotExecuted);
        }
        if resources.execution_availability().state() != ExecutionAvailabilityState::Available {
            return Err(ProductCapabilityContractError::ExecutionUnavailable);
        }
        if resources.execution_availability().reason().is_some() {
            return Err(ProductCapabilityContractError::AvailabilityReasonPresent);
        }
        if resources.result_completeness() != ExecutionCompletenessState::Complete {
            return Err(ProductCapabilityContractError::ResultIncomplete);
        }
        if resources.truncated() {
            return Err(ProductCapabilityContractError::ResultTruncated);
        }
        if resources.truncation_reason().is_some() {
            return Err(ProductCapabilityContractError::TruncationReasonPresent);
        }

        Ok(Self {
            contract: ProductCapabilityContract::PcTiling,
            result_kind: ProductCapabilityResultKind::PcTilingFamilyV1,
            command_kind: validated.command_kind(),
            query: validated.query().clone(),
            pc_probability_v2: None,
            pc_failed_queue_v2: None,
            pc_score_summary_v2: None,
            pc_score_portfolio_v2: None,
            pc_tiling_family_v1: Some(report.clone()),
            pc_minimum_cover_v2: None,
            pc_path_family_v2: None,
            pc_save_groups_v2: None,
            pc_best_save_v2: None,
            build_coverage_portfolio_v2: None,
            build_setup_v1: None,
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: false,
            },
            validation_count: 1,
        })
    }

    fn validate_pc_save(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
        core_result: &clearra_core_executor::CoreExecutionResult,
    ) -> Result<Self, ProductCapabilityContractError> {
        let (expected_query, expected_origin) = validated
            .pc_save_binding()
            .ok_or(ProductCapabilityContractError::ResponseSaveEvidenceMismatch)?;
        let evidence = response
            .pc_save_execution_evidence()
            .ok_or(ProductCapabilityContractError::ResponseSaveEvidenceMissing)?;
        let report = evidence.report();
        let expected_mode = match validated.contract() {
            ProductCapabilityContract::PcSaves => PcSaveResultMode::SaveGroups,
            ProductCapabilityContract::PcBestSave => PcSaveResultMode::BestSave,
            _ => return Err(ProductCapabilityContractError::ResponseSaveEvidenceMismatch),
        };
        let report_matches = match report {
            PcSaveExecutionReport::SaveGroups(report) => {
                report.contract_id() == ProductCapabilityResultKind::PcSaveGroupsV2.as_str()
                    && expected_query.matches_snapshot(report.query())
                    && report.origin() == expected_origin
                    && report.problem_preset().as_str()
                        == validated.expected_problem_preset().as_str()
                    && report.completeness().complete()
            }
            PcSaveExecutionReport::BestSave(report) => {
                report.contract_id() == ProductCapabilityResultKind::PcBestSaveV2.as_str()
                    && report.schema_id() == "clearra-save-v1"
                    && report.probability_basis() == "whole-universe-unconditional"
                    && expected_query.matches_snapshot(report.query())
                    && report.origin() == expected_origin
                    && report.problem_preset().as_str()
                        == validated.expected_problem_preset().as_str()
                    && report.completeness().complete()
            }
        };
        if !evidence.matches_core_result(core_result)
            || report.mode() != expected_mode
            || !report_matches
        {
            return Err(ProductCapabilityContractError::ResponseSaveEvidenceMismatch);
        }

        let resources = response.resource_report();
        if !resources.solver_executed() {
            return Err(ProductCapabilityContractError::SolverNotExecuted);
        }
        if resources.execution_availability().state() != ExecutionAvailabilityState::Available {
            return Err(ProductCapabilityContractError::ExecutionUnavailable);
        }
        if resources.execution_availability().reason().is_some() {
            return Err(ProductCapabilityContractError::AvailabilityReasonPresent);
        }
        if resources.result_completeness() != ExecutionCompletenessState::Complete {
            return Err(ProductCapabilityContractError::ResultIncomplete);
        }
        if resources.truncated() {
            return Err(ProductCapabilityContractError::ResultTruncated);
        }
        if resources.truncation_reason().is_some() {
            return Err(ProductCapabilityContractError::TruncationReasonPresent);
        }
        if !resources.probability_complete() {
            return Err(ProductCapabilityContractError::ResourceProbabilityIncomplete);
        }

        let (result_kind, pc_save_groups_v2, pc_best_save_v2) = match report {
            PcSaveExecutionReport::SaveGroups(report) => (
                ProductCapabilityResultKind::PcSaveGroupsV2,
                Some(report.clone()),
                None,
            ),
            PcSaveExecutionReport::BestSave(report) => (
                ProductCapabilityResultKind::PcBestSaveV2,
                None,
                Some(report.clone()),
            ),
        };
        Ok(Self {
            contract: validated.contract(),
            result_kind,
            command_kind: validated.command_kind(),
            query: validated.query().clone(),
            pc_probability_v2: None,
            pc_failed_queue_v2: None,
            pc_score_summary_v2: None,
            pc_score_portfolio_v2: None,
            pc_tiling_family_v1: None,
            pc_minimum_cover_v2: None,
            pc_path_family_v2: None,
            pc_save_groups_v2,
            pc_best_save_v2,
            build_coverage_portfolio_v2: None,
            build_setup_v1: None,
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: true,
            },
            validation_count: 1,
        })
    }

    pub(crate) fn prepare_pc_minimum_cover(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
    ) -> Result<PcMinimumCoverProductPreparation, ProductCapabilityContractError> {
        if validated.contract() != ProductCapabilityContract::PcMinimals {
            return Err(ProductCapabilityContractError::UnexpectedContract {
                actual: validated.contract(),
            });
        }
        if response.status() != AppStatus::Success {
            return Err(ProductCapabilityContractError::ResponseStatusNotSuccessful);
        }
        if response.product_capability_result().is_some() {
            return Err(ProductCapabilityContractError::ResponseAlreadyWrapped);
        }
        if response.command() != Some(validated.command_kind()) {
            return Err(ProductCapabilityContractError::ResponseCommandMismatch);
        }
        let expected_app_result = validated.expected_result_kind();
        let result = response
            .result()
            .ok_or(ProductCapabilityContractError::ResponseResultMissing)?;
        if result.kind() != expected_app_result.as_str() {
            return Err(ProductCapabilityContractError::ResponseResultKindMismatch);
        }
        let render_model = response
            .render_model()
            .ok_or(ProductCapabilityContractError::ResponseRenderModelMissing)?;
        if render_model.kind() != expected_app_result {
            return Err(ProductCapabilityContractError::ResponseRenderKindMismatch);
        }
        let core_result = match (validated.query(), render_model) {
            (QueryEnvelope::PcOpening, AppRenderModel::Pc(result))
            | (QueryEnvelope::PcScenario, AppRenderModel::Scenario(result)) => result,
            _ => return Err(ProductCapabilityContractError::ResponseRenderFamilyMismatch),
        };
        let (query, origin) = validated.pc_minimum_cover_binding().ok_or(
            ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch(
                "validated pc.minimals query binding is missing",
            ),
        )?;
        let source = validate_pc_minimum_cover_v2_source(query, origin, core_result)
            .map_err(ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch)?;
        Self::validate_pc_minimum_cover_resources(response)?;
        let report = PcMinimumCoverV2Preparation::new(source)
            .map_err(ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch)?;
        Ok(PcMinimumCoverProductPreparation {
            validated: Some(validated),
            report,
        })
    }

    fn from_validated_pc_minimum_cover_report(
        validated: ValidatedProductCapabilityContract,
        report: PcMinimumCoverV2Result,
    ) -> Result<Self, ProductCapabilityContractError> {
        if report.contract_id() != ProductCapabilityResultKind::PcMinimumCoverV2.as_str()
            || report.problem_preset().as_str() != validated.expected_problem_preset().as_str()
            || !report.completeness().complete()
        {
            return Err(
                ProductCapabilityContractError::ResponseMinimumCoverEvidenceMismatch(
                    "pc-minimum-cover.v2 compact report contract mismatch",
                ),
            );
        }

        Ok(Self {
            contract: ProductCapabilityContract::PcMinimals,
            result_kind: ProductCapabilityResultKind::PcMinimumCoverV2,
            command_kind: validated.command_kind(),
            query: validated.query().clone(),
            pc_probability_v2: None,
            pc_failed_queue_v2: None,
            pc_score_summary_v2: None,
            pc_score_portfolio_v2: None,
            pc_tiling_family_v1: None,
            pc_minimum_cover_v2: Some(report),
            pc_path_family_v2: None,
            pc_save_groups_v2: None,
            pc_best_save_v2: None,
            build_coverage_portfolio_v2: None,
            build_setup_v1: None,
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: true,
            },
            validation_count: 1,
        })
    }

    fn validate_pc_minimum_cover_resources(
        response: &AppResponse,
    ) -> Result<(), ProductCapabilityContractError> {
        let resources = response.resource_report();
        if !resources.solver_executed() {
            return Err(ProductCapabilityContractError::SolverNotExecuted);
        }
        if resources.execution_availability().state() != ExecutionAvailabilityState::Available {
            return Err(ProductCapabilityContractError::ExecutionUnavailable);
        }
        if resources.execution_availability().reason().is_some() {
            return Err(ProductCapabilityContractError::AvailabilityReasonPresent);
        }
        if resources.result_completeness() != ExecutionCompletenessState::Complete {
            return Err(ProductCapabilityContractError::ResultIncomplete);
        }
        if resources.truncated() {
            return Err(ProductCapabilityContractError::ResultTruncated);
        }
        if resources.truncation_reason().is_some() {
            return Err(ProductCapabilityContractError::TruncationReasonPresent);
        }
        if !resources.probability_complete() {
            return Err(ProductCapabilityContractError::ResourceProbabilityIncomplete);
        }
        Ok(())
    }

    fn validate_pc_path(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
        core_result: &clearra_core_executor::CoreExecutionResult,
        page_source: Option<Arc<crate::PcReplayPageSource>>,
    ) -> Result<Self, ProductCapabilityContractError> {
        let (query, origin) = validated.pc_path_binding().ok_or(
            ProductCapabilityContractError::ResponsePathEvidenceMismatch(
                "validated pc.path query binding is missing",
            ),
        )?;
        let report = validate_pc_path_family_v2_result(query, origin, core_result, page_source)
            .map_err(ProductCapabilityContractError::ResponsePathEvidenceMismatch)?;
        if report.contract_id() != ProductCapabilityResultKind::PcPathFamilyV2.as_str()
            || report.problem_preset().as_str() != validated.expected_problem_preset().as_str()
            || !report.completeness().complete()
        {
            return Err(
                ProductCapabilityContractError::ResponsePathEvidenceMismatch(
                    "pc-path-family.v2 report contract mismatch",
                ),
            );
        }

        let resources = response.resource_report();
        if !resources.solver_executed() {
            return Err(ProductCapabilityContractError::SolverNotExecuted);
        }
        if resources.execution_availability().state() != ExecutionAvailabilityState::Available {
            return Err(ProductCapabilityContractError::ExecutionUnavailable);
        }
        if resources.execution_availability().reason().is_some() {
            return Err(ProductCapabilityContractError::AvailabilityReasonPresent);
        }
        if resources.result_completeness() != ExecutionCompletenessState::Complete {
            return Err(ProductCapabilityContractError::ResultIncomplete);
        }
        if resources.truncated() {
            return Err(ProductCapabilityContractError::ResultTruncated);
        }
        if resources.truncation_reason().is_some() {
            return Err(ProductCapabilityContractError::TruncationReasonPresent);
        }

        Ok(Self {
            contract: ProductCapabilityContract::PcPath,
            result_kind: ProductCapabilityResultKind::PcPathFamilyV2,
            command_kind: validated.command_kind(),
            query: validated.query().clone(),
            pc_probability_v2: None,
            pc_failed_queue_v2: None,
            pc_score_summary_v2: None,
            pc_score_portfolio_v2: None,
            pc_tiling_family_v1: None,
            pc_minimum_cover_v2: None,
            pc_path_family_v2: Some(report),
            pc_save_groups_v2: None,
            pc_best_save_v2: None,
            build_coverage_portfolio_v2: None,
            build_setup_v1: None,
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: resources.probability_complete(),
            },
            validation_count: 1,
        })
    }

    fn validate_pc_chance(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
        core_result: &clearra_core_executor::CoreExecutionResult,
    ) -> Result<Self, ProductCapabilityContractError> {
        let (expected_query, expected_origin) = validated
            .pc_chance_binding()
            .ok_or(ProductCapabilityContractError::ResponseChanceEvidenceMismatch)?;
        let evidence = response
            .pc_chance_execution_evidence()
            .ok_or(ProductCapabilityContractError::ResponseChanceEvidenceMissing)?;
        let report = evidence.report();
        if !evidence.matches_core_result(core_result)
            || report.contract_id() != ProductCapabilityResultKind::PcProbabilityV2.as_str()
            || report.query() != &expected_query
            || report.origin() != expected_origin
            || report.problem_preset().as_str() != validated.expected_problem_preset().as_str()
            || !report.completeness().complete()
        {
            return Err(ProductCapabilityContractError::ResponseChanceEvidenceMismatch);
        }

        let resources = response.resource_report();
        if !resources.solver_executed() {
            return Err(ProductCapabilityContractError::SolverNotExecuted);
        }
        if resources.execution_availability().state() != ExecutionAvailabilityState::Available {
            return Err(ProductCapabilityContractError::ExecutionUnavailable);
        }
        if resources.execution_availability().reason().is_some() {
            return Err(ProductCapabilityContractError::AvailabilityReasonPresent);
        }
        if resources.result_completeness() != ExecutionCompletenessState::Complete {
            return Err(ProductCapabilityContractError::ResultIncomplete);
        }
        if resources.truncated() {
            return Err(ProductCapabilityContractError::ResultTruncated);
        }
        if resources.truncation_reason().is_some() {
            return Err(ProductCapabilityContractError::TruncationReasonPresent);
        }
        if !resources.probability_complete() {
            return Err(ProductCapabilityContractError::ResourceProbabilityIncomplete);
        }

        Ok(Self {
            contract: ProductCapabilityContract::PcChance,
            result_kind: ProductCapabilityResultKind::PcProbabilityV2,
            command_kind: validated.command_kind(),
            query: validated.query().clone(),
            pc_probability_v2: Some(report.clone()),
            pc_failed_queue_v2: None,
            pc_score_summary_v2: None,
            pc_score_portfolio_v2: None,
            pc_tiling_family_v1: None,
            pc_minimum_cover_v2: None,
            pc_path_family_v2: None,
            pc_save_groups_v2: None,
            pc_best_save_v2: None,
            build_coverage_portfolio_v2: None,
            build_setup_v1: None,
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: true,
            },
            validation_count: 1,
        })
    }

    fn validate_pc_score(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
        core_result: &clearra_core_executor::CoreExecutionResult,
    ) -> Result<Self, ProductCapabilityContractError> {
        let (expected_query, expected_origin) = validated
            .pc_score_binding()
            .ok_or(ProductCapabilityContractError::ResponseScoreEvidenceMismatch)?;
        let evidence = response
            .pc_score_execution_evidence()
            .ok_or(ProductCapabilityContractError::ResponseScoreEvidenceMissing)?;
        let report = evidence.report();
        let result_kind = match validated.contract() {
            ProductCapabilityContract::PcScore => ProductCapabilityResultKind::PcScoreSummaryV2,
            ProductCapabilityContract::PcScoreFinder => {
                ProductCapabilityResultKind::PcFixedScoreWitnessV2
            }
            _ => return Err(ProductCapabilityContractError::ResponseScoreEvidenceMismatch),
        };
        let fixed_score_shape_matches = validated.contract()
            != ProductCapabilityContract::PcScoreFinder
            || (expected_origin.is_score_finder()
                && report.materialized_pattern_count() == 1
                && report.total_pattern_count() == 1
                && report
                    .pattern_optimal_count()
                    .checked_add(report.failed_pc_pattern_count())
                    == Some(1)
                && report
                    .pattern_winners()
                    .iter()
                    .all(|winner| winner.pattern_id() == 0));
        if !evidence.matches_core_result(core_result)
            || report.contract_id() != result_kind.as_str()
            || !expected_query.matches_snapshot(report.query())
            || report.origin() != expected_origin
            || report.problem_preset().as_str() != validated.expected_problem_preset().as_str()
            || report.accuracy_level() != "basic-approximation"
            || report.accuracy_reason()
                != "profile-specific basic score/attack tables with configurable spin detection"
            || report.profile_specific_exact()
            || !report.completeness().complete()
            || !fixed_score_shape_matches
        {
            return Err(ProductCapabilityContractError::ResponseScoreEvidenceMismatch);
        }

        let resources = response.resource_report();
        if !resources.solver_executed() {
            return Err(ProductCapabilityContractError::SolverNotExecuted);
        }
        if resources.execution_availability().state() != ExecutionAvailabilityState::Available {
            return Err(ProductCapabilityContractError::ExecutionUnavailable);
        }
        if resources.execution_availability().reason().is_some() {
            return Err(ProductCapabilityContractError::AvailabilityReasonPresent);
        }
        if resources.result_completeness() != ExecutionCompletenessState::Complete {
            return Err(ProductCapabilityContractError::ResultIncomplete);
        }
        if resources.truncated() {
            return Err(ProductCapabilityContractError::ResultTruncated);
        }
        if resources.truncation_reason().is_some() {
            return Err(ProductCapabilityContractError::TruncationReasonPresent);
        }
        if !resources.probability_complete() {
            return Err(ProductCapabilityContractError::ResourceProbabilityIncomplete);
        }

        Ok(Self {
            contract: validated.contract(),
            result_kind,
            command_kind: validated.command_kind(),
            query: validated.query().clone(),
            pc_probability_v2: None,
            pc_failed_queue_v2: None,
            pc_score_summary_v2: Some(report.clone()),
            pc_score_portfolio_v2: None,
            pc_tiling_family_v1: None,
            pc_minimum_cover_v2: None,
            pc_path_family_v2: None,
            pc_save_groups_v2: None,
            pc_best_save_v2: None,
            build_coverage_portfolio_v2: None,
            build_setup_v1: None,
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: true,
            },
            validation_count: 1,
        })
    }

    fn validate_pc_score_minimals(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
        core_result: &clearra_core_executor::CoreExecutionResult,
    ) -> Result<Self, ProductCapabilityContractError> {
        let (expected_query, expected_origin) = validated
            .pc_score_minimals_binding()
            .ok_or(ProductCapabilityContractError::ResponseScorePortfolioEvidenceMismatch)?;
        let evidence = response
            .pc_score_portfolio_execution_evidence()
            .ok_or(ProductCapabilityContractError::ResponseScorePortfolioEvidenceMissing)?;
        let report = evidence.report();
        let candidate_map_is_bound = report.eligible_candidates().iter().all(|candidate| {
            report
                .portfolio_alternatives()
                .public_candidate_id(candidate.portfolio_candidate_id())
                == Some(candidate.score_candidate_id())
        });
        if !evidence.matches_core_result(core_result)
            || report.contract_id() != ProductCapabilityResultKind::PcScorePortfolioV2.as_str()
            || !expected_query.matches_snapshot(report.query())
            || report.origin() != expected_origin
            || report.problem_preset().as_str() != validated.expected_problem_preset().as_str()
            || !report.completeness().complete()
            || !candidate_map_is_bound
        {
            return Err(ProductCapabilityContractError::ResponseScorePortfolioEvidenceMismatch);
        }

        let resources = response.resource_report();
        if !resources.solver_executed() {
            return Err(ProductCapabilityContractError::SolverNotExecuted);
        }
        if resources.execution_availability().state() != ExecutionAvailabilityState::Available {
            return Err(ProductCapabilityContractError::ExecutionUnavailable);
        }
        if resources.execution_availability().reason().is_some() {
            return Err(ProductCapabilityContractError::AvailabilityReasonPresent);
        }
        if resources.result_completeness() != ExecutionCompletenessState::Complete {
            return Err(ProductCapabilityContractError::ResultIncomplete);
        }
        if resources.truncated() {
            return Err(ProductCapabilityContractError::ResultTruncated);
        }
        if resources.truncation_reason().is_some() {
            return Err(ProductCapabilityContractError::TruncationReasonPresent);
        }
        if !resources.probability_complete() {
            return Err(ProductCapabilityContractError::ResourceProbabilityIncomplete);
        }

        Ok(Self {
            contract: ProductCapabilityContract::PcScoreMinimals,
            result_kind: ProductCapabilityResultKind::PcScorePortfolioV2,
            command_kind: validated.command_kind(),
            query: validated.query().clone(),
            pc_probability_v2: None,
            pc_failed_queue_v2: None,
            pc_score_summary_v2: None,
            pc_score_portfolio_v2: Some(Arc::clone(evidence.report_owner())),
            pc_tiling_family_v1: None,
            pc_minimum_cover_v2: None,
            pc_path_family_v2: None,
            pc_save_groups_v2: None,
            pc_best_save_v2: None,
            build_coverage_portfolio_v2: None,
            build_setup_v1: None,
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: true,
            },
            validation_count: 1,
        })
    }

    fn validate_pc_failed_queue(
        validated: ValidatedProductCapabilityContract,
        response: &AppResponse,
        core_result: &clearra_core_executor::CoreExecutionResult,
    ) -> Result<Self, ProductCapabilityContractError> {
        let (expected_query, expected_origin, expected_failed_pattern_limit) = validated
            .pc_failed_queue_binding()
            .ok_or(ProductCapabilityContractError::ResponseFailedQueueEvidenceMismatch)?;
        let evidence = response
            .pc_failed_queue_execution_evidence()
            .ok_or(ProductCapabilityContractError::ResponseFailedQueueEvidenceMissing)?;
        let report = evidence.report();
        if !evidence.matches_core_result(core_result)
            || report.contract_id() != ProductCapabilityResultKind::PcFailedQueueV2.as_str()
            || report.query() != &expected_query
            || report.origin() != expected_origin
            || report.failed_pattern_limit() != expected_failed_pattern_limit
            || report.problem_preset().as_str() != validated.expected_problem_preset().as_str()
            || report
                .success_pattern_count()
                .checked_add(report.failed_pattern_count())
                != Some(report.pattern_count())
            || report.examples().len()
                != expected_failed_pattern_limit.min(report.failed_pattern_count())
        {
            return Err(ProductCapabilityContractError::ResponseFailedQueueEvidenceMismatch);
        }

        let resources = response.resource_report();
        if !resources.solver_executed() {
            return Err(ProductCapabilityContractError::SolverNotExecuted);
        }
        if resources.execution_availability().state() != ExecutionAvailabilityState::Available {
            return Err(ProductCapabilityContractError::ExecutionUnavailable);
        }
        if resources.execution_availability().reason().is_some() {
            return Err(ProductCapabilityContractError::AvailabilityReasonPresent);
        }
        if resources.result_completeness() != ExecutionCompletenessState::Complete {
            return Err(ProductCapabilityContractError::ResultIncomplete);
        }
        if resources.truncated() {
            return Err(ProductCapabilityContractError::ResultTruncated);
        }
        if resources.truncation_reason().is_some() {
            return Err(ProductCapabilityContractError::TruncationReasonPresent);
        }
        if !resources.probability_complete() {
            return Err(ProductCapabilityContractError::ResourceProbabilityIncomplete);
        }

        Ok(Self {
            contract: ProductCapabilityContract::PcFailedQueue,
            result_kind: ProductCapabilityResultKind::PcFailedQueueV2,
            command_kind: validated.command_kind(),
            query: validated.query().clone(),
            pc_probability_v2: None,
            pc_failed_queue_v2: Some(report.clone()),
            pc_score_summary_v2: None,
            pc_score_portfolio_v2: None,
            pc_tiling_family_v1: None,
            pc_minimum_cover_v2: None,
            pc_path_family_v2: None,
            pc_save_groups_v2: None,
            pc_best_save_v2: None,
            build_coverage_portfolio_v2: None,
            build_setup_v1: None,
            resource_evidence: ProductCapabilityResourceEvidence {
                solver_executed: true,
                availability: ExecutionAvailabilityState::Available,
                completeness: ExecutionCompletenessState::Complete,
                truncated: false,
                probability_complete: true,
            },
            validation_count: 1,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn pc_save_run_metadata(
    origin: &str,
    problem_preset: &str,
    problem_id: &str,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    materialized_pattern_count: usize,
    pc_success_pattern_count: usize,
    pc_probability: &str,
    completeness: PcSaveCompletenessEvidence,
) -> PcSaveRunMetadataPayload {
    PcSaveRunMetadataPayload::new(
        origin,
        problem_preset,
        problem_id,
        piece_source_id.to_string(),
        pattern_universe_id.to_string(),
        pattern_weight_model_id.to_string(),
        materialized_pattern_count.to_string(),
        pc_success_pattern_count.to_string(),
        pc_probability,
        PcSaveCompletenessPayload::new(
            completeness.source_universe_complete(),
            completeness.fixed_bag_boundary_proven(),
            completeness.execution_batch_complete(),
            completeness.pattern_weights_complete(),
            completeness.count_complete(),
            completeness.probability_complete(),
            completeness.complete(),
        ),
    )
}

fn pc_save_group_payload(group: &PcSaveGroupV2) -> PcSaveGroupPayload {
    PcSaveGroupPayload::new(
        group.identity_contract(),
        pc_save_piece_multiset_payload(group.identity()),
        group.successful_pattern_count().to_string(),
        group.unconditional_probability().decimal(),
        group.conditional_probability_given_pc().decimal(),
        group.canonical_candidate_id().to_string(),
        group
            .witnesses()
            .iter()
            .map(pc_save_witness_payload)
            .collect(),
    )
}

fn pc_save_witness_payload(witness: &PcSaveWitness) -> PcSaveWitnessPayload {
    PcSaveWitnessPayload::new(
        witness.pattern_index().to_string(),
        witness.candidate_id().to_string(),
        witness.trace_identity(),
        witness.source_cursor().to_string(),
        witness
            .terminal_hold()
            .map(|piece| piece.as_ascii().to_string()),
        pc_save_piece_multiset_payload(witness.active_bag_remainder()),
    )
}

fn pc_save_piece_multiset_payload(multiset: &PcSavePieceMultiset) -> PcSavePieceMultisetPayload {
    PcSavePieceMultisetPayload::new(
        multiset.canonical_id(),
        multiset.count(PieceKind::T),
        multiset.count(PieceKind::I),
        multiset.count(PieceKind::O),
        multiset.count(PieceKind::J),
        multiset.count(PieceKind::L),
        multiset.count(PieceKind::S),
        multiset.count(PieceKind::Z),
        multiset.total_count(),
    )
}
