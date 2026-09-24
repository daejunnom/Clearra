use clearra_pc4_qualifier::{
    generate_legal_board, legal_board_source_chain_identity,
    validate_legal_board_candidate_catalog, verify_legal_board_candidate_source_chain,
    LegalBoardGenerationOptions,
};
use clearra_rules::kicks::KickTableProfileId;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_BUNDLE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CATALOG_BYTES: u64 = 512 * 1024;

fn main() {
    if let Err(error) = run() {
        eprintln!("pc4_legal_board_error={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let (action, options) = parse_options()?;
    let profile = KickTableProfileId::parse(required(&options, "profile")?)
        .ok_or("legal-board profile is not a known kick-table profile")?;
    if action == "legal-board-verify-candidate" {
        if options.contains_key("layers")
            || options.contains_key("workers")
            || options.contains_key("max-new-steps")
        {
            return Err("candidate verification accepts only profile, bundle and catalog".into());
        }
        let bundle = absolute(&options, "bundle")?;
        let catalog = absolute(&options, "catalog")?;
        let bundle_name = bundle
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("candidate bundle filename must be UTF-8")?;
        let bundle_bytes = read_regular_bounded(&bundle, MAX_BUNDLE_BYTES)?;
        let catalog_bytes = read_regular_bounded(&catalog, MAX_CATALOG_BYTES)?;
        let summary = validate_legal_board_candidate_catalog(
            profile,
            bundle_name,
            Arc::from(bundle_bytes),
            &catalog_bytes,
        )
        .map_err(str::to_owned)?;
        println!(
            "pc4_legal_board_candidate=structurally_valid_unqualified profile={} bundle_bytes={} sparse_index_bytes={} layer_counts={:?}",
            clearra_core_executor::accelerator_profile_name(profile)
                .map_err(|_| "legal-board profile is unsupported")?,
            summary.bundle_bytes,
            summary.sparse_index_bytes,
            summary.layer_counts
        );
        return Ok(());
    }
    if action == "legal-board-verify-source-chain" {
        if options.contains_key("workers") || options.contains_key("max-new-steps") {
            return Err("proof verification cannot create new generation steps".into());
        }
        let layers = absolute(&options, "layers")?;
        let bundle = absolute(&options, "bundle")?;
        let catalog = absolute(&options, "catalog")?;
        verify_legal_board_candidate_source_chain(&LegalBoardGenerationOptions {
            profile,
            layers: layers.clone(),
            bundle: bundle.clone(),
            catalog: catalog.clone(),
            workers: 1,
            max_new_steps: 1,
        })?;
        let receipt = options.get("receipt");
        if let Some(receipt) = receipt {
            let generation_revision = required(&options, "generation-revision")?;
            let verifier_revision = required(&options, "verifier-revision")?;
            let receipt = PathBuf::from(receipt);
            write_source_chain_receipt(
                profile,
                &layers,
                &bundle,
                &catalog,
                &receipt,
                generation_revision,
                verifier_revision,
            )?;
        } else if options.contains_key("generation-revision")
            || options.contains_key("verifier-revision")
        {
            return Err("proof revisions require --receipt".into());
        }
        println!(
            "pc4_legal_board_source_chain=consistent_unqualified profile={}",
            clearra_core_executor::accelerator_profile_name(profile)
                .map_err(|_| "legal-board profile is unsupported")?
        );
        return Ok(());
    }
    let layers = absolute(&options, "layers")?;
    let profile_name = clearra_core_executor::accelerator_profile_name(profile)
        .map_err(|_| "legal-board profile is unsupported")?;
    let bundle = optional_absolute(&options, "bundle")?
        .unwrap_or_else(|| layers.join(format!("legal-board-{profile_name}.cllb")));
    let catalog = optional_absolute(&options, "catalog")?
        .unwrap_or_else(|| layers.join(format!("legal-board-{profile_name}.catalog.json")));
    generate_legal_board(&LegalBoardGenerationOptions {
        profile,
        layers,
        bundle,
        catalog,
        workers: numeric(&options, "workers")?,
        max_new_steps: numeric(&options, "max-new-steps")?,
    })
}

fn parse_options() -> Result<(String, BTreeMap<String, String>), String> {
    let mut args = std::env::args().skip(1);
    let action = args.next().ok_or("expected legal-board action")?;
    if !matches!(
        action.as_str(),
        "legal-board-run" | "legal-board-verify-candidate" | "legal-board-verify-source-chain"
    ) {
        return Err(
            "expected legal-board-run, legal-board-verify-candidate or legal-board-verify-source-chain"
                .to_owned(),
        );
    }
    let mut options = BTreeMap::new();
    while let Some(flag) = args.next() {
        let key = flag
            .strip_prefix("--")
            .ok_or("legal-board option must start with --")?;
        if !matches!(
            key,
            "profile"
                | "layers"
                | "workers"
                | "max-new-steps"
                | "bundle"
                | "catalog"
                | "receipt"
                | "generation-revision"
                | "verifier-revision"
        ) {
            return Err(format!("unknown legal-board option --{key}"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for --{key}"))?;
        if options.insert(key.to_owned(), value).is_some() {
            return Err(format!("duplicate legal-board option --{key}"));
        }
    }
    if action != "legal-board-verify-source-chain"
        && ["receipt", "generation-revision", "verifier-revision"]
            .iter()
            .any(|key| options.contains_key(*key))
    {
        return Err("source-chain receipt options require proof verification".into());
    }
    Ok((action, options))
}

#[allow(clippy::too_many_arguments)]
fn write_source_chain_receipt(
    profile: KickTableProfileId,
    layers: &Path,
    bundle: &Path,
    catalog: &Path,
    receipt: &Path,
    generation_revision: &str,
    verifier_revision: &str,
) -> Result<(), String> {
    for revision in [generation_revision, verifier_revision] {
        if revision.len() != 40
            || !revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("source-chain revision must be a lowercase 40-digit SHA".into());
        }
    }
    if !receipt.is_absolute()
        || receipt
            .parent()
            .and_then(|parent| fs::canonicalize(parent).ok())
            != fs::canonicalize(layers).ok()
    {
        return Err("source-chain receipt must be inside its verified layers directory".into());
    }
    let bundle_name = bundle
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("candidate bundle filename must be UTF-8")?;
    let bundle_bytes = read_regular_bounded(&bundle.to_path_buf(), MAX_BUNDLE_BYTES)?;
    let catalog_bytes = read_regular_bounded(&catalog.to_path_buf(), MAX_CATALOG_BYTES)?;
    let summary = validate_legal_board_candidate_catalog(
        profile,
        bundle_name,
        Arc::from(bundle_bytes.as_slice()),
        &catalog_bytes,
    )
    .map_err(str::to_owned)?;
    let candidate_catalog: serde_json::Value = serde_json::from_slice(&catalog_bytes)
        .map_err(|_| "verified candidate catalog changed before receipt")?;
    let bundle_identity: [u8; 32] = Sha256::digest(&bundle_bytes).into();
    let catalog_identity: [u8; 32] = Sha256::digest(&catalog_bytes).into();
    let profile_name = clearra_core_executor::accelerator_profile_name(profile)
        .map_err(|_| "legal-board profile is unsupported")?;
    let chain_identity =
        legal_board_source_chain_identity(profile, catalog_identity, bundle_identity)?;
    let serialized = serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "clearra.legal-board.source-chain-receipt.v1",
        "status": "source_chain_verified_unqualified",
        "profile": profile_name,
        "generation_revision": generation_revision,
        "verifier_revision": verifier_revision,
        "generation_identity": hex(&summary.generation_identity),
        "bundle_bytes": summary.bundle_bytes,
        "bundle_sha256": hex(&bundle_identity),
        "catalog_sha256": hex(&catalog_identity),
        "chain_identity": hex(&chain_identity),
        "layer_counts": summary.layer_counts.map(|count| count.to_string()),
        "layers": candidate_catalog["layers"],
    }))
    .map_err(|error| error.to_string())?;
    publish_immutable_receipt(receipt, &serialized)
}

fn publish_immutable_receipt(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != bytes.len() as u64
            || fs::read(path).map_err(|error| error.to_string())? != bytes
        {
            return Err("refusing to replace a different source-chain receipt".into());
        }
        return Ok(());
    }
    let parent = path.parent().ok_or("source-chain receipt has no parent")?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("source-chain receipt filename must be UTF-8")?;
    let pending = parent.join(format!(".{name}.pending-{}", std::process::id()));
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        output
            .write_all(bytes)
            .and_then(|_| output.sync_all())
            .map_err(|error| error.to_string())?;
        drop(output);
        fs::hard_link(&pending, path).map_err(|error| error.to_string())?;
        fs::remove_file(&pending).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 15)]));
    }
    output
}

fn read_regular_bounded(path: &PathBuf, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit {
        return Err("candidate input must be a bounded regular file".to_owned());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("candidate input grew beyond its bound".to_owned());
    }
    Ok(bytes)
}

fn required<'a>(options: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    options
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{key}"))
}

fn numeric(options: &BTreeMap<String, String>, key: &str) -> Result<usize, String> {
    required(options, key)?
        .parse()
        .map_err(|_| format!("--{key} must be an unsigned integer"))
}

fn absolute(options: &BTreeMap<String, String>, key: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(required(options, key)?);
    if !path.is_absolute() {
        return Err(format!("--{key} must be absolute"));
    }
    Ok(path)
}

fn optional_absolute(
    options: &BTreeMap<String, String>,
    key: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(value) = options.get(key) else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(format!("--{key} must be absolute"));
    }
    Ok(Some(path))
}
