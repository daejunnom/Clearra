//! Bind a local entry/first-exit candidate pack to its candidate catalog.
//!
//! This checks format, profile, rule, generation and stored-byte identities.
//! The structural check cannot rederive query-source hashes without the
//! source file. The separate source-bound cover check proves only its declared
//! occupancy subdomains, never a whole product profile. Neither grants
//! release authority.

use clearra_core_executor::{
    accelerator_profile_name, audited_local_relation_candidate_pack,
    built_in_local_relation_binding, load_local_relation_candidate_pack,
    LocalRelationCoverageDomain, LocalRelationCoverageResult,
};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::conditioned_local_relation_generation::{canonical_cover_identity, parse_cover_domains};

const MAX_CATALOG_BYTES: usize = 512 * 1024;
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_PACK_BYTES: usize = 16 * 1024 * 1024;
const CATALOG_FIELDS: usize = 18;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConditionedLocalCandidateSummary {
    pub generation_identity: [u8; 32],
    pub pack_bytes: usize,
    pub logical_resident_bytes: usize,
    pub record_count: usize,
}

/// Source-bound proof of only the declared occupancy subdomains. A complete
/// local cover is not a profile-wide qualification or release authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedBoundedLocalCover {
    pub candidate: ConditionedLocalCandidateSummary,
    pub covered_domains: usize,
    pub visited_proof_nodes: u64,
}

pub fn verify_conditioned_local_cover_source(
    profile: KickTableProfileId,
    pack_bytes: &[u8],
    catalog_bytes: &[u8],
    source_bytes: &[u8],
) -> Result<VerifiedBoundedLocalCover, String> {
    if source_bytes.is_empty()
        || source_bytes.len() > MAX_SOURCE_BYTES
        || pack_bytes.len() > MAX_PACK_BYTES
    {
        return Err("local relation cover input exceeds its bound".to_owned());
    }
    let candidate =
        validate_conditioned_local_candidate_catalog(profile, pack_bytes, catalog_bytes)
            .map_err(str::to_owned)?;
    let catalog: Value = serde_json::from_slice(catalog_bytes)
        .map_err(|_| "local relation cover catalog invalid".to_owned())?;
    if catalog["query_schema"] != "clearra.conditioned-local-relation.cover-set.v1"
        || catalog["source_file_identity"] != hex(Sha256::digest(source_bytes).into())
    {
        return Err("local relation cover source identity mismatch".to_owned());
    }
    let domains = parse_cover_domains(source_bytes, profile)?;
    let identity = canonical_cover_identity(&domains, profile)?;
    if catalog["query_set_identity"] != hex(identity)
        || catalog["query_count"].as_u64() != Some(domains.len() as u64)
    {
        return Err("local relation cover domain identity mismatch".to_owned());
    }
    let binding =
        built_in_local_relation_binding(profile).map_err(|error| error.code().to_owned())?;
    let pack = load_local_relation_candidate_pack(pack_bytes, binding, None)
        .map_err(|error| error.code().to_owned())?;
    let audited = audited_local_relation_candidate_pack(&pack)
        .map_err(|error| format!("local relation cover record audit failed: {error:?}"))?;
    let mut visited_proof_nodes = 0_u64;
    for (index, domain) in domains.iter().enumerate() {
        let query = &domain.query;
        let proof = audited.prove_context_coverage(
            LocalRelationCoverageDomain {
                width: query.width,
                height: query.height,
                frame: query.frame,
                piece: query.piece,
                profile,
                window: query.window,
                entries: &query.entries,
                fixed_mask: domain.fixed_mask,
                fixed_occupancy: domain.fixed_occupancy,
            },
            domain.max_nodes,
        );
        match proof {
            Ok(LocalRelationCoverageResult::Complete { visited_nodes, .. }) => {
                visited_proof_nodes += u64::from(visited_nodes);
            }
            Ok(LocalRelationCoverageResult::Uncovered { .. }) => {
                return Err(format!("local relation cover domain {index} is incomplete"));
            }
            Ok(LocalRelationCoverageResult::Inconclusive { .. }) => {
                return Err(format!(
                    "local relation cover domain {index} exceeded proof budget"
                ));
            }
            Err(error) => {
                return Err(format!(
                    "local relation cover domain {index} invalid: {error:?}"
                ));
            }
        }
    }
    Ok(VerifiedBoundedLocalCover {
        candidate,
        covered_domains: domains.len(),
        visited_proof_nodes,
    })
}

pub fn validate_conditioned_local_candidate_catalog(
    profile: KickTableProfileId,
    pack_bytes: &[u8],
    catalog_bytes: &[u8],
) -> Result<ConditionedLocalCandidateSummary, &'static str> {
    if catalog_bytes.len() > MAX_CATALOG_BYTES {
        return Err("local relation candidate catalog too large");
    }
    let catalog: Value = serde_json::from_slice(catalog_bytes)
        .map_err(|_| "local relation candidate catalog invalid")?;
    if !catalog
        .as_object()
        .is_some_and(|object| object.len() == CATALOG_FIELDS)
    {
        return Err("local relation candidate catalog fields invalid");
    }
    let binding = built_in_local_relation_binding(profile)
        .map_err(|_| "local relation candidate profile unsupported")?;
    let loaded = load_local_relation_candidate_pack(pack_bytes, binding, None)
        .map_err(|_| "local relation candidate pack invalid")?;
    let profile_name = accelerator_profile_name(profile)
        .map_err(|_| "local relation candidate profile unsupported")?;
    let record_count = loaded.record_count();
    let stored_sha256: [u8; 32] = Sha256::digest(pack_bytes).into();
    // A cover-set catalog reports its number of declared domains rather
    // than its expanded record count. This structural check never replays
    // the source request or upgrades the catalog to release authority.
    let source_count = catalog["query_count"].as_u64();
    let source_contract = match catalog["query_schema"].as_str() {
        Some("clearra.conditioned-local-relation.query-set.v1") => {
            source_count == Some(record_count as u64)
                && catalog["evidence_scope"] == "stored-record-and-collision-dependency-only"
        }
        Some("clearra.conditioned-local-relation.cover-set.v1") => {
            source_count.is_some_and(|count| count > 0 && count <= record_count as u64)
                && catalog["evidence_scope"] == "audited-record-and-declared-domain-coverage"
        }
        _ => false,
    };
    if catalog["schema"] != "clearra.conditioned-local-relation.candidate-catalog.v1"
        || catalog["status"] != "candidate_unqualified"
        || catalog["signed"] != false
        || catalog["release_authority"] != false
        || catalog["profile"] != profile_name
        || !source_contract
        || catalog["record_count"].as_u64() != Some(record_count as u64)
        || catalog["independent_checked_records"].as_u64() != Some(record_count as u64)
        || catalog["encoded_bytes"].as_u64() != Some(pack_bytes.len() as u64)
        || catalog["payload_identity"] != hex(stored_sha256)
        || catalog["generation_identity"] != hex(loaded.generation_identity())
        || catalog["rule_identity"] != hex(binding.rule_identity)
        || !valid_hex_digest(&catalog["query_set_identity"])
        || !valid_hex_digest(&catalog["source_file_identity"])
        || catalog["global_entry_reachability"] != "not_proven"
        || catalog["profile_completeness"] != "not_proven"
    {
        return Err("local relation candidate catalog binding mismatch");
    }
    Ok(ConditionedLocalCandidateSummary {
        generation_identity: loaded.generation_identity(),
        pack_bytes: pack_bytes.len(),
        logical_resident_bytes: loaded.logical_resident_bytes(),
        record_count,
    })
}

fn valid_hex_digest(value: &Value) -> bool {
    value.as_str().is_some_and(|digits| {
        digits.len() == 64
            && digits
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_core_executor::{
        derive_exact_conditioned_local_relation_with_frame, encode_local_relation_candidate_pack,
        ConditionedPoseWindow, ConditionedReachabilityEntryPose, LocalRelationRowFrame,
    };
    use serde_json::json;

    fn fixture() -> (Vec<u8>, Value) {
        let profile = KickTableProfileId::SrsPlus;
        let binding = built_in_local_relation_binding(profile).unwrap();
        let record = derive_exact_conditioned_local_relation_with_frame(
            10,
            4,
            0,
            LocalRelationRowFrame::new(4, 4).unwrap(),
            PieceKind::T,
            profile,
            ConditionedPoseWindow {
                min_x: 4,
                max_x: 4,
                min_y: 4,
                max_y: 4,
            },
            &[ConditionedReachabilityEntryPose {
                rotation: RotationState::Zero,
                x: 4,
                y: 4,
            }],
        )
        .unwrap();
        let bytes = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
        let loaded = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
        let catalog = json!({
            "schema": "clearra.conditioned-local-relation.candidate-catalog.v1",
            "status": "candidate_unqualified",
            "signed": false,
            "release_authority": false,
            "profile": "srs-plus",
            "query_schema": "clearra.conditioned-local-relation.query-set.v1",
            "query_count": 1,
            "query_set_identity": hex([1; 32]),
            "source_file_identity": hex([2; 32]),
            "record_count": 1,
            "independent_checked_records": 1,
            "encoded_bytes": bytes.len(),
            "payload_identity": hex(Sha256::digest(&bytes).into()),
            "generation_identity": hex(loaded.generation_identity()),
            "rule_identity": hex(binding.rule_identity),
            "evidence_scope": "stored-record-and-collision-dependency-only",
            "global_entry_reachability": "not_proven",
            "profile_completeness": "not_proven",
        });
        (bytes, catalog)
    }

    #[test]
    fn conditioned_local_pack_and_catalog_are_bound_but_unqualified() {
        let (bytes, catalog) = fixture();
        let encoded = serde_json::to_vec(&catalog).unwrap();
        let valid = validate_conditioned_local_candidate_catalog(
            KickTableProfileId::SrsPlus,
            &bytes,
            &encoded,
        )
        .unwrap();
        assert_eq!(valid.pack_bytes, bytes.len());
        assert!(valid.logical_resident_bytes >= core::mem::size_of_val(&valid));
        assert_eq!(valid.record_count, 1);
        assert!(validate_conditioned_local_candidate_catalog(
            KickTableProfileId::SrsX,
            &bytes,
            &encoded
        )
        .is_err());

        for pointer in [
            "/profile",
            "/payload_identity",
            "/generation_identity",
            "/rule_identity",
            "/query_count",
            "/independent_checked_records",
            "/query_set_identity",
            "/evidence_scope",
            "/profile_completeness",
        ] {
            let mut changed = catalog.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("incorrect");
            assert!(
                validate_conditioned_local_candidate_catalog(
                    KickTableProfileId::SrsPlus,
                    &bytes,
                    &serde_json::to_vec(&changed).unwrap(),
                )
                .is_err(),
                "{pointer}"
            );
        }
        let mut falsely_signed = catalog.clone();
        falsely_signed["signed"] = json!(true);
        assert!(validate_conditioned_local_candidate_catalog(
            KickTableProfileId::SrsPlus,
            &bytes,
            &serde_json::to_vec(&falsely_signed).unwrap(),
        )
        .is_err());
        let mut false_cover = catalog.clone();
        false_cover["query_schema"] = json!("clearra.conditioned-local-relation.cover-set.v1");
        assert!(validate_conditioned_local_candidate_catalog(
            KickTableProfileId::SrsPlus,
            &bytes,
            &serde_json::to_vec(&false_cover).unwrap(),
        )
        .is_err());
        let mut corrupted = bytes.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        assert!(validate_conditioned_local_candidate_catalog(
            KickTableProfileId::SrsPlus,
            &corrupted,
            &encoded,
        )
        .is_err());
    }

    #[test]
    fn structural_cover_claim_needs_the_original_source_and_symbolic_proof() {
        let profile = KickTableProfileId::NoKick;
        let binding = built_in_local_relation_binding(profile).unwrap();
        let frame = LocalRelationRowFrame::new(1, 0).unwrap();
        let window = ConditionedPoseWindow {
            min_x: 4,
            max_x: 4,
            min_y: 0,
            max_y: 1,
        };
        let entries = [ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 1,
        }];
        let record = derive_exact_conditioned_local_relation_with_frame(
            10,
            1,
            0,
            frame,
            PieceKind::T,
            profile,
            window,
            &entries,
        )
        .unwrap();
        let pack_bytes = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
        let pack = load_local_relation_candidate_pack(&pack_bytes, binding, None).unwrap();
        let source_bytes = serde_json::to_vec(&json!({
            "schema": "clearra.conditioned-local-relation.cover-set.v1",
            "profile": "no-kick",
            "domains": [{
                "query": {
                    "width": 10, "height": 1, "board": "0x0",
                    "deleted_original_rows": 0, "piece": "T",
                    "window": { "min_x": 4, "max_x": 4, "min_y": 0, "max_y": 1 },
                    "entries": [{ "rotation": 0, "x": 4, "y": 1 }]
                },
                "fixed_mask": "0x0", "fixed_occupancy": "0x0",
                "max_records": 1024, "max_nodes": 100000
            }]
        }))
        .unwrap();
        let domains = parse_cover_domains(&source_bytes, profile).unwrap();
        let catalog = json!({
            "schema": "clearra.conditioned-local-relation.candidate-catalog.v1",
            "status": "candidate_unqualified",
            "signed": false,
            "release_authority": false,
            "profile": "no-kick",
            "query_schema": "clearra.conditioned-local-relation.cover-set.v1",
            "query_count": 1,
            "query_set_identity": hex(canonical_cover_identity(&domains, profile).unwrap()),
            "source_file_identity": hex(Sha256::digest(&source_bytes).into()),
            "record_count": 1,
            "independent_checked_records": 1,
            "encoded_bytes": pack_bytes.len(),
            "payload_identity": hex(Sha256::digest(&pack_bytes).into()),
            "generation_identity": hex(pack.generation_identity()),
            "rule_identity": hex(binding.rule_identity),
            "evidence_scope": "audited-record-and-declared-domain-coverage",
            "global_entry_reachability": "not_proven",
            "profile_completeness": "not_proven",
        });
        let catalog_bytes = serde_json::to_vec(&catalog).unwrap();
        assert!(
            validate_conditioned_local_candidate_catalog(profile, &pack_bytes, &catalog_bytes,)
                .is_ok()
        );
        assert!(verify_conditioned_local_cover_source(
            profile,
            &pack_bytes,
            &catalog_bytes,
            &source_bytes,
        )
        .unwrap_err()
        .contains("incomplete"));
    }
}
