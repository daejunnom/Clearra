//! Local-only, resumable PC4 graph qualification evidence.
//!
//! This tool never activates a profile and never writes a product manifest.
//! `outgoing-shard` compares immutable graph adjacency with Clearra's exact
//! forward reachability for one bounded source-ID interval. `merge-outgoing`
//! accepts only an exact, non-overlapping cover and re-hashes every artifact.
//! Its output deliberately remains indexed-domain parity evidence: reachable
//! targets outside the upstream field index still require an independent dead
//! proof before an outgoing-edge completeness identity can be minted.

mod domain;
mod indexed_path;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_executor::enumerate_pc4_ilc_target_fields;
use clearra_pc4_tablebase::{
    clearra_board64_mask_to_hydra_field_hash_v1, decode_hydra_graph_record_v1,
    hydra_field_hash_v1_to_clearra_board64_mask, GraphTargetEncoding, Pc4GraphPiece,
};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

const SHARD_SCHEMA: &str = "clearra.pc4.indexed-domain-outgoing-shard.v1";
const MERGED_SCHEMA: &str = "clearra.pc4.indexed-domain-outgoing-merge.v1";
const ACTIVE_SCHEMA: &str = "clearra.pc4.benchmark-files.v1";
const GENERATION_SCHEMA: &str = "clearra.pc4.host-generation.v1";
const HEADER_BYTES: usize = 16;
const FIELD_RECORD_BYTES: usize = 8;
const MAX_SHARD_FIELDS: u32 = 262_144;
const IO_BUFFER_BYTES: usize = 1024 * 1024;
const MAX_MISMATCH_SAMPLES: usize = 16;

fn main() {
    if let Err(error) = run() {
        eprintln!("pc4_qualifier_error={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let command = arguments
        .next()
        .ok_or("expected a qualification command")?
        .into_string()
        .map_err(|_| "command must be UTF-8")?;
    let options = parse_options(arguments.collect())?;
    match command.as_str() {
        "outgoing-shard" => run_outgoing_shard(&options),
        "merge-outgoing" => run_merge(&options),
        "domain-seed" => run_domain_seed(&options),
        "domain-step" => run_domain_step(&options),
        "domain-run" => run_domain_run(&options),
        "domain-compare" => run_domain_compare(&options),
        "indexed-path-proof" => run_indexed_path_proof(&options),
        _ => Err(
            "expected outgoing-shard, merge-outgoing, domain-seed, domain-step, domain-run, domain-compare, or indexed-path-proof".to_owned(),
        ),
    }
}

fn run_indexed_path_proof(options: &BTreeMap<String, String>) -> Result<(), String> {
    let dataset_root = absolute_option(options, "dataset-root")?;
    let profile = required_option(options, "profile")?;
    let output = absolute_option(options, "output")?;
    let dataset = Dataset::open(&dataset_root, profile)?;
    indexed_path::prove(&dataset, &output)
}

fn run_domain_seed(options: &BTreeMap<String, String>) -> Result<(), String> {
    let dataset_root = absolute_option(options, "dataset-root")?;
    let profile = required_option(options, "profile")?;
    let direction = domain::DomainDirection::parse(required_option(options, "direction")?)?;
    let output = absolute_option(options, "output")?;
    let dataset = Dataset::open(&dataset_root, profile)?;
    let report = domain::seed(dataset.domain_binding()?, direction, &output)?;
    println!(
        "pc4_domain_seed={} direction={} layer={} fields={} identity={}",
        report.disposition,
        direction.as_str(),
        report.layer,
        report.field_count,
        report.file_identity
    );
    Ok(())
}

fn run_domain_step(options: &BTreeMap<String, String>) -> Result<(), String> {
    let dataset_root = absolute_option(options, "dataset-root")?;
    let profile = required_option(options, "profile")?;
    let direction = domain::DomainDirection::parse(required_option(options, "direction")?)?;
    let input = absolute_option(options, "input")?;
    let output = absolute_option(options, "output")?;
    let filter = options.get("filter").map(PathBuf::from);
    if filter.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err("--filter must be absolute".to_owned());
    }
    let workers = usize::try_from(numeric_option(options, "workers")?)
        .map_err(|_| "worker count overflow")?;
    let dataset = Dataset::open(&dataset_root, profile)?;
    let report = domain::step(
        dataset.domain_binding()?,
        direction,
        &input,
        filter.as_deref(),
        &output,
        workers,
    )?;
    print_domain_step(direction, &report);
    Ok(())
}

fn print_domain_step(direction: domain::DomainDirection, report: &domain::StepReport) {
    println!(
        "pc4_domain_step={} direction={} input_layer={} output_layer={} input_fields={} output_fields={} candidate_pairs={} workers={} identity={}",
        report.disposition,
        direction.as_str(),
        report.input_layer,
        report.output_layer,
        report.input_field_count,
        report.output_field_count,
        report.candidate_pair_count,
        report.workers,
        report.file_identity
    );
}

fn run_domain_run(options: &BTreeMap<String, String>) -> Result<(), String> {
    let dataset_root = absolute_option(options, "dataset-root")?;
    let profile = required_option(options, "profile")?;
    let direction = domain::DomainDirection::parse(required_option(options, "direction")?)?;
    let layers = absolute_option(options, "layers")?;
    require_real_directory(&layers)?;
    let workers = usize::try_from(numeric_option(options, "workers")?)
        .map_err(|_| "worker count overflow")?;
    let max_new_steps = usize::try_from(numeric_option(options, "max-new-steps")?)
        .map_err(|_| "step count overflow")?;
    if max_new_steps == 0 || max_new_steps > 10 {
        return Err("max-new-steps outside 1..=10".to_owned());
    }
    let dataset = Dataset::open(&dataset_root, profile)?;
    let binding = dataset.domain_binding()?;
    let seed_layer = match direction {
        domain::DomainDirection::Reverse => 10,
        domain::DomainDirection::Forward => 0,
    };
    let seed_path = layers.join(format!("{}-layer-{seed_layer:02}.bin", direction.as_str()));
    let seed = domain::seed(binding, direction, &seed_path)?;
    println!(
        "pc4_domain_seed={} direction={} layer={} fields={} identity={}",
        seed.disposition,
        direction.as_str(),
        seed.layer,
        seed.field_count,
        seed.file_identity
    );

    let transitions = match direction {
        domain::DomainDirection::Reverse => (1_u8..=10)
            .rev()
            .map(|input_layer| (input_layer, input_layer - 1))
            .collect::<Vec<_>>(),
        domain::DomainDirection::Forward => (0_u8..10)
            .map(|input_layer| (input_layer, input_layer + 1))
            .collect::<Vec<_>>(),
    };
    let mut created = 0_usize;
    for (input_layer, output_layer) in transitions {
        let input = layers.join(format!("{}-layer-{input_layer:02}.bin", direction.as_str()));
        let output = layers.join(format!(
            "{}-layer-{output_layer:02}.bin",
            direction.as_str()
        ));
        let filter = match direction {
            domain::DomainDirection::Reverse => None,
            domain::DomainDirection::Forward => {
                Some(layers.join(format!("reverse-layer-{output_layer:02}.bin")))
            }
        };
        let report = domain::step(
            binding,
            direction,
            &input,
            filter.as_deref(),
            &output,
            workers,
        )?;
        print_domain_step(direction, &report);
        if report.disposition == "created" {
            created += 1;
            if created == max_new_steps {
                break;
            }
        }
    }
    println!(
        "pc4_domain_run=complete direction={} new_steps={created} max_new_steps={max_new_steps}",
        direction.as_str()
    );
    Ok(())
}

fn run_domain_compare(options: &BTreeMap<String, String>) -> Result<(), String> {
    let dataset_root = absolute_option(options, "dataset-root")?;
    let profile = required_option(options, "profile")?;
    let layers = absolute_option(options, "layers")?;
    let output = absolute_option(options, "output")?;
    require_real_directory(&layers)?;
    let dataset = Dataset::open(&dataset_root, profile)?;
    let binding = dataset.domain_binding()?;
    let mut reverse_receipts = Vec::new();
    let mut reverse_digests = BTreeMap::new();
    for layer in (0_u8..=10).rev() {
        let path = layers.join(format!("reverse-layer-{layer:02}.bin"));
        let file = domain::read(&path, binding, Some(layer))?;
        if layer == 10 {
            if file.derivation != domain::DomainDerivation::ReverseSeed
                || file.input_digest != [0; 32]
                || file.filter_digest != [0; 32]
            {
                return Err("reverse domain seed provenance invalid".to_owned());
            }
        } else if file.derivation != domain::DomainDerivation::ReverseStep
            || file.input_digest != reverse_digests[&(layer + 1)]
            || file.filter_digest != [0; 32]
        {
            return Err("reverse domain chain provenance invalid".to_owned());
        }
        reverse_receipts.push(json!({
            "layer": layer,
            "field_count": file.fields.len(),
            "file_identity": file.file_identity,
        }));
        reverse_digests.insert(layer, file.file_digest);
    }

    let mut layer_receipts = Vec::new();
    let mut generated = Vec::new();
    let mut prior_forward_digest = None;
    for layer in 0_u8..=10 {
        let path = layers.join(format!("forward-layer-{layer:02}.bin"));
        let file = domain::read(&path, binding, Some(layer))?;
        if layer == 0 {
            if file.derivation != domain::DomainDerivation::ForwardSeed
                || file.input_digest != [0; 32]
                || file.filter_digest != [0; 32]
            {
                return Err("forward domain seed provenance invalid".to_owned());
            }
        } else if file.derivation != domain::DomainDerivation::ForwardStep
            || file.input_digest != prior_forward_digest.expect("forward predecessor exists")
            || file.filter_digest != reverse_digests[&layer]
        {
            return Err("forward domain chain provenance invalid".to_owned());
        }
        generated
            .try_reserve(file.fields.len())
            .map_err(|_| "domain comparison allocation failed")?;
        generated.extend_from_slice(&file.fields);
        layer_receipts.push(json!({
            "layer": layer,
            "field_count": file.fields.len(),
            "file_identity": file.file_identity,
        }));
        prior_forward_digest = Some(file.file_digest);
    }
    generated.sort_unstable();
    let before_dedup = generated.len();
    generated.dedup();
    if generated.len() != before_dedup {
        return Err("forward domain layers overlap".to_owned());
    }

    let field_index = fs::read(&dataset.fields.path).map_err(io_error)?;
    validate_index_bytes(
        &field_index,
        b"FHIDIDX1",
        dataset.field_count,
        dataset.fields.byte_len,
    )?;
    let field_identity = sha256_identity(&field_index);
    require_identity(&field_identity, &dataset.fields.identity, "field index")?;
    if generated.len()
        != usize::try_from(dataset.field_count).map_err(|_| "field count overflow")?
    {
        return Err("forward domain and field index counts differ".to_owned());
    }
    for (field_id, generated_hash) in generated.iter().copied().enumerate() {
        let indexed_hash = field_hash(
            &field_index,
            u32::try_from(field_id).map_err(|_| "field ID overflow")?,
        )?;
        if generated_hash != indexed_hash {
            return Err(format!("forward domain differs at field ID {field_id}"));
        }
    }
    let core = json!({
        "schema": "clearra.pc4.forward-completable-domain-parity.v1",
        "authority": "non-target-qualification-evidence",
        "qualification_status": "domain-parity-only",
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "kick_profile": dataset.kick_profile.as_str(),
        "field_count": dataset.field_count,
        "field_index": dataset.fields.public(),
        "observed_field_index_identity": field_identity,
        "domain_binding_identity": binding.identity_string(),
        "reverse_layers": reverse_receipts,
        "forward_layers": layer_receipts,
        "outgoing_edge_completeness_identity": Value::Null,
        "offline_exact_parity_identity": Value::Null,
    });
    let receipt = with_identity(core)?;
    write_json_atomic(&output, &receipt)?;
    println!(
        "pc4_domain_compare=domain-parity-only fields={} receipt={}",
        dataset.field_count,
        receipt["receipt_identity"].as_str().unwrap_or("invalid")
    );
    Ok(())
}

fn run_outgoing_shard(options: &BTreeMap<String, String>) -> Result<(), String> {
    let dataset_root = absolute_option(options, "dataset-root")?;
    let profile = required_option(options, "profile")?;
    let start = numeric_option(options, "start")?;
    let end = numeric_option(options, "end")?;
    let output = absolute_option(options, "output")?;
    let dataset = Dataset::open(&dataset_root, profile)?;
    if start >= end || end > dataset.field_count || end - start > MAX_SHARD_FIELDS {
        return Err("invalid bounded shard range".to_owned());
    }
    if output.exists() {
        let existing = read_json(&output, 16 * 1024 * 1024)?;
        validate_existing_shard(&existing, &dataset, start, end)?;
        println!(
            "pc4_outgoing_shard=already-complete start={start} end={end} receipt={}",
            existing["receipt_identity"].as_str().unwrap_or("invalid")
        );
        return Ok(());
    }

    let field_index = fs::read(&dataset.fields.path).map_err(io_error)?;
    validate_index_bytes(
        &field_index,
        b"FHIDIDX1",
        dataset.field_count,
        dataset.fields.byte_len,
    )?;
    let field_identity = sha256_identity(&field_index);
    require_identity(&field_identity, &dataset.fields.identity, "field index")?;

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

    let graph_start = graph_offset(&offsets, start)?;
    let graph_end = graph_offset(&offsets, end)?;
    if graph_start >= graph_end || graph_end > dataset.graph.byte_len {
        return Err("graph shard bounds are invalid".to_owned());
    }
    let graph_bytes = read_exact_range(&dataset.graph.path, graph_start, graph_end - graph_start)?;
    let graph_segment_identity = sha256_identity(&graph_bytes);

    let mut metrics = ShardMetrics::default();
    let mut mismatch_samples = Vec::new();
    for source_id in start..end {
        let source_hash = field_hash(&field_index, source_id)?;
        let source_cells = hydra_field_hash_v1_to_clearra_board64_mask(source_hash)
            .map_err(|error| error.reason().to_owned())?;
        if !source_cells.count_ones().is_multiple_of(4) {
            return Err("source field area is not tetromino aligned".to_owned());
        }
        let source_layer =
            usize::try_from(source_cells.count_ones() / 4).map_err(|_| "source layer overflow")?;
        let layer_count = metrics
            .source_area_layers
            .get_mut(source_layer)
            .ok_or("source layer outside four-row domain")?;
        *layer_count = layer_count.checked_add(1).ok_or("metric overflow")?;
        let record_start = graph_offset(&offsets, source_id)?;
        let record_end = graph_offset(&offsets, source_id + 1)?;
        let local_start = usize::try_from(record_start - graph_start)
            .map_err(|_| "graph shard offset overflow")?;
        let local_end =
            usize::try_from(record_end - graph_start).map_err(|_| "graph shard offset overflow")?;
        let record = graph_bytes
            .get(local_start..local_end)
            .ok_or("graph record outside shard bytes")?;
        let decoded = decode_hydra_graph_record_v1(
            record,
            source_hash,
            dataset.target_encoding,
            dataset.field_count,
        )
        .map_err(|error| error.reason().to_owned())?;

        for (graph_piece, piece) in PIECES {
            let targets =
                enumerate_pc4_ilc_target_fields(source_cells, piece, dataset.kick_profile)
                    .map_err(|error| error.reason().to_owned())?;
            metrics.forward_target_fields = metrics
                .forward_target_fields
                .checked_add(targets.len() as u64)
                .ok_or("metric overflow")?;
            let mut expected = BTreeSet::new();
            for cells in targets {
                let hash = clearra_board64_mask_to_hydra_field_hash_v1(cells)
                    .map_err(|error| error.reason().to_owned())?;
                if let Some(target_id) = lookup_field_id(&field_index, dataset.field_count, hash)? {
                    expected.insert(target_id);
                } else {
                    metrics.outside_index_reachable_targets = metrics
                        .outside_index_reachable_targets
                        .checked_add(1)
                        .ok_or("metric overflow")?;
                }
            }
            metrics.expected_indexed_edges = metrics
                .expected_indexed_edges
                .checked_add(expected.len() as u64)
                .ok_or("metric overflow")?;

            let encoded = decoded.targets(graph_piece);
            metrics.graph_edges = metrics
                .graph_edges
                .checked_add(encoded.len() as u64)
                .ok_or("metric overflow")?;
            let actual = encoded.iter().copied().collect::<BTreeSet<_>>();
            metrics.duplicate_graph_edges = metrics
                .duplicate_graph_edges
                .checked_add((encoded.len() - actual.len()) as u64)
                .ok_or("metric overflow")?;
            let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
            let extra = actual.difference(&expected).copied().collect::<Vec<_>>();
            if !missing.is_empty() || !extra.is_empty() || encoded.len() != actual.len() {
                metrics.mismatched_piece_records = metrics
                    .mismatched_piece_records
                    .checked_add(1)
                    .ok_or("metric overflow")?;
                metrics.missing_indexed_edges = metrics
                    .missing_indexed_edges
                    .checked_add(missing.len() as u64)
                    .ok_or("metric overflow")?;
                metrics.extra_graph_edges = metrics
                    .extra_graph_edges
                    .checked_add(extra.len() as u64)
                    .ok_or("metric overflow")?;
                if mismatch_samples.len() < MAX_MISMATCH_SAMPLES {
                    mismatch_samples.push(json!({
                        "source_id": source_id,
                        "source_hash": source_hash,
                        "piece": graph_piece_name(graph_piece),
                        "missing_target_ids": missing,
                        "extra_target_ids": extra,
                        "encoded_degree": encoded.len(),
                        "unique_degree": actual.len(),
                    }));
                }
            }
        }
        metrics.source_records += 1;
    }

    let passed = metrics.mismatched_piece_records == 0 && metrics.duplicate_graph_edges == 0;
    let core = json!({
        "schema": SHARD_SCHEMA,
        "authority": "non-target-qualification-evidence",
        "evidence_scope": "exact-forward-parity-with-indexed-target-domain-only",
        "qualification_status": if passed { "indexed-domain-parity-only" } else { "failed" },
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "kick_profile": dataset.kick_profile.as_str(),
        "reader_contract": dataset.reader_contract,
        "field_count": dataset.field_count,
        "target_width": dataset.target_encoding.byte_width(),
        "artifacts": dataset.public_artifacts(),
        "source_range": { "start": start, "end": end },
        "graph_byte_range": { "start": graph_start, "end": graph_end },
        "observed_identities": {
            "fields": field_identity,
            "offsets": offsets_identity,
            "graph_segment": graph_segment_identity,
        },
        "metrics": metrics.as_json(),
        "mismatch_samples": mismatch_samples,
        "missing_semantic_proofs": {
            "outside_index_target_dead_proof": metrics.outside_index_reachable_targets,
            "offline_exact_result_family_parity": true,
        },
    });
    let receipt = with_identity(core)?;
    write_json_atomic(&output, &receipt)?;
    println!(
        "pc4_outgoing_shard={} start={} end={} indexed_edges={} outside_index={} receipt={}",
        if passed { "passed" } else { "failed" },
        start,
        end,
        metrics.expected_indexed_edges,
        metrics.outside_index_reachable_targets,
        receipt["receipt_identity"].as_str().unwrap_or("invalid")
    );
    if passed {
        Ok(())
    } else {
        Err("indexed-domain outgoing parity mismatch".to_owned())
    }
}

fn run_merge(options: &BTreeMap<String, String>) -> Result<(), String> {
    let dataset_root = absolute_option(options, "dataset-root")?;
    let profile = required_option(options, "profile")?;
    let receipts_directory = absolute_option(options, "receipts")?;
    let output = absolute_option(options, "output")?;
    let dataset = Dataset::open(&dataset_root, profile)?;
    if !receipts_directory.is_dir()
        || fs::symlink_metadata(&receipts_directory)
            .map_err(io_error)?
            .file_type()
            .is_symlink()
    {
        return Err("receipts directory must be a real directory".to_owned());
    }

    let mut receipts = Vec::new();
    for entry in fs::read_dir(&receipts_directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if entry.file_type().map_err(io_error)?.is_symlink() {
            return Err("receipt symlink rejected".to_owned());
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let value = read_json(&entry.path(), 16 * 1024 * 1024)?;
        validate_receipt_identity(&value)?;
        validate_receipt_dataset_binding(&value, &dataset)?;
        receipts.push(value);
    }
    receipts.sort_by_key(|receipt| {
        receipt["source_range"]["start"]
            .as_u64()
            .unwrap_or(u64::MAX)
    });
    if receipts.is_empty() {
        return Err("no shard receipts found".to_owned());
    }

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

    let mut next_source = 0_u64;
    let mut next_graph_byte = 0_u64;
    let mut totals = ShardMetrics::default();
    let mut receipt_identities = Vec::with_capacity(receipts.len());
    let mut graph = File::open(&dataset.graph.path).map_err(io_error)?;
    let mut graph_digest = Sha256::new();
    for receipt in &receipts {
        if receipt["qualification_status"] != "indexed-domain-parity-only" {
            return Err("failed or unqualified shard receipt rejected".to_owned());
        }
        let start = receipt["source_range"]["start"]
            .as_u64()
            .ok_or("invalid source start")?;
        let end = receipt["source_range"]["end"]
            .as_u64()
            .ok_or("invalid source end")?;
        let graph_start = receipt["graph_byte_range"]["start"]
            .as_u64()
            .ok_or("invalid graph start")?;
        let graph_end = receipt["graph_byte_range"]["end"]
            .as_u64()
            .ok_or("invalid graph end")?;
        if start != next_source
            || graph_start != next_graph_byte
            || end <= start
            || graph_end <= graph_start
        {
            return Err("shard receipts do not form one exact non-overlapping cover".to_owned());
        }
        let start_id = u32::try_from(start).map_err(|_| "source start overflow")?;
        let end_id = u32::try_from(end).map_err(|_| "source end overflow")?;
        if graph_start != graph_offset(&offsets, start_id)?
            || graph_end != graph_offset(&offsets, end_id)?
        {
            return Err("shard graph byte range is not bound to its source range".to_owned());
        }
        let segment_identity = hash_open_range(
            &mut graph,
            graph_start,
            graph_end - graph_start,
            Some(&mut graph_digest),
        )?;
        if receipt["observed_identities"]["graph_segment"].as_str() != Some(&segment_identity) {
            return Err("graph segment identity drift".to_owned());
        }
        totals.add_json(&receipt["metrics"])?;
        receipt_identities.push(
            receipt["receipt_identity"]
                .as_str()
                .ok_or("missing shard receipt identity")?
                .to_owned(),
        );
        next_source = end;
        next_graph_byte = graph_end;
    }
    if next_source != u64::from(dataset.field_count) || next_graph_byte != dataset.graph.byte_len {
        return Err("shard receipts do not cover the complete graph domain".to_owned());
    }
    let graph_identity = format!("sha256:{}", hex_digest(graph_digest.finalize().as_slice()));
    require_identity(&graph_identity, &dataset.graph.identity, "graph")?;
    let fields_identity = hash_file(&dataset.fields.path)?;
    require_identity(&fields_identity, &dataset.fields.identity, "field index")?;
    if totals.mismatched_piece_records != 0 || totals.duplicate_graph_edges != 0 {
        return Err("merged shard metrics contain parity failures".to_owned());
    }
    if totals.source_records != u64::from(dataset.field_count)
        || totals.source_area_layers.iter().sum::<u64>() != u64::from(dataset.field_count)
    {
        return Err("merged source metrics do not cover the field domain".to_owned());
    }

    let core = json!({
        "schema": MERGED_SCHEMA,
        "authority": "non-target-qualification-evidence",
        "evidence_scope": "complete-indexed-domain-forward-adjacency-parity",
        "qualification_status": "not-qualified",
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "kick_profile": dataset.kick_profile.as_str(),
        "reader_contract": dataset.reader_contract,
        "field_count": dataset.field_count,
        "target_width": dataset.target_encoding.byte_width(),
        "artifacts": dataset.public_artifacts(),
        "observed_identities": {
            "fields": fields_identity,
            "offsets": offsets_identity,
            "graph": graph_identity,
        },
        "exact_source_cover": { "start": 0, "end": dataset.field_count },
        "shard_count": receipts.len(),
        "shard_receipt_identities": receipt_identities,
        "metrics": totals.as_json(),
        "indexed_domain_adjacency_identity_scope":
            "all exact forward locks whose normalized targets are present in the immutable field index",
        "outgoing_edge_completeness_identity": Value::Null,
        "offline_exact_parity_identity": Value::Null,
        "missing_semantic_proofs": {
            "outside_index_target_dead_proof": totals.outside_index_reachable_targets,
            "offline_exact_result_family_parity": true,
        },
    });
    let receipt = with_identity(core)?;
    write_json_atomic(&output, &receipt)?;
    println!(
        "pc4_outgoing_merge=indexed-domain-parity-only shards={} outside_index={} receipt={}",
        receipts.len(),
        totals.outside_index_reachable_targets,
        receipt["receipt_identity"].as_str().unwrap_or("invalid")
    );
    Ok(())
}

#[derive(Default)]
struct ShardMetrics {
    source_records: u64,
    source_area_layers: [u64; 11],
    forward_target_fields: u64,
    expected_indexed_edges: u64,
    outside_index_reachable_targets: u64,
    graph_edges: u64,
    duplicate_graph_edges: u64,
    mismatched_piece_records: u64,
    missing_indexed_edges: u64,
    extra_graph_edges: u64,
}

impl ShardMetrics {
    fn as_json(&self) -> Value {
        json!({
            "source_records": self.source_records,
            "source_area_layers": self.source_area_layers,
            "forward_target_fields": self.forward_target_fields,
            "expected_indexed_edges": self.expected_indexed_edges,
            "outside_index_reachable_targets": self.outside_index_reachable_targets,
            "graph_edges": self.graph_edges,
            "duplicate_graph_edges": self.duplicate_graph_edges,
            "mismatched_piece_records": self.mismatched_piece_records,
            "missing_indexed_edges": self.missing_indexed_edges,
            "extra_graph_edges": self.extra_graph_edges,
        })
    }

    fn add_json(&mut self, value: &Value) -> Result<(), String> {
        macro_rules! add {
            ($field:ident) => {
                self.$field = self
                    .$field
                    .checked_add(
                        value[stringify!($field)]
                            .as_u64()
                            .ok_or(concat!("invalid metric ", stringify!($field)))?,
                    )
                    .ok_or("merged metric overflow")?;
            };
        }
        add!(source_records);
        let layers = value["source_area_layers"]
            .as_array()
            .ok_or("invalid metric source_area_layers")?;
        if layers.len() != self.source_area_layers.len() {
            return Err("invalid metric source_area_layers".to_owned());
        }
        for (total, layer) in self.source_area_layers.iter_mut().zip(layers) {
            *total = total
                .checked_add(layer.as_u64().ok_or("invalid source area layer")?)
                .ok_or("merged metric overflow")?;
        }
        add!(forward_target_fields);
        add!(expected_indexed_edges);
        add!(outside_index_reachable_targets);
        add!(graph_edges);
        add!(duplicate_graph_edges);
        add!(mismatched_piece_records);
        add!(missing_indexed_edges);
        add!(extra_graph_edges);
        Ok(())
    }
}

struct Dataset {
    repository: String,
    revision: String,
    profile: String,
    reader_contract: String,
    field_count: u32,
    target_encoding: GraphTargetEncoding,
    kick_profile: KickTableProfileId,
    fields: Artifact,
    offsets: Artifact,
    graph: Artifact,
}

struct Artifact {
    path: PathBuf,
    logical_path: String,
    byte_len: u64,
    identity: String,
}

impl Dataset {
    fn open(base: &Path, requested_profile: &str) -> Result<Self, String> {
        require_real_absolute_directory(base)?;
        let profile = checked_name(requested_profile, "profile")?;
        let profile_root = base.join(profile);
        require_real_directory(&profile_root)?;
        let active_path = profile_root.join("active.json");
        require_regular_file(&active_path)?;
        let active = read_json(&active_path, 128 * 1024)?;
        if active["schema"] != ACTIVE_SCHEMA {
            return Err("unsupported benchmark pointer schema".to_owned());
        }
        let generation = active
            .get("generation")
            .and_then(Value::as_object)
            .ok_or("benchmark generation missing")?;
        if generation.get("schema").and_then(Value::as_str) != Some(GENERATION_SCHEMA) {
            return Err("unsupported host generation schema".to_owned());
        }
        let directory_name = checked_name(
            active["directory"]
                .as_str()
                .ok_or("generation directory missing")?,
            "generation directory",
        )?;
        if !directory_name.starts_with("gen-") {
            return Err("generation directory is not managed".to_owned());
        }
        let data_root = profile_root.join(directory_name);
        require_real_directory(&data_root)?;
        let slots = generation
            .get("profiles")
            .and_then(Value::as_array)
            .ok_or("generation profiles missing")?;
        let matching = slots
            .iter()
            .filter(|slot| slot["profile"].as_str() == Some(profile))
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err("profile slot must be unique".to_owned());
        }
        let slot = matching[0];
        if slot["status"] != "ready" || slot["upstream_complete"] != true {
            return Err("profile slot is not structurally ready".to_owned());
        }
        let field_count_u64 = slot["field_count"].as_u64().ok_or("field count missing")?;
        let field_count = u32::try_from(field_count_u64).map_err(|_| "field count overflow")?;
        if field_count < 2 || field_count > (1 << 24) {
            return Err("field count outside supported domain".to_owned());
        }
        let target_width = slot["target_width"]
            .as_u64()
            .ok_or("target width missing")?;
        let target_encoding = match target_width {
            3 => GraphTargetEncoding::U24LittleEndian,
            4 => GraphTargetEncoding::U32LittleEndian,
            _ => return Err("unsupported target width".to_owned()),
        };
        let kick_profile =
            KickTableProfileId::parse(profile).ok_or("profile has no exact built-in kick table")?;
        let artifacts = slot
            .get("artifacts")
            .and_then(Value::as_object)
            .ok_or("profile artifacts missing")?;
        let fields = artifact(
            &data_root,
            artifacts,
            "fields",
            "field_hash_to_id",
            "field-hash-index",
            16 + u64::from(field_count) * 8,
        )?;
        let offsets = artifact(
            &data_root,
            artifacts,
            "offsets",
            "graph_offsets",
            "graph-offsets",
            16 + (u64::from(field_count) + 1) * 4,
        )?;
        let graph = artifact(
            &data_root,
            artifacts,
            "graph",
            "graph",
            "graph-candidate",
            0,
        )?;
        if fields.logical_path == offsets.logical_path
            || fields.logical_path == graph.logical_path
            || offsets.logical_path == graph.logical_path
        {
            return Err("profile artifact paths must be distinct".to_owned());
        }
        Ok(Self {
            repository: generation
                .get("repository")
                .and_then(Value::as_str)
                .ok_or("repository missing")?
                .to_owned(),
            revision: checked_revision(
                generation
                    .get("revision")
                    .and_then(Value::as_str)
                    .ok_or("revision missing")?,
            )?,
            profile: profile.to_owned(),
            reader_contract: slot["reader_contract"]
                .as_str()
                .ok_or("reader contract missing")?
                .to_owned(),
            field_count,
            target_encoding,
            kick_profile,
            fields,
            offsets,
            graph,
        })
    }

    fn public_artifacts(&self) -> Value {
        json!({
            "fields": self.fields.public(),
            "offsets": self.offsets.public(),
            "graph": self.graph.public(),
        })
    }

    fn domain_binding(&self) -> Result<domain::DomainBinding, String> {
        let value = json!({
            "schema": "clearra.pc4.domain-binding.v1",
            "repository": self.repository,
            "revision": self.revision,
            "profile": self.profile,
            "kick_profile": self.kick_profile.as_str(),
            "field_count": self.field_count,
            "artifacts": self.public_artifacts(),
        });
        let canonical = canonical_json(&value)?;
        Ok(domain::DomainBinding::new(
            Sha256::digest(canonical.as_bytes()).into(),
            self.kick_profile,
        ))
    }
}

impl Artifact {
    fn public(&self) -> Value {
        json!({
            "path": self.logical_path,
            "byte_length": self.byte_len,
            "content_identity": self.identity,
        })
    }
}

fn artifact(
    data_root: &Path,
    artifacts: &Map<String, Value>,
    key: &str,
    prefix: &str,
    expected_role: &str,
    expected_len: u64,
) -> Result<Artifact, String> {
    let value = artifacts
        .get(key)
        .and_then(Value::as_object)
        .ok_or("artifact descriptor missing")?;
    let logical_path = checked_name(
        value
            .get("path")
            .and_then(Value::as_str)
            .ok_or("artifact path missing")?,
        "artifact path",
    )?;
    if !logical_path.starts_with(prefix) || !logical_path.ends_with(".bin") {
        return Err("artifact path does not match its role".to_owned());
    }
    if value.get("role").and_then(Value::as_str) != Some(expected_role) {
        return Err("artifact role mismatch".to_owned());
    }
    let byte_len = value
        .get("byte_length")
        .and_then(Value::as_u64)
        .ok_or("artifact length missing")?;
    if byte_len == 0 || (expected_len != 0 && byte_len != expected_len) {
        return Err("artifact length mismatch".to_owned());
    }
    let identity = checked_identity(
        value
            .get("content_identity")
            .and_then(Value::as_str)
            .ok_or("artifact identity missing")?,
    )?;
    let path = data_root.join(logical_path);
    require_regular_file(&path)?;
    if fs::metadata(&path).map_err(io_error)?.len() != byte_len {
        return Err("artifact file size mismatch".to_owned());
    }
    Ok(Artifact {
        path,
        logical_path: logical_path.to_owned(),
        byte_len,
        identity,
    })
}

fn validate_existing_shard(
    receipt: &Value,
    dataset: &Dataset,
    start: u32,
    end: u32,
) -> Result<(), String> {
    validate_receipt_identity(receipt)?;
    validate_receipt_dataset_binding(receipt, dataset)?;
    if receipt["schema"] != SHARD_SCHEMA
        || receipt["qualification_status"] != "indexed-domain-parity-only"
        || receipt["source_range"]["start"].as_u64() != Some(u64::from(start))
        || receipt["source_range"]["end"].as_u64() != Some(u64::from(end))
    {
        return Err("existing shard receipt does not match requested work".to_owned());
    }
    let mut metrics = ShardMetrics::default();
    metrics.add_json(&receipt["metrics"])?;
    if metrics.source_records != u64::from(end - start)
        || metrics.source_area_layers.iter().sum::<u64>() != u64::from(end - start)
    {
        return Err("existing shard receipt source metrics mismatch".to_owned());
    }
    Ok(())
}

fn validate_receipt_dataset_binding(receipt: &Value, dataset: &Dataset) -> Result<(), String> {
    if receipt["schema"] != SHARD_SCHEMA
        || receipt["repository"].as_str() != Some(&dataset.repository)
        || receipt["revision"].as_str() != Some(&dataset.revision)
        || receipt["profile"].as_str() != Some(&dataset.profile)
        || receipt["field_count"].as_u64() != Some(u64::from(dataset.field_count))
        || receipt["artifacts"] != dataset.public_artifacts()
    {
        return Err("shard receipt dataset binding mismatch".to_owned());
    }
    Ok(())
}

fn validate_receipt_identity(receipt: &Value) -> Result<(), String> {
    let object = receipt.as_object().ok_or("receipt must be an object")?;
    let identity = object
        .get("receipt_identity")
        .and_then(Value::as_str)
        .ok_or("receipt identity missing")?;
    checked_identity(identity)?;
    let mut core = object.clone();
    core.remove("receipt_identity");
    if sha256_identity(canonical_json(&Value::Object(core))?.as_bytes()) != identity {
        return Err("receipt identity mismatch".to_owned());
    }
    Ok(())
}

fn with_identity(core: Value) -> Result<Value, String> {
    let mut object = core
        .as_object()
        .ok_or("receipt core must be an object")?
        .clone();
    let identity = sha256_identity(canonical_json(&Value::Object(object.clone()))?.as_bytes());
    object.insert("receipt_identity".to_owned(), Value::String(identity));
    Ok(Value::Object(object))
}

fn parse_options(arguments: Vec<OsString>) -> Result<BTreeMap<String, String>, String> {
    if !arguments.len().is_multiple_of(2) {
        return Err("options must be --name value pairs".to_owned());
    }
    let mut options = BTreeMap::new();
    for pair in arguments.chunks_exact(2) {
        let key = pair[0]
            .to_str()
            .ok_or("option name must be UTF-8")?
            .strip_prefix("--")
            .ok_or("option name must begin with --")?;
        let value = pair[1].to_str().ok_or("option value must be UTF-8")?;
        if key.is_empty() || options.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err("duplicate or empty option".to_owned());
        }
    }
    Ok(options)
}

fn required_option<'a>(
    options: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str, String> {
    options
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{name}"))
}

fn absolute_option(options: &BTreeMap<String, String>, name: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(required_option(options, name)?);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(format!("--{name} must be an absolute non-parent path"));
    }
    Ok(path)
}

fn numeric_option(options: &BTreeMap<String, String>, name: &str) -> Result<u32, String> {
    required_option(options, name)?
        .parse::<u32>()
        .map_err(|_| format!("--{name} must be a u32"))
}

fn checked_name<'a>(value: &'a str, kind: &str) -> Result<&'a str, String> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("invalid {kind}"));
    }
    Ok(value)
}

fn checked_revision(value: &str) -> Result<String, String> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("revision must be a full hexadecimal Git object ID".to_owned());
    }
    Ok(value.to_ascii_lowercase())
}

fn checked_identity(value: &str) -> Result<String, String> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err("identity must be sha256".to_owned());
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("identity must contain 64 hexadecimal digits".to_owned());
    }
    Ok(format!("sha256:{}", hex.to_ascii_lowercase()))
}

fn require_real_absolute_directory(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || path.parent().is_none() {
        return Err("dataset root must be an absolute dedicated directory".to_owned());
    }
    require_real_directory(path)
}

fn require_real_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("directory symlink or non-directory rejected".to_owned());
    }
    Ok(())
}

fn require_regular_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("file symlink or non-file rejected".to_owned());
    }
    Ok(())
}

fn read_json(path: &Path, maximum_bytes: u64) -> Result<Value, String> {
    require_regular_file(path)?;
    let metadata = fs::metadata(path).map_err(io_error)?;
    if metadata.len() > maximum_bytes {
        return Err("JSON input exceeds its bound".to_owned());
    }
    serde_json::from_slice(&fs::read(path).map_err(io_error)?).map_err(|error| error.to_string())
}

fn validate_index_bytes(
    bytes: &[u8],
    magic: &[u8; 8],
    field_count: u32,
    expected_len: u64,
) -> Result<(), String> {
    if bytes.len() as u64 != expected_len
        || bytes.get(..8) != Some(magic.as_slice())
        || bytes.get(8..12) != Some(1_u32.to_le_bytes().as_slice())
        || bytes.get(12..16) != Some(field_count.to_le_bytes().as_slice())
    {
        return Err("index header or length mismatch".to_owned());
    }
    Ok(())
}

fn field_hash(index: &[u8], field_id: u32) -> Result<u64, String> {
    let offset = HEADER_BYTES
        .checked_add(
            usize::try_from(field_id).map_err(|_| "field ID overflow")? * FIELD_RECORD_BYTES,
        )
        .ok_or("field index offset overflow")?;
    let record = index
        .get(offset..offset + FIELD_RECORD_BYTES)
        .ok_or("field index record missing")?;
    let encoded_id = read_little(&record[5..8]);
    if encoded_id != u64::from(field_id) {
        return Err("field index ordinal mismatch".to_owned());
    }
    Ok(read_little(&record[..5]))
}

fn lookup_field_id(
    index: &[u8],
    field_count: u32,
    target_hash: u64,
) -> Result<Option<u32>, String> {
    let mut low = 0_u32;
    let mut high = field_count;
    while low < high {
        let middle = low + (high - low) / 2;
        let found = field_hash(index, middle)?;
        if found < target_hash {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    if low < field_count && field_hash(index, low)? == target_hash {
        Ok(Some(low))
    } else {
        Ok(None)
    }
}

fn graph_offset(offsets: &[u8], field_id: u32) -> Result<u64, String> {
    let offset = HEADER_BYTES
        .checked_add(usize::try_from(field_id).map_err(|_| "offset ID overflow")? * 4)
        .ok_or("offset index overflow")?;
    let bytes = offsets
        .get(offset..offset + 4)
        .ok_or("graph offset missing")?;
    Ok(u64::from(u32::from_le_bytes(
        bytes.try_into().map_err(|_| "graph offset width")?,
    )))
}

fn read_little(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .rev()
        .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte))
}

fn read_exact_range(path: &Path, offset: u64, length: u64) -> Result<Vec<u8>, String> {
    let length = usize::try_from(length).map_err(|_| "range length exceeds address space")?;
    let mut bytes = vec![0_u8; length];
    let mut file = File::open(path).map_err(io_error)?;
    file.seek(SeekFrom::Start(offset)).map_err(io_error)?;
    file.read_exact(&mut bytes).map_err(io_error)?;
    Ok(bytes)
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(io_error)?;
    let length = file.metadata().map_err(io_error)?.len();
    hash_open_range(&mut file, 0, length, None)
}

fn hash_open_range(
    file: &mut File,
    offset: u64,
    length: u64,
    mut whole_digest: Option<&mut Sha256>,
) -> Result<String, String> {
    file.seek(SeekFrom::Start(offset)).map_err(io_error)?;
    let mut remaining = length;
    let mut segment = Sha256::new();
    let mut buffer = vec![0_u8; IO_BUFFER_BYTES];
    while remaining != 0 {
        let count = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| "hash chunk overflow")?;
        file.read_exact(&mut buffer[..count]).map_err(io_error)?;
        segment.update(&buffer[..count]);
        if let Some(digest) = whole_digest.as_deref_mut() {
            digest.update(&buffer[..count]);
        }
        remaining -= count as u64;
    }
    Ok(format!(
        "sha256:{}",
        hex_digest(segment.finalize().as_slice())
    ))
}

fn sha256_identity(bytes: &[u8]) -> String {
    format!("sha256:{}", hex_digest(Sha256::digest(bytes).as_slice()))
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 15)]));
    }
    output
}

fn require_identity(actual: &str, expected: &str, artifact: &str) -> Result<(), String> {
    if actual != expected {
        Err(format!("{artifact} identity mismatch"))
    } else {
        Ok(())
    }
}

fn canonical_json(value: &Value) -> Result<String, String> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            serde_json::to_string(value).map_err(|error| error.to_string())
        }
        Value::Array(values) => {
            let encoded = values
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", encoded.join(",")))
        }
        Value::Object(values) => {
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let encoded = keys
                .into_iter()
                .map(|key| {
                    Ok(format!(
                        "{}:{}",
                        serde_json::to_string(key).map_err(|error| error.to_string())?,
                        canonical_json(&values[key])?
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(format!("{{{}}}", encoded.join(",")))
        }
    }
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    if path.exists() {
        return Err("refusing to overwrite an existing receipt".to_owned());
    }
    let parent = path.parent().ok_or("receipt output has no parent")?;
    require_real_directory(parent)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("receipt output name must be UTF-8")?;
    let pending = parent.join(format!(".{name}.pending-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(io_error)?;
    let result = (|| {
        serde_json::to_writer_pretty(&mut file, value).map_err(|error| error.to_string())?;
        file.write_all(b"\n").map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        drop(file);
        fs::rename(&pending, path).map_err(io_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

fn graph_piece_name(piece: Pc4GraphPiece) -> &'static str {
    match piece {
        Pc4GraphPiece::I => "I",
        Pc4GraphPiece::J => "J",
        Pc4GraphPiece::L => "L",
        Pc4GraphPiece::O => "O",
        Pc4GraphPiece::S => "S",
        Pc4GraphPiece::T => "T",
        Pc4GraphPiece::Z => "Z",
    }
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

const PIECES: [(Pc4GraphPiece, PieceKind); 7] = [
    (Pc4GraphPiece::I, PieceKind::I),
    (Pc4GraphPiece::J, PieceKind::J),
    (Pc4GraphPiece::L, PieceKind::L),
    (Pc4GraphPiece::O, PieceKind::O),
    (Pc4GraphPiece::S, PieceKind::S),
    (Pc4GraphPiece::T, PieceKind::T),
    (Pc4GraphPiece::Z, PieceKind::Z),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_receipt_identity_is_key_order_independent() {
        let left = json!({ "z": 2, "a": { "y": 1, "b": [3, 4] } });
        let right = json!({ "a": { "b": [3, 4], "y": 1 }, "z": 2 });
        assert_eq!(
            canonical_json(&left).unwrap(),
            canonical_json(&right).unwrap()
        );
        assert_eq!(
            sha256_identity(canonical_json(&left).unwrap().as_bytes()),
            sha256_identity(canonical_json(&right).unwrap().as_bytes())
        );
    }

    #[test]
    fn option_parser_rejects_duplicate_and_unpaired_values() {
        assert!(parse_options(vec![OsString::from("--a")]).is_err());
        assert!(parse_options(vec![
            OsString::from("--a"),
            OsString::from("1"),
            OsString::from("--a"),
            OsString::from("2"),
        ])
        .is_err());
    }

    #[test]
    fn field_index_binary_search_uses_u40_hash_and_u24_ordinal() {
        let mut bytes = b"FHIDIDX1".to_vec();
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        for (id, hash) in [0_u64, 15, (1_u64 << 40) - 1].into_iter().enumerate() {
            bytes.extend_from_slice(&hash.to_le_bytes()[..5]);
            bytes.extend_from_slice(&(id as u32).to_le_bytes()[..3]);
        }
        assert_eq!(lookup_field_id(&bytes, 3, 15).unwrap(), Some(1));
        assert_eq!(lookup_field_id(&bytes, 3, 16).unwrap(), None);
        assert_eq!(field_hash(&bytes, 2).unwrap(), (1_u64 << 40) - 1);
    }
}
