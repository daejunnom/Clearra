//! Exact, generation-bound checkpoint for the expensive offline result family.
//!
//! The JSON proof alone intentionally contains only count and a display hash.
//! Reusing the family for differential qualification therefore requires this
//! separate fixed-width artifact, its SHA-256 receipt, and a full decode that
//! recomputes canonical ordering and the normalized family hash.

use super::{
    hash_file, offline_family, read_json, require_real_directory, require_regular_file,
    validate_receipt_identity, with_identity, write_json_atomic, Dataset,
};
use clearra_core_domain::solution::normalized_tiling_solution::{
    normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
    StandardBoard64TilingIdentity,
};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
    time::Instant,
};

const SCHEMA: &str = "clearra.pc4.offline-exact-family-materialization.v1";
const MAGIC: &[u8; 8] = b"PC4FAM01";
const VERSION: u32 = 1;
const HEADER_BYTES: u64 = 40;
const PLACEMENTS_PER_RECORD: usize = 10;
const RECORD_BYTES: u32 = 8 + PLACEMENTS_PER_RECORD as u32 * 8;

#[allow(clippy::too_many_arguments)]
pub(crate) fn materialize(
    dataset: &Dataset,
    workers: usize,
    expected_count: usize,
    offline_path: &Path,
    family_output: &Path,
    receipt_output: &Path,
) -> Result<(), String> {
    if family_output == receipt_output {
        return Err("family artifact and receipt outputs must differ".to_owned());
    }
    let offline = read_json(offline_path, 16 * 1024 * 1024)?;
    validate_offline_receipt(&offline, dataset, expected_count)?;

    match (family_output.exists(), receipt_output.exists()) {
        (true, true) => {
            let loaded = load(
                dataset,
                expected_count,
                &offline,
                family_output,
                receipt_output,
            )?;
            println!(
                "pc4_offline_family_materialization=already-complete solutions={} receipt={}",
                loaded.count(),
                read_json(receipt_output, 16 * 1024 * 1024)?["receipt_identity"]
                    .as_str()
                    .unwrap_or("invalid")
            );
            return Ok(());
        }
        (false, false) => {}
        _ => {
            return Err(
                "offline family materialization artifact/receipt pair is incomplete".to_owned(),
            );
        }
    }

    let started = Instant::now();
    let exact = offline_family::execute(workers)?;
    validate_exact_against_offline(&exact, &offline, expected_count)?;
    write_family_atomic(family_output, &exact.identities)?;
    let artifact_identity = match hash_file(family_output) {
        Ok(identity) => identity,
        Err(error) => {
            let _ = fs::remove_file(family_output);
            return Err(error);
        }
    };
    let byte_length = fs::metadata(family_output).map_err(io_error)?.len();
    let file_name = family_output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("family artifact name must be UTF-8")?;
    let core = json!({
        "schema": SCHEMA,
        "authority": "non-target-qualification-evidence",
        "qualification_status": "offline-exact-family-materialized",
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "kick_profile": dataset.kick_profile.as_str(),
        "reader_contract": dataset.reader_contract,
        "artifacts": dataset.public_artifacts(),
        "input_identity": offline_family::INPUT_IDENTITY,
        "offline_family_receipt_identity": offline["receipt_identity"],
        "unique_solution_count": exact.count(),
        "normalized_solution_set_hash_algorithm": exact.normalized_hash_algorithm,
        "normalized_solution_set_hash": exact.normalized_hash,
        "identity_order": "strict-canonical-ascending",
        "materialization": {
            "format": "PC4FAM01",
            "version": VERSION,
            "file_name": file_name,
            "byte_length": byte_length,
            "content_identity": artifact_identity,
            "record_bytes": RECORD_BYTES,
            "placements_per_record": PLACEMENTS_PER_RECORD,
            "initial_board_mask": 0,
        },
    });
    let receipt = with_identity(core)?;
    if let Err(error) = write_json_atomic(receipt_output, &receipt) {
        let _ = fs::remove_file(family_output);
        return Err(error);
    }
    println!(
        "pc4_offline_family_materialization=passed solutions={} elapsed_ms={} artifact={} receipt={}",
        exact.count(),
        started.elapsed().as_millis(),
        receipt["materialization"]["content_identity"]
            .as_str()
            .unwrap_or("invalid"),
        receipt["receipt_identity"].as_str().unwrap_or("invalid")
    );
    Ok(())
}

pub(crate) fn load(
    dataset: &Dataset,
    expected_count: usize,
    offline: &Value,
    family_path: &Path,
    receipt_path: &Path,
) -> Result<offline_family::ExactFamily, String> {
    validate_offline_receipt(offline, dataset, expected_count)?;
    let receipt = read_json(receipt_path, 16 * 1024 * 1024)?;
    validate_receipt_identity(&receipt)?;
    if receipt["schema"] != SCHEMA
        || receipt["qualification_status"] != "offline-exact-family-materialized"
        || receipt["repository"].as_str() != Some(&dataset.repository)
        || receipt["revision"].as_str() != Some(&dataset.revision)
        || receipt["profile"].as_str() != Some(&dataset.profile)
        || receipt["artifacts"] != dataset.public_artifacts()
        || receipt["input_identity"] != offline_family::INPUT_IDENTITY
        || receipt["offline_family_receipt_identity"] != offline["receipt_identity"]
        || receipt["unique_solution_count"].as_u64() != u64::try_from(expected_count).ok()
        || receipt["normalized_solution_set_hash"] != offline["normalized_solution_set_hash"]
        || receipt["normalized_solution_set_hash_algorithm"]
            != offline["normalized_solution_set_hash_algorithm"]
        || receipt["materialization"]["format"] != "PC4FAM01"
        || receipt["materialization"]["version"].as_u64() != Some(u64::from(VERSION))
        || receipt["materialization"]["record_bytes"].as_u64() != Some(u64::from(RECORD_BYTES))
        || receipt["materialization"]["placements_per_record"].as_u64()
            != Some(PLACEMENTS_PER_RECORD as u64)
        || receipt["materialization"]["initial_board_mask"].as_u64() != Some(0)
    {
        return Err("offline family materialization receipt mismatch".to_owned());
    }
    require_regular_file(family_path)?;
    if receipt["materialization"]["file_name"].as_str()
        != family_path.file_name().and_then(|value| value.to_str())
    {
        return Err("offline family materialization file name mismatch".to_owned());
    }
    let byte_length = fs::metadata(family_path).map_err(io_error)?.len();
    if receipt["materialization"]["byte_length"].as_u64() != Some(byte_length)
        || receipt["materialization"]["content_identity"].as_str()
            != Some(hash_file(family_path)?.as_str())
    {
        return Err("offline family materialization identity mismatch".to_owned());
    }
    let identities = read_family(family_path, expected_count)?;
    if identities.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("offline materialized family is not one strict canonical set".to_owned());
    }
    let normalized_hash =
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(&identities);
    if offline["normalized_solution_set_hash"].as_str() != Some(&normalized_hash)
        || receipt["normalized_solution_set_hash"].as_str() != Some(&normalized_hash)
    {
        return Err("offline materialized family normalized hash mismatch".to_owned());
    }
    Ok(offline_family::ExactFamily {
        identities,
        normalized_hash,
        normalized_hash_algorithm: receipt["normalized_solution_set_hash_algorithm"]
            .as_str()
            .ok_or("materialization hash algorithm missing")?
            .to_owned(),
    })
}

fn validate_offline_receipt(
    offline: &Value,
    dataset: &Dataset,
    expected_count: usize,
) -> Result<(), String> {
    validate_receipt_identity(offline)?;
    if offline["schema"] != offline_family::SCHEMA
        || offline["qualification_status"] != "offline-exact-family-complete"
        || offline["repository"].as_str() != Some(&dataset.repository)
        || offline["revision"].as_str() != Some(&dataset.revision)
        || offline["profile"].as_str() != Some(&dataset.profile)
        || offline["artifacts"] != dataset.public_artifacts()
        || offline["input_identity"] != offline_family::INPUT_IDENTITY
        || offline["expected_unique_solution_count"].as_u64() != u64::try_from(expected_count).ok()
        || offline["unique_solution_count"].as_u64() != u64::try_from(expected_count).ok()
    {
        return Err("offline exact family proof does not match materialization inputs".to_owned());
    }
    Ok(())
}

fn validate_exact_against_offline(
    exact: &offline_family::ExactFamily,
    offline: &Value,
    expected_count: usize,
) -> Result<(), String> {
    if exact.count() != expected_count
        || offline["normalized_solution_set_hash"].as_str() != Some(exact.normalized_hash.as_str())
        || offline["normalized_solution_set_hash_algorithm"].as_str()
            != Some(exact.normalized_hash_algorithm.as_str())
    {
        return Err("fresh exact family differs from the offline proof".to_owned());
    }
    Ok(())
}

fn write_family_atomic(
    path: &Path,
    identities: &[StandardBoard64TilingIdentity],
) -> Result<(), String> {
    if path.exists() {
        return Err("refusing to overwrite an existing family artifact".to_owned());
    }
    let parent = path.parent().ok_or("family output has no parent")?;
    require_real_directory(parent)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("family output name must be UTF-8")?;
    let pending = parent.join(format!(".{name}.pending-{}", std::process::id()));
    let result = (|| {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(io_error)?;
        let mut output = BufWriter::new(file);
        output.write_all(MAGIC).map_err(io_error)?;
        output.write_all(&VERSION.to_le_bytes()).map_err(io_error)?;
        output
            .write_all(&RECORD_BYTES.to_le_bytes())
            .map_err(io_error)?;
        output
            .write_all(&(identities.len() as u64).to_le_bytes())
            .map_err(io_error)?;
        output.write_all(&0_u64.to_le_bytes()).map_err(io_error)?;
        output
            .write_all(&(PLACEMENTS_PER_RECORD as u32).to_le_bytes())
            .map_err(io_error)?;
        output.write_all(&0_u32.to_le_bytes()).map_err(io_error)?;
        for identity in identities {
            if identity.initial_board_mask() != 0
                || identity.placement_count() != PLACEMENTS_PER_RECORD
            {
                return Err("offline family identity is outside fixed PC4 format".to_owned());
            }
            output
                .write_all(&identity.packed_piece_codes().to_le_bytes())
                .map_err(io_error)?;
            for &mask in identity.placement_masks() {
                output.write_all(&mask.to_le_bytes()).map_err(io_error)?;
            }
        }
        output.flush().map_err(io_error)?;
        let file = output
            .into_inner()
            .map_err(|error| io_error(error.into_error()))?;
        file.sync_all().map_err(io_error)?;
        drop(file);
        fs::rename(&pending, path).map_err(io_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

fn read_family(
    path: &Path,
    expected_count: usize,
) -> Result<Vec<StandardBoard64TilingIdentity>, String> {
    let expected_length = HEADER_BYTES
        .checked_add(
            u64::try_from(expected_count)
                .map_err(|_| "family count overflow")?
                .checked_mul(u64::from(RECORD_BYTES))
                .ok_or("family length overflow")?,
        )
        .ok_or("family length overflow")?;
    if fs::metadata(path).map_err(io_error)?.len() != expected_length {
        return Err("offline family materialization length mismatch".to_owned());
    }
    let mut input = BufReader::new(File::open(path).map_err(io_error)?);
    let mut header = [0_u8; HEADER_BYTES as usize];
    input.read_exact(&mut header).map_err(io_error)?;
    if &header[..8] != MAGIC
        || read_u32(&header[8..12])? != VERSION
        || read_u32(&header[12..16])? != RECORD_BYTES
        || read_u64(&header[16..24])? != expected_count as u64
        || read_u64(&header[24..32])? != 0
        || read_u32(&header[32..36])? != PLACEMENTS_PER_RECORD as u32
        || read_u32(&header[36..40])? != 0
    {
        return Err("offline family materialization header mismatch".to_owned());
    }
    let mut identities = Vec::new();
    identities
        .try_reserve_exact(expected_count)
        .map_err(|_| "offline family identity allocation failed")?;
    let mut record = [0_u8; RECORD_BYTES as usize];
    for _ in 0..expected_count {
        input.read_exact(&mut record).map_err(io_error)?;
        let packed_piece_codes = read_u64(&record[..8])?;
        let mut masks = [0_u64; PLACEMENTS_PER_RECORD];
        for (index, mask) in masks.iter_mut().enumerate() {
            let offset = 8 + index * 8;
            *mask = read_u64(&record[offset..offset + 8])?;
        }
        identities.push(
            StandardBoard64TilingIdentity::from_compact_parts(0, packed_piece_codes, &masks)
                .map_err(|_| "offline family materialization record invalid")?,
        );
    }
    Ok(identities)
}

fn read_u32(bytes: &[u8]) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| "invalid u32 width")?,
    ))
}

fn read_u64(bytes: &[u8]) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes.try_into().map_err(|_| "invalid u64 width")?,
    ))
}

fn io_error(error: std::io::Error) -> String {
    format!("I/O error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::{
        piece::piece_kind::PieceKind, solution::normalized_tiling_solution::PiecePlacementMask,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn exact_family_codec_round_trips_strict_canonical_identities() {
        let first = identity(false);
        let second = identity(true);
        let mut identities = vec![first, second];
        identities.sort_unstable();
        let path = std::env::temp_dir().join(format!(
            "clearra-pc4-family-{}-{}.bin",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        write_family_atomic(&path, &identities).unwrap();
        let decoded = read_family(&path, identities.len()).unwrap();
        assert_eq!(decoded, identities);
        fs::remove_file(path).unwrap();
    }

    fn identity(reverse: bool) -> StandardBoard64TilingIdentity {
        let pieces = PieceKind::STANDARD_TETROMINOES;
        let placements = (0..PLACEMENTS_PER_RECORD).map(|index| {
            let slot = if reverse {
                PLACEMENTS_PER_RECORD - 1 - index
            } else {
                index
            };
            PiecePlacementMask::new(pieces[index % pieces.len()], 0xf_u64 << (slot * 4))
        });
        StandardBoard64TilingIdentity::from_placements(0, placements).unwrap()
    }
}
