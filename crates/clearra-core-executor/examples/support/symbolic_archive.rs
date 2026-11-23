//! Shared native archive boundary for the two research commands.

use std::fs::{self, File, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{
    SymbolicFamilyArchive, SymbolicFamilyParallelReport, SymbolicFamilyResult,
};
use clearra_ctk4::{ArchiveLimits, EncodeOptions, OperationControl, SeekReadAt, UnverifiedArchive};
use serde_json::{json, Value};

pub fn movement_parallel(report: &SymbolicFamilyParallelReport) -> Value {
    json!({
        "requested_workers": report.requested_workers,
        "eligible_pieces": report.eligible_pieces,
        "spawned_workers": report.spawned_workers,
        "active_workers": report.active_workers,
        "peak_concurrent_movement_tasks": report.peak_concurrent_movement_tasks,
        "fallback_reason": report.fallback_reason,
        "shared_prefix_nodes": report.shared_prefix_nodes,
        "shared_prefix_retained_bytes_estimate": report.shared_prefix_retained_bytes_estimate,
        "private_node_peak_sum_upper_bound": report.private_node_peak_sum_upper_bound,
        "private_retained_bytes_sum_estimate": report.private_retained_bytes_sum_estimate,
        "released_builder_bytes_sum_estimate": report.released_builder_bytes_sum_estimate,
        "predicate_compactions": report.predicate_compactions.to_string(),
        "predicate_reclaimed_nodes": report.predicate_reclaimed_nodes.to_string(),
        "predicate_compaction_work_steps": report.predicate_compaction_work_steps.to_string(),
        "predicate_discarded_apply_entries": report.predicate_discarded_apply_entries.to_string(),
        "compaction_peak_temporary_bytes_sum_estimate": report.compaction_peak_temporary_bytes_sum_estimate,
        "predicate_work_steps": report.predicate_work_steps.to_string(),
        "estimate_scope": "private sums are not simultaneous peaks or RSS",
    })
}

pub fn checked_output(path: &Path) -> Result<PathBuf, String> {
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("output cannot contain parent traversal".into());
    }
    let name = path.file_name().ok_or("output needs a file name")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|error| error.to_string())?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !parent.starts_with(repository.join("_local/artifacts")) {
        return Err("output must be inside the repository's _local/artifacts root".into());
    }
    Ok(parent.join(name))
}

pub fn persist(
    result: &mut SymbolicFamilyResult,
    path: &Path,
    annotation: &Value,
    control: &ExecutionControl,
) -> Result<Value, String> {
    let started = Instant::now();
    let path = checked_output(path)?;
    if path.try_exists().map_err(|error| error.to_string())? {
        return Err(format!("archive already exists: {}", path.display()));
    }
    let mut partial_name = path.as_os_str().to_owned();
    partial_name.push(".partial");
    let partial = PathBuf::from(partial_name);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&partial)
        .map_err(|error| format!("create {}: {error}", partial.display()))?;
    let annotation = serde_json::to_vec(annotation).map_err(|error| error.to_string())?;
    let cancellation = || control.is_cancelled();
    let archive_control = OperationControl {
        max_work: 0,
        cancellation: Some(&cancellation),
    };
    let seal = result
        .write_ctk4(
            &mut file,
            &annotation,
            EncodeOptions::default(),
            ArchiveLimits::default(),
            archive_control,
        )
        .map_err(|error| format!("CTK4 write: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("sync CTK4: {error}"))?;
    drop(file);
    // Same-directory hard-link publication is atomic and refuses to replace
    // any competing destination. The owned partial stays available on error.
    fs::hard_link(&partial, &path).map_err(|error| format!("publish CTK4: {error}"))?;
    fs::remove_file(&partial).map_err(|error| format!("remove owned CTK4 partial: {error}"))?;

    let source = SeekReadAt::new(File::open(&path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let raw = UnverifiedArchive::open(source, ArchiveLimits::default(), archive_control)
        .map_err(|error| error.to_string())?;
    let verified = raw
        .accept_producer_seal(&seal)
        .map_err(|error| error.to_string())?;
    let archive =
        SymbolicFamilyArchive::from_verified(verified).map_err(|error| error.to_string())?;
    let metrics = archive.metrics();
    let info = archive.info();
    let manifest_sha256: String = seal
        .manifest_digest()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(json!({
        "path": path.to_string_lossy(),
        "dialect": "CTK4ZDD-v1",
        "bytes": seal.archive_bytes(),
        "nodes": info.node_count,
        "variables": info.variable_count,
        "manifest_sha256": manifest_sha256,
        "archive_seconds": started.elapsed().as_secs_f64(),
        "serialized_graph_verified": true,
        "enumerated_solutions": "0",
        "lazy_reopen_block_fetches": metrics.block_fetches,
        "lazy_reopen_bytes_read": metrics.bytes_read,
        "cold_foreign_open_requires_node_scan": true,
    }))
}
