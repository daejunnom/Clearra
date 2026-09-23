//! Local-only candidate producer for board-conditioned entry/first-exit records.
//!
//! This is deliberately separate from the older spawn-to-lock producer. A
//! selected query set and record-by-record primitive audit do not prove that a
//! profile pack is complete, useful, signed, or releasable.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_core_executor::{
    accelerator_profile_name, audit_candidate_local_relation_pack, built_in_local_relation_binding,
    derive_exact_conditioned_local_relation_with_frame, encode_local_relation_candidate_pack,
    load_local_relation_candidate_pack, ConditionedPoseWindow, ConditionedReachabilityEntryPose,
    LocalRelationRowFrame,
};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::conditioned_reachability_generation::{hex, publish_immutable, read_bounded_query_file};

const QUERY_SCHEMA: &str = "clearra.conditioned-local-relation.query-set.v1";
const CANDIDATE_SCHEMA: &str = "clearra.conditioned-local-relation.candidate-catalog.v1";
const MAX_QUERIES: usize = 65_536;
const MAX_ENTRIES: usize = 640;

#[derive(Clone, Debug)]
pub struct ConditionedLocalRelationGenerationOptions {
    pub profile: KickTableProfileId,
    pub queries: PathBuf,
    pub pack: PathBuf,
    pub catalog: PathBuf,
}

struct Query {
    width: u8,
    height: u8,
    board: u64,
    frame: LocalRelationRowFrame,
    piece: PieceKind,
    window: ConditionedPoseWindow,
    entries: Vec<ConditionedReachabilityEntryPose>,
}

pub fn generate_conditioned_local_relation(
    options: &ConditionedLocalRelationGenerationOptions,
) -> Result<(), String> {
    validate_paths(options)?;
    let raw = read_bounded_query_file(&options.queries)?;
    let source_file_identity: [u8; 32] = Sha256::digest(&raw).into();
    let queries = parse_queries(&raw, options.profile)?;
    let query_identity = canonical_query_identity(&queries, options.profile)?;
    let binding = built_in_local_relation_binding(options.profile)
        .map_err(|error| error.code().to_owned())?;
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
    let bytes = encode_local_relation_candidate_pack(binding, &records)
        .map_err(|error| error.code().to_owned())?;
    let loaded = load_local_relation_candidate_pack(&bytes, binding, None)
        .map_err(|error| error.code().to_owned())?;
    let checked = audit_candidate_local_relation_pack(&loaded)
        .map_err(|error| format!("independent local relation audit failed: {error:?}"))?;
    if checked != queries.len() {
        return Err("independent audit did not cover every source query".to_owned());
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
        "query_schema": QUERY_SCHEMA,
        "query_count": queries.len(),
        "query_set_identity": hex(query_identity),
        "source_file_identity": hex(source_file_identity),
        "record_count": loaded.record_count(),
        "independent_checked_records": checked,
        "encoded_bytes": bytes.len(),
        "payload_identity": hex(Sha256::digest(&bytes).into()),
        "generation_identity": hex(loaded.generation_identity()),
        "rule_identity": hex(binding.rule_identity),
        "evidence_scope": "stored-record-and-collision-dependency-only",
        "global_entry_reachability": "not_proven",
        "profile_completeness": "not_proven"
    }))
    .map_err(|error| format!("candidate catalog encoding failed: {error}"))?;
    publish_immutable(&options.catalog, &catalog)?;
    println!(
        "local_relation_candidate=complete profile={} queries={} records={} bytes={} generation={}",
        accelerator_profile_name(options.profile).map_err(|_| "unsupported profile")?,
        queries.len(),
        loaded.record_count(),
        bytes.len(),
        hex(loaded.generation_identity()),
    );
    Ok(())
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

fn parse_query(value: &Value) -> Result<Query, String> {
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

fn query_key(query: &Query) -> Vec<u8> {
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
            let wrong = if profile == KickTableProfileId::Srs90 {
                KickTableProfileId::SrsX
            } else {
                KickTableProfileId::Srs90
            };
            assert!(parse_queries(&raw, wrong).is_err());
        }
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
