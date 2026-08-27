//! Fieldwise projection of validated normal ranked families to closed Host
//! payloads. This module never re-ranks candidates and never creates portfolio
//! alternative metadata.

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_host_contract::{
    ProductResultPayload, ProductResultPayloadContent, RankedFamilyPayloadError,
    SetupRankedCandidatePayload, SetupRankedFamilyPayload, SpinStructureCandidatePayload,
    SpinStructureFamilyPayload,
};

use crate::{
    setup_ranked_family_result::SetupRankedFamilySnapshot,
    setup_ranking_contract::SetupRankingKind,
    spin_structure_search_result::SpinStructureSearchResult,
};

pub(crate) fn project_setup_ranked_family(
    snapshot: &SetupRankedFamilySnapshot,
) -> Result<ProductResultPayload, RankedFamilyPayloadError> {
    let ordering = match snapshot.kind() {
        SetupRankingKind::Joint => "joint-probability-descending",
        SetupRankingKind::Build => "build-probability-descending",
        SetupRankingKind::ConditionalPc => "conditional-pc-probability-descending",
    };
    let candidates = snapshot
        .candidate_identities()
        .iter()
        .map(|candidate| {
            SetupRankedCandidatePayload::try_new(
                candidate.candidate_id(),
                candidate.condition_id(),
                candidate.setup_id(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let identities = snapshot.identities();
    let content = SetupRankedFamilyPayload::try_new(
        snapshot.result_schema(),
        identities.query_sha256(),
        identities.rule_profile(),
        identities.supply_sha256(),
        identities.universe_sha256(),
        identities.product_build(),
        ordering,
        snapshot.resolved_length_preference().keyword(),
        snapshot.candidate_count().to_string(),
        candidates,
    )?;
    Ok(ProductResultPayload::new(
        snapshot.capability_id(),
        snapshot.result_schema(),
        ProductResultPayloadContent::SetupRankedFamily(content),
    ))
}

pub(crate) fn project_spin_structure_family(
    result: &SpinStructureSearchResult,
) -> Result<ProductResultPayload, RankedFamilyPayloadError> {
    project_spin_structure_family_as(
        result,
        "spin-structure.search",
        "spin-structure-family.v2",
        None,
    )
}

pub(crate) fn project_spin_structure_guaranteed_family(
    result: &SpinStructureSearchResult,
    final_piece: PieceKind,
    dependency_report: bool,
) -> Result<ProductResultPayload, RankedFamilyPayloadError> {
    project_spin_structure_family_as(
        result,
        "spin-structure.guaranteed",
        "spin-structure-guaranteed.v1",
        Some((final_piece, dependency_report)),
    )
}

fn project_spin_structure_family_as(
    result: &SpinStructureSearchResult,
    capability_id: &'static str,
    result_schema: &'static str,
    guarantee: Option<(PieceKind, bool)>,
) -> Result<ProductResultPayload, RankedFamilyPayloadError> {
    let candidates = result
        .candidate_identities()
        .iter()
        .map(|candidate| {
            SpinStructureCandidatePayload::try_new(
                candidate.candidate_id(),
                if candidate.mini() { "mini" } else { "regular" },
                candidate.placement_count().to_string(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let identities = result.identities();
    let report = result.report();
    let guaranteed_final_piece = guarantee.map(|(piece, _)| piece.as_ascii().to_string());
    let guarantee_basis = guarantee
        .map(|_| "every-unique-non-target-piece-order-exact-replay-final-piece-last".to_owned());
    let dependency_report_included = guarantee.map(|(_, included)| included);
    let dependency_relation = guarantee
        .filter(|(_, included)| *included)
        .map(|_| "non-target-universal-precedence".to_owned());
    // A guaranteed family accepts every non-target permutation by construction,
    // so its universal non-target precedence relation is exactly empty.
    let dependency_edge_count = guarantee
        .filter(|(_, included)| *included)
        .map(|_| "0".to_owned());
    let content = SpinStructureFamilyPayload::try_new(
        result_schema,
        identities.query_sha256(),
        identities.rule_profile(),
        identities.spin_profile(),
        identities.supply_sha256(),
        identities.universe_sha256(),
        identities.product_build(),
        "regular-then-mini-canonical-operation-key",
        report.minimum_placements.map(|value| value.to_string()),
        guaranteed_final_piece,
        guarantee_basis,
        dependency_report_included,
        dependency_relation,
        dependency_edge_count,
        report.regular.len().to_string(),
        report.mini.len().to_string(),
        result.candidate_count().to_string(),
        report.complete,
        candidates,
    )?;
    Ok(ProductResultPayload::new(
        capability_id,
        result_schema,
        ProductResultPayloadContent::SpinStructureFamily(content),
    ))
}
