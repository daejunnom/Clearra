//! Local-only producer for sparse, exact BoardConditionedReachability packs.
//!
//! The input is an explicit canonical query set collected by a separate
//! benchmark trace.  Every present record is solved exhaustively; missing
//! records remain cache misses and never become negative answers.

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_executor::{
    built_in_conditioned_reachability_binding, derive_exact_conditioned_reachability_record,
    encode_conditioned_reachability, BoardConditionedReachability,
    ConditionedReachabilityExpectation, ConditionedReachabilityRecord,
};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

const QUERY_SCHEMA: &str = "clearra.conditioned-reachability.query-set.v1";
const CANDIDATE_SCHEMA: &str = "clearra.conditioned-reachability.candidate-catalog.v1";
const MAX_QUERY_BYTES: usize = 16 * 1024 * 1024;
const MAX_QUERIES: usize = 300_000;

#[derive(Clone, Debug)]
pub struct ConditionedReachabilityGenerationOptions {
    pub profile: KickTableProfileId,
    pub queries: PathBuf,
    pub pack: PathBuf,
    pub catalog: PathBuf,
    pub workers: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Query {
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
}

pub fn generate_conditioned_reachability(
    options: &ConditionedReachabilityGenerationOptions,
) -> Result<(), String> {
    validate_options(options)?;
    let raw = fs::read(&options.queries).map_err(|error| format!("query read failed: {error}"))?;
    if raw.is_empty() || raw.len() > MAX_QUERY_BYTES {
        return Err("query set size is outside the bounded producer contract".to_owned());
    }
    let input_identity: [u8; 32] = Sha256::digest(&raw).into();
    let mut queries = parse_queries(&raw, options.profile)?;
    queries.sort_unstable();
    if queries.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("query set contains duplicate identities".to_owned());
    }
    let records = solve_queries(&queries, options.profile, options.workers)?;
    let binding = built_in_conditioned_reachability_binding(options.profile)
        .map_err(|error| error.code().to_owned())?;
    let encoded = encode_conditioned_reachability(binding, &records)
        .map_err(|error| error.code().to_owned())?;
    let loaded = BoardConditionedReachability::load(
        encoded.clone().into(),
        ConditionedReachabilityExpectation {
            binding,
            generation_identity: None,
        },
    )
    .map_err(|error| error.code().to_owned())?;
    publish_immutable(&options.pack, &encoded)?;
    let payload_identity: [u8; 32] = Sha256::digest(&encoded).into();
    let catalog = serde_json::to_vec_pretty(&json!({
        "schema": CANDIDATE_SCHEMA,
        "status": "candidate_unqualified",
        "release_authority": false,
        "signed": false,
        "profile": options.profile.as_str(),
        "query_schema": QUERY_SCHEMA,
        "query_count": queries.len(),
        "query_set_identity": hex(input_identity),
        "record_count": loaded.record_count(),
        "compressed_bytes": encoded.len(),
        "payload_identity": hex(payload_identity),
        "generation_identity": hex(loaded.generation_identity()),
        "rule_identity": hex(binding.rule_identity),
        "evidence": {
            "producer": "clearra exhaustive spawn-to-lock traversal",
            "independent_bounded_differential": null
        }
    }))
    .map_err(|error| format!("catalog encoding failed: {error}"))?;
    publish_immutable(&options.catalog, &catalog)?;
    println!(
        "profile={} queries={} records={} bytes={} generation={}",
        options.profile.as_str(),
        queries.len(),
        loaded.record_count(),
        encoded.len(),
        hex(loaded.generation_identity())
    );
    Ok(())
}

fn validate_options(options: &ConditionedReachabilityGenerationOptions) -> Result<(), String> {
    if !(1..=64).contains(&options.workers) {
        return Err("workers must be in 1..=64".to_owned());
    }
    for path in [&options.queries, &options.pack, &options.catalog] {
        if !path.is_absolute() {
            return Err("all conditioned-reachability paths must be absolute".to_owned());
        }
    }
    if options.pack == options.catalog
        || options.queries == options.pack
        || options.queries == options.catalog
    {
        return Err("query, pack and catalog paths must be distinct".to_owned());
    }
    for output in [&options.pack, &options.catalog] {
        let parent = output
            .parent()
            .ok_or_else(|| "output path has no parent".to_owned())?;
        if !parent.is_dir()
            || fs::symlink_metadata(parent)
                .map_err(|error| error.to_string())?
                .file_type()
                .is_symlink()
        {
            return Err("output parent must be an existing real directory".to_owned());
        }
    }
    Ok(())
}

fn parse_queries(bytes: &[u8], profile: KickTableProfileId) -> Result<Vec<Query>, String> {
    let root: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("query JSON invalid: {error}"))?;
    let object = root
        .as_object()
        .ok_or_else(|| "query root must be an object".to_owned())?;
    if object.len() != 3 || root["schema"] != QUERY_SCHEMA || root["profile"] != profile.as_str() {
        return Err("query set schema or profile binding is invalid".to_owned());
    }
    let values = root["queries"]
        .as_array()
        .ok_or_else(|| "queries must be an array".to_owned())?;
    if values.is_empty() || values.len() > MAX_QUERIES {
        return Err("query count is outside the bounded producer contract".to_owned());
    }
    values.iter().map(parse_query).collect()
}

fn parse_query(value: &Value) -> Result<Query, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "query must be an object".to_owned())?;
    if object.len() != 4 {
        return Err("query has unknown or missing fields".to_owned());
    }
    let width = parse_u8(&value["width"], "width")?;
    let height = parse_u8(&value["height"], "height")?;
    let board_text = value["board"]
        .as_str()
        .ok_or_else(|| "board must be a canonical lowercase hex string".to_owned())?;
    let board = board_text
        .strip_prefix("0x")
        .filter(|digits| !digits.is_empty() && digits.len() <= 15)
        .and_then(|digits| {
            digits
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                .then(|| u64::from_str_radix(digits, 16).ok())
                .flatten()
        })
        .ok_or_else(|| "board must be a canonical lowercase hex string".to_owned())?;
    if format!("0x{board:x}") != board_text {
        return Err("board must not contain leading zeroes".to_owned());
    }
    let piece_text = value["piece"]
        .as_str()
        .ok_or_else(|| "piece must be a string".to_owned())?;
    let mut chars = piece_text.chars();
    let piece = chars
        .next()
        .filter(|_| chars.next().is_none())
        .and_then(|piece| PieceKind::from_ascii(piece).ok())
        .ok_or_else(|| "piece must be one standard uppercase tetromino".to_owned())?;
    if piece.as_ascii().to_string() != piece_text
        || width != 10
        || !(1..=6).contains(&height)
        || board >> (u32::from(width) * u32::from(height)) != 0
    {
        return Err("query is outside the width-10 height-1..6 domain".to_owned());
    }
    Ok(Query {
        width,
        height,
        board,
        piece,
    })
}

fn parse_u8(value: &Value, name: &str) -> Result<u8, String> {
    value
        .as_u64()
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| format!("{name} must be a bounded integer"))
}

fn solve_queries(
    queries: &[Query],
    profile: KickTableProfileId,
    requested_workers: usize,
) -> Result<Vec<ConditionedReachabilityRecord>, String> {
    let next = AtomicUsize::new(0);
    let workers = requested_workers.min(queries.len()).max(1);
    let mut indexed = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            handles.push(scope.spawn(|| {
                let mut local = Vec::new();
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(query) = queries.get(index).copied() else {
                        break;
                    };
                    let record = derive_exact_conditioned_reachability_record(
                        query.width,
                        query.height,
                        query.board,
                        query.piece,
                        profile,
                    )
                    .map_err(|error| error.code().to_owned())?;
                    local.push((index, record));
                }
                Ok::<_, String>(local)
            }));
        }
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| "reachability worker panicked".to_owned())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    indexed.sort_unstable_by_key(|(index, _)| *index);
    if indexed.len() != queries.len() {
        return Err("reachability producer returned an incomplete record set".to_owned());
    }
    Ok(indexed.into_iter().map(|(_, record)| record).collect())
}

fn publish_immutable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.exists() {
        let existing =
            fs::read(path).map_err(|error| format!("existing output read failed: {error}"))?;
        if existing == bytes {
            return Ok(());
        }
        return Err("immutable output already exists with a different identity".to_owned());
    }
    let temporary = path.with_extension(format!(
        "{}.tmp-{}",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("asset"),
        std::process::id()
    ));
    if temporary.exists() {
        fs::remove_file(&temporary)
            .map_err(|error| format!("stale temporary removal failed: {error}"))?;
    }
    fs::write(&temporary, bytes)
        .map_err(|error| format!("temporary output write failed: {error}"))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("immutable output publish failed: {error}"))
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_parser_requires_canonical_profile_bound_inputs() {
        let raw = br#"{"profile":"srs-plus","queries":[{"board":"0x7","height":4,"piece":"I","width":10}],"schema":"clearra.conditioned-reachability.query-set.v1"}"#;
        let parsed = parse_queries(raw, KickTableProfileId::SrsPlus).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].board, 7);
        assert!(parse_queries(raw, KickTableProfileId::SrsX).is_err());
    }
}
