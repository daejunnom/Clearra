//! Independent source-bound qualification of the v0.8.1 supported relation domain.
//!
//! An arbitrary complete cover-set is not a complete *product* source. This
//! boundary first proves every declared occupancy cube against primitive
//! reachability, then checks that the declaration contains every supported
//! solver context. It does not sign or publish an asset.

use std::collections::BTreeSet;

use clearra_rules::kicks::KickTableProfileId;

use crate::conditioned_local_candidate_validation::{
    verify_conditioned_local_cover_source, VerifiedBoundedLocalCover,
};
use crate::conditioned_local_relation_generation::parse_solver_cover_domains;

const PIECES: [char; 7] = ['I', 'O', 'T', 'S', 'Z', 'J', 'L'];
const SUPPORTED_CONTEXTS: usize = 56;
const MAX_PRODUCT_PACK_BYTES: usize = 16 * 1024 * 1024;
const MAX_ACTIVE_SESSION_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedV081ConditionedProductCandidate {
    pub bounded_cover: VerifiedBoundedLocalCover,
    /// The closed supported domain is 1-6L, canonical solver sky entries,
    /// seven pieces and the declared original-row frames. In each context,
    /// the lowest eight physical-board bits are unrestricted;
    /// every higher physical cell is fixed empty.
    pub supported_contexts: usize,
}

pub fn verify_v081_conditioned_product_candidate(
    profile: KickTableProfileId,
    pack_bytes: &[u8],
    catalog_bytes: &[u8],
    source_bytes: &[u8],
) -> Result<VerifiedV081ConditionedProductCandidate, String> {
    validate_v081_conditioned_source(profile, source_bytes)?;
    let bounded_cover =
        verify_conditioned_local_cover_source(profile, pack_bytes, catalog_bytes, source_bytes)?;
    if bounded_cover.covered_domains != SUPPORTED_CONTEXTS
        || bounded_cover.candidate.pack_bytes > MAX_PRODUCT_PACK_BYTES
        || bounded_cover
            .candidate
            .logical_resident_bytes
            .saturating_add(bounded_cover.candidate.pack_bytes)
            > MAX_ACTIVE_SESSION_BYTES
    {
        return Err("v0.8.1 conditioned product asset exceeds its declared domain or size".into());
    }
    Ok(VerifiedV081ConditionedProductCandidate {
        bounded_cover,
        supported_contexts: SUPPORTED_CONTEXTS,
    })
}

pub fn validate_v081_conditioned_source(
    profile: KickTableProfileId,
    source_bytes: &[u8],
) -> Result<(), String> {
    let domains = parse_solver_cover_domains(source_bytes, profile)?;
    if domains.len() != SUPPORTED_CONTEXTS {
        return Err("v0.8.1 conditioned source omits a supported solver context".into());
    }
    let mut observed = BTreeSet::new();
    for domain in &domains {
        let query = &domain.query;
        let height = query.height;
        let deleted = query.frame.deleted_original_rows();
        let allowed_frame = if height == 2 {
            matches!(deleted, 0..=2)
        } else {
            deleted == 0
        };
        let physical_bits = 10_u32 * u32::from(query.frame.surviving_rows());
        let free_bits = 8;
        let physical_mask = (1_u64 << physical_bits) - 1;
        let expected_fixed_mask = physical_mask & !((1_u64 << free_bits) - 1);
        if !allowed_frame
            || query.width != 10
            || !(1..=6).contains(&height)
            || query.window.min_y != height as i8
            || domain.fixed_mask != expected_fixed_mask
            || domain.fixed_occupancy != 0
            || query.board != 0
            || !PIECES.contains(&query.piece.as_ascii())
        {
            return Err("v0.8.1 conditioned source changes a supported context".into());
        }
        if !observed.insert((height, deleted, query.piece.as_ascii())) {
            return Err("v0.8.1 conditioned source repeats a solver context".into());
        }
    }
    for height in 1..=6 {
        let frames: &[u8] = if height == 2 { &[0, 1, 2] } else { &[0] };
        for &deleted in frames {
            for piece in PIECES {
                if !observed.contains(&(height, deleted, piece)) {
                    return Err("v0.8.1 conditioned source omits a supported solver context".into());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const SOURCES: &[(KickTableProfileId, &str)] = &[
        (
            KickTableProfileId::Srs90,
            include_str!(
                "../../../config/conditioned-reachability-sources/srs.solver-cover.v1.json"
            ),
        ),
        (
            KickTableProfileId::SrsPlus,
            include_str!(
                "../../../config/conditioned-reachability-sources/srs-plus.solver-cover.v1.json"
            ),
        ),
        (
            KickTableProfileId::SrsX,
            include_str!(
                "../../../config/conditioned-reachability-sources/srs-x.solver-cover.v1.json"
            ),
        ),
        (
            KickTableProfileId::Jstris180,
            include_str!(
                "../../../config/conditioned-reachability-sources/jstris-180.solver-cover.v1.json"
            ),
        ),
        (
            KickTableProfileId::NoKick,
            include_str!(
                "../../../config/conditioned-reachability-sources/no-kick.solver-cover.v1.json"
            ),
        ),
    ];

    #[test]
    fn checked_in_sources_declare_every_supported_context_without_substitution() {
        for &(profile, source) in SOURCES {
            validate_v081_conditioned_source(profile, source.as_bytes()).unwrap();
        }
    }

    #[test]
    fn missing_context_and_narrowed_board_domain_are_not_product_complete() {
        let source = SOURCES[1].1;
        let mut value: Value = serde_json::from_str(source).unwrap();
        value["domains"].as_array_mut().unwrap().pop();
        let truncated = serde_json::to_vec(&value).unwrap();
        assert!(validate_v081_conditioned_source(KickTableProfileId::SrsPlus, &truncated).is_err());

        let mut value: Value = serde_json::from_str(source).unwrap();
        value["domains"][0]["fixed_mask"] = Value::String("0x1".into());
        let narrowed = serde_json::to_vec(&value).unwrap();
        assert!(validate_v081_conditioned_source(KickTableProfileId::SrsPlus, &narrowed).is_err());
    }
}
