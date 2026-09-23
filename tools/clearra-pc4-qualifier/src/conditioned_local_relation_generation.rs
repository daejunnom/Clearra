//! Local-only candidate producer for board-conditioned entry/first-exit records.
//!
//! This is deliberately separate from the older spawn-to-lock producer. A
//! selected query set and record-by-record primitive audit do not prove that a
//! profile pack is complete, useful, signed, or releasable.

use std::{collections::BTreeMap, fs, path::PathBuf};

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_core_executor::{
    accelerator_profile_name, audit_candidate_local_relation_pack,
    audited_local_relation_candidate_pack, built_in_local_relation_binding,
    derive_exact_conditioned_local_relation_with_frame, encode_local_relation_candidate_pack,
    load_local_relation_candidate_pack, ConditionedPoseWindow, ConditionedReachabilityEntryPose,
    ExactConditionedLocalRelation, LocalRelationCoverageDomain, LocalRelationCoverageResult,
    LocalRelationRowFrame,
};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::conditioned_local_coverage_proof::parse_hex;
use crate::conditioned_reachability_generation::{hex, publish_immutable, read_bounded_query_file};

const QUERY_SCHEMA: &str = "clearra.conditioned-local-relation.query-set.v1";
const COVER_SCHEMA: &str = "clearra.conditioned-local-relation.cover-set.v1";
const CANDIDATE_SCHEMA: &str = "clearra.conditioned-local-relation.candidate-catalog.v1";
const MAX_QUERIES: usize = 65_536;
const MAX_ENTRIES: usize = 640;
const MAX_COVER_DOMAINS: usize = 64;

/// Build a candidate cover by repeatedly obtaining a concrete counterexample
/// to the currently audited cubes and deriving the exact relation there.
/// A successful return proves only this declared occupancy domain, not a
/// profile, product asset, or performance qualification. No output is written.
pub fn synthesize_bounded_local_relation_cover(
    domain: &LocalRelationCoverageDomain<'_>,
    max_records: usize,
    max_proof_nodes: u32,
) -> Result<(Vec<u8>, usize), String> {
    let records = synthesize_bounded_local_relation_records(domain, max_records, max_proof_nodes)?;
    let binding =
        built_in_local_relation_binding(domain.profile).map_err(|error| error.code().to_owned())?;
    let bytes = encode_local_relation_candidate_pack(binding, &records)
        .map_err(|error| error.code().to_owned())?;
    Ok((bytes, records.len()))
}

fn synthesize_bounded_local_relation_records(
    domain: &LocalRelationCoverageDomain<'_>,
    max_records: usize,
    max_proof_nodes: u32,
) -> Result<Vec<ExactConditionedLocalRelation>, String> {
    if !(1..=MAX_QUERIES).contains(&max_records) || !(1..=1_000_000).contains(&max_proof_nodes) {
        return Err("bounded cover limits are outside the candidate policy".to_owned());
    }
    let binding =
        built_in_local_relation_binding(domain.profile).map_err(|error| error.code().to_owned())?;
    let mut records = Vec::new();
    let mut next_board = domain.fixed_occupancy;
    let mut remaining_proof_nodes = max_proof_nodes;
    loop {
        if records.len() >= max_records {
            return Err("bounded cover exceeded its record limit".to_owned());
        }
        let relation = derive_exact_conditioned_local_relation_with_frame(
            domain.width,
            domain.height,
            next_board,
            domain.frame,
            domain.piece,
            domain.profile,
            domain.window,
            domain.entries,
        )
        .ok_or_else(|| format!("bounded cover board 0x{next_board:x} is not placeable"))?;
        records.push(relation);
        let bytes = encode_local_relation_candidate_pack(binding, &records)
            .map_err(|error| error.code().to_owned())?;
        let pack = load_local_relation_candidate_pack(&bytes, binding, None)
            .map_err(|error| error.code().to_owned())?;
        let audited = audited_local_relation_candidate_pack(&pack)
            .map_err(|error| format!("independent local relation audit failed: {error:?}"))?;
        let result = audited
            .prove_context_coverage(
                LocalRelationCoverageDomain {
                    width: domain.width,
                    height: domain.height,
                    frame: domain.frame,
                    piece: domain.piece,
                    profile: domain.profile,
                    window: domain.window,
                    entries: domain.entries,
                    fixed_mask: domain.fixed_mask,
                    fixed_occupancy: domain.fixed_occupancy,
                },
                remaining_proof_nodes,
            )
            .map_err(|error| format!("bounded cover domain is invalid: {error:?}"))?;
        match result {
            LocalRelationCoverageResult::Complete { .. } => return Ok(records),
            LocalRelationCoverageResult::Uncovered {
                counterexample_board,
                visited_nodes,
            } => {
                if counterexample_board == next_board {
                    return Err("bounded cover did not advance past its counterexample".to_owned());
                }
                remaining_proof_nodes = remaining_proof_nodes.saturating_sub(visited_nodes);
                if remaining_proof_nodes == 0 {
                    return Err("bounded cover exhausted its aggregate proof budget".to_owned());
                }
                next_board = counterexample_board;
            }
            LocalRelationCoverageResult::Inconclusive { .. } => {
                return Err("bounded cover exhausted its symbolic proof budget".to_owned());
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConditionedLocalRelationGenerationOptions {
    pub profile: KickTableProfileId,
    pub queries: PathBuf,
    pub pack: PathBuf,
    pub catalog: PathBuf,
}

pub(crate) struct Query {
    pub(crate) width: u8,
    pub(crate) height: u8,
    pub(crate) board: u64,
    pub(crate) frame: LocalRelationRowFrame,
    pub(crate) piece: PieceKind,
    pub(crate) window: ConditionedPoseWindow,
    pub(crate) entries: Vec<ConditionedReachabilityEntryPose>,
}

struct CoverDomain {
    query: Query,
    fixed_mask: u64,
    fixed_occupancy: u64,
    max_records: usize,
    max_nodes: u32,
}

pub fn generate_conditioned_local_relation(
    options: &ConditionedLocalRelationGenerationOptions,
) -> Result<(), String> {
    validate_paths(options)?;
    let raw = read_bounded_query_file(&options.queries)?;
    let source_file_identity: [u8; 32] = Sha256::digest(&raw).into();
    let binding = built_in_local_relation_binding(options.profile)
        .map_err(|error| error.code().to_owned())?;
    let source: Value =
        serde_json::from_slice(&raw).map_err(|error| format!("query JSON invalid: {error}"))?;
    let (records, source_count, query_identity, query_schema, evidence_scope) = if source["schema"]
        == COVER_SCHEMA
    {
        let domains = parse_cover_domains(&raw, options.profile)?;
        let identity = canonical_cover_identity(&domains, options.profile)?;
        let mut records = Vec::new();
        for (index, domain) in domains.iter().enumerate() {
            let query = &domain.query;
            let generated = synthesize_bounded_local_relation_records(
                &LocalRelationCoverageDomain {
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
                domain.max_records,
                domain.max_nodes,
            )
            .map_err(|error| format!("cover domain {index}: {error}"))?;
            records.extend(generated);
        }
        (
            records,
            domains.len(),
            identity,
            COVER_SCHEMA,
            "audited-record-and-declared-domain-coverage",
        )
    } else {
        let queries = parse_queries(&raw, options.profile)?;
        let identity = canonical_query_identity(&queries, options.profile)?;
        let records = queries
            .iter()
            .enumerate()
            .map(|(index, query)| {
                derive_exact_conditioned_local_relation_with_frame(
                    query.width,
                    query.height,
                    query.board,
                    query.frame,
                    query.piece,
                    options.profile,
                    query.window,
                    &query.entries,
                )
                .ok_or_else(|| format!("local relation query {index} is not placeable or in scope"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        (
            records,
            queries.len(),
            identity,
            QUERY_SCHEMA,
            "stored-record-and-collision-dependency-only",
        )
    };
    let bytes = encode_local_relation_candidate_pack(binding, &records)
        .map_err(|error| error.code().to_owned())?;
    let loaded = load_local_relation_candidate_pack(&bytes, binding, None)
        .map_err(|error| error.code().to_owned())?;
    let checked = audit_candidate_local_relation_pack(&loaded)
        .map_err(|error| format!("independent local relation audit failed: {error:?}"))?;
    if checked != records.len() {
        return Err("independent audit did not cover every source record".to_owned());
    }

    // Immutable candidate output is useful local evidence, not a signed
    // profile qualification. The product catalog and release gate never read
    // this candidate catalog as authority.
    publish_immutable(&options.pack, &bytes)?;
    let catalog = serde_json::to_vec_pretty(&json!({
        "schema": CANDIDATE_SCHEMA,
        "status": "candidate_unqualified",
        "signed": false,
        "release_authority": false,
        "profile": accelerator_profile_name(options.profile)
            .map_err(|_| "unsupported local relation profile")?,
        "query_schema": query_schema,
        "query_count": source_count,
        "query_set_identity": hex(query_identity),
        "source_file_identity": hex(source_file_identity),
        "record_count": loaded.record_count(),
        "independent_checked_records": checked,
        "encoded_bytes": bytes.len(),
        "payload_identity": hex(Sha256::digest(&bytes).into()),
        "generation_identity": hex(loaded.generation_identity()),
        "rule_identity": hex(binding.rule_identity),
        "evidence_scope": evidence_scope,
        "global_entry_reachability": "not_proven",
        "profile_completeness": "not_proven"
    }))
    .map_err(|error| format!("candidate catalog encoding failed: {error}"))?;
    publish_immutable(&options.catalog, &catalog)?;
    println!(
        "local_relation_candidate=complete profile={} queries={} records={} bytes={} generation={}",
        accelerator_profile_name(options.profile).map_err(|_| "unsupported profile")?,
        source_count,
        loaded.record_count(),
        bytes.len(),
        hex(loaded.generation_identity()),
    );
    Ok(())
}

/// A local status check may parse a candidate without granting it product
/// authority. The independent record audit runs during generation; parsing a
/// file later proves only its structure, profile binding and payload digest.
pub fn structurally_valid_conditioned_local_candidate(
    profile: KickTableProfileId,
    bytes: &[u8],
) -> bool {
    built_in_local_relation_binding(profile)
        .ok()
        .is_some_and(|binding| load_local_relation_candidate_pack(bytes, binding, None).is_ok())
}

fn validate_paths(options: &ConditionedLocalRelationGenerationOptions) -> Result<(), String> {
    if !options.queries.is_absolute()
        || !options.pack.is_absolute()
        || !options.catalog.is_absolute()
    {
        return Err("local relation input and output paths must be absolute".to_owned());
    }
    if options.queries == options.pack
        || options.queries == options.catalog
        || options.pack == options.catalog
    {
        return Err("local relation input and outputs must be distinct".to_owned());
    }
    for path in [&options.pack, &options.catalog] {
        let parent = path.parent().ok_or("candidate output has no parent")?;
        let metadata = fs::symlink_metadata(parent).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("candidate output parent must be a real directory".to_owned());
        }
    }
    accelerator_profile_name(options.profile)
        .map_err(|_| "unsupported local relation profile".to_owned())?;
    Ok(())
}

fn parse_queries(raw: &[u8], profile: KickTableProfileId) -> Result<Vec<Query>, String> {
    let root: Value =
        serde_json::from_slice(raw).map_err(|error| format!("query JSON invalid: {error}"))?;
    let object = root.as_object().ok_or("query root must be an object")?;
    let expected = accelerator_profile_name(profile).map_err(|_| "unsupported profile")?;
    if object.len() != 3 || root["schema"] != QUERY_SCHEMA || root["profile"] != expected {
        return Err("query schema or profile binding is invalid".to_owned());
    }
    let values = root["queries"]
        .as_array()
        .ok_or("queries must be an array")?;
    if values.is_empty() || values.len() > MAX_QUERIES {
        return Err("query count is outside the bounded candidate domain".to_owned());
    }
    let mut unique = BTreeMap::new();
    for value in values {
        let query = parse_query(value)?;
        if unique.insert(query_key(&query), query).is_some() {
            return Err("query set contains duplicate semantic identities".to_owned());
        }
    }
    Ok(unique.into_values().collect())
}

fn parse_cover_domains(
    raw: &[u8],
    profile: KickTableProfileId,
) -> Result<Vec<CoverDomain>, String> {
    let root: Value =
        serde_json::from_slice(raw).map_err(|error| format!("cover JSON invalid: {error}"))?;
    let object = root.as_object().ok_or("cover root must be an object")?;
    let expected = accelerator_profile_name(profile).map_err(|_| "unsupported profile")?;
    if object.len() != 3 || root["schema"] != COVER_SCHEMA || root["profile"] != expected {
        return Err("cover schema or profile binding is invalid".to_owned());
    }
    let values = root["domains"]
        .as_array()
        .ok_or("cover domains must be an array")?;
    if values.is_empty() || values.len() > MAX_COVER_DOMAINS {
        return Err("cover domain count is outside its bound".to_owned());
    }
    let mut unique = BTreeMap::new();
    let mut total_records = 0_usize;
    let mut total_nodes = 0_u64;
    for value in values {
        let fields = value.as_object().ok_or("cover domain must be an object")?;
        if fields.len() != 5 {
            return Err("cover domain has unknown or missing fields".to_owned());
        }
        let query = parse_query(&value["query"])?;
        let fixed_mask = parse_hex(&value["fixed_mask"])?;
        let fixed_occupancy = parse_hex(&value["fixed_occupancy"])?;
        if fixed_occupancy & !fixed_mask != 0 || query.board != fixed_occupancy {
            return Err("cover source board must equal its fixed occupancy".to_owned());
        }
        let max_records = value["max_records"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|&value| (1..=MAX_QUERIES).contains(&value))
            .ok_or("cover max_records outside its bound")?;
        let max_nodes = value["max_nodes"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .filter(|&value| (1..=1_000_000).contains(&value))
            .ok_or("cover max_nodes outside its bound")?;
        total_records = total_records.saturating_add(max_records);
        total_nodes = total_nodes.saturating_add(u64::from(max_nodes));
        if total_records > MAX_QUERIES || total_nodes > 1_000_000 {
            return Err("cover aggregate record or proof budget exceeded".to_owned());
        }
        let domain = CoverDomain {
            query,
            fixed_mask,
            fixed_occupancy,
            max_records,
            max_nodes,
        };
        if unique.insert(cover_domain_key(&domain), domain).is_some() {
            return Err("cover set contains duplicate semantic domains".to_owned());
        }
    }
    Ok(unique.into_values().collect())
}

fn cover_domain_key(domain: &CoverDomain) -> Vec<u8> {
    let mut key = query_key(&domain.query);
    key.extend_from_slice(&domain.fixed_mask.to_le_bytes());
    key.extend_from_slice(&domain.fixed_occupancy.to_le_bytes());
    key.extend_from_slice(&(domain.max_records as u32).to_le_bytes());
    key.extend_from_slice(&domain.max_nodes.to_le_bytes());
    key
}

fn canonical_cover_identity(
    domains: &[CoverDomain],
    profile: KickTableProfileId,
) -> Result<[u8; 32], String> {
    let mut digest = Sha256::new();
    digest.update(COVER_SCHEMA.as_bytes());
    digest.update([0]);
    digest.update(
        accelerator_profile_name(profile)
            .map_err(|_| "unsupported cover profile")?
            .as_bytes(),
    );
    digest.update([0]);
    for domain in domains {
        let key = cover_domain_key(domain);
        digest.update((key.len() as u32).to_le_bytes());
        digest.update(key);
    }
    Ok(digest.finalize().into())
}

pub(crate) fn parse_query(value: &Value) -> Result<Query, String> {
    let object = value.as_object().ok_or("query must be an object")?;
    if object.len() != 7 {
        return Err("query has unknown or missing fields".to_owned());
    }
    let width = parse_u8(&value["width"], "width")?;
    let height = parse_u8(&value["height"], "height")?;
    let frame = LocalRelationRowFrame::new(
        height,
        parse_u16(&value["deleted_original_rows"], "deleted_original_rows")?,
    )
    .ok_or("invalid original-row frame")?;
    let board_text = value["board"]
        .as_str()
        .ok_or("board must be lowercase hex")?;
    let board = board_text
        .strip_prefix("0x")
        .filter(|digits| !digits.is_empty() && digits.len() <= 15)
        .and_then(|digits| u64::from_str_radix(digits, 16).ok())
        .ok_or("board must be lowercase hex")?;
    if format!("0x{board:x}") != board_text || !frame.accepts_physical_board(width, board) {
        return Err("board is noncanonical or outside the compact physical frame".to_owned());
    }
    let piece_text = value["piece"].as_str().ok_or("piece must be a string")?;
    let mut letters = piece_text.chars();
    let piece = letters
        .next()
        .filter(|_| letters.next().is_none())
        .and_then(|letter| PieceKind::from_ascii(letter).ok())
        .ok_or("piece must be one standard tetromino")?;
    if piece.as_ascii().to_string() != piece_text {
        return Err("piece must use its uppercase canonical name".to_owned());
    }
    let bounds = value["window"]
        .as_object()
        .ok_or("window must be an object")?;
    if bounds.len() != 4 {
        return Err("window has unknown or missing fields".to_owned());
    }
    let window = ConditionedPoseWindow {
        min_x: parse_i8(&value["window"]["min_x"], "min_x")?,
        max_x: parse_i8(&value["window"]["max_x"], "max_x")?,
        min_y: parse_i8(&value["window"]["min_y"], "min_y")?,
        max_y: parse_i8(&value["window"]["max_y"], "max_y")?,
    };
    let encoded_entries = value["entries"]
        .as_array()
        .ok_or("entries must be an array")?;
    if encoded_entries.is_empty() || encoded_entries.len() > MAX_ENTRIES {
        return Err("entry count is outside the bounded relation domain".to_owned());
    }
    let mut entries = Vec::with_capacity(encoded_entries.len());
    for entry in encoded_entries {
        let fields = entry.as_object().ok_or("entry must be an object")?;
        if fields.len() != 3 {
            return Err("entry has unknown or missing fields".to_owned());
        }
        entries.push(ConditionedReachabilityEntryPose {
            rotation: RotationState::from_quarter_turns(parse_u8(&entry["rotation"], "rotation")?)
                .map_err(|_| "rotation outside 0..=3")?,
            x: parse_i8(&entry["x"], "entry x")?,
            y: parse_i8(&entry["y"], "entry y")?,
        });
    }
    entries.sort_unstable_by_key(pose_key);
    if entries
        .windows(2)
        .any(|pair| pose_key(&pair[0]) == pose_key(&pair[1]))
    {
        return Err("entry set contains duplicate poses".to_owned());
    }
    Ok(Query {
        width,
        height,
        board,
        frame,
        piece,
        window,
        entries,
    })
}

pub(crate) fn query_key(query: &Query) -> Vec<u8> {
    let mut key = vec![
        query.width,
        query.height,
        query.frame.deleted_original_rows(),
    ];
    key.extend_from_slice(&query.board.to_le_bytes());
    key.push(query.piece.as_ascii() as u8);
    key.extend_from_slice(&[
        query.window.min_x as u8,
        query.window.max_x as u8,
        query.window.min_y as u8,
        query.window.max_y as u8,
    ]);
    for pose in &query.entries {
        key.extend_from_slice(&[pose.rotation.quarter_turns(), pose.x as u8, pose.y as u8]);
    }
    key
}

fn canonical_query_identity(
    queries: &[Query],
    profile: KickTableProfileId,
) -> Result<[u8; 32], String> {
    let mut digest = Sha256::new();
    digest.update(QUERY_SCHEMA.as_bytes());
    digest.update([0]);
    let profile_name =
        accelerator_profile_name(profile).map_err(|_| "unsupported local relation profile")?;
    digest.update(profile_name.as_bytes());
    digest.update([0]);
    for query in queries {
        let key = query_key(query);
        digest.update((key.len() as u32).to_le_bytes());
        digest.update(key);
    }
    Ok(digest.finalize().into())
}

fn pose_key(pose: &ConditionedReachabilityEntryPose) -> (u8, i8, i8) {
    (pose.rotation.quarter_turns(), pose.x, pose.y)
}

fn parse_u8(value: &Value, name: &str) -> Result<u8, String> {
    value
        .as_u64()
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| format!("{name} must be a bounded unsigned integer"))
}

fn parse_u16(value: &Value, name: &str) -> Result<u16, String> {
    value
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| format!("{name} must be a bounded unsigned integer"))
}

fn parse_i8(value: &Value, name: &str) -> Result<i8, String> {
    value
        .as_i64()
        .and_then(|value| i8::try_from(value).ok())
        .ok_or_else(|| format!("{name} must be a bounded signed integer"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_query() -> Value {
        json!({
            "width": 10, "height": 4, "board": "0x0",
            "deleted_original_rows": 4, "piece": "T",
            "window": { "min_x": 4, "max_x": 4, "min_y": 4, "max_y": 4 },
            "entries": [{ "rotation": 0, "x": 4, "y": 4 }]
        })
    }

    #[test]
    fn candidate_queries_bind_profile_row_frame_and_independent_audit() {
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            let name = accelerator_profile_name(profile).unwrap();
            let raw = serde_json::to_vec(&json!({
                "schema": QUERY_SCHEMA, "profile": name,
                "queries": [fixture_query()]
            }))
            .unwrap();
            let queries = parse_queries(&raw, profile).unwrap();
            assert_eq!(queries.len(), 1);
            assert_eq!(queries[0].frame.deleted_original_rows(), 4);
            let binding = built_in_local_relation_binding(profile).unwrap();
            let records = queries
                .iter()
                .map(|query| {
                    derive_exact_conditioned_local_relation_with_frame(
                        query.width,
                        query.height,
                        query.board,
                        query.frame,
                        query.piece,
                        profile,
                        query.window,
                        &query.entries,
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            let bytes = encode_local_relation_candidate_pack(binding, &records).unwrap();
            let loaded = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
            assert_eq!(audit_candidate_local_relation_pack(&loaded), Ok(1));
            assert!(structurally_valid_conditioned_local_candidate(
                profile, &bytes
            ));
            let wrong = if profile == KickTableProfileId::Srs90 {
                KickTableProfileId::SrsX
            } else {
                KickTableProfileId::Srs90
            };
            assert!(!structurally_valid_conditioned_local_candidate(
                wrong, &bytes
            ));
            let mut corrupted = bytes.clone();
            *corrupted.last_mut().unwrap() ^= 1;
            assert!(!structurally_valid_conditioned_local_candidate(
                profile, &corrupted
            ));
            assert!(parse_queries(&raw, wrong).is_err());
        }
    }

    #[test]
    fn bounded_counterexample_cover_closes_one_row_context_without_profile_authority() {
        let profile = KickTableProfileId::NoKick;
        let entries = [ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 1,
        }];
        let window = ConditionedPoseWindow {
            min_x: 4,
            max_x: 4,
            min_y: 0,
            max_y: 1,
        };
        let domain = LocalRelationCoverageDomain {
            width: 10,
            height: 1,
            frame: LocalRelationRowFrame::new(1, 0).unwrap(),
            piece: PieceKind::T,
            profile,
            window,
            entries: &entries,
            fixed_mask: 0,
            fixed_occupancy: 0,
        };
        let (bytes, count) =
            synthesize_bounded_local_relation_cover(&domain, 1024, 100_000).unwrap();
        assert!(count > 1);
        assert!(synthesize_bounded_local_relation_cover(&domain, 1, 100_000).is_err());
        assert!(synthesize_bounded_local_relation_cover(&domain, 1024, 1)
            .unwrap_err()
            .contains("proof budget"));
        let binding = built_in_local_relation_binding(profile).unwrap();
        let pack = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
        assert_eq!(audit_candidate_local_relation_pack(&pack), Ok(count));
        assert!(matches!(
            audited_local_relation_candidate_pack(&pack)
                .unwrap()
                .prove_context_coverage(domain, 100_000),
            Ok(LocalRelationCoverageResult::Complete { .. })
        ));
        for board in 0..1_u64 << 10 {
            let exact = derive_exact_conditioned_local_relation_with_frame(
                10,
                1,
                board,
                LocalRelationRowFrame::new(1, 0).unwrap(),
                PieceKind::T,
                profile,
                window,
                &entries,
            )
            .unwrap();
            let clearra_core_executor::LocalRelationCandidateLookup::Hit(stored) = pack
                .lookup_with_frame(
                    10,
                    1,
                    board,
                    LocalRelationRowFrame::new(1, 0).unwrap(),
                    PieceKind::T,
                    profile,
                    window,
                    &entries,
                )
            else {
                panic!("declared one-row context was not covered at {board:#x}");
            };
            assert_eq!(
                stored.grounded_lock_anchors(),
                exact.grounded_lock_anchors()
            );
            assert_eq!(stored.exits(), exact.exits());
        }
    }

    #[test]
    fn cover_source_publishes_only_an_immutable_unqualified_candidate() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "clearra-conditioned-cover-producer-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let queries = root.join("cover.json");
        let pack = root.join("candidate.cllr");
        let catalog = root.join("candidate.catalog.json");
        let source = json!({
            "schema": COVER_SCHEMA,
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
        });
        fs::write(&queries, serde_json::to_vec(&source).unwrap()).unwrap();
        let options = ConditionedLocalRelationGenerationOptions {
            profile: KickTableProfileId::NoKick,
            queries: queries.clone(),
            pack: pack.clone(),
            catalog: catalog.clone(),
        };
        generate_conditioned_local_relation(&options).unwrap();
        let pack_bytes = fs::read(&pack).unwrap();
        let catalog_bytes = fs::read(&catalog).unwrap();
        let report: Value = serde_json::from_slice(&catalog_bytes).unwrap();
        assert_eq!(report["status"], "candidate_unqualified");
        assert_eq!(report["signed"], false);
        assert_eq!(report["query_schema"], COVER_SCHEMA);
        assert_eq!(report["query_count"], 1);
        assert!(report["record_count"].as_u64().unwrap() > 1);
        assert!(crate::validate_conditioned_local_candidate_catalog(
            KickTableProfileId::NoKick,
            &pack_bytes,
            &catalog_bytes,
        )
        .is_ok());
        generate_conditioned_local_relation(&options).unwrap();
        assert_eq!(fs::read(&pack).unwrap(), pack_bytes);
        assert_eq!(fs::read(&catalog).unwrap(), catalog_bytes);

        let mut limited = source;
        limited["domains"][0]["max_records"] = json!(1);
        fs::write(&queries, serde_json::to_vec(&limited).unwrap()).unwrap();
        let rejected = ConditionedLocalRelationGenerationOptions {
            pack: root.join("rejected.cllr"),
            catalog: root.join("rejected.catalog.json"),
            ..options
        };
        assert!(generate_conditioned_local_relation(&rejected)
            .unwrap_err()
            .contains("record limit"));
        assert!(!rejected.pack.exists());
        assert!(!rejected.catalog.exists());
        let canonical_root = fs::canonicalize(&root).unwrap();
        let canonical_temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        assert_eq!(canonical_root.parent(), Some(canonical_temp.as_path()));
        fs::remove_dir_all(&canonical_root).unwrap();
    }

    #[test]
    fn cover_source_rejects_duplicate_domains_and_nonmatching_seed_board() {
        let query = json!({
            "width": 10, "height": 1, "board": "0x0",
            "deleted_original_rows": 0, "piece": "T",
            "window": { "min_x": 4, "max_x": 4, "min_y": 0, "max_y": 1 },
            "entries": [{ "rotation": 0, "x": 4, "y": 1 }]
        });
        let domain = json!({
            "query": query, "fixed_mask": "0x0", "fixed_occupancy": "0x0",
            "max_records": 1024, "max_nodes": 100000
        });
        let duplicate = serde_json::to_vec(&json!({
            "schema": COVER_SCHEMA, "profile": "no-kick",
            "domains": [domain.clone(), domain.clone()]
        }))
        .unwrap();
        assert!(parse_cover_domains(&duplicate, KickTableProfileId::NoKick)
            .err()
            .unwrap()
            .contains("duplicate"));
        let mut changed = domain;
        changed["fixed_mask"] = json!("0x1");
        changed["fixed_occupancy"] = json!("0x1");
        let wrong_board = serde_json::to_vec(&json!({
            "schema": COVER_SCHEMA, "profile": "no-kick", "domains": [changed]
        }))
        .unwrap();
        assert!(
            parse_cover_domains(&wrong_board, KickTableProfileId::NoKick)
                .err()
                .unwrap()
                .contains("source board")
        );
        assert!(parse_cover_domains(&duplicate, KickTableProfileId::SrsX).is_err());
    }

    #[test]
    fn candidate_producer_publishes_only_audited_immutable_files() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "clearra-conditioned-local-producer-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create this test's isolated output directory");
        let queries = root.join("queries.json");
        let pack = root.join("candidate.cllr");
        let catalog = root.join("candidate.catalog.json");
        let profile = KickTableProfileId::SrsPlus;
        let source = json!({
            "schema": QUERY_SCHEMA,
            "profile": "srs-plus",
            "queries": [fixture_query()],
        });
        fs::write(&queries, serde_json::to_vec(&source).unwrap()).unwrap();
        let options = ConditionedLocalRelationGenerationOptions {
            profile,
            queries: queries.clone(),
            pack: pack.clone(),
            catalog: catalog.clone(),
        };
        generate_conditioned_local_relation(&options).expect("audited candidate generation");
        let first_pack = fs::read(&pack).unwrap();
        let first_catalog = fs::read(&catalog).unwrap();
        let report: Value = serde_json::from_slice(&first_catalog).unwrap();
        assert_eq!(report["status"], "candidate_unqualified");
        assert_eq!(report["signed"], false);
        assert_eq!(report["independent_checked_records"], 1);
        assert_eq!(report["encoded_bytes"], first_pack.len());
        let binding = built_in_local_relation_binding(profile).unwrap();
        let loaded = load_local_relation_candidate_pack(&first_pack, binding, None).unwrap();
        assert_eq!(audit_candidate_local_relation_pack(&loaded), Ok(1));
        let bound = crate::validate_conditioned_local_candidate_catalog(
            profile,
            &first_pack,
            &first_catalog,
        )
        .expect("published candidate pack and catalog bind exactly");
        assert_eq!(bound.record_count, 1);

        generate_conditioned_local_relation(&options).expect("identical rerun is idempotent");
        assert_eq!(fs::read(&pack).unwrap(), first_pack);
        assert_eq!(fs::read(&catalog).unwrap(), first_catalog);

        let mut changed = fixture_query();
        changed["piece"] = json!("L");
        fs::write(
            &queries,
            serde_json::to_vec(&json!({
                "schema": QUERY_SCHEMA,
                "profile": "srs-plus",
                "queries": [changed],
            }))
            .unwrap(),
        )
        .unwrap();
        let error = generate_conditioned_local_relation(&options)
            .expect_err("different candidate cannot replace an immutable pack");
        assert!(error.contains("immutable output already exists"));
        assert_eq!(fs::read(&pack).unwrap(), first_pack);
        assert_eq!(fs::read(&catalog).unwrap(), first_catalog);
        fs::remove_dir_all(&root).expect("remove only this test's isolated outputs");
    }

    #[test]
    fn query_set_rejects_noncanonical_board_frame_and_semantic_duplicates() {
        let profile = KickTableProfileId::Srs90;
        let name = accelerator_profile_name(profile).unwrap();
        let wrap = |queries: Vec<Value>| {
            serde_json::to_vec(&json!({
                "schema": QUERY_SCHEMA,
                "profile": name,
                "queries": queries,
            }))
            .unwrap()
        };
        let mut noncanonical = fixture_query();
        noncanonical["board"] = json!("0x00");
        assert!(parse_queries(&wrap(vec![noncanonical]), profile).is_err());
        let mut wrong_frame = fixture_query();
        wrong_frame["deleted_original_rows"] = json!(16);
        assert!(parse_queries(&wrap(vec![wrong_frame]), profile).is_err());
        let mut outside_physical_board = fixture_query();
        outside_physical_board["board"] = json!("0x40000000");
        assert!(parse_queries(&wrap(vec![outside_physical_board]), profile).is_err());
        let mut reversed_entries = fixture_query();
        reversed_entries["entries"] = json!([
            { "rotation": 1, "x": 4, "y": 4 },
            { "rotation": 0, "x": 4, "y": 4 },
        ]);
        let mut sorted_entries = reversed_entries.clone();
        sorted_entries["entries"] = json!([
            { "rotation": 0, "x": 4, "y": 4 },
            { "rotation": 1, "x": 4, "y": 4 },
        ]);
        assert!(parse_queries(&wrap(vec![reversed_entries, sorted_entries]), profile).is_err());
    }

    #[test]
    fn query_identity_is_independent_of_json_layout_and_source_order() {
        let profile = KickTableProfileId::Srs90;
        let name = accelerator_profile_name(profile).unwrap();
        let first = fixture_query();
        let mut second = fixture_query();
        second["piece"] = json!("L");
        let a = serde_json::to_vec(&json!({
            "schema": QUERY_SCHEMA, "profile": name,
            "queries": [first.clone(), second.clone()],
        }))
        .unwrap();
        let b = serde_json::to_vec_pretty(&json!({
            "schema": QUERY_SCHEMA, "profile": name,
            "queries": [second, first],
        }))
        .unwrap();
        assert_ne!(Sha256::digest(&a), Sha256::digest(&b));
        let a = parse_queries(&a, profile).unwrap();
        let b = parse_queries(&b, profile).unwrap();
        assert_eq!(
            canonical_query_identity(&a, profile),
            canonical_query_identity(&b, profile)
        );
    }
}
