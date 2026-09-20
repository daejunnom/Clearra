//! Final PC-search target receipt assembly.
//!
//! The receipt is accepted only after the complete indexed source cover, the
//! exact outside-boundary dead proof, and end-to-end tablebase/offline family
//! parity all bind the same immutable generation.

use super::{
    field_hash, lookup_field_id, read_json, sha256_identity, validate_index_bytes,
    validate_receipt_identity, with_identity, write_json_atomic, Dataset,
};
use serde_json::{json, Value};
use std::{fs, path::Path};

const SCHEMA: &str = "clearra.pc4.exact-target-qualification.v1";
const OUTGOING_SCHEMA: &str = "clearra.pc4.indexed-domain-outgoing-proof-merge.v2";
const BOUNDARY_SCHEMA: &str = "clearra.pc4.outside-boundary-dead-proof-shard.v3";
const PARITY_SCHEMA: &str = "clearra.pc4.tablebase-offline-exact-family-parity.v1";
const TERMINAL_SEMANTICS: &str = "clearra.pc4.full-bottom-rows-after-clear.v1";

pub(crate) fn qualify(
    dataset: &Dataset,
    outgoing_path: &Path,
    boundary_path: &Path,
    parity_path: &Path,
    output: &Path,
) -> Result<(), String> {
    let outgoing = read_json(outgoing_path, 16 * 1024 * 1024)?;
    let boundary = read_json(boundary_path, 16 * 1024 * 1024)?;
    let parity = read_json(parity_path, 16 * 1024 * 1024)?;
    for receipt in [&outgoing, &boundary, &parity] {
        validate_receipt_identity(receipt)?;
        validate_generation(receipt, dataset)?;
    }
    if outgoing["schema"] != OUTGOING_SCHEMA
        || outgoing["qualification_status"] != "indexed-path-and-boundary-unclassified"
        || outgoing["exact_source_cover"]["start"].as_u64() != Some(0)
        || outgoing["exact_source_cover"]["end"].as_u64() != Some(u64::from(dataset.field_count))
    {
        return Err("outgoing proof is not one complete source cover".to_owned());
    }
    if boundary["schema"] != BOUNDARY_SCHEMA
        || boundary["qualification_status"] != "outside-boundary-terminal-dead"
        || boundary["outside_boundary"]["content_identity"]
            != outgoing["outside_boundary"]["content_identity"]
        || boundary["outside_boundary"]["field_count"]
            != outgoing["outside_boundary"]["field_count"]
    {
        return Err("boundary dead proof does not close the outgoing proof".to_owned());
    }
    if parity["schema"] != PARITY_SCHEMA
        || parity["qualification_status"] != "tablebase-offline-exact-family-parity"
        || parity["outgoing_proof_receipt_identity"] != outgoing["receipt_identity"]
        || parity["boundary_dead_proof_receipt_identity"] != boundary["receipt_identity"]
    {
        return Err("family parity proof is not bound to the graph completeness proofs".to_owned());
    }
    let known_answer_identity = parity["offline_family_receipt_identity"]
        .as_str()
        .ok_or("family parity proof lacks offline family identity")?;
    exact_identity(known_answer_identity)?;
    let parity_identity = parity["receipt_identity"]
        .as_str()
        .ok_or("family parity receipt identity missing")?;
    exact_identity(parity_identity)?;

    let fields = fs::read(&dataset.fields.path).map_err(io_error)?;
    validate_index_bytes(
        &fields,
        b"FHIDIDX1",
        dataset.field_count,
        dataset.fields.byte_len,
    )?;
    if sha256_identity(&fields) != dataset.fields.identity {
        return Err("field index identity drift".to_owned());
    }
    let terminal_hash = (1_u64 << 40) - 1;
    let terminal_id = lookup_field_id(&fields, dataset.field_count, terminal_hash)?
        .ok_or("four-row terminal is absent from field index")?;
    if field_hash(&fields, terminal_id)? != terminal_hash
        || terminal_id.checked_add(1) != Some(dataset.field_count)
    {
        return Err("terminal field is not the canonical final record".to_owned());
    }

    let outgoing_statement = with_identity(json!({
        "schema": "clearra.pc4.outgoing-edge-completeness.v1",
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "reader_contract": dataset.reader_contract,
        "artifacts": dataset.public_artifacts(),
        "indexed_path_receipt_identity": outgoing["indexed_path_receipt_identity"],
        "outgoing_proof_receipt_identity": outgoing["receipt_identity"],
        "boundary_dead_proof_receipt_identity": boundary["receipt_identity"],
        "source_start": 0,
        "source_end": dataset.field_count,
        "outside_boundary_field_count": outgoing["outside_boundary"]["field_count"],
        "outside_boundary_identity": outgoing["outside_boundary"]["content_identity"],
    }))?;
    let outgoing_identity = outgoing_statement["receipt_identity"]
        .as_str()
        .ok_or("outgoing completeness identity missing")?;

    let core = json!({
        "schema": SCHEMA,
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "reader_contract": dataset.reader_contract,
        "use_case": "pc-search",
        "target_lines": 4,
        "terminal_id": terminal_id,
        "terminal_hash": terminal_hash,
        "terminal_semantics_identity": TERMINAL_SEMANTICS,
        "outgoing_edge_completeness_identity": outgoing_identity,
        "known_answer_identity": known_answer_identity,
        "offline_exact_parity_identity": parity_identity,
        "evidence": {
            "outgoing_statement": outgoing_statement,
            "family_unique_solution_count": parity["unique_solution_count"],
            "family_normalized_solution_set_hash": parity["normalized_solution_set_hash"],
        },
    });
    let receipt = with_identity(core)?;
    if output.exists() {
        let existing = read_json(output, 16 * 1024 * 1024)?;
        validate_receipt_identity(&existing)?;
        if existing != receipt {
            return Err("existing target qualification receipt differs".to_owned());
        }
        println!(
            "pc4_target_qualification=already-complete receipt={}",
            receipt["receipt_identity"].as_str().unwrap_or("invalid")
        );
        return Ok(());
    }
    write_json_atomic(output, &receipt)?;
    println!(
        "pc4_target_qualification=passed outgoing={} parity={} receipt={}",
        outgoing_identity,
        parity_identity,
        receipt["receipt_identity"].as_str().unwrap_or("invalid")
    );
    Ok(())
}

fn validate_generation(receipt: &Value, dataset: &Dataset) -> Result<(), String> {
    if receipt["repository"].as_str() != Some(&dataset.repository)
        || receipt["revision"].as_str() != Some(&dataset.revision)
        || receipt["profile"].as_str() != Some(&dataset.profile)
        || receipt["artifacts"] != dataset.public_artifacts()
    {
        return Err("qualification receipt generation mismatch".to_owned());
    }
    Ok(())
}

fn exact_identity(value: &str) -> Result<(), String> {
    if value.len() != 71
        || !value.starts_with("sha256:")
        || !value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || value[7..].bytes().all(|byte| byte == b'0')
    {
        return Err("qualification evidence identity invalid".to_owned());
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> String {
    format!("I/O error: {error}")
}
