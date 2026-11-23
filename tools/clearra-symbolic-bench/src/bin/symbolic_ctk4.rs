//! Independent native decoder for the experimental shared-result archive.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
use clearra_core_executor::{SymbolicFamilyArchive, SymbolicFamilyArchiveScope};
use clearra_ctk3::{encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece};
use clearra_ctk4::{
    ArchiveIoMetrics, ArchiveLimits, OperationControl, SeekReadAt, UnverifiedArchive,
};
use serde_json::{json, Value};

#[allow(dead_code)]
#[path = "../../../../crates/clearra-core-executor/examples/support/symbolic_archive.rs"]
mod symbolic_archive;

const USAGE: &str = "usage: symbolic_ctk4 inspect|verify|page ARCHIVE [options]\n\
  --offset N            zero-based u128 page rank; default 0\n\
  --limit N             requested identities, at most 4096; default 16\n\
  --cache-mib N         block payload cache; default 8, 0 disables retention\n\
  --ctk3-output PATH    save only the requested page as a CTK3 display view\n\
\n\
inspect reads the manifest and reports declared counts as unverified.\n\
verify/page first validate the unknown file's complete graph, without enumerating\n\
solutions. Subsequent paging loads needed blocks and skips counted branches.\n\
CTK3 comments retain each requested identity's canonical CTK1 partition.\n\
No time limit is installed. Large graph scans require normal runtime supervision.";

struct Options {
    mode: String,
    input: PathBuf,
    offset: u128,
    limit: usize,
    cache_mib: usize,
    ctk3_output: Option<PathBuf>,
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return;
    }
    match parse(args.into_iter()).and_then(execute) {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("{}", json!({"complete": false, "error": error}));
            std::process::exit(1);
        }
    }
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mode = args.next().ok_or(USAGE)?;
    if !matches!(mode.as_str(), "inspect" | "verify" | "page") {
        return Err(USAGE.into());
    }
    let mut options = Options {
        mode,
        input: PathBuf::from(args.next().ok_or(USAGE)?),
        offset: 0,
        limit: 16,
        cache_mib: 8,
        ctk3_output: None,
    };
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--offset" => options.offset = value.parse().map_err(|_| "invalid offset")?,
            "--limit" => options.limit = value.parse().map_err(|_| "invalid page limit")?,
            "--cache-mib" => options.cache_mib = value.parse().map_err(|_| "invalid cache size")?,
            "--ctk3-output" if options.ctk3_output.is_none() => {
                options.ctk3_output =
                    Some(symbolic_archive::checked_output(&PathBuf::from(value))?);
            }
            _ => return Err(format!("unknown or repeated argument {flag}")),
        }
    }
    if options.limit > 4096 || (options.mode != "page" && options.ctk3_output.is_some()) {
        return Err("--ctk3-output requires page mode; page limit must be at most 4096".into());
    }
    Ok(options)
}

fn execute(options: Options) -> Result<Value, String> {
    let started = Instant::now();
    let file = File::open(&options.input).map_err(|error| error.to_string())?;
    let source = SeekReadAt::new(file).map_err(|error| error.to_string())?;
    let limits = ArchiveLimits {
        cache_bytes: options
            .cache_mib
            .checked_mul(1024 * 1024)
            .ok_or("cache size overflow")?,
        ..ArchiveLimits::default()
    };
    let control = OperationControl::default();
    let raw =
        UnverifiedArchive::open(source, limits, control).map_err(|error| error.to_string())?;
    let mut report = json!({
        "schema": "clearra.symbolic-ctk4-query.v1",
        "mode": options.mode, "path": options.input.to_string_lossy(),
        "nodes": raw.info().node_count, "variables": raw.info().variable_count,
        "archive_bytes": raw.info().archive_bytes,
        "declared_roots": raw.info().roots.iter().map(|root| json!({
            "name": root.name, "count": root.declared_count.to_string()
        })).collect::<Vec<_>>(),
        "graph_verified": false, "gameplay_or_query_authority_verified": false,
        "manifest_io": io_metrics(raw.metrics()),
    });
    if options.mode == "inspect" {
        report["complete"] = json!(true);
        report["total_seconds"] = json!(started.elapsed().as_secs_f64());
        return Ok(report);
    }
    let verification_started = Instant::now();
    let verified = raw.verify_all(control).map_err(|error| error.to_string())?;
    report["graph_verification_seconds"] = json!(verification_started.elapsed().as_secs_f64());
    report["verification_io"] = io_metrics(verified.metrics());
    report["graph_verified"] = json!(true);
    let mut archive =
        SymbolicFamilyArchive::from_verified(verified).map_err(|error| error.to_string())?;
    report["identity_row_set_count"] = json!(archive
        .identity_row_set_count()
        .map_err(|error| error.to_string())?
        .to_string());
    report["covered_source_set_count"] = json!(archive
        .covered_source_set_count()
        .map_err(|error| error.to_string())?
        .to_string());
    report["joint_set_count"] = json!(archive
        .joint_set_count()
        .map_err(|error| error.to_string())?
        .to_string());
    if options.mode == "page" {
        // A cold cache after verification makes requested-page I/O visible.
        // It does not pretend that an unknown file needed no verification.
        archive.clear_cache();
        let before = archive.metrics();
        let page_started = Instant::now();
        let identities = archive
            .identity_page(options.offset, options.limit, control)
            .map_err(|error| error.to_string())?;
        let mut canonical = Vec::new();
        canonical
            .try_reserve_exact(identities.len())
            .map_err(|_| "page key allocation")?;
        for &identity in &identities {
            let mut key = String::new();
            identity
                .write_canonical(&mut key)
                .map_err(|_| "canonical identity")?;
            canonical.push(key);
        }
        let after = archive.metrics();
        report["page"] = json!({
            "offset": options.offset.to_string(), "requested": options.limit,
            "returned": identities.len(), "canonical_identities": canonical,
            "seconds": page_started.elapsed().as_secs_f64(),
            "additional_bytes_read": after.bytes_read.saturating_sub(before.bytes_read),
            "additional_block_fetches": after.block_fetches.saturating_sub(before.block_fetches),
            "additional_cache_hits": after.cache_hits.saturating_sub(before.cache_hits),
            "retained_cache_bytes": after.retained_cache_bytes,
            "uncached_block_bytes": after.uncached_block_bytes,
            "peak_page_stack_entries": after.peak_page_stack_entries,
        });
        if let Some(path) = &options.ctk3_output {
            if identities.is_empty() {
                return Err("empty identity page cannot be saved as a CTK3 document".into());
            }
            let pages = identities
                .iter()
                .copied()
                .zip(&canonical)
                .map(|(identity, key)| display_page(archive.scope(), identity, key))
                .collect::<Result<Vec<_>, _>>()?;
            let text =
                encode_ctk3_compact(&Ctk3Document::new(archive.scope().width as usize, pages))
                    .map_err(|error| error.to_string())?;
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| error.to_string())?;
            let mut output = BufWriter::new(file);
            output
                .write_all(text.as_bytes())
                .and_then(|()| output.write_all(b"\n"))
                .and_then(|()| output.flush())
                .map_err(|error| error.to_string())?;
            report["ctk3"] = json!({"path": path.to_string_lossy(), "pages": identities.len(), "bytes": text.len() + 1,
                "scope": "requested colored display pages with canonical partition comments"});
        }
    }
    report["complete"] = json!(true);
    report["total_seconds"] = json!(started.elapsed().as_secs_f64());
    Ok(report)
}

fn io_metrics(metrics: ArchiveIoMetrics) -> Value {
    json!({
        "bytes_read": metrics.bytes_read, "block_fetches": metrics.block_fetches,
        "cache_hits": metrics.cache_hits, "retained_cache_bytes": metrics.retained_cache_bytes,
        "uncached_block_bytes": metrics.uncached_block_bytes,
        "peak_cached_payload_bytes": metrics.peak_cached_payload_bytes,
        "peak_page_stack_entries": metrics.peak_page_stack_entries,
    })
}

fn display_page(
    scope: SymbolicFamilyArchiveScope,
    identity: StandardBoard64TilingIdentity,
    canonical: &str,
) -> Result<Ctk3Page, String> {
    let area = usize::from(scope.width) * usize::from(scope.height);
    let mut cells = vec![Ctk3Color::Empty; area];
    for (index, color) in cells.iter_mut().enumerate() {
        if identity.initial_board_mask() & (1_u64 << index) != 0 {
            *color = Ctk3Color::Gray;
        }
    }
    for index in 0..identity.placement_count() {
        let placement = identity.placement(index).ok_or("missing placement")?;
        let piece = match placement.piece() {
            PieceKind::I => Ctk3Piece::I,
            PieceKind::O => Ctk3Piece::O,
            PieceKind::T => Ctk3Piece::T,
            PieceKind::S => Ctk3Piece::S,
            PieceKind::Z => Ctk3Piece::Z,
            PieceKind::J => Ctk3Piece::J,
            PieceKind::L => Ctk3Piece::L,
        };
        for (cell, color) in cells.iter_mut().enumerate() {
            if placement.cells_mask() & (1_u64 << cell) != 0 {
                if *color != Ctk3Color::Empty {
                    return Err("overlapping placement in decoded display page".into());
                }
                *color = Ctk3Color::Piece(piece);
            }
        }
    }
    Ok(Ctk3Page::new(scope.height as usize, cells).with_comment(canonical))
}
