use clearra_core_domain::solution::{
    NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM, NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
};
use clearra_host_contract::{
    BuildV2PayloadKind, ProductResultPayloadContent, SolutionSetArtifactFormatPayload,
    SolutionSetArtifactPayload, HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES,
};
use clearra_output::artifact::{
    Ctk3SolutionSetEncoder, FumenSolutionSetEncoder, NeverCancelled, SolutionArtifactAnnotation,
    SolutionArtifactEncoder, SolutionArtifactEncodingError, SolutionArtifactEntry,
    SolutionSetArtifact,
};
use sha2::{Digest, Sha256};

use crate::{
    portfolio_alternative_store::{
        CoveragePortfolioAlternativeSet, CoveragePortfolioPageStore, PortfolioAlternativePage,
        ProductPageSourceOwner, PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT,
    },
    product_capability_contract::ProductCapabilityContract,
    product_capability_result::ProductCapabilityResultKind,
    AppResponse, AppStatus,
};

const NORMALIZED_TILING_SOURCE_CONTRACT: &str = "normalized-tiling-set";
const PORTFOLIO_MEMBER_KEY_ALGORITHM: &str = "portfolio-member-normalized-tiling-key.v1";
const PORTFOLIO_COLORED_FIELD_KEY_ALGORITHM: &str = "clearra-colored-field-key-v1";
const PORTFOLIO_PAGE_IDENTITY_ALGORITHM: &str = "portfolio-page-identity-sha256.v1";
const BUILD_FAMILY_KEY_SET_HASH_ALGORITHM: &str = "build-family-key-set-sha256.v1";

pub(crate) struct BoundSolutionSetArtifact {
    artifact: SolutionSetArtifact,
    source_result_kind: String,
    selection_kind: &'static str,
    selection_id: String,
    page_source_identity_sha256: Option<String>,
}

impl BoundSolutionSetArtifact {
    pub(super) fn into_artifact(self) -> SolutionSetArtifact {
        self.artifact
    }
}

pub(super) fn materialize_response(response: &AppResponse) -> Option<BoundSolutionSetArtifact> {
    if response.status() != AppStatus::Success
        || !response.resource_report().solver_executed()
        || response.resource_report().truncated()
        || response.resource_report().result_completeness().as_str() != "complete"
        || response
            .resource_report()
            .execution_availability()
            .state()
            .as_str()
            != "available"
        || response
            .resource_report()
            .execution_availability()
            .reason()
            .is_some()
    {
        return None;
    }

    if let Some(product) = response.product_capability_result() {
        match (product.contract(), product.result_kind()) {
            (
                ProductCapabilityContract::PcMinimals,
                ProductCapabilityResultKind::PcMinimumCoverV2,
            ) => {
                let report = product.pc_minimum_cover_v2()?;
                return materialize_portfolio_page(
                    report.portfolio_alternatives(),
                    report.portfolio_alternatives().canonical_page(),
                    product.result_kind().as_str(),
                );
            }
            (
                ProductCapabilityContract::PcScoreMinimals,
                ProductCapabilityResultKind::PcScorePortfolioV2,
            ) => {
                let report = product.pc_score_portfolio_v2()?;
                return materialize_portfolio_page(
                    report.portfolio_alternatives(),
                    report.portfolio_alternatives().canonical_page(),
                    product.result_kind().as_str(),
                );
            }
            (
                ProductCapabilityContract::PcTiling,
                ProductCapabilityResultKind::PcTilingFamilyV1,
            ) => {
                let family = product.pc_tiling_family_v1()?;
                let count = family.normalized_solution_count();
                if count == 0
                    || !family.completeness().family_complete()
                    || !family.completeness().initial_page_complete()
                    || family.completeness().incomplete_reason() != "none"
                {
                    return None;
                }
                let page_limit = family.initial_page_limit();
                if page_limit == 0 {
                    return None;
                }
                let mut entries = Vec::new();
                entries.try_reserve_exact(count).ok()?;
                while entries.len() < count {
                    let offset = entries.len();
                    let expected = (count - offset).min(page_limit);
                    let keys = family.page_keys(offset, expected).ok()?;
                    if keys.len() != expected {
                        return None;
                    }
                    for key in keys {
                        if entries.last().is_some_and(|entry: &SolutionArtifactEntry| {
                            entry.key() >= key.as_str()
                        }) {
                            return None;
                        }
                        entries.push(
                            SolutionArtifactEntry::try_new(key, SolutionArtifactAnnotation::new())
                                .ok()?,
                        );
                    }
                }
                let artifact = SolutionSetArtifact::try_new(
                    NORMALIZED_TILING_SOURCE_CONTRACT,
                    family.normalized_solution_key_algorithm(),
                    family.normalized_solution_set_hash_algorithm(),
                    family.normalized_solution_set_hash(),
                    count,
                    entries,
                )
                .ok()?;
                return Some(BoundSolutionSetArtifact {
                    artifact,
                    source_result_kind: product.result_kind().as_str().to_owned(),
                    selection_kind: "solution-family",
                    selection_id: family.normalized_solution_set_hash().to_owned(),
                    page_source_identity_sha256: None,
                });
            }
            (
                ProductCapabilityContract::BuildCover,
                ProductCapabilityResultKind::BuildCoveragePortfolioV2,
            ) => {
                let report = product.build_coverage_portfolio_v2()?;
                if !report.completeness().complete() {
                    return None;
                }
                let set = report.portfolio_alternative_owner()?;
                if !portfolio_page_matches_candidate_keys(
                    set,
                    set.canonical_page(),
                    report.canonical_candidate_keys(),
                ) {
                    return None;
                }
                return materialize_portfolio_page(
                    set,
                    set.canonical_page(),
                    product.result_kind().as_str(),
                );
            }
            (
                ProductCapabilityContract::BuildSetup,
                ProductCapabilityResultKind::BuildSetupFamilyV1,
            ) => {
                let report = product.build_setup_v1()?;
                if !report.completeness().replay_complete()
                    || report.candidates().len() != report.source_candidate_count()
                    || report.candidates().is_empty()
                {
                    return None;
                }
                let mut entries = Vec::new();
                entries.try_reserve_exact(report.candidates().len()).ok()?;
                for candidate in report.candidates() {
                    entries.push(
                        SolutionArtifactEntry::try_new(
                            candidate.candidate_key(),
                            SolutionArtifactAnnotation::new(),
                        )
                        .ok()?,
                    );
                }
                entries.sort_unstable_by(|left, right| left.key().cmp(right.key()));
                let set_hash = canonical_key_set_sha256(&entries);
                let artifact = SolutionSetArtifact::try_new(
                    report.contract_id(),
                    PORTFOLIO_COLORED_FIELD_KEY_ALGORITHM,
                    BUILD_FAMILY_KEY_SET_HASH_ALGORITHM,
                    &set_hash,
                    entries.len(),
                    entries,
                )
                .ok()?;
                return Some(BoundSolutionSetArtifact {
                    artifact,
                    source_result_kind: product.result_kind().as_str().to_owned(),
                    selection_kind: "solution-family",
                    selection_id: set_hash,
                    page_source_identity_sha256: None,
                });
            }
            _ => {}
        }
    }

    if let Some(public) = response.public_result_payload() {
        if let ProductResultPayloadContent::BuildV2(build) = public.content() {
            let complete = match build.kind() {
                BuildV2PayloadKind::Portfolio => build.completeness().portfolio_complete(),
                BuildV2PayloadKind::ScorePortfolio => {
                    build.completeness().score_portfolio_complete()
                }
                BuildV2PayloadKind::CandidateFamily | BuildV2PayloadKind::Probability => false,
            };
            if complete
                && build.page_source_available()
                && build.capability_id() == public.contract()
                && build.result_contract() == public.result_kind()
            {
                let ProductPageSourceOwner::CoveragePortfolio(set) =
                    response.public_page_source_owner()?
                else {
                    return None;
                };
                if build.page_source_identity_sha256() != Some(set.set_identity_sha256())
                    || !portfolio_page_matches_candidate_keys(
                        &set,
                        set.canonical_page(),
                        build.canonical_candidate_keys(),
                    )
                {
                    return None;
                }
                return materialize_portfolio_page(
                    &set,
                    set.canonical_page(),
                    public.result_kind(),
                );
            }
        }
    }

    let result = response.render_model()?.core_result()?;
    let availability = result.execution_report().solution_set_availability();
    let keys = result.normalized_solution_keys();
    if !availability.uses_explicit_contract()
        || !availability.contract_valid()
        || !availability.solution_count_calculated()
        || !availability.solution_set_materialized()
        || !availability.solution_keys_complete()
        || !availability.materialized_key_count_matches(keys.len())
        || keys.is_empty()
    {
        return None;
    }
    let fields = result.summary_fields();
    let count = unique_field(&fields, "normalized_unique_solution_count")?
        .parse::<usize>()
        .ok()?;
    if count != keys.len()
        || unique_field(&fields, "normalized_solution_key_algorithm")?
            != NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM
        || unique_field(&fields, "normalized_solution_set_hash_algorithm")?
            != NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM
    {
        return None;
    }
    let set_hash = unique_field(&fields, "normalized_solution_set_hash")?;
    if unique_field(&fields, "actual_normalized_solution_set_hash")? != set_hash {
        return None;
    }
    if unique_field(&fields, "count_complete").is_some_and(|value| value != "true") {
        return None;
    }
    let mut entries = Vec::new();
    entries.try_reserve_exact(keys.len()).ok()?;
    for key in keys {
        entries.push(SolutionArtifactEntry::try_new(key, SolutionArtifactAnnotation::new()).ok()?);
    }
    let artifact = SolutionSetArtifact::try_new(
        NORMALIZED_TILING_SOURCE_CONTRACT,
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
        NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
        set_hash,
        count,
        entries,
    )
    .ok()?;
    let source_result_kind = response
        .product_capability_result()
        .map(|product| product.result_kind().as_str())
        .or_else(|| response.result().map(|result| result.kind()))?
        .to_owned();
    Some(BoundSolutionSetArtifact {
        artifact,
        source_result_kind,
        selection_kind: "solution-family",
        selection_id: set_hash.to_owned(),
        page_source_identity_sha256: None,
    })
}

pub(crate) fn materialize_loaded_portfolio_page(
    store: &CoveragePortfolioPageStore,
    page_number: usize,
    source_result_kind: &str,
) -> Option<BoundSolutionSetArtifact> {
    let page = store.page(page_number)?;
    if source_result_kind.is_empty()
        || source_result_kind.trim() != source_result_kind
        || source_result_kind.chars().any(char::is_control)
    {
        return None;
    }
    materialize_portfolio_page(store.source(), page, source_result_kind)
}

fn materialize_portfolio_page(
    set: &CoveragePortfolioAlternativeSet,
    page: &PortfolioAlternativePage,
    source_result_kind: &str,
) -> Option<BoundSolutionSetArtifact> {
    if page.contract_id() != PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT
        || page.set_identity_sha256() != set.set_identity_sha256()
        || page.candidate_map_sha256() != set.candidate_map_sha256()
        || page.optimal_cardinality() == 0
        || page.portfolio().candidate_ids().len() != page.optimal_cardinality()
    {
        return None;
    }
    let mut previous_id = 0_u64;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(page.portfolio().candidate_ids().len())
        .ok()?;
    for candidate_id in page.portfolio().candidate_ids() {
        let index = candidate_id
            .checked_sub(1)
            .and_then(|value| usize::try_from(value).ok())?;
        let candidate = set.candidates().get(index)?;
        if candidate.candidate_id() != *candidate_id || *candidate_id <= previous_id {
            return None;
        }
        previous_id = *candidate_id;
        entries.push(
            SolutionArtifactEntry::try_new(
                candidate.normalized_key(),
                SolutionArtifactAnnotation::new(),
            )
            .ok()?,
        );
    }
    let identity = portfolio_page_identity_sha256(set, page);
    let key_algorithm = portfolio_member_key_algorithm(&entries)?;
    let artifact = SolutionSetArtifact::try_new(
        PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT,
        key_algorithm,
        PORTFOLIO_PAGE_IDENTITY_ALGORITHM,
        &identity,
        entries.len(),
        entries,
    )
    .ok()?;
    Some(BoundSolutionSetArtifact {
        artifact,
        source_result_kind: source_result_kind.to_owned(),
        selection_kind: "portfolio-alternative",
        selection_id: page.alternative_index_decimal().to_owned(),
        page_source_identity_sha256: Some(set.set_identity_sha256().to_owned()),
    })
}

fn portfolio_page_matches_candidate_keys(
    set: &CoveragePortfolioAlternativeSet,
    page: &PortfolioAlternativePage,
    expected_keys: &[String],
) -> bool {
    if page.portfolio().candidate_ids().len() != expected_keys.len() {
        return false;
    }
    page.portfolio()
        .candidate_ids()
        .iter()
        .zip(expected_keys)
        .all(|(candidate_id, expected)| {
            candidate_id
                .checked_sub(1)
                .and_then(|value| usize::try_from(value).ok())
                .and_then(|index| set.candidates().get(index))
                .is_some_and(|candidate| {
                    candidate.candidate_id() == *candidate_id
                        && candidate.normalized_key() == expected
                })
        })
}

fn portfolio_member_key_algorithm(entries: &[SolutionArtifactEntry]) -> Option<&'static str> {
    if entries.iter().all(|entry| entry.key().starts_with("ctk1|")) {
        Some(PORTFOLIO_MEMBER_KEY_ALGORITHM)
    } else if entries.iter().all(|entry| entry.key().starts_with("cfk1|")) {
        Some(PORTFOLIO_COLORED_FIELD_KEY_ALGORITHM)
    } else {
        None
    }
}

pub(crate) fn encode_bound_payload(
    source: BoundSolutionSetArtifact,
    maximum_bytes: u64,
) -> Option<SolutionSetArtifactPayload> {
    if maximum_bytes == 0 || maximum_bytes > HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES {
        return None;
    }
    let ctk3 = encode_format(&source.artifact, "ctk3", maximum_bytes)?;
    let fumen = encode_format(&source.artifact, "fumen", maximum_bytes)?;
    if !ctk3.available() && !fumen.available() {
        return None;
    }
    SolutionSetArtifactPayload::try_new(
        source.source_result_kind,
        source.artifact.source_solution_set_contract(),
        source.selection_kind,
        source.selection_id,
        source.page_source_identity_sha256,
        source.artifact.normalized_key_algorithm(),
        source.artifact.normalized_set_hash_algorithm(),
        source.artifact.normalized_set_hash(),
        u64::try_from(source.artifact.solution_count()).ok()?,
        vec![ctk3, fumen],
    )
    .ok()
}

fn encode_format(
    artifact: &SolutionSetArtifact,
    format: &'static str,
    maximum_bytes: u64,
) -> Option<SolutionSetArtifactFormatPayload> {
    let encoded = match format {
        "ctk3" => Ctk3SolutionSetEncoder.encode_checked(artifact, maximum_bytes, &NeverCancelled),
        "fumen" => FumenSolutionSetEncoder.encode_checked(artifact, maximum_bytes, &NeverCancelled),
        _ => return None,
    };
    let encoded = match encoded {
        Ok(encoded) => encoded,
        Err(error) => {
            return SolutionSetArtifactFormatPayload::try_unavailable(
                format,
                format_unavailable_reason(error),
            )
            .ok()
        }
    };
    let sha256 = sha256_hex(encoded.bytes());
    let bytes = encoded.into_bytes();
    let document = String::from_utf8(bytes).ok()?;
    let (media_type, filename) = if format == "ctk3" {
        ("application/vnd.clearra.ctk3", "clearra-solutions.ctk3")
    } else {
        ("text/plain;charset=utf-8", "clearra-solutions.fumen")
    };
    SolutionSetArtifactFormatPayload::try_available(
        format,
        media_type,
        filename,
        u64::try_from(document.len()).ok()?,
        sha256,
        u64::try_from(artifact.solution_count()).ok()?,
        document,
    )
    .ok()
}

fn format_unavailable_reason(error: SolutionArtifactEncodingError) -> &'static str {
    match error {
        SolutionArtifactEncodingError::EmptyDocument => "empty-solution-set",
        SolutionArtifactEncodingError::InvalidDocumentSolutionKey => "unsupported-solution-key",
        SolutionArtifactEncodingError::Ctk3PageLimitExceeded
        | SolutionArtifactEncodingError::FumenPageLimitExceeded => "page-limit-exceeded",
        SolutionArtifactEncodingError::CapacityExceeded => "transport-byte-limit-exceeded",
        _ => "encoding-failed",
    }
}

fn portfolio_page_identity_sha256(
    set: &CoveragePortfolioAlternativeSet,
    page: &PortfolioAlternativePage,
) -> String {
    let mut hasher = Sha256::new();
    hash_component(
        &mut hasher,
        b"contract",
        PORTFOLIO_PAGE_IDENTITY_ALGORITHM.as_bytes(),
    );
    hash_component(&mut hasher, b"set", set.set_identity_sha256().as_bytes());
    hash_component(
        &mut hasher,
        b"candidate-map",
        set.candidate_map_sha256().as_bytes(),
    );
    hash_component(
        &mut hasher,
        b"alternative-index",
        page.alternative_index_decimal().as_bytes(),
    );
    for candidate_id in page.portfolio().candidate_ids() {
        hash_component(
            &mut hasher,
            b"candidate-id",
            candidate_id.to_string().as_bytes(),
        );
    }
    hex_digest(hasher.finalize())
}

fn canonical_key_set_sha256(entries: &[SolutionArtifactEntry]) -> String {
    let mut hasher = Sha256::new();
    hash_component(
        &mut hasher,
        b"contract",
        BUILD_FAMILY_KEY_SET_HASH_ALGORITHM.as_bytes(),
    );
    for entry in entries {
        hash_component(&mut hasher, b"key", entry.key().as_bytes());
    }
    hex_digest(hasher.finalize())
}

fn hash_component(hasher: &mut Sha256, label: &[u8], value: &[u8]) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label);
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes))
}

fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    let digest = digest.as_ref();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use core::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

fn unique_field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let mut values = fields
        .iter()
        .filter_map(|(key, value)| (key == name).then_some(value.as_str()));
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::solution::{
        NormalizedTilingSolutionKey, NormalizedTilingSolutionSetHasher,
    };
    use clearra_output::{decode_ctk3_exact, Ctk3Color};

    use super::*;

    fn bound() -> BoundSolutionSetArtifact {
        let key = NormalizedTilingSolutionKey::parse_canonical(
            "ctk1|initial=0000000000000300|placements=I:000000000000000f",
        )
        .unwrap();
        let mut hasher = NormalizedTilingSolutionSetHasher::default();
        hasher.update_canonical_key(&key);
        let artifact = SolutionSetArtifact::try_new(
            NORMALIZED_TILING_SOURCE_CONTRACT,
            NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
            NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
            hasher.finish(),
            1,
            vec![
                SolutionArtifactEntry::try_new(key.as_str(), SolutionArtifactAnnotation::new())
                    .unwrap(),
            ],
        )
        .unwrap();
        BoundSolutionSetArtifact {
            artifact,
            source_result_kind: "pc-tiling-family.v1".to_owned(),
            selection_kind: "solution-family",
            selection_id: "cts1:test".to_owned(),
            page_source_identity_sha256: None,
        }
    }

    #[test]
    fn bounded_payload_uses_actual_native_ctk3_and_fumen_documents() {
        let payload = encode_bound_payload(bound(), HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES).unwrap();
        let ctk3 = payload
            .formats()
            .iter()
            .find(|format| format.format() == "ctk3")
            .unwrap();
        let decoded = decode_ctk3_exact(ctk3.document().unwrap()).unwrap();
        assert_eq!(decoded.pages.len(), 1);
        assert_eq!(
            decoded.pages[0].cells[0],
            Ctk3Color::Piece(clearra_output::Ctk3Piece::I)
        );

        let fumen = payload
            .formats()
            .iter()
            .find(|format| format.format() == "fumen")
            .unwrap();
        let decoded =
            clearra_fumen::SourceFumenDiagramSet::decode(fumen.document().unwrap()).unwrap();
        assert_eq!(decoded.page_count(), 1);
    }

    #[test]
    fn bound_too_small_omits_the_sidecar_instead_of_returning_partial_bytes() {
        assert!(encode_bound_payload(bound(), 1).is_none());
    }
}
