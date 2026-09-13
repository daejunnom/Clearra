//! SRP: bounded validation of the complete Jstris graph/index file format.
//! Completion is the upstream declaration; samples are format evidence only.
use super::{Artifact, Result, REPOSITORY};
use serde_json::{json, Value};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub(super) fn qualify(directory: &Path, revision: &str, artifacts: &[Artifact]) -> Result<Value> {
    if artifacts.len() != 3 {
        return Err("tablebase: three qualified artifacts are required");
    }
    let mut fields = File::open(directory.join(artifacts[0].path))
        .map_err(|_| "tablebase: cannot open field index")?;
    let mut offsets = File::open(directory.join(artifacts[1].path))
        .map_err(|_| "tablebase: cannot open offset index")?;
    let mut graph = File::open(directory.join(artifacts[2].path))
        .map_err(|_| "tablebase: cannot open graph")?;
    let count = header(&read(&mut fields, 0, 16)?, b"FHIDIDX1")?;
    if count < 2
        || count > 1 << 24
        || count != header(&read(&mut offsets, 0, 16)?, b"GOFFIDX1")?
        || artifacts[0].size != 16 + u64::from(count) * 8
        || artifacts[1].size != 16 + (u64::from(count) + 1) * 4
    {
        return Err("tablebase: profile index layout mismatch");
    }
    let mut ids = Vec::new();
    for id in [0, 1, 100, 10_000, count / 2, count - 1] {
        if id < count && !ids.contains(&id) {
            ids.push(id);
        }
    }
    let mut evidence = Vec::new();
    for id in ids {
        let field = read(&mut fields, 16 + u64::from(id) * 8, 8)?;
        let hash = little(&field[..5]);
        if little(&field[5..]) != u64::from(id) {
            return Err("tablebase: field ID is not its index ordinal");
        }
        let pair = read(&mut offsets, 16 + u64::from(id) * 4, 8)?;
        let start = little(&pair[..4]);
        let end = little(&pair[4..]);
        if end <= start
            || end > artifacts[2].size
            || end - start > 16_384
            || (id == 0 && (start != 0 || hash != 0))
            || (id == count - 1 && (end != artifacts[2].size || hash != (1 << 40) - 1))
        {
            return Err("tablebase: graph bounds or terminal identity mismatch");
        }
        let record = read(&mut graph, start, (end - start) as usize)?;
        if record.len() < 12
            || record[..5]
                .iter()
                .fold(0_u64, |n, b| n * 256 + u64::from(*b))
                != hash
        {
            return Err("tablebase: graph source bitmap mismatch");
        }
        let mut cursor = 5;
        for _ in 0..7 {
            let degree = usize::from(
                *record
                    .get(cursor)
                    .ok_or("tablebase: truncated graph record")?,
            );
            cursor += 1;
            for _ in 0..degree {
                let edge = record
                    .get(cursor..cursor + 3)
                    .ok_or("tablebase: truncated graph targets")?;
                if little(edge) >= u64::from(count) {
                    return Err("tablebase: graph target outside field domain");
                }
                cursor += 3;
            }
        }
        if cursor != record.len() {
            return Err("tablebase: graph record has trailing bytes");
        }
        evidence.push(json!({ "id": id, "hash": hash, "start": start, "end": end }));
    }
    let profiles = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"].into_iter().map(|profile| {
        if profile != "jstris-180" { return json!({ "profile": profile, "upstream_complete": false, "status": "unavailable", "reason": "missing-profile-specific-index" }); }
        json!({ "profile": profile, "upstream_complete": true, "status": "ready",
            "reader_contract": "hydra-jstris-180-complete-graph-v1", "field_count": count,
            "target_width": 3, "target_lines": [4], "terminal_id": count - 1,
            "artifacts": { "fields": artifacts[0].value(), "offsets": artifacts[1].value(), "graph": artifacts[2].value() }, "evidence": evidence })
    }).collect::<Vec<_>>();
    Ok(
        json!({ "schema": "clearra.pc4.host-generation.v1", "repository": REPOSITORY, "revision": revision,
        "profiles": profiles, "transferred_bytes": 0 }),
    )
}
fn read(file: &mut File, offset: u64, length: usize) -> Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| "tablebase: cannot seek artifact")?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)
        .map_err(|_| "tablebase: truncated artifact")?;
    Ok(bytes)
}
fn little(bytes: &[u8]) -> u64 {
    bytes.iter().rev().fold(0, |n, b| n * 256 + u64::from(*b))
}
fn header(bytes: &[u8], magic: &[u8; 8]) -> Result<u32> {
    if &bytes[..8] != magic || little(&bytes[8..12]) != 1 {
        return Err("tablebase: unsupported index header");
    }
    Ok(little(&bytes[12..16]) as u32)
}
