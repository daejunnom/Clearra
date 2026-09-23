//! Bind a local entry/first-exit candidate pack to its candidate catalog.
//!
//! This checks format, profile, rule, generation and stored-byte identities.
//! The query-source hashes cannot be rederived without the source file, and
//! neither this check nor a complete stored-record audit proves that the
//! selected query set covers a product domain. No release authority is given.

use clearra_core_executor::{
    accelerator_profile_name, built_in_local_relation_binding, load_local_relation_candidate_pack,
};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_CATALOG_BYTES: usize = 512 * 1024;
const CATALOG_FIELDS: usize = 18;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConditionedLocalCandidateSummary {
    pub generation_identity: [u8; 32],
    pub pack_bytes: usize,
    pub logical_resident_bytes: usize,
    pub record_count: usize,
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
}
