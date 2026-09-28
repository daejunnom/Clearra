//! Self-contained integrity check for a generated legal-board candidate.
//!
//! This binds the final bundle to its candidate catalog without loading the
//! intermediate layer files. It does not prove that the generator enumerated
//! every legal state and never grants product or release authority.

use std::sync::Arc;

use clearra_core_executor::{ExactLegalBoard, LegalBoardExpectation};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::domain::DomainBinding;

const MAX_CATALOG_BYTES: usize = 512 * 1024;
const LAYERS: usize = 11;
const MEET_LAYER: usize = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalBoardCandidateSummary {
    pub generation_identity: [u8; 32],
    pub bundle_bytes: usize,
    pub sparse_index_bytes: usize,
    pub layer_counts: [u64; LAYERS],
}

pub fn validate_legal_board_candidate_catalog(
    profile: KickTableProfileId,
    expected_bundle_name: &str,
    bundle: Arc<[u8]>,
    catalog_bytes: &[u8],
) -> Result<LegalBoardCandidateSummary, &'static str> {
    if catalog_bytes.len() > MAX_CATALOG_BYTES {
        return Err("legal-board candidate catalog too large");
    }
    let catalog: Value = serde_json::from_slice(catalog_bytes)
        .map_err(|_| "legal-board candidate catalog invalid")?;
    let binding = DomainBinding::legal_board(profile)
        .map_err(|_| "legal-board candidate profile unsupported")?;
    let profile_name = clearra_core_executor::accelerator_profile_name(profile)
        .map_err(|_| "legal-board candidate profile unsupported")?;
    if catalog["schema"] != "clearra.legal-board.catalog.candidate.v2"
        || catalog["status"] != "candidate_unqualified"
        || catalog["construction"] != "bidirectional_exact_intersection_v1"
        || catalog["meet_layer"] != MEET_LAYER
        || catalog["profile"] != profile_name
        || catalog["rule_identity"] != binding.identity_string()
        || catalog["bundle"]["file"] != expected_bundle_name
        || !catalog["bundle"]["url"].is_null()
        || catalog["qualification"]["exact_intersection"] != true
        || catalog["qualification"]["forward_prefix_complete"] != true
        || catalog["qualification"]["reverse_suffix_complete"] != true
        || catalog["qualification"]["inductive_legal_layers_complete"] != true
        || catalog["qualification"]["signed"] != false
        || catalog["qualification"]["release_authority"] != false
    {
        return Err("legal-board candidate catalog binding mismatch");
    }
    let bundle_bytes = bundle.len();
    let bundle_sha256: [u8; 32] = Sha256::digest(bundle.as_ref()).into();
    if catalog["bundle"]["bytes"].as_u64() != Some(bundle_bytes as u64)
        || catalog["bundle"]["sha256"] != digest_label(&bundle_sha256)
    {
        return Err("legal-board candidate bundle digest mismatch");
    }
    let loaded = ExactLegalBoard::load(
        bundle,
        LegalBoardExpectation {
            binding: binding.legal_board_binding(),
            generation_identity: None,
        },
    )
    .map_err(|_| "legal-board candidate bundle invalid")?;
    if catalog["generation_identity"] != digest_label(&loaded.generation_identity()) {
        return Err("legal-board candidate generation mismatch");
    }
    let layers = catalog["layers"]
        .as_array()
        .filter(|layers| layers.len() == LAYERS)
        .ok_or("legal-board candidate layer directory invalid")?;
    let mut layer_counts = [0_u64; LAYERS];
    for (layer, entry) in layers.iter().enumerate() {
        let count = loaded
            .layer_count(layer)
            .ok_or("legal-board candidate layer missing")?;
        let payload_digest = loaded
            .layer_payload_digest(layer)
            .ok_or("legal-board candidate layer missing")?;
        if entry["layer"].as_u64() != Some(layer as u64)
            || entry["field_count"].as_u64() != Some(count)
            || entry["payload_sha256"] != digest_label(&payload_digest)
            || !valid_digest_label(&entry["legal_layer_identity"])
            || (layer <= MEET_LAYER) != valid_digest_label(&entry["forward_domain_identity"])
            || (layer > MEET_LAYER) != valid_digest_label(&entry["reverse_domain_identity"])
        {
            return Err("legal-board candidate layer binding mismatch");
        }
        layer_counts[layer] = count;
    }
    Ok(LegalBoardCandidateSummary {
        generation_identity: loaded.generation_identity(),
        bundle_bytes,
        sparse_index_bytes: loaded.sparse_index_bytes(),
        layer_counts,
    })
}

fn digest_label(digest: &[u8; 32]) -> String {
    let mut output = String::with_capacity(71);
    output.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write;
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

fn valid_digest_label(value: &Value) -> bool {
    value
        .as_str()
        .and_then(|label| label.strip_prefix("sha256:"))
        .is_some_and(|digits| {
            digits.len() == 64
                && digits
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_executor::encode_exact_legal_board_intersection_streaming;

    fn fixture() -> (Arc<[u8]>, Value) {
        let binding = DomainBinding::legal_board(KickTableProfileId::SrsPlus).unwrap();
        let bytes = encode_exact_legal_board_intersection_streaming(
            binding.legal_board_binding(),
            |layer, emit| {
                if layer == 0 {
                    emit(0)?;
                } else if layer == 10 {
                    emit((1_u64 << 40) - 1)?;
                }
                Ok::<(), clearra_core_executor::LegalBoardAssetError>(())
            },
        )
        .unwrap();
        let loaded = ExactLegalBoard::load(
            Arc::from(bytes.clone()),
            LegalBoardExpectation {
                binding: binding.legal_board_binding(),
                generation_identity: None,
            },
        )
        .unwrap();
        let layers = (0..LAYERS)
            .map(|layer| {
                serde_json::json!({
                    "layer": layer,
                    "field_count": loaded.layer_count(layer).unwrap(),
                    "payload_sha256": digest_label(&loaded.layer_payload_digest(layer).unwrap()),
                    "forward_domain_identity": (layer <= MEET_LAYER).then(|| digest_label(&[0; 32])),
                    "reverse_domain_identity": (layer > MEET_LAYER).then(|| digest_label(&[0; 32])),
                    "legal_layer_identity": digest_label(&[0; 32]),
                })
            })
            .collect::<Vec<_>>();
        let catalog = serde_json::json!({
            "schema": "clearra.legal-board.catalog.candidate.v2",
            "status": "candidate_unqualified",
            "construction": "bidirectional_exact_intersection_v1",
            "meet_layer": MEET_LAYER,
            "profile": "srs-plus",
            "rule_identity": binding.identity_string(),
            "generation_identity": digest_label(&loaded.generation_identity()),
            "bundle": {
                "file": "legal-board-srs-plus-v2.cllb",
                "bytes": bytes.len(),
                "sha256": digest_label(&Sha256::digest(&bytes).into()),
                "url": null,
            },
            "qualification": {
                "exact_intersection": true,
                "forward_prefix_complete": true,
                "reverse_suffix_complete": true,
                "inductive_legal_layers_complete": true,
                "signed": false,
                "release_authority": false,
            },
            "layers": layers,
        });
        (Arc::from(bytes), catalog)
    }

    fn validate(
        bytes: Arc<[u8]>,
        catalog: &Value,
    ) -> Result<LegalBoardCandidateSummary, &'static str> {
        validate_legal_board_candidate_catalog(
            KickTableProfileId::SrsPlus,
            "legal-board-srs-plus-v2.cllb",
            bytes,
            &serde_json::to_vec(catalog).unwrap(),
        )
    }

    #[test]
    fn final_bundle_and_catalog_are_bound_without_granting_product_authority() {
        let (bytes, catalog) = fixture();
        let summary = validate(Arc::clone(&bytes), &catalog).unwrap();
        assert_eq!(summary.bundle_bytes, bytes.len());
        assert_eq!(summary.layer_counts[0], 1);
        assert_eq!(summary.layer_counts[10], 1);
        assert!(summary.sparse_index_bytes > 0);

        for pointer in [
            "/status",
            "/profile",
            "/generation_identity",
            "/bundle/sha256",
            "/layers/0/payload_sha256",
        ] {
            let mut corrupted = catalog.clone();
            *corrupted.pointer_mut(pointer).unwrap() = Value::String("wrong".into());
            assert!(
                validate(Arc::clone(&bytes), &corrupted).is_err(),
                "{pointer}"
            );
        }
        let mut falsely_qualified = catalog.clone();
        falsely_qualified["qualification"]["signed"] = Value::Bool(true);
        assert!(validate(Arc::clone(&bytes), &falsely_qualified).is_err());
        let mut corrupted_bytes = bytes.to_vec();
        *corrupted_bytes.last_mut().unwrap() ^= 1;
        assert!(validate(Arc::from(corrupted_bytes), &catalog).is_err());
    }
}
