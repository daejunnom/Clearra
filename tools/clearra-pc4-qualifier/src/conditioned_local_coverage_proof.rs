//! Bounded, non-publishing completeness proof for declared local contexts.
//!
//! A candidate's independent record audit is not a coverage proof. This
//! module proves that every valid board in each explicitly declared occupancy
//! subdomain hits an audited record. It deliberately cannot sign or qualify a
//! whole profile, and a narrow domain cannot satisfy the release speed gate.

use std::{collections::BTreeSet, fs, io::Read, path::PathBuf};

use clearra_core_executor::{
    accelerator_profile_name, audited_local_relation_candidate_pack,
    built_in_local_relation_binding, load_local_relation_candidate_pack,
    LocalRelationCandidateLookup, LocalRelationCoverageDomain, LocalRelationCoverageResult,
};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::conditioned_local_relation_generation::{parse_query, query_key, Query};
use crate::conditioned_reachability_generation::{hex, publish_immutable, read_bounded_query_file};

const REQUEST_SCHEMA: &str = "clearra.conditioned-local-relation.coverage-request.v1";
const REPORT_SCHEMA: &str = "clearra.conditioned-local-relation.coverage-report.v1";
const MAX_PACK_BYTES: u64 = 16 * 1024 * 1024;
const MAX_REPORT_BYTES: usize = 16 * 1024 * 1024;
const MAX_DOMAINS: usize = 1024;
// The proof is sequential and retains only one context's bounded search.
// Its aggregate allowance must cover a complete declared profile source,
// not accidentally stop after the first 1M-node context.
const MAX_TOTAL_PROOF_NODES: u64 = 64 * 1_000_000;

#[derive(Clone, Debug)]
pub struct ConditionedLocalCoverageProofOptions {
    pub profile: KickTableProfileId,
    pub pack: PathBuf,
    pub request: PathBuf,
    pub report: PathBuf,
}

struct RequestedDomain {
    query: Query,
    fixed_mask: u64,
    fixed_occupancy: u64,
    max_nodes: u32,
}

pub fn prove_conditioned_local_candidate_coverage(
    options: &ConditionedLocalCoverageProofOptions,
) -> Result<(), String> {
    validate_paths(options)?;
    let profile = accelerator_profile_name(options.profile)
        .map_err(|_| "unsupported local relation profile")?;
    let raw_request = read_bounded_query_file(&options.request)?;
    let domains = parse_request(&raw_request, profile)?;
    let pack_bytes = read_regular_bounded(&options.pack, MAX_PACK_BYTES)?;
    let binding = built_in_local_relation_binding(options.profile)
        .map_err(|error| error.code().to_owned())?;
    let pack = load_local_relation_candidate_pack(&pack_bytes, binding, None)
        .map_err(|error| error.code().to_owned())?;
    let audited = audited_local_relation_candidate_pack(&pack)
        .map_err(|error| format!("independent local relation audit failed: {error:?}"))?;

    let mut results = Vec::with_capacity(domains.len());
    for (index, domain) in domains.iter().enumerate() {
        let query = &domain.query;
        if query.board & domain.fixed_mask != domain.fixed_occupancy
            || !matches!(
                pack.lookup_with_frame(
                    query.width,
                    query.height,
                    query.board,
                    query.frame,
                    query.piece,
                    options.profile,
                    query.window,
                    &query.entries,
                ),
                LocalRelationCandidateLookup::Hit(_)
            )
        {
            return Err(format!(
                "coverage domain {index} is not anchored by a matching source query"
            ));
        }
        let proof = audited
            .prove_context_coverage(
                LocalRelationCoverageDomain {
                    width: query.width,
                    height: query.height,
                    frame: query.frame,
                    piece: query.piece,
                    profile: options.profile,
                    window: query.window,
                    entries: &query.entries,
                    fixed_mask: domain.fixed_mask,
                    fixed_occupancy: domain.fixed_occupancy,
                },
                domain.max_nodes,
            )
            .map_err(|error| format!("coverage domain {index} is invalid: {error:?}"))?;
        let (effective_mask, effective_value, context_records, visited_nodes) = match proof {
            LocalRelationCoverageResult::Complete {
                effective_fixed_mask,
                effective_fixed_occupancy,
                context_records,
                visited_nodes,
            } => (
                effective_fixed_mask,
                effective_fixed_occupancy,
                context_records,
                visited_nodes,
            ),
            LocalRelationCoverageResult::Uncovered {
                counterexample_board,
                ..
            } => {
                return Err(format!(
                    "coverage domain {index} has counterexample board 0x{counterexample_board:x}"
                ));
            }
            LocalRelationCoverageResult::Inconclusive { .. } => {
                return Err(format!(
                    "coverage domain {index} exhausted its proof budget"
                ));
            }
        };
        let physical_bits = u32::from(query.width) * u32::from(query.frame.surviving_rows());
        let physical_mask = (1_u64 << physical_bits) - 1;
        let free_board_bits = physical_bits - (effective_mask & physical_mask).count_ones();
        let entries = query
            .entries
            .iter()
            .map(|entry| {
                json!({
                    "rotation": entry.rotation.quarter_turns(), "x": entry.x, "y": entry.y,
                })
            })
            .collect::<Vec<_>>();
        results.push(json!({
            "source_query_identity": hex(Sha256::digest(query_key(query)).into()),
            "source_query": {
                "width": query.width,
                "height": query.height,
                "board": format!("0x{:x}", query.board),
                "deleted_original_rows": query.frame.deleted_original_rows(),
                "piece": query.piece.as_ascii().to_string(),
                "window": {
                    "min_x": query.window.min_x,
                    "max_x": query.window.max_x,
                    "min_y": query.window.min_y,
                    "max_y": query.window.max_y,
                },
                "entries": entries,
            },
            "fixed_mask": format!("0x{effective_mask:x}"),
            "fixed_occupancy": format!("0x{effective_value:x}"),
            "free_board_bits": free_board_bits,
            "context_records": context_records,
            "visited_proof_nodes": visited_nodes,
        }));
    }
    let report = serde_json::to_vec_pretty(&json!({
        "schema": REPORT_SCHEMA,
        "status": "bounded_context_coverage_only",
        "release_authority": false,
        "profile": profile,
        "pack_identity": hex(Sha256::digest(&pack_bytes).into()),
        "generation_identity": hex(pack.generation_identity()),
        "rule_identity": hex(binding.rule_identity),
        "request_identity": hex(Sha256::digest(&raw_request).into()),
        "audited_record_count": pack.record_count(),
        "covered_domains": results,
        "evidence_scope": "declared-placeable-entry-context-and-occupancy-subdomain",
        "global_entry_reachability": "not_proven",
        "profile_completeness": "not_proven",
        "performance_qualification": "not_run",
    }))
    .map_err(|error| format!("coverage report encoding failed: {error}"))?;
    if report.len() > MAX_REPORT_BYTES {
        return Err("coverage report exceeds its output bound".to_owned());
    }
    publish_immutable(&options.report, &report)?;
    println!(
        "local_relation_coverage=bounded_only profile={profile} domains={} records={} report={}",
        domains.len(),
        pack.record_count(),
        options.report.display(),
    );
    Ok(())
}

fn validate_paths(options: &ConditionedLocalCoverageProofOptions) -> Result<(), String> {
    if !options.pack.is_absolute()
        || !options.request.is_absolute()
        || !options.report.is_absolute()
    {
        return Err("coverage paths must be absolute".to_owned());
    }
    if options.pack == options.request
        || options.pack == options.report
        || options.request == options.report
    {
        return Err("coverage paths must be distinct".to_owned());
    }
    let parent = options
        .report
        .parent()
        .ok_or("coverage report has no parent")?;
    let metadata = fs::symlink_metadata(parent).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("coverage report parent must be a real directory".to_owned());
    }
    Ok(())
}

fn read_regular_bounded(path: &PathBuf, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit {
        return Err("coverage pack must be a bounded regular file".to_owned());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("coverage pack grew beyond its bound".to_owned());
    }
    Ok(bytes)
}

fn parse_request(raw: &[u8], profile: &str) -> Result<Vec<RequestedDomain>, String> {
    let root: Value =
        serde_json::from_slice(raw).map_err(|error| format!("coverage JSON invalid: {error}"))?;
    let object = root.as_object().ok_or("coverage root must be an object")?;
    if object.len() != 3 || root["schema"] != REQUEST_SCHEMA || root["profile"] != profile {
        return Err("coverage schema or profile binding is invalid".to_owned());
    }
    let values = root["domains"]
        .as_array()
        .ok_or("coverage domains must be an array")?;
    if values.is_empty() || values.len() > MAX_DOMAINS {
        return Err("coverage domain count is outside its bound".to_owned());
    }
    let mut seen = BTreeSet::new();
    let mut domains = Vec::with_capacity(values.len());
    let mut total_nodes = 0_u64;
    for value in values {
        let fields = value
            .as_object()
            .ok_or("coverage domain must be an object")?;
        if fields.len() != 4 {
            return Err("coverage domain has unknown or missing fields".to_owned());
        }
        let query = parse_query(&value["query"])?;
        let fixed_mask = parse_hex(&value["fixed_mask"])?;
        let fixed_occupancy = parse_hex(&value["fixed_occupancy"])?;
        let max_nodes = value["max_nodes"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .filter(|&value| (1..=1_000_000).contains(&value))
            .ok_or("coverage max_nodes outside 1..=1000000")?;
        total_nodes = total_nodes.saturating_add(u64::from(max_nodes));
        if total_nodes > MAX_TOTAL_PROOF_NODES {
            return Err("coverage aggregate proof budget exceeded".to_owned());
        }
        let mut key = query_key(&query);
        key.extend_from_slice(&fixed_mask.to_le_bytes());
        key.extend_from_slice(&fixed_occupancy.to_le_bytes());
        if !seen.insert(key) {
            return Err("coverage request has duplicate domains".to_owned());
        }
        domains.push(RequestedDomain {
            query,
            fixed_mask,
            fixed_occupancy,
            max_nodes,
        });
    }
    Ok(domains)
}

pub(crate) fn parse_hex(value: &Value) -> Result<u64, String> {
    let text = value
        .as_str()
        .ok_or("coverage mask must be lowercase hex")?;
    let number = text
        .strip_prefix("0x")
        .filter(|digits| !digits.is_empty() && digits.len() <= 15)
        .and_then(|digits| u64::from_str_radix(digits, 16).ok())
        .ok_or("coverage mask must be lowercase hex")?;
    if text != format!("0x{number:x}") {
        return Err("coverage mask must be canonical lowercase hex".to_owned());
    }
    Ok(number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_core_executor::{
        derive_exact_conditioned_local_relation, encode_local_relation_candidate_pack,
        ConditionedPoseWindow, ConditionedReachabilityEntryPose,
    };

    #[test]
    fn coverage_request_rejects_duplicate_or_ambiguous_domains() {
        let query = json!({
            "width": 10, "height": 4, "board": "0x0",
            "deleted_original_rows": 0, "piece": "T",
            "window": { "min_x": 4, "max_x": 4, "min_y": 4, "max_y": 4 },
            "entries": [{ "rotation": 0, "x": 4, "y": 4 }]
        });
        let domain = json!({
            "query": query, "fixed_mask": "0x0", "fixed_occupancy": "0x0",
            "max_nodes": 1024
        });
        let raw = serde_json::to_vec(&json!({
            "schema": REQUEST_SCHEMA, "profile": "srs-plus",
            "domains": [domain.clone(), domain]
        }))
        .unwrap();
        assert!(parse_request(&raw, "srs-plus").is_err());
        assert_eq!(
            parse_request(&raw, "no-kick").err().unwrap(),
            "coverage schema or profile binding is invalid"
        );
        assert!(parse_hex(&json!("0x00")).is_err());
    }

    #[test]
    fn coverage_report_is_immutable_and_never_grants_profile_authority() {
        let profile = KickTableProfileId::NoKick;
        let window = ConditionedPoseWindow {
            min_x: 4,
            max_x: 4,
            min_y: 4,
            max_y: 4,
        };
        let entries = [ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 4,
        }];
        let record = derive_exact_conditioned_local_relation(
            10,
            4,
            0,
            PieceKind::T,
            profile,
            window,
            &entries,
        )
        .unwrap();
        let mask = record.dependency_mask();
        assert_ne!(mask, 0);
        let binding = built_in_local_relation_binding(profile).unwrap();
        let bytes = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "clearra-conditioned-local-coverage-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let pack = directory.join("candidate.cllr");
        let request = directory.join("request.json");
        let report = directory.join("report.json");
        fs::write(&pack, &bytes).unwrap();
        let query = json!({
            "width": 10, "height": 4, "board": "0x0",
            "deleted_original_rows": 0, "piece": "T",
            "window": { "min_x": 4, "max_x": 4, "min_y": 4, "max_y": 4 },
            "entries": [{ "rotation": 0, "x": 4, "y": 4 }]
        });
        let source = serde_json::to_vec(&json!({
            "schema": REQUEST_SCHEMA,
            "profile": "no-kick",
            "domains": [{
                "query": query,
                "fixed_mask": format!("0x{mask:x}"),
                "fixed_occupancy": "0x0",
                "max_nodes": 1024
            }]
        }))
        .unwrap();
        fs::write(&request, &source).unwrap();
        let options = ConditionedLocalCoverageProofOptions {
            profile,
            pack: pack.clone(),
            request: request.clone(),
            report: report.clone(),
        };
        prove_conditioned_local_candidate_coverage(&options).unwrap();
        let value: Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
        assert_eq!(value["status"], "bounded_context_coverage_only");
        assert_eq!(value["release_authority"], false);
        assert_eq!(value["profile_completeness"], "not_proven");
        assert_eq!(value["covered_domains"][0]["context_records"], 1);
        assert_eq!(value["covered_domains"][0]["source_query"]["piece"], "T");
        assert!(value["covered_domains"][0]["free_board_bits"]
            .as_u64()
            .is_some_and(|bits| bits > 0));
        prove_conditioned_local_candidate_coverage(&options).unwrap();

        fs::remove_file(&report).unwrap();
        let incomplete = serde_json::to_vec(&json!({
            "schema": REQUEST_SCHEMA,
            "profile": "no-kick",
            "domains": [{
                "query": query,
                "fixed_mask": "0x0",
                "fixed_occupancy": "0x0",
                "max_nodes": 1024
            }]
        }))
        .unwrap();
        fs::write(&request, incomplete).unwrap();
        assert!(prove_conditioned_local_candidate_coverage(&options)
            .unwrap_err()
            .contains("counterexample board"));
        assert!(!report.exists());
        fs::remove_file(&request).unwrap();
        fs::remove_file(&pack).unwrap();
        fs::remove_dir(&directory).unwrap();
    }
}
