use super::{
    boundary, clearra_board64_mask_to_hydra_field_hash_v1, domain, enumerate_pc4_ilc_target_fields,
    field_hash, hydra_field_hash_v1_to_clearra_board64_mask, lookup_field_id, read_json,
    require_identity, require_real_directory, sha256_identity, validate_index_bytes,
    validate_indexed_path_receipt, validate_receipt_identity, with_identity, write_json_atomic,
    Dataset, PIECES,
};
use super::{
    boundary_completion::{CompletionNecessaryOracle, CompletionNecessity},
    boundary_dead_store::{RunFile, Workspace},
};
use serde_json::{json, Value};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    fs,
    path::Path,
    thread,
    time::Instant,
};

const SCHEMA: &str = "clearra.pc4.outside-boundary-dead-proof-shard.v3";
const PROOF_ALGORITHM: &str =
    "exact-layer-frontier-external-sort-with-projected-cover-negative-proof-v3";
const TERMINAL_FIELD: u64 = (1_u64 << 40) - 1;
#[allow(dead_code)]
const SORT_RUN_FIELDS: usize = 1_048_576;

pub(crate) fn prove(
    dataset: &Dataset,
    boundary_path: &Path,
    reverse_layers_directory: &Path,
    anchor_layer: u8,
    indexed_path_receipt_path: &Path,
    requested_workers: usize,
    workspace_path: Option<&Path>,
    output: &Path,
) -> Result<(), String> {
    if !(1..=9).contains(&anchor_layer) {
        return Err("boundary dead-proof anchor layer must be within 1..=9".to_owned());
    }
    if requested_workers == 0 || requested_workers > 64 {
        return Err("boundary dead-proof worker count outside 1..=64".to_owned());
    }
    require_real_directory(reverse_layers_directory)?;
    let binding = dataset.domain_binding()?;
    let reverse = load_reverse_chain(binding, reverse_layers_directory, anchor_layer)?;
    let anchor_receipts = reverse
        .iter()
        .rev()
        .map(|(layer, file)| {
            json!({
                "layer": layer,
                "field_count": file.fields.len(),
                "file_identity": file.file_identity,
            })
        })
        .collect::<Vec<_>>();
    let boundary_scan = boundary::scan(boundary_path, binding.raw_identity(), None, |_| Ok(()))?;
    let indexed_path_receipt = read_json(indexed_path_receipt_path, 16 * 1024 * 1024)?;
    validate_indexed_path_receipt(&indexed_path_receipt, dataset)?;
    let indexed_path_identity = indexed_path_receipt["receipt_identity"]
        .as_str()
        .ok_or("indexed-path receipt identity missing")?
        .to_owned();

    if output.exists() {
        let existing = read_json(output, 16 * 1024 * 1024)?;
        validate_receipt_identity(&existing)?;
        if existing["schema"] != SCHEMA
            || existing["qualification_status"] != "outside-boundary-terminal-dead"
            || existing["repository"].as_str() != Some(&dataset.repository)
            || existing["revision"].as_str() != Some(&dataset.revision)
            || existing["profile"].as_str() != Some(&dataset.profile)
            || existing["artifacts"] != dataset.public_artifacts()
            || existing["outside_boundary"]["content_identity"].as_str()
                != Some(&boundary_scan.file_identity)
            || existing["indexed_path_receipt_identity"].as_str() != Some(&indexed_path_identity)
            || existing["reverse_anchor_layers"] != Value::Array(anchor_receipts)
        {
            return Err(
                "existing boundary dead-proof receipt does not match current inputs".to_owned(),
            );
        }
        println!(
            "pc4_boundary_dead_proof=already-complete fields={} receipt={}",
            boundary_scan.field_count,
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
    let workspace_root = if let Some(path) = workspace_path {
        path.to_path_buf()
    } else {
        let parent = output
            .parent()
            .ok_or("boundary dead-proof output has no parent")?;
        let name = output
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("boundary dead-proof output name must be UTF-8")?;
        parent.join(format!(".{name}.boundary-dead-work"))
    };
    let workspace_marker = json!({
        "schema": "clearra.pc4.outside-boundary-dead-workspace.v2",
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "artifacts": dataset.public_artifacts(),
        "outside_boundary_identity": boundary_scan.file_identity.clone(),
        "reverse_anchor_layer": anchor_layer,
        "reverse_anchor_layers": anchor_receipts.clone(),
        "indexed_path_receipt_identity": indexed_path_identity.clone(),
        "output": output.to_string_lossy(),
    });
    let workspace = Workspace::prepare(&workspace_root, workspace_marker)?;
    let started = Instant::now();
    let classification = classify_boundary_disk(
        dataset,
        &field_index,
        &reverse,
        anchor_layer,
        boundary_path,
        binding.raw_identity(),
        &boundary_scan,
        requested_workers,
        &workspace,
    );
    let (worker_count, metrics) = classification?;
    let elapsed_ms = started.elapsed().as_millis();
    let core = json!({
        "schema": SCHEMA,
        "authority": "non-target-qualification-evidence",
        "qualification_status": "outside-boundary-terminal-dead",
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "kick_profile": dataset.kick_profile.as_str(),
        "reader_contract": dataset.reader_contract,
        "field_count": dataset.field_count,
        "artifacts": dataset.public_artifacts(),
        "outside_boundary": {
            "file_name": boundary_path.file_name().and_then(|value| value.to_str())
                .ok_or("outside-boundary file name must be UTF-8")?,
            "source_range": { "start": boundary_scan.start, "end": boundary_scan.end },
            "field_count": boundary_scan.field_count,
            "content_identity": boundary_scan.file_identity,
        },
        "reverse_anchor_layer": anchor_layer,
        "reverse_anchor_layers": anchor_receipts,
        "indexed_path_receipt_identity": indexed_path_identity,
        "observed_identities": { "fields": field_identity },
        "proof_algorithm": PROOF_ALGORITHM,
        "metrics": metrics.as_json(),
        "outgoing_edge_completeness_identity": Value::Null,
        "offline_exact_parity_identity": Value::Null,
    });
    let receipt = with_identity(core)?;
    workspace.cleanup()?;
    write_json_atomic(output, &receipt)?;
    println!(
        "pc4_boundary_dead_proof=passed fields={} expanded_frontier={} generated_targets={} workers={} elapsed_ms={} receipt={}",
        boundary_scan.field_count,
        metrics.expanded_frontier_fields,
        metrics.generated_target_fields,
        worker_count,
        elapsed_ms,
        receipt["receipt_identity"].as_str().unwrap_or("invalid")
    );
    Ok(())
}

fn classify_boundary_disk(
    dataset: &Dataset,
    field_index: &[u8],
    reverse: &BTreeMap<u8, domain::DomainFile>,
    anchor_layer: u8,
    boundary_path: &Path,
    binding: [u8; 32],
    expected: &boundary::BoundaryScan,
    requested_workers: usize,
    workspace: &Workspace,
) -> Result<(usize, Metrics), String> {
    if requested_workers == 0 || requested_workers > 64 {
        return Err("boundary dead-proof worker count outside 1..=64".to_owned());
    }
    let available = thread::available_parallelism().map_or(1, usize::from);
    let workers = requested_workers.min(available).min(
        usize::try_from(expected.field_count)
            .unwrap_or(usize::MAX)
            .max(1),
    );
    let completion_oracle = CompletionNecessaryOracle::compile();
    let (start_layer, mut frontiers, mut metrics) =
        if let Some(checkpoint) = load_latest_checkpoint(workspace, anchor_layer)? {
            println!(
                "pc4_boundary_dead_resume=checkpoint next_layer={}",
                checkpoint.next_layer
            );
            (
                checkpoint.next_layer,
                checkpoint.frontiers,
                checkpoint.metrics,
            )
        } else {
            workspace.retain_entries(&BTreeSet::new())?;
            let (frontiers, metrics) = initialize_boundary_frontiers(
                dataset,
                field_index,
                reverse,
                anchor_layer,
                boundary_path,
                binding,
                expected,
                &completion_oracle,
                workspace,
            )?;
            publish_checkpoint(workspace, 0, &frontiers, &metrics)?;
            (0, frontiers, metrics)
        };

    for layer in start_layer..anchor_layer {
        let layer_started = Instant::now();
        let frontier = frontiers[usize::from(layer)]
            .take()
            .ok_or("boundary disk frontier missing")?;
        metrics.frontier_fields_by_layer[usize::from(layer)] = frontier.count;
        metrics.expanded_frontier_fields = metrics
            .expanded_frontier_fields
            .checked_add(frontier.count)
            .ok_or("boundary dead-proof metric overflow")?;
        if frontier.count == 0 {
            publish_checkpoint(workspace, layer + 1, &frontiers, &metrics)?;
            continue;
        }
        let (generated, layer_metrics) = expand_frontier_layer_disk(
            dataset,
            field_index,
            reverse,
            anchor_layer,
            layer,
            &frontier,
            workers.min(usize::try_from(frontier.count).unwrap_or(usize::MAX)),
            &completion_oracle,
            workspace,
        )?;
        let generated_fields = generated.count;
        let generated_targets = layer_metrics.generated_target_fields;
        let anchor_checks = layer_metrics.reverse_anchor_checks;
        metrics.add(&layer_metrics)?;
        if layer + 1 < anchor_layer {
            let seed = frontiers[usize::from(layer + 1)]
                .take()
                .ok_or("boundary next-layer seed missing")?;
            let next = workspace.merge_runs_preserving_inputs(
                &format!("frontier-layer-{:02}", layer + 1),
                layer + 1,
                vec![seed, generated],
            )?;
            frontiers[usize::from(layer + 1)] = Some(next);
        } else {
            if generated.count != 0 {
                return Err("anchor expansion retained an unclassified frontier".to_owned());
            }
        }
        publish_checkpoint(workspace, layer + 1, &frontiers, &metrics)?;
        println!(
            "pc4_boundary_dead_layer=passed layer={} frontier={} generated_targets={} next_unique={} anchor_checks={} elapsed_ms={}",
            layer,
            frontier.count,
            generated_targets,
            generated_fields,
            anchor_checks,
            layer_started.elapsed().as_millis()
        );
    }
    Ok((workers, metrics))
}

struct CheckpointState {
    next_layer: u8,
    frontiers: Vec<Option<RunFile>>,
    metrics: Metrics,
}

#[allow(clippy::too_many_arguments)]
fn initialize_boundary_frontiers(
    dataset: &Dataset,
    field_index: &[u8],
    reverse: &BTreeMap<u8, domain::DomainFile>,
    anchor_layer: u8,
    boundary_path: &Path,
    binding: [u8; 32],
    expected: &boundary::BoundaryScan,
    completion_oracle: &CompletionNecessaryOracle,
    workspace: &Workspace,
) -> Result<(Vec<Option<RunFile>>, Metrics), String> {
    let mut seed_writers = (0..anchor_layer)
        .map(|layer| workspace.writer(&format!("seed-layer-{layer:02}.bin")))
        .collect::<Result<Vec<_>, String>>()?;
    let mut metrics = Metrics {
        boundary_fields: expected.field_count,
        ..Metrics::default()
    };
    let mut indexed_id = 0_u32;
    let mut reverse_indices = [0_usize; 11];
    let observed = boundary::scan(
        boundary_path,
        binding,
        Some((expected.start, expected.end)),
        |field| {
            while indexed_id < dataset.field_count {
                let indexed = field_hash(field_index, indexed_id)?;
                if indexed < field {
                    indexed_id = indexed_id
                        .checked_add(1)
                        .ok_or("field index cursor overflow")?;
                    continue;
                }
                if indexed == field {
                    return Err(format!(
                        "outside-boundary input contains indexed field {field:#012x}"
                    ));
                }
                break;
            }
            let layer = field_layer(field)?;
            if layer >= anchor_layer {
                metrics.reverse_anchor_checks = metrics
                    .reverse_anchor_checks
                    .checked_add(1)
                    .ok_or("boundary dead-proof metric overflow")?;
                let reverse_fields = &reverse
                    .get(&layer)
                    .ok_or("reverse anchor layer missing")?
                    .fields;
                let cursor = &mut reverse_indices[usize::from(layer)];
                while *cursor < reverse_fields.len() && reverse_fields[*cursor] < field {
                    *cursor += 1;
                }
                if *cursor < reverse_fields.len() && reverse_fields[*cursor] == field {
                    return Err(format!(
                        "outside-boundary field {field:#012x} reaches the exact reverse terminal domain"
                    ));
                }
            } else {
                let necessity = completion_oracle.classify(field)?;
                if metrics.retain_after_necessary_filter(necessity)? {
                    seed_writers[usize::from(layer)].push(field)?;
                }
            }
            Ok(())
        },
    )?;
    if observed.start != expected.start
        || observed.end != expected.end
        || observed.field_count != expected.field_count
        || observed.file_identity != expected.file_identity
    {
        return Err("outside-boundary changed between validation and classification".to_owned());
    }
    Ok((
        seed_writers
            .into_iter()
            .map(|writer| writer.finish().map(Some))
            .collect::<Result<Vec<_>, String>>()?,
        metrics,
    ))
}

fn load_latest_checkpoint(
    workspace: &Workspace,
    anchor_layer: u8,
) -> Result<Option<CheckpointState>, String> {
    let mut checkpoints = Vec::new();
    for entry in fs::read_dir(workspace.root()).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or("boundary checkpoint name must be UTF-8")?
            .to_owned();
        let Some(layer) = name
            .strip_prefix("checkpoint-layer-")
            .and_then(|value| value.strip_suffix(".json"))
            .and_then(|value| value.parse::<u8>().ok())
        else {
            continue;
        };
        checkpoints.push((layer, entry.path(), name));
    }
    let Some((file_layer, path, checkpoint_name)) =
        checkpoints.into_iter().max_by_key(|(layer, _, _)| *layer)
    else {
        return Ok(None);
    };
    let checkpoint = read_json(&path, 1024 * 1024)?;
    validate_receipt_identity(&checkpoint)?;
    let next_layer = checkpoint["next_layer"]
        .as_u64()
        .and_then(|value| u8::try_from(value).ok())
        .ok_or("boundary checkpoint next layer missing")?;
    if checkpoint["schema"] != "clearra.pc4.outside-boundary-dead-checkpoint.v1"
        || checkpoint["proof_algorithm"] != PROOF_ALGORITHM
        || next_layer != file_layer
        || next_layer > anchor_layer
    {
        return Err("boundary checkpoint contract mismatch".to_owned());
    }
    let metrics = Metrics::from_json(&checkpoint["metrics"])?;
    let descriptors = checkpoint["frontiers"]
        .as_array()
        .ok_or("boundary checkpoint frontiers missing")?;
    if descriptors.len() != usize::from(anchor_layer - next_layer) {
        return Err("boundary checkpoint frontier cover mismatch".to_owned());
    }
    let mut frontiers = (0..anchor_layer).map(|_| None).collect::<Vec<_>>();
    let mut retained = BTreeSet::from([checkpoint_name]);
    for (offset, descriptor) in descriptors.iter().enumerate() {
        let layer = next_layer
            .checked_add(u8::try_from(offset).map_err(|_| "checkpoint layer overflow")?)
            .ok_or("checkpoint layer overflow")?;
        if descriptor["layer"].as_u64() != Some(u64::from(layer)) {
            return Err("boundary checkpoint frontier order mismatch".to_owned());
        }
        let name = descriptor["file_name"]
            .as_str()
            .ok_or("boundary checkpoint frontier name missing")?;
        let count = descriptor["field_count"]
            .as_u64()
            .ok_or("boundary checkpoint frontier count missing")?;
        let identity = descriptor["content_identity"]
            .as_str()
            .ok_or("boundary checkpoint frontier identity missing")?;
        let run = workspace.restore_run(name, count, identity)?;
        retained.insert(name.to_owned());
        frontiers[usize::from(layer)] = Some(run);
    }
    workspace.retain_entries(&retained)?;
    Ok(Some(CheckpointState {
        next_layer,
        frontiers,
        metrics,
    }))
}

fn publish_checkpoint(
    workspace: &Workspace,
    next_layer: u8,
    frontiers: &[Option<RunFile>],
    metrics: &Metrics,
) -> Result<(), String> {
    let anchor_layer = u8::try_from(frontiers.len()).map_err(|_| "anchor layer overflow")?;
    if next_layer > anchor_layer
        || frontiers[..usize::from(next_layer)]
            .iter()
            .any(Option::is_some)
        || frontiers[usize::from(next_layer)..]
            .iter()
            .any(Option::is_none)
    {
        return Err("boundary checkpoint frontier state invalid".to_owned());
    }
    let descriptors = frontiers
        .iter()
        .enumerate()
        .skip(usize::from(next_layer))
        .map(|(layer, run)| {
            let run = run.as_ref().ok_or("boundary checkpoint frontier missing")?;
            let name = run
                .path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or("boundary checkpoint frontier name must be UTF-8")?;
            Ok(json!({
                "layer": layer,
                "file_name": name,
                "field_count": run.count,
                "content_identity": run.identity,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let checkpoint = with_identity(json!({
        "schema": "clearra.pc4.outside-boundary-dead-checkpoint.v1",
        "proof_algorithm": PROOF_ALGORITHM,
        "next_layer": next_layer,
        "metrics": metrics.as_json(),
        "frontiers": descriptors,
    }))?;
    let checkpoint_name = format!("checkpoint-layer-{next_layer:02}.json");
    let checkpoint_path = workspace.root().join(&checkpoint_name);
    write_json_atomic(&checkpoint_path, &checkpoint)?;
    let mut retained = BTreeSet::from([checkpoint_name]);
    for run in frontiers.iter().flatten() {
        retained.insert(
            run.path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or("boundary checkpoint frontier name must be UTF-8")?
                .to_owned(),
        );
    }
    workspace.retain_entries(&retained)
}

#[allow(clippy::too_many_arguments)]
fn expand_frontier_layer_disk(
    dataset: &Dataset,
    field_index: &[u8],
    reverse: &BTreeMap<u8, domain::DomainFile>,
    anchor_layer: u8,
    source_layer: u8,
    frontier: &RunFile,
    workers: usize,
    completion_oracle: &CompletionNecessaryOracle,
    workspace: &Workspace,
) -> Result<(RunFile, Metrics), String> {
    if workers == 0 {
        return Err("boundary disk expansion requires at least one worker".to_owned());
    }
    let partials = thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for worker in 0..workers {
            let start = frontier
                .count
                .checked_mul(worker as u64)
                .ok_or("boundary worker range overflow")?
                / workers as u64;
            let end = frontier
                .count
                .checked_mul((worker + 1) as u64)
                .ok_or("boundary worker range overflow")?
                / workers as u64;
            handles.push(scope.spawn(move || {
                let mut output = workspace.accumulator(source_layer + 1, worker)?;
                let mut metrics = Metrics::default();
                workspace.visit_range(frontier, start, end, |source| {
                    let cells = hydra_field_hash_v1_to_clearra_board64_mask(source)
                        .map_err(|error| error.reason().to_owned())?;
                    if cells.count_ones() / 4 != u32::from(source_layer) {
                        return Err("boundary frontier source layer mismatch".to_owned());
                    }
                    for (_, piece) in PIECES {
                        let targets =
                            enumerate_pc4_ilc_target_fields(cells, piece, dataset.kick_profile)
                                .map_err(|error| error.reason().to_owned())?;
                        metrics.generated_target_fields = metrics
                            .generated_target_fields
                            .checked_add(targets.len() as u64)
                            .ok_or("boundary dead-proof metric overflow")?;
                        for target in targets {
                            if target.count_ones() != cells.count_ones() + 4 {
                                return Err(
                                    "boundary dead-proof transition does not advance one layer"
                                        .to_owned(),
                                );
                            }
                            let target_hash = clearra_board64_mask_to_hydra_field_hash_v1(target)
                                .map_err(|error| error.reason().to_owned())?;
                            output.push(target_hash)?;
                        }
                    }
                    Ok(())
                })?;
                Ok((output.finish()?, metrics))
            }));
        }
        Ok::<_, String>(
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .map_err(|_| "boundary dead-proof worker panicked".to_owned())?
                })
                .collect::<Result<Vec<_>, String>>()?,
        )
    })?;
    let mut runs = Vec::new();
    let mut metrics = Metrics::default();
    for (worker_runs, partial_metrics) in partials {
        metrics.add(&partial_metrics)?;
        runs.extend(worker_runs);
    }
    let merged = workspace.merge_runs(
        &format!("generated-layer-{:02}", source_layer + 1),
        source_layer + 1,
        runs,
    )?;
    metrics.unique_outside_successors = merged.count;
    let retained = validate_generated_run(
        dataset,
        field_index,
        reverse,
        anchor_layer,
        source_layer + 1,
        &merged,
        completion_oracle,
        workspace,
        &mut metrics,
    )?;
    workspace.remove_run(&merged)?;
    Ok((retained, metrics))
}

#[allow(clippy::too_many_arguments)]
fn validate_generated_run(
    dataset: &Dataset,
    field_index: &[u8],
    reverse: &BTreeMap<u8, domain::DomainFile>,
    anchor_layer: u8,
    target_layer: u8,
    run: &RunFile,
    completion_oracle: &CompletionNecessaryOracle,
    workspace: &Workspace,
    metrics: &mut Metrics,
) -> Result<RunFile, String> {
    let mut indexed_id = 0_u32;
    let reverse_fields = (target_layer >= anchor_layer)
        .then(|| {
            reverse
                .get(&target_layer)
                .ok_or("reverse anchor layer missing")
                .map(|file| file.fields.as_slice())
        })
        .transpose()?;
    let mut reverse_index = 0_usize;
    let mut retained = (target_layer < anchor_layer)
        .then(|| workspace.writer(&format!("viable-layer-{target_layer:02}.bin")))
        .transpose()?;
    workspace.visit_range(run, 0, run.count, |target_hash| {
        while indexed_id < dataset.field_count && field_hash(field_index, indexed_id)? < target_hash
        {
            indexed_id += 1;
        }
        if indexed_id < dataset.field_count && field_hash(field_index, indexed_id)? == target_hash {
            return Err(format!(
                "outside-boundary path re-enters indexed terminal-live field {target_hash:#012x}"
            ));
        }
        if let Some(fields) = reverse_fields {
            metrics.reverse_anchor_checks = metrics
                .reverse_anchor_checks
                .checked_add(1)
                .ok_or("boundary dead-proof metric overflow")?;
            while reverse_index < fields.len() && fields[reverse_index] < target_hash {
                reverse_index += 1;
            }
            if reverse_index < fields.len() && fields[reverse_index] == target_hash {
                return Err(format!(
                    "outside-boundary path reaches reverse-live field {target_hash:#012x}"
                ));
            }
        }
        if let Some(writer) = retained.as_mut() {
            let necessity = completion_oracle.classify(target_hash)?;
            if metrics.retain_after_necessary_filter(necessity)? {
                writer.push(target_hash)?;
            }
        }
        Ok(())
    })?;
    if let Some(writer) = retained {
        writer.finish()
    } else {
        workspace.write_sorted(
            &format!("classified-layer-{target_layer:02}-empty.bin"),
            &[],
        )
    }
}

#[allow(dead_code)]
fn classify_boundary(
    dataset: &Dataset,
    field_index: &[u8],
    reverse: &BTreeMap<u8, domain::DomainFile>,
    anchor_layer: u8,
    fields: &[u64],
    requested_workers: usize,
) -> Result<(usize, Metrics), String> {
    if requested_workers == 0 || requested_workers > 64 {
        return Err("boundary dead-proof worker count outside 1..=64".to_owned());
    }
    let available = thread::available_parallelism().map_or(1, usize::from);
    let workers = requested_workers.min(available).min(fields.len().max(1));
    let mut frontiers = (0..anchor_layer).map(|_| Vec::new()).collect::<Vec<_>>();
    let mut metrics = Metrics {
        boundary_fields: fields.len() as u64,
        ..Metrics::default()
    };
    for &field in fields {
        let layer = field_layer(field)?;
        if layer >= anchor_layer {
            metrics.reverse_anchor_checks = metrics
                .reverse_anchor_checks
                .checked_add(1)
                .ok_or("boundary dead-proof metric overflow")?;
            if reverse
                .get(&layer)
                .ok_or("reverse anchor layer missing")?
                .fields
                .binary_search(&field)
                .is_ok()
            {
                return Err(format!(
                    "outside-boundary field {field:#012x} reaches the exact reverse terminal domain"
                ));
            }
        } else {
            frontiers[usize::from(layer)].push(field);
        }
    }
    for frontier in &mut frontiers {
        frontier.sort_unstable();
        frontier.dedup();
    }
    for layer in 0..anchor_layer {
        let layer_started = Instant::now();
        let frontier = std::mem::take(&mut frontiers[usize::from(layer)]);
        metrics.frontier_fields_by_layer[usize::from(layer)] = frontier.len() as u64;
        metrics.expanded_frontier_fields = metrics
            .expanded_frontier_fields
            .checked_add(frontier.len() as u64)
            .ok_or("boundary dead-proof metric overflow")?;
        if frontier.is_empty() {
            continue;
        }
        let (generated, layer_metrics) = expand_frontier_layer(
            dataset,
            field_index,
            reverse,
            anchor_layer,
            layer,
            &frontier,
            workers.min(frontier.len()),
        )?;
        let generated_fields = generated.len();
        let generated_targets = layer_metrics.generated_target_fields;
        let anchor_checks = layer_metrics.reverse_anchor_checks;
        metrics.add(&layer_metrics)?;
        if layer + 1 < anchor_layer {
            let existing = std::mem::take(&mut frontiers[usize::from(layer + 1)]);
            frontiers[usize::from(layer + 1)] = merge_sorted_partials(vec![existing, generated])?;
        } else if !generated.is_empty() {
            return Err("anchor expansion retained an unclassified frontier".to_owned());
        }
        println!(
            "pc4_boundary_dead_layer=passed layer={} frontier={} generated_targets={} next_unique={} anchor_checks={} elapsed_ms={}",
            layer,
            frontier.len(),
            generated_targets,
            generated_fields,
            anchor_checks,
            layer_started.elapsed().as_millis()
        );
    }
    Ok((workers, metrics))
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
fn expand_frontier_layer(
    dataset: &Dataset,
    field_index: &[u8],
    reverse: &BTreeMap<u8, domain::DomainFile>,
    anchor_layer: u8,
    source_layer: u8,
    frontier: &[u64],
    workers: usize,
) -> Result<(Vec<u64>, Metrics), String> {
    let partials = thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for worker in 0..workers {
            handles.push(scope.spawn(move || {
                let mut output = SortedRunAccumulator::new()?;
                let mut metrics = Metrics::default();
                for index in (worker..frontier.len()).step_by(workers) {
                    let source = frontier[index];
                    let cells = hydra_field_hash_v1_to_clearra_board64_mask(source)
                        .map_err(|error| error.reason().to_owned())?;
                    if cells.count_ones() / 4 != u32::from(source_layer) {
                        return Err("boundary frontier source layer mismatch".to_owned());
                    }
                    for (_, piece) in PIECES {
                        let targets = enumerate_pc4_ilc_target_fields(
                            cells,
                            piece,
                            dataset.kick_profile,
                        )
                        .map_err(|error| error.reason().to_owned())?;
                        metrics.generated_target_fields = metrics
                            .generated_target_fields
                            .checked_add(targets.len() as u64)
                            .ok_or("boundary dead-proof metric overflow")?;
                        for target in targets {
                            if target.count_ones() != cells.count_ones() + 4 {
                                return Err(
                                    "boundary dead-proof transition does not advance one layer"
                                        .to_owned(),
                                );
                            }
                            let target_hash = clearra_board64_mask_to_hydra_field_hash_v1(target)
                                .map_err(|error| error.reason().to_owned())?;
                            if lookup_field_id(field_index, dataset.field_count, target_hash)?
                                .is_some()
                            {
                                return Err(format!(
                                    "outside-boundary path re-enters indexed terminal-live field {target_hash:#012x}"
                                ));
                            }
                            if source_layer + 1 >= anchor_layer {
                                metrics.reverse_anchor_checks = metrics
                                    .reverse_anchor_checks
                                    .checked_add(1)
                                    .ok_or("boundary dead-proof metric overflow")?;
                                if reverse
                                    .get(&(source_layer + 1))
                                    .ok_or("reverse anchor layer missing")?
                                    .fields
                                    .binary_search(&target_hash)
                                    .is_ok()
                                {
                                    return Err(format!(
                                        "outside-boundary path reaches reverse-live field {target_hash:#012x}"
                                    ));
                                }
                            } else {
                                output.push(target_hash)?;
                            }
                        }
                    }
                }
                Ok((output.finish()?, metrics))
            }));
        }
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| "boundary dead-proof worker panicked".to_owned())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let mut outputs = Vec::with_capacity(partials.len());
    let mut metrics = Metrics::default();
    for (output, partial_metrics) in partials {
        metrics.add(&partial_metrics)?;
        outputs.push(output);
    }
    let merged = merge_sorted_partials(outputs)?;
    metrics.unique_outside_successors = merged.len() as u64;
    Ok((merged, metrics))
}

#[allow(dead_code)]
struct SortedRunAccumulator {
    pending: Vec<u64>,
    levels: Vec<Option<Vec<u64>>>,
}

#[allow(dead_code)]
impl SortedRunAccumulator {
    fn new() -> Result<Self, String> {
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(SORT_RUN_FIELDS)
            .map_err(|_| "boundary frontier sort-run allocation failed")?;
        Ok(Self {
            pending,
            levels: Vec::new(),
        })
    }

    fn push(&mut self, field: u64) -> Result<(), String> {
        self.pending.push(field);
        if self.pending.len() == SORT_RUN_FIELDS {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), String> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.pending.sort_unstable();
        self.pending.dedup();
        let mut run = std::mem::take(&mut self.pending);
        let mut level = 0_usize;
        loop {
            if level == self.levels.len() {
                self.levels.push(Some(run));
                break;
            }
            if let Some(existing) = self.levels[level].take() {
                run = merge_sorted_partials(vec![existing, run])?;
                level += 1;
            } else {
                self.levels[level] = Some(run);
                break;
            }
        }
        self.pending = Vec::new();
        self.pending
            .try_reserve_exact(SORT_RUN_FIELDS)
            .map_err(|_| "boundary frontier sort-run allocation failed")?;
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<u64>, String> {
        self.flush()?;
        merge_sorted_partials(self.levels.into_iter().flatten().collect())
    }
}

#[allow(dead_code)]
fn merge_sorted_partials(partials: Vec<Vec<u64>>) -> Result<Vec<u64>, String> {
    let mut merged = Vec::new();
    let capacity = partials
        .iter()
        .try_fold(0_usize, |total, values| total.checked_add(values.len()))
        .ok_or("boundary frontier merge capacity overflow")?;
    merged
        .try_reserve(capacity)
        .map_err(|_| "boundary frontier merge allocation failed")?;
    let mut heap = BinaryHeap::new();
    for (partition, values) in partials.iter().enumerate() {
        if let Some(&value) = values.first() {
            heap.push(Reverse((value, partition, 0_usize)));
        }
    }
    while let Some(Reverse((value, partition, index))) = heap.pop() {
        if merged.last().is_none_or(|prior| *prior != value) {
            merged.push(value);
        }
        let next = index + 1;
        if let Some(&next_value) = partials[partition].get(next) {
            heap.push(Reverse((next_value, partition, next)));
        }
    }
    Ok(merged)
}

fn field_layer(field: u64) -> Result<u8, String> {
    let cells = hydra_field_hash_v1_to_clearra_board64_mask(field)
        .map_err(|error| error.reason().to_owned())?;
    let occupied = cells.count_ones();
    if !occupied.is_multiple_of(4) || occupied > 40 {
        return Err("boundary dead-proof field lies outside PC4 layers".to_owned());
    }
    u8::try_from(occupied / 4).map_err(|_| "PC4 layer overflow".to_owned())
}

fn load_reverse_chain(
    binding: domain::DomainBinding,
    directory: &Path,
    anchor_layer: u8,
) -> Result<BTreeMap<u8, domain::DomainFile>, String> {
    let mut reverse = BTreeMap::new();
    let mut parent_digest = None;
    for layer in (anchor_layer..=10).rev() {
        let path = directory.join(format!("reverse-layer-{layer:02}.bin"));
        let file = domain::read(&path, binding, Some(layer))?;
        if layer == 10 {
            if file.derivation != domain::DomainDerivation::ReverseSeed
                || file.input_digest != [0; 32]
                || file.filter_digest != [0; 32]
                || file.fields != [TERMINAL_FIELD]
            {
                return Err("reverse terminal seed provenance invalid".to_owned());
            }
        } else if file.derivation != domain::DomainDerivation::ReverseStep
            || Some(file.input_digest) != parent_digest
            || file.filter_digest != [0; 32]
        {
            return Err("reverse anchor chain provenance invalid".to_owned());
        }
        parent_digest = Some(file.file_digest);
        reverse.insert(layer, file);
    }
    Ok(reverse)
}

#[derive(Default)]
struct Metrics {
    boundary_fields: u64,
    expanded_frontier_fields: u64,
    generated_target_fields: u64,
    unique_outside_successors: u64,
    reverse_anchor_checks: u64,
    necessary_filter_retained_fields: u64,
    full_column_strip_dead_fields: u64,
    projected_cover_dead_fields: u64,
    projected_cover_unknown_fields: u64,
    frontier_fields_by_layer: [u64; 10],
}

impl Metrics {
    fn from_json(value: &Value) -> Result<Self, String> {
        fn metric(value: &Value, name: &str) -> Result<u64, String> {
            value[name]
                .as_u64()
                .ok_or_else(|| format!("boundary checkpoint metric missing: {name}"))
        }
        let layers = value["frontier_fields_by_layer"]
            .as_array()
            .ok_or("boundary checkpoint layer metrics missing")?;
        if layers.len() != 10 {
            return Err("boundary checkpoint layer metric count mismatch".to_owned());
        }
        let mut frontier_fields_by_layer = [0_u64; 10];
        for (target, source) in frontier_fields_by_layer.iter_mut().zip(layers) {
            *target = source
                .as_u64()
                .ok_or("boundary checkpoint layer metric invalid")?;
        }
        Ok(Self {
            boundary_fields: metric(value, "boundary_fields")?,
            expanded_frontier_fields: metric(value, "expanded_frontier_fields")?,
            generated_target_fields: metric(value, "generated_target_fields")?,
            unique_outside_successors: metric(value, "unique_outside_successors")?,
            reverse_anchor_checks: metric(value, "reverse_anchor_checks")?,
            necessary_filter_retained_fields: metric(value, "necessary_filter_retained_fields")?,
            full_column_strip_dead_fields: metric(value, "full_column_strip_dead_fields")?,
            projected_cover_dead_fields: metric(value, "projected_cover_dead_fields")?,
            projected_cover_unknown_fields: metric(value, "projected_cover_unknown_fields")?,
            frontier_fields_by_layer,
        })
    }

    fn as_json(&self) -> Value {
        json!({
            "boundary_fields": self.boundary_fields,
            "expanded_frontier_fields": self.expanded_frontier_fields,
            "generated_target_fields": self.generated_target_fields,
            "unique_outside_successors": self.unique_outside_successors,
            "reverse_anchor_checks": self.reverse_anchor_checks,
            "necessary_filter_retained_fields": self.necessary_filter_retained_fields,
            "full_column_strip_dead_fields": self.full_column_strip_dead_fields,
            "projected_cover_dead_fields": self.projected_cover_dead_fields,
            "projected_cover_unknown_fields": self.projected_cover_unknown_fields,
            "frontier_fields_by_layer": self.frontier_fields_by_layer,
        })
    }

    fn retain_after_necessary_filter(
        &mut self,
        necessity: CompletionNecessity,
    ) -> Result<bool, String> {
        let field = match necessity {
            CompletionNecessity::Retain => &mut self.necessary_filter_retained_fields,
            CompletionNecessity::DeadFullColumnStrip => &mut self.full_column_strip_dead_fields,
            CompletionNecessity::DeadProjectedCover => &mut self.projected_cover_dead_fields,
            CompletionNecessity::UnknownProjectedCover => {
                self.projected_cover_unknown_fields = self
                    .projected_cover_unknown_fields
                    .checked_add(1)
                    .ok_or("boundary dead-proof metric overflow")?;
                &mut self.necessary_filter_retained_fields
            }
        };
        *field = field
            .checked_add(1)
            .ok_or("boundary dead-proof metric overflow")?;
        Ok(matches!(
            necessity,
            CompletionNecessity::Retain | CompletionNecessity::UnknownProjectedCover
        ))
    }

    fn add(&mut self, other: &Self) -> Result<(), String> {
        macro_rules! checked_add {
            ($field:ident) => {
                self.$field = self
                    .$field
                    .checked_add(other.$field)
                    .ok_or("boundary dead-proof metric overflow")?;
            };
        }
        checked_add!(boundary_fields);
        checked_add!(expanded_frontier_fields);
        checked_add!(generated_target_fields);
        checked_add!(unique_outside_successors);
        checked_add!(reverse_anchor_checks);
        checked_add!(necessary_filter_retained_fields);
        checked_add!(full_column_strip_dead_fields);
        checked_add!(projected_cover_dead_fields);
        checked_add!(projected_cover_unknown_fields);
        for (total, layer) in self
            .frontier_fields_by_layer
            .iter_mut()
            .zip(other.frontier_fields_by_layer)
        {
            *total = total
                .checked_add(layer)
                .ok_or("boundary dead-proof metric overflow")?;
        }
        Ok(())
    }
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorted_partial_merge_is_deterministic_and_unique() {
        assert_eq!(
            merge_sorted_partials(vec![
                vec![1, 3, 8, 13],
                vec![1, 2, 8, 21],
                Vec::new(),
                vec![3, 5, 34],
            ])
            .unwrap(),
            vec![1, 2, 3, 5, 8, 13, 21, 34]
        );
    }

    #[test]
    fn sorted_run_accumulator_deduplicates_pending_and_existing_levels() {
        let mut accumulator = SortedRunAccumulator::new().unwrap();
        accumulator.push(8).unwrap();
        accumulator.push(3).unwrap();
        accumulator.push(8).unwrap();
        accumulator.flush().unwrap();
        accumulator.push(5).unwrap();
        accumulator.push(3).unwrap();
        assert_eq!(accumulator.finish().unwrap(), vec![3, 5, 8]);
    }

    #[test]
    fn boundary_checkpoint_restores_hashed_frontiers_and_discards_partial_files() {
        let root = std::env::temp_dir().join(format!(
            "clearra-pc4-boundary-checkpoint-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let workspace = Workspace::prepare(&root, json!({ "test": "checkpoint" })).unwrap();
        let frontiers = vec![
            Some(workspace.write_sorted("seed-00.bin", &[1, 3]).unwrap()),
            Some(workspace.write_sorted("seed-01.bin", &[7]).unwrap()),
            Some(workspace.write_sorted("seed-02.bin", &[]).unwrap()),
        ];
        let metrics = Metrics {
            boundary_fields: 17,
            projected_cover_dead_fields: 5,
            ..Metrics::default()
        };
        publish_checkpoint(&workspace, 0, &frontiers, &metrics).unwrap();
        let partial = workspace
            .write_sorted("uncheckpointed-partial.bin", &[99])
            .unwrap();
        assert!(partial.path.exists());

        let mut restored = load_latest_checkpoint(&workspace, 3)
            .unwrap()
            .expect("checkpoint is present");
        assert_eq!(restored.next_layer, 0);
        assert_eq!(restored.metrics.boundary_fields, 17);
        assert_eq!(restored.metrics.projected_cover_dead_fields, 5);
        assert!(!partial.path.exists());

        let completed = restored.frontiers[0].take().unwrap();
        publish_checkpoint(&workspace, 1, &restored.frontiers, &restored.metrics).unwrap();
        assert!(!completed.path.exists());
        assert!(!root.join("checkpoint-layer-00.json").exists());
        assert!(root.join("checkpoint-layer-01.json").exists());
        let resumed = load_latest_checkpoint(&workspace, 3)
            .unwrap()
            .expect("advanced checkpoint is present");
        assert_eq!(resumed.next_layer, 1);
        workspace.cleanup().unwrap();
    }
}
