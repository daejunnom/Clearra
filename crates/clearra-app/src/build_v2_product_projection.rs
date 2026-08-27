//! Fieldwise projection from validated Build v2 reports to the closed Host DTO.
//!
//! No source document or replay table is reinterpreted here. Every value comes
//! from the already validated facade report, and portfolio paging transfers the
//! same immutable owner that proved the canonical first result.

use clearra_host_contract::{
    BuildV2CandidateCoveragePayload, BuildV2CompletenessPayload, BuildV2ProductPayload,
    BuildV2ProductPayloadError, BuildV2ScoreWinnerPayload, ProductResultPayload,
    ProductResultPayloadContent,
};

use crate::{
    build_solution_probability_result::build_v2_facade::{
        BuildColoredTargetCompleteness, BuildCongruentCoverV1, BuildCongruentV1,
        BuildSetupCoverPercentV1, BuildSetupCoverScoreV1, BuildSetupCoverV1,
        BuildSuppliedCoverPercentV1, BuildSuppliedCoverageV1, BuildSuppliedMinimumCoverV1,
        BuildSuppliedProbabilityCompleteness, BuildSuppliedReplayCompleteness,
        BuildSuppliedScoreV1,
    },
    portfolio_alternative_store::ProductPageSourceOwner,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildV2ProductProjectionError {
    HostPayload(BuildV2ProductPayloadError),
    PageSourceMissing,
}

impl From<BuildV2ProductPayloadError> for BuildV2ProductProjectionError {
    fn from(value: BuildV2ProductPayloadError) -> Self {
        Self::HostPayload(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedBuildV2Product {
    payload: ProductResultPayload,
    page_source_owner: Option<ProductPageSourceOwner>,
}

impl ProjectedBuildV2Product {
    fn new(
        capability_id: &'static str,
        result_contract: &'static str,
        content: BuildV2ProductPayload,
        page_source_owner: Option<ProductPageSourceOwner>,
    ) -> Self {
        Self {
            payload: ProductResultPayload::new(
                capability_id,
                result_contract,
                ProductResultPayloadContent::BuildV2(content),
            ),
            page_source_owner,
        }
    }

    pub const fn payload(&self) -> &ProductResultPayload {
        &self.payload
    }

    pub const fn page_source_owner(&self) -> Option<&ProductPageSourceOwner> {
        self.page_source_owner.as_ref()
    }

    pub fn into_parts(self) -> (ProductResultPayload, Option<ProductPageSourceOwner>) {
        (self.payload, self.page_source_owner)
    }
}

pub fn project_build_congruent_v1(
    report: &BuildCongruentV1,
) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError> {
    let candidates = colored_candidate_rows(report.candidates())?;
    let completeness = colored_completeness(report.completeness());
    let content = BuildV2ProductPayload::try_candidate_family(
        "build.congruent",
        report.contract_id(),
        report.input_identity_sha256(),
        report.evaluation_identity_sha256(),
        report.objective().as_str(),
        report.source_candidate_count().to_string(),
        report.reachable_candidate_count().to_string(),
        report.pattern_count().to_string(),
        report.covered_pattern_count().to_string(),
        report.union_probability(),
        None,
        candidates,
        completeness,
    )?;
    Ok(ProjectedBuildV2Product::new(
        "build.congruent",
        report.contract_id(),
        content,
        None,
    ))
}

pub fn project_build_congruent_cover_v1(
    report: &BuildCongruentCoverV1,
) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError> {
    project_colored_portfolio(
        "build.congruent-cover",
        report.contract_id(),
        report.input_identity_sha256(),
        report.objective().as_str(),
        report.source_candidate_count(),
        report.reachable_candidate_count(),
        report.selected_candidate_count(),
        report.pattern_count(),
        report.required_pattern_count(),
        report.union_probability(),
        report.canonical_candidate_keys(),
        report.completeness(),
        report.portfolio_alternative_owner(),
    )
}

pub fn project_build_setup_cover_v1(
    report: &BuildSetupCoverV1,
) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError> {
    project_colored_portfolio(
        "build.setup-cover",
        report.contract_id(),
        report.input_identity_sha256(),
        report.objective().as_str(),
        report.source_candidate_count(),
        report.reachable_candidate_count(),
        report.selected_candidate_count(),
        report.pattern_count(),
        report.required_pattern_count(),
        report.union_probability(),
        report.canonical_candidate_keys(),
        report.completeness(),
        report.portfolio_alternative_owner(),
    )
}

pub fn project_build_setup_cover_percent_v1(
    report: &BuildSetupCoverPercentV1,
) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError> {
    let content = BuildV2ProductPayload::try_probability(
        "build.setup-cover-percent",
        report.contract_id(),
        report.input_identity_sha256(),
        report.evaluation_identity_sha256(),
        None,
        report.objective().as_str(),
        report.source_candidate_count().to_string(),
        report.reachable_candidate_count().to_string(),
        report.pattern_count().to_string(),
        report.covered_pattern_count().to_string(),
        report.union_probability(),
        colored_completeness(report.completeness()),
    )?;
    Ok(ProjectedBuildV2Product::new(
        "build.setup-cover-percent",
        report.contract_id(),
        content,
        None,
    ))
}

pub fn project_build_setup_cover_score_v1(
    report: &BuildSetupCoverScoreV1,
) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError> {
    let owner = report
        .portfolio_alternative_owner()
        .ok_or(BuildV2ProductProjectionError::PageSourceMissing)?;
    let winners = report
        .winners()
        .iter()
        .map(|winner| {
            BuildV2ScoreWinnerPayload::try_new(
                winner.pattern_id().to_string(),
                winner.candidate_key(),
                winner.score().to_string(),
                winner.informational_attack().to_string(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let content = BuildV2ProductPayload::try_score_portfolio(
        "build.setup-cover-score",
        report.contract_id(),
        report.input_identity_sha256(),
        report.score_profile(),
        report.initial_b2b().to_string(),
        report.score_accuracy(),
        report.profile_specific_exact(),
        report.score_equality_basis(),
        report.informational_attack_basis(),
        report.source_candidate_count().to_string(),
        report.reachable_candidate_count().to_string(),
        report.selected_candidate_count().to_string(),
        report.pattern_count().to_string(),
        report.required_pattern_count().to_string(),
        report.canonical_candidate_keys().to_vec(),
        winners,
        colored_completeness(report.completeness()),
        owner.set_identity_sha256(),
    )?;
    Ok(ProjectedBuildV2Product::new(
        "build.setup-cover-score",
        report.contract_id(),
        content,
        Some(ProductPageSourceOwner::CoveragePortfolio(owner.clone())),
    ))
}

pub fn project_build_supplied_cover_percent_v1(
    report: &BuildSuppliedCoverPercentV1,
) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError> {
    let content = BuildV2ProductPayload::try_probability(
        "build.evaluate.cover-percent",
        report.contract_id(),
        report.input_identity_sha256(),
        report.evaluation_identity_sha256(),
        Some(report.replay_basis().to_owned()),
        "unique",
        report.source_candidate_count().to_string(),
        report.reachable_candidate_count().to_string(),
        report.pattern_count().to_string(),
        report.covered_pattern_count().to_string(),
        report.union_probability(),
        supplied_probability_completeness(report.completeness()),
    )?;
    Ok(ProjectedBuildV2Product::new(
        "build.evaluate.cover-percent",
        report.contract_id(),
        content,
        None,
    ))
}

pub fn project_build_supplied_coverage_v1(
    report: &BuildSuppliedCoverageV1,
) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError> {
    let capability_id = if report.b2b_preservation_required() {
        "build.evaluate.b2b-cover"
    } else {
        "build.evaluate.cover"
    };
    let mut candidates = report
        .candidates()
        .iter()
        .map(|candidate| {
            BuildV2CandidateCoveragePayload::try_new(
                candidate.candidate_key(),
                candidate.covered_pattern_count().to_string(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    candidates.sort_by(|left, right| left.candidate_key().cmp(right.candidate_key()));
    let content = BuildV2ProductPayload::try_candidate_family(
        capability_id,
        report.contract_id(),
        report.input_identity_sha256(),
        report.evaluation_identity_sha256(),
        "all",
        report.source_candidate_count().to_string(),
        report.reachable_candidate_count().to_string(),
        report.pattern_count().to_string(),
        report.covered_pattern_count().to_string(),
        report.union_probability(),
        Some(report.b2b_preservation_required()),
        candidates,
        supplied_probability_completeness(report.completeness()),
    )?;
    Ok(ProjectedBuildV2Product::new(
        capability_id,
        report.contract_id(),
        content,
        None,
    ))
}

pub fn project_build_supplied_minimum_cover_v1(
    report: &BuildSuppliedMinimumCoverV1,
) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError> {
    let owner = report
        .portfolio_alternative_owner()
        .ok_or(BuildV2ProductProjectionError::PageSourceMissing)?;
    let content = BuildV2ProductPayload::try_portfolio(
        "build.evaluate.minimals",
        report.contract_id(),
        report.input_identity_sha256(),
        Some(report.replay_basis().to_owned()),
        "min-cover",
        report.source_candidate_count().to_string(),
        report.reachable_candidate_count().to_string(),
        report.selected_candidate_count().to_string(),
        report.pattern_count().to_string(),
        report.required_pattern_count().to_string(),
        report.union_probability(),
        report.canonical_candidate_keys().to_vec(),
        supplied_replay_completeness(report.completeness(), false),
        owner.set_identity_sha256(),
    )?;
    Ok(ProjectedBuildV2Product::new(
        "build.evaluate.minimals",
        report.contract_id(),
        content,
        Some(ProductPageSourceOwner::CoveragePortfolio(owner.clone())),
    ))
}

pub fn project_build_supplied_score_v1(
    report: &BuildSuppliedScoreV1,
) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError> {
    let owner = report
        .portfolio_alternative_owner()
        .ok_or(BuildV2ProductProjectionError::PageSourceMissing)?;
    let winners = report
        .winners()
        .iter()
        .map(|winner| {
            BuildV2ScoreWinnerPayload::try_new(
                winner.pattern_id().to_string(),
                winner.candidate_key(),
                winner.score().to_string(),
                winner.informational_attack().to_string(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let content = BuildV2ProductPayload::try_score_portfolio(
        "build.evaluate.score",
        report.contract_id(),
        report.input_identity_sha256(),
        report.score_profile(),
        report.initial_b2b().to_string(),
        report.score_accuracy(),
        report.profile_specific_exact(),
        report.score_equality_basis(),
        report.informational_attack_basis(),
        report.source_candidate_count().to_string(),
        report.reachable_candidate_count().to_string(),
        report.selected_candidate_count().to_string(),
        report.pattern_count().to_string(),
        report.required_pattern_count().to_string(),
        report.canonical_candidate_keys().to_vec(),
        winners,
        supplied_replay_completeness(report.completeness(), true),
        owner.set_identity_sha256(),
    )?;
    Ok(ProjectedBuildV2Product::new(
        "build.evaluate.score",
        report.contract_id(),
        content,
        Some(ProductPageSourceOwner::CoveragePortfolio(owner.clone())),
    ))
}

#[allow(clippy::too_many_arguments)]
fn project_colored_portfolio(
    capability_id: &'static str,
    result_contract: &'static str,
    input_identity_sha256: &str,
    objective: &str,
    source_candidate_count: usize,
    reachable_candidate_count: usize,
    selected_candidate_count: usize,
    pattern_count: usize,
    required_pattern_count: usize,
    union_probability: &str,
    canonical_candidate_keys: &[String],
    completeness: BuildColoredTargetCompleteness,
    owner: Option<&std::sync::Arc<crate::CoveragePortfolioAlternativeSet>>,
) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError> {
    let owner = owner.ok_or(BuildV2ProductProjectionError::PageSourceMissing)?;
    let content = BuildV2ProductPayload::try_portfolio(
        capability_id,
        result_contract,
        input_identity_sha256,
        None,
        objective,
        source_candidate_count.to_string(),
        reachable_candidate_count.to_string(),
        selected_candidate_count.to_string(),
        pattern_count.to_string(),
        required_pattern_count.to_string(),
        union_probability,
        canonical_candidate_keys.to_vec(),
        colored_completeness(completeness),
        owner.set_identity_sha256(),
    )?;
    Ok(ProjectedBuildV2Product::new(
        capability_id,
        result_contract,
        content,
        Some(ProductPageSourceOwner::CoveragePortfolio(owner.clone())),
    ))
}

fn colored_candidate_rows(
    rows: &[crate::build_solution_probability_result::build_v2_facade::BuildColoredTargetCandidateCoverageV1],
) -> Result<Vec<BuildV2CandidateCoveragePayload>, BuildV2ProductPayloadError> {
    let mut candidates = rows
        .iter()
        .map(|candidate| {
            BuildV2CandidateCoveragePayload::try_new(
                candidate.candidate_key(),
                candidate.covered_pattern_count().to_string(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    candidates.sort_by(|left, right| left.candidate_key().cmp(right.candidate_key()));
    Ok(candidates)
}

fn colored_completeness(value: BuildColoredTargetCompleteness) -> BuildV2CompletenessPayload {
    BuildV2CompletenessPayload::new(
        value.input_identity_bound(),
        value.producer_filter_bound(),
        value.buildability_replay_complete(),
        value.coverage_rows_complete(),
        value.probability_weights_complete(),
        value.exact_minimum_proven(),
        value.score_evidence_complete(),
    )
}

fn supplied_probability_completeness(
    value: BuildSuppliedProbabilityCompleteness,
) -> BuildV2CompletenessPayload {
    BuildV2CompletenessPayload::new(
        value.input_identity_bound(),
        value.producer_filter_bound(),
        value.buildability_replay_complete(),
        value.coverage_rows_complete(),
        value.probability_weights_complete(),
        false,
        false,
    )
}

fn supplied_replay_completeness(
    value: BuildSuppliedReplayCompleteness,
    score_evidence_complete: bool,
) -> BuildV2CompletenessPayload {
    BuildV2CompletenessPayload::new(
        value.input_identity_bound(),
        value.producer_filter_bound(),
        value.buildability_replay_complete(),
        value.coverage_rows_complete(),
        value.probability_weights_complete(),
        value.exact_minimum_proven(),
        score_evidence_complete,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_validated_build_report_has_a_nominal_projection_function() {
        let _: fn(&BuildCongruentV1) -> Result<_, _> = project_build_congruent_v1;
        let _: fn(&BuildCongruentCoverV1) -> Result<_, _> = project_build_congruent_cover_v1;
        let _: fn(&BuildSetupCoverV1) -> Result<_, _> = project_build_setup_cover_v1;
        let _: fn(&BuildSetupCoverPercentV1) -> Result<_, _> = project_build_setup_cover_percent_v1;
        let _: fn(&BuildSetupCoverScoreV1) -> Result<_, _> = project_build_setup_cover_score_v1;
        let _: fn(&BuildSuppliedCoverPercentV1) -> Result<_, _> =
            project_build_supplied_cover_percent_v1;
        let _: fn(&BuildSuppliedCoverageV1) -> Result<_, _> = project_build_supplied_coverage_v1;
        let _: fn(&BuildSuppliedMinimumCoverV1) -> Result<_, _> =
            project_build_supplied_minimum_cover_v1;
        let _: fn(&BuildSuppliedScoreV1) -> Result<_, _> = project_build_supplied_score_v1;
    }
}
