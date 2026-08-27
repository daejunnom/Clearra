use std::sync::Arc;

use clearra_coverage::{cover::exact_minimum_cover, pattern::pattern_bitset::PatternBitSet};
use clearra_host_contract::{
    CoveragePortfolioPagePayload, ProductCandidateMemberPayload, ProductResultPayload,
    ProductResultPayloadContent,
};
use clearra_spin_structure_search::SpinStructureCoverageAnalysis;

use crate::{
    portfolio_alternative_store::{
        CoveragePortfolioAlternativeSet, PortfolioAlternativeSetIdentity, ProductPageSourceOwner,
        PORTFOLIO_MEMBER_PAGE_CONTRACT, PORTFOLIO_MEMBER_PAGE_SIZE,
    },
    spin_structure_search_result::SpinStructureSearchResult,
};

pub(crate) struct ProjectedSpinStructureCoverage {
    pub(crate) payload: ProductResultPayload,
    pub(crate) owner: Arc<CoveragePortfolioAlternativeSet>,
}

pub(crate) fn project_spin_structure_coverage(
    result: &SpinStructureSearchResult,
    coverage: &SpinStructureCoverageAnalysis,
) -> Result<ProjectedSpinStructureCoverage, &'static str> {
    if coverage.rows().len() != result.candidate_identities().len() {
        return Err("spin-structure coverage row count mismatch");
    }
    let pattern_count = coverage.pattern_count();
    let mut candidates = result
        .candidate_identities()
        .iter()
        .zip(coverage.rows())
        .map(|(candidate, row)| {
            PatternBitSet::from_pattern_indices(
                pattern_count,
                row.covered_pattern_indices().to_vec(),
            )
            .map(|row| (candidate.candidate_id().to_owned(), row))
            .map_err(|_| "spin-structure coverage row is invalid")
        })
        .collect::<Result<Vec<_>, _>>()?;
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    if candidates
        .windows(2)
        .any(|pair| pair[0].0.as_str() == pair[1].0.as_str())
    {
        return Err("spin-structure coverage candidate identity duplicated");
    }
    let mut required = PatternBitSet::new(pattern_count);
    for (_, row) in &candidates {
        required
            .union_with(row)
            .map_err(|_| "spin-structure coverage universe mismatch")?;
    }
    if required.count_ones() as usize != coverage.covered_pattern_count() {
        return Err("spin-structure covered-pattern count mismatch");
    }
    let rows = candidates
        .iter()
        .map(|(_, row)| row.clone())
        .collect::<Vec<_>>();
    let selection = exact_minimum_cover(&required, &rows)
        .map_err(|_| "spin-structure exact minimum-cover failed")?;
    if !selection.complete() || selection.covered_patterns() != &required {
        return Err("spin-structure exact minimum-cover is incomplete");
    }
    let canonical_keys = selection
        .row_indices()
        .iter()
        .map(|index| candidates[*index].0.clone())
        .collect::<Vec<_>>();
    let identities = result.identities();
    let identity = PortfolioAlternativeSetIdentity::new(
        format!(
            "spin-structure-cover-query.v1:{}",
            identities.query_sha256()
        ),
        format!(
            "spin-structure-cover-source.v1:{}",
            identities.supply_sha256()
        ),
        format!(
            "spin-structure-cover-profile.v1:{}:{}",
            identities.rule_profile(),
            identities.spin_profile(),
        ),
        format!(
            "spin-structure-cover-universe.v1:{}:{}:{}",
            identities.universe_sha256(),
            coverage.pattern_count(),
            coverage.covered_pattern_count(),
        ),
        identities.product_build(),
    )
    .map_err(|_| "spin-structure portfolio identity is invalid")?;
    let candidate_keys = candidates
        .into_iter()
        .map(|(candidate, _)| candidate)
        .collect::<Vec<_>>();
    let owner = Arc::new(
        CoveragePortfolioAlternativeSet::new(
            identity,
            candidate_keys,
            required,
            rows,
            &canonical_keys,
        )
        .map_err(|_| "spin-structure portfolio alternative set is invalid")?,
    );
    let page = owner.canonical_page();
    let member_count = page.portfolio().candidate_ids().len();
    let end = member_count.min(PORTFOLIO_MEMBER_PAGE_SIZE);
    let mut members = Vec::with_capacity(end);
    for candidate_id in &page.portfolio().candidate_ids()[..end] {
        let index = candidate_id
            .checked_sub(1)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("spin-structure portfolio candidate ID is invalid")?;
        let candidate = owner
            .candidates()
            .get(index)
            .ok_or("spin-structure portfolio candidate is missing")?;
        if candidate.candidate_id() != *candidate_id {
            return Err("spin-structure portfolio candidate map mismatch");
        }
        members.push(ProductCandidateMemberPayload::new(
            candidate_id.to_string(),
            candidate.normalized_key(),
        ));
    }
    let payload = ProductResultPayload::new(
        "spin-structure.cover",
        "spin-structure-coverage.v1",
        ProductResultPayloadContent::CoveragePortfolio(CoveragePortfolioPagePayload::new(
            owner.contract_id(),
            page.contract_id(),
            PORTFOLIO_MEMBER_PAGE_CONTRACT,
            owner.set_identity_sha256(),
            owner.candidate_map_sha256(),
            page.alternative_index_decimal(),
            page.optimal_cardinality().to_string(),
            page.known_alternative_count_decimal(),
            page.total_alternative_count_decimal()
                .map(ToOwned::to_owned),
            page.enumeration_complete(),
            "1",
            member_count
                .div_ceil(PORTFOLIO_MEMBER_PAGE_SIZE)
                .max(1)
                .to_string(),
            members,
            true,
        )),
    );
    Ok(ProjectedSpinStructureCoverage { payload, owner })
}

pub(crate) fn page_source(owner: Arc<CoveragePortfolioAlternativeSet>) -> ProductPageSourceOwner {
    ProductPageSourceOwner::CoveragePortfolio(owner)
}
