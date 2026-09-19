//! SRP: bounded validation of the Jstris graph/index reader format.
//! Upstream completion and sampled bytes make the reader ready, but never mint
//! product target authority without a separate exact qualification receipt.
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
    qualify_with_reader(revision, artifacts, |role, offset, length| {
        let file = match role {
            0 => &mut fields,
            1 => &mut offsets,
            _ => &mut graph,
        };
        read(file, offset, length)
    })
}

/// The same byte/format qualification is used for full files and bounded HTTP
/// slices. A transport callback grants no completeness or profile authority.
pub(super) fn qualify_with_reader(
    revision: &str,
    artifacts: &[Artifact],
    mut reader: impl FnMut(usize, u64, usize) -> Result<Vec<u8>>,
) -> Result<Value> {
    qualify_with_reader_many(revision, artifacts, |demands| {
        demands
            .iter()
            .map(|demand| reader(demand.role, demand.offset, demand.length))
            .collect()
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct QualificationDemand {
    pub(super) role: usize,
    pub(super) offset: u64,
    pub(super) length: usize,
}

/// Runs the same qualification in three dependency stages: headers, sampled
/// index addresses, then sampled graph records. A concurrent transport may
/// submit every demand in one stage together while the scalar/local adapter
/// above preserves the exact same validator and evidence contract.
pub(super) fn qualify_with_reader_many(
    revision: &str,
    artifacts: &[Artifact],
    mut reader: impl FnMut(&[QualificationDemand]) -> Result<Vec<Vec<u8>>>,
) -> Result<Value> {
    if artifacts.len() != 3 || !super::hex(revision, 40) {
        return Err("tablebase: invalid generation for qualification");
    }
    let mut read_stage = |demands: &[QualificationDemand]| -> Result<Vec<Vec<u8>>> {
        if demands.is_empty() || demands.len() > 16 {
            return Err("tablebase: qualification stage is outside transport bounds");
        }
        for demand in demands {
            let artifact = artifacts
                .get(demand.role)
                .ok_or("tablebase: qualification slice is outside artifact bounds")?;
            if demand.length == 0
                || demand.length > 65_536
                || demand
                    .offset
                    .checked_add(demand.length as u64)
                    .is_none_or(|end| end > artifact.size)
            {
                return Err("tablebase: qualification slice is outside artifact bounds");
            }
        }
        let bytes = reader(demands)?;
        if bytes.len() != demands.len()
            || bytes
                .iter()
                .zip(demands)
                .any(|(bytes, demand)| bytes.len() != demand.length)
        {
            return Err("tablebase: truncated qualification slice");
        }
        Ok(bytes)
    };
    let headers = read_stage(&[
        QualificationDemand {
            role: 0,
            offset: 0,
            length: 16,
        },
        QualificationDemand {
            role: 1,
            offset: 0,
            length: 16,
        },
    ])?;
    let count = header(&headers[0], b"FHIDIDX1")?;
    if count < 2
        || count > 1 << 24
        || count != header(&headers[1], b"GOFFIDX1")?
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
    let index_demands = ids
        .iter()
        .flat_map(|id| {
            [
                QualificationDemand {
                    role: 0,
                    offset: 16 + u64::from(*id) * 8,
                    length: 8,
                },
                QualificationDemand {
                    role: 1,
                    offset: 16 + u64::from(*id) * 4,
                    length: 8,
                },
            ]
        })
        .collect::<Vec<_>>();
    let indices = read_stage(&index_demands)?;
    let mut evidence = Vec::with_capacity(ids.len());
    for (index, id) in ids.into_iter().enumerate() {
        let field = &indices[index * 2];
        let hash = little(&field[..5]);
        if little(&field[5..]) != u64::from(id) {
            return Err("tablebase: field ID is not its index ordinal");
        }
        let pair = &indices[index * 2 + 1];
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
        evidence.push((id, hash, start, end));
    }
    let record_demands = evidence
        .iter()
        .map(|(_, _, start, end)| QualificationDemand {
            role: 2,
            offset: *start,
            length: (*end - *start) as usize,
        })
        .collect::<Vec<_>>();
    let records = read_stage(&record_demands)?;
    for (record, (_, hash, _, _)) in records.iter().zip(&evidence) {
        if record.len() < 12
            || record[..5]
                .iter()
                .fold(0_u64, |n, b| n * 256 + u64::from(*b))
                != *hash
        {
            return Err("tablebase: graph source bitmap mismatch");
        }
        let mut cursor = 5;
        let mut cumulative_degree = 0;
        for _ in 0..7 {
            let degree = usize::from(
                *record
                    .get(cursor)
                    .ok_or("tablebase: truncated graph record")?,
            );
            cursor += 1;
            cumulative_degree += degree;
            if cumulative_degree > 255 {
                return Err("tablebase: cumulative graph degree exceeds format bound");
            }
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
    }
    let evidence = evidence
        .into_iter()
        .map(|(id, hash, start, end)| json!({ "id": id, "hash": hash, "start": start, "end": end }))
        .collect::<Vec<_>>();
    let profiles = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"].into_iter().map(|profile| {
        if profile != "jstris-180" { return json!({ "profile": profile, "upstream_complete": false, "status": "unavailable", "reason": "missing-profile-specific-index" }); }
        json!({ "profile": profile, "upstream_complete": true, "status": "ready",
            "reader_contract": "hydra-jstris-180-complete-graph-v1", "field_count": count,
            "target_width": 3, "target_lines": [4], "pc_search_target_lines": [],
            "setup_search_target_lines": [], "target_qualification_receipts": [],
            "terminal_id": count - 1,
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
