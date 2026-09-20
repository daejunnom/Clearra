use super::{
    decode_hydra_graph_record_v1, field_hash, graph_offset,
    hydra_field_hash_v1_to_clearra_board64_mask, lookup_field_id, require_identity,
    sha256_identity, validate_index_bytes, with_identity, write_json_atomic, Dataset, PIECES,
};
use serde_json::{json, Value};
use std::{fs, path::Path};

const MAX_SAMPLES: usize = 16;

pub(crate) fn prove(dataset: &Dataset, output: &Path) -> Result<(), String> {
    let fields = fs::read(&dataset.fields.path).map_err(io_error)?;
    validate_index_bytes(
        &fields,
        b"FHIDIDX1",
        dataset.field_count,
        dataset.fields.byte_len,
    )?;
    let fields_identity = sha256_identity(&fields);
    require_identity(&fields_identity, &dataset.fields.identity, "field index")?;

    let offsets = fs::read(&dataset.offsets.path).map_err(io_error)?;
    validate_index_bytes(
        &offsets,
        b"GOFFIDX1",
        dataset.field_count,
        dataset.offsets.byte_len,
    )?;
    let offsets_identity = sha256_identity(&offsets);
    require_identity(
        &offsets_identity,
        &dataset.offsets.identity,
        "graph offsets",
    )?;

    let graph = fs::read(&dataset.graph.path).map_err(io_error)?;
    if u64::try_from(graph.len()).map_err(|_| "graph size overflow")? != dataset.graph.byte_len {
        return Err("graph length differs from its generation manifest".to_owned());
    }
    let graph_identity = sha256_identity(&graph);
    require_identity(&graph_identity, &dataset.graph.identity, "graph")?;

    let count = usize::try_from(dataset.field_count).map_err(|_| "field count overflow")?;
    let mut layer_ids = (0..=10).map(|_| Vec::new()).collect::<Vec<Vec<u32>>>();
    for field_id in 0..dataset.field_count {
        let hash = field_hash(&fields, field_id)?;
        let cells = hydra_field_hash_v1_to_clearra_board64_mask(hash)
            .map_err(|error| error.reason().to_owned())?;
        if !cells.count_ones().is_multiple_of(4) {
            return Err(format!(
                "field {field_id} is outside a tetromino area layer"
            ));
        }
        let layer = usize::try_from(cells.count_ones() / 4).map_err(|_| "field layer overflow")?;
        layer_ids
            .get_mut(layer)
            .ok_or("field outside four-row path domain")?
            .push(field_id);
    }

    let root_id = lookup_field_id(&fields, dataset.field_count, 0)?
        .ok_or("empty root is absent from field index")?;
    let terminal_hash = (1_u64 << 40) - 1;
    let terminal_id = lookup_field_id(&fields, dataset.field_count, terminal_hash)?
        .ok_or("four-row terminal is absent from field index")?;
    if layer_ids[0].as_slice() != [root_id] || layer_ids[10].as_slice() != [terminal_id] {
        return Err("root or terminal layer is not canonical singleton".to_owned());
    }

    let mut reachable = vec![false; count];
    reachable[usize::try_from(root_id).map_err(|_| "root ID overflow")?] = true;
    let mut edge_count = 0_u64;
    for layer in 0_usize..10 {
        for &source_id in &layer_ids[layer] {
            let source_hash = field_hash(&fields, source_id)?;
            let record = graph_record(&graph, &offsets, source_id)?;
            let decoded = decode_hydra_graph_record_v1(
                record,
                source_hash,
                dataset.target_encoding,
                dataset.field_count,
            )
            .map_err(|error| error.reason().to_owned())?;
            let source_reachable = reachable[source_id as usize];
            for (graph_piece, _) in PIECES {
                for &target_id in decoded.targets(graph_piece) {
                    edge_count = edge_count.checked_add(1).ok_or("edge count overflow")?;
                    if !layer_ids[layer + 1].binary_search(&target_id).is_ok() {
                        return Err(format!(
                            "graph edge does not advance one area layer: {source_id}->{target_id}"
                        ));
                    }
                    if source_reachable {
                        reachable[target_id as usize] = true;
                    }
                }
            }
        }
    }

    let mut completable = vec![false; count];
    completable[terminal_id as usize] = true;
    for layer in (0_usize..10).rev() {
        for &source_id in &layer_ids[layer] {
            let source_hash = field_hash(&fields, source_id)?;
            let record = graph_record(&graph, &offsets, source_id)?;
            let decoded = decode_hydra_graph_record_v1(
                record,
                source_hash,
                dataset.target_encoding,
                dataset.field_count,
            )
            .map_err(|error| error.reason().to_owned())?;
            let has_terminal_path = PIECES.iter().any(|(graph_piece, _)| {
                decoded
                    .targets(*graph_piece)
                    .iter()
                    .any(|target_id| completable[*target_id as usize])
            });
            completable[source_id as usize] = has_terminal_path;
        }
    }

    let mut unreachable_count = 0_u64;
    let mut dead_count = 0_u64;
    let mut unreachable_samples = Vec::new();
    let mut dead_samples = Vec::new();
    for field_id in 0..dataset.field_count {
        let index = field_id as usize;
        if !reachable[index] {
            unreachable_count += 1;
            if unreachable_samples.len() < MAX_SAMPLES {
                unreachable_samples.push(field_id);
            }
        }
        if !completable[index] {
            dead_count += 1;
            if dead_samples.len() < MAX_SAMPLES {
                dead_samples.push(field_id);
            }
        }
    }
    let passed = unreachable_count == 0 && dead_count == 0;
    let layer_counts = layer_ids.iter().map(Vec::len).collect::<Vec<_>>();
    let core = json!({
        "schema": "clearra.pc4.indexed-path-domain-proof.v1",
        "authority": "non-target-qualification-evidence",
        "qualification_status": if passed { "indexed-path-domain-only" } else { "failed" },
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "kick_profile": dataset.kick_profile.as_str(),
        "reader_contract": dataset.reader_contract,
        "field_count": dataset.field_count,
        "artifacts": dataset.public_artifacts(),
        "observed_identities": {
            "fields": fields_identity,
            "offsets": offsets_identity,
            "graph": graph_identity,
        },
        "root_id": root_id,
        "terminal_id": terminal_id,
        "layer_counts": layer_counts,
        "graph_edges": edge_count,
        "unreachable_indexed_fields": unreachable_count,
        "terminal_dead_indexed_fields": dead_count,
        "unreachable_samples": unreachable_samples,
        "terminal_dead_samples": dead_samples,
        "outgoing_edge_completeness_identity": Value::Null,
        "offline_exact_parity_identity": Value::Null,
    });
    let receipt = with_identity(core)?;
    write_json_atomic(output, &receipt)?;
    println!(
        "pc4_indexed_path_proof={} fields={} edges={} receipt={}",
        if passed { "passed" } else { "failed" },
        dataset.field_count,
        edge_count,
        receipt["receipt_identity"].as_str().unwrap_or("invalid")
    );
    if passed {
        Ok(())
    } else {
        Err("indexed graph contains unreachable or terminal-dead fields".to_owned())
    }
}

fn graph_record<'a>(graph: &'a [u8], offsets: &[u8], source_id: u32) -> Result<&'a [u8], String> {
    let start = usize::try_from(graph_offset(offsets, source_id)?)
        .map_err(|_| "graph record start overflow")?;
    let end = usize::try_from(graph_offset(offsets, source_id + 1)?)
        .map_err(|_| "graph record end overflow")?;
    graph
        .get(start..end)
        .ok_or_else(|| "graph record outside artifact bytes".to_owned())
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}
